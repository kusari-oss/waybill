//! Milestone 663 — PyPi cache probe.
//! Concrete impl lands in US3 (T031).

use std::path::Path;
use waybill_common::types::purl::Purl;

pub(super) fn try_match_pypi(_path: &Path) -> Option<Purl> {
    // US3 stub.
    None
}
