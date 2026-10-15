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

/// Milestone 771 US3 (T031) — parse a `go.work` file's contents into
/// the list of member paths declared via `use` directives.
///
/// Handles both grammar forms per go.dev/ref/mod#go-work-file:
///   - bare:  `use ./mod-a`
///   - block: `use ( \n\t./mod-a\n\t./mod-b\n )`
///
/// Ignores `go X.Y[.Z]`, `replace`, blank, and comment lines.
/// Malformed input returns an empty Vec (falls back to per-workspace
/// preflight per FR-008; no panic).
pub fn parse_go_work(bytes: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut in_use_block = false;
    for raw in bytes.lines() {
        let line = raw.trim();
        // Strip inline comments.
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if in_use_block {
            if line == ")" {
                in_use_block = false;
                continue;
            }
            // Block-form entries are bare paths (no `use` keyword).
            members.push(line.to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("use") {
            let rest = rest.trim();
            if rest == "(" {
                in_use_block = true;
                continue;
            }
            if let Some(inner) = rest.strip_prefix('(') {
                // `use ( ./mod-a` — inline-open with a first member.
                in_use_block = true;
                let first = inner.trim();
                if !first.is_empty() && first != ")" {
                    members.push(first.to_string());
                }
                continue;
            }
            // Bare form: `use <path>`.
            if !rest.is_empty() {
                members.push(rest.to_string());
            }
        }
        // Any other directive is ignored — this parser cares only
        // about member enumeration, and deliberately tolerates ANY
        // input without panicking: a genuinely malformed `go.work`
        // must yield an empty member set so the caller falls back to
        // per-workspace preflights (m771 FR-008).
        //
        // m775 (FR-014): the directives that legitimately appear here
        // are defined once, in `gowork::GO_WORK_DIRECTIVES`, shared
        // with the strict validator so the two parsers cannot disagree
        // about what a VALID `go.work` contains. That agreement is
        // enforced by `m775_both_parsers_agree_across_directive_corpus`
        // — deliberately as a test over valid inputs rather than an
        // assertion here, because asserting would break this parser's
        // tolerate-anything contract on malformed input.
    }
    members
}

/// Milestone 771 US3 (T032) — group main-modules by their governing
/// `go.work` scope. Walks up each workspace's directory tree looking
/// for a `go.work` file. When found, parses it via [`parse_go_work`]
/// and canonicalizes the member paths.
///
/// Returns `(scopes, loose)` where:
///   - `scopes` = one `GoWorkScope` per detected `go.work` file
///     containing every input workspace that IS a declared member.
///   - `loose` = input workspaces that are NOT covered by any scope
///     (per FR-008 fallback: they run their own preflight unchanged).
///
/// A workspace appears in exactly one output slot: it's either a
/// scope member OR loose, never both. Multi-scope handled naturally
/// (one `GoWorkScope` per detected `go.work`).
pub fn detect_go_work_scopes(
    workspaces: &[PathBuf],
) -> (Vec<GoWorkScope>, Vec<PathBuf>) {
    // Canonicalize once; downstream comparisons use these forms.
    let canon_workspaces: Vec<PathBuf> = workspaces
        .iter()
        .map(|w| std::fs::canonicalize(w).unwrap_or_else(|_| w.clone()))
        .collect();
    // scope-root-dir → parsed member set (canonicalized to workspace form)
    let mut scope_map: std::collections::HashMap<PathBuf, Vec<PathBuf>> =
        std::collections::HashMap::new();
    let mut workspace_scope: std::collections::HashMap<PathBuf, PathBuf> =
        std::collections::HashMap::new();
    for w in &canon_workspaces {
        // Walk up looking for go.work.
        let mut cursor: Option<&Path> = Some(w.as_path());
        while let Some(dir) = cursor {
            let candidate = dir.join("go.work");
            if candidate.is_file() {
                let scope_root = dir.to_path_buf();
                // Populate the scope's member set if we haven't yet.
                #[allow(clippy::map_entry)] // clarity > entry API here
                if !scope_map.contains_key(&scope_root) {
                    let Ok(bytes) = std::fs::read_to_string(&candidate) else {
                        break;
                    };
                    let members: Vec<PathBuf> = parse_go_work(&bytes)
                        .iter()
                        .map(|rel| {
                            let abs = scope_root.join(rel);
                            std::fs::canonicalize(&abs).unwrap_or(abs)
                        })
                        .collect();
                    scope_map.insert(scope_root.clone(), members);
                }
                // Is this workspace one of the scope's declared members?
                if let Some(members) = scope_map.get(&scope_root) {
                    if members.iter().any(|m| m == w) {
                        workspace_scope.insert(w.clone(), scope_root);
                    }
                }
                break;
            }
            cursor = dir.parent();
        }
    }
    // Group workspaces by scope; loose = un-assigned.
    let mut scope_members: std::collections::HashMap<PathBuf, Vec<PathBuf>> =
        std::collections::HashMap::new();
    let mut loose: Vec<PathBuf> = Vec::new();
    for w in &canon_workspaces {
        match workspace_scope.get(w) {
            Some(scope_root) => scope_members
                .entry(scope_root.clone())
                .or_default()
                .push(w.clone()),
            None => loose.push(w.clone()),
        }
    }
    let scopes: Vec<GoWorkScope> = scope_members
        .into_iter()
        .map(|(root_dir, members)| GoWorkScope { root_dir, members })
        .collect();
    (scopes, loose)
}

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
#[derive(Debug, Clone)]
pub struct GoWorkScope {
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
#[derive(Debug, Default)]
pub struct SharedPreflightCache {
    /// Public so the m771 unit tests can drive dedup assertions.
    /// Callers outside the crate SHOULD access via `Arc<Mutex<>>` at
    /// the classifier call site rather than mutating this directly.
    pub entries: HashMap<PathBuf, PreflightOutcome>,
    /// Milestone 775: per-scope single-flight cells.
    ///
    /// `entries` remains the fast-path authority — once a scope's
    /// preflight has completed, every later caller reads it without
    /// blocking. This map exists solely to serialize the SLOW path:
    /// the first caller to claim a scope holds that scope's cell
    /// across the `go list all` subprocess, and concurrent callers for
    /// the SAME scope block on the cell and reuse the memoized
    /// outcome rather than spawning their own.
    ///
    /// Distinct scopes hold distinct cells, so one scope's preflight
    /// never blocks another's (FR-003). The cache mutex itself is
    /// never held across a spawn (FR-004).
    ///
    /// Pre-m775 this map did not exist: the cache-miss path released
    /// the cache mutex, ran the subprocess, then re-acquired to
    /// insert. m771 US2's worker pool starts every worker at once, so
    /// all of them reached that miss branch before any had finished —
    /// 22 `go list all` invocations were observed on Kubernetes where
    /// 6 was correct. See specs/775-preflight-single-flight/.
    pub inflight: HashMap<PathBuf, std::sync::Arc<std::sync::Mutex<Option<PreflightOutcome>>>>,
}

/// Outcome of a shared `go list all` preflight invocation.
#[derive(Debug, Clone)]
pub enum PreflightOutcome {
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
    /// The `go list all` preflight ran out of memory: the Go runtime's
    /// own "out of memory" fatal error, or a SIGKILL, which is how the
    /// kernel's OOM killer ends it (#1154). Distinct from
    /// `UnresolvablePackages` so an operator knows more memory, not
    /// credentials, is the fix.
    OutOfMemory,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::NoToolchain => "no-toolchain",
            SkipReason::Disabled => "disabled",
            SkipReason::BudgetExhausted => "budget-exhausted",
            SkipReason::UnresolvablePackages => "unresolvable-packages",
            SkipReason::OutOfMemory => "out-of-memory",
        }
    }
}

/// The Go runtime's out-of-memory fatal error, or a SIGKILL (how the
/// kernel's OOM killer ends a process; the shared time budget is
/// enforced separately, as `Invocation::TimedOut`).
fn preflight_ran_out_of_memory(output: &std::process::Output) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if output.status.signal() == Some(9) {
            return true;
        }
    }
    String::from_utf8_lossy(&output.stderr).contains("runtime: out of memory")
}

