//! Milestone 074 — build-tier identifier auto-detection integration
//! tests.
//!
//! ## Test harness rationale
//!
//! `waybill trace run` (the build-tier entry point) attaches eBPF
//! kprobes/uprobes and therefore requires a Linux kernel with
//! `ebpf-tracing` feature flag enabled. On macOS — and on
//! unprivileged Linux — actually invoking the binary's
//! `trace run` subcommand against a real fixture is not feasible
//! (matches the milestone-073 policy documented at
//! `waybill-cli/tests/identifiers_per_tier.rs:11-16`).
//!
//! Instead this file exercises milestone 074's auto-detection at
//! two levels:
//!
//! 1. **Direct library-level tests** of
//!    `waybill::binding::identifiers::auto_detect::
//!    auto_detect_build_tier_identifiers` against tempdir-based git
//!    fixtures — covers FR-001, FR-002, FR-003, FR-009, FR-010 and
//!    the spec's US1/US2 acceptance scenarios. The function is the
//!    sole new behavioral surface for milestone 074, so testing it
//!    directly covers the contract.
//!
//! 2. **Cross-tier correlation test** combining (a) the same
//!    auto-detect helper with (b) the `waybill sbom scan --path`
//!    binary's source-tier auto-detection over the SAME git fixture
//!    — verifies that the `repo:` URL byte-for-byte matches across
//!    tiers (SC-002), even though we cannot drive a live `trace run`
//!    on this platform.
//!
//! The unit-level coverage in `binding/identifiers/auto_detect.rs::tests`
//! also covers the inner helpers (`git_rev_parse_head`,
//! `discover_repo_url`); this file's tests focus on the public
//! `auto_detect_build_tier_identifiers` boundary.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::Path;
use std::process::Command;

use waybill::binding::identifiers::auto_detect::auto_detect_build_tier_identifiers;
use waybill::binding::identifiers::IdentifierKind;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

// ---------------------------------------------------------------------
// Tempdir-based git fixture builder
// ---------------------------------------------------------------------

fn run_git(dir: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir.to_str().unwrap()).args(args);
    let status = cmd.status().expect("git subprocess");
    assert!(status.success(), "git command failed: {cmd:?}");
}

fn git_init(dir: &Path) {
    run_git(dir, &["init", "-q"]);
}

fn git_remote_add(dir: &Path, name: &str, url: &str) {
    run_git(dir, &["remote", "add", name, url]);
}

fn git_config_user(dir: &Path) {
    run_git(dir, &["config", "user.email", "test@example.com"]);
    run_git(dir, &["config", "user.name", "Test User"]);
}

fn git_commit_empty(dir: &Path, msg: &str) {
    git_config_user(dir);
    run_git(dir, &["commit", "--allow-empty", "-q", "-m", msg]);
}

fn git_rev_parse_head_subprocess(dir: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir.to_str().unwrap())
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git subprocess");
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Make a tempdir that's a git checkout with the supplied remotes
/// (and optionally one empty commit). The tempdir is returned so
/// callers can hold it for the test's lifetime.
fn make_git_fixture(remotes: &[(&str, &str)], commit: bool) -> tempfile::TempDir {
    let td = tempfile::tempdir().unwrap();
    git_init(td.path());
    for (name, url) in remotes {
        git_remote_add(td.path(), name, url);
    }
    if commit {
        git_commit_empty(td.path(), "initial");
    }
    td
}

// ---------------------------------------------------------------------
// User Story 1 — auto-detected `repo:` on build-tier scans
// ---------------------------------------------------------------------

/// US1 acceptance #1 — `origin` configured: `repo:` auto-detects.
#[test]
fn build_tier_autodetect_repo_in_git_checkout() {
    let td = make_git_fixture(&[("origin", "git@github.com:acme/foo.git")], false);
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert!(!ids.is_empty(), "expected `repo:` auto-detected");
    assert_eq!(ids[0].scheme.as_str(), "repo");
    assert_eq!(ids[0].value.as_str(), "git@github.com:acme/foo.git");
    let label = ids[0].source_label.as_deref().unwrap();
    assert!(
        label.contains("build-tier"),
        "VR-074-004: source_label must contain `build-tier`; got {label:?}"
    );
    assert_eq!(label, "auto-detected from build-tier git remote `origin`");
}

/// US1 acceptance #2 — `origin` absent, `upstream` present.
#[test]
fn build_tier_autodetect_upstream_fallback() {
    let td = make_git_fixture(&[("upstream", "git@github.com:acme/up.git")], false);
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].value.as_str(), "git@github.com:acme/up.git");
    assert_eq!(
        ids[0].source_label.as_deref(),
        Some("auto-detected from build-tier git remote `upstream`")
    );
}

