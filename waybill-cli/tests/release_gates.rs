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

// ---------------------------------------------------------------------------
// scripts/check-version-bump.py — the completeness pass
// ---------------------------------------------------------------------------

/// A throwaway git repo holding one fixture, so the completeness pass has a
/// base revision to diff against. The script shells out to `git diff`,
/// `git show` and `git grep`, and must run from the base revision (#1133), so
/// there is no way to exercise it without one.
struct BumpRepo {
    dir: tempfile::TempDir,
}

impl BumpRepo {
    /// Commit `fixtures` at the old version, then overwrite them with the
    /// post-bump content, leaving the tree in the state the script inspects.
    fn new(before: &[(&str, &str)], after: &[(&str, &str)], old: &str, new: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let git = |args: &[&str]| {
            let st = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git is on PATH");
            assert!(st.status.success(), "git {args:?}: {}", stderr_of(&st));
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@example.invalid"]);
        git(&["config", "user.name", "t"]);
        // The script only inspects Cargo.toml when it is among the changed
        // files, so the bump itself is represented by the fixtures alone.
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[workspace.package]\nversion = \"{old}\"\n"),
        )
        .expect("write Cargo.toml");
        for (path, body) in before {
            let p = root.join(path);
            std::fs::create_dir_all(p.parent().expect("fixture has a parent"))
                .expect("mkdir fixture dir");
            std::fs::write(&p, body).expect("write fixture");
        }
        git(&["add", "-A"]);
        git(&["commit", "-qm", "base"]);
        for (path, body) in after {
            std::fs::write(root.join(path), body).expect("overwrite fixture");
        }
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[workspace.package]\nversion = \"{new}\"\n"),
        )
        .expect("rewrite Cargo.toml");
        Self { dir }
    }

    fn check(&self, old: &str, new: &str) -> Output {
        Command::new("python3")
            .arg(repo_root().join("scripts/check-version-bump.py"))
            .args([old, new])
            .current_dir(self.dir.path())
            .output()
            .expect("python3 is on PATH")
    }
}

/// One CycloneDX golden: waybill's own entry under `metadata.tools`, plus a
/// dependency pinned at `dep_version`.
fn cdx_golden(tool_version: &str, dep_version: &str) -> String {
    format!(
        r#"{{
  "bomFormat": "CycloneDX",
  "metadata": {{
    "tools": {{
      "components": [
        {{ "type": "application", "name": "waybill", "version": "{tool_version}" }}
      ]
    }}
  }},
  "components": [
    {{ "type": "library", "name": "pathspec", "version": "{dep_version}",
       "purl": "pkg:pypi/pathspec@{dep_version}" }}
  ]
}}
"#
    )
}

/// #1251. The bump after v0.12.1 failed on `pkg:pypi/pathspec@0.12.1` in two
/// corpus goldens: a real dependency the regen sweep correctly left alone,
/// read as a golden the sweep had missed. The v0.12.0 cut failed the same way
/// on `pkg:hackage/tasty-quickcheck@0.11.1`. Corpus goldens mask the tool
/// version, so they never change in a bump and never reach the comparison
/// pass — the completeness grep was the only thing looking at them, and it
/// could not tell a dependency from the tool.
#[test]
fn a_dependency_sitting_at_the_released_version_does_not_fail_the_bump() {
    // The dependency stays at 0.12.1 on both sides, as a real one would.
    let repo = BumpRepo::new(
        &[(
            "waybill-cli/tests/fixtures/public_corpus/flask/cdx.json",
            &cdx_golden("0.12.1", "0.12.1"),
        )],
        &[(
            "waybill-cli/tests/fixtures/public_corpus/flask/cdx.json",
            &cdx_golden("0.12.2", "0.12.1"),
        )],
        "0.12.1",
        "0.12.2",
    );
    let out = repo.check("0.12.1", "0.12.2");
    assert!(
        out.status.success(),
        "a dependency at the old version must not be read as a stale golden:\n{}\n{}",
        stdout_of(&out),
        stderr_of(&out)
    );
}

/// The other half, and the one that matters more: the pass must still catch
/// what #1221 wrote it for. A fixture that did not change at all cannot
/// "differ beyond the version", so the comparison pass is silent about it;
/// only this grep sees it. A fix that merely silenced the false positive
/// would leave the gate unable to fail, which is the vacuity this file exists
/// to rule out.
#[test]
fn a_golden_the_regen_sweep_never_reached_still_fails_the_bump() {
    let stale = cdx_golden("0.12.1", "1.0.0");
    let repo = BumpRepo::new(
        &[
            (
                "waybill-cli/tests/fixtures/golden/cyclonedx/apk.cdx.json",
                &cdx_golden("0.12.1", "1.0.0"),
            ),
            (
                "waybill-cli/tests/fixtures/golden/cyclonedx/deb.cdx.json",
                &cdx_golden("0.12.1", "1.0.0"),
            ),
        ],
        &[
            (
                "waybill-cli/tests/fixtures/golden/cyclonedx/apk.cdx.json",
                &cdx_golden("0.12.2", "1.0.0"),
            ),
            // deb.cdx.json left at the old tool version: the missed golden.
            (
                "waybill-cli/tests/fixtures/golden/cyclonedx/deb.cdx.json",
                &stale,
            ),
        ],
        "0.12.1",
        "0.12.2",
    );
    let out = repo.check("0.12.1", "0.12.2");
    assert!(
        !out.status.success(),
        "a golden still carrying the old tool version must fail:\n{}",
        stdout_of(&out)
    );
    assert!(
        stderr_of(&out).contains("deb.cdx.json"),
        "the failure must name the stale golden: {}",
        stderr_of(&out)
    );
}

/// Fails closed. The pass exists to catch a fixture the sweep never reached,
/// and one the structural check cannot parse is that case unexamined — so it
/// is reported rather than skipped.
#[test]
fn a_fixture_the_structural_check_cannot_parse_is_reported() {
    let repo = BumpRepo::new(
        &[(
            "waybill-cli/tests/fixtures/golden/cyclonedx/truncated.cdx.json",
            "{ \"metadata\": { \"version\": \"0.12.1\"",
        )],
        &[(
            "waybill-cli/tests/fixtures/golden/cyclonedx/truncated.cdx.json",
            "{ \"metadata\": { \"version\": \"0.12.1\"",
        )],
        "0.12.1",
        "0.12.2",
    );
    let out = repo.check("0.12.1", "0.12.2");
    assert!(
        !out.status.success(),
        "an unparseable fixture carrying the old version must not pass silently:\n{}",
        stdout_of(&out)
    );
}
