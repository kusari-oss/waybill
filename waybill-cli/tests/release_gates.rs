//! Mutation tests for the two release gates in `scripts/`.
//!
//! Both gates exist because prose did not hold. `CLAUDE.md` has said "check
//! the gate's own exit status, not a pipeline's" and "measure before
//! designing" for milestones, with worked examples, and v0.11.0 still shipped
//! as a patch and still had its fixtures hand-edited to clear a grep. A rule
//! that needs cooperation is not a gate.
//!
//! So each test here answers the only question that matters about a gate:
//! **can it fail?** A gate that cannot is indistinguishable from one that
//! passed, which is the same vacuity trap the SBOM invariant catalogue exists
//! to avoid (`docs/reference/sbom-invariants.md`).
//!
//! Unix-only, deliberately and at the test level rather than via an early
//! `return` inside each body: `gate.sh` is POSIX shell, the gates run in CI's
//! Linux lane and on maintainers' macOS, and Windows has no role in cutting a
//! release. An early-return shim would also trip Windows clippy, which
//! `pre-pr.sh` cannot see.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("waybill-cli sits inside the workspace root")
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// scripts/check-release-version.py
// ---------------------------------------------------------------------------

/// Run the version gate against a synthetic changelog + Cargo.toml.
fn check_version(changelog: &str, cargo_version: &str) -> Output {
    let dir = tempfile::tempdir().expect("tempdir");
    let cl = dir.path().join("CHANGELOG.md");
    let ct = dir.path().join("Cargo.toml");
    std::fs::write(&cl, changelog).expect("write changelog");
    std::fs::write(
        &ct,
        format!("[workspace.package]\nversion = \"{cargo_version}\"\n"),
    )
    .expect("write cargo");
    Command::new("python3")
        .arg(repo_root().join("scripts/check-release-version.py"))
        .arg(&cl)
        .arg(&ct)
        .output()
        .expect("python3 is on PATH; CI workflows already invoke it for check-version-bump.py")
}

