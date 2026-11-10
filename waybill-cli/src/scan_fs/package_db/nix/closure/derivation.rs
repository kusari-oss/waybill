//! Parsing `nix derivation show -r` output.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

/// One derivation from the closure.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct RawDerivation {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    /// The typed attribute carrier, present when nixpkgs set
    /// `__structuredAttrs = true`.
    ///
    /// **This is the second place a derivation's attributes can live, and not
    /// reading it is #1348.** `nix derivation show` emits a flat `env` map of
    /// strings normally; under structured attributes the values move here with
    /// their types intact — lists stay lists — and `env` is left holding only
    /// the output paths. Measured on the `nix-closure-moat` closure, 421 of
    /// 1,749 derivations declared their dependencies only here, so 554
    /// dependency entries read as absent. Absent is indistinguishable from
    /// "nothing depends on this", which is why the symptom was a large
    /// `unreferenced` population that looked like a fact about nix.
    ///
    /// Held as a [`Value`] rather than a typed struct because its members are
    /// heterogeneous by design: `buildInputs` is a list, `doCheck` a boolean,
    /// `env` a nested object, `outputChecks` a map. Resolution is therefore
    /// total over `Value` and cannot panic on a shape nixpkgs adds later.
    #[serde(default, rename = "structuredAttrs")]
    pub(crate) structured_attrs: Option<Value>,
    #[serde(default)]
    pub(crate) outputs: BTreeMap<String, RawOutput>,
    /// Milestone 1280 (#1346) — the derivations whose outputs this build
    /// consumed, as nix records them.
    ///
    /// **This is the build graph, and not reading it is why the closure tier
    /// contributed no dependency edges at all.** The declared attributes above
    /// (`buildInputs` and the rest) are what a package *says* it needs, and
    /// they feed the C182 role; this is what the build *used*. Measured on the
    /// `nix-closure-moat` closure, `inputs.drvs` holds 10,342 edges against the
    /// declared view's 3,386 and explains 1,748 of 1,749 derivations against
    /// 434 — and its count on the darwin closure, 8,230, equals sbomnix's
    /// `dependsOn` total to the edge, which is how we know the other tool
    /// emits this graph and waybill did not.
    ///
    /// **Only the keys are read.** The value carries `outputs` and
    /// `dynamicOutputs`, which say *which* output was consumed; the edge is
    /// between derivations either way, and reading more would invite a second
    /// notion of identity.
    ///
    /// **`inputs.srcs` is deliberately not parsed.** A `src` is a store path,
    /// not a derivation, so it names no node in this graph. Including it would
    /// produce edges to things that are not components.
    #[serde(default)]
    pub(crate) inputs: RawInputs,
}

/// The `inputs` object of a derivation (milestone 1280).
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RawInputs {
    /// `.drv` path → `{outputs, dynamicOutputs}`. Only the keys are used.
    #[serde(default)]
    pub(crate) drvs: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawOutput {
    #[serde(default)]
    pub(crate) path: Option<String>,
    /// The content hash a fixed-output derivation records (milestone 1282).
    ///
    /// **The structured spelling, and the one that carries the population.**
    /// `env.outputHash` is the older form and appears on 139 of the 549
    /// fixed-output members; this field carries all 549. Reading only the
    /// older spelling undercounts the content-addressed population fourfold,
    /// which is how 549 was once measured as 139 (measurements README).
    #[serde(default)]
    pub(crate) hash: Option<String>,
}

/// The top-level shape: `{"derivations": {...}, "version": N}`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawClosure {
    pub(crate) derivations: BTreeMap<String, RawDerivation>,
    /// The output format version nix reported.
    ///
    /// Parsed and discarded before #1348, which is how the move from version 3
    /// to version 4 — the move that introduced the typed carrier — landed
    /// unnoticed. It is recorded in the emitted document (FR-012) and
    /// deliberately **never** consulted when resolving an attribute (FR-013):
    /// a version allowlist is an allowlist, and this project has already been
    /// bitten by one falling out of date.
    #[serde(default)]
    pub(crate) version: Option<u64>,
}

impl RawDerivation {
    /// A string-valued attribute, from whichever carrier holds it.
    ///
    /// The flat map wins a disagreement (FR-006). Measured: no field on either
    /// closure examined is present in both carriers, so the precedence is a
    /// defined default rather than an observed preference — which is why
    /// [`Self::carrier_disagreements`] exists to report it if it ever happens
    /// instead of letting the choice pass silently.
    fn attr_str(&self, field: &str) -> Option<&str> {
        if let Some(v) = self.env.get(field) {
            return Some(v.as_str());
        }
        self.structured_attrs
            .as_ref()
            .and_then(|sa| sa.get(field))
            .and_then(Value::as_str)
    }

