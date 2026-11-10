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
///
/// The two cross-compilation triples are here rather than on the tooling side
/// (#1346, FR-010), and the reason is what the alternative **asserts**.
/// `depsTargetTarget` names dependencies of the *target* platform: what a
/// compiler being built hands to things it later compiles. Milestone 1277
/// emits CycloneDX `scope: "excluded"` for tooling roles — a positive claim
/// that the member is absent from the artifact — and a cross-compiler ships
/// its target libc inside its own output, so that claim can be false. That is
/// the defect class of #1342 and #1348. `artifact-input` asserts no scope at
/// all, which is the honest answer when waybill cannot tell.
///
/// The cost is accepted and recorded: `artifact-input` now covers two
/// distinguishable things. A fifth role value is the follow-up if a genuinely
/// cross closure makes that expensive — it would change the C182 closed enum,
/// every consumer of it, invariant I10's population and C203's vocabulary.
const ARTIFACT_FIELDS: &[&str] = &[
    "buildInputs",
    "propagatedBuildInputs",
    "depsHostHost",
    "depsTargetTarget",
    "depsTargetTargetPropagated",
];

/// Fields naming tooling that runs on the *build* machine.
///
/// `depsBuildTarget` is tooling by the same reading as `depsBuildBuild`: it
/// runs on the build machine to produce target-platform output (FR-011).
///
/// `propagatedNativeBuildInputs` is the one whose absence was not local.
/// It propagates to consumers of the declaring derivation, so leaving it
/// unread understated reachability by more than its own declaration count —
/// a member reachable only through a chain of them read as though nothing
/// depended on it.
///
/// **Transitivity needs no extra pass, and the reason is worth stating
/// precisely** (T024): `classify` below is not a walk from a root. It is a
/// flat scan over *every* derivation in the closure, crediting whatever each
/// one names. So a member reached only through two hops of propagation is
/// credited by the second hop's own declaration, at whatever depth it sits,
/// and adding these attributes to the constant is the whole of the fix. A
/// root-anchored walk would have needed a propagation phase; this does not.
const TOOLING_FIELDS: &[&str] = &[
    "nativeBuildInputs",
    "depsBuildBuild",
    "depsBuildHost",
    "nativeCheckInputs",
    "depsBuildTarget",
    "propagatedNativeBuildInputs",
];

