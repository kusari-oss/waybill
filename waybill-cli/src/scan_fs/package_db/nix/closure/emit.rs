//! Turning a classified closure into components.
//!
//! Closure components **supplement** the manifest-derived set; they never
//! replace it (spec FR-003a). The two answer different questions: a manifest
//! covers every stanza a project declares, while a closure covers only what
//! the selected attribute builds. Measured — one project's executable-stanza
//! dependencies and the whole GHC boot set are absent from its library's
//! closure, so treating closure-absence as evidence of spuriousness would
//! discard the Haskell standard distribution.

use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::Purl;

use super::classify::DerivationRole;
use super::derivation::RawDerivation;
use super::member::{self, MemberRole};
use super::ClassifiedClosure;

/// C182 — how nix referenced this member.
pub(crate) const ANN_CLOSURE_ROLE: &str = "waybill:closure-role";

/// C185 — the patches this member applies.
///
/// CycloneDX carries these natively in `pedigree.patches[]` and suppresses
/// this property; SPDX 2.3 and SPDX 3 have no equivalent and carry the
/// annotation. That is the one place the three formats differ in capability
/// rather than in spelling. The value is the JSON array encoded as a string,
/// matching the m134 / m147 / m173 convention for array-valued annotations.
pub(crate) const ANN_CLOSURE_PATCHES: &str = "waybill:closure-patches";

/// C205 (m1279, #1346) — dependency attributes that name this member and that
/// role classification does not read.
///
/// A sorted JSON array of attribute names, encoded as a string, matching the
/// m134 / m147 / m173 convention this file already follows for C185.
///
/// It is what stops `unreferenced` meaning two things. Without it, "nothing
/// in this closure depends on this member" and "the only thing that depends
/// on it is an attribute waybill does not classify" are the same output — the
/// first a fact about the closure, the second a fact about waybill — and a
/// consumer has no way to tell which they are looking at.
///
/// **It names the attributes** (FR-013). "Referenced by something waybill does
/// not classify" without saying what is a smaller version of the same defect:
/// it tells a consumer a gap exists and not where.
///
/// Present whenever an unclassified attribute names the member, **whatever its
/// role** — a reader comparing two `artifact-input` members should be able to
/// tell that one of them also has an unread reference. Absence therefore means
/// "no unclassified attribute names this member", never "not checked", and
/// that is only trustworthy because the FR-007 guard reports an attribute the
/// classifier does not recognise rather than ignoring it.
///
/// **Reach, stated rather than assumed.** This can only land on a component,
/// and [`emits_as_component`] drops an `Unreferenced` member that applies no
/// patch. So the gating table's "`unreferenced` + named" row materialises
/// only for patch-appliers, which is also the only place `unreferenced` is
/// observable in output at all. The other half of #1346's fix is the widening
/// in `classify.rs`: a member the four newly-consulted attributes reach stops
/// being `Unreferenced`, which is what makes it emitted in the first place.
pub(crate) const ANN_CLOSURE_UNCLASSIFIED_REFS: &str = "waybill:closure-unclassified-refs";

/// Roles that become components on the strength of the role alone.
///
/// `Unreferenced` is excluded here, but that is not the whole rule — see
/// [`emits_as_component`], which readmits the patch-appliers among them.
fn role_alone_emits(role: DerivationRole) -> bool {
    matches!(
        role,
        DerivationRole::ArtifactInput | DerivationRole::BuildTooling | DerivationRole::Both
    )
}

