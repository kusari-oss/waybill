// Milestone 112 — `go mod why -m -vendor` build-graph classification.
//
// Subprocess runner + output parser per
// specs/112-go-build-inclusion/contracts/go-toolchain-invocation.md:
//
//   - chunks of at most 20 module paths per invocation
//     (cyclonedx-gomod `FilterModules` parity);
//   - one shared wall-clock budget (60s default,
//     `WAYBILL_GO_MOD_WHY_BUDGET_MS` test-only override) across ALL
//     invocations in a scan — preflight + every chunk, every main
//     module;
//   - per-main-module `go list all` reliability preflight: `go mod why`
//     exits 0 and silently reports false not-needed verdicts when
//     module resolution fails (verified empirically on go 1.26.2), so
//     a failed preflight skips the main module entirely with ZERO
//     verdicts accepted;
//   - offline env pinning (`GOPROXY=off`, `GOFLAGS=-mod=mod`,
//     `GOTOOLCHAIN=local`) when `--offline` / `WAYBILL_OFFLINE` is set;
//   - every failure class degrades — the scan never errors because of
//     this pass (FR-007).
//
// The spawn-thread + `mpsc::recv_timeout` subprocess pattern mirrors
// `golang/go_mod_graph.rs:81–158`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Maximum module paths per `go mod why` invocation.
///
/// Milestone 771 (issue #745) bumped this from 20 → 500 per research
/// R1: reducing per-workspace subprocess count from ~13 → 1 on
/// Kubernetes-scale monorepos eliminates the dominant wall-time cost
/// of the classifier pass (Go toolchain startup × subprocess count).
/// Chunk size doesn't change `go mod why -m` output semantics —
/// `parse_go_mod_why` already handles multi-section output regardless
/// of section count (see `multi_section_output` test below).
///
/// A defensive argv-length guard (see [`ARG_MAX_SAFE`] +
/// [`select_chunks`]) auto-bisects any batch whose projected argv
/// byte-length approaches operating-system limits, so real workloads
/// never hit E2BIG even if module paths are pathologically long.
const CHUNK_SIZE: usize = 500;

/// Defensive argv-byte-length cap per subprocess invocation.
///
/// POSIX ARG_MAX minimum is 128 KiB; macOS reports `sysctl kern.argmax
/// = 1048576`, Linux typically ~2 MiB. This 96 KiB cap (75% of the
/// POSIX floor) leaves headroom for env vars, the executable path,
/// and the working-dir string, matching the safe envelope other Go
/// tooling (goimports, gopls) uses internally. See research R2.
const ARG_MAX_SAFE: usize = 96 * 1024;

/// Default shared budget across all invocations in a scan.
const DEFAULT_BUDGET: Duration = Duration::from_secs(60);

/// Milestone 771 US2 — bounded worker-count computation.
///
/// Returns `min(workspace_count, available_parallelism())`, clamped
/// to `[1, workspace_count]`. When `available_parallelism()` is
/// unavailable (rare — should only happen on unusual embedded
/// targets), falls back to 1 (serial path).
///
/// Extracted from the caller site (`apply_go_mod_why_pass`) so the
/// clamping logic is unit-testable without spinning up subprocess
/// pools. Per research R3.
pub fn worker_count(workspace_count: usize) -> usize {
    if workspace_count == 0 {
        return 0;
    }
    let cap = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    std::cmp::min(workspace_count, cap).max(1)
}

/// Split `all` into contiguous sub-slices that satisfy both the
/// `max_per_batch` count cap AND the `max_argv_bytes` argv-length
/// cap. Pure function; no I/O.
///
/// Algorithm: greedy slicing by `max_per_batch`; if a slice's
/// projected argv byte-length exceeds `max_argv_bytes`, recurse by
/// bisection until every returned sub-slice fits. Worst-case
/// termination: log₂(max_per_batch) recursion depth before individual
/// paths are argument-list-single (POSIX PATH_MAX = 4 KiB per path
/// bounds the single-path case).
///
/// Argv projection accounts for: subcommand tokens `go mod why -m
/// -vendor` (fixed 24 bytes + 5 separator NULs = 29 bytes overhead)
/// plus per-path `strlen + 1` for the separator NUL. Everything the
/// kernel actually counts. FR-002 + research R2.
pub(super) fn select_chunks(
    all: &[String],
    max_per_batch: usize,
    max_argv_bytes: usize,
) -> Vec<&[String]> {
    fn projected_argv_len(paths: &[String]) -> usize {
        // "go\0mod\0why\0-m\0-vendor\0" = 24 bytes for the fixed part.
        let mut total = 24usize;
        for p in paths {
            total = total.saturating_add(p.len() + 1);
        }
        total
    }
    fn bisect<'a>(
        out: &mut Vec<&'a [String]>,
        slice: &'a [String],
        max_argv_bytes: usize,
    ) {
        if projected_argv_len(slice) <= max_argv_bytes || slice.len() <= 1 {
            out.push(slice);
            return;
        }
        let mid = slice.len() / 2;
        let (left, right) = slice.split_at(mid);
        bisect(out, left, max_argv_bytes);
        bisect(out, right, max_argv_bytes);
    }
    let mut out = Vec::new();
    for chunk in all.chunks(max_per_batch) {
        bisect(&mut out, chunk, max_argv_bytes);
    }
    out
}

// -------------------------------------------------------------------------
// Milestone 771 (issue #745) — subprocess-scaling type declarations.
//
// These types are DECLARED in Foundational phase (T005 + T006) so US1 /
// US2 / US3 can be developed against a stable data-model. They are NOT
// yet wired into `analyze_main_module` — US3 (T031–T035) does that.
// -------------------------------------------------------------------------