    /// Which carrier holds `field`, for diagnostics.
    ///
    /// Defined immediately beside [`Self::attr_str`] and matching its order
    /// deliberately: this exists so `waybill debug closure-dump` can report
    /// where a value came from, and a reporter that drifted from the reader
    /// would be worse than none. A probe that read only the flat `env` map
    /// called 66 of one closure's 1,275 derivations nameless and reported a
    /// milestone-1282 defect that did not exist.
    pub(crate) fn attr_carrier(&self, field: &str) -> &'static str {
        if self.env.contains_key(field) {
            return "env";
        }
        if self
            .structured_attrs
            .as_ref()
            .and_then(|sa| sa.get(field))
            .and_then(Value::as_str)
            .is_some()
        {
            return "structuredAttrs";
        }
        "none"
    }

    /// The package name, preferring `pname` over the derivation's own name.
    pub(crate) fn pname(&self) -> Option<&str> {
        self.attr_str("pname").or(self.name.as_deref())
    }

    pub(crate) fn version(&self) -> Option<&str> {
        self.attr_str("version")
    }

    /// The package name, only when the derivation actually declares one.
    ///
    /// Distinct from [`Self::pname`], which falls back to the derivation's
    /// own `name` so that every closure member has something to be called.
    /// That fallback is right for emission and wrong for asking nixpkgs
    /// questions: measured on a real closure, 447 of 823 names it produces
    /// are patch files and fetched tarballs — `CVE-2019-13232-1.patch`,
    /// `001-fix-rpath.patch`, bare commit hashes. Those have no `meta`, will
    /// never be an attribute, and counting them as unreachable packages
    /// understates coverage by more than half while measuring the wrong
    /// thing entirely.
    pub(crate) fn declared_pname(&self) -> Option<&str> {
        self.attr_str("pname")
    }

    /// Every output path this derivation declares.
    ///
    /// All of them, not the default one: a derivation commonly has `out`,
    /// `dev`, `man` and `lib`, and an attribute's `outPath` is only ever the
    /// default. Matching against a single arbitrarily-chosen output rejects
    /// real matches — which is how a coverage measurement first came back at
    /// zero.
    pub(crate) fn output_paths(&self) -> impl Iterator<Item = &str> {
        self.outputs.values().filter_map(|o| o.path.as_deref())
    }

    /// The path of the output an attribute's `outPath` names: the first entry
    /// of the derivation's `outputs` attribute.
    ///
    /// Milestone 1423. `nix derivation show` keys its `outputs` object
    /// alphabetically, so the order survives only in the attribute (`env`, or
    /// the typed carrier under structured attributes). Measured against the
    /// evaluated `outPath` for shellcheck, curl, hlint and pandoc: 4 of 4
    /// agree (`specs/1423-nix-runtime-scope/measurements/nix-behaviour/q9-*`).
    /// Without the attribute, `out`, then a sole output.
    pub(crate) fn default_output_path(&self) -> Option<&str> {
        let named = self
            .paths_in("outputs")
            .next()
            .and_then(|head| self.outputs.get(head))
            .or_else(|| self.outputs.get("out"))
            .or_else(|| {
                (self.outputs.len() == 1)
                    .then(|| self.outputs.values().next())
                    .flatten()
            })?;
        named.path.as_deref()
    }

    /// Whether nixpkgs' Haskell generic builder built this derivation.
    ///
    /// Milestone 1423. The generic builder, and nothing else in nixpkgs,
    /// defines `setupCompilerEnvironmentPhase`; this is the test the
    /// static-linking rule was scored with
    /// (`specs/1423-nix-runtime-scope/measurements/nix-behaviour/q6-rule.py`).
    pub(crate) fn is_haskell_builder(&self) -> bool {
        self.attr_str("setupCompilerEnvironmentPhase").is_some()
    }

    /// Store paths listed in a field, from whichever carrier holds it.
    ///
    /// The resolution rule, per
    /// `specs/1278-closure-structured-attrs/contracts/attribute-resolution.md`:
    /// the flat map first, split on whitespace; then the typed carrier, where
    /// an array yields its string members and a bare string is split like the
    /// flat form; anything else yields nothing.
    ///
    /// **Nothing else yields nothing, and that is the whole safety argument.**
    /// `doCheck` is a boolean, `env` a nested object, `outputChecks` a map, and
    /// all three sit beside the dependency fields on real derivations. The
    /// function is total over [`Value`], so a shape nixpkgs introduces later
    /// cannot panic the scan — it can only go unread, which is what
    /// [`ReadOutcome`] exists to disclose.
    ///
    /// An **empty** array yields nothing, identically to an absent field. This
    /// is load-bearing rather than incidental: `buildInputs: []` is present on
    /// all 56 `source` derivations of the measured closure, and crediting it
    /// would manufacture references that read as an improvement to the census.
    pub(crate) fn paths_in(&self, field: &str) -> impl Iterator<Item = &str> + '_ {
        self.resolve_paths(field).into_iter()
    }

    fn resolve_paths(&self, field: &str) -> Vec<&str> {
        if let Some(flat) = self.env.get(field) {
            return flat.split_whitespace().collect();
        }
        let Some(value) = self
            .structured_attrs
            .as_ref()
            .and_then(|sa| sa.get(field))
        else {
            return Vec::new();
        };
        match value {
            // A non-string member is skipped individually, never fatally: one
            // odd entry must not discard the entries beside it.
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            Value::String(one) => one.split_whitespace().collect(),
            _ => Vec::new(),
        }
    }

    /// Fields this derivation declares in both carriers with differing values.
    ///
    /// Reported rather than resolved silently (FR-006). Empty on every closure
    /// measured, so a non-empty result is news.
    pub(crate) fn carrier_disagreements(&self) -> Vec<&str> {
        let Some(Value::Object(sa)) = self.structured_attrs.as_ref() else {
            return Vec::new();
        };
        sa.iter()
            .filter_map(|(key, typed)| {
                let flat = self.env.get(key)?;
                let same = match typed {
                    Value::String(one) => one == flat,
                    Value::Array(items) => {
                        let joined: Vec<&str> =
                            items.iter().filter_map(Value::as_str).collect();
                        joined == flat.split_whitespace().collect::<Vec<&str>>()
                    }
                    _ => false,
                };
                (!same).then_some(key.as_str())
            })
            .collect()
    }

    /// Every field this derivation declares that is a nixpkgs **dependency
    /// attribute** and holds at least one store path.
    ///
    /// Used to find the dependency attributes classification does **not**
    /// consult (FR-014), which is this milestone's measurement of the gap
    /// #1346 describes.
    ///
    /// **The name test is load-bearing, and an earlier version of this
    /// function did without it and was useless.** That version asked only
    /// whether a field's value contained a `/nix/store/` path — a pure shape
    /// test, chosen on the argument that a name allowlist is what fell out of
    /// date and produced #1348 in the first place. Measured on the real
    /// corpus closure, it reported **98,342 "unconsulted declarations" across
    /// 1,610 of 1,749 derivations**, naming 113 fields including `PATH`,
    /// `SHELL`, `mkdir`, `builder` and `buildPhase`. Nearly every attribute a
    /// derivation has mentions a store path; almost none of them declare a
    /// dependency. A disclosure saying 98,342 would tell a consumer the
    /// document is worthless when the truth is that 554 dependency entries
    /// were missed.
    ///
    /// The earlier argument conflated two different allowlists. The one that
    /// fell out of date is the list of fields we **classify** — seven of
    /// roughly twenty, which is #1346. Deciding which fields *are* dependency
    /// attributes is a separate question, and nixpkgs' naming convention is
    /// the actual contract for it: `*Inputs`, and `deps<Host><Target>` with
    /// an optional `Propagated` suffix. A shape test cannot answer that
    /// question, as the measurement above shows.
    ///
    /// Still a conservative filter rather than a fixed list: any future
    /// `somethingInputs` or `depsWhatever` is caught without an edit here.
    pub(crate) fn store_path_fields(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let flat = self.env.keys().map(String::as_str);
        let typed = match self.structured_attrs.as_ref() {
            Some(Value::Object(sa)) => sa.keys().map(String::as_str).collect::<Vec<&str>>(),
            _ => Vec::new(),
        };
        for key in flat.chain(typed) {
            if out.contains(&key) || !is_dependency_attribute(key) {
                continue;
            }
            if self
                .resolve_paths(key)
                .iter()
                .any(|p| p.starts_with("/nix/store/"))
            {
                out.push(key);
            }
        }
        out
    }
}

