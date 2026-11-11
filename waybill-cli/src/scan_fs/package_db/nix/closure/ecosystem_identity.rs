//! Milestone 1366 (#1365) — an ecosystem identity for closure components a
//! vulnerability feed could match.
//!
//! waybill emits a component for `urllib3 2.6.3` that is correctly named,
//! correctly versioned, and **unmatchable**: its identifier is
//! `pkg:generic/urllib3@2.6.3`, and `pkg:generic` carries no ecosystem, so no
//! feed can answer "is this build affected by CVE-X" about it. Measured on the
//! pinned `nix-closure-moat` closure, 781 of 942 components are in that state.
//!
//! Most of them should stay that way. 539 are closure members — patch files,
//! source tarballs, build fragments — for which `pkg:generic` is *correct* and
//! changing it is a regression (FR-005). This module addresses the 83
//! derivations / 75 distinct names that are PyPI packages.
//!
//! # What confirmation does and does not establish
//!
//! A candidate becomes `pkg:pypi` only when the enrichment resolution path
//! confirms a published package exists at that name and version. That is
//! necessary and **not sufficient** to prove this derivation *is* that
//! project.
//!
//! Measured 2026-11-06 against deps.dev: **75 of 75 candidates confirm, 0
//! decline.** The spec was written believing `librt` and `roman-numerals`
//! would be rejected; both resolve, and both appear to be correct matches
//! (`librt` is mypyc's runtime, and this closure contains `mypy`). So on the
//! measured population confirmation is a filter that filters nothing, and the
//! accuracy of this feature rests on [`candidates`]'s rule rather than on the
//! check. It is kept because a closure we have not measured can hold a name no
//! registry carries, and because emitting an unconfirmed coordinate is the one
//! outcome worse than the generic identifier it would replace.
//!
//! See `specs/1366-closure-ecosystem-purl/measurements/`.
//!
//! # Where this runs, and why it cannot move
//!
//! Between the closure merge and the closure edge build in `scan_cmd`.
//! `Relationship.from` / `.to` are PURL **strings**, and
//! [`super::graph::relationships`] builds its join table from the live
//! component slice — it reads `c.purl` when it is called. Running before that
//! call means corrected identifiers flow into the edges for free; running
//! after it orphans every component this module fixes.

use std::collections::{BTreeMap, HashMap, HashSet};

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::purl::Purl;

use super::member::ANN_MEMBER_ROLE;
use super::ClassifiedClosure;

/// The per-component disclosure that an identifier was derived (C207).
pub(crate) const ANN_ECOSYSTEM_IDENTITY: &str = "waybill:closure-ecosystem-identity";

/// The schema tag carried by [`ConfirmationRecord`].
pub(crate) const RECORD_SCHEMA: &str = "waybill-closure-ecosystem-identity/v1";

/// The one ecosystem this milestone identifies. A second is a new value here
/// plus a new [`CandidateEvidence`] variant, not a reshaped field.
pub(crate) const ECOSYSTEM: &str = "pypi";

/// What suggested that a derivation is a published package.
///
/// Closed, and carried into C207 so a consumer can see the basis rather than
/// infer it from the identifier's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateEvidence {
    /// The derivation's `name` carries a **versioned** interpreter prefix
    /// (`python3.13-urllib3`) while `pname` carries the bare project name.
    VersionedInterpreterPrefix,
}

impl CandidateEvidence {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::VersionedInterpreterPrefix => "versioned-interpreter-prefix",
        }
    }
}

/// A closure component the recorded evidence suggests is a published package.
#[derive(Debug, Clone)]
pub(crate) struct EcosystemCandidate {
    /// Position in the merged component vector. The pass mutates in place.
    pub(crate) component_index: usize,
    pub(crate) ecosystem: &'static str,
    /// `pname`, exactly as the derivation declares it.
    pub(crate) declared_name: String,
    /// The spelling the emitted PURL uses.
    pub(crate) normalised_name: String,
    /// Verbatim from the closure. Never rewritten (FR-010).
    pub(crate) version: String,
    pub(crate) evidence: CandidateEvidence,
}

