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

// ---------------------------------------------------------------------------
// scripts/nightly-baseline.sh
// ---------------------------------------------------------------------------

/// Pipe a tag list into the baseline script, the way `nightly.yml` pipes
/// `git tag -l 'v*'` into it.
fn nightly_baseline(tags: &[&str]) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(repo_root().join("scripts/nightly-baseline.sh"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("nightly-baseline.sh is executable");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(format!("{}\n", tags.join("\n")).as_bytes())
        .expect("write tag list");
    child.wait_with_output().expect("collect output")
}

/// #1263 — the case that published an inverted nightly. With `v0.12.1` the
/// latest stable, the baseline must be `0.12.2`, never `0.12.1`: a
/// `v0.12.1-nightly.<date>` tag sorts BEFORE `v0.12.1` under SemVer, so a
/// newer build would look older. Before this the baseline came from
/// `Cargo.toml`, which still said `0.12.1` because the step-10 bump had
/// not merged.
#[test]
fn the_nightly_baseline_outranks_the_latest_stable() {
    let out = nightly_baseline(&["v0.12.0", "v0.12.1", "v0.12.1-nightly.20261023"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(stdout_of(&out).trim(), "0.12.2");
}

/// Prerelease tags must not seed a baseline. A nightly built off the last
/// nightly would ratchet the version forward every day, and an `-alpha.`
/// bridge pre-release (RELEASING.md §4) is not a release either.
#[test]
fn prerelease_tags_do_not_seed_the_nightly_baseline() {
    let out = nightly_baseline(&[
        "v0.12.1",
        "v0.13.0-nightly.20261024",
        "v0.14.0-alpha.1",
        "v0.99.0-rc.1",
    ]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(
        stdout_of(&out).trim(),
        "0.12.2",
        "only the bare vX.Y.Z tag may seed the baseline"
    );
}

/// Numeric comparison, not lexical: `v0.10.0` outranks `v0.9.0`, and
/// `sort` without numeric keys would disagree.
#[test]
fn the_nightly_baseline_compares_versions_numerically() {
    let out = nightly_baseline(&["v0.9.0", "v0.10.0", "v0.2.0"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(stdout_of(&out).trim(), "0.10.1");
}

/// A repository with no stable release yet still has to produce a usable
/// baseline — the nightly channel predates the first stable tag.
#[test]
fn a_repository_with_no_stable_tag_still_gets_a_baseline() {
    let out = nightly_baseline(&["v0.1.0-alpha.3"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(stdout_of(&out).trim(), "0.0.1");
}

// ---------------------------------------------------------------------------
// scripts/release-channel.sh
// ---------------------------------------------------------------------------

/// Classify one tag, the way both `release.yml` steps call the script.
fn release_channel(tag: &str) -> Output {
    Command::new(repo_root().join("scripts/release-channel.sh"))
        .arg(tag)
        .output()
        .expect("release-channel.sh is executable")
}

fn channel_of(tag: &str) -> String {
    let out = release_channel(tag);
    assert!(out.status.success(), "{tag}: {}", stderr_of(&out));
    stdout_of(&out).trim().to_string()
}

/// #1266 — the tag shapes that must NOT move `:latest`. The container job
/// emitted it unconditionally, so every 06:00Z nightly took
/// `ghcr.io/kusari-oss/waybill:latest` off the newest stable; measured on
/// 2026-10-23, `:latest` pointed at `0.12.1-nightly.20261023` from 06:37Z
/// until v0.13.0 published at 18:55Z.
#[test]
fn only_a_bare_semver_tag_is_the_stable_channel() {
    for stable in ["v0.13.0", "v1.0.0", "v0.0.1", "v10.20.30"] {
        assert_eq!(channel_of(stable), "stable", "{stable} is a stable tag");
    }
    for prerelease in [
        "v0.12.1-nightly.20261023",
        "v0.14.0-alpha.1",
        "v0.99.0-rc.1",
        "v0.1.0+build.5",
        "v0.13.0-0",
    ] {
        assert_eq!(
            channel_of(prerelease),
            "prerelease",
            "{prerelease} must never move `:latest`"
        );
    }
}

/// A tag that is not SemVer at all is not a stable release either. The
/// classifier must not fall through to `stable` on a shape it does not
/// recognise — that is the direction that republishes `:latest`.
#[test]
fn an_unrecognised_tag_shape_is_not_stable() {
    for odd in ["0.13.0", "v0.13", "v0.13.0.1", "main", "latest", "v0.13.0 "] {
        assert_eq!(
            channel_of(odd),
            "prerelease",
            "{odd:?} is not a bare vX.Y.Z tag"
        );
    }
}

/// A missing or empty tag is a caller bug, and must not be answered with a
/// channel. Exiting non-zero under `set -euo pipefail` fails the step; a
/// silent `prerelease` would hide a workflow that lost `TAG_NAME`.
#[test]
fn a_missing_tag_is_an_error_not_a_channel() {
    for argv in [vec![], vec![""], vec!["v0.13.0", "extra"]] {
        let out = Command::new(repo_root().join("scripts/release-channel.sh"))
            .args(&argv)
            .output()
            .expect("release-channel.sh is executable");
        assert!(
            !out.status.success(),
            "argv {argv:?} must not produce a channel: {}",
            stdout_of(&out)
        );
        assert!(
            stdout_of(&out).trim().is_empty(),
            "argv {argv:?} printed a channel on the error path: {}",
            stdout_of(&out)
        );
    }
}

/// Both callers run the script by a repository-relative path, so both jobs
/// must check the repository out and the file must be executable. A
/// workflow that calls a path that is not there fails the release, not the
/// nightly only, so assert the shape here rather than discovering it at
/// tag-push time.
#[test]
fn both_release_yml_steps_call_the_one_channel_script() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("release.yml is readable");
    assert_eq!(
        yml.matches("./scripts/release-channel.sh").count(),
        2,
        "`Compute image tags` and `Compute prerelease flag from tag` must \
         both consult the shared classifier; a third inline copy of the \
         bare-SemVer regex is how #1266 happened"
    );
    // Every line that emits `:latest` must sit inside the stable branch.
    // Walk back from it to the nearest `if`/`fi` and assert which one it is
    // — a grep for the two strings anywhere in the file would pass even if
    // the guard moved off this echo, which is the exact mistake #1266 was.
    let lines: Vec<&str> = yml.lines().collect();
    let latest_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("waybill:latest"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        latest_lines.len(),
        1,
        "exactly one step may emit the `:latest` tag"
    );
    let guard = lines[..latest_lines[0]]
        .iter()
        .rev()
        .find(|l| {
            let s = l.trim_start();
            s.starts_with("if ") || s.starts_with("fi")
        })
        .copied()
        .unwrap_or("<no enclosing conditional>");
    assert_eq!(
        guard.trim(),
        "if [ \"$channel\" = \"stable\" ]; then",
        "`:latest` is emitted outside the stable-channel guard"
    );
    let meta = std::fs::metadata(repo_root().join("scripts/release-channel.sh"))
        .expect("release-channel.sh exists");
    use std::os::unix::fs::PermissionsExt;
    assert!(
        meta.permissions().mode() & 0o111 != 0,
        "release-channel.sh must be executable; the workflow invokes it directly"
    );
}

/// The baseline has to sort after the latest stable under SemVer, not just
/// under whatever `sort` does. This is the comparison the fix turns on, and
/// `sort -V` gets it wrong — it places `v0.12.1-nightly.20261023` AFTER
/// `v0.12.1`, which is how an inverted tag could look correct to a guard
/// built on it. Asserted here against the real tool so the claim in
/// `nightly-baseline.sh`'s header cannot rot.
#[test]
fn sort_v_disagrees_with_semver_on_prereleases() {
    let out = Command::new("sh")
        .arg("-c")
        .arg("printf 'v0.12.1\\nv0.12.1-nightly.20261023\\n' | sort -V | tail -1")
        .output()
        .expect("sh is on PATH");
    assert_eq!(
        stdout_of(&out).trim(),
        "v0.12.1-nightly.20261023",
        "if sort -V ever becomes SemVer-aware, nightly-baseline.sh's \
         rationale needs rewriting — not its behaviour, which is correct \
         either way"
    );
}

// ---------------------------------------------------------------------------
// #1287 — a workflow that picks its own checkout ref must resolve the
// commit it scanned
// ---------------------------------------------------------------------------

/// On a `workflow_dispatch`, `github.sha` is the SHA of the ref the workflow
/// definition was loaded from — the default branch — whatever a `branch`
/// input names. A workflow whose Checkout uses `inputs.branch` and whose
/// output quotes `github.sha` therefore attests to a commit it never looked
/// at. The corpus lane did exactly that inside `corpus-run-summary.md`, the
/// durable artifact a maintainer quotes when landing a change that moves
/// goldens, and `quality-corpus.yml` did it in an artifact name.
///
/// The rule is derived from the files rather than from a list of workflow
/// names, because a list that mirrors a directory falls out of date the way
/// #1287's own un-audited guess did: it named `ebpf-canary.yml` and the
/// release path as suspects, and neither mentions `github.sha` at all,
/// while `quality-corpus.yml` — which it did not name — had the defect.
///
/// What is asserted is that such a workflow resolves the scanned commit
/// itself and uses the result. Whether a *label* describes a `github.sha`
/// honestly is not mechanically decidable, so the two honest shapes are a
/// closed set below: a third one has to justify itself by editing this test.
#[test]
fn a_workflow_that_picks_its_own_ref_resolves_the_commit_it_scanned() {
    let dir = repo_root().join(".github/workflows");
    let mut checked: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect(".github/workflows is readable")
        .map(|e| e.expect("readable dir entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yml"))
        .collect();
    paths.sort();

    for path in paths {
        let yml = std::fs::read_to_string(&path).expect("workflow is readable");
        // A checkout that resolves a ref from a dispatch input is what makes
        // `github.sha` and the scanned commit diverge. A `pull_request_target`
        // workflow that checks out the default branch on purpose is not in
        // this class — it resolves its subject from the API instead, which
        // `review-gate.yml` does and documents.
        if !yml.contains("ref: ${{ github.event.inputs.branch") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unnamed>")
            .to_string();
        checked.push(name.clone());

        if !yml.contains("git rev-parse HEAD") {
            problems.push(format!(
                "{name}: checks out a ref of its own choosing but never resolves the \
                 commit it got. Without `git rev-parse HEAD` after checkout there is \
                 nothing to report but `github.sha`, which is the default branch's tip."
            ));
        }
        if !yml.contains("steps.scanned.outputs.sha") {
            problems.push(format!(
                "{name}: resolves nothing that anything consumes. A resolution step \
                 whose output is never read leaves the reported commit as wrong as \
                 before while looking fixed."
            ));
        }
        for (i, line) in yml.lines().enumerate() {
            if !line.contains("github.sha") {
                continue;
            }
            // A comment mentioning it is inert, in YAML and in a `run:`
            // block's shell alike — and this file's own rationale mentions
            // it repeatedly, which is how this skip was discovered.
            if line.trim_start().starts_with('#') {
                continue;
            }
            // The two honest shapes, deliberately a closed set.
            let fallback_after_scanned = line.contains("steps.scanned.outputs.sha ||");
            let labelled_as_definition = line.contains("DEFINITION");
            if !fallback_after_scanned && !labelled_as_definition {
                problems.push(format!(
                    "{name}:{}: `{}` — a `github.sha` here is either the workflow \
                     definition's commit, in which case name it so (a variable whose \
                     name contains DEFINITION), or a fallback after \
                     `steps.scanned.outputs.sha ||`. Reported as anything else it \
                     names a commit this run did not scan (#1287).",
                    i + 1,
                    line.trim()
                ));
            }
        }
    }

    // The positive control. Without it this test passes when the filter
    // matches nothing — a reworded checkout, a renamed input or a path typo
    // all look exactly like "no workflow has this defect".
    assert!(
        checked.len() >= 2,
        "expected at least two workflows resolving a checkout ref from a dispatch \
         input (public-corpus.yml and quality-corpus.yml both do); found {checked:?}. \
         Either they were reworded, in which case this test checks nothing, or the \
         filter is wrong."
    );
    assert!(
        problems.is_empty(),
        "#1287:\n  {}",
        problems.join("\n  ")
    );
}
