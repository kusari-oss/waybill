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
//   - C161 (`waybill:resolve-ownership`) had to stay byte-identical inside a
//     split document (FR-007 / SC-006) while only the Python reader fed it.
//
// So the namespace rode alongside as an index, and the one thing that reached
// the wire was the document identity C163 derives from it. Milestone 1064
// (#924) then deliberately qualified C161's names with `qualify` and let the
// JVM reader feed it, superseding m912 SC-006's "unchanged" pin.

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

// -------------------------------------------------------------------
// Issue #919 (m922) — the namespace, PER COMPONENT.
//
// Milestone 912 recorded the namespace at DOCUMENT scope, which is enough to
// say "this document represents python:default and jvm:default" and not
// enough to say "this component belongs to the Python one". The split groups
// on the emitted component set, and that gap is what lets two unrelated
// resolves merge into one document.
//
// **Scalar, measured.** Across every corpus golden and every fixture, no
// component belongs to resolves in two namespaces (research R1). It cannot
// arise today: a component comes from one reader, the Python readers emit
// `pkg:pypi/*` and `pkg:generic/*`, the coursier reader emits `pkg:maven/*`,
// and dedup only unions components sharing a PURL — so the sets never meet.
//
// That is a property of which PURL types today's readers happen to emit, not
// an invariant anything enforces. So the impossible case is DETECTED rather
// than assumed away — see `namespace_conflict`. Guessing would file a
// component into the wrong resolve's document, which is this milestone's own
// defect one layer down.

/// The qualified identity of one resolve, e.g. `python:default`.
///
/// The separator is `:` because it is what `pants.toml`'s own section paths
/// read like, and because neither a namespace nor a Pants resolve name may
/// contain one. Single spelling of the identity, used by the split's grouping
/// and by the C163 document identity it feeds.
pub fn qualify(namespace: LanguageNamespace, resolve: &str) -> String {
    format!("{}:{}", namespace.as_str(), resolve)
}

/// Milestone 1064 (#924) — Pants's built-in default for a language whose
/// resolves the repository does not configure, as `(name, lockfile path)`.
/// Pants 2.31: `jvm/subsystems.py:66-72`,
/// `backend/python/subsystems/setup.py:187-237` (research R2).
pub(crate) fn pants_builtin_default(namespace: LanguageNamespace) -> (&'static str, &'static str) {
    match namespace {
        LanguageNamespace::Python => ("python-default", "3rdparty/python/default.lock"),
        LanguageNamespace::Jvm => ("jvm-default", "3rdparty/jvm/default.lock"),
    }
}

/// Milestone 1064 (#924) — whether Pants's built-in default resolve applies:
/// the repository has a readable `pants.toml` (it is a Pants repository) and
/// that file has no `<language>.resolves` table. An explicit table, even an
/// empty inline one, replaces the default, as it does in Pants.
pub(crate) fn builtin_default_applies(scan_root: &std::path::Path, namespace: LanguageNamespace) -> bool {
    let Ok(text) = std::fs::read_to_string(scan_root.join("pants.toml")) else {
        return false;
    };
    let Ok(doc) = text.parse::<toml::Table>() else {
        return false;
    };
    doc.get(namespace.as_str())
        .and_then(|section| section.get("resolves"))
        .is_none()
}

/// Milestone 1064 (#924) — the owning component's PURL:
/// `pkg:generic/<resolve>?pants-namespace=<namespace>`.
///
/// The qualifier keeps two same-named resolves in different namespaces
/// distinct (contracts/anchor-identity.md) while `purl.name()` stays the
/// resolve's name, which split filenames derive from. `pants-namespace` is
/// waybill's own qualifier key; there is no community convention yet (#1106).
pub(crate) fn anchor_purl(
    namespace: LanguageNamespace,
    resolve: &str,
) -> Option<waybill_common::types::purl::Purl> {
    waybill_common::types::purl::Purl::new(&format!(
        "pkg:generic/{}?pants-namespace={}",
        waybill_common::types::purl::encode_purl_segment(resolve),
        namespace.as_str()
    ))
    .ok()
}

