//! Tier 2: poetry.lock parser (v1 and v2 formats).
//!
//! Dispatches on the top-level `[metadata] lock-version` field to
//! handle both v1 (`"1.1"` / `"1.2"`) and v2 (`"2.0"` / `"2.1"`)
//! shapes. Returns None when no `poetry.lock` exists or the file is
//! unparseable. Called from [`super::read`] after the venv tier.

use std::path::Path;

use mikebom_common::types::purl::Purl;

use super::super::PackageDbEntry;
use super::build_pypi_purl_str;

// -----------------------------------------------------------------------
// Tier 2: Poetry lockfile (v1 + v2)
// -----------------------------------------------------------------------

/// Read `<rootfs>/poetry.lock` if present. Returns None when absent or
/// unparseable. Dispatches on the top-level `[metadata] lock-version`
/// field to handle both v1 (`"1.1"` / `"1.2"`) and v2 (`"2.0"` / `"2.1"`)
/// shapes.
pub(super) fn read_poetry_lock(rootfs: &Path, include_dev: bool) -> Option<Vec<PackageDbEntry>> {
    let path = rootfs.join("poetry.lock");
    let text = std::fs::read_to_string(&path).ok()?;
    let parsed: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "poetry.lock parse failed");
            return None;
        }
    };
    let source_path = path.to_string_lossy().into_owned();
    Some(parse_poetry_lock(&parsed, &source_path, include_dev))
}

/// Parse an already-deserialised `poetry.lock` TOML document.
/// Public-in-module for unit testing.
pub(crate) fn parse_poetry_lock(
    root: &toml::Value,
    source_path: &str,
    include_dev: bool,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();

    // [[package]] array-of-tables.
    let Some(packages) = root.get("package").and_then(|v| v.as_array()) else {
        return out;
    };

    for pkg in packages {
        let Some(tbl) = pkg.as_table() else {
            continue;
        };
        let name = tbl.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
        let version = tbl.get("version").and_then(|v| v.as_str()).unwrap_or("").trim();
        if name.is_empty() || version.is_empty() {
            continue;
        }

        // Dev detection:
        // v1: `category = "main"` (prod) / `"dev"` (dev)
        // v2+: `groups = ["main", ...]` — prod if "main" is present.
        // Milestone 052 rename: poetry_is_dev returns the legacy
        // boolean.
        //
        // Milestone 183 US1 — also consult the per-package `optional`
        // flag. Poetry's `optional = true` means the package is
        // extras-gated (`poetry install --extras <name>` required).
        // Semantically maps to `LifecycleScope::Optional` per m179.
        //
        // Precedence (Decision 2 dev-wins-over-optional): if the
        // package is dev-classified, the dev classification wins and
        // the `mikebom:optional-derivation` annotation is NOT emitted
        // (one-derivation-per-component invariant).
        let legacy_is_dev = poetry_is_dev(tbl);
        let is_optional = poetry_is_optional(tbl);
        let (lifecycle_scope, is_m183_optional) = match (legacy_is_dev, is_optional) {
            // Dev wins over optional — no annotation.
            (Some(true), _) => (
                Some(mikebom_common::resolution::LifecycleScope::Development),
                false,
            ),
            // Non-dev + optional = true → LifecycleScope::Optional.
            (Some(false), true) | (None, true) => (
                Some(mikebom_common::resolution::LifecycleScope::Optional),
                true,
            ),
            // Non-dev + optional = false → Runtime (unchanged).
            (Some(false), false) => (
                Some(mikebom_common::resolution::LifecycleScope::Runtime),
                false,
            ),
            // Unclassifiable + not optional → None (unchanged).
            (None, false) => (None, false),
        };

        // Honour the dev filter at source.
        if matches!(lifecycle_scope, Some(mikebom_common::resolution::LifecycleScope::Development))
            && !include_dev
        {
            continue;
        }

        // Nested dependencies table — keys are the dep names.
        let depends = tbl
            .get("dependencies")
            .and_then(|v| v.as_table())
            .map(|t| t.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        // Per-package hashes from `[[package.files]]`.
        let hashes = tbl
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| f.as_table()?.get("hash")?.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let purl_str = build_pypi_purl_str(name, version);
        let Ok(purl) = Purl::new(&purl_str) else {
            continue;
        };

        // Milestone 183 US1 — emit the C122 derivation annotation
        // when the classification came from the poetry `optional = true`
        // path (i.e., dev-wins-over-optional did NOT fire).
        let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();
        if is_m183_optional {
            extra_annotations.insert(
                "mikebom:optional-derivation".to_string(),
                serde_json::Value::String("pip-optional-dependencies".to_string()),
            );
        }

        out.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: source_path.to_string(),
            depends,
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope,
            requirement_range: None,
            source_type: None,
            // Lockfile entries are pre-build declarations of what WILL
            // be installed, not what IS installed. Tier = "source" per
            // research.md R13.
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            raw_version: None,
            parent_purl: None,
            npm_role: None,
            co_owned_by: None,
            hashes: Vec::new(),
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        });
        // `hashes` currently collected but not wired into ContentHash;
        // hash propagation from lockfiles is a follow-up (would need
        // SRI-style string parsing like npm integrity). The variable
        // is held in-scope as documentation of the intent.
        let _ = hashes;
    }

    out
}

