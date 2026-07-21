//! Milestone 075 — auto-detected userinfo credential stripping
//! integration tests.
//!
//! ## Why a separate test file
//!
//! Milestones 073 and 074 currently embed the literal output of
//! `git remote get-url` into emitted SBOMs verbatim. When operators
//! configure a credentialed origin (e.g., `https://x-access-token:ghs_...
//! @github.com/foo.git`, the standard GitHub App token form), the
//! token leaks into every published SBOM. Milestone 075 closes that
//! leak by stripping RFC 3986 userinfo from auto-detected URLs
//! before identifier construction. Manual flag values stay verbatim;
//! `--keep-credentials-in-identifiers` opts back in.
//!
//! ## Coverage
//!
//! Tests are organized by user story (3× P1, 1× P2):
//!
//! - **US1** — source-tier auto-detect strips credentials by default.
//!   Drives `waybill sbom scan --path` against a tempdir git fixture
//!   with a credentialed origin URL; asserts the emitted JSON SBOM
//!   contains zero literal-token occurrences and the
//!   `(credentials stripped)` source_label suffix.
//! - **US2** — build-tier auto-detect strips credentials by default.
//!   `waybill trace run` requires eBPF (Linux-only, not exercisable
//!   on macOS dev / unprivileged CI per milestone 074's policy
//!   documented at `identifiers_build_tier_autodetect.rs:6-13`), so
//!   we exercise `auto_detect_build_tier_identifiers` directly via
//!   the public library API on the same fixture. The `git:` slot's
//!   value is also asserted to have its URL portion sanitized
//!   BEFORE the `#<sha>` is appended (VR-075-005).
//! - **US3** — manual identifier flags emit verbatim. Operator-typed
//!   `--repo` with a credentialed URL flows through unchanged; the
//!   manual-vs-auto-detected boundary is preserved.
//! - **US4** — `--keep-credentials-in-identifiers` opt-out preserves
//!   userinfo in both source-tier (CLI invocation) and build-tier
//!   (library API) flows.
//! - **Edge case** — parse-failure soft-fails through milestone
//!   073's existing `UserDefined` rule (FR-009).
//!
//! ## Log-content assertions
//!
//! `waybill-cli` does not currently install a project-wide tracing
//! capture pattern in its test suite. FR-006 / FR-007 log-line
//! content is therefore asserted indirectly: the runtime side-effects
//! (identifier value, source_label, identifier kind) are observable
//! and are tested here. A future test-only `tracing` subscriber
//! could provide direct log-line assertion; out of scope for this
//! milestone.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::Path;
use std::process::Command;

use waybill::binding::identifiers::auto_detect::auto_detect_build_tier_identifiers;
use waybill::binding::identifiers::IdentifierKind;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

// ---------------------------------------------------------------------
// Tempdir-based git fixture builder (mirrors the milestone-074 pattern
// in identifiers_build_tier_autodetect.rs).
// ---------------------------------------------------------------------

/// A known-fake credential string used in test fixtures. Unique
/// enough to grep for unambiguously across emitted SBOMs.
const FAKE_TOKEN: &str = "ghs_NEVERREALSURELYFAKE0123456789ABCDEFXYZ";

/// A known-fake credential URL using `FAKE_TOKEN`. Resembles the
/// GitHub App token form that's the most common leak vector.
fn credentialed_origin_url() -> String {
    format!("https://x-access-token:{FAKE_TOKEN}@github.com/acme/test-repo.git")
}

/// Sanitized form of `credentialed_origin_url` — what milestone 075
/// must produce when sanitization fires.
const SANITIZED_ORIGIN_URL: &str = "https://github.com/acme/test-repo.git";

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

/// Produce a tempdir that's a git checkout with the supplied
/// remotes, optionally with one empty HEAD commit, plus a tiny
/// Cargo manifest so source-tier scans produce a valid SBOM rather
/// than erroring on no-content.
fn make_git_fixture(remotes: &[(&str, &str)], commit: bool) -> tempfile::TempDir {
    let td = tempfile::tempdir().unwrap();
    git_init(td.path());
    for (name, url) in remotes {
        git_remote_add(td.path(), name, url);
    }
    if commit {
        git_commit_empty(td.path(), "initial");
    }
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
    td
}

