//! Milestone 106 — FR-012 offline-mode audit.
//!
//! Build-time test that greps the new reader modules added by
//! milestone 106 (uv, Bun, Gradle, NuGet) for accidental network or
//! subprocess access. The four readers MUST be pure filesystem +
//! manifest parsing — no `reqwest::`, no `tokio::net::`, no
//! `hyper::`, no `Command::new("curl"`/`"wget"`/`"http"`. Online
//! enrichment for `pkg:pypi/`, `pkg:npm/`, `pkg:maven/`, `pkg:nuget/`
//! happens in the existing deps.dev / ClearlyDefined enrichment
//! passes, which are gated by the global `--offline` flag — never in
//! the per-ecosystem readers themselves.
//!
//! Independent of the implementations' own claims: this test reads the
//! raw source files at test time and fails the build if any of the
//! tripwire patterns appear.

use std::path::PathBuf;

const READER_FILES: &[&str] = &[
    "src/scan_fs/package_db/pip/uv_lock.rs",
    "src/scan_fs/package_db/npm/bun_lock.rs",
    "src/scan_fs/package_db/npm/jsonc.rs",
    "src/scan_fs/package_db/gradle/mod.rs",
    "src/scan_fs/package_db/gradle/lockfile.rs",
    "src/scan_fs/package_db/nuget/mod.rs",
    "src/scan_fs/package_db/nuget/csproj.rs",
    "src/scan_fs/package_db/nuget/directory_packages_props.rs",
    "src/scan_fs/package_db/nuget/private_assets.rs",
    "src/scan_fs/package_db/nuget/packages_lock.rs",
    "src/scan_fs/package_db/workspace.rs",
];

/// Tripwire substrings indicating a network or subprocess call. The
/// strings are deliberately broad — any false positive can be fixed
/// by either renaming an identifier or by adding the file to an
/// allowlist (none today).
const FORBIDDEN_SUBSTRINGS: &[&str] = &[
    "reqwest::",
    "tokio::net::",
    "hyper::",
    "Command::new(\"curl\"",
    "Command::new(\"wget\"",
    "Command::new(\"http\"",
    // Catch anything that creates a TcpStream / TcpListener directly.
    "TcpStream::",
    "TcpListener::",
    "std::net::TcpStream",
    "std::net::TcpListener",
];

#[test]
fn milestone_106_readers_make_no_network_or_subprocess_calls() {
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
        "milestone 106 readers MUST be offline-only (FR-012). Violations:\n{}",
        violations.join("\n"),
    );
}