impl EcosystemCandidate {
    /// The coordinate this candidate would carry if confirmed.
    pub(crate) fn coordinate(&self) -> String {
        format!(
            "pkg:{}/{}@{}",
            self.ecosystem, self.normalised_name, self.version
        )
    }
}

/// Why a candidate could not be checked.
///
/// Closed; the value that reaches C208's `reason` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UncheckedReason {
    /// `--no-deps-dev` or `--no-deps-dev-license`. **This is the corpus
    /// path**: the one corpus target that runs a closure passes
    /// `--no-deps-dev`, so a corpus run always lands here.
    Disabled,
    /// `--offline` with no cached answer. Unreachable in practice — the
    /// closure tier refuses `--offline` at [`super::admission`] — and
    /// representable so the state is a value rather than an assumption.
    Offline,
    /// A transport error. Nothing was learnt about the package.
    Unreachable,
    /// The registry asked us to slow down.
    Throttled,
    /// No lookup key could be formed: an empty or placeholder version.
    Unkeyable,
    /// Two candidates normalise to one coordinate, so **both** decline.
    Collision,
}

impl UncheckedReason {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Offline => "offline",
            Self::Unreachable => "unreachable",
            Self::Throttled => "throttled",
            Self::Unkeyable => "unkeyable",
            Self::Collision => "collision",
        }
    }
}

/// What the resolution path concluded about one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmationOutcome {
    /// A published package exists at this name and version.
    Confirmed,
    /// The ecosystem answered and holds no such package. **An answer, not a
    /// failure** — FR-003 turns on this distinction.
    Declined,
    /// Nothing was learnt.
    Unchecked(UncheckedReason),
}

/// The document-scope summary (C208).
///
/// `candidates == 0` is meaningful and distinct from every degraded state: it
/// is how a reader tells a closure with no Python packages from a confirmation
/// step that silently did nothing (FR-008).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConfirmationRecord {
    pub(crate) candidates: usize,
    pub(crate) confirmed: usize,
    pub(crate) declined: usize,
    pub(crate) unchecked: usize,
    /// Present iff `unchecked > 0`.
    pub(crate) reason: Option<UncheckedReason>,
}

impl ConfirmationRecord {
    /// `confirmed + declined + unchecked == candidates`.
    ///
    /// An arithmetic identity that holds by construction is the cheapest thing
    /// for a later edit to break, so it is asserted rather than commented.
    pub(crate) fn is_consistent(&self) -> bool {
        self.confirmed + self.declined + self.unchecked == self.candidates
            && self.reason.is_some() == (self.unchecked > 0)
    }

    /// The C208 value: a JSON object encoded as a string, on the C190 / C192 /
    /// C204 template.
    pub(crate) fn wire(&self) -> String {
        let mut obj = serde_json::Map::new();
        obj.insert("schema".into(), RECORD_SCHEMA.into());
        obj.insert("ecosystem".into(), ECOSYSTEM.into());
        obj.insert("candidates".into(), self.candidates.into());
        obj.insert("confirmed".into(), self.confirmed.into());
        obj.insert("declined".into(), self.declined.into());
        obj.insert("unchecked".into(), self.unchecked.into());
        if let Some(r) = self.reason {
            obj.insert("reason".into(), r.wire().into());
        }
        serde_json::Value::Object(obj).to_string()
    }
}