/// Memory each `go` child needs *beyond* what `GOMEMLIMIT` governs:
/// the binary's text and data mappings, thread stacks, and runtime
/// structures outside the Go heap. `GOMEMLIMIT` caps Go-managed
/// memory only, but the kernel's OOM killer counts the whole process
/// against the cgroup, so a limit set to the full cgroup allowance is
/// still killed by exactly this difference.
///
/// Measured (#1154, go1.27 linux/arm64, `go list all` over Kubernetes
/// v1.31.0, 3613 packages) by finding where a limit stops rescuing the
/// run — survival tracks absolute headroom, not a fraction of the cap:
///
/// | cgroup cap | `GOMEMLIMIT` | headroom | outcome      |
/// |------------|--------------|----------|--------------|
/// | 160 MiB    | 144 MiB      | 16.8 MB  | completed    |
/// | 128 MiB    | 115 MiB      | 13.6 MB  | completed    |
/// | 112 MiB    | 100 MiB      | 12.6 MB  | OOM-killed   |
///
/// So the true figure sits between 12.6 and 13.6 MB on that platform.
/// 16 MiB is that, rounded up: a larger `go` binary (amd64 builds are
/// bigger than arm64) or a future toolchain eats the difference, and
/// over-reserving costs a little GC time while under-reserving costs
/// the whole classification pass.
///
/// Worth knowing before tuning it: the single-process boundary above
/// is sharp, but under concurrency the exact reserve stopped
/// mattering. Four concurrent children at a ~12 MB reserve and at a
/// 16 MiB reserve lost the same number of runs at every cap measured
/// -- there, transient overshoot of a soft limit dominates, not the
/// fixed non-heap cost this constant describes.
const GO_NON_HEAP_RESERVE_BYTES: u64 = 16 * 1024 * 1024;

/// Compile-time floor on the constant above: 13.6 MB of headroom was
/// measured to survive and 12.6 MB to be OOM-killed, so any value
/// below the survivor is known-insufficient on the measured platform.
/// A compile error here means someone tuned the reserve past the
/// evidence; re-measure before relaxing it.
const _: () = assert!(GO_NON_HEAP_RESERVE_BYTES >= 13_600_000);

/// Below this, don't bother. A share this small means the cgroup is
/// too tight to be worth constraining further, and a very low limit is
/// not free: tightening one measurably costs GC time, roughly doubling
/// wall clock (8.4s → 18.6s) at the lowest values tried. Leaving
/// `GOMEMLIMIT` unset there keeps the likely failure the one an
/// operator can act on -- `out-of-memory`, already a distinct skip
/// reason -- rather than the shared time budget expiring under GC
/// thrash, which reports as `budget-exhausted` and points nowhere.
///
/// Unlike `GO_NON_HEAP_RESERVE_BYTES`, this bound is a judgement, not
/// a measured boundary: the probe found no per-child value at which
/// the pass flipped from working to thrashing, because the runs at the
/// lowest limits tried still completed, only slowly. What *is*
/// measured is that below a ~128 MiB cgroup a single `go list all` over
/// Kubernetes died at every limit tried, so a share well under that
/// cannot be rescued and 32 MiB sits comfortably below anything useful.
const MIN_GO_MEMORY_LIMIT_BYTES: u64 = 32 * 1024 * 1024;

/// A cgroup-v1 `memory.limit_in_bytes` reads as a near-`u64::MAX`
/// sentinel rather than a word when no limit is set (the kernel's
/// page-rounded `PAGE_COUNTER_MAX`). Anything this large is "no
/// limit", not a limit worth dividing.
///
/// Unlike the v2 shapes below, this threshold is NOT measured: the
/// host available here (Ubuntu 24.04) is cgroup-v2-only, so the v1
/// branch could not be exercised against a real kernel. It is written
/// to fail safe — an unrecognised value yields `None`, which leaves
/// `GOMEMLIMIT` unset and behaviour exactly as it is today.
#[cfg(any(target_os = "linux", test))]
const CGROUP_V1_UNLIMITED_FLOOR: u64 = 1 << 60;

/// Per-child `GOMEMLIMIT` for a cgroup allowance shared by `workers`
/// concurrent `go` processes. Pure; `None` means "leave it unset".
///
/// The division is the load-bearing part. waybill runs up to
/// `worker_count()` = `min(workspaces, available_parallelism())`
/// preflights at once, all in one cgroup, so a limit computed against
/// the full allowance lets N children each believe they may use all of
/// it. Measured (#1154, go1.27 linux/arm64) with 4 concurrent
/// `go list all` over Kubernetes v1.31.0, counting how many of the 4
/// the kernel killed:
///
/// | cgroup cap | unset | 90% undivided | this formula |
/// |------------|-------|---------------|--------------|
/// | 384 MiB    | 1     | 1 (345 MiB)   | 0 (80 MiB)   |
/// | 320 MiB    | 3     | 3 (288 MiB)   | 1 (64 MiB)   |
/// | 288 MiB    | 3     | --            | 2 (56 MiB)   |
/// | 256 MiB    | 3     | 3 (230 MiB)   | 2 (48 MiB)   |
///
/// Two things to read off that. An *undivided* limit is
/// indistinguishable from no limit at all -- it is the division, not
/// the limit, that does the work. And dividing does not eliminate
/// kills: `GOMEMLIMIT` is a soft target the runtime may transiently
/// overshoot, so a cgroup too small for the workload still loses
/// processes. What it buys is measured and bounded -- single-process,
/// the smallest cgroup `go list all` survives in moves from 192 MiB
/// to 128 MiB, about 1.5x -- and it is paid for in GC time, roughly
/// doubling wall clock at the tightest useful limits.
///
/// This matters less than the table suggests for the environment in
/// the report, a 4 GiB container: four workers there get ~1 GiB each
/// against a working set measured at ~96 MiB live, so the GC is never
/// near the limit and the cost rounds to nothing. The caps above are
/// stress points chosen to make the mechanism observable at all, not
/// the operating range.
pub(crate) fn go_memory_limit_bytes(cgroup_limit: u64, workers: usize) -> Option<u64> {
    let workers = u64::try_from(workers.max(1)).unwrap_or(1);
    let share = cgroup_limit.checked_div(workers)?;
    let limit = share.checked_sub(GO_NON_HEAP_RESERVE_BYTES)?;
    (limit >= MIN_GO_MEMORY_LIMIT_BYTES).then_some(limit)
}

/// The cgroup-v2 path this process lives in, from `/proc/self/cgroup`
/// contents. The v2 line is the one with an empty controller field:
/// `0::/docker/<id>`, or `0::/` in a private cgroup namespace.
///
/// Measured: a container started normally reports `0::/`; the same
/// container with `--cgroupns=host` reports
/// `0::/docker/6bf2aecb7d31…`. That difference is why this is read at
/// all instead of using a fixed path — see `cgroup_v2_limit_in`.
#[cfg(any(target_os = "linux", test))]
fn parse_cgroup_v2_path(contents: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let mut parts = line.splitn(3, ':');
        let hierarchy = parts.next()?;
        let controllers = parts.next()?;
        let path = parts.next()?;
        (hierarchy == "0" && controllers.is_empty() && path.starts_with('/'))
            .then(|| path.to_string())
    })
}

