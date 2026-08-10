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
use std::time::{Duration, Instant};

/// Maximum module paths per `go mod why` invocation.
const CHUNK_SIZE: usize = 20;

/// Default shared budget across all invocations in a scan.
const DEFAULT_BUDGET: Duration = Duration::from_secs(60);

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

    // Chunked `go mod why -m -vendor` queries.
    for chunk in module_paths.chunks(CHUNK_SIZE) {
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
        let all: Vec<String> = (0..45).map(|i| format!("m{i}")).collect();
        let chunks: Vec<&[String]> = all.chunks(CHUNK_SIZE).collect();
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
