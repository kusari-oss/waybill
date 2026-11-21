//! Spec 1639 (#1558): C56, `waybill:also-detected-via`.
//!
//! When one component absorbs another at a merge site (research R3, S1–S9),
//! the survivor records the absorbed member's source mechanism (C55) in C56,
//! unless it is the survivor's own. Every merge site calls [`record`], so the
//! union, the exclusion and the order live in one place.
//!
//! Rules (data-model "C56 rules"): C56 is a sorted, unique array of strings;
//! it never holds the survivor's own C55; it is absent when empty and when the
//! survivor has no C55 (research R5), because "also" needs something to be
//! also to. An absorbed member that already carries C56 from an earlier merge
//! contributes those values too.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

pub(crate) const ANN_ALSO_DETECTED_VIA: &str = "waybill:also-detected-via";
const ANN_SOURCE_MECHANISM: &str = "waybill:source-mechanism";

type Annotations = BTreeMap<String, Value>;

fn mechanism(a: &Annotations) -> Option<&str> {
    a.get(ANN_SOURCE_MECHANISM).and_then(Value::as_str)
}

fn recorded(a: &Annotations) -> BTreeSet<String> {
    a.get(ANN_ALSO_DETECTED_VIA)
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(|m| m.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// The census event (research R6): one per absorbed member, at every site.
/// `subject` is the survivor's PURL, so events group into merges.
pub(crate) fn observe(site: &'static str, subject: &str, survivor: &Annotations, absorbed: &Annotations) {
    let kept = mechanism(survivor).unwrap_or("-");
    let lost = mechanism(absorbed).unwrap_or("-");
    if site == "S1" {
        let rank_winner = rank_winner(subject, kept, lost);
        tracing::debug!(target: "waybill::also_detected", site, subject, kept, absorbed = lost, rank_winner);
    } else {
        tracing::debug!(target: "waybill::also_detected", site, subject, kept, absorbed = lost);
    }
}

/// Which member milestone 105's ranking would have kept, for the census
/// only (research R2): `survivor`, `absorbed`, or `unranked` when either
/// mechanism is not one of its variants.
fn rank_winner(purl: &str, kept: &str, lost: &str) -> &'static str {
    match (super::dedup::rank_key(kept, purl), super::dedup::rank_key(lost, purl)) {
        (Some(k), Some(l)) if l < k => "absorbed",
        (Some(_), Some(_)) => "survivor",
        _ => "unranked",
    }
}

/// The part of `absorbed` that [`record`] reads (its C55 and C56), for a site
/// that consumes the absorbed member before it can call `record` (S1's fold
/// moves the loser's map).
pub(crate) fn marks(absorbed: &Annotations) -> Annotations {
    absorbed
        .iter()
        .filter(|(k, _)| k.as_str() == ANN_SOURCE_MECHANISM || k.as_str() == ANN_ALSO_DETECTED_VIA)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Record `absorbed` on `survivor` at a merge site.
pub(crate) fn record(site: &'static str, subject: &str, survivor: &mut Annotations, absorbed: &Annotations) {
    observe(site, subject, survivor, absorbed);
    let mut set = recorded(survivor);
    set.extend(mechanism(absorbed).map(str::to_string));
    set.extend(recorded(absorbed));
    write(survivor, set);
}

fn write(survivor: &mut Annotations, mut set: BTreeSet<String>) {
    let Some(own) = mechanism(survivor).map(str::to_string) else {
        survivor.remove(ANN_ALSO_DETECTED_VIA);
        return;
    };
    set.remove(&own);
    if set.is_empty() {
        survivor.remove(ANN_ALSO_DETECTED_VIA);
    } else {
        survivor.insert(ANN_ALSO_DETECTED_VIA.to_string(), Value::from(set.into_iter().collect::<Vec<_>>()));
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;

    fn ann(c55: Option<&str>, c56: &[&str]) -> Annotations {
        let mut a = Annotations::new();
        if let Some(m) = c55 {
            a.insert(ANN_SOURCE_MECHANISM.to_string(), json!(m));
        }
        if !c56.is_empty() {
            a.insert(ANN_ALSO_DETECTED_VIA.to_string(), json!(c56));
        }
        a
    }

    fn c56(a: &Annotations) -> Option<Value> {
        a.get(ANN_ALSO_DETECTED_VIA).cloned()
    }

    #[test]
    fn a_different_mechanism_is_recorded() {
        let mut s = ann(Some("git-submodule"), &[]);
        record("S1", "pkg:x", &mut s, &ann(Some("cmake-fetchcontent-git"), &[]));
        assert_eq!(c56(&s), Some(json!(["cmake-fetchcontent-git"])));
    }

    #[test]
    fn the_survivors_own_mechanism_is_never_recorded() {
        let mut s = ann(Some("pkg-config"), &[]);
        record("S1", "pkg:x", &mut s, &ann(Some("pkg-config"), &[]));
        assert_eq!(c56(&s), None, "same mechanism: nothing to record");
        record("S1", "pkg:x", &mut s, &ann(Some("cmake-package-version"), &["pkg-config"]));
        assert_eq!(c56(&s), Some(json!(["cmake-package-version"])));
    }

    #[test]
    fn no_survivor_mechanism_means_no_record() {
        let mut s = ann(None, &[]);
        record("S3", "pkg:x", &mut s, &ann(Some("pip-requirements-txt"), &[]));
        assert_eq!(c56(&s), None, "research R5: nothing to be also to");
    }

    #[test]
    fn empty_is_absent_and_a_member_without_mechanism_adds_nothing() {
        let mut s = ann(Some("git-submodule"), &[]);
        record("S1", "pkg:x", &mut s, &ann(None, &[]));
        assert!(!s.contains_key(ANN_ALSO_DETECTED_VIA));
    }

    #[test]
    fn values_are_sorted_and_unique() {
        let mut s = ann(Some("git-submodule"), &[]);
        for m in ["cmake-fetchcontent-git", "cmake-externalproject", "cmake-fetchcontent-git"] {
            record("S1", "pkg:x", &mut s, &ann(Some(m), &[]));
        }
        assert_eq!(c56(&s), Some(json!(["cmake-externalproject", "cmake-fetchcontent-git"])));
    }

    #[test]
    fn an_absorbed_record_joins_the_survivors() {
        let mut s = ann(Some("cmake-externalproject"), &[]);
        let absorbed = ann(Some("git-submodule"), &["cmake-fetchcontent-git"]);
        record("S1", "pkg:x", &mut s, &absorbed);
        assert_eq!(c56(&s), Some(json!(["cmake-fetchcontent-git", "git-submodule"])));
    }

    #[test]
    fn the_census_ranks_only_known_mechanisms() {
        assert_eq!(rank_winner("pkg:github/o/r@1", "git-submodule", "cmake-fetchcontent-git"), "absorbed");
        assert_eq!(rank_winner("pkg:github/o/r@1", "cmake-fetchcontent-git", "git-submodule"), "survivor");
        assert_eq!(rank_winner("pkg:generic/x@1", "pkg-config", "cmake-package-version"), "unranked");
        assert_eq!(rank_winner("pkg:generic/x@1", "-", "git-submodule"), "unranked");
    }
}
