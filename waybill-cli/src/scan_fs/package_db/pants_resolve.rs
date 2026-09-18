//! Issue #911 (#902 item 1) — resolve membership: the set of Pants resolves
//! that pin a package.
//!
//! The relation is many-to-many. The same package at the same version is
//! routinely pinned by several resolves, so the annotation carries a
//! **lexically sorted JSON array** rather than a bare name.
//!
//! Every read and write of the annotation goes through this module. Four
//! sites used to read it directly with `.as_str()`, and two of them carried
//! milestone 910's edge-scoping fix — an array would have made all four
//! return `None` and fall through to a default, reverting #910 with nothing
//! failing.
//!
//! **A trap this module exists to prevent.** The value's cardinality depends
//! on *where* you read it:
//!
//! | read site | cardinality |
//! |---|---|
//! | a `PackageDbEntry`, before dedup | always exactly one — an entry comes from one lockfile |
//! | an emitted component, after dedup | one *or more* — dedup unions membership |
//!
//! Edges are emitted at `scan_fs/mod.rs:1081`, before `deduplicate` at
//! `:1253`. So edge scoping sees the singular form and always will; only the
//! emitted document sees the plural one. Code that assumes plurality at scan
//! time is reading a shape that cannot occur.

// Deliberately not in `pants_common`: that module is BUILD-file walking
// helpers shared by the pants_shell and pants_go readers. This one is about
// an annotation, and its consumers include `scan_fs/mod.rs`, which is not a
// Pants reader at all.

use serde_json::Value;
use std::collections::BTreeMap;

/// The per-component annotation key. Catalogue row C143.
pub(crate) const ANNOTATION_KEY: &str = "waybill:pants-resolve";

/// Read membership from an annotation bag.
///
/// Accepts the array form and the pre-#911 bare-string form. The tolerance is
/// deliberate but narrow: it exists so the reader migration and the writer
/// migration can land in separate commits without the tree being broken in
/// between, NOT as a lasting compatibility shim.
///
/// It is explicitly **not** the thing that catches a writer left on the old
/// form — a lenient reader would mask that. The output assertion does: no
/// emitted document may contain a bare-string value for this key.
pub(crate) fn read(annotations: &BTreeMap<String, Value>) -> Vec<String> {
    match annotations.get(ANNOTATION_KEY) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// The single resolve an entry belongs to, or `None`.
///
/// For scan-time code that legitimately expects one — an entry comes from one
/// lockfile. Returns `None` when membership is absent OR plural, because a
/// plural value here means an assumption has been violated and silently using
/// the first element would hide it.
pub(crate) fn read_single(annotations: &BTreeMap<String, Value>) -> Option<String> {
    let names = read(annotations);
    match names.len() {
        1 => names.into_iter().next(),
        0 => None,
        n => {
            tracing::warn!(
                count = n,
                "resolve membership is plural at a site that expects exactly one — \
                 entries come from a single lockfile, so this means membership was \
                 unioned earlier than dedup. Edge scoping is skipped for this entry \
                 rather than guessing which resolve was meant."
            );
            None
        }
    }
}

/// Build the annotation value: lexically sorted, deduplicated.
///
/// Sorting is not cosmetic. Two scans of one repository must produce
/// byte-identical membership (FR-003), and read order is the thing that
/// varies between them.
pub(crate) fn write<I, S>(names: I) -> Value
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut v: Vec<String> = names.into_iter().map(Into::into).collect();
    v.sort();
    v.dedup();
    Value::Array(v.into_iter().map(Value::String).collect())
}

/// Union two membership values, for the dedup merge.
///
/// Order-independent by construction: the result does not depend on which
/// component won the merge.
pub(crate) fn union(a: &Value, b: &Value) -> Value {
    let mut names: Vec<String> = Vec::new();
    for value in [a, b] {
        match value {
            Value::Array(items) => {
                names.extend(items.iter().filter_map(|v| v.as_str()).map(str::to_string))
            }
            Value::String(s) => names.push(s.clone()),
            _ => {}
        }
    }
    write(names)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;

    fn bag(v: Value) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert(ANNOTATION_KEY.to_string(), v);
        m
    }

    #[test]
    fn read_accepts_the_array_form() {
        assert_eq!(read(&bag(json!(["app", "tools"]))), vec!["app", "tools"]);
    }

    /// Narrow migration tolerance — see the doc comment on `read`.
    #[test]
    fn read_accepts_the_pre_911_bare_string() {
        assert_eq!(read(&bag(json!("app"))), vec!["app"]);
    }

    #[test]
    fn read_of_an_absent_key_is_empty_not_a_panic() {
        assert!(read(&BTreeMap::new()).is_empty());
    }

    #[test]
    fn write_sorts_and_dedups() {
        assert_eq!(write(["tools", "app", "tools"]), json!(["app", "tools"]));
    }

    /// FR-003. If this regresses, two scans of one repository stop agreeing
    /// and the goldens flap without any input changing.
    #[test]
    fn write_is_order_independent() {
        assert_eq!(write(["tools", "app"]), write(["app", "tools"]));
    }

    #[test]
    fn union_is_commutative_and_sorted() {
        let a = json!(["tools"]);
        let b = json!(["app"]);
        assert_eq!(union(&a, &b), json!(["app", "tools"]));
        assert_eq!(union(&a, &b), union(&b, &a));
    }

    #[test]
    fn union_drops_duplicates() {
        assert_eq!(union(&json!(["app"]), &json!(["app"])), json!(["app"]));
    }

    #[test]
    fn read_single_returns_the_one_resolve_an_entry_has() {
        assert_eq!(read_single(&bag(json!(["app"]))), Some("app".to_string()));
    }

    /// A plural value at a scan-time site means an assumption was violated.
    /// Returning `None` makes edge scoping fall back rather than silently
    /// picking a resolve the caller never chose.
    #[test]
    fn read_single_refuses_to_guess_when_membership_is_plural() {
        assert_eq!(read_single(&bag(json!(["app", "tools"]))), None);
    }
}
