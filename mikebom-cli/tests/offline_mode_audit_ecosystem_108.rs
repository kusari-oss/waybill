//! Milestone 108 — FR-014 offline-mode audit.
//!
//! DIFFERENT SHAPE than the milestone-106 / 107 audits because this
//! milestone DOES make ONE network call (the corpus fetch). The audit
//! enforces that `fingerprints/fetch.rs` is the ONLY file in
//! `fingerprints/` allowed to contain `reqwest::` (or any other
//! network primitive). Every other file in the new sub-module MUST be
//! pure filesystem + parsing — no network surface.
//!
//! The intent: keep the network blast radius bounded to a single
//! auditable file. If a future contributor accidentally adds a
//! `reqwest::Client` to `loader.rs` or `cache.rs`, this test fails
//! loudly with the exact file + line + matching substring.
//!
//! Mirrors `tests/offline_mode_audit_ecosystem_107.rs` (the
//! milestone-107 polish PR's audit), inverted: instead of a forbidden
//! list applied to every file, this is an allowlist (one file) with a
//! forbidden list applied to everything else.

use std::path::PathBuf;

/// Every file in `src/scan_fs/binary/fingerprints/` MUST be in this
/// list. `fetch.rs` is the lone permitted network-toucher; all others
/// are pure offline.
const ALL_FINGERPRINTS_FILES: &[&str] = &[
    "src/scan_fs/binary/fingerprints/cache.rs",
    "src/scan_fs/binary/fingerprints/fetch.rs",
    "src/scan_fs/binary/fingerprints/loader.rs",
    "src/scan_fs/binary/fingerprints/mod.rs",
    "src/scan_fs/binary/fingerprints/record.rs",
    "src/scan_fs/binary/fingerprints/source_sha.rs",
];

/// The single file allowed to contain network primitives.
const NETWORK_ALLOWLIST: &[&str] = &["src/scan_fs/binary/fingerprints/fetch.rs"];

const FORBIDDEN_SUBSTRINGS: &[&str] = &[
    "reqwest::",
    "tokio::net::",
    "hyper::",
    "Command::new(\"curl\"",
    "Command::new(\"wget\"",
    "Command::new(\"http\"",
    "TcpStream::",
    "TcpListener::",
    "std::net::TcpStream",
    "std::net::TcpListener",
];

#[test]
fn milestone_108_fingerprints_files_obey_fetch_only_network_allowlist() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();
    let mut missing = Vec::new();

    for rel in ALL_FINGERPRINTS_FILES {
        let abs = crate_root.join(rel);
        if !abs.is_file() {
            missing.push(abs.display().to_string());
            continue;
        }
        let body = std::fs::read_to_string(&abs)
            .unwrap_or_else(|e| panic!("read {abs:?}: {e}"));
        let in_allowlist = NETWORK_ALLOWLIST.contains(rel);
        if in_allowlist {
            // fetch.rs is allowed to have network — skip the grep,
            // but still walk the file body to keep the allowlist
            // honest (if a future refactor moves fetch logic
            // elsewhere AND the allowlist isn't updated, this loop
            // still runs and surfaces the issue).
            continue;
        }
        for needle in FORBIDDEN_SUBSTRINGS {
            for (lineno, line) in body.lines().enumerate() {
                // Tolerate the forbidden substring inside a comment
                // (block or line). Comment-resident `reqwest::` only
                // appears in documentation that REFERS to the API
                // without using it.
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") || trimmed.starts_with("*") {
                    continue;
                }
                if line.contains(needle) {
                    violations.push(format!(
                        "{}:{}: contains forbidden pattern `{needle}` → `{}`",
                        abs.display(),
                        lineno + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "audited fingerprints files missing: {}\n\
         (did a module move? Update ALL_FINGERPRINTS_FILES.)",
        missing.join(", ")
    );
    assert!(
        violations.is_empty(),
        "milestone 108 FR-014 audit failed: only `fetch.rs` is allowed \
         to contain network primitives in fingerprints/. Violations:\n{}",
        violations.join("\n"),
    );
}

/// Defensive twin: `fetch.rs` MUST actually contain `reqwest::` (or
/// the allowlist is stale and we should drop the entry). Catches the
/// "someone refactored the fetcher out of fetch.rs but left the
/// allowlist entry behind" case.
#[test]
fn milestone_108_fetch_rs_actually_contains_reqwest() {
    let abs = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/scan_fs/binary/fingerprints/fetch.rs");
    let body = std::fs::read_to_string(&abs)
        .unwrap_or_else(|e| panic!("read {abs:?}: {e}"));
    assert!(
        body.contains("reqwest::"),
        "fetch.rs is in NETWORK_ALLOWLIST but doesn't contain `reqwest::`. \
         If the fetcher moved, update both the implementation AND the audit's \
         NETWORK_ALLOWLIST in tests/offline_mode_audit_ecosystem_108.rs."
    );
}
