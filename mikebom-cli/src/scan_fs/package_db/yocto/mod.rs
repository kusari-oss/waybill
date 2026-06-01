//! Yocto / OpenEmbedded source-tree readers (milestone 107).
//!
//! Sub-modules:
//! - `context` — sysroot-vs-rootfs detection (US3, FR-005a)
//! - `manifest` — `<image>.manifest` reader (US2, FR-003)
//! - `recipe` — `.bb` filename walker (US4) — added by Phase 5
//!
//! `context` is consumed by `package_db/opkg.rs` to decide
//! lifecycle-scope tagging; `manifest` is a standalone reader called
//! directly from `read_all`.

pub(crate) mod context;
pub mod manifest;
