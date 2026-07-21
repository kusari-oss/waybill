//! Milestone 128 — `conf/layer.conf` parser + nearest-ancestor
//! recipe-to-layer attribution.
//!
//! Per FR-006 + Clarifications Q2: use the **nearest-ancestor
//! `conf/layer.conf`** heuristic to attribute each recipe to its
//! owning layer. NO `BBFILES`-pattern parsing — the conventional
//! `<layer>/recipes-*/<dir>/*.bb` layout is correct for ≥99% of
//! real meta-layers including all three motivating fixtures.

use std::path::{Path, PathBuf};

use super::super::exclude_path::ExclusionSet;

/// Parsed shape of one `conf/layer.conf` declaration.
///
/// A single `conf/layer.conf` file MAY declare multiple
/// `BBFILE_COLLECTIONS` entries (the rare two-layer-in-one-file
/// case from the spec's edge-case list) — see `parse` which
/// returns a `Vec<LayerConf>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerConf {
    /// `BBFILE_COLLECTIONS += "<name>"` value. Required field —
    /// when absent, the layer.conf is malformed and `parse` skips
    /// it with a warn log.
    pub collection: String,
    /// `LAYERVERSION_<collection> = "<version>"` value.
    /// `None` when absent (some layers omit it; caller falls back
    /// to `"0.0.0"` placeholder when emitting the layer-root
    /// component per the spec's FR-007 conventions).
    pub version: Option<String>,
    /// `LAYERSERIES_COMPAT_<collection> = "<series>"` value
    /// (e.g., `"scarthgap"` or `"kirkstone honister"`), split on
    /// whitespace. Empty when absent.
    pub series_compat: Vec<String>,
    /// Filesystem path of the `conf/layer.conf` file.
    pub source_path: PathBuf,
}

/// Parse one `conf/layer.conf` file. Returns the list of declared
/// `BBFILE_COLLECTIONS` entries (typically one per file; the rare
/// two-layer-in-one-file case from spec edge-cases returns a
/// multi-element `Vec`).
pub(crate) fn parse(path: &Path) -> Vec<LayerConf> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Milestone 128 FR-006: could not read conf/layer.conf; skipping"
            );
            return Vec::new();
        }
    };

    let mut collections: Vec<String> = Vec::new();
    let mut versions: std::collections::BTreeMap<String, String> = Default::default();
    let mut series: std::collections::BTreeMap<String, Vec<String>> = Default::default();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // BBFILE_COLLECTIONS += "<name>" (also handle ?= and =).
        if let Some((field, value)) = split_assignment(line) {
            let value = strip_quotes(value);
            if field == "BBFILE_COLLECTIONS" {
                for c in value.split_whitespace() {
                    if !collections.iter().any(|x| x == c) {
                        collections.push(c.to_string());
                    }
                }
            } else if let Some(suffix) = field.strip_prefix("LAYERVERSION_") {
                versions.insert(suffix.to_string(), value.trim().to_string());
            } else if let Some(suffix) = field.strip_prefix("LAYERSERIES_COMPAT_") {
                let entries: Vec<String> =
                    value.split_whitespace().map(|s| s.to_string()).collect();
                series.insert(suffix.to_string(), entries);
            }
        }
    }

    let mut out = Vec::with_capacity(collections.len());
    for collection in collections {
        let version = versions.get(&collection).cloned();
        let series_compat = series.get(&collection).cloned().unwrap_or_default();
        out.push(LayerConf {
            collection,
            version,
            series_compat,
            source_path: path.to_path_buf(),
        });
    }
    out
}

/// Split a `FIELD <op> "value"` line into `(field, value-including-quotes)`.
/// Recognized operators: `=`, `?=`, `??=`, `:=`, `+=`. Returns None for
/// non-assignment lines (directives, comments).
fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let ops: &[&str] = &["??=", "?=", ":=", "+=", "="];
    for op in ops {
        if let Some(idx) = line.find(op) {
            let field = line[..idx].trim();
            let value = line[idx + op.len()..].trim();
            if !field.is_empty() && !value.is_empty() {
                return Some((field, value));
            }
        }
    }
    None
}

fn strip_quotes(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.chars().next();
        let last = trimmed.chars().last();
        if matches!((first, last), (Some('"'), Some('"')) | (Some('\''), Some('\''))) {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

/// Walk the scan tree for `conf/layer.conf` files; parse each into
/// one or more `LayerConf` entries. Bounded to depth 8 to match the
/// other Yocto-readers' convention.
pub(crate) fn build_index(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<LayerConf> {
    let mut out: Vec<LayerConf> = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 8,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(super::super::project_roots::should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if !path.is_file() {
            return;
        }
        let is_layer_conf = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|n| n == "layer.conf")
            .unwrap_or(false);
        if !is_layer_conf {
            return;
        }
        // Must be inside a `conf/` directory.
        let in_conf_dir = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(|n| n == "conf")
            .unwrap_or(false);
        if !in_conf_dir {
            return;
        }
        out.extend(parse(path));
    });
    out
}

/// Nearest-ancestor attribution per FR-006 + Clarifications Q2.
/// Walks the recipe's path upward, checking each ancestor directory
/// against the parents of every `LayerConf.source_path` (which is
/// `<layer>/conf/layer.conf` — so the layer root is two parents up).
/// Returns the deepest matching `LayerConf`.
pub(crate) fn attribute_recipe<'a>(
    recipe_path: &Path,
    layer_index: &'a [LayerConf],
) -> Option<&'a LayerConf> {
    let canonical_recipe = std::fs::canonicalize(recipe_path).ok()?;
    let mut best: Option<(&LayerConf, usize)> = None;
    for layer in layer_index {
        // Layer root = parent of `conf/` = parent of parent of source_path.
        let layer_root = layer
            .source_path
            .parent()
            .and_then(|p| p.parent());
        let Some(layer_root) = layer_root else {
            continue;
        };
        let canonical_layer = match std::fs::canonicalize(layer_root) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if canonical_recipe.starts_with(&canonical_layer) {
            let depth = canonical_layer.components().count();
            match best {
                None => best = Some((layer, depth)),
                Some((_, best_depth)) if depth > best_depth => best = Some((layer, depth)),
                _ => {}
            }
        }
    }
    best.map(|(layer, _)| layer)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_typical_layer_conf() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("layer.conf");
        std::fs::write(
            &path,
            r#"BBPATH .= ":${LAYERDIR}"
BBFILE_COLLECTIONS += "balena-generic"
LAYERVERSION_balena-generic = "1"
LAYERSERIES_COMPAT_balena-generic = "scarthgap"
"#,
        )
        .unwrap();
        let layers = parse(&path);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].collection, "balena-generic");
        assert_eq!(layers[0].version.as_deref(), Some("1"));
        assert_eq!(layers[0].series_compat, vec!["scarthgap"]);
    }

    #[test]
    fn parse_multi_collection_layer_conf() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("layer.conf");
        std::fs::write(
            &path,
            r#"BBFILE_COLLECTIONS += "alpha beta"
LAYERVERSION_alpha = "1"
LAYERVERSION_beta = "2"
"#,
        )
        .unwrap();
        let layers = parse(&path);
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].collection, "alpha");
        assert_eq!(layers[1].collection, "beta");
    }

    #[test]
    fn parse_handles_quotes_and_comments() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("layer.conf");
        std::fs::write(
            &path,
            r#"# This is a comment
BBFILE_COLLECTIONS += "foo"

LAYERVERSION_foo = "7"
"#,
        )
        .unwrap();
        let layers = parse(&path);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].version.as_deref(), Some("7"));
    }
}
