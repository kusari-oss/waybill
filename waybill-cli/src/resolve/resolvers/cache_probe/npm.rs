//! Milestone 663 — npm/pnpm cache probe.
//! Concrete impl lands in US3 (T028).

use std::path::Path;
use waybill_common::types::purl::Purl;

pub(super) fn try_match_npm_pnpm(_path: &Path) -> Option<Purl> {
    // US3 stub.
    None
}