/// A group of sibling main-modules that share a single `go.work` file.
///
/// The shared `go list all` reliability preflight is invoked from
/// `root_dir` (per spec.md §Clarifications 2026-09-04 Q1 — the go.work
/// file's parent directory returns the workspace-mode union graph
/// deterministically) and the result cached in
/// [`SharedPreflightCache`] for every member of this scope to reuse.
///
/// See specs/771-gomodwhy-subprocess-scale/data-model.md §Entities.
#[allow(dead_code)] // Wired in by US3 (T031–T034); declared in T005.
#[derive(Debug, Clone)]
pub(super) struct GoWorkScope {
    /// Absolute path to the `go.work` file's parent directory.
    pub root_dir: PathBuf,
    /// Absolute paths to each member main-module directory. Populated
    /// by `parse_go_work` + canonicalized via `std::fs::canonicalize`.
    pub members: Vec<PathBuf>,
}

/// Per-scan cache of shared `go list all` preflight outcomes, keyed by
/// [`GoWorkScope::root_dir`]. Populated lazily on first access per
/// scope; wrapped in `Arc<Mutex<>>` at the call site so concurrent US2
/// workers can share reads.
///
/// The mutex is held only for the brief cache lookup + one-shot insert;
/// worst-case contention is bounded by (concurrent workers) × (one
/// scope). See specs/771-gomodwhy-subprocess-scale/data-model.md
/// §SharedPreflightCache.
#[allow(dead_code)] // Wired in by US3 (T033–T035); declared in T006.
#[derive(Debug, Default)]
pub(super) struct SharedPreflightCache {
    entries: HashMap<PathBuf, PreflightOutcome>,
}

/// Outcome of a shared `go list all` preflight invocation.
#[allow(dead_code)] // Companion to SharedPreflightCache; declared in T006.
#[derive(Debug, Clone)]
pub(super) enum PreflightOutcome {
    /// Preflight succeeded — every member of this scope can proceed to
    /// per-member `go mod why -m` chunks.
    Ok,
    /// Preflight failed — every member of this scope MUST be marked
    /// with `SkipReason::UnresolvablePackages` per FR-007. No `go mod
    /// why -m` chunks MUST be attempted for any of them.
    Skipped(SkipReason),
}

/// Work-queue unit for the concurrent classifier orchestrator (US2 +
/// US3). US2 ships this enum with only the `Loose` variant used;
/// US3 (T033) extends the caller-site to also produce `Scope` jobs.
///
/// See specs/771-gomodwhy-subprocess-scale/data-model.md §AnalysisJob.
#[allow(dead_code)] // Wired in by US2 (T021) with Loose only; extended by US3 (T033).
#[derive(Debug)]
pub(super) enum AnalysisJob {
    /// Non-workspace main-module. Runs its own `go list all` preflight
    /// (per FR-008).
    Loose { main_module: PathBuf },
    /// Member of a detected go.work scope. Shares the preflight with
    /// other members of the same scope via [`SharedPreflightCache`]
    /// (per FR-006).
    Scope {
        scope: Arc<GoWorkScope>,
        member: PathBuf,
    },
}

// Suppress unused-import warning for `Arc`/`Mutex` while US2/US3 are
// unlanded; both types are needed by the AnalysisJob enum above but
// the Mutex is only exercised at the caller-site in US2/US3.
#[allow(dead_code)]
type _MutexAliveMarker = Mutex<()>;

/// Per-module classification verdict from `go mod why -m -vendor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoModWhyVerdict {
    /// Reachable from the main module's production import graph.
    ProdNeeded,
    /// Reachable only through a `.test` node in the import chain.
    TestOnly,
    /// `(main module does not need …)` — outside the build graph.
    NotNeeded,
    /// Empty/garbled section, missing section, or chunk-level failure.
    /// Eligible for the FR-001 unknown-marker pass.
    Unresolved,
}

/// Why analysis was skipped or degraded (FR-007 / FR-013 skip reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    NoToolchain,
    Disabled,
    BudgetExhausted,
    UnresolvablePackages,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::NoToolchain => "no-toolchain",
            SkipReason::Disabled => "disabled",
            SkipReason::BudgetExhausted => "budget-exhausted",
            SkipReason::UnresolvablePackages => "unresolvable-packages",
        }
    }
}

/// Result of analyzing one main module's dependency set.
#[derive(Debug, Default)]
pub struct MainModuleAnalysis {
    /// Module path → verdict. Every queried module path appears here
    /// (modules whose chunk failed or that lacked an output section
    /// are `Unresolved`) UNLESS the whole main module was skipped, in
    /// which case the map is empty.
    pub verdicts: HashMap<String, GoModWhyVerdict>,
    /// Set when analysis for this main module was skipped or cut
    /// short. `UnresolvablePackages` ⇒ `verdicts` is empty (the
    /// preflight gate). `BudgetExhausted` ⇒ verdicts already obtained
    /// are kept; the rest are `Unresolved`.
    pub skip_reason: Option<SkipReason>,
    /// Milestone 231 (FR-006): true when workspace mode was active for
    /// this main module (a `go.work` was found via ancestor walk OR
    /// `GOWORK` pointed at an explicit workspace file). Aggregated by
    /// the scan-level classifier into the `workspace_modules=` counter
    /// on the summary log line.
    pub workspace_active: bool,
}

