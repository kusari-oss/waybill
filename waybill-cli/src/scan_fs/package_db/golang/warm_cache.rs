//! Milestone 173: opt-in Go cache warming.
//!
//! Runs `go mod download` in every discovered Go workspace BEFORE the
//! transitive-resolution ladder (m055 / m091) runs so step 1
//! (`go mod graph`) can find every module locally and produce true
//! parent-child topology — instead of falling through to step 5's
//! `go.sum` flat fallback.
//!
//! ## Subprocess pattern
//!
//! Mirrors `waybill-cli/src/scan_fs/package_db/golang/go_mod_graph.rs`'s
//! `run_go_mod_graph` verbatim (research §R2): `std::process::Command`
//! spawned in a worker `std::thread` with `mpsc::channel()` for the
//! result and `rx.recv_timeout(duration)` to enforce the timeout.
//!
//! ## Concurrency pattern
//!
//! Mirrors `graph_resolver.rs`'s `parallel_fetch` (research §R3):
//! `std::thread` worker pool + `mpsc::sync_channel(workers)` bounded
//! job queue + `mpsc::channel()` result collector.
//!
//! No tokio. Everything is synchronous, matching the existing Go
//! subprocess call sites.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;

use serde::Serialize;

/// Effective cache-warming mode after CLI + `--offline` reconciliation.
///
/// Set once at CLI parse time from the `--warm-go-cache` flag (`Off`
/// or `PerWorkspace`). If `--offline` is also set + user picked
/// `PerWorkspace`, the mode is upgraded to `OfflineInhibited` before
/// the warmer would have run.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheWarmingMode {
    /// Default. No warming performed. C117 (`waybill:go-transitive-
    /// fallback-count`) reflects whatever state the operator's env is
    /// in.
    Off,
    /// One `go mod download` invocation per discovered `go.mod`
    /// workspace before the transitive resolver runs.
    PerWorkspace,
    /// Internal-only variant. Set when the operator requested
    /// `--warm-go-cache=per-workspace` but `--offline` is also set.
    /// The warmer skips all work; the C118 annotation surfaces the
    /// override for operator awareness (FR-003 + FR-011).
    OfflineInhibited,
}

impl CacheWarmingMode {
    /// Wire-string value for the FR-011 doc-scope annotation.
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::PerWorkspace => "per-workspace",
            Self::OfflineInhibited => "offline-inhibited",
        }
    }
}

/// Closed enum of per-workspace warming failure classes (FR-007).
///
/// The six variants are exhaustive; adding a new class is a
/// breaking change to the wire contract and requires a follow-up
/// milestone.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarmingFailureReason {
    /// `go` binary not found on PATH.
    GoBinaryAbsent,
    /// `Command::new("go")` spawn failed (permission denied,
    /// executable-format error, etc.).
    SpawnFailed,
    /// The subprocess exceeded the per-workspace timeout budget.
    Timeout,
    /// The subprocess exited with a non-zero status. Distinguished
    /// from `ParseError` because the go command signaled the failure
    /// itself.
    SubcommandFailed,
    /// The subprocess exited zero but its output couldn't be
    /// interpreted (extremely rare for `go mod download`; kept in
    /// the enum for future subcommands that may emit structured
    /// output). Currently never constructed — reserved slot in the
    /// closed-6 wire enum per data-model.md Entity 2.
    #[allow(dead_code)]
    ParseError,
    /// The overall wall-clock budget was exhausted before this
    /// workspace could be attempted.
    BudgetExhausted,
}

impl WarmingFailureReason {
    /// Bare wire-string value. Used by the T011 per-failure
    /// `tracing::warn!` line (FR-005) so operators grep-friendly
    /// see the same kebab-case values that appear in the C119
    /// annotation payload. Verified to match serde's emission of
    /// each variant by a unit test in this module.
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            Self::GoBinaryAbsent => "go-binary-absent",
            Self::SpawnFailed => "spawn-failed",
            Self::Timeout => "timeout",
            Self::SubcommandFailed => "subcommand-failed",
            Self::ParseError => "parse-error",
            Self::BudgetExhausted => "budget-exhausted",
        }
    }
}

