//! Classifying and identifying the closure members that declare no `pname`.
//!
//! **Not [`super::classify`], and the difference is the one that falsified a
//! clarification.** `classify.rs` assigns the milestone-1277 C182 role —
//! `artifact-input` / `build-tooling` / `both` / `unreferenced` — to every
//! *derivation* in the closure, all 1,749 of them, by walking the attribute
//! fields that name store paths. This module assigns a different vocabulary to
//! the subset that becomes *components* under milestone 1282. The two coexist;
//! neither is derived from the other, and a member carries both.
//!
//! The confusion is easy and was expensive: a clarification question was put
//! on the premise that the C182 census described only named members, and the
//! golden showed it counts all 1,749 (research R4). Separate files make the
//! distinction visible at the import site.
//!
//! ## What was actually hiding these members
//!
//! Not a `pname` test. [`super::derivation::RawDerivation::pname`] falls back
//! to the derivation's own `name`, and every one of the 1,145 names is
//! non-empty, so that accessor returns `Some` for all of them. The gate is
//! `emit::emits_as_component`, which admits a member on its C182 role —
//! and all 1,145 are `unreferenced` non-patch-appliers. Read from the
//! committed golden: 1,315 `unreferenced` derivations yield **11** components
//! (research R8a). An implementation that edits a `pname` test changes
//! nothing and compiles clean.

use super::derivation::RawDerivation;

/// C206 (m1282) — which of four kinds a `pname`-less closure member is.
pub(crate) const ANN_MEMBER_ROLE: &str = "waybill:closure-member-role";

/// Which of four kinds a `pname`-less closure member is.
///
/// An enum rather than a string because the role selects both the identity
/// basis and the edge eligibility: a typo in a string would silently change
/// what a component is called *and* whether the graph can reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum MemberRole {
    /// A fixed-output fetch — the tarballs and patches the build consumed.
    /// 549 on the pinned closure, and the clearest part of the gap: a
    /// consumer asking "what source did this come from" gets silence today.
    Source,
    /// A version-bearing bootstrap package (`mes-libc-mini-0.27.1`). 83.
    Package,
    /// Spec 1380 (#1374): a version check (`bash-get-version-5.3`) or test
    /// program (`gcc-simple-program-15.2.0`), which verifies a bootstrap
    /// stage rather than being one. 46 on moat, where all of them were
    /// `package` before. A graph endpoint like `package`, so reclassifying
    /// them moves no edge.
    Check,
    /// `writeText` output — `itoa-builder`, `setup-hook.sh`. 267.
    Scaffolding,
    /// A live-bootstrap C function — `strncpy`, `memcpy`, `malloc`. 246.
    Fragment,
}

impl MemberRole {
    /// The wire string a consumer sees.
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Package => "package",
            Self::Check => "check",
            Self::Scaffolding => "scaffolding",
            Self::Fragment => "fragment",
        }
    }

    /// FR-013 — only these may be dependency-edge endpoints. Spec 1380 adds
    /// `check`, split out of `package`, so the endpoint set is unchanged.
    ///
    /// The projection stops at them and walks *through* the other two, so the
    /// emitted graph shows which source a package was built from rather than
    /// which build script stood between them. The 513 scaffolding and
    /// fragment components are orphans by construction as a result, which is
    /// why FR-014 requires each to carry a reason saying so.
    pub(crate) fn is_edge_endpoint(self) -> bool {
        matches!(self, Self::Source | Self::Package | Self::Check)
    }

    /// Every variant, for the FR-009 count record.
    ///
    /// An array rather than a `matches!` allowlist at each call site: a
    /// variant added later is a compile error in the array's length, and
    /// silently absent from an allowlist (`feedback_enum_allowlist_falls_out_of_date`).
    pub(crate) const ALL: [Self; 5] = [
        Self::Source,
        Self::Package,
        Self::Check,
        Self::Scaffolding,
        Self::Fragment,
    ];
}