/// Whether a field name is a nixpkgs dependency attribute.
///
/// nixpkgs names them by convention, and the convention is the contract: a
/// list of dependencies is either `<something>Inputs` (`buildInputs`,
/// `nativeBuildInputs`, `propagatedBuildInputs`, `nativeCheckInputs`,
/// `extraNativeBuildInputs`, …) or `deps<Host><Target>` with an optional
/// `Propagated` suffix (`depsBuildHost`, `depsTargetTargetPropagated`, …).
///
/// Matching the convention rather than enumerating the members means a
/// dependency attribute nixpkgs adds later is still *reported* as unconsulted
/// rather than silently ignored — which is the whole point of the FR-014
/// diagnostic.
fn is_dependency_attribute(name: &str) -> bool {
    name.ends_with("Inputs")
        || name.ends_with("InputsPropagated")
        || (name.starts_with("deps") && name.len() > 4)
}

/// The basename of a store path, which is the only form in which two sides of
/// the closure can be compared.
///
/// **This is the join that fails silently.** Output paths under
/// `outputs.*.path` are stored WITHOUT the `/nix/store/` prefix, while the
/// `env` fields that reference them carry it. Comparing the raw strings
/// matches nothing — and matches nothing without erroring, producing a
/// classification in which every member is unreferenced. The first draft of
/// the analysis script did exactly that and reported 1,275 of 1,275
/// unreferenced, which looked like a finding rather than a bug.
impl RawDerivation {
    /// The `.drv` paths this derivation consumed, filtered to those the
    /// closure's own map contains (FR-005).
    ///
    /// nix emits entries naming derivations outside the queried closure, and
    /// an edge to a node that is not in the graph is not an edge. The filter
    /// lives here rather than at the call site so every consumer gets it.
    pub(crate) fn input_drvs<'a>(
        &'a self,
        closure: &'a BTreeMap<String, RawDerivation>,
    ) -> impl Iterator<Item = &'a str> + 'a {
        self.inputs
            .drvs
            .keys()
            .map(String::as_str)
            .filter(|k| closure.contains_key(*k))
    }
}

