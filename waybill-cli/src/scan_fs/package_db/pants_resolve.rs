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
use std::collections::{BTreeMap, BTreeSet};

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

// -------------------------------------------------------------------
// Issue #914 (m912) — the Pants language namespace.
//
// `[python.resolves]` and `[jvm.resolves]` are separate namespaces in
// `pants.toml`, so one repository can legitimately declare `default` in both.
// Nothing recorded which section a resolve came from before this, which is
// why a bare resolve name cannot identify a document (FR-001a).
//
// **This is deliberately not an annotation.** It is a derivation input
// threaded in-process to the split, not a fact a consumer reads per
// component. Two constraints made that the right call:
//
//   - C143 (`waybill:pants-resolve`) ships a bare-name array as of v0.9.0,
//     and qualifying it in place would be a breaking value change, not the
//     additive one the contract promises.
//   - C161 (`waybill:resolve-ownership`) must stay byte-identical inside a
//     split document (FR-007 / SC-006), and it is populated by the Python
//     reader alone — teaching it about JVM resolves would change its value on
//     every JVM repository.
//
// So the namespace rides alongside as an index, and the ONE thing that
// reaches the wire is the document identity C163 derives from it.

/// The `pants.toml` section that declares a resolve.
///
/// A closed set rather than a string, per Constitution Principle IV: the
/// namespaces are fixed by Pants' own configuration schema, and a typo in a
/// string convention would produce an identity that looks right and matches
/// nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LanguageNamespace {
    /// `[python.resolves]` — Pex lockfiles, and uv lockfiles used as a Pants
    /// Python backend.
    Python,
    /// `[jvm.resolves]` — coursier lockfiles.
    Jvm,
}

impl LanguageNamespace {
    /// The wire spelling, which is also the `pants.toml` section name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Jvm => "jvm",
        }
    }
}

impl std::fmt::Display for LanguageNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolve name → the namespaces that declare or discover it.
///
/// Plural by necessity: a repository declaring `default` under both sections
/// maps that one name to both, which is the collision FR-001a exists for.
pub type NamespaceIndex = BTreeMap<String, BTreeSet<LanguageNamespace>>;

/// The qualified identity of one resolve, e.g. `python:default`.
///
/// The separator is `:` because it is what `pants.toml`'s own section paths
/// read like, and because neither a namespace nor a Pants resolve name may
/// contain one.
pub fn qualify(namespace: LanguageNamespace, resolve: &str) -> String {
    format!("{}:{}", namespace.as_str(), resolve)
}

/// Every qualified identity a bare resolve name maps to, lexically sorted.
///
/// Returns more than one only in the #919 collision case, where a document
/// genuinely represents two resolves and naming either alone would be false
/// (contract C-6). Returns empty when the name is unknown to the index, which
/// a caller must treat as "cannot identify" rather than substituting the bare
/// name — a half-qualified identity is the ambiguity this feature removes.
pub fn qualified_for(index: &NamespaceIndex, resolve: &str) -> Vec<String> {
    let mut out: Vec<String> = index
        .get(resolve)
        .map(|namespaces| namespaces.iter().map(|ns| qualify(*ns, resolve)).collect())
        .unwrap_or_default();
    // The explicit sort is load-bearing and was caught by its own test.
    // `BTreeSet<LanguageNamespace>` iterates in *discriminant* order, so this
    // returned `["python:default", "jvm:default"]` — deterministic, but not
    // lexical, and the doc comment above promised lexical. Milestone 671 hit
    // the identical trap with a language-grouped enum and the same fix.
    out.sort();
    out
}

/// Record that `resolve` exists under `namespace`.
pub fn index_insert(index: &mut NamespaceIndex, namespace: LanguageNamespace, resolve: &str) {
    index.entry(resolve.to_string()).or_default().insert(namespace);
}

/// Record every resolve named by `bags` as belonging to `namespace`.
///
/// Called once per reader with that reader's own output, so the namespace
/// comes from **which reader produced the entry** rather than from what its
/// members look like. Contract C-2 rejects inferring it from member PURL
/// ecosystem: that works on today's fixtures only because every fixture
/// resolve happens to be single-ecosystem, and a polyglot resolve would
/// silently mis-qualify.
pub fn index_record_all<'a, I>(index: &mut NamespaceIndex, namespace: LanguageNamespace, bags: I)
where
    I: IntoIterator<Item = &'a BTreeMap<String, Value>>,
{
    for bag in bags {
        for resolve in read(bag) {
            index_insert(index, namespace, &resolve);
        }
    }
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


    // --- m912: the language namespace ---

    #[test]
    fn qualify_reads_like_a_pants_section_path() {
        assert_eq!(qualify(LanguageNamespace::Python, "default"), "python:default");
        assert_eq!(qualify(LanguageNamespace::Jvm, "default"), "jvm:default");
    }

    /// FR-001a. The whole point of the namespace: one bare name, two
    /// resolves, two distinguishable identities.
    #[test]
    fn a_name_declared_in_both_namespaces_qualifies_to_both() {
        let mut idx = NamespaceIndex::new();
        index_insert(&mut idx, LanguageNamespace::Jvm, "default");
        index_insert(&mut idx, LanguageNamespace::Python, "default");
        assert_eq!(
            qualified_for(&idx, "default"),
            vec!["jvm:default", "python:default"]
        );
    }

    /// Sorted, so a document's identity is byte-stable across scans the way
    /// membership already is.
    #[test]
    fn qualified_for_is_order_independent() {
        let mut a = NamespaceIndex::new();
        index_insert(&mut a, LanguageNamespace::Python, "x");
        index_insert(&mut a, LanguageNamespace::Jvm, "x");
        let mut b = NamespaceIndex::new();
        index_insert(&mut b, LanguageNamespace::Jvm, "x");
        index_insert(&mut b, LanguageNamespace::Python, "x");
        assert_eq!(qualified_for(&a, "x"), qualified_for(&b, "x"));
    }

    /// An unknown name yields nothing rather than the bare name. Substituting
    /// the bare name would reintroduce exactly the ambiguity FR-001a removes,
    /// in the one case where we know we cannot resolve it.
    #[test]
    fn an_unknown_resolve_does_not_fall_back_to_the_bare_name() {
        assert!(qualified_for(&NamespaceIndex::new(), "default").is_empty());
    }



    /// The namespace comes from the reader, not from what the members look
    /// like — C-2 rejects ecosystem inference explicitly.
    #[test]
    fn index_record_all_takes_the_namespace_from_the_caller() {
        let bags = [bag(json!(["default", "lint"])), bag(json!(["default"]))];
        let mut idx = NamespaceIndex::new();
        index_record_all(&mut idx, LanguageNamespace::Jvm, bags.iter());
        assert_eq!(qualified_for(&idx, "default"), vec!["jvm:default"]);
        assert_eq!(qualified_for(&idx, "lint"), vec!["jvm:lint"]);
    }

    #[test]
    fn index_record_all_ignores_entries_without_membership() {
        let bags = [BTreeMap::new()];
        let mut idx = NamespaceIndex::new();
        index_record_all(&mut idx, LanguageNamespace::Python, bags.iter());
        assert!(idx.is_empty());
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