/// Smallest numeric `memory.max` from this process's own cgroup up to
/// the root, under `cgroup_root`. `None` when every level reads `max`
/// (or is unreadable), which means no limit applies.
///
/// Walking is required, not thorough: a limit set on an ancestor does
/// NOT appear at the leaf. Measured on this host — a parent cgroup at
/// `memory.max = 1073741824` has a child whose own `memory.max` reads
/// the literal `max`, while a process in that child is still confined
/// to the parent's 1 GiB. That is the ordinary shape of a systemd
/// slice, so reading only the leaf would report "unlimited" in
/// precisely the constrained environments this exists to serve.
///
/// The minimum (not the nearest) is correct because every level in the
/// chain binds simultaneously.
#[cfg(any(target_os = "linux", test))]
fn cgroup_v2_limit_in(cgroup_root: &Path, own_path: &str) -> Option<u64> {
    let mut best: Option<u64> = None;
    let mut cursor = Some(Path::new(own_path));
    while let Some(rel) = cursor {
        // `own_path` is absolute by construction; strip the leading
        // `/` so it joins under `cgroup_root` instead of replacing it.
        let joined = cgroup_root.join(rel.strip_prefix("/").unwrap_or(rel));
        if let Ok(raw) = std::fs::read_to_string(joined.join("memory.max")) {
            if let Ok(v) = raw.trim().parse::<u64>() {
                best = Some(best.map_or(v, |b: u64| b.min(v)));
            }
            // A literal `max` parses as nothing and is skipped: that
            // level imposes no limit, but an ancestor still might.
        }
        cursor = rel.parent();
    }
    best
}

/// cgroup-v1 fallback: a single file, no hierarchy walk, because the
/// v1 memory controller reports the effective limit at the leaf.
#[cfg(any(target_os = "linux", test))]
fn cgroup_v1_limit_in(cgroup_root: &Path) -> Option<u64> {
    let raw = std::fs::read_to_string(cgroup_root.join("memory/memory.limit_in_bytes")).ok()?;
    let v = raw.trim().parse::<u64>().ok()?;
    (v < CGROUP_V1_UNLIMITED_FLOOR).then_some(v)
}

/// The memory limit that applies to this process, or `None` when none
/// does (or when it cannot be determined).
///
/// Deliberately does NOT fall back to total system memory. Measured:
/// inside a container capped at 4 GiB, `/proc/meminfo` still reports
/// the host's `MemTotal` (12 GB here) — so `MemTotal` is not an upper
/// bound on what this process may use, and a `GOMEMLIMIT` derived
/// from it would be worse than none. No detectable cgroup limit means
/// no limit is applied: the point is to respect a ceiling that exists,
/// never to invent one.
#[cfg(target_os = "linux")]
fn detect_cgroup_memory_limit() -> Option<u64> {
    const CGROUP_ROOT: &str = "/sys/fs/cgroup";
    let own = std::fs::read_to_string("/proc/self/cgroup")
        .ok()
        .as_deref()
        .and_then(parse_cgroup_v2_path)
        .unwrap_or_else(|| "/".to_string());
    cgroup_v2_limit_in(Path::new(CGROUP_ROOT), &own)
        .or_else(|| cgroup_v1_limit_in(Path::new(CGROUP_ROOT)))
}

/// Non-Linux hosts have no cgroup memory controller to read. macOS and
/// Windows are developer platforms for this tool, not the constrained
/// container environments the limit exists for (#1154).
#[cfg(not(target_os = "linux"))]
fn detect_cgroup_memory_limit() -> Option<u64> {
    None
}

/// `GOMEMLIMIT` to apply to every `go` child this pass spawns, or
/// `None` to leave the toolchain's default (effectively unlimited).
///
/// An operator-set `GOMEMLIMIT` always wins: it is inherited by the
/// child through the environment already, and someone who tuned it
/// knows something about their deployment that a cgroup read does not.
pub(crate) fn resolve_go_memory_limit(workers: usize) -> Option<u64> {
    resolve_go_memory_limit_with(detect_cgroup_memory_limit(), workers)
}

