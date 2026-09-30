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

use crate::scan_fs::package_db::nix::closure::patches::ComponentPatches;

/// Build the `pedigree` object for one component, or `None` when it applies
/// no patches.
///
/// Absent rather than empty: an empty `patches[]` asserts that the closure
/// was consulted and held nothing, which is a different claim from the
/// component not having been in the closure at all.
pub(crate) fn pedigree_for(record: &ComponentPatches) -> Option<Value> {
    if record.patches.is_empty() {
        return None;
    }
    let patches: Vec<Value> = record
        .patches
        .iter()
        .map(|p| {
            // `type` is the only required member. The enum is
            // ['unofficial','monkey','backport','cherry-pick']; a nixpkgs
            // patch applied over a released version is a backport.
            let mut obj = json!({ "type": "backport" });
            let resolves: Vec<Value> = p
                .resolves
                .iter()
                .map(|cve| json!({ "type": "security", "id": cve.id }))
                .collect();
            // A patch naming no CVE is still recorded, with no `resolves`
            // (spec FR-010). Silence about it would make partial coverage
            // read as absence — and measured, 89% and 91% of the patches in
            // the two closures name no CVE, so that is the common case.
            if !resolves.is_empty() {
                obj["resolves"] = Value::Array(resolves);
            }
            obj
        })
        .collect();
    Some(json!({ "patches": patches }))
}

/// Index the records by the `(name, version)` a CycloneDX component is
/// matched on.
pub(crate) fn index(
    records: &[ComponentPatches],
) -> std::collections::HashMap<(&str, &str), &ComponentPatches> {
    records
        .iter()
        .map(|r| {
            (
                (r.component.as_str(), r.version.as_deref().unwrap_or("")),
                r,
            )
        })
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::nix::closure::derivation::RawClosure;
    use crate::scan_fs::package_db::nix::closure::patches::attribute;

    const CLOSURE: &str = r#"{
      "derivations": {
        "d-z.drv": { "env": {"pname":"zippy","version":"6.0",
                             "patches":"/nix/store/h-CVE-2019-13232-1.patch /nix/store/h-tidy.patch"},
                     "outputs":{"out":{"path":"o1"}} },
        "d-q.drv": { "env": {"pname":"quiet","version":"1.0"},
                     "outputs":{"out":{"path":"o2"}} }
      },
      "version": 3
    }"#;

    fn records() -> Vec<ComponentPatches> {
        attribute(&RawClosure::parse(CLOSURE).unwrap())
    }

    #[test]
    fn a_cve_named_patch_resolves_a_security_issue() {
        let r = records();
        // CONTROL: attribution produced a record at all, so the shape
        // assertions below are not passing over an empty list.
        assert_eq!(r.len(), 1, "only the patch-applier is recorded");
        let p = pedigree_for(&r[0]).unwrap();
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
        let r = records();
        let p = pedigree_for(&r[0]).unwrap();
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
        let empty = ComponentPatches {
            component: "quiet".into(),
            version: Some("1.0".into()),
            patches: vec![],
        };
        assert!(pedigree_for(&empty).is_none(), "absent, not empty");
    }

    #[test]
    fn the_index_keys_on_name_and_version() {
        let r = records();
        let idx = index(&r);
        assert!(idx.contains_key(&("zippy", "6.0")));
        assert!(!idx.contains_key(&("zippy", "7.0")), "version is part of the key");
    }
}
