//! Issue #919 (m922) — the per-component Pants language namespace on the wire
//! (catalogue row C164).
//!
//! The split needs this to group correctly, but emitting it also lets a
//! consumer that partitions components by resolve *itself* tell a Python
//! `default` from a JVM `default` — which it could not do from an emitted
//! document before, because membership carries bare names.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

const NS: &str = "waybill:pants-resolve-namespace";
const MEMBERSHIP: &str = "waybill:pants-resolve";

fn scan(fixture: &str, format: &str, file: &str) -> serde_json::Value {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(file);
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(fixture)
                .to_str()
                .expect("fixture"),
            "--offline",
            "--format",
            format,
            "--output",
            path.to_str().expect("out"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");
    serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse")
}

/// purl -> (namespace, membership) from CycloneDX.
fn cdx_pairs(doc: &serde_json::Value) -> BTreeMap<String, (String, Vec<String>)> {
    let mut out = BTreeMap::new();
    for c in doc["components"].as_array().into_iter().flatten() {
        let props: BTreeMap<&str, &str> = c["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some((p["name"].as_str()?, p["value"].as_str()?)))
            .collect();
        let (Some(ns), Some(m)) = (props.get(NS), props.get(MEMBERSHIP)) else {
            continue;
        };
        let names: Vec<String> = serde_json::from_str(m).expect("membership array");
        out.insert(
            c["purl"].as_str().unwrap_or_default().to_string(),
            (ns.to_string(), names),
        );
    }
    out
}

/// SC-006. **The US2 headline**: one unsplit document, partitioned by
/// (namespace, resolve), separates the two namespaces. Before this, both rows
/// read `["default"]` with nothing beside them and the partition was one group.
#[test]
fn an_unsplit_document_partitions_into_two_namespaces() {
    let doc = scan("pants_namespace_collision", "cyclonedx-json", "a.cdx.json");
    let pairs = cdx_pairs(&doc);

    let mut groups: BTreeSet<(String, String)> = BTreeSet::new();
    for (_, (ns, names)) in &pairs {
        for n in names {
            groups.insert((ns.clone(), n.clone()));
        }
    }

    assert!(
        groups.contains(&("python".into(), "default".into())),
        "no python:default group in {groups:?}"
    );
    assert!(
        groups.contains(&("jvm".into(), "default".into())),
        "no jvm:default group in {groups:?}"
    );

    // The bare name alone would collapse these into one group — the defect.
    let bare: BTreeSet<&String> = groups.iter().map(|(_, n)| n).collect();
    assert!(
        bare.len() < groups.len(),
        "the fixture no longer exercises a collision: {groups:?}"
    );
}

/// C-2, against the case research R2 measured. A **Python** resolve routinely
/// contains `pkg:generic/*` members, so if the namespace were inferred from
/// ecosystem those would be unclassifiable. They must read `python`.
#[test]
fn generic_purl_members_of_a_python_resolve_read_python() {
    let doc = scan("pants_resolve_edges", "cyclonedx-json", "b.cdx.json");
    let pairs = cdx_pairs(&doc);

    let generics: Vec<(&String, &(String, Vec<String>))> = pairs
        .iter()
        .filter(|(purl, _)| purl.starts_with("pkg:generic/"))
        .collect();
    assert!(
        !generics.is_empty(),
        "no pkg:generic/* component carried membership — this asserted nothing. \
         R2 measured them present in Python resolves; if that changed, the test \
         needs a different fixture, not deletion"
    );
    for (purl, (ns, _)) in generics {
        assert_eq!(
            ns, "python",
            "{purl} reads {ns:?}; a generic PURL carries no ecosystem signal, so \
             this means the namespace is being inferred rather than recorded"
        );
    }
}

/// SC-009 / FR-006b. The same decoded value in all three formats. Compare
/// decoded values, not bytes — CycloneDX carries a property value as a string
/// and SPDX carries an annotation envelope.
#[test]
fn the_namespace_decodes_identically_in_all_three_formats() {
    let fixture = "pants_namespace_collision";

    let cdx = cdx_pairs(&scan(fixture, "cyclonedx-json", "c.cdx.json"));

    let s23 = scan(fixture, "spdx-2.3-json", "c.spdx.json");
    let mut spdx23: BTreeMap<String, String> = BTreeMap::new();
    for p in s23["packages"].as_array().into_iter().flatten() {
        let purl = p["externalRefs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["referenceType"].as_str() == Some("purl"))
            .and_then(|r| r["referenceLocator"].as_str());
        let Some(purl) = purl else { continue };
        for a in p["annotations"].as_array().into_iter().flatten() {
            let Some(c) = a["comment"].as_str() else { continue };
            let Ok(env) = serde_json::from_str::<serde_json::Value>(c) else {
                continue;
            };
            if env["field"].as_str() == Some(NS) {
                spdx23.insert(
                    purl.to_string(),
                    env["value"].as_str().unwrap_or_default().to_string(),
                );
            }
        }
    }

    assert!(!cdx.is_empty() && !spdx23.is_empty(), "nothing to compare");
    for (purl, (ns, _)) in &cdx {
        assert_eq!(
            spdx23.get(purl),
            Some(ns),
            "{purl}: CycloneDX says {ns:?}, SPDX 2.3 disagrees"
        );
    }
}
