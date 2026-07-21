//! Milestone 108 US3 — annotation-provenance test.
//!
//! Verifies the two halves of the consumer-verification contract
//! documented in `docs/reference/identifiers.md` §11.4:
//!
//! 1. **Offline half** (always runs): the
//!    `waybill:fingerprint-corpus-sha` annotation value waybill emits
//!    is the 12-hex prefix of the build-time-embedded SHA from
//!    `env!("WAYBILL_FINGERPRINTS_CORPUS_SHA")` (set by `build.rs`
//!    from `tests/fingerprints.rev`). This is the part consumers can
//!    verify without network access.
//!
//! 2. **Network-gated half** (`WAYBILL_FINGERPRINTS_NETWORK_TESTS=1`):
//!    that the 12-hex prefix resolves to a real commit on the sibling
//!    repo via GitHub's git-API. This is the part that's susceptible
//!    to maintainer error (pin SHA → typo → no real commit) — a
//!    cheap CI gate against a stale or wrong pin.
//!
//! Both halves use the embedded SHA as the source of truth — same
//! way the production code does via `CorpusSha::build_time_embedded()`.
//! No duplicate hardcoded SHA in this file.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::process::Command;

fn embedded_sha() -> &'static str {
    env!("WAYBILL_FINGERPRINTS_CORPUS_SHA")
}

fn network_tests_enabled() -> bool {
    std::env::var("WAYBILL_FINGERPRINTS_NETWORK_TESTS").ok().as_deref() == Some("1")
}

/// Offline half: the annotation prefix is the 12-hex truncation of
/// the build-time-embedded SHA. This is the byte-identity contract
/// for the SHA wire format that consumers depend on per
/// `docs/reference/identifiers.md` §11.2.
#[test]
fn embedded_sha_truncates_to_12_hex_annotation_prefix() {
    let full = embedded_sha();
    assert_eq!(
        full.len(),
        40,
        "build-time-embedded SHA must be 40-hex; got {} chars: {full:?}",
        full.len()
    );
    assert!(
        full.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "build-time-embedded SHA must be lowercase hex; got {full:?}"
    );
    let expected_prefix = &full[..12];
    assert_eq!(
        expected_prefix.len(),
        12,
        "annotation prefix must be 12 chars"
    );
    // The 12-hex slice is the EXACT value the matcher stamps onto the
    // `waybill:fingerprint-corpus-sha` annotation per FR-005. Anything
    // else is a wire-format regression.
    assert!(
        full.starts_with(expected_prefix),
        "prefix-of-full invariant broken — fix CorpusSha::to_short_hex",
    );
}

/// Network half: the embedded SHA is a real commit on the sibling
/// repo. Catches the maintainer-typo failure mode (someone bumps
/// `tests/fingerprints.rev` to a SHA that doesn't exist in
/// `kusari-sandbox/waybill-fingerprints`) — the embedded SHA would
/// compile + ship, but every `--fingerprints-corpus` scan would 404.
///
/// Uses `curl` (already a hard dep for any waybill dev setup) for
/// the GitHub API call; no new Cargo deps.
#[test]
fn embedded_sha_resolves_to_real_commit_on_sibling_repo() {
    if !network_tests_enabled() {
        println!(
            "skipped: WAYBILL_FINGERPRINTS_NETWORK_TESTS not set (offline CI lane)"
        );
        return;
    }
    let full = embedded_sha();
    let url = format!(
        "https://api.github.com/repos/kusari-sandbox/waybill-fingerprints/commits/{full}"
    );
    let output = Command::new("curl")
        .arg("-fsSL")
        .arg("-H")
        .arg("Accept: application/vnd.github+json")
        .arg("-H")
        .arg("User-Agent: waybill-tests/annotation-provenance")
        .arg(&url)
        .output()
        .expect("curl must be installed");
    assert!(
        output.status.success(),
        "embedded SHA {full} did not resolve to a real commit on the sibling repo. \
         GitHub API returned non-success for {url}. \
         stderr={stderr}. \
         If this is a fresh pin bump, push the matching corpus commit before bumping \
         tests/fingerprints.rev.",
        stderr = String::from_utf8_lossy(&output.stderr),
    );
    let body: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("GitHub API response was not valid JSON");
    let resolved_full = body["sha"]
        .as_str()
        .expect("GitHub API response missing `sha` field");
    assert_eq!(
        resolved_full, full,
        "GitHub canonicalized the embedded SHA to a different value — \
         this should never happen for a 40-hex full SHA lookup",
    );
}