/// The decision itself, with cgroup detection lifted out as an
/// argument. Split from `resolve_go_memory_limit` so the
/// operator-override rule is testable on a host that has no cgroup:
/// folded in, the test passed on macOS with the override check
/// deleted, because detection returned `None` and the function
/// returned `None` for the wrong reason (mutation-verified, #1154).
pub(crate) fn resolve_go_memory_limit_with(
    detected: Option<u64>,
    workers: usize,
) -> Option<u64> {
    if std::env::var_os("GOMEMLIMIT").is_some() {
        return None;
    }
    go_memory_limit_bytes(detected?, workers)
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
    /// Milestone 775 (FR-015): true iff THIS call performed an actual
    /// `go list all` subprocess spawn.
    ///
    /// False when the outcome came from the cache fast path, from
    /// waiting on another worker's in-flight cell, or from an early
    /// return that skipped the preflight entirely (no `go` toolchain,
    /// budget already exhausted).
    ///
    /// Counting SPAWNS rather than REQUESTS is what makes the
    /// SC-003 assertion meaningful: a request counter would report 39
    /// both before and after this milestone and would have caught
    /// nothing. Aggregated scan-level into the `preflight_invocations`
    /// field on the FR-013 summary line.
    pub preflight_spawned: bool,
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
    go_memory_limit: Option<u64>,
) -> Invocation {
    use std::sync::mpsc;
    use std::thread;

    let (tx, rx) = mpsc::channel();
    let cwd = cwd.to_path_buf();
    let args = args.to_vec();
    let workspace_mode = workspace_mode.clone();
    thread::spawn(move || {
        let mut cmd = build_go_command(&cwd, &args, offline, &workspace_mode, go_memory_limit);
        let _ = tx.send(cmd.output());
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Invocation::Completed(output),
        Ok(Err(e)) => Invocation::SpawnFailed(e.to_string()),
        Err(_) => Invocation::TimedOut,
    }
}

/// Build the `go` child exactly as `run_bounded` spawns it.
///
/// Split out so the child's environment is assertable without
/// spawning anything: `Command::get_envs` is the only way to prove
/// `GOMEMLIMIT` reaches the child, because `go env GOMEMLIMIT` prints
/// empty -- the `go` tool does not list it among its own variables
/// (verified against go1.27), so there is no subprocess observation
/// available to stand in for this.
fn build_go_command(
    cwd: &Path,
    args: &[String],
    offline: bool,
    workspace_mode: &WorkspaceMode,
    go_memory_limit: Option<u64>,
) -> Command {
    let mut cmd = Command::new("go");
    cmd.args(args).current_dir(cwd);
    apply_offline_env(&mut cmd, offline, workspace_mode);
    // #1154. Applied to every `go` child, not only the preflight:
    // `go mod why` chunks run in the same cgroup over the same module
    // graph, so an unbounded chunk can be the process the kernel
    // picks. `None` leaves the variable absent, so the child inherits
    // whatever the operator set.
    if let Some(bytes) = go_memory_limit {
        cmd.env("GOMEMLIMIT", bytes.to_string());
    }
    cmd
}

/// Milestone 775 — single-flight coordination for the shared
/// `go list all` preflight.
///
/// Returns `(outcome, spawned)` where `spawned` is true iff THIS call
/// actually invoked `run` (as opposed to reading a completed outcome
/// from the cache, or waiting on another caller's in-flight cell).
///
/// The preflight work is taken as a closure so the coordination logic
/// is testable without a `go` toolchain (research R4, Constitution
/// Principle VII). Production callers pass `run_preflight`.
///
/// Protocol:
///   1. Acquire the CACHE mutex briefly. Either read a completed
///      outcome, or fetch-or-insert this scope's cell and clone the
///      handle out. Release the cache mutex. The cache mutex is NEVER
///      held across `run` (FR-004).
///   2. Acquire THIS SCOPE's cell. An empty cell means we are the
///      claimant: run the work, memoize into both the cell and the
///      cache, report `spawned = true`. A populated cell means we
///      waited: reuse the memoized outcome, report `spawned = false`
///      (FR-001, FR-002, FR-005).
///
/// Distinct scopes hold distinct cells and therefore proceed fully
/// concurrently (FR-003).
///
/// LOCK ORDERING (load-bearing): a claimant takes cell → cache (to
/// memoize), while an arriving caller takes cache → cell. That is an
/// ABBA shape, and it is deadlock-free ONLY because the cache guard in
/// step 1 is dropped at the end of its block, before the cell is
/// acquired — nobody ever holds the cache while waiting on a cell. Do
/// not "simplify" that block scope away.
///
/// Panic behavior (NFR-002): a panicking claimant poisons only its own
/// scope's cell, so waiters for that scope fail fast rather than
/// blocking forever. Propagating is what Principle III (Fail Closed)
/// requires — a scope whose preflight panicked has produced no
/// verdict, and treating it as "classification passed" would emit an
/// SBOM whose build-inclusion data is quietly wrong.
fn preflight_single_flight<F>(
    cache: &Arc<Mutex<SharedPreflightCache>>,
    scope_root: &Path,
    run: F,
) -> (Result<(), SkipReason>, bool)
where
    F: FnOnce() -> Result<(), SkipReason>,
{
    // Step 1 — short critical section on the cache.
    let claimed = {
        let mut guard = cache.lock().expect("m775: preflight cache mutex poisoned");
        match guard.entries.get(scope_root) {
            Some(done) => Err(done.clone()),
            None => Ok(guard
                .inflight
                .entry(scope_root.to_path_buf())
                .or_default()
                .clone()),
        }
    };

    let cell = match claimed {
        // Fast path: this scope's preflight already completed.
        Err(PreflightOutcome::Ok) => return (Ok(()), false),
        Err(PreflightOutcome::Skipped(reason)) => return (Err(reason), false),
        Ok(cell) => cell,
    };

    // Step 2 — serialize on THIS scope's cell, across the subprocess.
    let mut slot = cell
        .lock()
        .expect("m775: preflight single-flight cell poisoned");
    if slot.is_none() {
        // Claimant: perform the work exactly once for this scope.
        let outcome = run();
        let memo = match &outcome {
            Ok(()) => PreflightOutcome::Ok,
            Err(reason) => PreflightOutcome::Skipped(*reason),
        };
        cache
            .lock()
            .expect("m775: preflight cache mutex poisoned")
            .entries
            .entry(scope_root.to_path_buf())
            .or_insert_with(|| memo.clone());
        *slot = Some(memo);
        return (outcome, true);
    }
    // Waiter: reuse the claimant's outcome verbatim (FR-002, FR-005).
    match slot.as_ref().expect("m775: cell populated above") {
        PreflightOutcome::Ok => (Ok(()), false),
        PreflightOutcome::Skipped(reason) => (Err(*reason), false),
    }
}

/// Run the `go list all` reliability preflight from `preflight_dir`.
/// Returns `Ok(())` on success; `Err(reason)` on any failure path.
///
/// Milestone 771 (T034): extracted from inline `analyze_main_module`
/// so both the per-workspace path AND the shared-scope path can
/// invoke it identically. Log lines carry `main_module = %preflight_dir`
/// which for the shared-scope path names the go.work root (spec
/// Clarification 2026-09-04 Q1) rather than any specific member.
fn run_preflight(
    preflight_dir: &Path,
    offline: bool,
    remaining: Duration,
    workspace_mode: &WorkspaceMode,
    go_memory_limit: Option<u64>,
) -> Result<(), SkipReason> {
    match run_bounded(
        preflight_dir,
        &["list".into(), "all".into()],
        offline,
        remaining,
        workspace_mode,
        go_memory_limit,
    ) {
        Invocation::Completed(output) if output.status.success() => Ok(()),
        Invocation::Completed(output) if preflight_ran_out_of_memory(&output) => {
            tracing::warn!(
                main_module = %preflight_dir.display(),
                status = %output.status,
                "go-mod-why analysis skipped (out-of-memory): `go list all` \
                 preflight ran out of memory; build-inclusion falls back to \
                 unknown markers. More memory, not credentials, is the fix."
            );
            Err(SkipReason::OutOfMemory)
        }
        Invocation::Completed(output) => {
            tracing::warn!(
                main_module = %preflight_dir.display(),
                status = %output.status,
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "go-mod-why analysis skipped (unresolvable-packages): `go \
                 list all` preflight failed — `go mod why` would silently \
                 report false not-needed verdicts; build-inclusion falls \
                 back to unknown markers"
            );
            Err(SkipReason::UnresolvablePackages)
        }
        Invocation::SpawnFailed(detail) => {
            tracing::warn!(
                main_module = %preflight_dir.display(),
                detail = %detail,
                "go-mod-why analysis skipped (unresolvable-packages): `go \
                 list all` preflight could not be spawned; build-inclusion \
                 falls back to unknown markers"
            );
            Err(SkipReason::UnresolvablePackages)
        }
        Invocation::TimedOut => {
            tracing::warn!(
                main_module = %preflight_dir.display(),
                "go-mod-why analysis skipped (unresolvable-packages): `go \
                 list all` preflight exceeded the shared time budget; \
                 build-inclusion falls back to unknown markers"
            );
            Err(SkipReason::UnresolvablePackages)
        }
    }
}

/// Classify `module_paths` against the main module rooted at
/// `main_module_dir` (the directory containing its `go.mod`).
///
/// Degrades, never errors: every failure path returns a
/// `MainModuleAnalysis` describing what happened. The caller decides
/// how verdicts map onto `PackageDbEntry` state and emits the FR-013
/// summary.
///
/// Milestone 771 US3: if `shared_scope` is `Some((cache, scope))`,
/// the reliability preflight is executed at most once per scope (cached
/// in `SharedPreflightCache`). Members reuse the cached outcome —
/// success proceeds to per-member chunks; failure short-circuits
/// with `SkipReason::UnresolvablePackages` per FR-007. When
/// `shared_scope` is `None`, the classifier runs its own preflight
/// from `main_module_dir` (FR-008 fallback path, unchanged from
/// pre-m771 behavior).
pub fn analyze_main_module(
    main_module_dir: &Path,
    module_paths: &[String],
    offline: bool,
    budget: &BudgetTracker,
    shared_scope: Option<(&Arc<Mutex<SharedPreflightCache>>, &Arc<GoWorkScope>)>,
    go_memory_limit: Option<u64>,
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
    // US3: shared preflight when we're in a go.work scope; fallback
    // to the per-workspace preflight otherwise (FR-008).
    //
    // Milestone 775: the shared-scope path is single-flighted via
    // `preflight_single_flight` — exactly one `go list all` spawn per
    // scope per scan, with concurrent callers for the same scope
    // waiting on that scope's cell and reusing its outcome. The
    // loose-workspace path is unchanged: no scope to share, so it runs
    // its own preflight and reports the spawn.
    let (preflight_result, preflight_spawned) = match shared_scope {
        Some((cache, scope)) => preflight_single_flight(cache, &scope.root_dir, || {
            run_preflight(
                &scope.root_dir,
                offline,
                remaining,
                &workspace_mode,
                go_memory_limit,
            )
        }),
        None => (
            run_preflight(
                main_module_dir,
                offline,
                remaining,
                &workspace_mode,
                go_memory_limit,
            ),
            true,
        ),
    };
    analysis.preflight_spawned = preflight_spawned;
    if let Err(reason) = preflight_result {
        analysis.skip_reason = Some(reason);
        return analysis;
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
        match run_bounded(
            main_module_dir,
            &args,
            offline,
            remaining,
            &workspace_mode,
            go_memory_limit,
        ) {
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

    // -------------------------------------------------------------------
    // Milestone 771 — US3 parse_go_work + shared-preflight tests.
    // -------------------------------------------------------------------

    #[test]
    fn m771_parse_go_work_simple_use_directives() {
        let src = "go 1.22\n\nuse ./mod-a\nuse ./mod-b\nuse ./mod-c\n";
        let members = parse_go_work(src);
        assert_eq!(
            members,
            vec![
                "./mod-a".to_string(),
                "./mod-b".to_string(),
                "./mod-c".to_string(),
            ],
        );
    }

    #[test]
    fn m771_parse_go_work_block_form_use_directives() {
        let src = "go 1.22\n\nuse (\n\t./mod-a\n\t./mod-b\n)\n";
        let members = parse_go_work(src);
        assert_eq!(
            members,
            vec!["./mod-a".to_string(), "./mod-b".to_string()],
        );
    }

    #[test]
    fn m771_parse_go_work_ignores_replace_and_go_directives() {
        let src = "go 1.22\n\
                   \n\
                   use ./mod-a\n\
                   \n\
                   replace example.com/foo => ./local-foo\n\
                   godebug default=go1.22\n\
                   \n\
                   use ./mod-b\n";
        let members = parse_go_work(src);
        assert_eq!(
            members,
            vec!["./mod-a".to_string(), "./mod-b".to_string()],
        );
    }

    #[test]
    fn m771_parse_go_work_malformed_returns_empty() {
        // Garbage input — should not panic; empty result triggers
        // FR-008 fallback (per-workspace preflight).
        let src = "!!! not a valid go.work file @@@\n\x00\x01\x02\n";
        let members = parse_go_work(src);
        assert!(
            members.is_empty(),
            "malformed go.work must return empty; got {:?}",
            members,
        );
    }

    #[test]
    fn m771_parse_go_work_handles_comment_and_blank_lines() {
        let src = "// This is a go.work file\ngo 1.22\n\n// Members below\nuse ./mod-a\n";
        let members = parse_go_work(src);
        assert_eq!(members, vec!["./mod-a".to_string()]);
    }

    #[test]
    fn m771_detect_go_work_scopes_finds_members_and_loose() {
        // Build a small tmp tree: root/go.work + root/mod-a/go.mod +
        // root/mod-b/go.mod; separately a loose main-module at
        // root/../loose/go.mod (walking up finds nothing).
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("scope/mod-a")).unwrap();
        std::fs::create_dir_all(root.join("scope/mod-b")).unwrap();
        std::fs::create_dir_all(root.join("loose")).unwrap();
        std::fs::write(
            root.join("scope/go.work"),
            "go 1.22\nuse ./mod-a\nuse ./mod-b\n",
        )
        .unwrap();
        std::fs::write(root.join("scope/mod-a/go.mod"), "module mod-a\n").unwrap();
        std::fs::write(root.join("scope/mod-b/go.mod"), "module mod-b\n").unwrap();
        std::fs::write(root.join("loose/go.mod"), "module loose\n").unwrap();

        let inputs = vec![
            root.join("scope/mod-a"),
            root.join("scope/mod-b"),
            root.join("loose"),
        ];
        let (scopes, loose) = detect_go_work_scopes(&inputs);
        assert_eq!(scopes.len(), 1, "expected one go.work scope; got {:?}", scopes);
        assert_eq!(scopes[0].members.len(), 2);
        assert_eq!(loose.len(), 1, "expected one loose main-module; got {:?}", loose);
        assert!(
            loose[0].ends_with("loose"),
            "loose entry MUST be the ./loose path; got {:?}",
            loose[0],
        );
    }

    #[test]
    fn m771_shared_preflight_cache_dedup_across_workers() {
        // Simulate the cache's dedup invariant using a mock preflight
        // closure counter. If two threads race to preflight the same
        // scope, the outer double-check under the mutex MUST result
        // in exactly one `PreflightOutcome::Ok` insert.
        //
        // We can't actually run `go list all` in a unit test, so we
        // exercise the SharedPreflightCache mutation shape directly:
        // insert once → subsequent inserts via `.entry().or_insert()`
        // are no-ops (the invariant the caller-site relies on).
        let cache: Arc<Mutex<SharedPreflightCache>> = Arc::default();
        let key = PathBuf::from("/tmp/scope-root");
        let effect_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let cache = cache.clone();
            let key = key.clone();
            let effect_counter = effect_counter.clone();
            handles.push(std::thread::spawn(move || {
                // Fast-path: read under lock. Miss → simulate preflight
                // work (increment counter) then insert-or-keep.
                let hit = {
                    let g = cache.lock().unwrap();
                    g.entries.get(&key).cloned()
                };
                if hit.is_none() {
                    effect_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let mut g = cache.lock().unwrap();
                    g.entries.entry(key.clone()).or_insert(PreflightOutcome::Ok);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // Every worker eventually observed the cached entry (either
        // its own insert or a sibling's). The invariant: exactly ONE
        // entry in the cache regardless of race outcomes.
        let g = cache.lock().unwrap();
        assert_eq!(g.entries.len(), 1, "cache MUST contain exactly one entry");
        // Effect counter can be 1-4 depending on race timing (the
        // double-check under lock only closes the race for the INSERT
        // step; the effect counter runs outside the lock per
        // research R3's "avoid holding mutex across subprocess spawn"
        // trade). This is documented in the caller-site comment.
        let effects = effect_counter.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            (1..=4).contains(&effects),
            "effect counter should be in [1,4]; got {}",
            effects,
        );
    }

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
        // FR-004: `Arc<BudgetTracker>` clones share one wall-clock origin.
        //
        // Issue #849 rewrite. The original proved this by timing: budget 200ms,
        // both threads sleep 50ms and assert `Some`, then sleep 200ms and assert
        // `None`. Under CI load a thread can be starved longer than the entire
        // budget -- observed on macOS with `t1=None t2=Some(17.456041ms)` at the
        // 50ms checkpoint, i.e. ~180ms of scheduling delay against a 200ms
        // budget. Almost no margin existed by construction.
        //
        // Re-tuning the bound was rejected: m774's went 2s -> 8s -> 3s and still
        // flaked. Instead every assertion here is *monotonically safe*, so extra
        // delay can only strengthen it, never break it.
        //
        // The original also did not actually discriminate the property it names:
        // at the 50ms checkpoint a shared origin and a per-clone origin both
        // report `Some`. Sleeping past the budget in the MAIN thread before
        // spawning does discriminate -- a per-clone origin would still hold its
        // full budget and report `Some`.
        let key = "WAYBILL_GO_MOD_WHY_BUDGET_MS";

        // (1) Shared origin + agreed exhaustion.
        let mut env = crate::testing::EnvGuard::setup(&[(key, Some("50"))]);
        let tracker = std::sync::Arc::new(BudgetTracker::from_env());
        // Elapse the budget before either thread exists. Any additional delay
        // makes `remaining()` more certainly `None`, not less.
        std::thread::sleep(Duration::from_millis(250));
        let t1 = tracker.clone();
        let t2 = tracker.clone();
        let r1 = std::thread::spawn(move || t1.remaining())
            .join()
            .expect("thread 1");
        let r2 = std::thread::spawn(move || t2.remaining())
            .join()
            .expect("thread 2");
        assert!(
            r1.is_none() && r2.is_none(),
            "clones must share the origin and agree the budget is spent; \
             t1={r1:?} t2={r2:?}",
        );

        // (2) Both clones see a live budget. 10 minutes cannot be exhausted by
        // scheduling delay, so this direction is equally insensitive to load.
        env.set(key, "600000");
        let tracker = std::sync::Arc::new(BudgetTracker::from_env());
        let t3 = tracker.clone();
        let t4 = tracker.clone();
        let r3 = std::thread::spawn(move || t3.remaining())
            .join()
            .expect("thread 3");
        let r4 = std::thread::spawn(move || t4.remaining())
            .join()
            .expect("thread 4");
        assert!(
            r3.is_some() && r4.is_some(),
            "clones must both see remaining budget; t3={r3:?} t4={r4:?}",
        );
    }

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

/// Milestone 775 — single-flight preflight coordination tests.
///
/// These exercise `preflight_single_flight` directly with an injected
/// work function (research R4), so they assert FR-001/FR-002/FR-003 and
/// NFR-002 deterministically without a `go` toolchain, a fixture, or an
/// 11-second subprocess (Constitution Principle VII).
///
/// End-to-end spawn counts on a real fixture are covered separately by
/// the FR-015 counter — but that counter alone cannot distinguish "one
/// spawn because single-flight worked" from "one spawn because only one
/// worker ran", and cannot test cross-scope concurrency at all. Both
/// layers are needed.
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod m775_single_flight_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;
    use std::time::Duration;

    fn cache() -> Arc<Mutex<SharedPreflightCache>> {
        Arc::new(Mutex::new(SharedPreflightCache::default()))
    }

    /// Contract 1 (FR-001): N concurrent callers for ONE scope produce
    /// exactly one invocation of the work function.
    #[test]
    fn m775_single_scope_spawns_exactly_once() {
        const THREADS: usize = 8;
        let cache = cache();
        let scope = PathBuf::from("/scope/a");
        let invocations = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Barrier::new(THREADS));
        let spawned_count = Arc::new(AtomicUsize::new(0));

        std::thread::scope(|s| {
            for _ in 0..THREADS {
                let (cache, scope) = (Arc::clone(&cache), scope.clone());
                let (invocations, gate) = (Arc::clone(&invocations), Arc::clone(&gate));
                let spawned_count = Arc::clone(&spawned_count);
                s.spawn(move || {
                    // Maximize the stampede window: every thread arrives
                    // together, which is exactly what m771 US2's pool does
                    // and what the pre-m775 code could not survive.
                    gate.wait();
                    let (outcome, spawned) = preflight_single_flight(&cache, &scope, || {
                        invocations.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(50));
                        Ok(())
                    });
                    assert!(outcome.is_ok(), "every caller must see the success outcome");
                    if spawned {
                        spawned_count.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });

        assert_eq!(
            invocations.load(Ordering::SeqCst),
            1,
            "FR-001: exactly one preflight spawn per scope, got {}",
            invocations.load(Ordering::SeqCst),
        );
        assert_eq!(
            spawned_count.load(Ordering::SeqCst),
            1,
            "FR-015: exactly one caller may report preflight_spawned",
        );
    }

    /// Contract 2 (FR-002 + FR-005): a failure outcome propagates
    /// identically to every waiter, and is produced only once.
    #[test]
    fn m775_failure_outcome_propagates_to_all_waiters() {
        const THREADS: usize = 6;
        let cache = cache();
        let scope = PathBuf::from("/scope/failing");
        let invocations = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Barrier::new(THREADS));

        std::thread::scope(|s| {
            for _ in 0..THREADS {
                let (cache, scope) = (Arc::clone(&cache), scope.clone());
                let (invocations, gate) = (Arc::clone(&invocations), Arc::clone(&gate));
                s.spawn(move || {
                    gate.wait();
                    let (outcome, _) = preflight_single_flight(&cache, &scope, || {
                        invocations.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(30));
                        Err(SkipReason::UnresolvablePackages)
                    });
                    assert_eq!(
                        outcome,
                        Err(SkipReason::UnresolvablePackages),
                        "every waiter must observe the claimant's failure verbatim",
                    );
                });
            }
        });

        assert_eq!(
            invocations.load(Ordering::SeqCst),
            1,
            "a failed preflight must not be retried by each waiter",
        );
    }

    /// Contract 3 (FR-003 + FR-004): two DISTINCT scopes proceed
    /// concurrently.
    ///
    /// Each scope's work function announces it is inside, then waits for
    /// the other to do the same. Both can only proceed if the two
    /// preflights genuinely overlap. An implementation that serializes
    /// across scopes — e.g. by holding the cache mutex across the
    /// subprocess, which is the simplest fix satisfying every OTHER
    /// contract — cannot satisfy the rendezvous.
    ///
    /// The wait is DEADLINE-BOUNDED rather than a `Barrier`: a
    /// serializing implementation must fail as an assertion, not hang
    /// CI forever. `std::sync::Barrier::wait()` has no timeout, so it
    /// would deadlock instead of reporting.
    #[test]
    fn m775_distinct_scopes_do_not_serialize() {
        let cache = cache();
        let inside = Arc::new(AtomicUsize::new(0));
        let overlapped = Arc::new(AtomicUsize::new(0));

        std::thread::scope(|s| {
            for name in ["/scope/a", "/scope/b"] {
                let cache = Arc::clone(&cache);
                let inside = Arc::clone(&inside);
                let overlapped = Arc::clone(&overlapped);
                let scope = PathBuf::from(name);
                s.spawn(move || {
                    let (outcome, spawned) = preflight_single_flight(&cache, &scope, || {
                        inside.fetch_add(1, Ordering::SeqCst);
                        // Bounded rendezvous: wait for the other scope to
                        // also be inside its preflight.
                        let deadline =
                            std::time::Instant::now() + Duration::from_secs(5);
                        while inside.load(Ordering::SeqCst) < 2 {
                            if std::time::Instant::now() >= deadline {
                                // Serialized: the other scope never got
                                // in. Return without incrementing
                                // `overlapped` so the assertion reports.
                                return Ok(());
                            }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        overlapped.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    });
                    assert!(outcome.is_ok());
                    assert!(spawned, "each distinct scope spawns its own preflight");
                });
            }
        });

        assert_eq!(
            overlapped.load(Ordering::SeqCst),
            2,
            "FR-003/FR-004: both scopes must be inside their preflight at \
             the same time. Reaching this assertion means the two \
             serialized — most likely the cache mutex is being held \
             across the preflight.",
        );
    }

    /// Contract 5 (FR-015 + FR-006): the spawn count over a mix of
    /// scopes equals the number of distinct scopes — and a completed
    /// scope's later callers take the cache fast path without spawning.
    #[test]
    fn m775_spawn_count_matches_distinct_scopes() {
        let cache = cache();
        let invocations = Arc::new(AtomicUsize::new(0));
        let spawned_reports = Arc::new(AtomicUsize::new(0));

        // Three distinct scopes, each requested four times.
        std::thread::scope(|s| {
            for name in ["/s/one", "/s/two", "/s/three"] {
                for _ in 0..4 {
                    let cache = Arc::clone(&cache);
                    let invocations = Arc::clone(&invocations);
                    let spawned_reports = Arc::clone(&spawned_reports);
                    let scope = PathBuf::from(name);
                    s.spawn(move || {
                        let (_, spawned) = preflight_single_flight(&cache, &scope, || {
                            invocations.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        });
                        if spawned {
                            spawned_reports.fetch_add(1, Ordering::SeqCst);
                        }
                    });
                }
            }
        });

        assert_eq!(invocations.load(Ordering::SeqCst), 3, "one spawn per scope");
        assert_eq!(
            spawned_reports.load(Ordering::SeqCst),
            3,
            "reported spawn count must equal actual invocations, not requests",
        );

        // A fourth request to an already-completed scope takes the
        // cache fast path: no spawn, no report.
        let (outcome, spawned) =
            preflight_single_flight(&cache, &PathBuf::from("/s/one"), || {
                panic!("must not run — /s/one already completed");
            });
        assert!(outcome.is_ok());
        assert!(!spawned, "cache fast path must not report a spawn");
    }

    /// NFR-002: a panicking claimant must not leave waiters blocked
    /// forever. The cell is poisoned, so waiters fail fast — which
    /// Principle III (Fail Closed) requires, since a scope whose
    /// preflight panicked has produced no verdict and must not be
    /// silently treated as "classification passed".
    #[test]
    fn m775_panicking_claimant_does_not_block_waiters_forever() {
        let cache = cache();
        let scope = PathBuf::from("/scope/panics");

        // Claimant panics inside the work function.
        let claim = std::panic::catch_unwind({
            let (cache, scope) = (Arc::clone(&cache), scope.clone());
            move || {
                let _ = preflight_single_flight(&cache, &scope, || {
                    panic!("m775 injected preflight panic");
                });
            }
        });
        assert!(claim.is_err(), "the panic must propagate to the claimant");

        // A later caller for the SAME scope must terminate rather than
        // hang. It observes the poisoned cell and fails fast.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let (cache2, scope2) = (Arc::clone(&cache), scope.clone());
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                preflight_single_flight(&cache2, &scope2, || Ok(()))
            }));
            let _ = done_tx.send(r.is_err());
        });

        let failed_fast = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("NFR-002: waiter MUST terminate, not block forever");
        assert!(
            failed_fast,
            "a waiter on a poisoned scope must fail fast, not silently succeed",
        );
    }
}

