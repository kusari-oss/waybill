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
