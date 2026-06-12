//! BitBake recipe walker (milestone 107 US4, FR-007, FR-008).
//!
//! Walks the scan target for `.bb` recipe files in Yocto/OE layer
//! directories and emits one component per recipe, drawn from the
//! filename pattern `<name>_<version>.bb` without parsing the recipe
//! body. This is the lowest-authority Yocto reader — recipes declared
//! by a layer may never have been selected by any image build — but
//! it's the only signal for a layer-tree scan with no build artifacts
//! present (security researchers auditing a vendor `meta-*/` layer
//! before adoption).
//!
//! Per FR-007: filename-only emission, no BitBake variable expansion.
//! Recipes whose filenames contain unexpanded `${...}` (typically
//! shared-base recipes like `${PN}_${PV}.bb`) are silently skipped
//! with a `tracing::warn!` per FR-008.
//!
//! Per FR-010 precedence: `BitbakeRecipe` is the lowest tier (2) —
//! installed-DB readers and image-manifest readers both outrank it.

use std::path::{Path, PathBuf};

use mikebom_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;

use super::super::PackageDbEntry;

const RECIPE_FILENAME_REGEX: &str =
    r"^(?P<name>[a-zA-Z0-9_\-\+\.]+)_(?P<version>[a-zA-Z0-9_\-\+\.\~]+)\.bb$";

/// Walk the scan target for `.bb` recipe files and emit one
/// `PackageDbEntry` per recipe. Bounded to depth 8 (matches the
/// established source-tree-walker convention) to avoid runaway
/// traversal in deep monorepos.
pub fn read(
    rootfs: &Path,
    exclude_set: &super::super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let Ok(regex) = Regex::new(RECIPE_FILENAME_REGEX) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk(rootfs, rootfs, 0, 8, &regex, &mut out, exclude_set);
    out
}

fn walk(
    dir: &Path,
    rootfs: &Path,
    depth: usize,
    max_depth: usize,
    regex: &Regex,
    out: &mut Vec<PackageDbEntry>,
    exclude_set: &super::super::exclude_path::ExclusionSet,
) {
    if depth > max_depth {
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if super::super::project_roots::should_skip_default_descent(name) {
                continue;
            }
            // Milestone 113 — user-supplied directory exclusion.
            if !exclude_set.is_empty() {
                if let Ok(rel) = path.strip_prefix(rootfs) {
                    if exclude_set.matches(&rel.to_string_lossy()) {
                        continue;
                    }
                }
            }
            walk(&path, rootfs, depth + 1, max_depth, regex, out, exclude_set);
        } else if ft.is_file() {
            let Some(filename) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !filename.ends_with(".bb") {
                continue;
            }
            if let Some(entry) = process_recipe(&path, filename, regex) {
                out.push(entry);
            }
        }
    }
}

fn process_recipe(path: &Path, filename: &str, regex: &Regex) -> Option<PackageDbEntry> {
    // FR-008: silently skip recipes whose filenames carry unexpanded
    // BitBake variable expansion. The literal sequence `${` is the
    // canonical marker.
    if filename.contains("${") {
        tracing::warn!(
            path = %path.display(),
            "BitBake recipe filename contains unexpanded variable; skipping per FR-008"
        );
        return None;
    }

    let captures = regex.captures(filename);
    let (name, version, version_missing) = if let Some(caps) = captures {
        let name = caps.name("name")?.as_str().to_string();
        let version = caps.name("version")?.as_str().to_string();
        (name, version, false)
    } else {
        // `.bb` file with no `_<version>` segment (rare; e.g.,
        // `helloworld.bb`). Per data-model: emit with version="unknown"
        // and a `mikebom:version-status: "missing"` annotation rather
        // than dropping.
        let stem = filename.strip_suffix(".bb")?;
        if stem.is_empty() {
            return None;
        }
        (stem.to_string(), "unknown".to_string(), true)
    };

    let layer_name = detect_layer_name(path);
    let purl = build_bitbake_purl(&name, &version, layer_name.as_deref())?;

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("bitbake-recipe".to_string()),
    );
    if let Some(layer) = &layer_name {
        extra_annotations.insert(
            "mikebom:layer-name".to_string(),
            serde_json::Value::String(layer.clone()),
        );
    }
    if version_missing {
        extra_annotations.insert(
            "mikebom:version-status".to_string(),
            serde_json::Value::String("missing".to_string()),
        );
    }

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version,
        arch: None,
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
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
        // R13: design-tier (declared but not necessarily built).
        sbom_tier: Some("design".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    })
}

