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
use super::ClassifiedClosure;

/// C182 — how nix referenced this member.
pub(crate) const ANN_CLOSURE_ROLE: &str = "waybill:closure-role";

/// Roles that become components.
///
/// `Unreferenced` is excluded here, but **not** because it is irrelevant:
/// members in that role which apply a patch are in scope (spec FR-004a), and
/// that is handled by the patch path rather than by this filter. Measured,
/// `jq` and `lua` are `Unreferenced` and carry 6 of one project's 18 CVEs.
fn emits_as_component(role: DerivationRole) -> bool {
    matches!(
        role,
        DerivationRole::ArtifactInput | DerivationRole::BuildTooling | DerivationRole::Both
    )
}

/// Build the components a classified closure contributes.
///
/// Returns them rather than mutating a caller's vector, so the supplementing
/// relationship is visible at the call site instead of buried here.
pub(crate) fn components(closure: &ClassifiedClosure) -> Vec<ResolvedComponent> {
    let mut out = Vec::new();
    for (key, drv) in &closure.raw.derivations {
        let Some(role) = closure.roles.get(key).copied() else {
            continue;
        };
        if !emits_as_component(role) {
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

        let mut component = ResolvedComponent {
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
        component.extra_annotations.insert(
            ANN_CLOSURE_ROLE.to_string(),
            serde_json::Value::String(role.wire().to_string()),
        );
        out.push(component);
    }
    out
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