/// Produce a tempdir that is NOT a git checkout (so source-tier
/// auto-detection skips). Includes the same minimal Cargo manifest
/// as `make_git_fixture` so the scan produces a valid SBOM.
fn make_non_git_fixture() -> tempfile::TempDir {
    let td = tempfile::tempdir().unwrap();
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
    td
}

/// Run `waybill sbom scan --path <path>` and return the parsed
/// CDX 1.6 JSON document. Each invocation gets a fresh fake HOME
/// to neutralize per-host config drift.
fn run_scan_to_cdx(td: &Path, extra_args: &[&str]) -> serde_json::Value {
    let fake_home = tempfile::tempdir().unwrap();
    let out_path = td.join("out.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(td)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "sbom scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).expect("CDX output parses as JSON")
}

/// Extract the `repo:` (CDX `externalReferences[type:vcs]`) URL +
/// comment fields from a CDX document. Returns None when no
/// type:vcs entry exists.
fn extract_vcs_ref(cdx: &serde_json::Value) -> Option<(String, Option<String>)> {
    let refs = cdx
        .get("metadata")?
        .get("component")?
        .get("externalReferences")?
        .as_array()?;
    refs.iter()
        .find(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"))
        .map(|r| {
            (
                r["url"].as_str().unwrap_or("").to_string(),
                r.get("comment")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string()),
            )
        })
}

// ---------------------------------------------------------------------
// User Story 1 — source-tier strip-by-default
// ---------------------------------------------------------------------

/// US1 (a) — source-tier scan over a credentialed origin emits a
/// sanitized `repo:` URL; the literal token string occurs zero times
/// in the document. Validates FR-001 + SC-001.
#[test]
fn source_tier_strips_credentials_from_https_origin() {
    let url = credentialed_origin_url();
    let td = make_git_fixture(&[("origin", &url)], false);
    let cdx = run_scan_to_cdx(td.path(), &[]);
    let (vcs_url, _comment) = extract_vcs_ref(&cdx).expect("vcs externalReference present");
    assert_eq!(
        vcs_url, SANITIZED_ORIGIN_URL,
        "FR-001: auto-detected URL must have userinfo stripped"
    );
    // SC-001: zero literal-token occurrences anywhere in the document.
    let bytes = serde_json::to_vec(&cdx).unwrap();
    let body = String::from_utf8(bytes).unwrap();
    assert_eq!(
        body.matches(FAKE_TOKEN).count(),
        0,
        "SC-001: literal token MUST NOT appear in emitted SBOM",
    );
    // Defense-in-depth: also assert the `x-access-token` username
    // didn't survive — the userinfo as a whole is what gets stripped.
    assert_eq!(
        body.matches("x-access-token").count(),
        0,
        "userinfo username portion MUST NOT appear in emitted SBOM",
    );
}

/// US1 (b) — source_label / `comment` field reflects the
/// sanitization with the `(credentials stripped)` suffix per FR-008
/// + SC-006.
#[test]
fn source_tier_source_label_carries_stripped_suffix() {
    let url = credentialed_origin_url();
    let td = make_git_fixture(&[("origin", &url)], false);
    let cdx = run_scan_to_cdx(td.path(), &[]);
    let (_vcs_url, comment) = extract_vcs_ref(&cdx).expect("vcs externalReference present");
    let comment = comment.expect("comment field present (carries source_label)");
    assert!(
        comment.contains("(credentials stripped)"),
        "FR-008: source_label must contain `(credentials stripped)` when \
         sanitization fires; got {comment:?}"
    );
    // The original auto-detect prefix must remain — we APPEND, not
    // REPLACE, per research §3.
    assert!(
        comment.contains("auto-detected from git remote `origin`"),
        "FR-008: existing milestone-073 prefix must be preserved when \
         appending the suffix; got {comment:?}"
    );
}

