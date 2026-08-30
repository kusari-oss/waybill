// milestone 669 - see specs/669-bench-harness/plan.md
// Fixture × mode enumeration.
//
// T017: enumerate() reads the manifest and iterates fixtures × their
// supported_modes, applying an optional filter (multiple globs = union).
// T018: unit tests over enumerate() semantics.
//
// Glob support is intentionally hand-rolled (no globset dep) — plan.md
// pins exactly one new xtask dep (sysinfo). The '*' wildcard is the only
// meta-char in the anticipated filter patterns (`cargo-*`, `*-medium`,
// `cargo-workspace-medium`, `*`) per contract xtask-bench-cli.md C-2.

use std::error::Error;
use std::path::Path;

use crate::bench::schema::{Fixture, Mode};

/// Enumerate the `(fixture, mode)` product from the manifest at
/// `manifest_path`. If `filter` is `Some`, keep only fixtures whose
/// `name` matches at least one of the glob patterns (union). Empty
/// match sets are legal — the caller decides whether that's an error.
///
/// Per contract xtask-bench-cli.md C-1 (default matrix runs all
/// fixtures × all modes) and C-2 (filter short-circuits + empty is
/// not an error).
pub fn enumerate(
    manifest_path: &Path,
    filter: Option<&Vec<String>>,
) -> Result<Vec<(Fixture, Mode)>, Box<dyn Error>> {
    let fixtures = Fixture::all_from_manifest(manifest_path)?;
    let mut out: Vec<(Fixture, Mode)> = Vec::new();
    for fixture in fixtures {
        if !matches_any_filter(filter, &fixture.name) {
            continue;
        }
        for mode in &fixture.supported_modes {
            out.push((fixture.clone(), *mode));
        }
    }
    Ok(out)
}

/// Returns `true` iff `filter` is `None` (no filter → keep-all) or the
/// name matches at least one pattern in the filter list (union).
fn matches_any_filter(filter: Option<&Vec<String>>, name: &str) -> bool {
    match filter {
        None => true,
        Some(patterns) => patterns.iter().any(|p| matches_glob(p, name)),
    }
}

/// Simple '*'-only glob matcher. Semantics:
/// - `*` matches zero-or-more characters (any).
/// - Any other character is literal.
/// - No `?` / `[…]` / escaping (unnecessary for benchmark fixture names).
fn matches_glob(pattern: &str, name: &str) -> bool {
    // Fast path: no wildcard → literal equality.
    if !pattern.contains('*') {
        return pattern == name;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    // First segment must be a prefix (empty string if pattern starts with '*').
    let first = parts[0];
    if !name.starts_with(first) {
        return false;
    }
    let mut cursor = &name[first.len()..];
    // Middle segments must appear in order.
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        match cursor.find(part) {
            Some(idx) => cursor = &cursor[idx + part.len()..],
            None => return false,
        }
    }
    // Last segment must be a suffix of what remains.
    let last = parts[parts.len() - 1];
    cursor.ends_with(last)
}