/// Milestone 183 US1 — determine the `optional` flag for a
/// `poetry.lock` `[[package]]` entry.
///
/// Poetry surfaces extras-gated packages via a per-package boolean:
///
/// ```toml
/// [[package]]
/// name = "foo"
/// version = "1.0"
/// optional = true
/// ```
///
/// A missing or non-boolean field returns `false` (default: not
/// optional). This preserves pre-m183 behavior for every entry that
/// doesn't declare the field.
fn poetry_is_optional(tbl: &toml::value::Table) -> bool {
    tbl.get("optional").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Determine the dev-flag for a `poetry.lock` `[[package]]` entry.
/// Handles both lock-version dialects.
fn poetry_is_dev(tbl: &toml::value::Table) -> Option<bool> {
    // v1: `category = "main" | "dev"`
    if let Some(cat) = tbl.get("category").and_then(|v| v.as_str()) {
        return Some(cat == "dev");
    }
    // v2+: `groups = [...]` — prod iff "main" appears.
    if let Some(arr) = tbl.get("groups").and_then(|v| v.as_array()) {
        let has_main = arr
            .iter()
            .any(|g| g.as_str().is_some_and(|s| s == "main"));
        return Some(!has_main);
    }
    // No dev/prod info in the entry — preserve None so downstream
    // dedup treats it as "source didn't assert a scope."
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    #[test]
    fn poetry_lock_v1_category_dev_filtered_by_default() {
        let src = r#"
[[package]]
name = "requests"
version = "2.31.0"
description = "HTTP for Humans"
category = "main"
optional = false
python-versions = ">=3.7"

[[package]]
name = "pytest"
version = "7.4.0"
description = "testing framework"
category = "dev"
optional = false
python-versions = ">=3.7"

[metadata]
lock-version = "1.1"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", /*include_dev=*/ false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "requests");
        assert_eq!(out[0].lifecycle_scope, Some(mikebom_common::resolution::LifecycleScope::Runtime));
        assert_eq!(out[0].sbom_tier.as_deref(), Some("source"));
    }

    #[test]
    fn poetry_lock_v1_include_dev_surfaces_both() {
        let src = r#"
[[package]]
name = "requests"
version = "2.31.0"
category = "main"

[[package]]
name = "pytest"
version = "7.4.0"
category = "dev"

[metadata]
lock-version = "1.1"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", true);
        assert_eq!(out.len(), 2);
        let pytest = out.iter().find(|e| e.name == "pytest").expect("pytest present");
        assert_eq!(pytest.lifecycle_scope, Some(mikebom_common::resolution::LifecycleScope::Development));
    }

    #[test]
    fn poetry_lock_v2_groups_main_marks_prod() {
        let src = r#"
[[package]]
name = "requests"
version = "2.31.0"
groups = ["main"]

[[package]]
name = "pytest"
version = "7.4.0"
groups = ["dev"]

[metadata]
lock-version = "2.0"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", true);
        assert_eq!(out.len(), 2);
        let req = out.iter().find(|e| e.name == "requests").unwrap();
        let pyt = out.iter().find(|e| e.name == "pytest").unwrap();
        assert_eq!(req.lifecycle_scope, Some(mikebom_common::resolution::LifecycleScope::Runtime));
        assert_eq!(pyt.lifecycle_scope, Some(mikebom_common::resolution::LifecycleScope::Development));
    }

    #[test]
    fn poetry_lock_dependencies_table_populates_depends() {
        let src = r#"
[[package]]
name = "requests"
version = "2.31.0"
category = "main"

[package.dependencies]
urllib3 = ">=1.21.1,<3"
certifi = ">=2017.4.17"

[metadata]
lock-version = "1.1"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", false);
        assert_eq!(out.len(), 1);
        let e = &out[0];
        assert!(e.depends.contains(&"urllib3".to_string()));
        assert!(e.depends.contains(&"certifi".to_string()));
    }

    // ── Milestone 183 US1 — poetry.lock `optional = true` classification ──

    #[test]
    fn optional_true_non_dev_classifies_as_optional() {
        let src = r#"
[[package]]
name = "extras-only-pkg"
version = "1.0.0"
category = "main"
optional = true

[metadata]
lock-version = "1.1"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "extras-only-pkg");
        assert_eq!(
            out[0].lifecycle_scope,
            Some(mikebom_common::resolution::LifecycleScope::Optional)
        );
    }

    #[test]
    fn optional_true_annotation_carries_pip_optional_dependencies() {
        let src = r#"
[[package]]
name = "extras-only-pkg"
version = "1.0.0"
groups = ["main"]
optional = true

[metadata]
lock-version = "2.0"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", true);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].extra_annotations.get("mikebom:optional-derivation"),
            Some(&serde_json::Value::String("pip-optional-dependencies".to_string())),
        );
    }

    #[test]
    fn dev_classified_package_still_dev_ignoring_optional_flag() {
        // US1 acceptance 4 + Decision 2: dev-wins-over-optional. A
        // package with BOTH `category = "dev"` AND `optional = true`
        // must classify as Development, NOT Optional; the derivation
        // annotation MUST NOT appear.
        let src = r#"
[[package]]
name = "dev-and-optional"
version = "1.0.0"
category = "dev"
optional = true

[metadata]
lock-version = "1.1"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", true);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].lifecycle_scope,
            Some(mikebom_common::resolution::LifecycleScope::Development)
        );
        assert!(!out[0]
            .extra_annotations
            .contains_key("mikebom:optional-derivation"));
    }

    #[test]
    fn optional_false_stays_runtime() {
        let src = r#"
[[package]]
name = "regular-pkg"
version = "1.0.0"
category = "main"
optional = false

[metadata]
lock-version = "1.1"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", true);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].lifecycle_scope,
            Some(mikebom_common::resolution::LifecycleScope::Runtime)
        );
        assert!(!out[0]
            .extra_annotations
            .contains_key("mikebom:optional-derivation"));
    }

    #[test]
    fn optional_true_stays_in_reader_output_when_include_dev_false() {
        // Milestone 183 FR-008 boundary pin (R1 remediation from
        // /speckit-analyze U1):
        //
        // The `--include-dev=false` CLI flag filters `is_non_runtime()`
        // targets at EMITTER time via m179's `LifecycleScope::Optional.
        // is_non_runtime() == true` extension. The poetry.rs READER
        // does NOT filter Optional entries at collection time — only
        // Development entries (per the include_dev guard at line 98+).
        //
        // This test documents that boundary: with `include_dev=false`,
        // an `optional = true, category = "main"` entry IS returned by
        // `parse_poetry_lock`, and its `LifecycleScope::Optional`
        // classification lets the downstream emitter apply the
        // `is_non_runtime()` filter as designed.
        let src = r#"
[[package]]
name = "extras-only-pkg"
version = "1.0.0"
category = "main"
optional = true

[[package]]
name = "dev-pkg"
version = "1.0.0"
category = "dev"

[metadata]
lock-version = "1.1"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", /*include_dev=*/ false);
        // Development entries ARE filtered at reader level.
        assert!(
            out.iter().all(|e| e.name != "dev-pkg"),
            "dev-pkg should be filtered by include_dev=false"
        );
        // Optional entries are NOT filtered at reader level — deferred
        // to emitter via m179's is_non_runtime() extension.
        let optional_entry = out
            .iter()
            .find(|e| e.name == "extras-only-pkg")
            .expect("Optional entry retained at reader level");
        assert_eq!(
            optional_entry.lifecycle_scope,
            Some(mikebom_common::resolution::LifecycleScope::Optional),
            "the retained Optional entry MUST carry the classification so the \
             emitter's is_non_runtime() filter can drop it downstream"
        );
    }

    #[test]
    fn optional_field_absent_stays_runtime() {
        let src = r#"
[[package]]
name = "regular-pkg"
version = "1.0.0"
category = "main"

[metadata]
lock-version = "1.1"
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_poetry_lock(&parsed, "/poetry.lock", true);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].lifecycle_scope,
            Some(mikebom_common::resolution::LifecycleScope::Runtime)
        );
        assert!(!out[0]
            .extra_annotations
            .contains_key("mikebom:optional-derivation"));
    }
}