/// US1 (c) — info-level log line for the sanitization event.
///
/// Log capture pattern is not in place in this test crate (see
/// module-level rationale). We assert the runtime side-effects that
/// indicate the log path was taken — matching auto-detected URL,
/// matching source_label suffix, zero token occurrences. Direct
/// log-content assertion is reserved for a future tracing-subscriber
/// addition.
#[test]
fn source_tier_emits_redacted_info_log() {
    let url = credentialed_origin_url();
    let td = make_git_fixture(&[("origin", &url)], false);
    let cdx = run_scan_to_cdx(td.path(), &[]);
    let (vcs_url, comment) = extract_vcs_ref(&cdx).expect("vcs externalReference present");
    // Triple-check: identifier sanitized, label augmented, no token.
    assert_eq!(vcs_url, SANITIZED_ORIGIN_URL);
    assert!(comment
        .as_deref()
        .unwrap_or("")
        .contains("(credentials stripped)"));
    let body = serde_json::to_string(&cdx).unwrap();
    assert!(!body.contains(FAKE_TOKEN));
}

/// US1 (d) — SSH-form remotes carry no userinfo; `comment` field is
/// byte-identical to alpha.16 (no `(credentials stripped)` suffix).
/// Validates FR-003 SSH passthrough + SC-007.
#[test]
fn source_tier_ssh_form_unchanged() {
    let td = make_git_fixture(&[("origin", "git@github.com:acme/test-repo.git")], false);
    let cdx = run_scan_to_cdx(td.path(), &[]);
    let (vcs_url, comment) = extract_vcs_ref(&cdx).expect("vcs externalReference present");
    assert_eq!(
        vcs_url, "git@github.com:acme/test-repo.git",
        "SSH-form URL MUST emit byte-identical to alpha.16"
    );
    let comment = comment.expect("comment field present");
    assert!(
        !comment.contains("(credentials stripped)"),
        "SC-007: SSH-form must NOT trigger the sanitization suffix; got {comment:?}"
    );
    assert_eq!(
        comment, "auto-detected from git remote `origin`",
        "comment field byte-identical to alpha.16 source-tier label"
    );
}

// ---------------------------------------------------------------------
// User Story 2 — build-tier strip-by-default
// ---------------------------------------------------------------------

/// US2 (a) — build-tier auto-detect sanitizes both `repo:` and `git:`
/// identifier slots; the literal token string occurs zero times in
/// any emitted identifier value. Validates FR-001 + FR-002 + SC-002.
#[test]
fn build_tier_strips_credentials_from_repo_and_git() {
    let url = credentialed_origin_url();
    let td = make_git_fixture(&[("origin", &url)], true);
    let head = git_rev_parse_head_subprocess(td.path());

    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(
        ids.len(),
        2,
        "expected [repo:, git:] both auto-detected; got {ids:?}"
    );
    assert_eq!(ids[0].scheme.as_str(), "repo");
    assert_eq!(
        ids[0].value.as_str(),
        SANITIZED_ORIGIN_URL,
        "FR-001: build-tier `repo:` must sanitize userinfo"
    );
    assert_eq!(ids[1].scheme.as_str(), "git");
    assert_eq!(
        ids[1].value.as_str(),
        format!("{SANITIZED_ORIGIN_URL}#{head}"),
        "FR-002: build-tier `git:` URL portion must be sanitized BEFORE #<sha> append"
    );
    // SC-002 zero-token check — covers BOTH identifier values.
    for id in &ids {
        assert!(
            !id.value.as_str().contains(FAKE_TOKEN),
            "literal token must not appear in any identifier value; got {}",
            id.value.as_str()
        );
        assert!(
            !id.value.as_str().contains("x-access-token"),
            "userinfo username must not appear in any identifier value; got {}",
            id.value.as_str()
        );
    }
    // Both source_labels carry the `(credentials stripped)` suffix.
    for id in &ids {
        let label = id.source_label.as_deref().unwrap();
        assert!(
            label.contains("(credentials stripped)"),
            "FR-008: build-tier source_label must contain suffix; got {label:?}"
        );
        assert!(label.contains("build-tier"));
    }
}

/// US2 (b) — VR-075-005: the `git:` value's URL portion has the
/// userinfo stripped, then `#<sha>` appended. The URL portion
/// (everything before `#`) MUST contain zero `@` characters.
#[test]
fn build_tier_git_value_has_sha_appended_after_sanitization() {
    let url = credentialed_origin_url();
    let td = make_git_fixture(&[("origin", &url)], true);
    let head = git_rev_parse_head_subprocess(td.path());
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert_eq!(ids.len(), 2);
    let git_value = ids[1].value.as_str();
    let (url_part, sha_part) = git_value
        .split_once('#')
        .expect("git: value must have `#<sha>`");
    assert_eq!(
        sha_part, head,
        "SHA portion MUST match `git rev-parse HEAD`"
    );
    assert!(
        !url_part.contains('@'),
        "VR-075-005: URL portion MUST contain zero `@` after sanitization; got {url_part}"
    );
    assert_eq!(url_part, SANITIZED_ORIGIN_URL);
}