/// Does this derivation name carry a **versioned** interpreter prefix?
///
/// Both halves of the rule are load-bearing and both were measured:
///
/// * A bare `python-` prefix matches nixpkgs **setup hooks**
///   (`python-relax-deps-hook`, `python-namespaces-hook.sh`) — build
///   machinery, not packages. Twenty of them on the pinned closure.
/// * nixpkgs **bootstrap variants** carry the prefix inside `pname` itself
///   (`python3.13-bootstrap-tomli`). They are not the PyPI project, and the
///   `pname` test below drops all six.
fn has_versioned_python_prefix(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("python3.") else {
        return false;
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    !digits.is_empty() && rest[digits.len()..].starts_with('-')
}

/// What the resolution path answered about one coordinate.
///
/// A deliberate mirror of `enrich::depsdev_source::LookupResult`, and the
/// reason this module imports nothing from `enrich`: `scan_fs` depends on no
/// enrichment type anywhere in the tree, and this feature is not the right
/// place to be the first. The caller's translation is a four-arm exhaustive
/// match, so a state added to `LookupResult` breaks the build rather than
/// falling into a default that would quietly read as "could not check".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LookupAnswer {
    /// The registry holds a package at this name and version.
    Found,
    /// The registry answered and holds no such package.
    Absent,
    /// A transport failure. Nothing was learnt about the package.
    Failed,
    /// Offline, and no cache held it.
    Unqueried,
}

impl ConfirmationOutcome {
    /// FR-003 lives here.
    ///
    /// "The published ecosystem has no such package at this version" is a
    /// **decline**, not a failure. The distinction is the whole requirement:
    /// a decline is a settled answer about the package and leaves today's
    /// identifier in place deliberately, while a failure leaves it in place
    /// because nothing was learnt. They produce the same identifier and must
    /// not produce the same record, or a reader cannot tell a registry that
    /// said "no" from one that never answered.
    pub(crate) fn from_answer(answer: LookupAnswer) -> Self {
        match answer {
            LookupAnswer::Found => Self::Confirmed,
            LookupAnswer::Absent => Self::Declined,
            LookupAnswer::Failed => Self::Unchecked(UncheckedReason::Unreachable),
            LookupAnswer::Unqueried => Self::Unchecked(UncheckedReason::Offline),
        }
    }
}

impl ConfirmationRecord {
    /// Build the document-scope record from the settled outcomes.
    ///
    /// The dominant unchecked reason wins when several occur, because C208
    /// carries one. Ordering is by how much it tells an operator: a disabled
    /// pass is a configuration choice they made, a throttle is transient, and
    /// the rest are degrees of "we could not ask".
    pub(crate) fn tally(outcomes: &[ConfirmationOutcome]) -> Self {
        let mut rec = Self {
            candidates: outcomes.len(),
            ..Self::default()
        };
        let mut reasons: Vec<UncheckedReason> = Vec::new();
        for o in outcomes {
            match o {
                ConfirmationOutcome::Confirmed => rec.confirmed += 1,
                ConfirmationOutcome::Declined => rec.declined += 1,
                ConfirmationOutcome::Unchecked(r) => {
                    rec.unchecked += 1;
                    reasons.push(*r);
                }
            }
        }
        rec.reason = [
            UncheckedReason::Disabled,
            UncheckedReason::Offline,
            UncheckedReason::Throttled,
            UncheckedReason::Unreachable,
            UncheckedReason::Collision,
            UncheckedReason::Unkeyable,
        ]
        .into_iter()
        .find(|r| reasons.contains(r));
        rec
    }
}

/// Is this derivation a PyPI candidate?
///
/// The whole rule in one place, so that `waybill debug closure-dump` can
/// report the candidate population by **calling the reader** rather than by
/// retyping its regex. A reporter that drifted from the reader would be worse
/// than none: four probe errors during this feature's measurement all had the
/// shape of modelling what the code does instead of running it.
pub(crate) fn is_pypi_candidate(drv: &super::derivation::RawDerivation) -> bool {
    let Some(name) = drv.name.as_deref() else {
        return false;
    };
    if !has_versioned_python_prefix(name) {
        return false;
    }
    // V1 — reads BOTH carriers, the flat `env` map and top-level
    // `structuredAttrs`; 66 of the pinned closure's 1,275 derivations use only
    // the latter.
    match drv.declared_pname() {
        // A bootstrap variant keeps the interpreter prefix in `pname` itself.
        Some(p) => !p.starts_with("python3"),
        None => false,
    }
}