/// The `pants-namespace` qualifier of an owning component's PURL; `None` for
/// every other PURL. Identity that compares PURLs without qualifiers must add
/// this, or same-named resolves in different namespaces merge.
pub(crate) fn anchor_namespace(purl: &waybill_common::types::purl::Purl) -> Option<&str> {
    if purl.ecosystem() != "generic" {
        return None;
    }
    let (_, query) = purl.as_str().split_once('?')?;
    let query = query.split('#').next().unwrap_or_default();
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("pants-namespace="))
}

/// Milestone 1064 (#924) — how the repository establishes that a resolve
/// exists. It decides whether the resolve gets an owning component and how
/// strongly it is classified (data-model.md `Declaration`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Declaration {
    /// Found by the lockfile-filename convention only. Not a declaration, so
    /// not anchored (m868 FR-003).
    Discovered,
    /// Pants's built-in default (`jvm-default` / `python-default` at the
    /// default lockfile path), applied when `pants.toml` exists and names no
    /// resolves for the language. Pants declares it even though the file
    /// does not spell it out.
    PantsDefault,
    /// JVM only: a `[<scope>].lockfile` path in any table other than `[jvm]`
    /// and `[python]` (Pants `JvmToolBase`, research R5). Named after the
    /// tool's scope.
    ToolLockfile,
    /// A key in `[<language>.resolves]`.
    Configured,
}

impl Declaration {
    /// Whether the resolve gets an owning component.
    pub(crate) fn is_declared(self) -> bool {
        !matches!(self, Self::Discovered)
    }

    /// The stronger of two declarations for the same lockfile:
    /// `Configured > ToolLockfile > PantsDefault > Discovered`.
    pub(crate) fn stronger(self, other: Self) -> Self {
        fn rank(d: Declaration) -> u8 {
            match d {
                Declaration::Discovered => 0,
                Declaration::PantsDefault => 1,
                Declaration::ToolLockfile => 2,
                Declaration::Configured => 3,
            }
        }
        if rank(other) > rank(self) {
            other
        } else {
            self
        }
    }
}

/// Per-component annotation key. Catalogue row C164.
pub(crate) const NAMESPACE_KEY: &str = "waybill:pants-resolve-namespace";

/// Build the per-component namespace value.
pub fn write_namespace(namespace: LanguageNamespace) -> Value {
    Value::String(namespace.as_str().to_string())
}

/// The component's namespace, or `None`.
///
/// `None` means "this component belongs to no Pants resolve" — or, after a
/// conflict, "we could not answer". Both are honest; neither is a guess.
pub fn read_namespace(annotations: &BTreeMap<String, Value>) -> Option<LanguageNamespace> {
    match annotations.get(NAMESPACE_KEY)?.as_str()? {
        "python" => Some(LanguageNamespace::Python),
        "jvm" => Some(LanguageNamespace::Jvm),
        other => {
            tracing::warn!(
                value = other,
                "unrecognised Pants language namespace on a component; treating it as \
                 absent rather than guessing. The namespace is a closed set."
            );
            None
        }
    }
}

