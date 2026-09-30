//! CycloneDX `component.pedigree.patches[]`.
//!
//! waybill's first use of `pedigree`, so this is new emission machinery
//! rather than a new field on an existing path.
//!
//! It exists because nixpkgs backports security fixes without moving a
//! version string. `unzip` is 6.0 in both measured closures — unchanged
//! since 2009 — and carries 11 CVEs across its patch set. A version-keyed
//! SBOM cannot say "this build is patched" and cannot say "this version was
//! considered vulnerable"; `pedigree` is the native place CycloneDX keeps
//! that, and the only one of the three formats that has it.

use serde_json::{json, Value};

use crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_PATCHES;

/// Build the `pedigree` object from the annotation the closure emitter
/// stamped on the component.
///
/// Reading the component's own annotation rather than re-joining against the
/// closure means CycloneDX and SPDX render the same stored fact, and neither
/// can drift from the other.
pub(crate) fn from_annotation(
    component: &waybill_common::resolution::ResolvedComponent,
) -> Option<Value> {
    let raw = component
        .extra_annotations
        .get(ANN_CLOSURE_PATCHES)?
        .as_str()?;
    let patches: Value = serde_json::from_str(raw).ok()?;
    // Absent rather than empty: an empty `patches[]` would assert the closure
    // was consulted and held nothing, a different claim from the component
    // never having been in a closure.
    if patches.as_array().is_none_or(|a| a.is_empty()) {
        return None;
    }
    Some(json!({ "patches": patches }))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::nix::closure::classify::classify;
    use crate::scan_fs::package_db::nix::closure::derivation::RawClosure;
    use crate::scan_fs::package_db::nix::closure::{emit, ClassifiedClosure};

    const CLOSURE: &str = r#"{
      "derivations": {
        "d-z.drv": { "env": {"pname":"zippy","version":"6.0",
                             "patches":"/nix/store/h-CVE-2019-13232-1.patch /nix/store/h-tidy.patch"},
                     "outputs":{"out":{"path":"o1"}} },
        "d-p.drv": { "env": {"pname":"plain","version":"2.0"},
                     "outputs":{"out":{"path":"o3"}} },
        "d-a.drv": { "env": {"pname":"app","version":"1.0",
                             "buildInputs":"/nix/store/o1 /nix/store/o3"},
                     "outputs":{"out":{"path":"o2"}} }
      },
      "version": 3
    }"#;

    fn components() -> Vec<waybill_common::resolution::ResolvedComponent> {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let roles = classify(&raw);
        emit::components(&ClassifiedClosure { attribute: "default".into(), raw, roles })
    }

    #[test]
    fn a_cve_named_patch_resolves_a_security_issue() {
        let cs = components();
        // CONTROL: the patch-applier is present at all, so the shape
        // assertions below are not passing over an empty document.
        let z = cs.iter().find(|c| c.name == "zippy").expect("zippy emitted");
        let p = from_annotation(z).expect("pedigree from the stamped annotation");
        let patches = p["patches"].as_array().unwrap();
        assert_eq!(patches.len(), 2);

        let with_cve: Vec<&Value> =
            patches.iter().filter(|x| x.get("resolves").is_some()).collect();
        assert_eq!(with_cve.len(), 1);
        assert_eq!(with_cve[0]["type"], "backport");
        assert_eq!(with_cve[0]["resolves"][0]["type"], "security");
        assert_eq!(with_cve[0]["resolves"][0]["id"], "CVE-2019-13232");
    }

    #[test]
    fn a_patch_naming_no_cve_is_recorded_without_resolves() {
        // Recorded rather than dropped: measured, this is ~90% of them, and
        // dropping them would make partial coverage read as absence.
        let cs = components();
        let z = cs.iter().find(|c| c.name == "zippy").unwrap();
        let p = from_annotation(z).unwrap();
        let bare: Vec<&Value> = p["patches"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x.get("resolves").is_none())
            .collect();
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0]["type"], "backport");
    }

    #[test]
    fn a_component_applying_nothing_gets_no_pedigree_at_all() {
        let cs = components();
        // `plain` is emitted — it is an artifact input — but applies no
        // patch. `app` would not do: it is the unreferenced root and is not
        // emitted at all, so it could not distinguish "no patches" from
        // "no component".
        let a = cs.iter().find(|c| c.name == "plain").expect("plain emitted");
        assert!(
            !a.extra_annotations.contains_key(ANN_CLOSURE_PATCHES),
            "no annotation is stamped when nothing is applied"
        );
        assert!(from_annotation(a).is_none(), "absent, not empty");
    }

    #[test]
    fn the_annotation_and_the_native_field_carry_the_same_array() {
        // The point of reading the component's own annotation: SPDX renders
        // that string and CycloneDX renders this object, so if they were
        // built separately they could disagree about one fact.
        let cs = components();
        let z = cs.iter().find(|c| c.name == "zippy").unwrap();
        let stored: Value =
            serde_json::from_str(z.extra_annotations[ANN_CLOSURE_PATCHES].as_str().unwrap())
                .unwrap();
        assert_eq!(from_annotation(z).unwrap()["patches"], stored);
    }
}