// ────────────────────────────────────────────────────────────────
// T018 — enumerate() + matches_glob unit tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::schema::{FixtureKind, ScanClass};

    fn write_manifest(dir: &Path, json: &str) -> std::path::PathBuf {
        let path = dir.join("manifest.json");
        std::fs::write(&path, json).unwrap();
        path
    }

    /// Fixture #1: two modes.
    /// Fixture #2: three modes.
    /// Fixture #3: one mode.
    /// Cartesian expansion: 2 + 3 + 1 = 6 (fixture, mode) pairs.
    fn three_fixture_manifest_json() -> &'static str {
        r#"{
            "fixtures": [
                {
                    "name": "cargo-workspace-medium",
                    "path": "benchmark/source-tier/cargo-workspace-medium",
                    "kind": "source-tree",
                    "ecosystem": "cargo",
                    "supported_modes": ["default", "no-deep-hash"],
                    "expected_scan_class": "medium"
                },
                {
                    "name": "npm-monorepo-medium",
                    "path": "benchmark/source-tier/npm-monorepo-medium",
                    "kind": "source-tree",
                    "ecosystem": "npm",
                    "supported_modes": ["default", "triple-format", "fingerprints-corpus"],
                    "expected_scan_class": "medium"
                },
                {
                    "name": "debian-slim",
                    "path": "benchmark/container-images/debian-slim.tar",
                    "kind": "container-image",
                    "ecosystem": null,
                    "supported_modes": ["default"],
                    "expected_scan_class": "slow"
                }
            ]
        }"#
    }

    #[test]
    fn enumerate_produces_full_cartesian_product_without_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_manifest(tmp.path(), three_fixture_manifest_json());

        let out = enumerate(&path, None).unwrap();
        assert_eq!(out.len(), 6); // 2 + 3 + 1

        // Order: fixture order preserved from manifest; modes preserved per fixture.
        let names: Vec<&str> = out.iter().map(|(f, _)| f.name.as_str()).collect();
        assert_eq!(names[0], "cargo-workspace-medium");
        assert_eq!(names[1], "cargo-workspace-medium");
        assert_eq!(names[2], "npm-monorepo-medium");
        assert_eq!(names[4], "npm-monorepo-medium");
        assert_eq!(names[5], "debian-slim");
    }

    #[test]
    fn enumerate_single_glob_filter_narrows_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_manifest(tmp.path(), three_fixture_manifest_json());

        let filter = vec!["cargo-*".to_string()];
        let out = enumerate(&path, Some(&filter)).unwrap();
        assert_eq!(out.len(), 2); // cargo has 2 modes
        for (f, _) in &out {
            assert_eq!(f.name, "cargo-workspace-medium");
        }
    }

    #[test]
    fn enumerate_multi_glob_filter_is_union() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_manifest(tmp.path(), three_fixture_manifest_json());

        // Two disjoint patterns → union covers both fixtures.
        let filter = vec!["cargo-*".to_string(), "debian-*".to_string()];
        let out = enumerate(&path, Some(&filter)).unwrap();
        assert_eq!(out.len(), 3); // cargo(2) + debian(1)

        let names: Vec<&str> = out.iter().map(|(f, _)| f.name.as_str()).collect();
        assert!(names.contains(&"cargo-workspace-medium"));
        assert!(names.contains(&"debian-slim"));
        assert!(!names.contains(&"npm-monorepo-medium"));
    }

    #[test]
    fn enumerate_filter_matching_no_fixtures_returns_empty_ok() {
        // Contract C-2: empty match set is valid, NOT an error.
        let tmp = tempfile::tempdir().unwrap();
        let path = write_manifest(tmp.path(), three_fixture_manifest_json());

        let filter = vec!["nonexistent-*".to_string()];
        let out = enumerate(&path, Some(&filter)).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn enumerate_exact_name_filter_matches_that_one() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_manifest(tmp.path(), three_fixture_manifest_json());

        let filter = vec!["debian-slim".to_string()];
        let out = enumerate(&path, Some(&filter)).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0.name, "debian-slim");
        assert_eq!(out[0].1, Mode::Default);
    }

    #[test]
    fn enumerate_star_matches_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_manifest(tmp.path(), three_fixture_manifest_json());

        let filter = vec!["*".to_string()];
        let out = enumerate(&path, Some(&filter)).unwrap();
        assert_eq!(out.len(), 6); // full cartesian product
    }

    #[test]
    fn enumerate_propagates_manifest_load_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("does-not-exist.json");
        assert!(enumerate(&bogus, None).is_err());
    }

    #[test]
    fn enumerate_preserves_fixture_field_values() {
        // Sanity: the tuple's Fixture retains its manifest-derived fields.
        let tmp = tempfile::tempdir().unwrap();
        let path = write_manifest(tmp.path(), three_fixture_manifest_json());

        let filter = vec!["debian-slim".to_string()];
        let out = enumerate(&path, Some(&filter)).unwrap();
        assert_eq!(out[0].0.kind, FixtureKind::ContainerImage);
        assert_eq!(out[0].0.ecosystem, None);
        assert_eq!(out[0].0.expected_scan_class, ScanClass::Slow);
    }

    // ────────────────────────────────────────────────────────────
    // matches_glob() direct tests
    // ────────────────────────────────────────────────────────────

    #[test]
    fn matches_glob_literal() {
        assert!(matches_glob("cargo-workspace-medium", "cargo-workspace-medium"));
        assert!(!matches_glob("cargo-workspace-medium", "cargo-workspace-small"));
    }

    #[test]
    fn matches_glob_prefix_star() {
        assert!(matches_glob("cargo-*", "cargo-workspace-medium"));
        assert!(matches_glob("cargo-*", "cargo-"));
        assert!(!matches_glob("cargo-*", "npm-monorepo-medium"));
    }

    #[test]
    fn matches_glob_suffix_star() {
        assert!(matches_glob("*-medium", "cargo-workspace-medium"));
        assert!(matches_glob("*-medium", "npm-monorepo-medium"));
        assert!(!matches_glob("*-medium", "cargo-workspace-small"));
    }

    #[test]
    fn matches_glob_bracketing_stars() {
        // Middle segment appears in order.
        assert!(matches_glob("cargo-*-medium", "cargo-workspace-medium"));
        assert!(matches_glob("cargo-*-medium", "cargo-mono-medium"));
        assert!(!matches_glob("cargo-*-medium", "cargo-workspace-small"));
        assert!(!matches_glob("cargo-*-medium", "npm-workspace-medium"));
    }

    #[test]
    fn matches_glob_lone_star_matches_everything() {
        assert!(matches_glob("*", ""));
        assert!(matches_glob("*", "cargo"));
        assert!(matches_glob("*", "anything-at-all"));
    }

    #[test]
    fn matches_glob_multiple_middle_segments() {
        assert!(matches_glob("*-a-*-b-*", "x-a-y-b-z"));
        assert!(matches_glob("*-a-*-b-*", "-a--b-"));
        assert!(!matches_glob("*-a-*-b-*", "x-a-y-z")); // no '-b-'
    }
}
