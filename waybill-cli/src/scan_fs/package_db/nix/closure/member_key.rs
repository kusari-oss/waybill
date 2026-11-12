//! How a closure derivation and the component it became are matched.
//!
//! Extracted from `contents.rs` for milestone 1423, where the scope pass needs
//! the same join to map a runtime store path to a component. One definition
//! rather than two, because two joins that have to agree are the drift this
//! project keeps paying for.

use std::collections::{BTreeSet, HashMap};

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::purl::Purl;

use super::derivation::{RawClosure, RawDerivation};
use super::emit;

/// The identity a closure member is matched on.
///
/// A member that declares a `pname` is matched on `(name, version)`, the join
/// the edge builder already uses and one that survives the identity passes
/// rewriting a PURL. A `pname`-less member is matched on the hash-bearing
/// PURL it was emitted under, because 56 of them are named `source` and share
/// the pair.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum MemberKey {
    Named(String, String),
    Hashed(String),
}

/// The key a derivation's component will carry.
pub(crate) fn key_of_derivation(drv: &RawDerivation, key: &str) -> Option<MemberKey> {
    if emit::is_hash_keyed_member(drv) {
        let (coordinate, _) = emit::coordinate(drv, key)?;
        let purl = Purl::new(&coordinate).ok()?;
        Some(MemberKey::Hashed(purl.as_str().to_string()))
    } else {
        Some(MemberKey::Named(
            drv.pname()?.to_string(),
            drv.version().unwrap_or_default().to_string(),
        ))
    }
}

/// The key of a component the closure tier emitted, or `None` for a
/// component that is not a closure member (it carries no C182 role).
pub(crate) fn key_of_component(c: &ResolvedComponent) -> Option<MemberKey> {
    c.extra_annotations.get(emit::ANN_CLOSURE_ROLE)?;
    if c.extra_annotations.contains_key(super::member::ANN_MEMBER_ROLE) {
        Some(MemberKey::Hashed(c.purl.as_str().to_string()))
    } else {
        Some(MemberKey::Named(c.name.clone(), c.version.clone()))
    }
}

/// Every derivation in the closure, grouped by the key its component would
/// carry.
///
/// **All derivations, not only the emitted ones.** A component is an
/// identity, and build variants share it: if any variant ships, the identity
/// ships. A derivation whose key no component carries simply joins to
/// nothing, which the caller counts.
pub(crate) fn drvs_by_key(raw: &RawClosure) -> HashMap<MemberKey, BTreeSet<String>> {
    let mut out: HashMap<MemberKey, BTreeSet<String>> = HashMap::new();
    for (key, drv) in &raw.derivations {
        if let Some(mk) = key_of_derivation(drv, key) {
            out.entry(mk).or_default().insert(key.clone());
        }
    }
    out
}