/// What a member's identity is built from.
///
/// Two variants rather than `Option<ContentHash>` so the merging behaviour is
/// a property of the type. An `Option` lets a caller forget that `None` still
/// needs an identity and fall back to a name-keyed component — which is
/// exactly the collision FR-002 forbids, and which the existing
/// `(name, version)` merge key would produce for all 56 members named
/// `source`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum IdentityBasis {
    /// The derivation records a content hash. Two members sharing a name and
    /// this hash **merge** into one component: the same content fetched twice
    /// is one artifact (FR-003). Measured: 549 members become 502 components.
    Content(String),
    /// No content hash. The derivation's own store-path hash — nix's
    /// content-address of the build recipe — keeps it distinct (FR-004).
    /// Measured collision-free over all 1,145.
    Recipe(String),
}

impl IdentityBasis {
    /// The hash itself, whichever basis supplied it.
    ///
    /// **Untruncated, deliberately.** R6 measured zero collisions over *full*
    /// hashes; collision-freedom over a truncated prefix is a different claim
    /// needing its own measurement, and the closure capture that measurement
    /// would need is not available here. A conventional prefix length adopted
    /// without measuring it would assume precisely what has to be shown.
    pub(crate) fn hash(&self) -> &str {
        match self {
            Self::Content(h) | Self::Recipe(h) => h,
        }
    }
}

/// The content hash a fixed-output derivation records, under either spelling.
///
/// **Both carriers, and that is load-bearing.** `outputs[<name>].hash` is the
/// structured form and carries 549 of them; `env.outputHash` is the older
/// spelling and carries 139. Reading only the latter undercounts the
/// content-addressed population **fourfold** — which is how 549 was once
/// measured as 139 (measurements README, probe error 2).
pub(crate) fn content_hash(drv: &RawDerivation) -> Option<&str> {
    drv.outputs
        .values()
        .find_map(|o| o.hash.as_deref())
        .or_else(|| drv.env.get("outputHash").map(String::as_str))
}

/// Does this name encode a version, in the shape a bootstrap package uses?
///
/// `mes-libc-mini-0.27.1` does; `strncpy` does not. Deliberately not reading
/// the derivation's top-level `version` key: that is the derivation *format*
/// version, `"4"` for all 1,749 nodes, and reading it as a package version
/// makes every bootstrap fragment look versioned including `strncpy`
/// (measurements README, probe error 3).
fn name_encodes_a_version(name: &str) -> bool {
    // `-<digits>` followed by any number of `.<digits>` groups, matching the
    // probe's `-[0-9]+(\.[0-9]+)*`. Hand-rolled rather than a `regex` so the
    // classifier stays a pure function over one record with no lazy static.
    let bytes = name.as_bytes();
    for (i, w) in bytes.windows(2).enumerate() {
        if w[0] != b'-' || !w[1].is_ascii_digit() {
            continue;
        }
        // A run of digits, then zero or more `.<digits>` groups.
        let mut j = i + 1;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        loop {
            if j + 1 < bytes.len() && bytes[j] == b'.' && bytes[j + 1].is_ascii_digit() {
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
            } else {
                break;
            }
        }
        return true;
    }
    false
}

/// Spec 1380 (#1374) research R2 — the store basenames every closure
/// derivation lists in `patches`, in either carrier. Built once per closure.
///
/// A member whose output is listed here is a patch, by the closure's own
/// record rather than by its name.
#[derive(Debug, Default)]
pub(crate) struct PatchOutputs<'a>(std::collections::BTreeSet<&'a str>);

impl<'a> PatchOutputs<'a> {
    pub(crate) fn of(closure: &'a super::derivation::RawClosure) -> Self {
        Self(
            closure
                .derivations
                .values()
                .flat_map(|d| d.paths_in("patches"))
                .map(super::derivation::store_basename)
                .collect(),
        )
    }

    fn lists(&self, drv: &RawDerivation) -> bool {
        drv.output_paths()
            .any(|p| self.0.contains(super::derivation::store_basename(p)))
    }
}

