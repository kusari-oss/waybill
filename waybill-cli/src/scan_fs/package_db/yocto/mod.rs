//! Yocto / OpenEmbedded source-tree readers (milestones 107 + 128).
//!
//! Sub-modules:
//! - `context` — sysroot-vs-rootfs detection (milestone 107 US3, FR-005a)
//! - `manifest` — `<image>.manifest` reader (milestone 107 US2, FR-003)
//! - `recipe` — `.bb` filename walker + body parser (milestone 107 + 128)
//! - `recipe_body` — line-oriented BitBake body parser (milestone 128 FR-001..FR-005)
//! - `layer_conf` — `conf/layer.conf` parser + nearest-ancestor attribution (milestone 128 FR-006)
//! - `bbappend` — `.bbappend` walker + match index (milestone 128 FR-008)
//! - `cpe_name_map` — embedded openembedded-core recipe-to-CPE-product mapping (milestone 128 FR-017)
//!
//! `context` is consumed by `package_db/opkg.rs` to decide
//! lifecycle-scope tagging; `manifest`, `recipe`, `layer_conf`, and
//! `bbappend` are standalone readers called directly from `read_all`.

pub(crate) mod bbappend;
pub(crate) mod context;
pub(crate) mod cpe_name_map;
pub(crate) mod layer_conf;
pub mod manifest;
pub mod recipe;
pub(crate) mod recipe_body;

// Milestone 664 US2 T059: shared-walker discovery for `.bbappend` and
// `conf/layer.conf` files (the two secondary yocto walkers). The
// primary `.bb` walker inside `recipe::read` stays on legacy (deferred
// T029). Registration + state + extract helpers live here so both
// `bbappend.rs` and `layer_conf.rs` sub-readers share one traversal.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext,
};

/// Discovery state for the T059 + T029 yocto shared-walker
/// registration. Three parallel vectors covering the three legacy
/// walker sites in the yocto reader family:
/// - `bbappend_paths`: T059 `.bbappend` for `bbappend::build_index_from_paths`
/// - `layer_conf_paths`: T059 `conf/layer.conf` for `layer_conf::build_index_from_paths`
/// - `bb_paths`: T029 `.bb` recipe files for `recipe::read`'s primary walker
#[derive(Default, Debug)]
pub(crate) struct YoctoLayersDiscoveredPaths {
    /// `.bbappend` file paths — consumed by `bbappend::build_index_from_paths`.
    pub(crate) bbappend_paths: Vec<PathBuf>,
    /// `conf/layer.conf` file paths — consumed by
    /// `layer_conf::build_index_from_paths`. Callback enforces the
    /// canonical `<layer>/conf/layer.conf` layout (parent basename ==
    /// `conf`) mirroring the legacy `build_index` filter.
    pub(crate) layer_conf_paths: Vec<PathBuf>,
    /// T029: `.bb` recipe file paths — consumed by `recipe::read`'s
    /// new `precomputed_bb_paths: Option<Vec<PathBuf>>` parameter to
    /// skip the primary recipe walker (was the last remaining
    /// scan-tree safe_walk in the yocto reader family).
    pub(crate) bb_paths: Vec<PathBuf>,
}

fn on_yocto_layers_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    let Some(basename) = path.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    let Some(state) = ctx.state::<Mutex<YoctoLayersDiscoveredPaths>>(ReaderId::YOCTO_LAYERS)
    else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    // Check `.bbappend` FIRST — its basename also ends with `.bb`, so
    // the order matters. The early return ensures a `.bbappend` file
    // is never double-classified as a `.bb` recipe.
    if basename.ends_with(".bbappend") {
        guard.bbappend_paths.push(path.to_path_buf());
        return;
    }
    // T029: `.bb` recipe files. Skip parity: legacy `recipe::read`
    // used `should_skip_default_descent` which matches the shared
    // walker's default exactly — no ancestor-path filter needed.
    if basename.ends_with(".bb") {
        guard.bb_paths.push(path.to_path_buf());
        return;
    }
    if basename == "layer.conf" {
        // Canonical `<layer>/conf/layer.conf` — parent basename MUST
        // be `conf` (mirrors legacy filter at
        // `layer_conf::build_index` line 156-164).
        let parent_is_conf = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            == Some("conf");
        if parent_is_conf {
            guard.layer_conf_paths.push(path.to_path_buf());
        }
    }
}

pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&[
        "**/*.bbappend",
        "**/*.bb",
        "**/layer.conf",
    ])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::YOCTO_LAYERS,
        state: Some(Arc::new(Mutex::new(YoctoLayersDiscoveredPaths::default()))),
        patterns,
        on_file: Some(on_yocto_layers_file),
        on_dir: None,
        descend_into: None,
    })
}

pub(crate) fn extract_paths(
    registration: &ReaderRegistration,
) -> YoctoLayersDiscoveredPaths {
    let Some(state_arc) = registration.state.as_ref() else {
        return YoctoLayersDiscoveredPaths::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<YoctoLayersDiscoveredPaths>>() else {
        return YoctoLayersDiscoveredPaths::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}