pub(crate) fn store_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// What the reader saw while reading a closure.
///
/// The disclosure of FR-009, assembled once per closure. **Purely
/// observational**: it must not influence any member's role, which is what
/// makes the corrected census attributable to the attribute-shape fix alone
/// rather than to two changes at once (SC-014).
///
/// Absence of a disclosure in an emitted document means "nothing to
/// disclose", never "not checked" — the distinction this whole milestone is
/// about. A closure that was never read is C190's case, not this one, which
/// is why a zero-valued outcome is not emitted: it would claim a closure was
/// read where none was.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReadOutcome {
    /// The `nix derivation show` output format version (FR-012).
    pub(crate) format_version: Option<u64>,
    /// Store-path fields declared but not consulted by classification.
    ///
    /// A sorted set, so the emitted value is byte-stable — an unordered
    /// collection here would make the corpus goldens non-deterministic.
    pub(crate) fields_unconsulted: BTreeSet<String>,
    pub(crate) declarations_unconsulted: usize,
    pub(crate) derivations_affected: usize,
    /// Fields a derivation declared in both carriers with differing values
    /// (FR-006). Empty on every closure measured.
    pub(crate) carrier_disagreements: BTreeSet<String>,
    /// Which unconsulted attributes name each closure member (#1346, FR-002).
    ///
    /// Keyed **per member**, not per declaring derivation: a consumer asks
    /// "why is this component here", not "what did that derivation declare".
    /// The value is sorted so the emitted annotation is byte-stable.
    ///
    /// This is what stops `unreferenced` meaning two things. Without it, a
    /// member nothing depends on and a member whose only dependency edge runs
    /// through an attribute waybill does not classify are reported
    /// identically — the first is a fact about the closure, the second a fact
    /// about waybill, and a consumer cannot tell them apart.
    ///
    /// Purely observational: it must never feed a role. That is what keeps
    /// the census attributable to the four newly-consulted attributes alone
    /// (SC-011), and it is asserted rather than assumed.
    pub(crate) unclassified_refs: BTreeMap<String, BTreeSet<String>>,
}

impl ReadOutcome {
    /// Whether there is anything to disclose.
    ///
    /// Asserted on a positive signal: something was seen and not consulted.
    /// The format version alone is not a reason to disclose, or every document
    /// would carry the field and its presence would stop meaning anything.
    pub(crate) fn has_gap(&self) -> bool {
        !self.fields_unconsulted.is_empty() || !self.carrier_disagreements.is_empty()
    }
}

impl RawClosure {
    pub(crate) fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Assemble the [`ReadOutcome`] for this closure.
    ///
    /// `consulted` is the set of fields classification reads — passed in
    /// rather than duplicated here, so the two cannot drift into disagreeing
    /// about what "consulted" means.
    pub(crate) fn read_outcome(&self, consulted: &[&str]) -> ReadOutcome {
        let mut outcome = ReadOutcome {
            format_version: self.version,
            ..Default::default()
        };
        // The same basename join classification uses. Comparing unstripped
        // paths resolves nothing, silently — see `store_basename`.
        let index = self.output_index();
        for drv in self.derivations.values() {
            let mut affected = false;
            for field in drv.store_path_fields() {
                if consulted.contains(&field) {
                    continue;
                }
                outcome.fields_unconsulted.insert(field.to_string());
                outcome.declarations_unconsulted += drv.paths_in(field).count();
                affected = true;
                // FR-002 / FR-013 — record WHICH members this unconsulted
                // attribute names, and through which attribute. Built from
                // this same walk rather than a second pass, so the index and
                // the disclosure cannot disagree about what was unconsulted.
                for path in drv.paths_in(field) {
                    if let Some(key) = index.get(store_basename(path)) {
                        outcome
                            .unclassified_refs
                            .entry((*key).to_string())
                            .or_default()
                            .insert(field.to_string());
                    }
                }
            }
            for field in drv.carrier_disagreements() {
                outcome.carrier_disagreements.insert(field.to_string());
            }
            if affected {
                outcome.derivations_affected += 1;
            }
        }
        outcome
    }

    /// Map every output basename to the `.drv` key that produces it.
    pub(crate) fn output_index(&self) -> BTreeMap<&str, &str> {
        let mut index = BTreeMap::new();
        for (key, drv) in &self.derivations {
            for out in drv.outputs.values() {
                if let Some(path) = out.path.as_deref() {
                    index.insert(store_basename(path), key.as_str());
                }
            }
        }
        index
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Shaped like the real thing: outputs carry a bare basename, env fields
    /// carry a full store path to the same output.
    const CLOSURE: &str = r#"{
      "derivations": {
        "aaa-lib-1.0.drv": {
          "name": "lib-1.0",
          "env": { "pname": "lib", "version": "1.0" },
          "outputs": { "out": { "path": "hash1-lib-1.0" } }
        },
        "bbb-app-2.0.drv": {
          "name": "app-2.0",
          "env": { "pname": "app", "version": "2.0",
                   "buildInputs": "/nix/store/hash1-lib-1.0" },
          "outputs": { "out": { "path": "hash2-app-2.0" } }
        }
      },
      "version": 3
    }"#;

    #[test]
    fn basename_join_resolves_across_the_prefix_mismatch() {
        let c = RawClosure::parse(CLOSURE).unwrap();
        let index = c.output_index();

        // CONTROL: the fixture really does hold two derivations. A parse that
        // silently produced nothing would make the assertion below vacuous.
        assert_eq!(c.derivations.len(), 2);
        assert_eq!(index.len(), 2);

        let app = &c.derivations["bbb-app-2.0.drv"];
        let referenced: Vec<&str> = app
            .paths_in("buildInputs")
            .filter_map(|p| index.get(store_basename(p)).copied())
            .collect();

        assert_eq!(
            referenced,
            vec!["aaa-lib-1.0.drv"],
            "the env field carries /nix/store/hash1-lib-1.0 and the output \
             carries hash1-lib-1.0; comparing them unstripped resolves nothing"
        );
    }

