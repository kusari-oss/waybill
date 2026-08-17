//! Milestone 236 — cross-reader `waybill:unresolved-reason` verification.
//!
//! Verifies the wire contract locked in
//! `specs/236-unresolved-reason/contracts/per-reader-strings.md`:
//!
//! - **SC-001** every design-tier-emitting reader ships its
//!   locked-string annotation (verified structurally by greping
//!   the shipped reader source for the locked string).
//! - **FR-010** no reason string contains PII / paths / credentials.
//!
//! Per-reader emission is separately verified by the per-reader
//! inline unit tests in each reader's `mod tests`
//! (`m236_<reader>_design_tier_carries_unresolved_reason` — 10 total).
//! This file provides cross-reader consistency + FR-010 guarantees.

use std::path::PathBuf;

/// Locked per-reader contract from
/// `specs/236-unresolved-reason/contracts/per-reader-strings.md`.
///
/// Each entry: `(reader_source_path, expected_reason_string)`.
///
/// NuGet is included as the FR-006 regression guard — its wire value
/// must remain byte-identical to the PR-#656 shipment.
fn locked_reason_strings() -> Vec<(&'static str, &'static str)> {
    vec![
        // NuGet regression guard (FR-006 / SC-003).
        (
            "waybill-cli/src/scan_fs/package_db/nuget/mod.rs",
            "no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry",
        ),
        // US1 (PR #703).
        (
            "waybill-cli/src/scan_fs/package_db/maven.rs",
            "no <version> in pom.xml; no dependency-reduced-pom.xml or effective-pom fallback",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/npm/walk.rs",
            "workspace member; no lockfile-resolved version",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs",
            "no version specifier in requirements.txt; no uv.lock / poetry.lock fallback",
        ),
        // US2 (PR #704).
        (
            "waybill-cli/src/scan_fs/package_db/kotlin_dsl/build_script.rs",
            "Kotlin DSL buildscript declaration; --include-declared-deps enables emission",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/scala.rs",
            "declared in build.sbt; no coursier-resolved lockfile",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/gradle/static_parser.rs",
            "declared in build.gradle; US2 cache reader had no matching seed",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/helm.rs",
            "unrendered Chart.yaml dependency; --helm-render subprocess disabled or unavailable",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/yocto/recipe.rs",
            "recipe .bb declaration; no PV/PR resolution",
        ),
        // US3 (PR #705).
        (
            "waybill-cli/src/scan_fs/package_db/cocoapods.rs",
            "no matching entry in Podfile.lock",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/composer.rs",
            "no matching entry in composer.lock",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/dart.rs",
            "no matching entry in pubspec.lock",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/elixir.rs",
            "no matching entry in mix.lock",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/erlang.rs",
            "no matching entry in rebar.lock",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/haskell.rs",
            "declared in stack.yaml / .cabal; no stack.yaml.lock fallback",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/pants_shell/component_emit.rs",
            "pants shell tool pin without version specifier",
        ),
        (
            "waybill-cli/src/scan_fs/package_db/pants_go/mod.rs",
            "pants_go expected_version declared; no matching go corpus component",
        ),
    ]
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir has parent")
        .to_path_buf()
}

/// SC-001 (structural): every locked reader source contains its
/// locked reason string. Fails loudly if a reader is refactored
/// without updating this contract file.
#[test]
fn sc001_every_reader_ships_locked_reason_string() {
    let root = repo_root();
    let mut missing: Vec<String> = Vec::new();
    for (path_rel, expected) in locked_reason_strings() {
        let path = root.join(path_rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        if !src.contains(expected) {
            missing.push(format!(
                "  {path_rel}\n    expected: {expected:?}",
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "The following readers do not ship their locked m236 reason string:\n{}\nSee specs/236-unresolved-reason/contracts/per-reader-strings.md for the wire contract.",
        missing.join("\n"),
    );
}

/// FR-010: reason strings must not contain PII, paths, hostnames,
/// or credential-shaped substrings. Failure prints the offending
/// reader + substring so the operator can trace the leak.
#[test]
fn fr010_reason_strings_no_pii_paths_credentials() {
    // Substrings that would indicate a leak. Kept intentionally
    // narrow — the m236 wire contract only ships ASCII English
    // human-readable phrases.
    let blacklist: &[&str] = &[
        "/home/",
        "/Users/",
        "/root/",
        "C:\\",
        "%USERPROFILE%",
        "password=",
        "token=",
        "api_key=",
        "Bearer ",
        "192.168.",
        "10.0.",
        "@gmail.com",
        "@example.com",
        "@waybill.dev",
    ];
    let mut leaks: Vec<String> = Vec::new();
    for (path_rel, reason) in locked_reason_strings() {
        for bad in blacklist {
            if reason.contains(bad) {
                leaks.push(format!("  {path_rel}\n    contains: {bad:?}\n    in reason: {reason:?}"));
            }
        }
    }
    assert!(
        leaks.is_empty(),
        "m236 reason strings leaked PII / paths / credentials:\n{}",
        leaks.join("\n"),
    );
}

/// FR-002: reason strings must be human-readable + boundary-naming.
/// Enforced as: non-empty + <200 chars + ASCII-only. Positive
/// human-readability is verified at code-review time against the
/// locked contract file.
#[test]
fn fr002_reason_strings_are_ascii_bounded_length() {
    for (path_rel, reason) in locked_reason_strings() {
        assert!(
            !reason.is_empty(),
            "{path_rel}: reason string is empty",
        );
        assert!(
            reason.len() < 200,
            "{path_rel}: reason string is {} chars, over 200-char limit",
            reason.len(),
        );
        assert!(
            reason.is_ascii(),
            "{path_rel}: reason string contains non-ASCII",
        );
    }
}

/// Contract enumeration test — asserts the shipped inventory count
/// matches the spec's Q2 clarification: **17 covered reader files** =
/// NuGet regression guard (1) + 3 US1 + 5 US2 + 8 US3 = 17.
#[test]
fn m236_scope_matches_q2_clarification() {
    let entries = locked_reason_strings();
    assert_eq!(
        entries.len(),
        17,
        "m236 covers 17 reader source files (NuGet regression guard + 3 US1 + 5 US2 + 8 US3). \
         If this count changes, the spec Q2 clarification is stale — update contracts/per-reader-strings.md.",
    );
}