/// US1 edge case — both `origin` and `upstream` absent, exotic
/// remotes only. Picks the alphabetically first remote name and
/// records the `(origin/upstream absent; first-listed)` label suffix.
#[test]
fn build_tier_autodetect_first_listed_fallback() {
    let td = make_git_fixture(
        &[
            ("zebra", "git@example.com:z/foo.git"),
            ("alpha", "git@example.com:a/foo.git"),
        ],
        false,
    );
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].value.as_str(), "git@example.com:a/foo.git");
    let label = ids[0].source_label.as_deref().unwrap();
    assert!(label.contains("build-tier"));
    assert!(label.contains("`alpha`"));
    assert!(label.contains("first-listed"));
}

/// US1 acceptance #3 / SC-006 — non-git tempdir produces empty
/// identifier vec, no error, no panic.
#[test]
fn build_tier_skips_outside_git() {
    let td = tempfile::tempdir().unwrap();
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert!(
        ids.is_empty(),
        "non-git dir must yield zero auto-detected identifiers; got {ids:?}"
    );
}

/// US1 acceptance #4 / FR-004 — manual `--repo` overrides auto-
/// detected `repo:` (same scheme, different value). The integration
/// surface here is the `resolve_identifiers` helper which the
/// `run.rs::execute` flow routes through. We exercise it through
/// the public library API since we cannot invoke `trace run`
/// directly on this platform.
#[test]
fn build_tier_manual_repo_overrides_autodetect() {
    let td = make_git_fixture(&[("origin", "git@github.com:acme/foo.git")], false);
    let auto = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(auto.len(), 1);
    let manual = waybill::binding::identifiers::Identifier::parse(
        "repo:git@github.com:acme/override.git",
    )
    .unwrap();
    let resolved = waybill::binding::identifiers::resolve_identifiers(
        auto.clone(),
        std::slice::from_ref(&manual),
    );
    // The auto-detected `repo:` is dropped, manual takes its place
    // (in supply order — index 0 since there were no other entries).
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].value.as_str(), "git@github.com:acme/override.git");
}

// ---------------------------------------------------------------------
// User Story 2 — auto-detected commit-anchored `git:` on build-tier scans
// ---------------------------------------------------------------------

/// US2 acceptance #1 — full 40-char SHA preserved verbatim.
#[test]
fn build_tier_autodetect_git_with_full_sha() {
    let td = make_git_fixture(&[("origin", "git@github.com:acme/foo.git")], true);
    let head = git_rev_parse_head_subprocess(td.path());
    assert_eq!(head.len(), 40);

    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(ids.len(), 2, "expected [repo:, git:] when both resolvable");
    assert_eq!(ids[1].scheme.as_str(), "git");
    let git_value = ids[1].value.as_str();
    assert!(
        git_value.ends_with(&format!("#{head}")),
        "expected git: value to end with #<HEAD-sha>; got {git_value}"
    );
    assert_eq!(
        git_value,
        format!("git@github.com:acme/foo.git#{head}"),
        "git: value must be <repo-url>#<sha> form"
    );
    assert_eq!(
        ids[1].source_label.as_deref(),
        Some("auto-detected from build-tier `git rev-parse HEAD`")
    );
}

/// US2 acceptance #2 — detached HEAD treated as a normal scan.
/// `git:` still emits with the detached SHA.
#[test]
fn build_tier_autodetect_git_in_detached_head() {
    let td = make_git_fixture(&[("origin", "git@github.com:acme/foo.git")], true);
    let head = git_rev_parse_head_subprocess(td.path());
    // Detach HEAD by checking out the commit by SHA.
    run_git(td.path(), &["checkout", "--detach", "-q", &head]);

    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(ids.len(), 2, "detached HEAD is not an error condition");
    assert_eq!(ids[1].scheme.as_str(), "git");
    assert_eq!(
        ids[1].value.as_str(),
        format!("git@github.com:acme/foo.git#{head}")
    );
}

/// US2 acceptance #3 / FR-003 partial-skip — empty repo, no commits.
/// `repo:` still emits if a remote is configured; `git:` skips
/// silently.
#[test]
fn build_tier_skips_git_in_empty_repo() {
    let td = make_git_fixture(&[("origin", "git@github.com:acme/foo.git")], false);
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(
        ids.len(),
        1,
        "empty repo: expected only [repo:], got {ids:?}"
    );
    assert_eq!(ids[0].scheme.as_str(), "repo");
}

/// US2 acceptance #4 / SC-006 — non-git directory emits NEITHER
/// `repo:` nor `git:`.
#[test]
fn build_tier_emits_zero_identifiers_in_non_git_dir() {
    let td = tempfile::tempdir().unwrap();
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert!(ids.is_empty());
}

