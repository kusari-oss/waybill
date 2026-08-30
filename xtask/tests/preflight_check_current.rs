//! Milestone 669 T043 — `xtask bench --preflight-check` fresh-baseline
//! contract test (xtask-bench-cli.md T3).
//!
//! Plants a `baseline.json` whose `metadata.waybill_commit_sha` equals
//! HEAD, asserts:
//!   1. `preflight::check` returns `is_stale == false`.
//!   2. `outcome.diff_output` is empty (or whitespace-only).

use std::path::Path;
use std::process::Command;

use xtask::bench::preflight;

fn git(args: &[&str], cwd: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    if !out.status.success() {
        panic!(
            "git {args:?}: exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn git_stdout(args: &[&str], cwd: &Path) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn write(p: &Path, body: &str) {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, body).unwrap();
}

#[test]
fn preflight_check_current_baseline_is_not_stale() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    git(&["init", "--initial-branch=main"], root);
    git(&["config", "user.email", "test@waybill.dev"], root);
    git(&["config", "user.name", "Test"], root);
    git(&["config", "commit.gpgsign", "false"], root);
    write(&root.join("waybill-cli/src/main.rs"), "fn main() {}\n");
    write(&root.join("Cargo.lock"), "# lockfile\n");
    git(&["add", "-A"], root);
    git(&["commit", "-m", "baseline commit"], root);
    let head_sha = git_stdout(&["rev-parse", "HEAD"], root);

    // Plant baseline pointing at THE SAME sha as HEAD.
    let baseline_path = root.join("baseline.json");
    let baseline_json = format!(
        r#"{{
            "schema_version": 1,
            "metadata": {{
                "waybill_commit_sha": "{head_sha}",
                "fixture_sha": "1111111111111111111111111111111111111111",
                "runner_uname": "test",
                "noise_class": "reference",
                "started_at": "2026-08-29T00:00:00Z",
                "finished_at": "2026-08-29T00:00:00Z",
                "total_duration_sec": 0
            }},
            "results": []
        }}"#
    );
    std::fs::write(&baseline_path, &baseline_json).unwrap();

    let outcome = preflight::check(&baseline_path, root).unwrap();
    assert!(
        !outcome.is_stale,
        "expected is_stale=false; got diff_output={:?}",
        outcome.diff_output
    );
    assert!(
        outcome.diff_output.trim().is_empty(),
        "expected empty diff; got: {:?}",
        outcome.diff_output
    );
    assert_eq!(outcome.baseline_sha, head_sha);
    assert_eq!(outcome.head_sha, head_sha);
}