/// Milestone 231 — Go workspace-mode state for one main-module
/// preflight invocation. Determines whether `-mod=mod` is safe to
/// force in the child-process env (it is NOT, when workspace mode is
/// active — Go rejects the flag with an error). Detection algorithm
/// lives in `detect_workspace_mode`; consequences in `apply_offline_env`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WorkspaceMode {
    /// `GOWORK=off` — workspace mode explicitly disabled by operator.
    Off,
    /// `GOWORK=auto` or unset AND no `go.work` on disk.
    Inactive,
    /// `GOWORK=auto` or unset AND `go.work` found via ancestor walk.
    /// The variant carries the discovered `go.work` path for logging.
    Active(PathBuf),
    /// `GOWORK=<explicit-path>` — explicit override; path is the
    /// (existing) file the operator pointed at.
    Explicit(PathBuf),
}

impl WorkspaceMode {
    /// True when Go's toolchain is in workspace mode for this module.
    /// Determines whether `-mod=mod` is safe to force in the env.
    fn is_active(&self) -> bool {
        matches!(self, WorkspaceMode::Active(_) | WorkspaceMode::Explicit(_))
    }
}

/// Milestone 231 — detect the Go workspace state for a single main-
/// module directory. Mirrors the Go toolchain's own resolution:
/// (1) honor `GOWORK` env var; (2) otherwise walk ancestors looking
/// for a `go.work` file. See `specs/231-fix-go-work-preflight/
/// contracts/go-work-detection.md § Detection algorithm` for the
/// authoritative contract.
///
/// Never errors: any filesystem-metadata failure at any level is
/// treated as "no `go.work` here, keep walking". The actual `go`
/// invocation is the source of truth on filesystem-actual failures.
pub(super) fn detect_workspace_mode(main_module_dir: &Path) -> WorkspaceMode {
    // (1) GOWORK env-var precedence.
    if let Ok(raw) = std::env::var("GOWORK") {
        let normalized = raw.trim();
        if normalized.eq_ignore_ascii_case("off") {
            return WorkspaceMode::Off;
        }
        if !normalized.is_empty() && !normalized.eq_ignore_ascii_case("auto") {
            // Treat as explicit path. If it exists, honor it.
            let explicit = PathBuf::from(normalized);
            if explicit.is_file() {
                let canon = std::fs::canonicalize(&explicit).unwrap_or(explicit);
                return WorkspaceMode::Explicit(canon);
            }
            // Missing explicit path → fall through to on-disk detection
            // (Go's own behavior on an invalid GOWORK is to error at
            // build time; waybill degrades to on-disk so the preflight
            // still runs. The `go list all` invocation itself will
            // surface any real error via the existing WARN path.)
        }
        // Empty string or "auto" → fall through.
    }

    // (2) Ancestor walk for go.work.
    for ancestor in main_module_dir.ancestors() {
        let candidate = ancestor.join("go.work");
        if candidate.is_file() {
            let canon = std::fs::canonicalize(&candidate).unwrap_or(candidate);
            return WorkspaceMode::Active(canon);
        }
    }
    WorkspaceMode::Inactive
}

/// Shared wall-clock budget across every subprocess in a scan.
#[derive(Debug)]
pub struct BudgetTracker {
    started: Instant,
    budget: Duration,
}