/// Whether a closure member becomes a component.
///
/// Role is the usual test, with one exception that spec FR-004a makes
/// load-bearing: an `Unreferenced` member that applies a patch is in scope.
/// Measured, `jq` and `lua` are `Unreferenced` in one project's closure and
/// carry 6 of its 18 CVEs. Dropping them would leave those patches with no
/// component to hang a `pedigree` on, and the CVEs would simply vanish —
/// silently, since nothing downstream can tell an absent component from a
/// component with nothing to say.
///
/// The test is local to the derivation: applying a patch means a non-empty
/// `patches` field, which needs no cross-reference to the attribution pass.
/// Visible to the crate so `waybill debug closure-dump` can report the real
/// answer rather than a second implementation of this rule. Four probe
/// errors in one session came from modelling this logic in Python instead of
/// asking the binary; a debug surface that calls the same function cannot
/// drift from it.
pub(crate) fn emits_as_component(role: DerivationRole, drv: &RawDerivation) -> bool {
    role_alone_emits(role)
        || drv.paths_in("patches").next().is_some()
        // Milestone 1282 FR-001 — a member declaring no `pname` is emitted
        // whatever its C182 role.
        //
        // **This is the line that closes Gap A, and it is not a `pname`
        // test.** `pname()` falls back to the derivation's own `name`, so it
        // returns `Some` for all 1,145 and gates nothing; the clause that
        // hid them is `role_alone_emits`, because every one of them is
        // `Unreferenced` and applies no patch. Read from the golden: 1,315
        // `unreferenced` derivations yielded 11 components (research R8a).
        //
        // Deliberately asymmetric: a `pname`-**bearing** `Unreferenced`
        // member is still suppressed unless it applies a patch. Roughly 170
        // sit on that side, and emitting them is a question about a
        // population this milestone did not measure (spec Assumptions).
        || drv.declared_pname().is_none()
}

/// Is this a member milestone 1282 added, rather than one emitted before it?
///
/// The test is on the **declaration**, matching [`emits_as_component`]'s new
/// clause, so the two cannot drift into disagreeing about which population
/// they are talking about.
fn is_1282_member(drv: &RawDerivation) -> bool {
    drv.declared_pname().is_none()
}

/// The `.drv` keys that will become components, for the milestone-1280
/// projection.
///
/// Derived from [`emits_as_component`] rather than from a second copy of the
/// rule: the projection must emit an edge to exactly the nodes the emitter
/// emits, and two predicates that have to agree is the drift this project
/// keeps paying for. A derivation yielding no name is excluded here for the
/// same reason `components` skips it.
pub(crate) fn emitted_drv_keys<'a>(
    raw: &'a super::derivation::RawClosure,
    roles: &std::collections::BTreeMap<String, DerivationRole>,
) -> std::collections::HashSet<&'a str> {
    raw.derivations
        .iter()
        .filter(|(key, drv)| {
            roles
                .get(*key)
                .copied()
                .is_some_and(|role| emits_as_component(role, drv))
                && drv.pname().is_some()
                // Milestone 1282 FR-013 — of the members this feature adds,
                // only `source` and `package` may be edge endpoints. The
                // other two are emitted and walked *through*, so the edge
                // names the source a package was built from rather than the
                // build script that stood between them.
                //
                // Derived from the same classification the emitter uses, not
                // from a second copy of the rule: emission is now a strict
                // **superset** of edge participation, and
                // [`edge_endpoint_keys_are_a_subset_of_emitted`] holds the
                // two in that relation rather than trusting them to agree.
                && (!is_1282_member(drv) || member::classify(drv).is_edge_endpoint())
        })
        .map(|(key, _)| key.as_str())
        .collect()
}

/// Every `.drv` key this scan emits as a component.
///
/// Test-only: production derives the same population by building the
/// components themselves, and a second production caller would be the second
/// copy of the rule `emitted_drv_keys`' comment warns about. The set form
/// exists so the subset relation can be asserted.
///
/// Milestone 1282 — the superset of [`emitted_drv_keys`]. Before this feature
/// the two sets were one set, and `emitted_drv_keys` carries a comment saying
/// why that mattered: *two predicates that have to agree is the drift this
/// project keeps paying for*. The divergence here is deliberate, so it is
/// asserted rather than assumed.
#[cfg(test)]
pub(crate) fn component_drv_keys<'a>(
    raw: &'a super::derivation::RawClosure,
    roles: &std::collections::BTreeMap<String, DerivationRole>,
) -> std::collections::HashSet<&'a str> {
    raw.derivations
        .iter()
        .filter(|(key, drv)| {
            roles
                .get(*key)
                .copied()
                .is_some_and(|role| emits_as_component(role, drv))
                && drv.pname().is_some()
        })
        .map(|(key, _)| key.as_str())
        .collect()
}

/// What `components` merges on: `(name, version, identity-hash)`.
///
/// The third element is empty for every member emitted before milestone 1282,
/// so those keys sort and behave exactly as the `(name, version)` pair they
/// replaced.
type MergeKey = (String, String, String);