fn stdout_of(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr_of(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// The v0.11.0 shape: a patch version over a changelog section that adds a
/// subcommand. This is the defect the gate was written for.
#[test]
fn a_patch_release_that_adds_something_is_refused() {
    let out = check_version(
        "# Changelog\n\n## [Unreleased]\n\n## [0.10.1] - 2026-10-16\n\n\
         ### Added\n\n- **`waybill sbom edit`** derives an edited SBOM.\n\
         - **`waybill sbom verify-chain`** checks a signature.\n\n\
         ## [0.10.0] - 2026-10-05\n\n### Fixed\n\n- something\n",
        "0.10.1",
    );
    assert!(
        !out.status.success(),
        "a patch release carrying `### Added` entries must be refused; got stdout={} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    let msg = format!("{}{}", stdout_of(&out), stderr_of(&out));
    assert!(msg.contains("patch bump"), "the message must name the class: {msg}");
    assert!(
        msg.contains("sbom edit"),
        "the message must quote the offending entry, so the reader can judge it: {msg}"
    );
}

/// The control that makes the test above mean something: the *same* entries
/// under a minor version pass. Without this, the gate could be refusing the
/// `### Added` heading rather than the version class.
#[test]
fn the_same_added_entries_under_a_minor_version_are_accepted() {
    let out = check_version(
        "# Changelog\n\n## [Unreleased]\n\n## [0.11.0] - 2026-10-16\n\n\
         ### Added\n\n- **`waybill sbom edit`** derives an edited SBOM.\n\
         - **`waybill sbom verify-chain`** checks a signature.\n\n\
         ## [0.10.0] - 2026-10-05\n\n### Fixed\n\n- something\n",
        "0.11.0",
    );
    assert!(
        out.status.success(),
        "a minor release may add things: {}{}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert!(stdout_of(&out).contains("minor bump"), "{}", stdout_of(&out));
}

#[test]
fn a_patch_release_that_only_fixes_is_accepted() {
    let out = check_version(
        "# Changelog\n\n## [0.10.1] - 2026-10-16\n\n### Fixed\n\n- a real fix\n\n\
         ## [0.10.0] - 2026-10-05\n\n### Fixed\n\n- something\n",
        "0.10.1",
    );
    assert!(
        out.status.success(),
        "a patch that only fixes is exactly what a patch is for: {}{}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert!(stdout_of(&out).contains("patch bump"), "{}", stdout_of(&out));
}

/// Between releases `Cargo.toml` is one patch ahead of the changelog, because
/// `post-release.yml` put it there. That state must be *named*, not silently
/// passed: a gate whose quiet path looks like approval is how the placeholder
/// went unquestioned in the first place.
#[test]
fn a_development_state_is_named_rather_than_passed_silently() {
    let out = check_version(
        "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- work in progress\n\n\
         ## [0.11.0] - 2026-10-16\n\n### Fixed\n\n- x\n\n## [0.10.0] - 2026-10-05\n\n### Fixed\n\n- y\n",
        "0.11.1",
    );
    assert!(out.status.success(), "{}{}", stdout_of(&out), stderr_of(&out));
    let s = stdout_of(&out);
    assert!(
        s.contains("NOT A RELEASE"),
        "the development state must say so in its own words, never print OK: {s}"
    );
    assert!(
        !s.contains("OK:"),
        "a skipped judgement must not read as a passed one: {s}"
    );
}

/// Regression: the gate first compared `0.10.0` against its own
/// `0.10.0-alpha.2`, whose numerics are identical, read that as a patch bump
/// from itself, and would have let any `### Added` through. The predecessor is
/// the previous *stable* release.
#[test]
fn a_pre_release_of_the_same_version_is_not_the_comparison_basis() {
    let out = check_version(
        "# Changelog\n\n## [0.10.0] - 2026-10-05\n\n### Added\n\n- a feature\n\n\
         ## [0.10.0-alpha.2] - 2026-09-27\n\n### Fixed\n\n- x\n\n\
         ## [0.9.0] - 2026-09-01\n\n### Fixed\n\n- y\n",
        "0.10.0",
    );
    assert!(
        out.status.success(),
        "0.10.0's predecessor is 0.9.0, making this a minor: {}{}",
        stdout_of(&out),
        stderr_of(&out)
    );
    let s = stdout_of(&out);
    assert!(
        s.contains("minor bump from 0.9.0"),
        "the basis must be the previous stable release, not this version's own alpha: {s}"
    );
}

/// The committed tree must satisfy its own gate, or the gate is decoration.
#[test]
fn the_committed_tree_satisfies_the_version_gate() {
    let root = repo_root();
    let out = Command::new("python3")
        .arg(root.join("scripts/check-release-version.py"))
        .arg(root.join("CHANGELOG.md"))
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("python3 on PATH");
    assert!(
        out.status.success(),
        "the committed CHANGELOG.md + Cargo.toml must pass: {}{}",
        stdout_of(&out),
        stderr_of(&out)
    );
}

// ---------------------------------------------------------------------------
// scripts/gate.sh
// ---------------------------------------------------------------------------

struct Gate {
    out: Output,
    log: String,
}

fn run_gate(args: &[&str], shell_cmd: &str) -> Gate {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = args
        .iter()
        .rev()
        .find(|a| !a.starts_with("--"))
        .copied()
        .unwrap_or("gate");
    let out = Command::new(repo_root().join("scripts/gate.sh"))
        .args(args)
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(shell_cmd)
        .env("GATE_LOG_DIR", dir.path())
        .output()
        .expect("gate.sh is executable");
    let log = std::fs::read_to_string(dir.path().join(format!("{name}.log"))).unwrap_or_default();
    Gate { out, log }
}

#[test]
fn a_failing_command_fails_the_gate() {
    let g = run_gate(&["boom"], "echo working; exit 3");
    assert!(!g.out.status.success(), "a command exiting 3 must fail the gate");
    assert!(g.log.contains("GATE_EXIT=3"), "log must record the real status: {}", g.log);
    assert!(g.log.contains("GATE_RESULT=fail"), "{}", g.log);
}

/// The reason `--expect` exists. A build that never compiled contains no
/// failures either, so exiting 0 is not evidence the work happened.
#[test]
fn exiting_zero_without_the_expected_signal_fails() {
    let g = run_gate(&["--expect", "all tests passed", "quiet"], "echo nothing to do");
    assert!(
        !g.out.status.success(),
        "a gate that exits 0 without its positive signal must fail: {}",
        String::from_utf8_lossy(&g.out.stdout)
    );
    assert!(g.log.contains("GATE_EXIT=0"), "{}", g.log);
    assert!(g.log.contains("GATE_RESULT=fail"), "{}", g.log);
    assert!(
        g.log.contains("GATE_MISSING_EXPECTED=all tests passed"),
        "the log must name which signal was absent: {}",
        g.log
    );
}

#[test]
fn a_present_expected_signal_passes() {
    let g = run_gate(&["--expect", "all tests passed", "good"], "echo all tests passed");
    assert!(
        g.out.status.success(),
        "stdout={} log={}",
        String::from_utf8_lossy(&g.out.stdout),
        g.log
    );
    assert!(g.log.contains("GATE_RESULT=pass"), "{}", g.log);
}

/// The property the script exists for: a caller who pipes it loses `$?`, and
/// the log still carries the verdict.
#[test]
fn the_verdict_survives_a_caller_that_loses_the_exit_code() {
    let g = run_gate(&["boom"], "exit 3");
    assert!(
        g.log.lines().any(|l| l == "GATE_RESULT=fail"),
        "GATE_RESULT must be recoverable from the log alone: {}",
        g.log
    );
    assert!(
        g.log.lines().any(|l| l == "GATE_EXIT=3"),
        "GATE_EXIT must be recoverable from the log alone: {}",
        g.log
    );
}

/// Regression: the summary used to re-grep the log *after* appending a footer
/// that quotes the patterns it reports on, so a missing signal was found in
/// the very line saying it was missing, and the gate printed
/// `expect: found` for something absent. The verdict was right and the
/// human-readable line lied — the defect this script exists to prevent,
/// committed by the script itself.
#[test]
fn an_expected_signal_is_not_satisfied_by_the_gates_own_footer() {
    let g = run_gate(&["--expect", "GATE_MISSING_EXPECTED", "selfref"], "echo nothing");
    let stdout = String::from_utf8_lossy(&g.out.stdout);
    assert!(
        !g.out.status.success(),
        "a pattern absent from the command's output must fail even when the footer contains it: {stdout}"
    );
    assert!(
        stdout.contains("expect: MISSING"),
        "the summary must report it missing, not found: {stdout}"
    );
    assert!(
        !stdout.contains("expect: found"),
        "the summary must not claim to have found it: {stdout}"
    );
}
