//! Spec 1531 (#1531), User Story 2: `Requires` and `Requires.private`
//! become dependency edges by exact PURL (SC-002).

#![allow(clippy::unwrap_used)]

mod common;

use common::installed_lib::{component, coverage, depends_on, prop, row_all_formats, scan, write};

#[test]
fn sc002_requires_become_edges_and_private_only_targets_are_annotated() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    write(
        &root,
        "opt/sdk/lib/pkgconfig/waybill-fixture-a.pc",
        "version=1.2.3\nVersion: ${version}\nRequires: waybill-fixture-c >= 1.0, waybill-fixture-gone\nRequires.private: waybill-fixture-d\n",
    );
    write(&root, "opt/sdk/lib/pkgconfig/waybill-fixture-c.pc", "Version: 2.0\n");
    write(&root, "opt/sdk/lib/pkgconfig/waybill-fixture-d.pc", "Version: 3.0\n");
    // Another prefix's `c`, at a different version: the same prefix wins.
    write(&root, "opt/other/lib/pkgconfig/waybill-fixture-c.pc", "Version: 9.0\n");
    let docs = scan(&root);

    let a = "pkg:generic/waybill-fixture-a@1.2.3";
    assert_eq!(depends_on(&docs.cdx, a), vec!["pkg:generic/waybill-fixture-c@2.0", "pkg:generic/waybill-fixture-d@3.0"]);
    assert!(component(&docs.cdx, "pkg:generic/waybill-fixture-c@9.0").is_some(), "control: the other prefix's c exists");
    assert_eq!(
        prop(component(&docs.cdx, a).unwrap(), "waybill:pkg-config-private-requires"),
        Some(r#"["pkg:generic/waybill-fixture-d@3.0"]"#)
    );
    assert!(
        !common::installed_lib::components(&docs.cdx)
            .iter()
            .any(|c| c["name"].as_str().is_some_and(|n| n.contains("gone"))),
        "an unresolved requirement fabricates no component"
    );
    assert_eq!(coverage(&docs).unwrap()["unresolved-requires"], 1);

    // C224 agrees across the three formats, and the edges reach SPDX too.
    let c224 = row_all_formats(&docs, "C224");
    assert_eq!(c224.len(), 1, "{c224:?}");
    let spdx_deps = docs.spdx23["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["relationshipType"] == "DEPENDS_ON")
        .count();
    assert!(spdx_deps >= 2, "SPDX 2.3 carries the edges: {spdx_deps}");
}