/// nixpkgs attributes that inject **stdenv's own machinery**, deliberately
/// left unclassified (#1346, FR-012).
///
/// They hold most of the unread declarations — 208 of 230 on the pinned
/// x86_64-linux closure — and they are not a package declaring a dependency.
/// Classifying them would move the role census substantially and make this
/// milestone's effect unattributable; their members get the FR-002 marker
/// instead, which is what #1346 actually asks for: that `unreferenced` stop
/// meaning two things.
///
/// **All three are named even though only two are populated on any one
/// system.** `extraNativeBuildInputs` is populated on linux and empty on
/// darwin; `defaultBuildInputs` is the reverse. The first draft of FR-012
/// enumerated the darwin pair and was therefore incomplete on the closure the
/// goldens pin — the two systems do not merely differ in magnitude, they
/// populate different attributes.
/// **Test-only, and that is the point.** No production code path consults
/// this list: FR-012's exclusion is an *absence* from the two constants
/// above, not a rule applied on top of them. A production reference here
/// would turn "we do not classify these" into a behaviour that could be
/// wrong, and SC-011 — a stdenv-named member has the same role with and
/// without the marker — is only trivially true while nothing branches on it.
/// It exists so the exclusion is asserted rather than assumed.
#[cfg(test)]
const STDENV_INJECTED_FIELDS: &[&str] = &[
    "defaultNativeBuildInputs",
    "defaultBuildInputs",
    "extraNativeBuildInputs",
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
/// Milestone 1372 (#1366) — the derivations some other derivation's build
/// actually consumed.
///
/// **This is not the same question the two field lists above ask.** They ask
/// which derivations a *declared dependency attribute* names;
/// `inputs.drvs` is nix's own record of what each build consumed, and the two
/// disagree on 741 of the pinned closure's 1,275 derivations.
///
/// Measured: `gzip 1.14` is named by three derivations' `inputs.drvs` and by
/// no dependency attribute of any of them — it appears in `postInstall`,
/// `initialPath` and `allowedRequisites`, which are a script body and two
/// pieces of stdenv bookkeeping. Before this, `gzip` was `Unreferenced`, which
/// told a reader that nothing in the build depended on it.
///
/// **V1 — a derivation naming itself does not count.** Self-reference is not
/// evidence that something else consumed it. Not observed on the pinned
/// closure, so it is excluded here rather than left to be discovered as a
/// silent self-admission.
///
/// **V2 — the referrer need not itself be emitted.** A derivation consumed
/// only by something that is not emitted was still consumed. Requiring an
/// emitted referrer would make the signal depend on its own output.
///
/// **V3 — a key that resolves to nothing is ignored.** `inputs.drvs` keys are
/// `.drv` paths that are keys of `derivations`; milestone 1280 measured
/// dangling ones and has a fixture for them.
fn graph_referrers(closure: &RawClosure) -> BTreeSet<&str> {
    let mut referred: BTreeSet<&str> = BTreeSet::new();
    for (key, drv) in &closure.derivations {
        for dep in drv.inputs.drvs.keys() {
            // V1
            if dep == key {
                continue;
            }
            // V3 — `get_key_value` so the borrow is the map's key, not `dep`.
            if let Some((k, _)) = closure.derivations.get_key_value(dep) {
                referred.insert(k.as_str());
            }
        }
    }
    referred
}

/// Milestone 1372 (#1366) — the derivations the **build graph alone** put in
/// a role, i.e. those no declared dependency attribute names.
///
/// This is what C211 discloses and what C210 counts. It is a separate
/// function rather than a second return value from [`classify`] because
/// `classify` has ten call sites and none of the other nine wants this; the
/// shared work is in [`declared_sets`], so neither duplicates the field scan.
pub(crate) fn graph_admitted(closure: &RawClosure) -> BTreeSet<String> {
    let (artifact, tooling) = declared_sets(closure);
    let graph = graph_referrers(closure);
    closure
        .derivations
        .keys()
        .filter(|k| {
            let k = k.as_str();
            !artifact.contains(k) && !tooling.contains(k) && graph.contains(k)
        })
        .cloned()
        .collect()
}

/// The two sets the declared dependency attributes produce.
///
/// Extracted so [`classify`] and [`graph_admitted`] share one scan of the
/// field lists; a second copy would be free to drift from the first, and the
/// drift would show up as a component disclosing an admission signal that
/// contradicts its own role.
fn declared_sets(closure: &RawClosure) -> (BTreeSet<&str>, BTreeSet<&str>) {
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
    (artifact, tooling)
}

pub(crate) fn classify(closure: &RawClosure) -> BTreeMap<String, DerivationRole> {
    let graph = graph_referrers(closure);
    let (artifact, tooling) = declared_sets(closure);

    closure
        .derivations
        .keys()
        .map(|key| {
            let k = key.as_str();
            let role = match (artifact.contains(k), tooling.contains(k)) {
                (true, true) => DerivationRole::Both,
                (true, false) => DerivationRole::ArtifactInput,
                (false, true) => DerivationRole::BuildTooling,
                // Milestone 1372 (#1366) — only this arm consults the build
                // graph, and that bound is FR-003a: a declared attribute
                // always wins, so a derivation any dependency field names
                // keeps exactly the role it had. The graph answers only where
                // nothing else does.
                //
                // `BuildTooling` rather than `Both`: every attribute measured
                // as naming these derivations is build-time — `postInstall`,
                // `installPhase`, `buildPhase`, `initialPath`,
                // `allowedRequisites`, `stdenv`. That supports one claim;
                // `Both` would assert a second nothing supports.
                //
                // The limit, recorded because it is real: `allowedRequisites`
                // is nixpkgs' assertion about the RUNTIME closure, so a
                // derivation named only there is arguably a runtime requisite
                // rather than tooling. An undifferentiated signal cannot
                // separate the two.
                (false, false) if graph.contains(k) => DerivationRole::BuildTooling,
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

    // ---- m1372 (#1366): the build-graph admission signal ----

    /// V1 — a derivation naming itself proves nothing about who consumed it.
    ///
    /// Constructed, because the pinned closure contains no self-reference: a
    /// test written against it would pass with or without the guard.
    #[test]
    fn m1372_self_reference_is_not_a_referrer() {
        const SELF: &str = r#"{
          "derivations": {
            "d-self.drv": {
              "env": {"pname":"selfref"},
              "outputs": {"out":{"path":"h-self"}},
              "inputs": {"drvs": {"d-self.drv": {"outputs":["out"]}}}
            }
          }
        }"#;
        let raw = RawClosure::parse(SELF).unwrap();
        assert!(
            graph_referrers(&raw).is_empty(),
            "a derivation that names only itself has no referrer"
        );
        assert_eq!(
            classify(&raw)["d-self.drv"],
            DerivationRole::Unreferenced,
            "and so it stays unreferenced"
        );
    }

    /// V2 — a referrer that is not itself emitted still counts.
    ///
    /// Requiring an emitted referrer would make the signal depend on its own
    /// output, which is circular and is not what `inputs.drvs` records.
    #[test]
    fn m1372_a_referrer_need_not_be_emitted_itself() {
        const C: &str = r#"{
          "derivations": {
            "d-mid.drv": {
              "env": {},
              "outputs": {"out":{"path":"h-mid"}},
              "inputs": {"drvs": {"d-leaf.drv": {"outputs":["out"]}}}
            },
            "d-leaf.drv": { "env": {"pname":"leaf"}, "outputs": {"out":{"path":"h-leaf"}} }
          }
        }"#;
        let raw = RawClosure::parse(C).unwrap();
        assert!(graph_referrers(&raw).contains("d-leaf.drv"));
        assert_eq!(classify(&raw)["d-leaf.drv"], DerivationRole::BuildTooling);
    }

    /// V3 — an `inputs.drvs` key naming no derivation is ignored.
    #[test]
    fn m1372_a_dangling_input_key_is_ignored() {
        const C: &str = r#"{
          "derivations": {
            "d-app.drv": {
              "env": {"pname":"app"},
              "outputs": {"out":{"path":"h-app"}},
              "inputs": {"drvs": {"d-gone.drv": {"outputs":["out"]}}}
            }
          }
        }"#;
        let raw = RawClosure::parse(C).unwrap();
        assert!(
            graph_referrers(&raw).is_empty(),
            "a key resolving to no derivation must not enter the set"
        );
    }

    /// **FR-003a / SC-004a — the precedence rule, and the one the corpus
    /// cannot test.**
    ///
    /// Every derivation a declared attribute names already has a
    /// non-`Unreferenced` role on the pinned closure, so a mutation that
    /// applied the graph signal to all four arms would leave the corpus
    /// entirely green. Only a constructed closure catches it.
    #[test]
    fn m1372_a_declared_attribute_always_wins() {
        const C: &str = r#"{
          "derivations": {
            "d-lib.drv": { "env": {"pname":"lib"}, "outputs": {"out":{"path":"h-lib"}} },
            "d-app.drv": {
              "env": {"pname":"app", "buildInputs":"/nix/store/h-lib"},
              "outputs": {"out":{"path":"h-app"}},
              "inputs": {"drvs": {"d-lib.drv": {"outputs":["out"]}}}
            }
          }
        }"#;
        let raw = RawClosure::parse(C).unwrap();
        // The graph names it AND `buildInputs` names it.
        assert!(graph_referrers(&raw).contains("d-lib.drv"));
        assert_eq!(
            classify(&raw)["d-lib.drv"],
            DerivationRole::ArtifactInput,
            "a declared attribute must win; the graph answers only where \
             nothing else does"
        );
    }

    /// US2 — the signal admits where no attribute does.
    #[test]
    fn m1372_a_script_body_reference_earns_build_tooling() {
        const C: &str = r#"{
          "derivations": {
            "d-gzip.drv": { "env": {"pname":"gzip","version":"1.14"},
                            "outputs": {"out":{"path":"h-gzip"}} },
            "d-app.drv": {
              "env": {"pname":"app",
                      "postInstall":"${/nix/store/h-gzip}/bin/gzip -9 $out/thing"},
              "outputs": {"out":{"path":"h-app"}},
              "inputs": {"drvs": {"d-gzip.drv": {"outputs":["out"]}}}
            }
          }
        }"#;
        let raw = RawClosure::parse(C).unwrap();
        assert_eq!(
            classify(&raw)["d-gzip.drv"],
            DerivationRole::BuildTooling,
            "named only in a script body, but the build consumed it"
        );
    }

    /// SC-004 — the closure root, which nothing consumes, stays honest.
    #[test]
    fn m1372_the_root_stays_unreferenced() {
        const C: &str = r#"{
          "derivations": {
            "d-root.drv": {
              "env": {"pname":"root"},
              "outputs": {"out":{"path":"h-root"}},
              "inputs": {"drvs": {"d-dep.drv": {"outputs":["out"]}}}
            },
            "d-dep.drv": { "env": {"pname":"dep"}, "outputs": {"out":{"path":"h-dep"}} }
          }
        }"#;
        let raw = RawClosure::parse(C).unwrap();
        let roles = classify(&raw);
        assert_eq!(roles["d-dep.drv"], DerivationRole::BuildTooling);
        assert_eq!(
            roles["d-root.drv"],
            DerivationRole::Unreferenced,
            "nothing consumes the root, and the label must keep saying so"
        );
    }

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
            runtime: None,
            attribute: "default".to_string(),
            raw: closure,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
            identifiers: Default::default(),
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

    // ---- m1279 (#1346): the consulted set ----

    #[test]
    fn m1279_the_consulted_set_is_the_two_constants_and_patches() {
        // T028's recorded property, asserted. `read_outcome` derives what
        // counts as a gap from this function, and this function derives from
        // the two constants the classifier reads -- so widening a constant
        // narrows the disclosure in the same edit, and the two cannot drift.
        // A second hand-maintained list is the drift that would make the
        // disclosure claim a gap the classifier had already closed.
        let consulted = consulted_fields();
        for field in ARTIFACT_FIELDS.iter().chain(TOOLING_FIELDS) {
            assert!(consulted.contains(field), "{field} must be consulted");
        }
        assert_eq!(
            consulted.len(),
            ARTIFACT_FIELDS.len() + TOOLING_FIELDS.len() + 1,
            "`patches` is the one entry that is not a dependency attribute; \
             anything else here is a field the classifier does not read"
        );
    }

    #[test]
    fn m1279_the_newly_consulted_attributes_are_on_the_side_the_contract_says() {
        // FR-010 / FR-011. The two target triples are artifact-side on
        // purpose: `build-tooling` would make m1277 emit `scope: "excluded"`,
        // a positive claim that a cross-compiler's target libc is absent
        // from the artifact -- which can be false. Asserted here because the
        // cheapest way to undo that reasoning is to move one string.
        for artifact_side in ["depsTargetTarget", "depsTargetTargetPropagated"] {
            assert!(
                ARTIFACT_FIELDS.contains(&artifact_side),
                "{artifact_side} must stay artifact-side; moving it to tooling \
                 asserts an absence waybill cannot verify"
            );
            assert!(!TOOLING_FIELDS.contains(&artifact_side));
        }
        for tooling_side in ["depsBuildTarget", "propagatedNativeBuildInputs"] {
            assert!(TOOLING_FIELDS.contains(&tooling_side), "{tooling_side}");
            assert!(!ARTIFACT_FIELDS.contains(&tooling_side));
        }
    }

    #[test]
    fn m1279_the_stdenv_defaults_stay_unclassified() {
        // FR-012 / P6. All three are named, though only two are populated on
        // any one system: `extraNativeBuildInputs` holds paths on linux and
        // none on darwin, `defaultBuildInputs` the reverse. The first draft
        // of this exclusion enumerated the darwin pair and was therefore
        // incomplete on the closure the goldens pin -- the two systems do not
        // merely differ in magnitude, they populate different attributes.
        for excluded in STDENV_INJECTED_FIELDS {
            assert!(
                !ARTIFACT_FIELDS.contains(excluded) && !TOOLING_FIELDS.contains(excluded),
                "{excluded} must not be classified: it is stdenv injecting its \
                 own machinery, not a package declaring a dependency, and \
                 classifying it would move the census this milestone has to \
                 attribute to four specific attributes"
            );
            assert!(
                !consulted_fields().contains(excluded),
                "{excluded} must therefore still be reported as a gap when it \
                 is populated -- disclosure instead of classification is the \
                 whole of FR-012"
            );
        }
        assert_eq!(STDENV_INJECTED_FIELDS.len(), 3);
    }
}