/// Derive the candidates from the closure's own records.
///
/// `appended` names the `(name, version)` identities the merge **appended**
/// rather than merged into a component another reader had already produced.
/// Only those are eligible: a merged component belongs to that reader and
/// already carries its identifier (FR-006, validation rule V4).
pub(crate) fn candidates(
    closure: &ClassifiedClosure,
    components: &[ResolvedComponent],
    appended: &HashSet<(String, String)>,
) -> Vec<EcosystemCandidate> {
    // Index the component set once by the identity the merge keys on.
    let by_identity: HashMap<(&str, &str), usize> = components
        .iter()
        .enumerate()
        .map(|(i, c)| ((c.name.as_str(), c.version.as_str()), i))
        .collect();

    let mut found: BTreeMap<(String, String), EcosystemCandidate> = BTreeMap::new();
    for drv in closure.raw.derivations.values() {
        if !is_pypi_candidate(drv) {
            continue;
        }
        // Total by construction: `is_pypi_candidate` returned true.
        let Some(pname) = drv.declared_pname() else {
            continue;
        };
        let version = drv.version().unwrap_or_default();
        let Some(&idx) = by_identity.get(&(pname, version)) else {
            // The rule matched a derivation the closure emits no component
            // for. Measured on the pinned closure: exactly one,
            // `build 1.4.4`, whose C182 role is `unreferenced` — so it is
            // part of the SEPARATE pname-bearing coverage gap (issue #1366),
            // not of this feature's population. This pass can only change
            // the identity of a component that exists; closing the coverage
            // gap is what would give this one a component to re-identify.
            //
            // Logged rather than passed over, so the difference between the
            // candidate-rule's 75 derivation identities and C208's 74
            // components is recoverable from a scan instead of only from
            // `waybill debug closure-dump`.
            tracing::debug!(
                pname,
                version,
                "nix-closure: ecosystem candidate has no emitted component"
            );
            continue;
        };
        // V4 — appended only.
        if !appended.contains(&(pname.to_string(), version.to_string())) {
            continue;
        }
        // V3 — never a closure member. FR-005 protects those absolutely, and
        // asserting it here is cheaper than trusting the rule not to reach one.
        if components[idx]
            .extra_annotations
            .contains_key(ANN_MEMBER_ROLE)
        {
            continue;
        }
        // Build variants produce the same identity several times; one
        // candidate per identity, keyed as the emitter keys components.
        found
            .entry((pname.to_string(), version.to_string()))
            .or_insert_with(|| EcosystemCandidate {
                component_index: idx,
                ecosystem: ECOSYSTEM,
                declared_name: pname.to_string(),
                normalised_name: super::super::super::pip::normalize_pypi_name_for_purl(pname),
                version: version.to_string(),
                evidence: CandidateEvidence::VersionedInterpreterPrefix,
            });
    }
    found.into_values().collect()
}

/// Validation rules V2 and V5, applied before any lookup is issued.
///
/// Returns the candidates still eligible, plus the outcomes already settled
/// for those that are not. A rejected candidate is **recorded**, never
/// dropped: a dropped candidate is invisible to C208's counts, and the whole
/// purpose of those counts is that a reader can tell "nothing to do" from
/// "did nothing".
pub(crate) fn screen(
    candidates: Vec<EcosystemCandidate>,
) -> (Vec<EcosystemCandidate>, Vec<(EcosystemCandidate, ConfirmationOutcome)>) {
    // V5 — a group of candidates sharing one normalised coordinate declines
    // ENTIRELY. Picking a winner would assert an identity for an artifact that
    // did not earn it; declining the group costs only the generic identifier
    // they already carry.
    //
    // Measured on the pinned closure: zero such groups. This is defensive, and
    // no test on that population exercises it.
    let mut seen: HashMap<String, usize> = HashMap::new();
    for c in &candidates {
        *seen.entry(c.coordinate()).or_insert(0) += 1;
    }

    let (mut eligible, mut settled) = (Vec::new(), Vec::new());
    for c in candidates {
        if seen.get(&c.coordinate()).copied().unwrap_or(0) > 1 {
            settled.push((c, ConfirmationOutcome::Unchecked(UncheckedReason::Collision)));
            continue;
        }
        // V2 — a key cannot be formed from an empty version, and a lookup that
        // cannot succeed must not be issued: an unusable entry consumes a
        // batch slot and comes back empty, indistinguishable from a package
        // the registry genuinely does not carry.
        if c.version.is_empty() || c.declared_name.is_empty() {
            settled.push((c, ConfirmationOutcome::Unchecked(UncheckedReason::Unkeyable)));
            continue;
        }
        eligible.push(c);
    }
    (eligible, settled)
}