    #[test]
    fn comparing_unstripped_paths_resolves_nothing() {
        // The bug this join exists to avoid, asserted directly so the reason
        // for `store_basename` cannot be optimised away by a later reader.
        let c = RawClosure::parse(CLOSURE).unwrap();
        let index = c.output_index();
        let app = &c.derivations["bbb-app-2.0.drv"];
        let naive: Vec<&str> = app
            .paths_in("buildInputs")
            .filter_map(|p| index.get(p).copied())
            .collect();
        assert!(
            naive.is_empty(),
            "if this ever resolves, the prefix mismatch is gone and \
             store_basename may be simplified"
        );
    }

    #[test]
    fn pname_is_preferred_over_the_derivation_name() {
        let c = RawClosure::parse(CLOSURE).unwrap();
        assert_eq!(c.derivations["aaa-lib-1.0.drv"].pname(), Some("lib"));
        assert_eq!(c.derivations["aaa-lib-1.0.drv"].version(), Some("1.0"));
    }

    // ---------------------------------------------------------------
    // Milestone 1278 (#1348) — the typed attribute carrier.
    //
    // Against the committed fixtures rather than inline strings, because the
    // shapes here are the ones nix actually emits: they were lifted from a
    // real closure, and a hand-invented typed form would test the reader
    // against a shape nixpkgs does not produce.
    // ---------------------------------------------------------------

    fn fixture(name: &str) -> RawClosure {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nix_closure_structured_attrs")
            .join(name);
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        RawClosure::parse(&json).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
    }

    #[test]
    fn m1278_a_typed_field_resolves_to_the_same_paths_a_flat_one_would() {
        let c = fixture("matched_pair.json");
        // CONTROL: the fixture holds both consumers, so a parse that dropped
        // the typed one could not make the assertions below pass vacuously.
        assert_eq!(c.derivations.len(), 6);

        let flat = &c.derivations["eee-waybill-fixture-flatconsumer-1.0.drv"];
        let typed = &c.derivations["fff-waybill-fixture-typedconsumer-1.0.drv"];

        assert_eq!(
            flat.paths_in("buildInputs").count(),
            1,
            "the flat form is unchanged by this feature"
        );
        assert_eq!(
            typed.paths_in("buildInputs").collect::<Vec<&str>>(),
            vec!["/nix/store/h2-waybill-fixture-libbeta-2.0"],
            "a typed array must resolve exactly as a whitespace-joined flat \
             string would -- this is the read that was missing entirely"
        );
        assert_eq!(
            typed.paths_in("nativeBuildInputs").count(),
            1,
            "tooling fields resolve from the typed carrier too, or the \
             artifact/tooling split silently collapses to artifact-only"
        );
    }

    #[test]
    fn m1278_identity_resolves_from_the_typed_carrier() {
        let c = fixture("matched_pair.json");
        let typed = &c.derivations["fff-waybill-fixture-typedconsumer-1.0.drv"];
        assert_eq!(typed.pname(), Some("waybill-fixture-typedconsumer"));
        assert_eq!(typed.version(), Some("1.0"));
        assert_eq!(
            typed.declared_pname(),
            Some("waybill-fixture-typedconsumer"),
            "declared_pname must see the typed carrier as well, or a member \
             that declares a name reads as having none and is asked nixpkgs \
             questions it cannot answer"
        );
    }

    #[test]
    fn m1278_an_empty_typed_list_credits_nothing() {
        let c = fixture("awkward_shapes.json");
        let d = &c.derivations["bbb-waybill-fixture-emptylist-1.0.drv"];
        assert_eq!(d.paths_in("buildInputs").count(), 0);
        // Not a curiosity: `buildInputs: []` is on all 56 `source` derivations
        // of the measured closure. Crediting it would invent references and
        // the invention would read as an improved census.
        assert_eq!(d.paths_in("nativeBuildInputs").count(), 0);
    }

    #[test]
    fn m1278_a_typed_value_that_is_not_a_string_list_yields_nothing() {
        let c = fixture("awkward_shapes.json");
        for (key, field) in [
            ("ccc-waybill-fixture-boolfield-1.0.drv", "buildInputs"),
            ("ccc-waybill-fixture-boolfield-1.0.drv", "nativeBuildInputs"),
            ("ddd-waybill-fixture-nestedobject-1.0.drv", "buildInputs"),
            ("eee-waybill-fixture-nullfield-1.0.drv", "buildInputs"),
        ] {
            assert_eq!(
                c.derivations[key].paths_in(field).count(),
                0,
                "{key}.{field} is not a string list and must resolve to \
                 nothing without failing the scan"
            );
        }
    }

    #[test]
    fn m1278_a_typed_string_splits_like_the_flat_form() {
        let c = fixture("awkward_shapes.json");
        let d = &c.derivations["fff-waybill-fixture-singlestring-1.0.drv"];
        assert_eq!(
            d.paths_in("buildInputs").collect::<Vec<&str>>(),
            vec!["/nix/store/h1-waybill-fixture-libalpha-1.0"]
        );
    }

    #[test]
    fn m1278_a_mixed_typed_list_keeps_the_strings_and_drops_the_rest() {
        let c = fixture("awkward_shapes.json");
        let d = &c.derivations["ggg-waybill-fixture-mixedlist-1.0.drv"];
        assert_eq!(
            d.paths_in("buildInputs").collect::<Vec<&str>>(),
            vec!["/nix/store/h1-waybill-fixture-libalpha-1.0"],
            "one odd member must not discard the members beside it"
        );
    }

