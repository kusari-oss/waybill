//! Deciding whether a candidate attribute is really the closure member.
//!
//! This is where the feature's accuracy lives. Attribute names and `pname`s
//! coincide often but not always: measured on a real closure, 35 of 380
//! members (9%) resolve to a top-level attribute that builds something else
//! entirely. Believing those would attach a security claim to a component
//! that was never in the build — worse than saying nothing, because a false
//! `affected` sends somebody to patch the wrong thing and costs the reader
//! their trust in every other statement in the document.

use std::collections::BTreeMap;

use super::evaluate::Candidate;
use super::AttributeResolution;
use crate::scan_fs::package_db::nix::closure::derivation::store_basename;

/// Decide one member's resolution against what evaluation returned.
///
/// `member_outputs` is every output path the closure records for the member,
/// not just one: a derivation has several (`out`, `dev`, `man`, `lib`) and
/// the candidate's `outPath` is only ever the default. Comparing against an
/// arbitrarily-chosen single output rejects real matches.
pub(crate) fn resolve_one<'a, I>(candidate: Option<&Candidate>, member_outputs: I) -> AttributeResolution
where
    I: IntoIterator<Item = &'a str>,
{
    let Some(c) = candidate else {
        return AttributeResolution::NoAttribute;
    };
    // Both sides through `store_basename`. The closure JSON stores outputs
    // WITHOUT the `/nix/store/` prefix; evaluation returns them WITH it.
    // Comparing the raw strings matches nothing at all, and a 0% result reads
    // as "the mechanism does not work" rather than "the strings are shaped
    // differently" — which is exactly how it was first misread.
    let want = store_basename(&c.out_path);
    if member_outputs.into_iter().any(|p| store_basename(p) == want) {
        AttributeResolution::Confirmed {
            source: c.source,
            declarations: c.declarations.clone(),
        }
    } else {
        AttributeResolution::PathMismatch
    }
}

/// Resolve every member. `members` maps a member's `pname` to its output
/// paths.
pub(crate) fn resolve_all(
    members: &BTreeMap<String, Vec<String>>,
    candidates: &BTreeMap<String, Candidate>,
) -> BTreeMap<String, AttributeResolution> {
    members
        .iter()
        .map(|(pname, outs)| {
            let r = resolve_one(candidates.get(pname), outs.iter().map(String::as_str));
            (pname.clone(), r)
        })
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::nix::declarations::parse::Declaration;
    use crate::scan_fs::package_db::nix::declarations::DeclarationSource;

    fn candidate(out: &str, kv: &[&str]) -> Candidate {
        Candidate {
            source: DeclarationSource::TopLevel,
            out_path: out.to_string(),
            declarations: kv.iter().map(|s| Declaration::parse(s)).collect(),
        }
    }

    #[test]
    fn a_matching_output_path_confirms_and_carries_the_declarations() {
        let c = candidate("/nix/store/aaa-thing-1.0", &["CVE-2020-1: boom"]);
        let r = resolve_one(Some(&c), ["aaa-thing-1.0"]);
        match r {
            AttributeResolution::Confirmed { declarations, .. } => {
                assert_eq!(declarations.len(), 1);
                assert_eq!(declarations[0].cves, vec!["CVE-2020-1"]);
            }
            other => panic!("expected Confirmed, got {other:?}"),
        }
    }

    #[test]
    fn a_same_named_attribute_building_something_else_is_rejected() {
        // The 9%. An attribute called `thing` exists, but it is not THIS
        // `thing`, and its declarations say nothing about this build.
        let c = candidate("/nix/store/zzz-thing-9.9", &["CVE-2020-1: boom"]);
        // CONTROL: the two really do differ, so the rejection below is about
        // the path and not about an empty comparison.
        assert_ne!(store_basename("/nix/store/zzz-thing-9.9"), "aaa-thing-1.0");
        assert_eq!(
            resolve_one(Some(&c), ["aaa-thing-1.0"]),
            AttributeResolution::PathMismatch,
            "a declaration from a different build must not be believed"
        );
    }

    #[test]
    fn comparing_unstripped_paths_would_resolve_nothing() {
        // Pins research R4 from both directions. The closure side omits the
        // `/nix/store/` prefix that the evaluation side carries, so a naive
        // string comparison yields zero matches across the board — which
        // presents as a broken mechanism rather than a formatting bug.
        let stored = "/nix/store/aaa-thing-1.0"; // as evaluation returns it
        let member = "aaa-thing-1.0"; // as the closure records it
        assert_ne!(stored, member, "the two forms differ, which is the trap");
        assert_eq!(store_basename(stored), store_basename(member));

        // And the resolver, which normalises, finds them equal.
        let c = candidate(stored, &[]);
        assert!(matches!(
            resolve_one(Some(&c), [member]),
            AttributeResolution::Confirmed { .. }
        ));
    }

    #[test]
    fn a_non_default_output_still_confirms() {
        // A derivation has several outputs; `outPath` is only the default.
        // Checking against one arbitrarily-chosen output rejects real matches.
        let c = candidate("/nix/store/aaa-thing-1.0", &[]);
        assert!(matches!(
            resolve_one(Some(&c), ["ddd-thing-1.0-dev", "aaa-thing-1.0", "mmm-thing-1.0-man"]),
            AttributeResolution::Confirmed { .. }
        ));
    }

    #[test]
    fn no_candidate_at_all_is_distinct_from_a_mismatch() {
        // Both report as unchecked, but they say different things about why,
        // and a later milestone widening the probed sets moves members
        // between them.
        assert_eq!(resolve_one(None, ["aaa-thing-1.0"]), AttributeResolution::NoAttribute);
    }

    #[test]
    fn a_confirmed_member_with_no_declarations_is_still_confirmed() {
        // The healthy majority: nixpkgs was asked and said nothing. That is a
        // real answer and must not read as "we could not ask".
        let c = candidate("/nix/store/aaa-thing-1.0", &[]);
        let r = resolve_one(Some(&c), ["aaa-thing-1.0"]);
        assert!(r.is_checked());
        match r {
            AttributeResolution::Confirmed { declarations, .. } => assert!(declarations.is_empty()),
            other => panic!("expected Confirmed, got {other:?}"),
        }
    }
}
