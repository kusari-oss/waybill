// milestone 669 - see specs/669-bench-harness/plan.md
// T041 — `xtask bench --preflight-check` R7 staleness algorithm.
//
// Contract xtask-bench-cli.md C-5:
//   1. Refuse to run if `docs/perf/baseline.json` is missing.
//   2. Extract `metadata.waybill_commit_sha` from the baseline JSON.
//   3. Run `git diff --stat <baseline-sha>..HEAD -- 'waybill-cli/**'
//      'waybill-common/**' 'waybill-ebpf/**' Cargo.lock` — SC-006 scope.
//   4. If non-empty: exit 1 + print the C-5 diagnostic with the
//      recovery command.
//   5. If empty: exit 0 silently.
//
// Split as a separate module so `preflight_check_stale.rs` +
// `preflight_check_current.rs` integration tests can drive it against
// tempdir-scoped git repos without invoking the full `run()` path.

use std::error::Error;
use std::path::Path;
use std::process::Command;

use crate::bench::schema::BenchRun;

/// Outcome of the staleness check. `is_stale = true` ⟹ CLI exits 1.
#[derive(Debug, Clone)]
pub struct PreflightOutcome {
    /// waybill_commit_sha the baseline was captured at.
    pub baseline_sha: String,
    /// Result of `git rev-parse HEAD` at check time.
    pub head_sha: String,
    /// Raw `git diff --stat` output over the SC-006 scope. Empty when
    /// baseline is fresh.
    pub diff_output: String,
    /// True iff `diff_output` is non-empty (any waybill-runtime file
    /// changed since the baseline SHA).
    pub is_stale: bool,
}

/// Load a baseline JSON, extract its waybill_commit_sha, run the
/// `git diff --stat` scope check under `workspace_root`, return the
/// outcome. Any I/O or subprocess failure returns Err.
pub fn check(baseline_path: &Path, workspace_root: &Path) -> Result<PreflightOutcome, Box<dyn Error>> {
    let bytes = std::fs::read(baseline_path).map_err(|e| -> Box<dyn Error> {
        format!("failed to read baseline at {}: {e}", baseline_path.display()).into()
    })?;
    let baseline: BenchRun = serde_json::from_slice(&bytes)?;
    // V1 fail-close: an unreadable baseline is worse than a fresh one.
    baseline.validate()?;
    let baseline_sha = baseline.metadata.waybill_commit_sha;

    let head_sha = run_git(&["rev-parse", "HEAD"], workspace_root)?;

    // git-diff pathspec per contract C-5. Trailing single-quoted
    // globs use git's `**` pathspec magic which matches recursively.
    let range = format!("{baseline_sha}..HEAD");
    let diff_output = run_git(
        &[
            "diff",
            "--stat",
            &range,
            "--",
            "waybill-cli/**",
            "waybill-common/**",
            "waybill-ebpf/**",
            "Cargo.lock",
        ],
        workspace_root,
    )?;

    let is_stale = !diff_output.trim().is_empty();
    Ok(PreflightOutcome {
        baseline_sha,
        head_sha,
        diff_output,
        is_stale,
    })
}

/// Format the C-5 stale-baseline diagnostic. Includes the recovery
/// command block operators are expected to run.
pub fn format_diagnostic(outcome: &PreflightOutcome) -> String {
    let first_10 = outcome
        .diff_output
        .lines()
        .take(10)
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Perf baseline is stale.\n\
         Baseline was captured at {baseline}; HEAD is {head}.\n\
         The following waybill-runtime files changed since:\n\
         {diff}\n\
         Refresh the baseline before releasing:\n\
         \n\
           $ cargo run -p xtask -- bench --update-baseline\n\
           $ git add docs/perf/baseline.json\n\
           $ git commit -m \"release: refresh perf baseline\"\n",
        baseline = outcome.baseline_sha,
        head = outcome.head_sha,
        diff = first_10,
    )
}

/// Invoke `git <args>` in `cwd`, return trimmed stdout. Non-zero exit
/// becomes Err with stderr in the message.
fn run_git(args: &[&str], cwd: &Path) -> Result<String, Box<dyn Error>> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| -> Box<dyn Error> {
            format!("git {args:?}: spawn failed: {e}").into()
        })?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?}: exited non-zero: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    let stdout = String::from_utf8(out.stdout)?.trim().to_string();
    Ok(stdout)
}

// ────────────────────────────────────────────────────────────────
// Unit tests — algorithm-only (git-independent).
// Git-dependent scenarios live in the T042/T043 integration tests.
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(baseline: &str, head: &str, diff: &str) -> PreflightOutcome {
        PreflightOutcome {
            baseline_sha: baseline.into(),
            head_sha: head.into(),
            diff_output: diff.into(),
            is_stale: !diff.trim().is_empty(),
        }
    }

    #[test]
    fn format_diagnostic_includes_stale_marker() {
        let o = outcome(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            " waybill-cli/src/scan_fs/mod.rs | 3 +--\n 1 file changed",
        );
        let msg = format_diagnostic(&o);
        assert!(msg.contains("Perf baseline is stale."));
    }

    #[test]
    fn format_diagnostic_includes_recovery_command() {
        let o = outcome(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "some-file | 1 +",
        );
        let msg = format_diagnostic(&o);
        assert!(msg.contains("cargo run -p xtask -- bench --update-baseline"));
        assert!(msg.contains("git add docs/perf/baseline.json"));
        assert!(msg.contains("release: refresh perf baseline"));
    }

    #[test]
    fn format_diagnostic_includes_both_shas() {
        let o = outcome(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "some-file",
        );
        let msg = format_diagnostic(&o);
        assert!(msg.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(msg.contains("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    }

    #[test]
    fn format_diagnostic_truncates_diff_output_to_10_lines() {
        // Contract C-5.4: `<first-10-lines-of-git-diff>` in the message.
        let mut diff = String::new();
        for i in 0..20 {
            diff.push_str(&format!("file-{i} | 1 +\n"));
        }
        let o = outcome(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            &diff,
        );
        let msg = format_diagnostic(&o);
        // Lines 0-9 present; lines 10-19 absent from the diff section.
        assert!(msg.contains("file-0 |"));
        assert!(msg.contains("file-9 |"));
        assert!(!msg.contains("file-10 |"));
        assert!(!msg.contains("file-19 |"));
    }

    #[test]
    fn check_errors_on_missing_baseline_file() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist.json");
        assert!(check(&missing, tmp.path()).is_err());
    }

    #[test]
    fn check_errors_on_malformed_baseline_json() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = tmp.path().join("bad.json");
        std::fs::write(&bad, "{not-valid-json").unwrap();
        assert!(check(&bad, tmp.path()).is_err());
    }

    #[test]
    fn check_errors_on_future_schema_version() {
        // V1 fail-close: `check()` refuses future-schema baselines
        // the same way `bench-docs::run()` does.
        let tmp = tempfile::tempdir().unwrap();
        let future = tmp.path().join("future.json");
        let json = r#"{
            "schema_version": 2,
            "metadata": {
                "waybill_commit_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "fixture_sha": "1111111111111111111111111111111111111111",
                "runner_uname": "test",
                "noise_class": "reference",
                "started_at": "2026-08-29T00:00:00Z",
                "finished_at": "2026-08-29T00:00:00Z",
                "total_duration_sec": 0
            },
            "results": []
        }"#;
        std::fs::write(&future, json).unwrap();
        let err = check(&future, tmp.path()).unwrap_err();
        assert!(err.to_string().contains("V1 violation"));
    }
}
