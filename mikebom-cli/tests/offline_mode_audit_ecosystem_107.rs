//! Milestone 107 — FR-011 offline-mode audit.
//!
//! Build-time test that greps the new reader modules added by
//! milestone 107 (opkg, yocto/{context, manifest, recipe}) plus the
//! foundational `control_file.rs` refactor for accidental network or
//! subprocess access. The four readers MUST be pure filesystem +
//! manifest parsing — no `reqwest::`, no `tokio::net::`, no
//! `hyper::`, no `Command::new("curl"`/`"wget"`/`"http"`.
//!
//! Mirrors `tests/offline_mode_audit_ecosystem_106.rs` (the
//! milestone-106 polish PR's audit), extended to cover the
//! milestone-107 reader set.

use std::path::PathBuf;

const READER_FILES: &[&str] = &[
    "src/scan_fs/package_db/control_file.rs",
    "src/scan_fs/package_db/opkg.rs",
    "src/scan_fs/package_db/yocto/mod.rs",
    "src/scan_fs/package_db/yocto/context.rs",
    "src/scan_fs/package_db/yocto/manifest.rs",
    "src/scan_fs/package_db/yocto/recipe.rs",
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
fn milestone_107_readers_make_no_network_or_subprocess_calls() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();
    for rel in READER_FILES {
        let abs = crate_root.join(rel);
        assert!(
            abs.is_file(),
            "audited reader file missing: {} (did a module move? Update READER_FILES.)",
            abs.display()
        );
        let body = std::fs::read_to_string(&abs)
            .unwrap_or_else(|e| panic!("read {abs:?}: {e}"));
        for needle in FORBIDDEN_SUBSTRINGS {
            for (lineno, line) in body.lines().enumerate() {
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
        "milestone 107 readers MUST be offline-only (FR-011). Violations:\n{}",
        violations.join("\n"),
    );
}