    #[test]
    fn m1278_the_flat_carrier_wins_a_disagreement_and_the_disagreement_is_reported() {
        let c = fixture("both_shapes_disagree.json");
        let d = &c.derivations["ccc-waybill-fixture-disagree-1.0.drv"];
        assert_eq!(
            d.paths_in("buildInputs").collect::<Vec<&str>>(),
            vec!["/nix/store/h1-waybill-fixture-libalpha-1.0"],
            "FR-006: the flat map wins"
        );
        assert_eq!(d.pname(), Some("waybill-fixture-disagree"));
        let mut reported = d.carrier_disagreements();
        reported.sort_unstable();
        assert_eq!(
            reported,
            vec!["buildInputs", "pname", "version"],
            "winning silently is the failure mode; every disagreeing field \
             must be named"
        );
    }

    #[test]
    fn m1278_a_derivation_that_agrees_across_carriers_reports_no_disagreement() {
        // The control for the test above: a fixture where both carriers are
        // present but consistent must report nothing, or the detector is just
        // reporting the presence of the typed carrier.
        let c = fixture("matched_pair.json");
        for key in c.derivations.keys() {
            assert!(
                c.derivations[key].carrier_disagreements().is_empty(),
                "{key} declares nothing twice, so nothing should be reported"
            );
        }
    }

    #[test]
    fn m1278_the_format_version_is_captured_and_never_gates_resolution() {
        assert_eq!(fixture("matched_pair.json").version, Some(4));
        let unknown = fixture("version_unknown.json");
        assert_eq!(unknown.version, Some(99));
        // FR-013: a version the reader has never seen resolves exactly as a
        // known one. A version allowlist is an allowlist.
        let typed = &unknown.derivations["ddd-waybill-fixture-typedconsumer-1.0.drv"];
        assert_eq!(
            typed.paths_in("buildInputs").collect::<Vec<&str>>(),
            vec!["/nix/store/h2-waybill-fixture-libbeta-2.0"]
        );
    }

    #[test]
    fn m1278_store_path_fields_are_found_by_shape_in_either_carrier() {
        let c = fixture("matched_pair.json");
        assert_eq!(
            c.derivations["eee-waybill-fixture-flatconsumer-1.0.drv"].store_path_fields(),
            vec!["buildInputs", "nativeBuildInputs"]
        );
        assert_eq!(
            c.derivations["fff-waybill-fixture-typedconsumer-1.0.drv"].store_path_fields(),
            vec!["buildInputs", "nativeBuildInputs"],
            "an empty propagatedBuildInputs holds no store path, so it is not \
             a store-path field here"
        );
    }

    #[test]
    fn m1278_a_fully_consulted_closure_has_nothing_to_disclose() {
        // The gating property: absence of a disclosure must mean "nothing to
        // disclose". If this fires, every document gains the field and its
        // presence stops carrying information.
        let c = fixture("matched_pair.json");
        let outcome = c.read_outcome(&["buildInputs", "nativeBuildInputs", "patches"]);
        assert!(!outcome.has_gap(), "{outcome:?}");
        assert_eq!(outcome.format_version, Some(4));
        assert_eq!(outcome.declarations_unconsulted, 0);
        assert_eq!(outcome.derivations_affected, 0);
    }

    #[test]
    fn m1278_an_unconsulted_field_is_counted_in_both_carriers() {
        let c = fixture("matched_pair.json");
        // Consult only the artifact field, so the tooling field is the gap --
        // declared once flat and once typed, which is the point: the count
        // must include both carriers.
        let outcome = c.read_outcome(&["buildInputs"]);
        assert!(outcome.has_gap());
        assert_eq!(
            outcome.fields_unconsulted.iter().cloned().collect::<Vec<String>>(),
            vec!["nativeBuildInputs".to_string()]
        );
        assert_eq!(outcome.declarations_unconsulted, 2);
        assert_eq!(outcome.derivations_affected, 2);
    }

    #[test]
    fn m1278_env_vars_that_mention_store_paths_are_not_dependency_declarations() {
        // The test the earlier fixtures were too clean to provide.
        //
        // Nearly every attribute of a real derivation mentions a store path:
        // `PATH`, `SHELL`, `builder`, `buildPhase`, `src`, `stdenv`, and
        // coreutils binaries referenced by name. None of them declares a
        // dependency. A shape-only filter reported 98,342 "unconsulted
        // declarations" across 1,610 of 1,749 derivations on the real corpus
        // closure and named 113 fields -- a disclosure that would tell a
        // consumer the document is worthless when 554 entries were missed.
        //
        // Every unit and integration test passed with that filter in place,
        // because the fixtures held only tidy fields. A fixture modelled on
        // the SHAPE of real data can still omit its MESS.
        let c = fixture("real_world_noise.json");
        let d = &c.derivations["bbb-waybill-fixture-noisy-1.0.drv"];

        assert_eq!(
            d.store_path_fields(),
            vec!["depsBuildTarget"],
            "only the dependency attribute counts -- everything else here \
             holds a store path and declares nothing"
        );

        let outcome = c.read_outcome(&["buildInputs", "nativeBuildInputs", "patches"]);
        assert_eq!(
            outcome.fields_unconsulted.iter().cloned().collect::<Vec<String>>(),
            vec!["depsBuildTarget".to_string()]
        );
        assert_eq!(
            outcome.declarations_unconsulted, 1,
            "one declaration, not nine: {outcome:?}"
        );
    }

