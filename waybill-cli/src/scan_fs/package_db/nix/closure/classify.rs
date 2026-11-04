//! How nix itself references each closure member.
//!
//! The artifact-versus-tooling split is not inferred. nix records it:
//! `buildInputs` is what goes into the thing being built, `nativeBuildInputs`
//! is what builds it. Reading those fields means no name heuristic, and no
//! list of "things that look like compilers" to maintain.
//!
//! **Nothing in this file knows where those fields live.** It asks the
//! derivation record for them, and that record resolves from either of nix's
//! two attribute carriers. That is the design, not an accident of layering:
//! milestone 1278 (#1348) fixed a defect in which one carrier was never read,
//! and fixing it required no change here at all. Four consumers depend on
//! that resolution — this classifier, component emission, patch collection
//! and the nixpkgs declaration pass — and two of them resolving attributes
//! differently would disagree about the same derivation. So the resolution
//! belongs to the record and nowhere else (FR-007).

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

/// Every field the closure reader consults, for the FR-014 diagnostic.
///
/// Derived from the same two constants classification reads, plus `patches`,
/// which the patch collector reads. Assembled here rather than written out a
/// second time: a hand-maintained copy would drift, and the drift would show
/// up as the disclosure naming a field that *is* read — a false alarm in the
/// mechanism whose whole job is to be trustworthy when it stays silent.
pub(crate) fn consulted_fields() -> Vec<&'static str> {
    ARTIFACT_FIELDS
        .iter()
        .chain(TOOLING_FIELDS)
        .copied()
        .chain(std::iter::once("patches"))
        .collect()
}

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
    /// Milestone 1277 FR-004a / SC-009 — do these two field lists cover the
    /// dependency keys real derivations actually carry?
    ///
    /// The `Unreferenced` role is derived by **absence** from
    /// `ARTIFACT_FIELDS` (3 names) and `TOOLING_FIELDS` (4 names), while
    /// nixpkgs' stdenv sets more dependency attributes than those seven. If
    /// any of the others appears in a real derivation then `unreferenced`
    /// conflates "nothing references it" with "we did not read the
    /// reference" — and milestone 1277 assigns a non-runtime marker to that
    /// role, which would be unsound for such a member.
    ///
    /// **This cannot be answered from the in-tree fixtures.** They carry
    /// only the seven names the code already knows, because they were
    /// written from these lists, so a grep over them returns the answer it
    /// was built from. It needs a real closure, hence a probe.
    ///
    /// `#[ignore]`d rather than early-returning on a missing env var: a
    /// gated test that prints `skipping` and reports `ok` is
    /// indistinguishable from one that ran, and the neighbouring probe below
    /// predates that rule.
    ///
    /// ```sh
    /// WAYBILL_TEST_CLOSURE_JSON=/path/to/closure.json \
    ///   cargo test -p waybill --bin waybill m1277_field_coverage -- --ignored --nocapture
    /// ```
    ///
    /// SC-009 asks for the count to be **known and recorded**, not to be
    /// zero. A non-zero answer is a finding to scope, not a test failure.
    #[test]
    #[ignore = "FR-004a probe; needs a real closure via WAYBILL_TEST_CLOSURE_JSON"]
    fn m1277_field_coverage() {
        let path = std::env::var("WAYBILL_TEST_CLOSURE_JSON")
            .expect("set WAYBILL_TEST_CLOSURE_JSON to a real closure dump");
        let json = std::fs::read_to_string(&path).expect("closure json");
        let closure = RawClosure::parse(&json).expect("parses");

        // Every env key whose value names at least one store path, counted
        // by how many derivations carry it. Store paths are what a
        // dependency attribute holds, so this is the shape worth auditing;
        // keys like `pname` and `buildPhase` are excluded by it.
        let mut carriers: BTreeMap<&str, usize> = BTreeMap::new();
        for drv in closure.derivations.values() {
            for (key, value) in &drv.env {
                if value.contains("/nix/store/") {
                    *carriers.entry(key.as_str()).or_default() += 1;
                }
            }
        }

        let known: BTreeSet<&str> =
            ARTIFACT_FIELDS.iter().chain(TOOLING_FIELDS.iter()).copied().collect();

        // nixpkgs' stdenv dependency attributes, the only keys whose absence
        // from the two lists can misclassify a member.
        //
        // **The distinction matters, and the first version of this probe got
        // it wrong.** A store path appears in `builder` (677 derivations
        // here), `buildCommand`, `checkPhase`, `installPhase`, and in the
        // `bin` / `dev` / `doc` / `dist` keys that name a multi-output
        // derivation's *own* outputs. None is a dependency. Counting all of
        // them said 287 derivations were referenced through an unread key,
        // which is an upper bound on nothing in particular — a patch file
        // named in `patches` is not a dependency the classifier missed.
        const DEPENDENCY_ATTRS: &[&str] = &[
            "buildInputs",
            "checkInputs",
            "depsBuildBuild",
            "depsBuildBuildPropagated",
            "depsBuildHost",
            "depsBuildHostPropagated",
            "depsBuildTarget",
            "depsBuildTargetPropagated",
            "depsHostHost",
            "depsHostHostPropagated",
            "depsHostTarget",
            "depsHostTargetPropagated",
            "depsTargetTarget",
            "depsTargetTargetPropagated",
            "installCheckInputs",
            "nativeBuildInputs",
            "nativeCheckInputs",
            "nativeInstallCheckInputs",
            "propagatedBuildInputs",
            "propagatedNativeBuildInputs",
        ];

        eprintln!(
            "\n{}: {} derivations\n",
            path,
            closure.derivations.len()
        );
        eprintln!("store-path-carrying env keys, and whether a field list names them:");
        for (key, n) in &carriers {
            eprintln!(
                "  {:<34} {:>5} derivations   {}",
                key,
                n,
                if known.contains(*key) { "READ" } else { "NOT READ" }
            );
        }

        // The answer SC-009 asks for: dependency attributes this closure
        // carries that neither field list names.
        let unread_deps: Vec<(&&str, &usize)> = carriers
            .iter()
            .filter(|(k, _)| DEPENDENCY_ATTRS.contains(*k) && !known.contains(**k))
            .collect();
        eprintln!(
            "\nFR-004a: {} store-path-carrying keys in all; {} are nixpkgs dependency \
             attributes; {} of THOSE are named by neither field list:",
            carriers.len(),
            carriers.iter().filter(|(k, _)| DEPENDENCY_ATTRS.contains(*k)).count(),
            unread_deps.len()
        );
        for (key, n) in &unread_deps {
            eprintln!("  {key:<34} {n:>5} derivations");
        }

        // The population FR-004's marker would be unsound for: members
        // classified `Unreferenced` that are in fact referenced through a
        // dependency attribute neither list reads.
        let roles = classify(&closure);
        let index = closure.output_index();
        let unread_dep_keys: BTreeSet<&str> =
            unread_deps.iter().map(|(k, _)| **k).collect();
        let mut misclassified: Vec<String> = Vec::new();
        for (key, drv) in &closure.derivations {
            if roles.get(key).copied() != Some(DerivationRole::Unreferenced) {
                continue;
            }
            let referenced = closure.derivations.values().any(|other| {
                unread_dep_keys.iter().any(|k| {
                    other.paths_in(k).any(|p| {
                        index.get(store_basename(p)).map(|id| *id == key.as_str())
                            == Some(true)
                    })
                })
            });
            if referenced {
                misclassified.push(drv.pname().unwrap_or(key).to_string());
            }
        }
        misclassified.sort();
        misclassified.dedup();
        eprintln!(
            "\nunreferenced members actually referenced through an unread DEPENDENCY \
             attribute: {}",
            misclassified.len()
        );
        if !misclassified.is_empty() {
            eprintln!("  {misclassified:?}");
        }
        eprintln!();
    }

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
            read_outcome: Default::default(),
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
