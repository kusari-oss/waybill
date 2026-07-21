//! Milestone 109 — FR-011 offline-mode audit.
//!
//! All files in `src/scan_fs/binary/source_binding/` MUST be pure
//! filesystem + in-memory parsing. Unlike milestone 108 (which makes
//! ONE legitimate network call from `fingerprints/fetch.rs`),
//! milestone 109's attribution layer reads ONLY local filesystem
//! state — there's nothing to fetch.
//!
//! Mirrors `tests/offline_mode_audit_ecosystem_107.rs` (no-allowlist
//! variant of the milestone-108 audit): every file is forbidden from
//! containing network or subprocess primitives.

use std::path::PathBuf;

const READER_FILES: &[&str] = &[
    "src/scan_fs/binary/source_binding/mod.rs",
    "src/scan_fs/binary/source_binding/cmake_observer.rs",
    "src/scan_fs/binary/source_binding/registry.rs",
];

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
fn milestone_109_source_binding_files_make_no_network_or_subprocess_calls() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();
    for rel in READER_FILES {
        let abs = crate_root.join(rel);
        assert!(
            abs.is_file(),
            "audited source-binding file missing: {} (did a module move? \
             Update READER_FILES in tests/offline_mode_audit_ecosystem_109.rs.)",
            abs.display()
        );
        let body = std::fs::read_to_string(&abs)
            .unwrap_or_else(|e| panic!("read {abs:?}: {e}"));
        for needle in FORBIDDEN_SUBSTRINGS {
            for (lineno, line) in body.lines().enumerate() {
                // Tolerate the forbidden substring inside a comment
                // (doc-comments that REFER to the API without using it
                // are fine). Mirrors milestone-108's audit policy.
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
        violations.is_empty(),
        "milestone 109 source-binding files MUST be offline-only (FR-011). \
         The attribution layer is pure filesystem + parsing — there is no \
         network primitive that legitimately belongs in any of these files. \
         Violations:\n{}",
        violations.join("\n"),
    );
}