    #[test]
    fn m1278_the_dependency_attribute_test_follows_nixpkgs_naming() {
        // Matching the convention, not enumerating members, so an attribute
        // nixpkgs adds later is still reported rather than silently ignored.
        for yes in [
            "buildInputs",
            "nativeBuildInputs",
            "propagatedBuildInputs",
            "propagatedNativeBuildInputs",
            "nativeCheckInputs",
            "extraNativeBuildInputs",
            "depsBuildBuild",
            "depsTargetTargetPropagated",
            "somethingEntirelyNewInputs",
        ] {
            assert!(is_dependency_attribute(yes), "{yes} should count");
        }
        for no in [
            "PATH", "SHELL", "builder", "buildPhase", "src", "mkdir", "stdenv",
            "out", "outputs", "deps", "LD_LIBRARY_PATH", "configureFlags",
        ] {
            assert!(!is_dependency_attribute(no), "{no} should not count");
        }
    }

    #[test]
    fn m1278_clearing_the_typed_carrier_reproduces_the_defect() {
        // SC-010 / T039 -- the mutation, run as a mutation rather than
        // asserted about.
        //
        // The pre-#1348 reader is exactly "this reader with the typed carrier
        // absent", so remove the carrier from the parsed data and the old
        // behaviour must come back: the member resolves to nothing. A test
        // that only checks the fixed behaviour cannot distinguish a working
        // fix from a fixture that would have passed anyway.
        let mut c = fixture("matched_pair.json");
        let key = "fff-waybill-fixture-typedconsumer-1.0.drv";

        assert_eq!(
            c.derivations[key].paths_in("buildInputs").count(),
            1,
            "CONTROL: with the carrier present the dependency resolves"
        );

        c.derivations.get_mut(key).unwrap().structured_attrs = None;

        assert_eq!(
            c.derivations[key].paths_in("buildInputs").count(),
            0,
            "with the typed carrier gone the dependency must vanish -- if it \
             survives, the resolution is reading it from somewhere else and \
             this feature is not what makes the test above pass"
        );
        assert_eq!(
            c.derivations[key].pname(),
            Some("waybill-fixture-typedconsumer-1.0"),
            "and the identity falls back to the derivation's own name, which \
             is the pre-fix behaviour that made 11 members unnameable"
        );
    }

    #[test]
    fn m1278_a_name_declared_in_neither_carrier_is_still_not_declared() {
        // T027. This feature adds a SOURCE for the name; it does not invent
        // one. A derivation declaring `pname` nowhere must still report none,
        // or the nixpkgs declaration pass starts asking questions about
        // patch files and fetched tarballs.
        let c = fixture("awkward_shapes.json");
        let d = &c.derivations["aaa-waybill-fixture-libalpha-1.0.drv"];
        assert_eq!(d.declared_pname(), Some("waybill-fixture-libalpha"));

        let mut stripped = c.clone();
        let target = stripped
            .derivations
            .get_mut("aaa-waybill-fixture-libalpha-1.0.drv")
            .unwrap();
        target.env.remove("pname");
        target.structured_attrs = None;
        assert_eq!(
            stripped.derivations["aaa-waybill-fixture-libalpha-1.0.drv"].declared_pname(),
            None
        );
    }

    #[test]
    fn a_derivation_without_the_field_yields_no_paths() {
        let c = RawClosure::parse(CLOSURE).unwrap();
        assert_eq!(c.derivations["aaa-lib-1.0.drv"].paths_in("buildInputs").count(), 0);
    }

    // ---- m1280 (#1346): the inputs.drvs parse ----

    fn edge_fixture(name: &str) -> RawClosure {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nix_closure_edges")
            .join(name);
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        RawClosure::parse(&json).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
    }

    #[test]
    fn m1280_t008_an_input_outside_the_closure_is_dropped() {
        // FR-005. nix emits these, so the filter is not defensive padding.
        let c = edge_fixture("dangling_input.json");

        // CONTROL: the fixture really does name something absent, or the
        // assertion below passes for a fixture that never tested the filter.
        let raw = &c.derivations["dangle.drv"].inputs.drvs;
        assert!(
            raw.contains_key("absent-from-this-closure.drv"),
            "the fixture must declare the dangling input it exists to test"
        );
        assert_eq!(raw.len(), 1);

        let kept: Vec<&str> = c.derivations["dangle.drv"]
            .input_drvs(&c.derivations)
            .collect();
        assert!(
            kept.is_empty(),
            "an edge to a node not in the graph is not an edge; got {kept:?}"
        );
    }

    #[test]
    fn m1280_t008_a_derivation_with_no_inputs_key_yields_nothing() {
        // The field is `#[serde(default)]`, so a closure from a nix that does
        // not emit it must parse to an empty set rather than failing. The
        // reader deliberately does not gate on the format version (m1278), so
        // this is the only handling such a closure gets.
        let c = RawClosure::parse(CLOSURE).unwrap();
        assert_eq!(c.derivations.len(), 2, "CLOSURE declares no `inputs` at all");
        for (key, drv) in &c.derivations {
            assert_eq!(
                drv.input_drvs(&c.derivations).count(),
                0,
                "{key} has no inputs key and must contribute no edges"
            );
        }
    }

