//! Milestone 128 — `.bbappend` walker + match-index for
//! base-recipe customization tracking.
//!
//! Per FR-008: when a `.bbappend` file's basename matches an
//! existing recipe (with the `_%` glob expanded as a wildcard),
//! the recipe component receives a `mikebom:bbappend-applied`
//! annotation listing the matching append paths. Orphan
//! `.bbappend`s (no matching recipe in scan) emit a
//! `tracing::warn!` log and do NOT produce phantom components
//! (Constitution VIII completeness).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::super::exclude_path::ExclusionSet;

/// Maps `(recipe-name, version-glob)` keys → list of `.bbappend`
/// file paths.
///
/// The version-glob string is the version segment from the
/// `.bbappend` filename:
/// - `u-boot_%.bbappend` → key `("u-boot", "%")` (wildcard)
/// - `u-boot_2024.07.bbappend` → key `("u-boot", "2024.07")` (exact)
///
/// `appends_for(name, version)` matches both exact-version AND
/// version-glob keys.
#[derive(Debug, Clone, Default)]
pub(crate) struct BbAppendIndex {
    pub by_recipe: BTreeMap<(String, String), Vec<PathBuf>>,
    /// `.bbappend` files that had no matching recipe in the scan.
    /// Surfaced via the `tracing::warn!` log per US4 AC#3 at
    /// scan-end; no phantom components are emitted.
    pub orphans: Vec<PathBuf>,
}

impl BbAppendIndex {
    /// For a recipe `(name, version)`, return the lex-sorted list
    /// of append paths modifying it. Matches both exact-version
    /// entries (`name_<version>.bbappend`) and version-glob entries
    /// (`name_%.bbappend`).
    pub(crate) fn appends_for(&self, name: &str, version: &str) -> Vec<&PathBuf> {
        let mut out: Vec<&PathBuf> = Vec::new();
        // Exact version match.
        if let Some(paths) = self.by_recipe.get(&(name.to_string(), version.to_string())) {
            out.extend(paths.iter());
        }
        // Wildcard `%` match.
        if let Some(paths) = self.by_recipe.get(&(name.to_string(), "%".to_string())) {
            out.extend(paths.iter());
        }
        // Lex-sort by path string for deterministic output.
        out.sort_by(|a, b| a.as_os_str().cmp(b.as_os_str()));
        out.dedup();
        out
    }
}

/// Filename pattern for `.bbappend` files. Same shape as
/// `recipe.rs::RECIPE_FILENAME_REGEX` but with `.bbappend` suffix and
/// `%` (BitBake's version wildcard) allowed in the version segment.
const BBAPPEND_FILENAME_RE: &str =
    r"^(?P<name>[a-zA-Z0-9_\-\+\.]+)_(?P<version>%|[a-zA-Z0-9_\-\+\.\~%]+)\.bbappend$";

/// Walk the scan tree for `.bbappend` files; build the recipe-match
/// index. Bounded to depth 8 (matches the established Yocto-walker
/// convention).
pub(crate) fn build_from_walk(rootfs: &Path, exclude_set: &ExclusionSet) -> BbAppendIndex {
    let mut idx = BbAppendIndex::default();
    let Ok(regex) = regex::Regex::new(BBAPPEND_FILENAME_RE) else {
        return idx;
    };
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
        let Some(filename) = path.file_name().and_then(|s| s.to_str()) else {
            return;
        };
        if !filename.ends_with(".bbappend") {
            return;
        }
        let Some(captures) = regex.captures(filename) else {
            return;
        };
        let name = captures.name("name").map(|m| m.as_str().to_string());
        let version = captures.name("version").map(|m| m.as_str().to_string());
        if let (Some(name), Some(version)) = (name, version) {
            idx.by_recipe
                .entry((name, version))
                .or_default()
                .push(path.to_path_buf());
        }
    });
    idx
}

/// Finalize the orphan list per FR-008: after recipe components have
/// been emitted, move any `(name, version)` keys that didn't match a
/// recipe into `orphans` and emit a `tracing::warn!` per US4 AC#3.
pub(crate) fn finalize_orphans(
    idx: &mut BbAppendIndex,
    recipe_keys: &std::collections::BTreeSet<(String, String)>,
) {
    let mut orphans: Vec<PathBuf> = Vec::new();
    let mut to_keep: BTreeMap<(String, String), Vec<PathBuf>> = BTreeMap::new();
    for ((name, version), paths) in idx.by_recipe.iter() {
        let matched = if version == "%" {
            recipe_keys.iter().any(|(n, _)| n == name)
        } else {
            recipe_keys.contains(&(name.clone(), version.clone()))
        };
        if matched {
            to_keep.insert((name.clone(), version.clone()), paths.clone());
        } else {
            for p in paths {
                orphans.push(p.clone());
                tracing::warn!(
                    bbappend = %p.display(),
                    "Milestone 128 FR-008: orphan .bbappend (no matching recipe in scan); not synthesizing phantom component"
                );
            }
        }
    }
    idx.by_recipe = to_keep;
    idx.orphans.extend(orphans);
}
