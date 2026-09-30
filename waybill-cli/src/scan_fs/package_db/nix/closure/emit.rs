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
use super::ClassifiedClosure;

/// C182 — how nix referenced this member.
pub(crate) const ANN_CLOSURE_ROLE: &str = "waybill:closure-role";

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
fn emits_as_component(role: DerivationRole, drv: &RawDerivation) -> bool {
    role_alone_emits(role) || drv.paths_in("patches").next().is_some()
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
    let mut merged: std::collections::BTreeMap<
        (String, String),
        (DerivationRole, ResolvedComponent),
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

        // `pkg:nix` is not a purl-spec type, so these carry `pkg:generic`
        // with the name and version nix recorded. A wrong-but-well-formed
        // PURL would be worse than a generic one: it would collide with
        // whatever really owns that coordinate.
        let coordinate = if version.is_empty() {
            format!("pkg:generic/{name}")
        } else {
            format!("pkg:generic/{name}@{version}")
        };
        let Ok(purl) = Purl::new(&coordinate) else {
            tracing::debug!(name, "nix-closure: skipping a name that yields no valid PURL");
            continue;
        };

        let component = ResolvedComponent {
            build_inclusion: None,
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
            external_references: vec![],
            extra_annotations: Default::default(),
            binary_role: None,
        };
        merged
            .entry((name.to_string(), version.to_string()))
            // The variants are otherwise identical — same name, version and
            // PURL — so the role is the only thing that can differ, and it
            // does: measured, 13 of 97 duplicated identities disagree.
            .and_modify(|(seen, _)| *seen = seen.union(role))
            .or_insert((role, component));
    }
    merged
        .into_values()
        .map(|(role, mut component)| {
            component.extra_annotations.insert(
                ANN_CLOSURE_ROLE.to_string(),
                serde_json::Value::String(role.wire().to_string()),
            );
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