    #[test]
    fn m1280_t008_only_the_keys_are_read_and_srcs_are_ignored() {
        // `inputs.srcs` names store paths, not derivations. A src-derived edge
        // would point at something that is not a node in this graph.
        let c = edge_fixture("through_fetch.json");
        let inputs: Vec<&str> = c.derivations["pkga.drv"]
            .input_drvs(&c.derivations)
            .collect();
        assert_eq!(inputs, vec!["fetch.drv"]);
    }

    // ---- m1279 (#1346): the unclassified-reference index ----

    fn dep_fixture(name: &str) -> RawClosure {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nix_closure_dep_fields")
            .join(name);
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        RawClosure::parse(&json).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
    }

    fn dep_outcome(name: &str) -> (RawClosure, ReadOutcome) {
        let c = dep_fixture(name);
        let outcome = c.read_outcome(&super::super::classify::consulted_fields());
        (c, outcome)
    }

    #[test]
    fn m1279_t008_the_index_names_the_attribute_that_reached_each_member() {
        // T008 against T002's fixture. Each of the three stdenv-injected
        // attributes names exactly one member, so the index's value set is
        // separable per attribute rather than a union nobody can attribute.
        let (c, outcome) = dep_outcome("stdenv_only.json");

        // CONTROL, twice over. The fixture must really hold the derivations
        // it claims, and something must really be referenced: an index built
        // from a closure that parsed to nothing satisfies every "names the
        // right attribute" assertion below vacuously, and so does one whose
        // basename join resolved no path at all.
        assert_eq!(c.derivations.len(), 5, "fixture shape changed");
        assert_eq!(
            outcome.unclassified_refs.len(),
            4,
            "four members are named by a stdenv default -- one per attribute, \
             plus `m-two` named by two -- and all four must be indexed; got \
             {:?}",
            outcome.unclassified_refs
        );
        // The member named by TWO unclassified attributes carries both, sorted
        // (FR-013). A marker that kept only the first would under-report, and
        // nothing else in this module would notice.
        assert_eq!(
            outcome.unclassified_refs["m-two.drv"]
                .iter()
                .cloned()
                .collect::<Vec<String>>(),
            vec![
                "defaultNativeBuildInputs".to_string(),
                "extraNativeBuildInputs".to_string()
            ]
        );

        for (member, attribute) in [
            ("m-dnbi.drv", "defaultNativeBuildInputs"),
            ("m-dbi.drv", "defaultBuildInputs"),
            ("m-enbi.drv", "extraNativeBuildInputs"),
        ] {
            let got = outcome
                .unclassified_refs
                .get(member)
                .unwrap_or_else(|| panic!("{member} missing from the index"));
            assert_eq!(
                got.iter().cloned().collect::<Vec<String>>(),
                vec![attribute.to_string()],
                "{member} must name {attribute} and nothing else"
            );
        }
    }

    #[test]
    fn m1279_t008_a_member_reached_only_by_consulted_attributes_is_not_indexed() {
        // T008 against T003's fixture. The index is the complement of what
        // classification reads, so a member the classifier *can* explain must
        // not appear in it -- otherwise every component would carry the
        // marker and the marker would mean nothing.
        let (c, outcome) = dep_outcome("attribute_states.json");

        // CONTROL: the consulted side really did resolve. Were the basename
        // join broken, `m-art` would be absent from the index for the wrong
        // reason and this test would pass while the feature was inert.
        let roles = super::super::classify::classify(&c);
        assert!(
            roles.contains_key("m-art.drv") && roles.contains_key("m-tool.drv"),
            "the consulted attributes must resolve before their absence from \
             the index means anything; roles={roles:?}"
        );

        for consulted_only in ["m-art.drv", "m-tool.drv"] {
            assert!(
                !outcome.unclassified_refs.contains_key(consulted_only),
                "{consulted_only} is reached only through attributes \
                 classification reads, so it must not be indexed"
            );
        }
        assert_eq!(
            outcome
                .unclassified_refs
                .get("m-unref.drv")
                .map(|a| a.iter().cloned().collect::<Vec<String>>()),
            Some(vec!["defaultNativeBuildInputs".to_string()]),
            "the member reached ONLY by an unconsulted attribute is the one \
             the index exists for"
        );
        // And the gating row T044 asserts at integration level: `m-both` is
        // reached by two consulted attributes AND one unclassified one, so it
        // is indexed despite having a role. An index that only recorded
        // otherwise-unreferenced members would pass every other assertion
        // here.
        assert_eq!(
            outcome
                .unclassified_refs
                .get("m-both.drv")
                .map(|a| a.iter().cloned().collect::<Vec<String>>()),
            Some(vec!["defaultNativeBuildInputs".to_string()])
        );
    }

    #[test]
    fn m1279_t008_an_empty_attribute_credits_nothing_to_the_index() {
        // FR-005. `attribute_states.json` declares `depsHostHostPropagated`
        // and `propagatedBuildInputs` as empty arrays. A declared-but-empty
        // attribute is not a gap and names no member, so neither may reach
        // either the index or the disclosure.
        let (_, outcome) = dep_outcome("attribute_states.json");
        for empty in ["depsHostHostPropagated", "propagatedBuildInputs", "depsHostHost"] {
            assert!(
                !outcome.fields_unconsulted.contains(empty),
                "{empty} is declared empty and must not be reported as a gap"
            );
            for attributes in outcome.unclassified_refs.values() {
                assert!(
                    !attributes.contains(empty),
                    "{empty} is empty and can have named no member"
                );
            }
        }
    }
}