/// Walk UP from the recipe's directory looking for the enclosing
/// `meta-<name>/` directory (the layer root). Returns the layer's
/// directory name without the `meta-` prefix? No — per contract, the
/// layer's BASENAME verbatim (e.g., `meta-mikebom-fixture`).
///
/// Fallback when no `meta-*/` ancestor is found: returns the path
/// component immediately above the first `recipes-*/` directory.
/// Returns None when neither pattern matches (caller emits no
/// `?layer=` qualifier and no `mikebom:layer-name` annotation).
fn detect_layer_name(recipe_path: &Path) -> Option<String> {
    // Strategy 1: walk up looking for `meta-<name>/`.
    let mut cursor = recipe_path.parent();
    while let Some(dir) = cursor {
        if let Some(name) = dir.file_name().and_then(|s| s.to_str()) {
            if name.starts_with("meta-") || name == "meta" {
                return Some(name.to_string());
            }
        }
        cursor = dir.parent();
    }
    // Strategy 2: walk up looking for `recipes-*/` and return its
    // parent's basename.
    let mut last_dir: Option<PathBuf> = None;
    let mut cursor = recipe_path.parent().map(PathBuf::from);
    while let Some(dir) = &cursor {
        if let Some(name) = dir.file_name().and_then(|s| s.to_str()) {
            if name.starts_with("recipes-") {
                // Return the parent's basename (the "layer root" by
                // structure even without a `meta-` prefix).
                return dir
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .map(str::to_string);
            }
        }
        last_dir = Some(dir.clone());
        cursor = dir.parent().map(PathBuf::from);
    }
    drop(last_dir);
    None
}

fn build_bitbake_purl(name: &str, version: &str, layer: Option<&str>) -> Option<Purl> {
    let purl_str = match layer {
        Some(l) => format!(
            "pkg:bitbake/{}@{}?layer={}",
            encode_purl_segment(name),
            encode_purl_segment(version),
            encode_purl_segment(l)
        ),
        None => format!(
            "pkg:bitbake/{}@{}",
            encode_purl_segment(name),
            encode_purl_segment(version),
        ),
    };
    Purl::new(&purl_str).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn extracts_name_and_version_from_filename() {
        let tmp = tempfile::tempdir().unwrap();
        let recipe = tmp
            .path()
            .join("meta-mikebom-fixture")
            .join("recipes-mikebom")
            .join("mikebom-fixture-lib")
            .join("mikebom-fixture-lib_1.2.3.bb");
        touch(&recipe);
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-lib");
        assert_eq!(entries[0].version, "1.2.3");
    }

    #[test]
    fn emits_layer_qualifier_from_meta_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let recipe = tmp
            .path()
            .join("meta-mikebom-fixture")
            .join("recipes-mikebom")
            .join("mikebom-fixture-lib")
            .join("mikebom-fixture-lib_1.2.3.bb");
        touch(&recipe);
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:bitbake/mikebom-fixture-lib@1.2.3?layer=meta-mikebom-fixture"
        );
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:layer-name")
                .and_then(|v| v.as_str()),
            Some("meta-mikebom-fixture")
        );
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("bitbake-recipe")
        );
    }

    #[test]
    fn unexpanded_variables_skipped_silently() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-shared")
                .join("${PN}_${PV}.bb"),
        );
        // Also include one valid recipe to confirm the valid one still emits.
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-real")
                .join("mikebom-fixture-real_1.0.bb"),
        );
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-real");
    }

    #[test]
    fn version_only_filename_emits_unknown_version_annotation() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-noversion")
                .join("mikebom-fixture-noversion.bb"),
        );
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-noversion");
        assert_eq!(entries[0].version, "unknown");
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:version-status")
                .and_then(|v| v.as_str()),
            Some("missing")
        );
    }

    #[test]
    fn bbappend_and_bbclass_files_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-lib")
                .join("mikebom-fixture-lib_1.0.bbappend"),
        );
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("classes")
                .join("mikebom-fixture-helper.bbclass"),
        );
        // Add one real `.bb` to confirm walker is working but is just
        // ignoring the .bbappend and .bbclass.
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-lib")
                .join("mikebom-fixture-lib_1.0.bb"),
        );
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-lib");
    }

    #[test]
    fn git_version_suffix_preserved_in_version() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path()
                .join("meta-mikebom-fixture")
                .join("recipes-mikebom")
                .join("mikebom-fixture-lib")
                .join("mikebom-fixture-lib_1.2.3+git0abc123.bb"),
        );
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "1.2.3+git0abc123");
    }
}
