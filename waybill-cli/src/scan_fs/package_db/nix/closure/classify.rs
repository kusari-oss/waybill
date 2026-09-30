//! How nix itself references each closure member.
//!
//! The artifact-versus-tooling split is not inferred. nix records it:
//! `buildInputs` is what goes into the thing being built, `nativeBuildInputs`
//! is what builds it. Reading those fields means no name heuristic, and no
//! list of "things that look like compilers" to maintain.

use std::collections::{BTreeMap, BTreeSet};

use super::derivation::{store_basename, RawClosure};

/// Fields naming dependencies that go *into* the artifact.
const ARTIFACT_FIELDS: &[&str] = &["buildInputs", "propagatedBuildInputs", "depsHostHost"];

/// Fields naming tooling that runs on the *build* machine.
const TOOLING_FIELDS: &[&str] = &[
    "nativeBuildInputs",
    "depsBuildBuild",
    "depsBuildHost",
    "nativeCheckInputs",
];

/// How nix referenced a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DerivationRole {
    /// Goes into the artifact.
    ArtifactInput,
    /// Builds the artifact but is not part of it.
    BuildTooling,
    /// Referenced both ways.
    Both,
    /// Referenced by neither — fetched sources, patches, setup hooks,
    /// bootstrap stages.
    ///
    /// Emphatically **not** "irrelevant": measured on two real closures, `jq`
    /// and `lua` land here and carry 6 of one project's 18 CVEs. Members in
    /// this role that apply a patch are in scope (spec FR-004a).
    Unreferenced,
}

impl DerivationRole {
    /// Combine the roles of two derivations that share one component
    /// identity.
    ///
    /// One closure holds several derivations per `(pname, version)` — build
    /// variants — and measured, 13 of 97 duplicated identities in a real
    /// closure disagree about their role: `flex 2.6.4` is `Both` as one
    /// variant and `Unreferenced` as another. Picking either would be a
    /// coin toss reported as a fact, so the flags are unioned.
    ///
    /// `Unreferenced` means neither flag, so it is absorbed rather than
    /// winning: a component referenced through any variant is referenced.
    pub(crate) fn union(self, other: Self) -> Self {
        let (a_in, a_tool) = self.flags();
        let (b_in, b_tool) = other.flags();
        match (a_in || b_in, a_tool || b_tool) {
            (true, true) => Self::Both,
            (true, false) => Self::ArtifactInput,
            (false, true) => Self::BuildTooling,
            (false, false) => Self::Unreferenced,
        }
    }

    /// `(goes into the artifact, builds the artifact)`.
    fn flags(self) -> (bool, bool) {
        match self {
            Self::ArtifactInput => (true, false),
            Self::BuildTooling => (false, true),
            Self::Both => (true, true),
            Self::Unreferenced => (false, false),
        }
    }

    /// The stable wire form for the `waybill:closure-role` annotation.
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::ArtifactInput => "artifact-input",
            Self::BuildTooling => "build-tooling",
            Self::Both => "both",
            Self::Unreferenced => "unreferenced",
        }
    }
}

