//! Spec 1654 (#1695) — the parity verdict of one catalog row.
//!
//! `waybill sbom parity-check` decided pass or fail per row, counted the
//! failures in `summary.parity_gaps`, and reported the decision nowhere per
//! row. Its JSON's `universal_parity` field is the catalog's *expectation*
//! (should the row appear in every format), and ten integration tests read it
//! as the result, so their parity assertions could not fail.
//!
//! This is the one place the per-directionality rule lives. `parity-check`
//! reports it per row and counts it; `tests/holistic_parity.rs` asserts its
//! own outcome agrees with it, so the two cannot drift.

use std::collections::BTreeSet;

use super::extractors::Directionality;

/// One row's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Universal, carried by at least one format, and the invariant holds.
    Pass,
    /// Universal, and the invariant does not hold. Counted by
    /// `summary.parity_gaps`.
    Fail,
    /// Not universal (format-restricted), or universal and carried by no
    /// format. A universal row carried by none still counts as passing in
    /// `summary.universal_parity_rows_passing`, as it always has.
    NotChecked,
}

impl Verdict {
    /// The JSON value.
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::NotChecked => "not-checked",
        }
    }
}

/// The verdict of one row, from its directionality and the three formats'
/// extracted values. `directional` is `None` when the catalog row has no
/// extractor, which is a failure for a universal row.
pub fn row_verdict(
    directional: Option<Directionality>,
    universal: bool,
    cdx: &BTreeSet<String>,
    spdx23: &BTreeSet<String>,
    spdx3: &BTreeSet<String>,
) -> Verdict {
    if !universal {
        return Verdict::NotChecked;
    }
    let any_present = !cdx.is_empty() || !spdx23.is_empty() || !spdx3.is_empty();
    let holds = match directional {
        None => return Verdict::Fail,
        Some(Directionality::SymmetricEqual) => cdx == spdx23 && spdx23 == spdx3,
        Some(Directionality::CdxSubsetOfSpdx) => cdx.is_subset(spdx23) && cdx.is_subset(spdx3),
        Some(Directionality::PresenceOnly) => {
            !any_present || (!cdx.is_empty() && !spdx23.is_empty() && !spdx3.is_empty())
        }
        Some(Directionality::CdxOnly) => true,
    };
    match (holds, any_present) {
        (false, _) => Verdict::Fail,
        (true, true) => Verdict::Pass,
        (true, false) => Verdict::NotChecked,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn symmetric_equal() {
        let d = Some(Directionality::SymmetricEqual);
        assert_eq!(row_verdict(d, true, &s(&["a"]), &s(&["a"]), &s(&["a"])), Verdict::Pass);
        assert_eq!(row_verdict(d, true, &s(&["a"]), &s(&["a"]), &s(&[])), Verdict::Fail);
        assert_eq!(row_verdict(d, true, &s(&["a"]), &s(&["b"]), &s(&["a"])), Verdict::Fail);
    }

    /// The spec 1653 case: present in two formats, absent in the third.
    #[test]
    fn a_row_dropped_from_one_format_fails() {
        let d = Some(Directionality::SymmetricEqual);
        let v = s(&["{\"source\":\"x\"}"]);
        assert_eq!(row_verdict(d, true, &v, &v, &s(&[])), Verdict::Fail);
    }

    #[test]
    fn cdx_subset_of_spdx() {
        let d = Some(Directionality::CdxSubsetOfSpdx);
        assert_eq!(row_verdict(d, true, &s(&["a"]), &s(&["a", "b"]), &s(&["a", "c"])), Verdict::Pass);
        assert_eq!(row_verdict(d, true, &s(&["a", "z"]), &s(&["a", "b"]), &s(&["a", "z"])), Verdict::Fail);
        assert_eq!(row_verdict(d, true, &s(&[]), &s(&["b"]), &s(&["c"])), Verdict::Pass);
    }

    #[test]
    fn presence_only() {
        let d = Some(Directionality::PresenceOnly);
        assert_eq!(row_verdict(d, true, &s(&["a"]), &s(&["b", "c"]), &s(&["d"])), Verdict::Pass);
        assert_eq!(row_verdict(d, true, &s(&[]), &s(&["b"]), &s(&["d"])), Verdict::Fail);
    }

    #[test]
    fn cdx_only_always_passes_when_present() {
        let d = Some(Directionality::CdxOnly);
        assert_eq!(row_verdict(d, true, &s(&["a"]), &s(&[]), &s(&[])), Verdict::Pass);
    }

    #[test]
    fn unexercised_and_restricted_rows_are_not_checked() {
        for d in [
            Directionality::SymmetricEqual,
            Directionality::CdxSubsetOfSpdx,
            Directionality::PresenceOnly,
            Directionality::CdxOnly,
        ] {
            assert_eq!(row_verdict(Some(d), true, &s(&[]), &s(&[]), &s(&[])), Verdict::NotChecked);
        }
        assert_eq!(
            row_verdict(Some(Directionality::SymmetricEqual), false, &s(&["a"]), &s(&[]), &s(&[])),
            Verdict::NotChecked
        );
    }

    #[test]
    fn a_universal_row_without_an_extractor_fails() {
        assert_eq!(row_verdict(None, true, &s(&[]), &s(&[]), &s(&[])), Verdict::Fail);
    }

    #[test]
    fn wire_values() {
        assert_eq!(Verdict::Pass.as_str(), "pass");
        assert_eq!(Verdict::Fail.as_str(), "fail");
        assert_eq!(Verdict::NotChecked.as_str(), "not-checked");
    }
}