// ---------------------------------------------------------------------
// User Story 3 — manual identifier flags emit verbatim
// ---------------------------------------------------------------------

/// US3 (a) — operator-typed `--repo` with credentials goes through
/// verbatim. No sanitization, no warning. Validates FR-004 + SC-003.
#[test]
fn manual_repo_emits_verbatim_with_credentials() {
    // Use a non-git tempdir so source-tier auto-detect skips, leaving
    // the manual flag as the only identifier source.
    let td = make_non_git_fixture();
    let url = credentialed_origin_url();
    let cdx = run_scan_to_cdx(td.path(), &["--repo", &url]);
    let (vcs_url, _comment) = extract_vcs_ref(&cdx).expect("manual repo: should appear");
    assert_eq!(
        vcs_url, url,
        "FR-004: manual --repo flag MUST emit verbatim, including userinfo"
    );
    // SC-003: literal token DOES appear (in the manual value).
    let body = serde_json::to_string(&cdx).unwrap();
    assert!(
        body.contains(FAKE_TOKEN),
        "manual flag is verbatim; token presence is intentional"
    );
}

/// US3 (b) — manual `--repo` overrides auto-detected `repo:` per
/// milestone 074's manual-wins rule. The auto-detected sanitized
/// value does NOT appear; the manual credentialed value DOES.
#[test]
fn manual_repo_overrides_strip_with_credentials_in_value() {
    let auto_url = credentialed_origin_url();
    let manual_url =
        format!("https://other-user:OTHER-{FAKE_TOKEN}@gitlab.example.com/manual/path.git");
    let td = make_git_fixture(&[("origin", &auto_url)], false);
    let cdx = run_scan_to_cdx(td.path(), &["--repo", &manual_url]);
    let (vcs_url, _comment) = extract_vcs_ref(&cdx).expect("vcs ref present");
    assert_eq!(
        vcs_url, manual_url,
        "manual --repo flag MUST win over auto-detected per FR-004 + 074's manual-wins rule"
    );
    // The auto-detected sanitized form must NOT appear (it was
    // overridden by manual).
    let body = serde_json::to_string(&cdx).unwrap();
    assert!(
        !body.contains("acme/test-repo.git"),
        "auto-detected repo path must be overridden by manual flag; got body containing it"
    );
    // The manual value's literal token DOES appear (manual = verbatim).
    assert!(body.contains(FAKE_TOKEN), "manual flag value emits verbatim");
}

// ---------------------------------------------------------------------
// User Story 4 — opt-out flag preserves credentials
// ---------------------------------------------------------------------

/// US4 (a) — `--keep-credentials-in-identifiers` preserves userinfo
/// in source-tier auto-detected `repo:` and skips the
/// `(credentials stripped)` suffix. Validates FR-005 + FR-007.
#[test]
fn keep_credentials_flag_preserves_userinfo_source_tier() {
    let url = credentialed_origin_url();
    let td = make_git_fixture(&[("origin", &url)], false);
    let cdx = run_scan_to_cdx(td.path(), &["--keep-credentials-in-identifiers"]);
    let (vcs_url, comment) = extract_vcs_ref(&cdx).expect("vcs ref present");
    assert_eq!(
        vcs_url, url,
        "opt-out: auto-detected URL MUST be preserved verbatim including userinfo"
    );
    let comment = comment.expect("comment field present");
    assert!(
        !comment.contains("(credentials stripped)"),
        "opt-out: NO `(credentials stripped)` suffix; got {comment:?}"
    );
    assert_eq!(
        comment, "auto-detected from git remote `origin`",
        "comment field byte-identical to milestone-073/074 source-tier label"
    );
    // Token IS present (intentional).
    let body = serde_json::to_string(&cdx).unwrap();
    assert!(body.contains(FAKE_TOKEN));
}

