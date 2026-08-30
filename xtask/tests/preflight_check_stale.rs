//! Milestone 669 T042 — `xtask bench --preflight-check` stale-baseline
//! contract test (xtask-bench-cli.md T2).
//!
//! Plants a `baseline.json` with `metadata.waybill_commit_sha` pointing
//! at a git commit that has waybill-runtime changes on top of HEAD.
//! Asserts:
//!   1. `preflight::check` returns `is_stale == true`.
//!   2. The formatted diagnostic contains "Perf baseline is stale" +
//!      the recovery-command block.
//!
//! Uses a tempdir git repo so it doesn't perturb the workspace.

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
fn preflight_check_flags_stale_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // Set up a git repo with one committed waybill-runtime file.
    git(&["init", "--initial-branch=main"], root);
    git(&["config", "user.email", "test@waybill.dev"], root);
    git(&["config", "user.name", "Test"], root);
    git(&["config", "commit.gpgsign", "false"], root);
    write(&root.join("waybill-cli/src/scan_fs/mod.rs"), "// v1\n");
    write(&root.join("Cargo.lock"), "# lockfile\n");
    git(&["add", "-A"], root);
    git(&["commit", "-m", "baseline commit"], root);
    let baseline_sha = git_stdout(&["rev-parse", "HEAD"], root);
    assert_eq!(baseline_sha.len(), 40, "sha should be 40 chars");

    // Advance HEAD with a change to a waybill-runtime file.
    write(&root.join("waybill-cli/src/scan_fs/mod.rs"), "// v2 — CHANGED\n");
    git(&["add", "-A"], root);
    git(&["commit", "-m", "advance HEAD"], root);
    let head_sha = git_stdout(&["rev-parse", "HEAD"], root);
    assert_ne!(baseline_sha, head_sha, "HEAD should have advanced");

    // Plant a baseline pointing at the old commit.
    let baseline_path = root.join("baseline.json");
    let baseline_json = format!(
        r#"{{
            "schema_version": 1,
            "metadata": {{
                "waybill_commit_sha": "{baseline_sha}",
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

    // Run the staleness check.
    let outcome = preflight::check(&baseline_path, root).unwrap();
    assert!(outcome.is_stale, "expected is_stale=true (diff was: {:?})", outcome.diff_output);
    assert_eq!(outcome.baseline_sha, baseline_sha);
    assert_eq!(outcome.head_sha, head_sha);
    // The diff should mention the modified file.
    assert!(
        outcome.diff_output.contains("waybill-cli/src/scan_fs/mod.rs"),
        "expected diff to mention the modified file; got: {:?}",
        outcome.diff_output
    );

    // Format the diagnostic — contract C-5 anchors.
    let msg = preflight::format_diagnostic(&outcome);
    assert!(msg.contains("Perf baseline is stale."));
    assert!(msg.contains("cargo run -p xtask -- bench --update-baseline"));
    assert!(msg.contains(&baseline_sha));
    assert!(msg.contains(&head_sha));
}

#[test]
fn preflight_check_ignores_docs_only_changes() {
    // Contract R7: docs-only OR CI-only changes should NOT trigger
    // staleness — only waybill-runtime scope matters.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    git(&["init", "--initial-branch=main"], root);
    git(&["config", "user.email", "test@waybill.dev"], root);
    git(&["config", "user.name", "Test"], root);
    git(&["config", "commit.gpgsign", "false"], root);
    write(&root.join("waybill-cli/src/main.rs"), "// baseline\n");
    write(&root.join("docs/README.md"), "docs v1\n");
    git(&["add", "-A"], root);
    git(&["commit", "-m", "baseline"], root);
    let baseline_sha = git_stdout(&["rev-parse", "HEAD"], root);

    // Docs-only change.
    write(&root.join("docs/README.md"), "docs v2 — WORDS ADDED\n");
    git(&["add", "-A"], root);
    git(&["commit", "-m", "docs: update README"], root);

    let baseline_path = root.join("baseline.json");
    let baseline_json = format!(
        r#"{{
            "schema_version": 1,
            "metadata": {{
                "waybill_commit_sha": "{baseline_sha}",
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
        "docs-only change should NOT trigger staleness; diff: {:?}",
        outcome.diff_output
    );
}