/// Per-workspace failure record. Field order is intentional:
/// `reason` is declared FIRST so serde's default emission
/// (struct-declaration order) produces alphabetical JSON:
/// `{"reason":..., "workspace":...}`. This matches the
/// contracts/annotation-wire-shapes.md byte-identity requirement.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceFailure {
    /// Reason class from the closed enum.
    pub reason: WarmingFailureReason,
    /// Workspace path RELATIVE to the scan root (for portability
    /// across environments and byte-identity of goldens).
    pub workspace: String,
}

/// Aggregated cache-warming outcome for one scan. Flows through
/// `GoScanSignals` → `ScanDiagnostics` → `ScanResult` → `ScanArtifacts`
/// → format emitters.
#[derive(Debug, Clone, Serialize)]
pub struct CacheWarmingResult {
    /// The effective mode the warmer operated under. Used by the
    /// C118 `waybill:go-cache-warming-mode` annotation.
    pub mode: CacheWarmingMode,
    /// Per-workspace failures, sorted alphabetically by `workspace`
    /// for byte-identity across regenerations. Successful workspaces
    /// are OMITTED; the vec contains only failures (FR-007 aggregation).
    /// Empty vec ⇒ no C119 annotation emitted.
    pub failures: Vec<WorkspaceFailure>,
}

/// Convert an operator-supplied `--warm-go-cache-concurrency <N>`
/// value into an effective worker-pool size (FR-014).
///
/// * `raw == 0` ⇒ auto: `min(available_parallelism, 8)`.
/// * `1..=32` ⇒ passed through unchanged.
/// * `raw > 32` ⇒ clamped to `32` with a `tracing::warn!` (defense
///   against typos / config-file mistakes flooding GOPROXY).
pub fn effective_concurrency(raw: u32) -> usize {
    if raw == 0 {
        let cpus = std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(4);
        cpus.min(8)
    } else if raw > 32 {
        tracing::warn!(
            requested = raw,
            "--warm-go-cache-concurrency clamped to 32 (per FR-014)"
        );
        32
    } else {
        raw as usize
    }
}

