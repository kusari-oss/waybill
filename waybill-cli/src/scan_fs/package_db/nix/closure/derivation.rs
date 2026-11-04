//! Parsing `nix derivation show -r` output.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

/// One derivation from the closure.
#[derive(Debug, Clone, Deserialize)]
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
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawOutput {
    #[serde(default)]
    pub(crate) path: Option<String>,
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

    /// Every field this derivation declares that holds store paths.
    ///
    /// Used to find the fields classification does **not** consult (FR-014),
    /// which is this milestone's measurement of the gap #1346 describes. A
    /// field counts when its name ends in a dependency-ish suffix and its
    /// value resolves to at least one `/nix/store/` path — a shape test, not a
    /// name allowlist, because an allowlist is the thing that fell out of date
    /// in the first place.
    pub(crate) fn store_path_fields(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let flat = self.env.keys().map(String::as_str);
        let typed = match self.structured_attrs.as_ref() {
            Some(Value::Object(sa)) => sa.keys().map(String::as_str).collect::<Vec<&str>>(),
            _ => Vec::new(),
        };
        for key in flat.chain(typed) {
            if out.contains(&key) || key == "out" || key == "outputs" {
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
        for drv in self.derivations.values() {
            let mut affected = false;
            for field in drv.store_path_fields() {
                if consulted.contains(&field) {
                    continue;
                }
                outcome.fields_unconsulted.insert(field.to_string());
                outcome.declarations_unconsulted += drv.paths_in(field).count();
                affected = true;
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
}