/// Role for every member of a closure, keyed by `.drv` path.
pub(crate) fn classify(closure: &RawClosure) -> BTreeMap<String, DerivationRole> {
    let index = closure.output_index();
    let mut artifact: BTreeSet<&str> = BTreeSet::new();
    let mut tooling: BTreeSet<&str> = BTreeSet::new();

    for drv in closure.derivations.values() {
        for (fields, sink) in [(ARTIFACT_FIELDS, &mut artifact), (TOOLING_FIELDS, &mut tooling)] {
            for field in fields {
                for path in drv.paths_in(field) {
                    if let Some(key) = index.get(store_basename(path)) {
                        sink.insert(key);
                    }
                }
            }
        }
    }

    closure
        .derivations
        .keys()
        .map(|key| {
            let k = key.as_str();
            let role = match (artifact.contains(k), tooling.contains(k)) {
                (true, true) => DerivationRole::Both,
                (true, false) => DerivationRole::ArtifactInput,
                (false, true) => DerivationRole::BuildTooling,
                (false, false) => DerivationRole::Unreferenced,
            };
            (key.clone(), role)
        })
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// `app` takes `lib` as a build input and `cc` as native tooling; `both`
    /// is referenced each way; `orphan` by neither.
    const CLOSURE: &str = r#"{
      "derivations": {
        "d-lib.drv":    { "env": {"pname":"lib"},  "outputs": {"out":{"path":"h1-lib"}} },
        "d-cc.drv":     { "env": {"pname":"cc"},   "outputs": {"out":{"path":"h2-cc"}} },
        "d-both.drv":   { "env": {"pname":"both"}, "outputs": {"out":{"path":"h3-both"}} },
        "d-orphan.drv": { "env": {"pname":"orphan"}, "outputs": {"out":{"path":"h4-orphan"}} },
        "d-app.drv": {
          "env": { "pname":"app",
                   "buildInputs":"/nix/store/h1-lib /nix/store/h3-both",
                   "nativeBuildInputs":"/nix/store/h2-cc /nix/store/h3-both" },
          "outputs": {"out":{"path":"h5-app"}}
        }
      },
      "version": 3
    }"#;

    fn roles() -> BTreeMap<String, DerivationRole> {
        classify(&RawClosure::parse(CLOSURE).unwrap())
    }

    #[test]
    fn reads_the_split_from_nixs_own_fields() {
        let r = roles();
        // CONTROL: every member is classified, so a run that resolved nothing
        // cannot pass the assertions below by returning an empty map.
        assert_eq!(r.len(), 5);
        assert_ne!(
            r.values().filter(|v| **v != DerivationRole::Unreferenced).count(),
            0,
            "nothing was referenced — the basename join is broken, which is \
             silent rather than an error"
        );

        assert_eq!(r["d-lib.drv"], DerivationRole::ArtifactInput);
        assert_eq!(r["d-cc.drv"], DerivationRole::BuildTooling);
        assert_eq!(r["d-both.drv"], DerivationRole::Both);
        assert_eq!(r["d-orphan.drv"], DerivationRole::Unreferenced);
    }

    #[test]
    fn the_root_is_unreferenced_because_nothing_depends_on_it() {
        // Worth pinning: the thing being built is referenced by no other
        // member, so it lands in Unreferenced. A caller that treated that role
        // as "drop" would drop the artifact itself.
        assert_eq!(roles()["d-app.drv"], DerivationRole::Unreferenced);
    }

    /// Cross-check against a real closure, when one is available.
    ///
    /// The fixture above is synthetic and could be wrong about the shape nix
    /// actually emits. Point `WAYBILL_TEST_CLOSURE_JSON` at the output of
    /// `nix derivation show -r .#default` to check this classifier against
    /// it. The reference figures come from the committed Python classifier at
    /// `specs/1034-nix-eval-tier/measurements/`, which was validated against
    /// two real projects.
    #[test]
    fn agrees_with_a_real_closure_when_one_is_supplied() {
        let Ok(path) = std::env::var("WAYBILL_TEST_CLOSURE_JSON") else {
            eprintln!("skipping: set WAYBILL_TEST_CLOSURE_JSON to a closure dump");
            return;
        };
        let json = std::fs::read_to_string(&path).expect("closure json");
        let closure = RawClosure::parse(&json).expect("parses");
        let roles = classify(&closure);

        let count = |want: DerivationRole| roles.values().filter(|r| **r == want).count();
        let (artifact, tooling, both, unref) = (
            count(DerivationRole::ArtifactInput),
            count(DerivationRole::BuildTooling),
            count(DerivationRole::Both),
            count(DerivationRole::Unreferenced),
        );
        eprintln!(
            "{}: {} derivations -> artifact {artifact}, tooling {tooling}, \
             both {both}, unreferenced {unref}",
            path,
            closure.derivations.len()
        );

        // A real closure is in the low thousands with hundreds classified.
        // These bounds catch the failure that matters — a join that resolves
        // nothing, which yields every member unreferenced and no error.
        assert!(closure.derivations.len() > 100, "suspiciously small closure");
        assert!(
            artifact > 50,
            "only {artifact} artifact inputs: the basename join is resolving \
             nothing, which is silent"
        );
        assert!(tooling > 20, "only {tooling} build-tooling members");

        // SC-001: the closure contributes at least 200 components. Measured
        // after merging build variants: 348 on one project. Counted through
        // `emit::components` rather than from the role tallies, because the
        // tallies are per-derivation and the emitted set is per-identity —
        // asserting on the former would not notice the merge breaking.
        let emitted = super::super::emit::components(&super::super::ClassifiedClosure {
            attribute: "default".to_string(),
            raw: closure,
            roles,
        });
        eprintln!("emitted components after variant merge: {}", emitted.len());
        assert!(
            emitted.len() >= 200,
            "only {} components emitted; measured is 348",
            emitted.len()
        );
        // And they are distinct identities. A regression in the merge shows
        // up here as a count that is high for the wrong reason.
        let ids: std::collections::BTreeSet<String> = emitted
            .iter()
            .map(|c| format!("{}@{}", c.name, c.version))
            .collect();
        assert_eq!(
            ids.len(),
            emitted.len(),
            "{} duplicate identities among {} emitted components",
            emitted.len() - ids.len(),
            emitted.len()
        );
    }

    #[test]
    fn wire_forms_are_distinct_and_kebab_case() {
        let all = [
            DerivationRole::ArtifactInput,
            DerivationRole::BuildTooling,
            DerivationRole::Both,
            DerivationRole::Unreferenced,
        ];
        let wires: Vec<_> = all.iter().map(|r| r.wire()).collect();
        let mut sorted = wires.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "wire forms must be distinct: {wires:?}");
        for w in wires {
            assert!(w.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        }
    }
}