impl BudgetTracker {
    /// Budget from the contract: 60s, or the test-only
    /// `WAYBILL_GO_MOD_WHY_BUDGET_MS` integer-milliseconds override.
    pub fn from_env() -> Self {
        let budget = std::env::var("WAYBILL_GO_MOD_WHY_BUDGET_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_BUDGET);
        BudgetTracker { started: Instant::now(), budget }
    }

    /// Time left, or `None` when the budget is exhausted.
    pub fn remaining(&self) -> Option<Duration> {
        self.budget.checked_sub(self.started.elapsed()).filter(|d| !d.is_zero())
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }
}

/// `WAYBILL_NO_GO_MOD_WHY` opt-out: any non-empty value other than
/// `0` disables classification (contracts/cli-flags.md). The
/// `--no-go-mod-why` flag is bridged into this env var by `main.rs`.
pub fn classification_disabled() -> bool {
    match std::env::var("WAYBILL_NO_GO_MOD_WHY") {
        Ok(v) => !v.is_empty() && v != "0",
        Err(_) => false,
    }
}

/// Fast `go` availability probe (same approach as
/// `go_mod_graph.rs:90`). `false` ⇒ skip reason `no-toolchain`.
pub fn toolchain_available() -> bool {
    Command::new("go").arg("version").output().is_ok()
}

/// Offline env pinning per FR-012: applied when `--offline` /
/// `WAYBILL_OFFLINE` is in effect so the toolchain answers from local
/// cache or fails fast (and `GOTOOLCHAIN=local` blocks go.mod
/// `toolchain`-directive downloads).
///
/// Milestone 231: `GOFLAGS=-mod=mod` is INCOMPATIBLE with Go workspace
/// mode (Go rejects the flag with `-mod may only be set to readonly or
/// vendor when in workspace mode`). When `workspace_mode.is_active()`,
/// omit `GOFLAGS` entirely so Go's workspace default `-mod=readonly`
/// applies. Non-workspace paths preserve pre-231 behavior verbatim
/// (FR-003 byte-parity guarantee).
fn apply_offline_env(cmd: &mut Command, offline: bool, workspace_mode: &WorkspaceMode) {
    if offline {
        cmd.env("GOPROXY", "off");
        if !workspace_mode.is_active() {
            cmd.env("GOFLAGS", "-mod=mod");
        }
        cmd.env("GOTOOLCHAIN", "local");
    }
}

/// Outcome of one bounded subprocess invocation.
enum Invocation {
    Completed(std::process::Output),
    SpawnFailed(String),
    TimedOut,
}

/// Run a `go` subcommand in `cwd`, bounded by `timeout`. Uses the
/// spawn-thread plus `mpsc::recv_timeout` pattern from
/// `go_mod_graph.rs:113–146`: the worker thread keeps running past a
/// timeout but the subprocess gets reaped eventually; we simply stop
/// waiting.
fn run_bounded(
    cwd: &Path,
    args: &[String],
    offline: bool,
    timeout: Duration,
    workspace_mode: &WorkspaceMode,
) -> Invocation {
    use std::sync::mpsc;
    use std::thread;

    let (tx, rx) = mpsc::channel();
    let cwd = cwd.to_path_buf();
    let args = args.to_vec();
    let workspace_mode = workspace_mode.clone();
    thread::spawn(move || {
        let mut cmd = Command::new("go");
        cmd.args(&args).current_dir(&cwd);
        apply_offline_env(&mut cmd, offline, &workspace_mode);
        let _ = tx.send(cmd.output());
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Invocation::Completed(output),
        Ok(Err(e)) => Invocation::SpawnFailed(e.to_string()),
        Err(_) => Invocation::TimedOut,
    }
}

/// Classify `module_paths` against the main module rooted at
/// `main_module_dir` (the directory containing its `go.mod`).
///
/// Degrades, never errors: every failure path returns a
/// `MainModuleAnalysis` describing what happened. The caller decides
/// how verdicts map onto `PackageDbEntry` state (T015) and emits the
/// FR-013 summary (T020).
pub fn analyze_main_module(
    main_module_dir: &Path,
    module_paths: &[String],
    offline: bool,
    budget: &BudgetTracker,
) -> MainModuleAnalysis {
    let mut analysis = MainModuleAnalysis::default();
    if module_paths.is_empty() {
        return analysis;
    }

    // Milestone 231 (FR-001) — detect Go workspace mode once per main
    // module. Passed to every `run_bounded` invocation below so their
    // child-process env is workspace-compatible when appropriate.
    let workspace_mode = detect_workspace_mode(main_module_dir);
    analysis.workspace_active = workspace_mode.is_active();

    // Reliability preflight: `go list all` must succeed before ANY
    // `go mod why` verdict is trusted for this main module.
    let Some(remaining) = budget.remaining() else {
        tracing::warn!(
            main_module = %main_module_dir.display(),
            "go-mod-why analysis skipped (budget-exhausted): shared time \
             budget consumed before preflight; build-inclusion falls back \
             to unknown markers"
        );
        analysis.skip_reason = Some(SkipReason::BudgetExhausted);
        mark_unresolved(&mut analysis, module_paths);
        return analysis;
    };
    match run_bounded(main_module_dir, &["list".into(), "all".into()], offline, remaining, &workspace_mode) {
        Invocation::Completed(output) if output.status.success() => {}
        Invocation::Completed(output) => {
            tracing::warn!(
                main_module = %main_module_dir.display(),
                status = %output.status,
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "go-mod-why analysis skipped (unresolvable-packages): `go \
                 list all` preflight failed — `go mod why` would silently \
                 report false not-needed verdicts; build-inclusion falls \
                 back to unknown markers"
            );
            analysis.skip_reason = Some(SkipReason::UnresolvablePackages);
            return analysis;
        }
        Invocation::SpawnFailed(detail) => {
            tracing::warn!(
                main_module = %main_module_dir.display(),
                detail = %detail,
                "go-mod-why analysis skipped (unresolvable-packages): `go \
                 list all` preflight could not be spawned; build-inclusion \
                 falls back to unknown markers"
            );
            analysis.skip_reason = Some(SkipReason::UnresolvablePackages);
            return analysis;
        }
        Invocation::TimedOut => {
            tracing::warn!(
                main_module = %main_module_dir.display(),
                "go-mod-why analysis skipped (unresolvable-packages): `go \
                 list all` preflight exceeded the shared time budget; \
                 build-inclusion falls back to unknown markers"
            );
            analysis.skip_reason = Some(SkipReason::UnresolvablePackages);
            return analysis;
        }
    }

    // Chunked `go mod why -m -vendor` queries. m771 (issue #745): uses
    // `select_chunks` which bumps default batch size from 20 → 500
    // AND enforces the ARG_MAX_SAFE defensive cap. Byte-identical
    // output vs pre-m771 for workloads that previously fit in a
    // single 20-item chunk; batch-count reduction is transparent to
    // `parse_go_mod_why` (multi-section handling unchanged).
    for chunk in select_chunks(module_paths, CHUNK_SIZE, ARG_MAX_SAFE) {
        let Some(remaining) = budget.remaining() else {
            tracing::warn!(
                main_module = %main_module_dir.display(),
                "go-mod-why analysis cut short (budget-exhausted): shared \
                 time budget consumed; remaining modules fall back to \
                 unknown markers"
            );
            analysis.skip_reason = Some(SkipReason::BudgetExhausted);
            mark_unresolved(&mut analysis, chunk_and_rest(module_paths, chunk));
            return analysis;
        };
        let mut args: Vec<String> =
            vec!["mod".into(), "why".into(), "-m".into(), "-vendor".into()];
        args.extend(chunk.iter().cloned());
        match run_bounded(main_module_dir, &args, offline, remaining, &workspace_mode) {
            Invocation::Completed(output) if output.status.success() => {
                let parsed = parse_go_mod_why(&String::from_utf8_lossy(&output.stdout));
                for module in chunk {
                    let verdict = parsed
                        .get(module.as_str())
                        .copied()
                        .unwrap_or(GoModWhyVerdict::Unresolved);
                    analysis.verdicts.insert(module.clone(), verdict);
                }
            }
            Invocation::Completed(output) => {
                tracing::warn!(
                    main_module = %main_module_dir.display(),
                    status = %output.status,
                    stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                    "go-mod-why chunk degraded (subprocess-error): non-zero \
                     exit; this chunk's modules fall back to unknown markers"
                );
                mark_unresolved(&mut analysis, chunk);
            }
            Invocation::SpawnFailed(detail) => {
                tracing::warn!(
                    main_module = %main_module_dir.display(),
                    detail = %detail,
                    "go-mod-why chunk degraded (subprocess-error): spawn \
                     failed; this chunk's modules fall back to unknown markers"
                );
                mark_unresolved(&mut analysis, chunk);
            }
            Invocation::TimedOut => {
                tracing::warn!(
                    main_module = %main_module_dir.display(),
                    "go-mod-why analysis cut short (budget-exhausted): chunk \
                     exceeded the shared time budget; remaining modules fall \
                     back to unknown markers"
                );
                analysis.skip_reason = Some(SkipReason::BudgetExhausted);
                mark_unresolved(&mut analysis, chunk_and_rest(module_paths, chunk));
                return analysis;
            }
        }
    }

    analysis
}

/// The given chunk plus every module after it (used when abandoning
/// the remainder on budget exhaustion).
fn chunk_and_rest<'a>(all: &'a [String], chunk: &'a [String]) -> &'a [String] {
    // `chunk` is a sub-slice of `all` produced by `chunks()`, so
    // pointer arithmetic gives its offset.
    let offset = (chunk.as_ptr() as usize - all.as_ptr() as usize)
        / std::mem::size_of::<String>();
    &all[offset..]
}

fn mark_unresolved(analysis: &mut MainModuleAnalysis, modules: &[String]) {
    for module in modules {
        analysis
            .verdicts
            .entry(module.clone())
            .or_insert(GoModWhyVerdict::Unresolved);
    }
}

/// Parse `go mod why -m -vendor` stdout into module-path → verdict.
///
/// Output is a sequence of sections, each headed by `# <module-path>`:
///
/// - a body line starting with `(main module does not need` →
///   [`GoModWhyVerdict::NotNeeded`]. The prefix covers both the plain
///   phrasing (`does not need module X`) and the `-vendor` phrasing
///   (`does not need to vendor module X`) — verified on go 1.26.2;
/// - an import chain (one package per line) containing a node with a
///   `.test` suffix → [`GoModWhyVerdict::TestOnly`];
/// - a non-empty chain with no `.test` node →
///   [`GoModWhyVerdict::ProdNeeded`];
/// - an empty or unparseable body → [`GoModWhyVerdict::Unresolved`].
///
/// Never errors; lines before the first header are ignored.
pub fn parse_go_mod_why(stdout: &str) -> HashMap<String, GoModWhyVerdict> {
    let mut verdicts = HashMap::new();
    let mut current: Option<(String, Vec<String>)> = None;

    let flush = |section: Option<(String, Vec<String>)>,
                     verdicts: &mut HashMap<String, GoModWhyVerdict>| {
        if let Some((module, body)) = section {
            verdicts.insert(module, classify_section(&body));
        }
    };

    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix('#') {
            flush(current.take(), &mut verdicts);
            let module = header.trim();
            if !module.is_empty() {
                current = Some((module.to_string(), Vec::new()));
            }
        } else if let Some((_, body)) = current.as_mut() {
            if !trimmed.is_empty() {
                body.push(trimmed.to_string());
            }
        }
    }
    flush(current.take(), &mut verdicts);
    verdicts
}

fn classify_section(body: &[String]) -> GoModWhyVerdict {
    if body.is_empty() {
        return GoModWhyVerdict::Unresolved;
    }
    if body.iter().any(|l| l.starts_with("(main module does not need")) {
        return GoModWhyVerdict::NotNeeded;
    }
    // A parenthesized body that isn't the not-needed message is some
    // other diagnostic (e.g. `(module X is not in the module graph)`)
    // — treat as unresolved rather than guessing.
    if body.iter().all(|l| l.starts_with('(')) {
        return GoModWhyVerdict::Unresolved;
    }
    if body.iter().any(|l| l.ends_with(".test")) {
        return GoModWhyVerdict::TestOnly;
    }
    GoModWhyVerdict::ProdNeeded
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // Milestone 771 — US1 CHUNK_SIZE + argv-length guard tests.
    // Ordered first so a `cargo test m771_` grep surfaces them together.
    // -------------------------------------------------------------------

    /// FR-001 regression pin: catches any accidental revert of the m771
    /// CHUNK_SIZE bump. Constant intentionally checked as a literal 500
    /// to make the pin's intent obvious.
    #[test]
    fn m771_chunk_size_default_is_500() {
        assert_eq!(
            CHUNK_SIZE, 500,
            "FR-001: CHUNK_SIZE MUST be 500 by default post-m771; \
             pre-m771 value was 20 (subprocess-spawn amplification)."
        );
    }

    /// FR-001 + FR-002 happy path: 246 short paths (k8s-shape workload)
    /// fits in a single 500-item batch. Verifies the argv-guard does
    /// NOT bisect normal workloads unnecessarily.
    #[test]
    fn m771_argv_guard_passes_normal_workload_intact() {
        // ~50-char path average, matches real Go module coordinate
        // shape (github.com/owner/repo/v2).
        let paths: Vec<String> = (0..246)
            .map(|i| format!("example.com/waybill-fixture/repo-{:03}/pkg/v2", i))
            .collect();
        let chunks = select_chunks(&paths, CHUNK_SIZE, ARG_MAX_SAFE);
        assert_eq!(
            chunks.len(),
            1,
            "246 short paths MUST fit in a single 500-item batch; got \
             {} chunks. First chunk length: {}",
            chunks.len(),
            chunks.first().map(|c| c.len()).unwrap_or(0),
        );
        assert_eq!(
            chunks[0].len(),
            246,
            "single chunk MUST contain all 246 paths",
        );
    }

    // -------------------------------------------------------------------
    // Milestone 771 — US2 concurrent-orchestration helper tests.
    // -------------------------------------------------------------------

    #[test]
    fn m771_worker_count_bounded_by_available_parallelism() {
        let cap = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        // Many-workspace case: capped at available_parallelism().
        assert_eq!(
            worker_count(100),
            std::cmp::min(100, cap),
            "FR-003: 100 workspaces MUST cap at available_parallelism() = {}",
            cap,
        );
        // Single workspace: 1 worker (parallelism has nothing to bite on).
        assert_eq!(worker_count(1), 1, "single workspace should return 1");
        // Zero workspaces: 0 workers (no work to do).
        assert_eq!(worker_count(0), 0, "zero workspaces should return 0");
        // Two workspaces on any reasonable machine: min(2, cap>=1).
        assert!(
            worker_count(2) <= 2 && worker_count(2) >= 1,
            "2 workspaces should yield 1 or 2 workers, got {}",
            worker_count(2),
        );
    }

    #[test]
    fn m771_budget_tracker_shared_across_arc_clones() {
        // FR-004: `Arc<BudgetTracker>` clones share the same wall-clock
        // origin. Two threads observing `.remaining()` MUST see
        // monotonically-decreasing values and MUST both agree once the
        // budget is exhausted.
        let key = "WAYBILL_GO_MOD_WHY_BUDGET_MS";
        // Use a small budget for test speed; guard the env var via
        // set_var/remove_var (per project convention — no EnvGuard for
        // this one-off since we don't intersect other tests via the
        // same key at the same time).
        let prior = std::env::var(key).ok();
        std::env::set_var(key, "200");
        let tracker = std::sync::Arc::new(BudgetTracker::from_env());
        let t1 = tracker.clone();
        let t2 = tracker.clone();
        let h1 = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let r1 = t1.remaining();
            std::thread::sleep(Duration::from_millis(200));
            let r2 = t1.remaining();
            (r1, r2)
        });
        let h2 = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let r1 = t2.remaining();
            std::thread::sleep(Duration::from_millis(200));
            let r2 = t2.remaining();
            (r1, r2)
        });
        let (t1_r1, t1_r2) = h1.join().expect("thread 1");
        let (t2_r1, t2_r2) = h2.join().expect("thread 2");
        // Both threads observe non-None at 50ms (budget = 200ms).
        assert!(
            t1_r1.is_some() && t2_r1.is_some(),
            "both threads should see remaining>0 at 50ms; t1={:?} t2={:?}",
            t1_r1,
            t2_r1,
        );
        // Both threads observe None at 250ms (budget exhausted).
        assert!(
            t1_r2.is_none() && t2_r2.is_none(),
            "both threads should see remaining=None at 250ms; t1={:?} t2={:?}",
            t1_r2,
            t2_r2,
        );
        // Restore env.
        match prior {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    /// FR-002 argv-guard bisection: 500 paths at 300 chars each project
    /// to ~150 KB argv (500 × 301 + 24 fixed = 150,524 bytes), well
    /// above the 96 KiB (98,304 bytes) cap. Guard MUST bisect until
    /// every sub-batch's projected argv fits.
    #[test]
    fn m771_argv_guard_bisects_when_projected_length_exceeds_limit() {
        let long_path = "example.com/waybill-fixture/".to_string()
            + &"x".repeat(272); // 28 + 272 = 300-char path
        assert_eq!(long_path.len(), 300);
        let paths: Vec<String> = (0..500).map(|_| long_path.clone()).collect();
        let chunks = select_chunks(&paths, CHUNK_SIZE, ARG_MAX_SAFE);
        assert!(
            chunks.len() >= 2,
            "FR-002: pathologically long paths MUST trigger bisection; \
             got {} chunks",
            chunks.len(),
        );
        // Every returned sub-batch's projected argv MUST fit under
        // the cap (except single-path chunks where the guard is
        // unreachable per algorithm — PATH_MAX bounds the single case).
        for (i, chunk) in chunks.iter().enumerate() {
            let projected = 24usize
                + chunk.iter().map(|p| p.len() + 1).sum::<usize>();
            if chunk.len() > 1 {
                assert!(
                    projected <= ARG_MAX_SAFE,
                    "FR-002: chunk[{}] len={} projects to {} bytes, \
                     exceeds ARG_MAX_SAFE={}",
                    i,
                    chunk.len(),
                    projected,
                    ARG_MAX_SAFE,
                );
            }
        }
        // Every input path MUST appear exactly once across the output.
        let total_len: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(
            total_len,
            paths.len(),
            "FR-002: bisection MUST preserve total path count",
        );
    }

    #[test]
    fn parses_prod_needed_chain() {
        let out = "# github.com/google/uuid\n\
                   sigs.k8s.io/cri-tools/cmd/crictl\n\
                   github.com/google/uuid\n";
        let v = parse_go_mod_why(out);
        assert_eq!(v["github.com/google/uuid"], GoModWhyVerdict::ProdNeeded);
    }

    #[test]
    fn parses_test_only_chain() {
        let out = "# github.com/stretchr/testify\n\
                   example.com/app\n\
                   example.com/app.test\n\
                   github.com/stretchr/testify/assert\n";
        let v = parse_go_mod_why(out);
        assert_eq!(
            v["github.com/stretchr/testify"],
            GoModWhyVerdict::TestOnly
        );
    }

    #[test]
    fn parses_not_needed_plain_phrasing() {
        let out = "# github.com/beorn7/perks\n\
                   (main module does not need module github.com/beorn7/perks)\n";
        let v = parse_go_mod_why(out);
        assert_eq!(v["github.com/beorn7/perks"], GoModWhyVerdict::NotNeeded);
    }

    #[test]
    fn parses_not_needed_vendor_phrasing() {
        let out = "# github.com/beorn7/perks\n\
                   (main module does not need to vendor module github.com/beorn7/perks)\n";
        let v = parse_go_mod_why(out);
        assert_eq!(v["github.com/beorn7/perks"], GoModWhyVerdict::NotNeeded);
    }

    #[test]
    fn empty_section_is_unresolved() {
        let out = "# github.com/empty/module\n# github.com/google/uuid\nexample.com/app\ngithub.com/google/uuid\n";
        let v = parse_go_mod_why(out);
        assert_eq!(v["github.com/empty/module"], GoModWhyVerdict::Unresolved);
        assert_eq!(v["github.com/google/uuid"], GoModWhyVerdict::ProdNeeded);
    }

    #[test]
    fn unknown_parenthesized_diagnostic_is_unresolved() {
        let out = "# github.com/odd/module\n\
                   (module github.com/odd/module is not in the module graph)\n";
        let v = parse_go_mod_why(out);
        assert_eq!(v["github.com/odd/module"], GoModWhyVerdict::Unresolved);
    }

    #[test]
    fn multi_section_output() {
        let out = "# a.example/prod\n\
                   main.example/app\n\
                   a.example/prod\n\
                   \n\
                   # b.example/testonly\n\
                   main.example/app\n\
                   main.example/app.test\n\
                   b.example/testonly\n\
                   \n\
                   # c.example/unneeded\n\
                   (main module does not need module c.example/unneeded)\n";
        let v = parse_go_mod_why(out);
        assert_eq!(v.len(), 3);
        assert_eq!(v["a.example/prod"], GoModWhyVerdict::ProdNeeded);
        assert_eq!(v["b.example/testonly"], GoModWhyVerdict::TestOnly);
        assert_eq!(v["c.example/unneeded"], GoModWhyVerdict::NotNeeded);
    }

    #[test]
    fn garbage_before_first_header_ignored() {
        let out = "warning: something\n# a.example/m\nmain.example/app\na.example/m\n";
        let v = parse_go_mod_why(out);
        assert_eq!(v.len(), 1);
        assert_eq!(v["a.example/m"], GoModWhyVerdict::ProdNeeded);
    }

    #[test]
    fn empty_output_yields_no_verdicts() {
        assert!(parse_go_mod_why("").is_empty());
    }

    #[test]
    fn bare_hash_header_is_skipped() {
        let out = "#\nsome.example/line\n";
        assert!(parse_go_mod_why(out).is_empty());
    }

    #[test]
    fn disabled_env_semantics() {
        // NOTE: process-global env — keep all cases in ONE test to
        // avoid parallel-test races on the same var.
        let key = "WAYBILL_NO_GO_MOD_WHY";
        let prior = std::env::var(key).ok();
        std::env::remove_var(key);
        assert!(!classification_disabled());
        std::env::set_var(key, "0");
        assert!(!classification_disabled());
        std::env::set_var(key, "");
        assert!(!classification_disabled());
        std::env::set_var(key, "1");
        assert!(classification_disabled());
        std::env::set_var(key, "true");
        assert!(classification_disabled());
        match prior {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn budget_tracker_env_override() {
        let key = "WAYBILL_GO_MOD_WHY_BUDGET_MS";
        let prior = std::env::var(key).ok();
        std::env::set_var(key, "50");
        let tracker = BudgetTracker::from_env();
        assert!(tracker.budget <= Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(60));
        assert!(tracker.remaining().is_none());
        match prior {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn chunk_and_rest_returns_suffix() {
        // Uses a local chunk size (not the global CHUNK_SIZE) so
        // future m771-scale bumps to CHUNK_SIZE don't break this
        // pointer-arithmetic invariant test. `chunk_and_rest` only
        // requires that `chunk` be a sub-slice of `all` — the chunk
        // size is irrelevant to its correctness.
        const LOCAL_CHUNK: usize = 20;
        let all: Vec<String> = (0..45).map(|i| format!("m{i}")).collect();
        let chunks: Vec<&[String]> = all.chunks(LOCAL_CHUNK).collect();
        assert_eq!(chunk_and_rest(&all, chunks[1]).len(), 25);
        assert_eq!(chunk_and_rest(&all, chunks[2]).len(), 5);
        assert_eq!(chunk_and_rest(&all, chunks[0]).len(), 45);
    }

    // =========================================================
    // Milestone 231 — WorkspaceMode detection + env-effect tests
    // =========================================================

    use crate::testing::env_guard::EnvGuard;

    fn write_gowork(dir: &Path) {
        std::fs::write(dir.join("go.work"), "go 1.22\n").unwrap();
    }

    #[test]
    fn detect_workspace_mode_returns_off_when_env_off() {
        // Contract invariant #1 (SC-005) — GOWORK=off + go.work on disk → Off.
        let mut env = EnvGuard::acquire();
        env.set("GOWORK", "off");
        let tmp = tempfile::tempdir().unwrap();
        write_gowork(tmp.path());
        assert_eq!(detect_workspace_mode(tmp.path()), WorkspaceMode::Off);
    }

    #[test]
    fn detect_workspace_mode_returns_inactive_when_no_go_work() {
        // Contract invariant #2 — GOWORK unset + no go.work → Inactive.
        let mut env = EnvGuard::acquire();
        env.remove("GOWORK");
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(detect_workspace_mode(tmp.path()), WorkspaceMode::Inactive);
    }

    #[test]
    fn detect_workspace_mode_active_from_immediate_parent() {
        // Contract invariant #3 — go.work at parent of module dir.
        let mut env = EnvGuard::acquire();
        env.remove("GOWORK");
        let tmp = tempfile::tempdir().unwrap();
        write_gowork(tmp.path());
        let module = tmp.path().join("sub");
        std::fs::create_dir(&module).unwrap();
        match detect_workspace_mode(&module) {
            WorkspaceMode::Active(p) => assert!(
                p.ends_with("go.work"),
                "Active variant must carry the go.work path; got {}",
                p.display()
            ),
            other => panic!("expected Active(_), got {:?}", other),
        }
    }

    #[test]
    fn detect_workspace_mode_active_from_two_levels_up() {
        // Contract invariant #4 — go.work at grandparent (multi-level walk).
        let mut env = EnvGuard::acquire();
        env.remove("GOWORK");
        let tmp = tempfile::tempdir().unwrap();
        write_gowork(tmp.path());
        let module = tmp.path().join("a").join("b");
        std::fs::create_dir_all(&module).unwrap();
        assert!(matches!(
            detect_workspace_mode(&module),
            WorkspaceMode::Active(_)
        ));
    }

    #[test]
    fn detect_workspace_mode_explicit_path_returns_explicit() {
        // Contract invariant #5 — GOWORK=<existing-path> → Explicit.
        let mut env = EnvGuard::acquire();
        let tmp = tempfile::tempdir().unwrap();
        write_gowork(tmp.path());
        let explicit = tmp.path().join("go.work");
        env.set("GOWORK", &explicit);
        // Detection is called against an unrelated directory; the explicit
        // path takes precedence regardless.
        let unrelated = tempfile::tempdir().unwrap();
        match detect_workspace_mode(unrelated.path()) {
            WorkspaceMode::Explicit(p) => assert!(
                p.ends_with("go.work"),
                "Explicit variant must carry the pointed-at path; got {}",
                p.display()
            ),
            other => panic!("expected Explicit(_), got {:?}", other),
        }
    }

    #[test]
    fn detect_workspace_mode_falls_through_when_explicit_missing() {
        // Contract invariant #6 — GOWORK=<nonexistent> + go.work on disk
        // → fall through to on-disk detection (Active).
        let mut env = EnvGuard::acquire();
        env.set("GOWORK", "/nonexistent/nowhere/go.work");
        let tmp = tempfile::tempdir().unwrap();
        write_gowork(tmp.path());
        assert!(matches!(
            detect_workspace_mode(tmp.path()),
            WorkspaceMode::Active(_)
        ));
    }

    #[test]
    fn apply_offline_env_workspace_omits_goflags() {
        // FR-002 — workspace active + offline → omit GOFLAGS.
        let mut cmd = Command::new("echo");
        let mode = WorkspaceMode::Active(PathBuf::from("/tmp/fake-go.work"));
        apply_offline_env(&mut cmd, true, &mode);
        let envs: HashMap<_, _> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string())))
            .collect();
        assert_eq!(envs.get("GOPROXY"), Some(&Some("off".to_string())));
        assert_eq!(envs.get("GOTOOLCHAIN"), Some(&Some("local".to_string())));
        assert!(
            !envs.contains_key("GOFLAGS"),
            "GOFLAGS must NOT be set when workspace mode is active; env: {:?}",
            envs
        );
    }

    #[test]
    fn apply_offline_env_non_workspace_keeps_mod_mod() {
        // FR-003 — non-workspace + offline → pre-231 byte-parity:
        // GOFLAGS=-mod=mod.
        let mut cmd = Command::new("echo");
        apply_offline_env(&mut cmd, true, &WorkspaceMode::Inactive);
        let envs: HashMap<_, _> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string())))
            .collect();
        assert_eq!(envs.get("GOFLAGS"), Some(&Some("-mod=mod".to_string())));
        assert_eq!(envs.get("GOPROXY"), Some(&Some("off".to_string())));
        assert_eq!(envs.get("GOTOOLCHAIN"), Some(&Some("local".to_string())));
    }

    #[test]
    fn apply_offline_env_gowork_off_keeps_mod_mod() {
        // FR-003 + SC-005 — GOWORK=off drops workspace mode; the
        // resulting Off variant preserves pre-231 GOFLAGS=-mod=mod.
        let mut cmd = Command::new("echo");
        apply_offline_env(&mut cmd, true, &WorkspaceMode::Off);
        let envs: HashMap<_, _> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string())))
            .collect();
        assert_eq!(envs.get("GOFLAGS"), Some(&Some("-mod=mod".to_string())));
    }
}
