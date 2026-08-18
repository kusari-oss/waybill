//! Milestone 663 — rubygems cache probe.
//! Concrete impl lands in US-phase per tasks.md.

use std::path::Path;
use waybill_common::types::purl::Purl;

pub(super) fn try_match_rubygems(_path: &Path) -> Option<Purl> {
    // Stub.
    None
}