/// Research §7 — SSH-form remote produces a `git:` identifier slot
/// in the emitted vec with the verbatim URL. We do NOT assert on
/// `IdentifierKind` (that's a milestone-073 implementation detail).
#[test]
fn build_tier_autodetect_git_with_ssh_form_remote() {
    let td = make_git_fixture(&[("origin", "git@github.com:fake/repo.git")], true);
    let head = git_rev_parse_head_subprocess(td.path());
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[1].scheme.as_str(), "git");
    // Verbatim URL is preserved (no normalization to https).
    assert_eq!(
        ids[1].value.as_str(),
        format!("git@github.com:fake/repo.git#{head}")
    );
    // The kind is whatever `validate_git` happens to return — both
    // Builtin and UserDefined are acceptable per research §7. We
    // just confirm the identifier is well-formed.
    match ids[1].kind {
        IdentifierKind::Builtin(_) | IdentifierKind::UserDefined => {}
    }
}

// ---------------------------------------------------------------------
// SC-002 cross-tier correlation
// ---------------------------------------------------------------------

/// SC-002 cross-tier correlation. Source-tier `waybill sbom scan
/// --path` over the SAME git fixture must auto-detect the SAME
/// `repo:` URL byte-for-byte. The build-tier `git:` identifier's
/// commit SHA must match `git rev-parse HEAD` of the same fixture.
///
/// We can't drive `waybill trace run` directly on macOS, so we
/// invoke the source-tier scan binary AND call the build-tier
/// auto-detect helper on the same fixture — the two URLs are
/// extracted from each tier's auto-detection and asserted equal.
#[test]
fn build_tier_cross_tier_correlation_byte_identical_repo() {
    let td = make_git_fixture(&[("origin", "git@github.com:acme/foo.git")], true);
    // Drop a tiny manifest so the source-tier scan produces a
    // valid SBOM document (otherwise it errors out on no-content).
    std::fs::write(
        td.path().join("Cargo.toml"),
        b"[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(
        td.path().join("Cargo.lock"),
        b"version = 3\n\n[[package]]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let head = git_rev_parse_head_subprocess(td.path());

    // Build-tier auto-detect via library API.
    let build_ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(build_ids.len(), 2, "expected [repo:, git:] for build-tier");
    assert_eq!(build_ids[0].scheme.as_str(), "repo");
    let build_repo_value = build_ids[0].value.as_str().to_string();
    assert_eq!(build_ids[1].scheme.as_str(), "git");
    let build_git_value = build_ids[1].value.as_str().to_string();
    assert!(build_git_value.ends_with(&format!("#{head}")));

    // Source-tier auto-detect via the binary `sbom scan --path`.
    let fake_home = tempfile::tempdir().unwrap();
    let out_path = td.path().join("source.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    let out = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(td.path())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash")
        .output()
        .expect("scan runs");
    assert!(
        out.status.success(),
        "source-tier scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let cdx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("externalReferences array");
    let source_repo_url = refs
        .iter()
        .find(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"))
        .and_then(|r| r["url"].as_str())
        .expect("source-tier auto-detected `repo:` (type:vcs) entry must be present")
        .to_string();

    // SC-002: build-tier `repo:` value MUST match source-tier
    // `repo:` value byte-for-byte.
    assert_eq!(
        build_repo_value, source_repo_url,
        "SC-002: cross-tier `repo:` URL byte-equality required"
    );
    // SC-002: build-tier `git:` SHA MUST match `git rev-parse HEAD`
    // of the same checkout.
    assert!(build_git_value.contains(&head));
}

// ---------------------------------------------------------------------
// SC-007 determinism
// ---------------------------------------------------------------------

/// SC-007 / FR-009 — repeated invocations produce byte-identical
/// identifier slots given fixed git remote + fixed HEAD.
#[test]
fn build_tier_autodetect_deterministic_across_reruns() {
    let td = make_git_fixture(&[("origin", "git@github.com:acme/foo.git")], true);
    let a = auto_detect_build_tier_identifiers(td.path(), false);
    let b = auto_detect_build_tier_identifiers(td.path(), false);
    let c = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), c.len());
    for (idx, ((x, y), z)) in a.iter().zip(b.iter()).zip(c.iter()).enumerate() {
        assert_eq!(x.scheme.as_str(), y.scheme.as_str(), "scheme drift at {idx}");
        assert_eq!(x.scheme.as_str(), z.scheme.as_str(), "scheme drift at {idx}");
        assert_eq!(x.value.as_str(), y.value.as_str(), "value drift at {idx}");
        assert_eq!(x.value.as_str(), z.value.as_str(), "value drift at {idx}");
        assert_eq!(x.source_label, y.source_label, "label drift at {idx}");
        assert_eq!(x.source_label, z.source_label, "label drift at {idx}");
    }
}