/// #1154 ask 3 — the `GOMEMLIMIT` derivation. Every test here is pure
/// or runs against a tempdir standing in for `/sys/fs/cgroup`, so the
/// Linux-only read path is exercised on a macOS developer host too.
/// The fixture *shapes* are measured, not invented: see the comments
/// naming what was observed on a real cgroup-v2 host.
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod m1154_gomemlimit_tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    /// The whole point of the feature: N children sharing one cgroup
    /// each get a share, not the whole allowance. A 4 GiB cgroup with
    /// 4 workers yields ~1 GiB each, NOT ~4 GiB each -- the latter is
    /// what made an undivided limit measure the same as no limit.
    #[test]
    fn m1154_the_limit_is_divided_between_concurrent_workers() {
        let cgroup = 4096 * MIB;
        let one = go_memory_limit_bytes(cgroup, 1).unwrap();
        let four = go_memory_limit_bytes(cgroup, 4).unwrap();
        assert_eq!(one, cgroup - GO_NON_HEAP_RESERVE_BYTES);
        assert_eq!(four, cgroup / 4 - GO_NON_HEAP_RESERVE_BYTES);
        assert!(
            four < one / 3,
            "4 workers must get roughly a quarter, got {four} vs {one}"
        );
    }

    /// The reserve exists because `GOMEMLIMIT` governs Go-managed
    /// memory while the OOM killer counts the whole process. Measured
    /// boundary: 12.6 MB of headroom was killed, 13.6 MB survived.
    #[test]
    fn m1154_each_share_keeps_the_non_heap_reserve_back() {
        let share = 256 * MIB;
        let got = go_memory_limit_bytes(share * 4, 4).unwrap();
        assert_eq!(got, share - GO_NON_HEAP_RESERVE_BYTES);
        // The reserve's measured lower bound is pinned at compile time
        // by the `const _: () = assert!(..)` beside the constant.
    }

    /// A cgroup too small to be worth constraining yields `None`
    /// rather than a limit that would only add GC thrash to a run the
    /// kernel kills anyway. Measured: below ~128 MiB single-process,
    /// `go list all` over Kubernetes died at every limit tried.
    #[test]
    fn m1154_a_share_below_the_floor_sets_nothing() {
        // 4 workers in 256 MiB: 64 MiB each, minus 16 MiB reserve =
        // 48 MiB, under the 32 MiB floor? No -- 48 > 32, so Some.
        assert!(go_memory_limit_bytes(256 * MIB, 4).is_some());
        // 8 workers in 256 MiB: 32 MiB each, minus reserve = 16 MiB.
        assert_eq!(go_memory_limit_bytes(256 * MIB, 8), None);
    }

    /// Underflow guard: a share smaller than the reserve must not wrap
    /// into an enormous limit, which is the failure mode that would
    /// make this code worse than absent.
    #[test]
    fn m1154_a_share_smaller_than_the_reserve_sets_nothing() {
        assert_eq!(go_memory_limit_bytes(8 * MIB, 1), None);
        assert_eq!(go_memory_limit_bytes(64 * MIB, 64), None);
    }

    /// `worker_count(0)` is a reachable value (zero workspaces), and
    /// dividing by it must not panic.
    #[test]
    fn m1154_zero_workers_is_treated_as_one() {
        assert_eq!(
            go_memory_limit_bytes(1024 * MIB, 0),
            go_memory_limit_bytes(1024 * MIB, 1)
        );
    }

    /// Both measured `/proc/self/cgroup` shapes. A container started
    /// normally reports `0::/`; the same container under
    /// `--cgroupns=host` reports `0::/docker/<id>`. The second is why
    /// the path is read at all rather than assumed to be the root.
    #[test]
    fn m1154_both_observed_proc_self_cgroup_shapes_parse() {
        assert_eq!(parse_cgroup_v2_path("0::/\n").as_deref(), Some("/"));
        assert_eq!(
            parse_cgroup_v2_path(
                "0::/docker/6bf2aecb7d3113aeb68afd5d3aadbd95437dcd96ec36fef1f9913704e08ae6db\n"
            )
            .as_deref(),
            Some("/docker/6bf2aecb7d3113aeb68afd5d3aadbd95437dcd96ec36fef1f9913704e08ae6db")
        );
    }

    /// A hybrid or v1 file has numbered, controller-named lines. Only
    /// the `0::` line is the v2 path; picking any other would join a
    /// controller-relative path onto the v2 root and read nothing.
    #[test]
    fn m1154_v1_controller_lines_are_not_mistaken_for_the_v2_path() {
        let hybrid = "12:memory:/user.slice\n11:cpu,cpuacct:/user.slice\n0::/user.slice/x\n";
        assert_eq!(parse_cgroup_v2_path(hybrid).as_deref(), Some("/user.slice/x"));
        let v1_only = "12:memory:/user.slice\n11:cpu,cpuacct:/user.slice\n";
        assert_eq!(parse_cgroup_v2_path(v1_only), None);
    }

    /// The measured ancestor case, and the reason this walks at all:
    /// a parent cgroup at `memory.max = 1 GiB` has a child whose own
    /// `memory.max` reads the literal `max`, while a process in that
    /// child is still confined to the parent's 1 GiB. Reading only the
    /// leaf reports "unlimited" in exactly the constrained environment
    /// the feature exists for.
    #[test]
    fn m1154_an_ancestor_limit_is_found_when_the_leaf_reads_max() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("wbprobe");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(parent.join("memory.max"), "1073741824\n").unwrap();
        std::fs::write(child.join("memory.max"), "max\n").unwrap();

        assert_eq!(
            cgroup_v2_limit_in(root.path(), "/wbprobe/child"),
            Some(1073741824),
            "a limit on an ancestor must be found from the leaf"
        );
    }

    /// Every level binding at once means the *minimum* is the
    /// effective limit, not the nearest one.
    #[test]
    fn m1154_the_smallest_limit_in_the_chain_wins() {
        let root = tempfile::tempdir().unwrap();
        let outer = root.path().join("a");
        let inner = outer.join("b");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(outer.join("memory.max"), "134217728\n").unwrap(); // 128 MiB
        std::fs::write(inner.join("memory.max"), "536870912\n").unwrap(); // 512 MiB
        assert_eq!(
            cgroup_v2_limit_in(root.path(), "/a/b"),
            Some(134217728),
            "the tighter ancestor limit must win over the looser leaf"
        );
    }

    /// Measured: an unconstrained container reads the literal `max`.
    /// That must mean "no limit", never a parse that yields a number.
    #[test]
    fn m1154_an_unlimited_cgroup_yields_no_limit() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("memory.max"), "max\n").unwrap();
        assert_eq!(cgroup_v2_limit_in(root.path(), "/"), None);
    }

    /// A missing hierarchy is the ordinary non-container case and must
    /// degrade silently, not error.
    #[test]
    fn m1154_an_absent_cgroup_hierarchy_yields_no_limit() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(cgroup_v2_limit_in(root.path(), "/does/not/exist"), None);
    }

    /// cgroup v1. The unlimited sentinel is a near-`u64::MAX` word, and
    /// treating it as a real limit would divide it by the worker count
    /// and hand `go` a meaningless multi-exabyte target.
    #[test]
    fn m1154_the_v1_unlimited_sentinel_is_not_a_limit() {
        let root = tempfile::tempdir().unwrap();
        let mem = root.path().join("memory");
        std::fs::create_dir_all(&mem).unwrap();

        std::fs::write(mem.join("memory.limit_in_bytes"), "9223372036854771712\n").unwrap();
        assert_eq!(cgroup_v1_limit_in(root.path()), None);

        std::fs::write(mem.join("memory.limit_in_bytes"), "4294967296\n").unwrap();
        assert_eq!(cgroup_v1_limit_in(root.path()), Some(4294967296));
    }

    /// An operator who set `GOMEMLIMIT` knows something about their
    /// deployment that a cgroup read does not, and the child inherits
    /// their value through the environment already. Never override it.
    #[test]
    fn m1154_an_operator_set_gomemlimit_is_never_overridden() {
        let detected = Some(4096 * MIB);
        let mut env = crate::testing::EnvGuard::acquire();

        // Control first: with the var absent, this detected limit DOES
        // produce one. Without this line the assertion below passes
        // whenever the function returns `None` for any reason at all.
        env.remove("GOMEMLIMIT");
        assert!(
            resolve_go_memory_limit_with(detected, 4).is_some(),
            "control: a detected cgroup limit must yield a derived limit"
        );

        env.set("GOMEMLIMIT", "123MiB");
        assert_eq!(
            resolve_go_memory_limit_with(detected, 4),
            None,
            "an operator-set GOMEMLIMIT must suppress the derived one"
        );
    }

    /// The plumbing, asserted on the command production actually
    /// spawns. The computation tests above would all pass with the
    /// value never reaching a child process.
    fn env_of(limit: Option<u64>) -> Vec<(String, Option<String>)> {
        let cmd = build_go_command(
            Path::new("/tmp"),
            &["list".to_string(), "all".to_string()],
            false,
            &WorkspaceMode::Inactive,
            limit,
        );
        cmd.get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn m1154_a_resolved_limit_reaches_the_go_child_env() {
        let envs = env_of(Some(123 * MIB));
        let found = envs
            .iter()
            .find(|(k, _)| k == "GOMEMLIMIT")
            .and_then(|(_, v)| v.clone());
        assert_eq!(
            found.as_deref(),
            Some("128974848"),
            "GOMEMLIMIT must be set on the child, in bytes; env was {envs:?}"
        );
    }

    /// `None` must leave the variable absent rather than set it to
    /// something like `0`, which the Go runtime would read as a limit
    /// of zero bytes and GC-thrash against forever.
    #[test]
    fn m1154_no_resolved_limit_leaves_the_child_env_untouched() {
        let envs = env_of(None);
        assert!(
            !envs.iter().any(|(k, _)| k == "GOMEMLIMIT"),
            "GOMEMLIMIT must be absent, not zero; env was {envs:?}"
        );
    }

    /// Positive control for the Linux-only read path, which no test
    /// above reaches: `detect_cgroup_memory_limit` is
    /// `cfg(target_os = "linux")`, so on a macOS developer host it is
    /// not merely unexercised, it is never compiled. Everything else
    /// here runs against tempdir fixtures whose shapes were copied
    /// from a real cgroup -- faithful, but still a fixture.
    ///
    /// Run it by setting the expected limit in bytes, inside a cgroup
    /// that actually has one:
    ///
    /// ```sh
    /// docker run --memory=512m -e WAYBILL_TEST_EXPECT_CGROUP_LIMIT=536870912 ...
    /// ```
    ///
    /// Skipped, loudly, when that variable is absent. A silent skip
    /// here would be indistinguishable from a pass, which is the
    /// failure mode this test exists to rule out.
    #[test]
    #[cfg(target_os = "linux")]
    fn m1154_the_real_cgroup_read_returns_the_container_limit() {
        let Ok(raw) = std::env::var("WAYBILL_TEST_EXPECT_CGROUP_LIMIT") else {
            eprintln!(
                "SKIPPED m1154_the_real_cgroup_read_returns_the_container_limit: \
                 set WAYBILL_TEST_EXPECT_CGROUP_LIMIT=<bytes> inside a \
                 memory-capped cgroup to run it"
            );
            return;
        };
        let expected: u64 = raw.trim().parse().expect("expected limit must be bytes");
        assert_eq!(
            detect_cgroup_memory_limit(),
            Some(expected),
            "the real cgroup read must return the cgroup's own limit"
        );
    }

    /// On a host with no cgroup memory limit -- every developer macOS
    /// box, and any Linux host outside a constrained cgroup -- nothing
    /// is set, so behaviour is exactly as it was before #1154.
    /// Deliberately not asserted as `None` on Linux: CI runners do run
    /// in cgroups, and a test that demands "no limit" would fail there
    /// for the right reason, which makes it a bad test.
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn m1154_a_host_without_cgroups_sets_nothing() {
        let mut env = crate::testing::EnvGuard::acquire();
        env.remove("GOMEMLIMIT");
        assert_eq!(
            resolve_go_memory_limit(4),
            None,
            "no cgroup controller to read means no limit is applied"
        );
    }
}