/// Spec 1380 research R3 — nixpkgs `srcOnly`: unpack, patch, copy out.
fn is_src_only(drv: &RawDerivation) -> bool {
    let phases = match drv.attr_str("phases") {
        Some(p) => p.split_whitespace().collect::<Vec<_>>(),
        None => drv
            .structured_attrs
            .as_ref()
            .and_then(|sa| sa.get("phases"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default(),
    };
    phases == ["unpackPhase", "patchPhase", "installPhase"]
}

/// Spec 1380 research R4 — a source tree with no derivation evidence:
/// `clang-src-21.1.8` (a `runCommand` copy), `mes-src-0.27.1` and
/// `tinycc-…-source` (`kaem`), `cargo-auditable-0.7.2-vendor`.
fn names_a_source_tree(name: &str) -> bool {
    name.ends_with("-source")
        || name.ends_with("-vendor")
        || name
            .rsplit_once("-src-")
            .is_some_and(|(_, v)| v.starts_with(|c: char| c.is_ascii_digit()) && !v.contains('-'))
}

/// Spec 1380 research R1 — a version check or test program. Nothing in the
/// derivation marks one, so the name is all there is: the segment, then a
/// version or an `unstable-` date.
fn names_a_check(name: &str) -> bool {
    ["-get-version-", "-simple-program-"].iter().any(|seg| {
        name.split_once(seg).is_some_and(|(_, v)| {
            v.starts_with(|c: char| c.is_ascii_digit()) || v.starts_with("unstable-")
        })
    })
}

/// Which of the five kinds this member is.
///
/// **Total** (FR-005): never errors, never declines, every input gets exactly
/// one role. The four counts must therefore sum to the member count, which is
/// how the probe confirmed there is no residue.
///
/// **The evidence order is fixed and load-bearing**: content hash, then
/// `env.text`, then a version-shaped name, then fragment. A member bearing
/// several signals takes the strongest. Reversing the middle two describes a
/// different classifier producing different counts — the data-model's role
/// table had them the other way round until the analyze pass caught it
/// against this probe.
///
/// Spec 1380 (#1374) splits the version-name rung and touches nothing else
/// (research R5): evidence first (listed in `patches`, then `srcOnly`), then
/// the source-tree names, then the check names, and `package` for the rest.
/// Every outcome of that rung is a graph endpoint, so the split moves no edge.
pub(crate) fn classify(drv: &RawDerivation, patches: &PatchOutputs<'_>) -> MemberRole {
    if content_hash(drv).is_some() {
        return MemberRole::Source;
    }
    if drv.env.contains_key("text") {
        return MemberRole::Scaffolding;
    }
    let name = drv.name.as_deref().unwrap_or_default();
    if name_encodes_a_version(name) {
        if patches.lists(drv) || is_src_only(drv) || names_a_source_tree(name) {
            return MemberRole::Source;
        }
        if names_a_check(name) {
            return MemberRole::Check;
        }
        return MemberRole::Package;
    }
    MemberRole::Fragment
}

/// Whether this member is a dependency-graph endpoint (FR-013).
///
/// Asked without the closure's patch set, and the answer cannot differ from
/// [`classify`]'s with it. The patch set is consulted only inside the
/// version-name rung, and every outcome of that rung (`source`, `check`,
/// `package`) is an endpoint. `the_patch_set_never_changes_the_endpoint_answer`
/// holds that.
pub(crate) fn is_edge_endpoint(drv: &RawDerivation) -> bool {
    classify(drv, &PatchOutputs::default()).is_edge_endpoint()
}

/// This member's identity basis (FR-003 / FR-004).
///
/// `drv_key` is the closure map key — a `.drv` store path whose leading
/// component is nix's content-address of the build recipe. It is present for
/// every node, including the ones carrying no content hash of their own,
/// which is what makes FR-004 satisfiable at all.
pub(crate) fn identity(drv: &RawDerivation, drv_key: &str) -> IdentityBasis {
    match content_hash(drv) {
        Some(h) => IdentityBasis::Content(h.to_string()),
        None => IdentityBasis::Recipe(recipe_hash(drv_key).to_string()),
    }
}

/// The leading hash of a `.drv` store path.
///
/// `/nix/store/<hash>-<name>.drv` → `<hash>`. Falls back to the whole key
/// rather than to an empty string: an empty identity would collide with every
/// other empty one, turning a parsing surprise into the collision FR-002
/// exists to prevent.
fn recipe_hash(drv_key: &str) -> &str {
    let base = super::derivation::store_basename(drv_key);
    if let Some((hash, _)) = base.split_once('-') {
        if !hash.is_empty() {
            return hash;
        }
    }
    // Longest non-empty material wins, in order. The basename can itself be
    // empty — `store_basename("/nix/store/")` is `""` — and an empty identity
    // collides with every other empty one, turning a parsing surprise into
    // the collision FR-002 exists to prevent. Found by the test below rather
    // than by reasoning about it.
    if !base.is_empty() {
        return base;
    }
    drv_key
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::super::derivation::RawClosure;
    use super::*;

    fn drv(json: &str) -> RawDerivation {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn m1282_t011_classification_is_total_and_reproduces_the_distribution() {
        let raw = RawClosure::parse(include_str!(
            "../../../../../tests/fixtures/nix_closure_members/closure.json"
        ))
        .unwrap();
        let nameless: Vec<_> = raw
            .derivations
            .values()
            .filter(|d| d.declared_pname().is_none())
            .collect();

        let mut counts = std::collections::BTreeMap::new();
        for d in &nameless {
            *counts.entry(classify(d, &PatchOutputs::default()).wire()).or_insert(0usize) += 1;
        }

        // The four counts, not merely the total: a classifier returning one
        // role for everything still sums correctly, and would pass a
        // total-only assertion while emitting a document in which `strncpy`
        // and a tarball are indistinguishable.
        assert_eq!(counts.get("source").copied().unwrap_or(0), 5, "source");
        assert_eq!(counts.get("package").copied().unwrap_or(0), 1, "package");
        assert_eq!(
            counts.get("scaffolding").copied().unwrap_or(0),
            2,
            "scaffolding"
        );
        assert_eq!(counts.get("fragment").copied().unwrap_or(0), 2, "fragment");
        assert_eq!(
            counts.values().sum::<usize>(),
            nameless.len(),
            "every member must receive exactly one role, with no residue"
        );
    }

    #[test]
    fn m1282_t012_a_content_hash_outranks_env_text() {
        // `r##` rather than `r#`: the shebang's `"#` would close a `r#` literal.
        let d = drv(
            r##"{"name":"both","env":{"text":"#!/bin/sh"},
                "outputs":{"out":{"path":"p","hash":"deadbeef"}}}"##,
        );
        assert_eq!(classify(&d, &PatchOutputs::default()), MemberRole::Source);
    }

    #[test]
    fn m1282_t012_env_text_outranks_a_version_shaped_name() {
        // The case the data-model table had backwards. Only the probe and the
        // contract were right, and nothing tested it until now.
        let d = drv(r#"{"name":"helper-1.2.3","env":{"text":"echo"}}"#);
        assert_eq!(classify(&d, &PatchOutputs::default()), MemberRole::Scaffolding);
    }

    #[test]
    fn m1282_t012_a_version_shaped_name_with_no_other_signal_is_a_package() {
        assert_eq!(
            classify(&drv(r#"{"name":"mes-libc-mini-0.27.1","env":{}}"#), &PatchOutputs::default()),
            MemberRole::Package
        );
    }

    #[test]
    fn m1282_t012_a_bare_name_is_a_fragment() {
        assert_eq!(classify(&drv(r#"{"name":"strncpy","env":{}}"#), &PatchOutputs::default()), MemberRole::Fragment);
    }

    #[test]
    fn m1282_t013_identity_reads_the_hash_under_either_spelling() {
        let structured = drv(r#"{"name":"s","outputs":{"out":{"path":"p","hash":"sha256-aaa"}}}"#);
        assert_eq!(
            identity(&structured, "/nix/store/rrr-s.drv"),
            IdentityBasis::Content("sha256-aaa".into())
        );

        // The older spelling. Reading only this one undercounts the
        // content-addressed population fourfold.
        let legacy = drv(r#"{"name":"s","env":{"outputHash":"sha256-bbb"}}"#);
        assert_eq!(
            identity(&legacy, "/nix/store/rrr-s.drv"),
            IdentityBasis::Content("sha256-bbb".into())
        );
    }

    #[test]
    fn m1282_t013_a_member_with_no_content_hash_falls_back_to_the_recipe() {
        let d = drv(r#"{"name":"strncpy","env":{}}"#);
        assert_eq!(
            identity(&d, "/nix/store/abc123-strncpy.drv"),
            IdentityBasis::Recipe("abc123".into())
        );
    }

    #[test]
    fn m1282_t050_classification_reads_one_record_and_nothing_else() {
        // FR-006 — no network, no second evaluation, no maintained name list.
        // Asserted by construction rather than by observation: the test holds
        // a derivation record and no fixture, no closure, no path and no
        // revision, so there is nothing for `classify` to consult but its
        // argument. A classifier that reached outside it could not compile
        // against this call.
        let d = drv(r#"{"name":"strncpy","env":{}}"#);
        assert_eq!(classify(&d, &PatchOutputs::default()), MemberRole::Fragment);
        assert!(!classify(&d, &PatchOutputs::default()).is_edge_endpoint());
    }

    #[test]
    fn m1282_only_sources_and_packages_are_edge_endpoints() {
        assert!(MemberRole::Source.is_edge_endpoint());
        assert!(MemberRole::Package.is_edge_endpoint());
        // Spec 1380 — `check` is split out of `package` and keeps its place.
        assert!(MemberRole::Check.is_edge_endpoint());
        assert!(!MemberRole::Scaffolding.is_edge_endpoint());
        assert!(!MemberRole::Fragment.is_edge_endpoint());
    }

    #[test]
    fn m1282_the_recipe_hash_never_degrades_to_an_empty_identity() {
        // An empty identity collides with every other empty one, which turns
        // a parsing surprise into the collision FR-002 exists to prevent.
        for key in ["-noname.drv", "/nix/store/", "notastorepath", "/nix/store/h-.drv"] {
            let basis = identity(&drv(r#"{"name":"x","env":{}}"#), key);
            if let IdentityBasis::Recipe(h) = &basis {
                assert!(!h.is_empty(), "empty recipe hash for key {key:?}");
            } else {
                panic!("expected a recipe basis");
            }
        }
    }

    #[test]
    fn m1282_an_empty_drv_key_is_the_one_case_with_no_unique_material() {
        // Recorded rather than papered over. Identity for a member with no
        // content hash is derived from the closure map key; an empty key
        // offers nothing to derive from, and synthesising a constant would
        // be worse than an empty string — it would collide just as surely
        // while looking deliberate.
        //
        // It is out of FR-017's scope, which covers a member whose *name* is
        // absent or unusable, not one whose key is. A `BTreeMap` key here
        // always comes from `nix derivation show`, which emits a store path.
        assert_eq!(
            identity(&drv(r#"{"name":"x","env":{}}"#), ""),
            IdentityBasis::Recipe(String::new())
        );
    }

    #[test]
    fn m1282_t051_a_member_with_no_name_is_still_uniquely_identified() {
        // FR-017 — defensive: measured as zero occurrences in the closure
        // under test, so the test constructs one rather than waiting to meet
        // it. Dropping the member would make the FR-009 counts fail to
        // reconcile, which is why absence must not be a skip.
        let anon = drv(r#"{"env":{}}"#);
        assert_eq!(classify(&anon, &PatchOutputs::default()), MemberRole::Fragment);
        assert_eq!(
            identity(&anon, "/nix/store/anon123-.drv"),
            IdentityBasis::Recipe("anon123".into())
        );
    }

    #[test]
    fn m1282_a_version_shaped_name_is_told_from_a_bare_one() {
        for yes in ["mes-libc-mini-0.27.1", "x-1", "foo-2.3", "a-b-10.0.1"] {
            assert!(name_encodes_a_version(yes), "{yes} should look versioned");
        }
        for no in ["strncpy", "memcpy", "setup-hook.sh", "source", "crt-builder", ""] {
            assert!(!name_encodes_a_version(no), "{no} should not look versioned");
        }
    }

    // ---- Spec 1380 (#1374) ----

    /// The pre-1380 ladder, kept only so T005 can compare against it.
    fn classify_before_1380(drv: &RawDerivation) -> MemberRole {
        if content_hash(drv).is_some() {
            return MemberRole::Source;
        }
        if drv.env.contains_key("text") {
            return MemberRole::Scaffolding;
        }
        if name_encodes_a_version(drv.name.as_deref().unwrap_or_default()) {
            return MemberRole::Package;
        }
        MemberRole::Fragment
    }

    fn none() -> PatchOutputs<'static> {
        PatchOutputs::default()
    }

    fn role(json: &str) -> MemberRole {
        classify(&drv(json), &none())
    }

    /// One member per rung, plus a consumer that lists two patches: one with
    /// a version token (rung 3a) and one without (stays a fragment, R5).
    const CASES: &str = r#"{"derivations":{
      "c.drv":{"name":"waybill-fixture-consumer-1.0","env":{"pname":"waybill-fixture-consumer",
        "patches":"/nix/store/h1-waybill-fixture-fix-16.patch /nix/store/h2-waybill-fixture-local.patch"},
        "outputs":{"out":{"path":"hc-waybill-fixture-consumer-1.0"}}},
      "p.drv":{"name":"waybill-fixture-fix-16.patch","env":{"dontUnpack":"1"},"outputs":{"out":{"path":"h1-waybill-fixture-fix-16.patch"}}},
      "q.drv":{"name":"waybill-fixture-local.patch","env":{"dontUnpack":"1"},"outputs":{"out":{"path":"h2-waybill-fixture-local.patch"}}},
      "s.drv":{"name":"waybill-fixture-ghc-9.10.3-tree","env":{"phases":"unpackPhase patchPhase installPhase"},"outputs":{"out":{"path":"hs"}}},
      "t.drv":{"name":"waybill-fixture-tree-1.2","structuredAttrs":{"phases":["unpackPhase","patchPhase","installPhase"]},"outputs":{"out":{"path":"ht"}}},
      "r.drv":{"name":"waybill-fixture-clang-src-21.1.8","env":{"buildCommand":"cp -r"},"outputs":{"out":{"path":"hr"}}},
      "v.drv":{"name":"waybill-fixture-tool-0.7.2-vendor","env":{},"outputs":{"out":{"path":"hv"}}},
      "o.drv":{"name":"waybill-fixture-tinycc-unstable-2025-12-03-source","env":{"PATH":"x"},"outputs":{"out":{"path":"ho"}}},
      "g.drv":{"name":"waybill-fixture-bash-get-version-5.3","env":{"buildCommand":"x"},"outputs":{"out":{"path":"hg"}}},
      "u.drv":{"name":"waybill-fixture-tcc-boot-get-version-unstable-2024-07-07","env":{"PATH":"x"},"outputs":{"out":{"path":"hu"}}},
      "x.drv":{"name":"waybill-fixture-gcc-cxx-simple-program-4.6.4","env":{"buildCommand":"x"},"outputs":{"out":{"path":"hx"}}},
      "k.drv":{"name":"waybill-fixture-libc-mini-0.27.1","env":{"PATH":"x"},"outputs":{"out":{"path":"hk"}}},
      "e.drv":{"name":"waybill-fixture-python3-3.13.12-env","env":{},"outputs":{"out":{"path":"he"}}},
      "f.drv":{"name":"waybill-fixture-src-3510200.zip","outputs":{"out":{"hash":"sha256-AAAA","method":"flat"}}}
    }}"#;

    fn cases() -> RawClosure {
        RawClosure::parse(CASES).unwrap()
    }

    /// T005 — only `package` members move, and only to endpoint roles. So
    /// the endpoint set, and with it the graph, is the same as before.
    #[test]
    fn s1380_only_package_members_move_and_only_between_endpoints() {
        let raw = cases();
        let patches = PatchOutputs::of(&raw);
        let mut moved = 0;
        for d in raw.derivations.values().filter(|d| d.declared_pname().is_none()) {
            let (before, after) = (classify_before_1380(d), classify(d, &patches));
            if before != after {
                moved += 1;
                assert_eq!(before, MemberRole::Package, "{:?} moved from {before:?}", d.name);
            }
            assert_eq!(before.is_edge_endpoint(), after.is_edge_endpoint(), "{:?}", d.name);
        }
        assert!(moved >= 9, "CONTROL: the fixture must exercise every new rung, moved {moved}");
    }

    /// The endpoint helper agrees with the full classification, with the
    /// patch set or without it.
    #[test]
    fn s1380_the_patch_set_never_changes_the_endpoint_answer() {
        let raw = cases();
        let patches = PatchOutputs::of(&raw);
        for d in raw.derivations.values() {
            assert_eq!(is_edge_endpoint(d), classify(d, &patches).is_edge_endpoint(), "{:?}", d.name);
        }
    }

    /// T006 — US2: patches, `srcOnly` trees and named source trees are
    /// `source`, and keep a recipe identity (FR-003).
    #[test]
    fn s1380_source_trees_and_patches_are_source() {
        let raw = cases();
        let patches = PatchOutputs::of(&raw);
        let at = |k: &str| classify(&raw.derivations[k], &patches);
        assert_eq!(at("p.drv"), MemberRole::Source, "listed in patches (E1)");
        assert_eq!(at("s.drv"), MemberRole::Source, "srcOnly in env (E2)");
        assert_eq!(at("t.drv"), MemberRole::Source, "srcOnly in structuredAttrs (E2)");
        assert_eq!(at("r.drv"), MemberRole::Source, "-src-<v>");
        assert_eq!(at("v.drv"), MemberRole::Source, "-vendor");
        assert_eq!(at("o.drv"), MemberRole::Source, "-source");
        assert!(matches!(identity(&raw.derivations["r.drv"], "r.drv"), IdentityBasis::Recipe(_)));
        // R5: a listed patch with no version token is outside the split.
        assert_eq!(at("q.drv"), MemberRole::Fragment);
        // Controls: a fetch is `source` by rung 1; a genuine bootstrap package
        // and an environment stay `package`.
        assert_eq!(at("f.drv"), MemberRole::Source);
        assert_eq!(at("k.drv"), MemberRole::Package);
        assert_eq!(at("e.drv"), MemberRole::Package);
        // Without the patch set, the versioned patch is a package by name.
        assert_eq!(classify(&raw.derivations["p.drv"], &none()), MemberRole::Package);
    }

    /// T008 — US3: version checks and test programs are `check`.
    #[test]
    fn s1380_checks_are_check() {
        let raw = cases();
        let patches = PatchOutputs::of(&raw);
        for k in ["g.drv", "u.drv", "x.drv"] {
            assert_eq!(classify(&raw.derivations[k], &patches), MemberRole::Check, "{k}");
        }
        // A check segment with no version after it is not a check.
        assert_eq!(role(r#"{"name":"waybill-fixture-get-version-tool-1.0","env":{}}"#), MemberRole::Package);
        // Evidence outranks the check name (R6).
        let both = r#"{"derivations":{
          "c.drv":{"name":"w-1","env":{"pname":"w","patches":"/nix/store/hz-waybill-fixture-get-version-1.0"}},
          "z.drv":{"name":"waybill-fixture-get-version-1.0","env":{},"outputs":{"out":{"path":"hz-waybill-fixture-get-version-1.0"}}}}}"#;
        let raw = RawClosure::parse(both).unwrap();
        assert_eq!(classify(&raw.derivations["z.drv"], &PatchOutputs::of(&raw)), MemberRole::Source);
    }

    #[test]
    fn s1380_the_wire_value_and_the_count_set() {
        assert_eq!(MemberRole::Check.wire(), "check");
        assert_eq!(MemberRole::ALL.len(), 5);
    }
}