/// Warm the operator's `$GOMODCACHE` by running `go mod download` in
/// every discovered Go workspace before the m055/m091 transitive
/// resolver runs. Bounded concurrency; per-workspace + overall
/// wall-clock timeout; graceful degradation on failure.
///
/// Called from the Go reader (`legacy.rs`) after workspace discovery
/// but before the resolver ladder runs against each workspace. On a
/// cold cache, warming primes step 1 (`go mod graph`) so it produces
/// true parent-child topology instead of falling through to step 5's
/// go.sum flat fallback.
///
/// * `mode` — `Off` / `PerWorkspace` / `OfflineInhibited`. Callers
///   with `Off` should NOT invoke this function (it's a no-op guard);
///   `OfflineInhibited` is also a no-op (returns an empty-failure
///   result with the mode preserved so the C118 annotation surfaces
///   the operator's request).
/// * `workspace_paths` — every discovered `go.mod` workspace directory
///   (absolute paths).
/// * `scan_root` — used to compute each workspace's RELATIVE path
///   for the FR-007 wire contract.
/// * `concurrency` — post-`effective_concurrency` worker pool size.
/// * `per_workspace_timeout` — kill an individual `go mod download`
///   that runs longer than this. Emits `Timeout` reason.
/// * `overall_budget` — total wall-clock budget for the warming phase.
///   Remaining workspaces at exhaustion get `BudgetExhausted`.
pub fn warm_workspaces(
    mode: CacheWarmingMode,
    workspace_paths: &[PathBuf],
    scan_root: &Path,
    concurrency: usize,
    per_workspace_timeout: Duration,
    overall_budget: Duration,
) -> CacheWarmingResult {
    // Belt-and-braces guards for the two no-op modes.
    if matches!(
        mode,
        CacheWarmingMode::Off | CacheWarmingMode::OfflineInhibited
    ) {
        return CacheWarmingResult {
            mode,
            failures: Vec::new(),
        };
    }

    if workspace_paths.is_empty() {
        return CacheWarmingResult {
            mode,
            failures: Vec::new(),
        };
    }

    // FR-005 / research §R2: probe `go version` once before spawning
    // workers. If the toolchain is absent, EVERY workspace gets
    // `GoBinaryAbsent` (US3 Acceptance Scenario 2 — behavior
    // converges to no-warming for all workspaces).
    match Command::new("go").arg("version").output() {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let failures = collect_all_workspaces_with_reason(
                workspace_paths,
                scan_root,
                WarmingFailureReason::GoBinaryAbsent,
            );
            emit_failure_warns(&failures);
            return CacheWarmingResult { mode, failures };
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "`go version` probe failed during cache warming; falling back to no-warming"
            );
            let failures = collect_all_workspaces_with_reason(
                workspace_paths,
                scan_root,
                WarmingFailureReason::SpawnFailed,
            );
            emit_failure_warns(&failures);
            return CacheWarmingResult { mode, failures };
        }
    }

    // Worker pool per research §R3 (mirrors `parallel_fetch` at
    // graph_resolver.rs:1001).
    let n = workspace_paths.len();
    let workers = concurrency.max(1).min(n);
    let (job_tx, job_rx) = mpsc::sync_channel::<PathBuf>(workers);
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (result_tx, result_rx) = mpsc::channel::<(PathBuf, Result<(), WarmingFailureReason>)>();

    let mut worker_handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let job_rx = Arc::clone(&job_rx);
        let result_tx = result_tx.clone();
        let handle = thread::spawn(move || loop {
            let path = {
                let rx = match job_rx.lock() {
                    Ok(g) => g,
                    Err(_) => break, // poisoned mutex
                };
                match rx.recv() {
                    Ok(p) => p,
                    Err(_) => break, // channel closed → done
                }
            };
            let r = run_go_mod_download(&path, per_workspace_timeout);
            if result_tx.send((path, r)).is_err() {
                break; // collector dropped
            }
        });
        worker_handles.push(handle);
    }
    drop(result_tx); // drop original clone so drain terminates

    // Feeder: iterate workspaces, budget-check between sends, drop
    // the sender when the budget is exhausted so workers exit.
    let started = Instant::now();
    let mut budget_exhausted_paths: Vec<PathBuf> = Vec::new();
    for (fed_count, path) in workspace_paths.iter().enumerate() {
        if started.elapsed() >= overall_budget {
            budget_exhausted_paths.extend(workspace_paths[fed_count..].iter().cloned());
            break;
        }
        if job_tx.send(path.clone()).is_err() {
            break; // workers all exited unexpectedly
        }
    }
    drop(job_tx); // signal end of jobs so workers exit their recv loops

    // Collector: drain until all workers exit.
    let mut failures: Vec<WorkspaceFailure> = Vec::new();
    while let Ok((path, r)) = result_rx.recv() {
        if let Err(reason) = r {
            failures.push(WorkspaceFailure {
                reason,
                workspace: relative_workspace_path(&path, scan_root),
            });
        }
    }

    // Anything the feeder couldn't send before budget exhaustion.
    for path in budget_exhausted_paths {
        failures.push(WorkspaceFailure {
            reason: WarmingFailureReason::BudgetExhausted,
            workspace: relative_workspace_path(&path, scan_root),
        });
    }

    for h in worker_handles {
        let _ = h.join();
    }

    // FR-005 per-workspace warn emission (in addition to the C119
    // aggregate annotation). Emit one line per failure so operators
    // grepping tool output see the failing workspaces in real time.
    emit_failure_warns(&failures);

    // Sort alphabetically by workspace path for byte-identity across
    // regenerations (data-model.md Entity 3 contract).
    failures.sort_by(|a, b| a.workspace.cmp(&b.workspace));

    CacheWarmingResult { mode, failures }
}

/// FR-005 per-failure warn log emission. One `tracing::warn!` line
/// per failure with the workspace path + kebab-case reason class.
/// Operators grepping stderr in real time see failing workspaces as
/// they happen; the C119 doc-scope annotation aggregates the same
/// data for post-scan audit.
fn emit_failure_warns(failures: &[WorkspaceFailure]) {
    for f in failures {
        tracing::warn!(
            workspace = %f.workspace,
            reason = f.reason.as_wire_str(),
            "go mod download failed for workspace"
        );
    }
}

/// Fan-out helper for the "go binary absent" / "probe spawn failed"
/// case: every discovered workspace inherits the same failure reason.
fn collect_all_workspaces_with_reason(
    workspace_paths: &[PathBuf],
    scan_root: &Path,
    reason: WarmingFailureReason,
) -> Vec<WorkspaceFailure> {
    let mut out: Vec<WorkspaceFailure> = workspace_paths
        .iter()
        .map(|p| WorkspaceFailure {
            reason,
            workspace: relative_workspace_path(p, scan_root),
        })
        .collect();
    out.sort_by(|a, b| a.workspace.cmp(&b.workspace));
    out
}