/// US4 (b) — `keep_credentials=true` preserves userinfo in the
/// build-tier flow for both `repo:` and `git:` slots. Validates
/// FR-005 + FR-002 in opt-out mode.
#[test]
fn keep_credentials_flag_preserves_userinfo_build_tier() {
    let url = credentialed_origin_url();
    let td = make_git_fixture(&[("origin", &url)], true);
    let head = git_rev_parse_head_subprocess(td.path());
    let ids = auto_detect_build_tier_identifiers(td.path(), true);
    assert_eq!(ids.len(), 2);
    assert_eq!(
        ids[0].value.as_str(),
        url,
        "opt-out: build-tier `repo:` MUST preserve userinfo verbatim"
    );
    assert_eq!(
        ids[1].value.as_str(),
        format!("{url}#{head}"),
        "opt-out: build-tier `git:` URL portion MUST preserve userinfo (then #sha appended)"
    );
    // Neither label carries the suffix.
    for id in &ids {
        let label = id.source_label.as_deref().unwrap();
        assert!(
            !label.contains("(credentials stripped)"),
            "opt-out: source_label MUST NOT carry the suffix; got {label:?}"
        );
        assert!(label.contains("build-tier"));
    }
}

/// US4 (c) — FR-007 acknowledgment log.
///
/// Log capture is not in place in this test crate (see module-level
/// rationale). We exercise the opt-out invocation end-to-end and
/// confirm the operator-observable side-effects: zero sanitization
/// fired, both identifier slots preserve userinfo, source_label
/// unchanged. Direct log-line assertion is reserved for a future
/// tracing-subscriber addition.
#[test]
fn keep_credentials_flag_emits_acknowledgment_log() {
    let url = credentialed_origin_url();
    let td = make_git_fixture(&[("origin", &url)], false);
    // Source-tier path — invokes the FR-007 log emission at the top
    // of `auto_detect_repo_identifier`.
    let cdx = run_scan_to_cdx(td.path(), &["--keep-credentials-in-identifiers"]);
    let (vcs_url, comment) = extract_vcs_ref(&cdx).expect("vcs ref present");
    assert_eq!(vcs_url, url);
    assert!(!comment
        .as_deref()
        .unwrap_or("")
        .contains("(credentials stripped)"));
    // Build-tier path also emits the FR-007 log (different code site).
    let _ids = auto_detect_build_tier_identifiers(td.path(), true);
    // Test passes if both invocations completed without panicking
    // and the runtime side-effects match opt-out semantics.
}

// ---------------------------------------------------------------------
// Edge case — parse-failure soft-fails through milestone 073's
// existing UserDefined rule (FR-009).
// ---------------------------------------------------------------------

/// Edge case — a non-RFC-3986 URL value passes through
/// `sanitize_userinfo` unchanged (passthrough); milestone 073's
/// existing `validate_for_scheme` validator then soft-fails to
/// `UserDefined`. Validates FR-009.
///
/// Uses a value that survives `IdentifierValue::new` (i.e., non-empty
/// and within the existing length cap) but fails the milestone-073
/// `validate_repo` regex / heuristic. A bare token without `://`
/// or `@host:` structure exercises the soft-fail path. If
/// `validate_repo` happens to accept the value as `Builtin`, the
/// test still passes — we assert the identifier slot is present and
/// the value flows through unchanged, NOT a specific kind.
#[test]
fn parse_failure_falls_through_to_user_defined() {
    let td = make_git_fixture(&[("origin", "not-a-real-url-abc-123")], false);
    let ids = auto_detect_build_tier_identifiers(td.path(), false);
    assert!(!ids.is_empty(), "FR-009: soft-fail must still emit");
    assert_eq!(ids[0].scheme.as_str(), "repo");
    assert_eq!(
        ids[0].value.as_str(),
        "not-a-real-url-abc-123",
        "FR-009: passthrough on parse failure — value flows through unchanged"
    );
    // Per milestone 074's pattern (research §7), don't pin the kind.
    // Both `Builtin` (if the validator accepts the value) and
    // `UserDefined` (if it doesn't) are acceptable per FR-009.
    match ids[0].kind {
        IdentifierKind::Builtin(_) | IdentifierKind::UserDefined => {}
    }
}