/// Apply one settled outcome to the component set.
///
/// The identifier moves **only** on [`ConfirmationOutcome::Confirmed`]. Every
/// other outcome leaves the component exactly as it is, which FR-009 names as
/// correct behaviour rather than as a fallback.
///
/// `name` and `version` are deliberately untouched: they are the key the merge
/// and the edge join both use, and invariant I2 groups on them.
pub(crate) fn apply_outcome(
    components: &mut [ResolvedComponent],
    candidate: &EcosystemCandidate,
    outcome: ConfirmationOutcome,
) -> bool {
    if outcome != ConfirmationOutcome::Confirmed {
        return false;
    }
    let coordinate = candidate.coordinate();
    // Constitution Principle V: an invalid PURL must not reach output. A
    // coordinate that will not parse declines the candidate instead.
    let Ok(purl) = Purl::new(&coordinate) else {
        tracing::debug!(
            coordinate = %coordinate,
            "nix-closure: a confirmed candidate yields no valid PURL; leaving it generic"
        );
        return false;
    };
    let Some(component) = components.get_mut(candidate.component_index) else {
        return false;
    };
    component.purl = purl;
    component.extra_annotations.insert(
        ANN_ECOSYSTEM_IDENTITY.to_string(),
        serde_json::Value::from(candidate.evidence.wire()),
    );
    true
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn versioned_prefix_accepts_a_real_package() {
        assert!(has_versioned_python_prefix("python3.13-urllib3"));
        assert!(has_versioned_python_prefix("python3.9-six"));
    }

    #[test]
    fn versioned_prefix_rejects_a_setup_hook() {
        // Twenty of these on the pinned closure. A bare `python-` prefix is
        // build machinery, not a package.
        assert!(!has_versioned_python_prefix("python-relax-deps-hook"));
        assert!(!has_versioned_python_prefix("python-namespaces-hook.sh"));
    }

    #[test]
    fn versioned_prefix_rejects_a_bare_interpreter() {
        assert!(!has_versioned_python_prefix("python3"));
        assert!(!has_versioned_python_prefix("python3.13"));
        assert!(!has_versioned_python_prefix("python3.13.ignore"));
    }

    #[test]
    fn a_record_is_consistent_only_when_the_counts_add_up() {
        let ok = ConfirmationRecord {
            candidates: 3,
            confirmed: 1,
            declined: 2,
            unchecked: 0,
            reason: None,
        };
        assert!(ok.is_consistent());

        let bad = ConfirmationRecord {
            candidates: 3,
            confirmed: 1,
            declined: 1,
            unchecked: 0,
            reason: None,
        };
        assert!(!bad.is_consistent());

        // A reason without an unchecked candidate is as wrong as the reverse.
        let orphan_reason = ConfirmationRecord {
            candidates: 1,
            confirmed: 1,
            declined: 0,
            unchecked: 0,
            reason: Some(UncheckedReason::Disabled),
        };
        assert!(!orphan_reason.is_consistent());
    }

    #[test]
    fn the_empty_record_is_distinguishable_from_a_degraded_one() {
        let nothing_to_do = ConfirmationRecord::default();
        assert!(nothing_to_do.is_consistent());
        let wire = nothing_to_do.wire();
        assert!(wire.contains("\"candidates\":0"));
        // FR-008: no reason, because nothing failed — there was nothing to do.
        assert!(!wire.contains("reason"));

        let did_nothing = ConfirmationRecord {
            candidates: 75,
            confirmed: 0,
            declined: 0,
            unchecked: 75,
            reason: Some(UncheckedReason::Disabled),
        };
        assert!(did_nothing.is_consistent());
        assert!(did_nothing.wire().contains("\"reason\":\"disabled\""));
        assert_ne!(nothing_to_do.wire(), did_nothing.wire());
    }

    #[test]
    fn a_collision_declines_every_member_of_the_group() {
        let make = |declared: &str, normalised: &str| EcosystemCandidate {
            component_index: 0,
            ecosystem: ECOSYSTEM,
            declared_name: declared.to_string(),
            normalised_name: normalised.to_string(),
            version: "1.0".to_string(),
            evidence: CandidateEvidence::VersionedInterpreterPrefix,
        };
        let (eligible, settled) = screen(vec![
            make("Foo_Bar", "foo-bar"),
            make("foo-bar", "foo-bar"),
            make("alone", "alone"),
        ]);
        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].declared_name, "alone");
        assert_eq!(settled.len(), 2);
        assert!(settled.iter().all(|(_, o)| *o
            == ConfirmationOutcome::Unchecked(UncheckedReason::Collision)));
    }

    #[test]
    fn a_versionless_candidate_is_recorded_not_dropped() {
        let (eligible, settled) = screen(vec![EcosystemCandidate {
            component_index: 0,
            ecosystem: ECOSYSTEM,
            declared_name: "somepkg".to_string(),
            normalised_name: "somepkg".to_string(),
            version: String::new(),
            evidence: CandidateEvidence::VersionedInterpreterPrefix,
        }]);
        assert!(eligible.is_empty());
        assert_eq!(
            settled[0].1,
            ConfirmationOutcome::Unchecked(UncheckedReason::Unkeyable)
        );
    }

    #[test]
    fn only_a_confirmation_moves_the_identifier() {
        let mut components = vec![component("urllib3", "2.6.3")];
        let candidate = EcosystemCandidate {
            component_index: 0,
            ecosystem: ECOSYSTEM,
            declared_name: "urllib3".to_string(),
            normalised_name: "urllib3".to_string(),
            version: "2.6.3".to_string(),
            evidence: CandidateEvidence::VersionedInterpreterPrefix,
        };

        for outcome in [
            ConfirmationOutcome::Declined,
            ConfirmationOutcome::Unchecked(UncheckedReason::Disabled),
            ConfirmationOutcome::Unchecked(UncheckedReason::Unreachable),
        ] {
            assert!(!apply_outcome(&mut components, &candidate, outcome));
            assert_eq!(components[0].purl.as_str(), "pkg:generic/urllib3@2.6.3");
            assert!(!components[0]
                .extra_annotations
                .contains_key(ANN_ECOSYSTEM_IDENTITY));
        }

        assert!(apply_outcome(
            &mut components,
            &candidate,
            ConfirmationOutcome::Confirmed
        ));
        assert_eq!(components[0].purl.as_str(), "pkg:pypi/urllib3@2.6.3");
        assert_eq!(
            components[0].extra_annotations.get(ANN_ECOSYSTEM_IDENTITY),
            Some(&serde_json::Value::from("versioned-interpreter-prefix"))
        );
        // The merge key and the edge join both read these.
        assert_eq!(components[0].name, "urllib3");
        assert_eq!(components[0].version, "2.6.3");
    }

    /// A component shaped like one the closure emitter produces. Written out
    /// in full rather than cloned from a fixture so that a field added to
    /// `ResolvedComponent` breaks this test loudly instead of silently
    /// inheriting a default nobody chose.
    fn component(name: &str, version: &str) -> ResolvedComponent {
        use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
        ResolvedComponent {
            purl: Purl::new(&format!("pkg:generic/{name}@{version}")).unwrap(),
            name: name.to_string(),
            version: version.to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.95,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            build_inclusion: None,
            artifact_presence: None,
            scope_evidence: None,
            requirement_ranges: vec![],
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: vec![],
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }
}