/// A content hash, made safe for a PURL name segment **without losing
/// distinctness**.
///
/// nix writes `sha256-<base64>` or a bare base32 digest, so the characters to
/// deal with are `+`, `/`, `=` and the algorithm separator `-`.
/// `encode_purl_segment` normalises only `+`, which is not enough.
///
/// **The obvious encoding is wrong, and it was written here first.** Mapping
/// `+`→`p`, `/`→`s`, `=`→`e` is not injective: `p`, `s` and `e` are
/// themselves base64 characters, so the hashes `a+b` and `apb` both encode to
/// `apb` and two distinct artifacts compare equal — the exact collision
/// FR-002 exists to prevent, reintroduced by the function meant to preserve
/// identity.
///
/// `.` is the escape introducer because it appears in **no** hash alphabet —
/// not base64, not base32, not the algorithm prefix — and so is never passed
/// through. Every escape is therefore unambiguous and the mapping is
/// injective, which is what the identity requires.
fn encode_hash_segment(hash: &str) -> String {
    let mut out = String::with_capacity(hash.len() + 8);
    for c in hash.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' => out.push(c),
            '+' => out.push_str(".p"),
            '/' => out.push_str(".s"),
            '=' => out.push_str(".e"),
            '-' => out.push_str(".h"),
            '.' => out.push_str(".d"),
            // Unreachable for every hash nix emits, and encoded rather than
            // replaced anyway: a catch-all that collapsed unknown characters
            // to one symbol would be the same defect as the mapping above.
            other => out.push_str(&format!(".x{:04x}", other as u32)),
        }
    }
    out
}

/// The URL a fixed-output derivation fetched from (FR-008).
///
/// `urls` is the plural form `fetchurl` sets and may hold several
/// whitespace-separated mirrors; `url` is the singular older spelling. The
/// first is taken — the mirrors are alternative routes to one artifact whose
/// identity is already the content hash, so listing them all would suggest
/// several sources where there is one.
///
/// Carried as a CycloneDX-native `externalReference` of type `distribution`
/// rather than a `waybill:` property, per Constitution Principle V: a
/// download location is a thing every format already has a slot for.
/// Measured: 138 of 1,145 members record one.
fn source_url(drv: &RawDerivation) -> Option<&str> {
    drv.env
        .get("urls")
        .and_then(|v| v.split_whitespace().next())
        .or_else(|| drv.env.get("url").map(String::as_str))
        .filter(|u| !u.is_empty())
}