/// Compute the relative-to-scan-root form of a workspace path. Falls
/// back to the absolute path if `strip_prefix` fails (defensive; the
/// caller should always pass paths under the scan root).
fn relative_workspace_path(workspace: &Path, scan_root: &Path) -> String {
    workspace
        .strip_prefix(scan_root)
        .unwrap_or(workspace)
        .to_string_lossy()
        .into_owned()
}

/// Invoke `go mod download` in `workspace` with a wall-clock timeout.
/// Mirrors `run_go_mod_graph` from `go_mod_graph.rs:81-158` — worker
/// thread + `mpsc::recv_timeout` for the deadline, so `Command::output()`
/// (which has no built-in timeout) can still be bounded.
fn run_go_mod_download(
    workspace: &Path,
    timeout: Duration,
) -> Result<(), WarmingFailureReason> {
    let (tx, rx) = mpsc::channel();
    let workspace = workspace.to_path_buf();
    thread::spawn(move || {
        let result = Command::new("go")
            .args(["mod", "download"])
            .current_dir(&workspace)
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(o)) => o,
        Ok(Err(_)) => return Err(WarmingFailureReason::SpawnFailed),
        Err(_) => return Err(WarmingFailureReason::Timeout),
    };

    if output.status.success() {
        Ok(())
    } else {
        Err(WarmingFailureReason::SubcommandFailed)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn cache_warming_mode_wire_strings() {
        assert_eq!(CacheWarmingMode::Off.as_wire_str(), "off");
        assert_eq!(CacheWarmingMode::PerWorkspace.as_wire_str(), "per-workspace");
        assert_eq!(
            CacheWarmingMode::OfflineInhibited.as_wire_str(),
            "offline-inhibited"
        );
    }

    #[test]
    fn warming_failure_reason_wire_str_matches_serde() {
        // FR-005 warn log uses `as_wire_str()` for the reason field;
        // FR-007 C119 annotation uses serde. Both MUST produce the
        // same kebab-case strings so operators grepping either source
        // find the same values.
        let variants = [
            (
                WarmingFailureReason::GoBinaryAbsent,
                "\"go-binary-absent\"",
            ),
            (WarmingFailureReason::SpawnFailed, "\"spawn-failed\""),
            (WarmingFailureReason::Timeout, "\"timeout\""),
            (
                WarmingFailureReason::SubcommandFailed,
                "\"subcommand-failed\"",
            ),
            (WarmingFailureReason::ParseError, "\"parse-error\""),
            (
                WarmingFailureReason::BudgetExhausted,
                "\"budget-exhausted\"",
            ),
        ];
        for (variant, serde_expected) in variants {
            let serde_actual = serde_json::to_string(&variant).unwrap();
            assert_eq!(
                serde_actual, serde_expected,
                "serde emission for {variant:?}"
            );
            let bare = variant.as_wire_str();
            let quoted = format!("\"{bare}\"");
            assert_eq!(
                quoted, serde_expected,
                "as_wire_str/serde parity for {variant:?}"
            );
        }
    }

    #[test]
    fn workspace_failure_serializes_alphabetically() {
        // Wire contract: JSON output must be `{"reason":..., "workspace":...}`.
        // Serde emits in struct-declaration order; we declared
        // `reason` first specifically for this reason.
        let f = WorkspaceFailure {
            reason: WarmingFailureReason::Timeout,
            workspace: "cmd/foo".to_string(),
        };
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#"{"reason":"timeout","workspace":"cmd/foo"}"#);
    }

    #[test]
    fn effective_concurrency_bounds() {
        // Auto: raw == 0 → min(available_parallelism, 8).
        let auto = effective_concurrency(0);
        assert!(
            (1..=8).contains(&auto),
            "auto concurrency must be in 1..=8, got {auto}"
        );

        // Passthrough: 1..=32 unchanged.
        assert_eq!(effective_concurrency(1), 1);
        assert_eq!(effective_concurrency(4), 4);
        assert_eq!(effective_concurrency(32), 32);

        // Clamp: >32 → 32.
        assert_eq!(effective_concurrency(33), 32);
        assert_eq!(effective_concurrency(100), 32);
        assert_eq!(effective_concurrency(u32::MAX), 32);
    }
}