/// Merge policy for the namespace when deduplication combines two components.
///
/// Returns the agreed value, or `None` when they disagree — the case research
/// R1 measured as currently unreachable. On disagreement this warns and yields
/// nothing, so the component ends up with no namespace and the split declines
/// to place it, loudly, rather than filing it into one of two resolves by
/// coin-flip.
pub fn namespace_conflict(existing: &Value, incoming: &Value) -> Option<Value> {
    if existing == incoming {
        return Some(existing.clone());
    }
    tracing::warn!(
        existing = %existing,
        incoming = %incoming,
        "two components merged with DIFFERENT Pants language namespaces. This is not \
         reachable with the current readers — a component comes from one reader and \
         the readers' PURL types do not overlap — so it means a new reader has broken \
         that assumption. The namespace is dropped rather than guessed; the component \
         will not be placed in a per-resolve document."
    );
    None
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


    // --- m922: the per-component namespace ---

    fn ns_bag(v: Value) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert(NAMESPACE_KEY.to_string(), v);
        m
    }

    #[test]
    fn namespace_round_trips() {
        for ns in [LanguageNamespace::Python, LanguageNamespace::Jvm] {
            assert_eq!(read_namespace(&ns_bag(write_namespace(ns))), Some(ns));
        }
    }

    #[test]
    fn an_absent_namespace_reads_as_none() {
        assert_eq!(read_namespace(&BTreeMap::new()), None);
    }

    /// The set is closed. An unrecognised value reads as absent rather than
    /// passing through, so a typo cannot become a third namespace that groups
    /// on its own.
    #[test]
    fn an_unrecognised_namespace_is_not_invented() {
        assert_eq!(read_namespace(&ns_bag(json!("kotlin"))), None);
    }

    #[test]
    fn agreeing_namespaces_merge_to_that_value() {
        let a = write_namespace(LanguageNamespace::Python);
        assert_eq!(namespace_conflict(&a, &a.clone()), Some(a));
    }

    /// C-3. Currently unreachable, deliberately detected anyway: guessing here
    /// would file a component into the wrong resolve's document, which is the
    /// defect this milestone fixes.
    #[test]
    fn disagreeing_namespaces_yield_nothing_rather_than_a_guess() {
        assert_eq!(
            namespace_conflict(
                &write_namespace(LanguageNamespace::Python),
                &write_namespace(LanguageNamespace::Jvm)
            ),
            None
        );
    }

    #[test]
    fn builtin_default_needs_pants_toml_and_no_resolves_table() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(!builtin_default_applies(root, LanguageNamespace::Jvm), "no pants.toml");
        std::fs::write(root.join("pants.toml"), "[jvm]\njdk = \"x\"\n").unwrap();
        assert!(builtin_default_applies(root, LanguageNamespace::Jvm));
        assert!(builtin_default_applies(root, LanguageNamespace::Python));
        std::fs::write(root.join("pants.toml"), "[jvm.resolves]\nmain = \"m.lock\"\n").unwrap();
        assert!(!builtin_default_applies(root, LanguageNamespace::Jvm));
        assert!(builtin_default_applies(root, LanguageNamespace::Python));
        std::fs::write(root.join("pants.toml"), "[python]\nresolves = {}\n").unwrap();
        assert!(!builtin_default_applies(root, LanguageNamespace::Python), "an empty table still replaces the default");
    }

    #[test]
    fn anchor_purl_carries_the_namespace_qualifier() {
        let py = anchor_purl(LanguageNamespace::Python, "default").unwrap();
        let jvm = anchor_purl(LanguageNamespace::Jvm, "default").unwrap();
        assert_eq!(py.as_str(), "pkg:generic/default?pants-namespace=python");
        assert_eq!(jvm.as_str(), "pkg:generic/default?pants-namespace=jvm");
        assert_eq!(anchor_namespace(&py), Some("python"));
        assert_eq!(anchor_namespace(&jvm), Some("jvm"));
        for other in [
            "pkg:generic/default",
            "pkg:generic/x@1?download_url=https://e.example/a",
            "pkg:deb/debian/libc6@2.36?arch=amd64",
        ] {
            let p = waybill_common::types::purl::Purl::new(other).unwrap();
            assert_eq!(anchor_namespace(&p), None, "{other}");
        }
        assert_ne!(py.as_str(), jvm.as_str());
        assert_eq!(py.name(), "default", "the name segment stays the resolve name");
    }

    #[test]
    fn only_discovered_is_undeclared() {
        assert!(!Declaration::Discovered.is_declared());
        for d in [Declaration::PantsDefault, Declaration::Configured] {
            assert!(d.is_declared(), "{d:?}");
        }
    }

    #[test]
    fn stronger_follows_the_precedence_in_either_order() {
        use Declaration::*;
        let order = [Discovered, PantsDefault, Configured];
        for (i, a) in order.iter().enumerate() {
            for (j, b) in order.iter().enumerate() {
                let want = order[i.max(j)];
                assert_eq!(a.stronger(*b), want, "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn qualify_reads_like_a_pants_section_path() {
        assert_eq!(qualify(LanguageNamespace::Python, "default"), "python:default");
        assert_eq!(qualify(LanguageNamespace::Jvm, "default"), "jvm:default");
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