/// Build the components a classified closure contributes.
///
/// Returns them rather than mutating a caller's vector, so the supplementing
/// relationship is visible at the call site instead of buried here.
pub(crate) fn components(closure: &ClassifiedClosure) -> Vec<ResolvedComponent> {
    // Keyed by the identity a consumer sees, not by derivation, because a
    // closure holds several derivations per `(pname, version)` — build
    // variants. Measured on a real closure: 497 derivations emitted 348
    // distinct identities, so without this a third of the components were
    // duplicates carrying the same PURL, and `perl 5.42.0` appeared seven
    // times. A BTreeMap also fixes the output order independently of the
    // derivation hashes, which vary between machines.
    // The role travels beside the component rather than being read back out
    // of its own annotation: a round-trip through the wire string would make
    // merging depend on parsing what emission just wrote.
    // Milestone 1282 — the key gained a third element, and that is the whole
    // of research R8c. It was `(name, version)`; the members this feature
    // adds declare no `version` and keep their `name` unmodified (FR-016), so
    // all 56 members named `source` shared one key and merged into **one**
    // component — the collision FR-002, SC-014 and User Story 1 scenario 3
    // each forbid separately. The third element is the identity hash, and it
    // is empty for every member emitted before this feature, so their keys,
    // their ordering and their output are unchanged.
    let mut merged: std::collections::BTreeMap<
        MergeKey,
        (DerivationRole, Option<MemberRole>, ResolvedComponent),
    > = std::collections::BTreeMap::new();
    // C205 — unioned over build variants for the same reason the role is:
    // the component a consumer sees is one identity, and a reference through
    // any variant of it is a reference. Keyed by that identity rather than by
    // `.drv` path, which is what the index carries.
    let mut unclassified: std::collections::BTreeMap<
        MergeKey,
        std::collections::BTreeSet<String>,
    > = std::collections::BTreeMap::new();
    // Milestone 1281 — unioned over build variants for the same reason the
    // role and C205 are: the component a consumer sees is one identity, while
    // the license index is keyed by `.drv` path. Order is first-seen rather
    // than sorted, because FR-005 carries nixpkgs' stated order.
    let mut licenses_for: std::collections::BTreeMap<
        MergeKey,
        Vec<waybill_common::types::license::SpdxExpression>,
    > = std::collections::BTreeMap::new();
    for (key, drv) in &closure.raw.derivations {
        let Some(role) = closure.roles.get(key).copied() else {
            continue;
        };
        if !emits_as_component(role, drv) {
            continue;
        }
        let Some(name) = drv.pname() else {
            continue;
        };
        let version = drv.version().unwrap_or_default();

        // Milestone 1282 — `None` for every member emitted before this
        // feature, which is what keeps their identity and their key exactly
        // as they were.
        let member_role = is_1282_member(drv).then(|| member::classify(drv));
        let basis = member_role
            .is_some()
            .then(|| member::identity(drv, key));

        // `pkg:nix` is not a purl-spec type, so these carry `pkg:generic`
        // with the name and version nix recorded. A wrong-but-well-formed
        // PURL would be worse than a generic one: it would collide with
        // whatever really owns that coordinate.
        let coordinate = match &basis {
            // FR-002 — the hash occupies the **name segment**. A qualifier
            // does not satisfy the requirement: PURL equality is how
            // downstream tooling matches components, some tools compare
            // without qualifiers, and under such a comparison the 56 distinct
            // artifacts named `source` would collapse to one. That is
            // precisely the collision this identity exists to prevent.
            Some(b) => format!("pkg:generic/{}-{}", name, encode_hash_segment(b.hash())),
            None if version.is_empty() => format!("pkg:generic/{name}"),
            None => format!("pkg:generic/{name}@{version}"),
        };
        let identity_key = basis.as_ref().map(|b| b.hash().to_string()).unwrap_or_default();
        let merge_key: MergeKey = (name.to_string(), version.to_string(), identity_key);
        let Ok(purl) = Purl::new(&coordinate) else {
            tracing::debug!(name, "nix-closure: skipping a name that yields no valid PURL");
            continue;
        };

        let component = ResolvedComponent {
            build_inclusion: None,
            artifact_presence: None,
            name: name.to_string(),
            version: version.to_string(),
            purl,
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
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: Some("build".to_string()),
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
            external_references: match member_role.and(source_url(drv)) {
                Some(url) => vec![waybill_common::resolution::ExternalReference {
                    ref_type: "distribution".to_string(),
                    url: url.to_string(),
                }],
                None => vec![],
            },
            extra_annotations: Default::default(),
            binary_role: None,
        };
        if let Some(found) = closure.licenses.get(key) {
            let slot = licenses_for.entry(merge_key.clone()).or_default();
            for expr in found {
                if !slot.iter().any(|e| e.as_str() == expr.as_str()) {
                    slot.push(expr.clone());
                }
            }
        }
        if let Some(attributes) = closure.read_outcome.unclassified_refs.get(key) {
            unclassified
                .entry(merge_key.clone())
                .or_default()
                .extend(attributes.iter().cloned());
        }
        merged
            .entry(merge_key)
            // The variants are otherwise identical — same name, version and
            // PURL — so the role is the only thing that can differ, and it
            // does: measured, 13 of 97 duplicated identities disagree.
            //
            // FR-003 — this is also where the content-hash merge happens for
            // milestone 1282: two members sharing a name and a content hash
            // share a key, so they land here as one component. Measured, 549
            // fixed-output members become 502 components, and the merge is
            // correct — the same content fetched twice is one artifact.
            .and_modify(|(seen, _, _)| *seen = seen.union(role))
            .or_insert((role, member_role, component));
    }
    // Attribution runs here so the patch facts ride on the component itself.
    // One source of truth: CycloneDX translates it to native `pedigree`, the
    // two SPDX emitters carry it as an annotation, and nothing has to join
    // the two sides back together at emission time.
    let patched = super::patches::attribute(&closure.raw);
    let index: std::collections::HashMap<(&str, &str), &super::patches::ComponentPatches> =
        patched
            .iter()
            .map(|r| ((r.component.as_str(), r.version.as_deref().unwrap_or("")), r))
            .collect();

    merged
        .into_iter()
        .map(|(identity, (role, member_role, mut component))| {
            // FR-001 / FR-009 — the licenses land in the component's own
            // `licenses` field, which every emitter already carries to its
            // format's native license construct. `concluded_licenses` stays
            // empty: a nixpkgs `meta.license` is what the packager declares,
            // not a conclusion drawn from inspecting the artifact.
            if let Some(found) = licenses_for.remove(&identity) {
                component.licenses = found;
            }
            component.extra_annotations.insert(
                ANN_CLOSURE_ROLE.to_string(),
                serde_json::Value::String(role.wire().to_string()),
            );
            // FR-005 — exactly one role, on exactly the members this feature
            // adds. A component emitted before milestone 1282 carries none,
            // which is what lets a consumer tell the two populations apart
            // and what FR-007's filter relies on (a filter keeping only
            // `source` and `package` would otherwise discard all 381 named
            // packages).
            if let Some(m) = member_role {
                component.extra_annotations.insert(
                    member::ANN_MEMBER_ROLE.to_string(),
                    serde_json::Value::String(m.wire().to_string()),
                );
            }
            if let Some(attributes) = unclassified.get(&identity) {
                // Non-empty by construction: the index only records a member
                // an attribute actually named. Guarded anyway, because an
                // empty array would assert "checked, and nothing" in the one
                // slot whose absence already says that.
                if !attributes.is_empty() {
                    component.extra_annotations.insert(
                        ANN_CLOSURE_UNCLASSIFIED_REFS.to_string(),
                        serde_json::Value::String(
                            serde_json::Value::Array(
                                attributes
                                    .iter()
                                    .map(|a| serde_json::Value::String(a.clone()))
                                    .collect(),
                            )
                            .to_string(),
                        ),
                    );
                }
            }
            // Only for members that predate milestone 1282. A new member
            // has an empty `version`, so an unguarded lookup could attach a
            // named component's patch record to an unrelated source that
            // happens to share its name.
            if let Some(record) = (member_role.is_none())
                .then(|| index.get(&(component.name.as_str(), component.version.as_str())))
                .flatten()
            {
                if !record.patches.is_empty() {
                    component.extra_annotations.insert(
                        ANN_CLOSURE_PATCHES.to_string(),
                        serde_json::Value::String(
                            super::patches::patches_json(record).to_string(),
                        ),
                    );
                }
            }
            component
        })
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::super::classify::classify;
    use super::super::derivation::RawClosure;
    use super::*;

    const CLOSURE: &str = r#"{
      "derivations": {
        "d-lib.drv":  { "env": {"pname":"lib","version":"1.0"}, "outputs":{"out":{"path":"h1-lib"}} },
        "d-cc.drv":   { "env": {"pname":"cc","version":"13"},   "outputs":{"out":{"path":"h2-cc"}} },
        "d-orph.drv": { "env": {"pname":"orph","version":"9"},  "outputs":{"out":{"path":"h4-orph"}} },
        "d-app.drv": {
          "env": {"pname":"app","version":"2.0",
                  "buildInputs":"/nix/store/h1-lib",
                  "nativeBuildInputs":"/nix/store/h2-cc"},
          "outputs":{"out":{"path":"h5-app"}}
        }
      },
      "version": 3
    }"#;

    fn built() -> Vec<ResolvedComponent> {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let roles = classify(&raw);
        components(&ClassifiedClosure {
            attribute: "default".into(),
            raw,
            roles,
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
        })
    }

    #[test]
    fn emits_artifact_inputs_and_tooling_each_marked_with_its_role() {
        let c = built();
        // CONTROL: the fixture yields components at all, so the role
        // assertions below are not passing over an empty vector.
        assert_eq!(c.len(), 2, "expected lib and cc, got {:?}",
                   c.iter().map(|x| &x.name).collect::<Vec<_>>());

        let role_of = |name: &str| {
            c.iter()
                .find(|x| x.name == name)
                .and_then(|x| x.extra_annotations.get(ANN_CLOSURE_ROLE))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
        assert_eq!(role_of("lib").as_deref(), Some("artifact-input"));
        assert_eq!(role_of("cc").as_deref(), Some("build-tooling"));
    }

    #[test]
    fn build_tooling_is_emitted_rather_than_dropped() {
        // The clarified decision, and load-bearing beyond tidiness: the
        // richest vulnerability signal in both measured closures — `unzip`
        // with 11 CVEs — is build tooling. Dropping tooling would discard it.
        assert!(built().iter().any(|c| c.name == "cc"));
    }

    #[test]
    fn build_variants_merge_into_one_component_and_union_their_roles() {
        // A closure holds several derivations per (pname, version).
        // Measured on a real closure: 497 derivations, 348 distinct
        // identities, `perl 5.42.0` seven times — and 13 of 97 duplicated
        // identities disagreed about their role, so the union is not a
        // formality.
        const VARIANTS: &str = r#"{
          "derivations": {
            "v1.drv":  { "env": {"pname":"dup","version":"1.0"},
                         "outputs":{"out":{"path":"o1"}} },
            "v2.drv":  { "env": {"pname":"dup","version":"1.0"},
                         "outputs":{"out":{"path":"o2"}} },
            "app.drv": { "env": {"pname":"app","version":"2",
                                 "buildInputs":"/nix/store/o1",
                                 "nativeBuildInputs":"/nix/store/o2"},
                         "outputs":{"out":{"path":"o3"}} }
          },
          "version": 3
        }"#;
        let raw = RawClosure::parse(VARIANTS).unwrap();
        let roles = classify(&raw);
        // CONTROL: the two variants really do carry different roles, so the
        // union below is exercised rather than trivially satisfied.
        assert_eq!(roles["v1.drv"], DerivationRole::ArtifactInput);
        assert_eq!(roles["v2.drv"], DerivationRole::BuildTooling);

        let c = components(&ClassifiedClosure {
            attribute: "default".into(),
            raw,
            roles,
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
        });
        let dups: Vec<_> = c.iter().filter(|x| x.name == "dup").collect();
        assert_eq!(dups.len(), 1, "two variants, one component");
        assert_eq!(
            dups[0].extra_annotations[ANN_CLOSURE_ROLE],
            serde_json::Value::String("both".into()),
            "artifact-input unioned with build-tooling is both"
        );
    }

    #[test]
    fn unreferenced_is_absorbed_rather_than_winning_a_union() {
        // A component referenced through any variant is referenced.
        assert_eq!(
            DerivationRole::Both.union(DerivationRole::Unreferenced),
            DerivationRole::Both
        );
        assert_eq!(
            DerivationRole::Unreferenced.union(DerivationRole::Unreferenced),
            DerivationRole::Unreferenced
        );
    }

    #[test]
    fn an_unreferenced_member_that_applies_a_patch_is_still_a_component() {
        // FR-004a, and the reason it exists: measured, `jq` and `lua` are
        // Unreferenced in a real closure and carry 6 of its 18 CVEs. Without
        // this exception those patches have no component to attach to.
        const WITH_PATCHER: &str = r#"{
          "derivations": {
            "d-orph.drv": { "env": {"pname":"orph","version":"9"},
                            "outputs":{"out":{"path":"h4-orph"}} },
            "d-fix.drv":  { "env": {"pname":"fix","version":"1",
                                    "patches":"/nix/store/h-CVE-2020-1.patch"},
                            "outputs":{"out":{"path":"h6-fix"}} }
          },
          "version": 3
        }"#;
        let raw = RawClosure::parse(WITH_PATCHER).unwrap();
        let roles = classify(&raw);
        // CONTROL: both are Unreferenced, so the assertions below turn on the
        // patch field and not on some role difference between the two.
        assert_eq!(roles["d-orph.drv"], DerivationRole::Unreferenced);
        assert_eq!(roles["d-fix.drv"], DerivationRole::Unreferenced);

        let names: Vec<&str> = components(&ClassifiedClosure {
            attribute: "default".into(),
            raw,
            roles,
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
        })
        .iter()
        .map(|c| c.name.clone())
        .collect::<Vec<_>>()
        .leak()
        .iter()
        .map(String::as_str)
        .collect();
        assert_eq!(names, vec!["fix"], "the patch-applier is readmitted, the other is not");
    }

    #[test]
    fn unreferenced_members_are_not_components_here() {
        // Including the root: nothing depends on the thing being built. The
        // patch path handles the Unreferenced members that are in scope.
        let built = built();
        let named = |n: &str| built.iter().any(|c| c.name == n);
        assert!(!named("orph"));
        assert!(!named("app"));
    }

    #[test]
    fn every_component_carries_a_version_and_a_valid_purl() {
        for c in built() {
            assert!(!c.version.is_empty(), "{} has no version", c.name);
            assert!(c.purl.as_str().starts_with("pkg:generic/"));
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod m1282_tests {
    use super::super::classify::classify;
    use super::super::derivation::RawClosure;
    use super::*;

    fn fixture() -> RawClosure {
        RawClosure::parse(include_str!(
            "../../../../../tests/fixtures/nix_closure_members/closure.json"
        ))
        .unwrap()
    }

    #[test]
    fn m1282_t031_edge_endpoints_are_a_subset_of_emitted_components() {
        // **The assertion this milestone exists to keep honest.**
        //
        // `emitted_drv_keys` carries a comment warning that *two predicates
        // that have to agree is the drift this project keeps paying for*, and
        // milestone 1282 breaks that identity deliberately: after FR-001 the
        // emitted set is every member, and after FR-013 the endpoint set is
        // only the `pname`-bearing members plus `source` and `package`.
        //
        // Emission is therefore a strict **superset** of edge participation.
        // That relation is what keeps the divergence from becoming a
        // disagreement, so it is asserted rather than trusted. An endpoint
        // outside the emitted set is an edge naming a component the document
        // does not contain — the thing FR-013's last sentence forbids.
        let raw = fixture();
        let roles = classify(&raw);
        let endpoints = emitted_drv_keys(&raw, &roles);
        let components = component_drv_keys(&raw, &roles);

        for key in &endpoints {
            assert!(
                components.contains(key),
                "edge endpoint {key} is not an emitted component"
            );
        }

        // CONTROL: the two sets must actually DIFFER on this fixture, or the
        // subset assertion above holds vacuously and would keep passing if
        // the feature stopped emitting the 513.
        assert!(
            components.len() > endpoints.len(),
            "emission must be a STRICT superset here; \
             components={} endpoints={} — if they are equal this test proves nothing",
            components.len(),
            endpoints.len()
        );
    }

    #[test]
    fn m1282_t016_identical_content_merges_and_differing_content_does_not() {
        let raw = fixture();
        let roles = classify(&raw);
        let keys = component_drv_keys(&raw, &roles);

        // Both members named `source` carry different hashes: two components.
        assert!(keys.contains("/nix/store/cccccccc-source.drv"));
        assert!(keys.contains("/nix/store/eeeeeeee-source.drv"));
        // Both `waybill-fixture-tarball` members carry the SAME hash, under
        // the two different spellings: one component after the merge, which
        // the component-level test in `closure_members.rs` asserts.
        assert!(keys.contains("/nix/store/ffffffff-waybill-fixture-tarball.drv"));
        assert!(keys.contains("/nix/store/11111111-waybill-fixture-tarball.drv"));
    }

    #[test]
    fn m1282_the_hash_segment_encoding_is_injective() {
        // The obvious encoding (`+`->`p`, `/`->`s`, `=`->`e`) is NOT, because
        // those letters are themselves base64 characters: `a+b` and `apb`
        // both become `apb`. That version was written here first and would
        // have re-created the collision FR-002 exists to prevent, inside the
        // function meant to preserve identity.
        //
        // Exhaustive over every string up to length 4 drawn from the full
        // character set a nix hash can contain.
        let alphabet = ['A', 'B', 'Z', 'a', 'z', '0', '9', '+', '/', '=', '-', '.'];
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut count = 0usize;
        for n in 1..=4 {
            let mut idx = vec![0usize; n];
            loop {
                let s: String = idx.iter().map(|&i| alphabet[i]).collect();
                let e = encode_hash_segment(&s);
                if let Some(prev) = seen.insert(e.clone(), s.clone()) {
                    assert_eq!(prev, s, "collision: {prev:?} and {s:?} both encode to {e:?}");
                }
                count += 1;
                let mut k = n;
                loop {
                    if k == 0 {
                        break;
                    }
                    k -= 1;
                    idx[k] += 1;
                    if idx[k] < alphabet.len() {
                        break;
                    }
                    idx[k] = 0;
                    if k == 0 {
                        break;
                    }
                }
                if idx.iter().all(|&i| i == 0) {
                    break;
                }
            }
        }
        assert!(count > 20_000, "expected an exhaustive sweep, ran {count}");

        // And the output is safe in a PURL name segment.
        for encoded in seen.keys() {
            assert!(
                encoded
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.'),
                "unsafe character in {encoded:?}"
            );
        }
    }
}
