//! Spec 1531 (#1531), User Story 3: a library's evidence lists its
//! prefix's matching shared libraries (SC-008), and a `.pc` and an
//! embedded version string for the same library merge (SC-003).

#![allow(clippy::unwrap_used)]

mod common;

use common::installed_lib::{component, components, elf_shared_lib, prop, scan, source_files, write};
use serde_json::Value;

fn put_bytes(root: &std::path::Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// The file component of `name`, with its PURL and properties; the scan's
/// own fields that legitimately differ (bom-ref) are kept as is.
fn file_component(cdx: &Value, name: &str) -> Value {
    components(cdx).into_iter().find(|c| c["name"] == name).cloned().unwrap_or(Value::Null)
}

#[test]
fn sc008_matching_shared_libraries_are_listed_and_left_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    write(&root, "opt/sdk/lib/pkgconfig/waybill-fixture-a.pc", "Version: 1.2.3\n");
    put_bytes(&root, "opt/sdk/lib/libwaybill-fixture-a.so.1.2.3", &elf_shared_lib(b"\0nothing\0"));
    put_bytes(&root, "opt/sdk/lib/libwaybill-fixture-zz.so", &elf_shared_lib(b"\0other\0"));
    let docs = scan(&root);

    let a = component(&docs.cdx, "pkg:generic/waybill-fixture-a@1.2.3").unwrap();
    assert_eq!(
        source_files(a),
        vec!["opt/sdk/lib/libwaybill-fixture-a.so.1.2.3", "opt/sdk/lib/pkgconfig/waybill-fixture-a.pc"]
    );
    let zz = file_component(&docs.cdx, "libwaybill-fixture-zz.so");
    assert!(!zz.is_null(), "control: the unmatched library is a file component");

    // The matched binary's own component is unchanged by the `.pc`.
    let so = file_component(&docs.cdx, "libwaybill-fixture-a.so.1.2.3");
    assert!(!so.is_null(), "the shared library is a file component");
    std::fs::remove_file(root.join("opt/sdk/lib/pkgconfig/waybill-fixture-a.pc")).unwrap();
    let without = scan(&root);
    let so_without = file_component(&without.cdx, "libwaybill-fixture-a.so.1.2.3");
    let strip = |mut v: Value| {
        v["bom-ref"] = Value::Null;
        v
    };
    assert_eq!(strip(so), strip(so_without), "the binary's component does not change");
}

/// SC-003 / T029: zlib from its `.pc` and from the binary's embedded
/// version string is one component, keeping the `.pc`'s mechanism and tier.
/// Control: without the `.pc` the same component is `analyzed`.
#[test]
fn sc003_a_pc_and_an_embedded_version_merge() {
    let zlib = elf_shared_lib(b"\0 deflate 1.3.1 Copyright 1995-2024 Jean-loup Gailly and Mark Adler \0");
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    write(&root, "opt/sdk/lib/pkgconfig/zlib.pc", "Version: 1.3.1\n");
    put_bytes(&root, "opt/sdk/lib/libz.so.1.3.1", &zlib);
    let docs = scan(&root);
    let matching: Vec<&Value> =
        components(&docs.cdx).into_iter().filter(|c| c["purl"] == "pkg:generic/zlib@1.3.1").collect();
    assert_eq!(matching.len(), 1, "one zlib component");
    let z = matching[0];
    assert_eq!(prop(z, "waybill:source-mechanism"), Some("pkg-config"));
    assert_eq!(prop(z, "waybill:sbom-tier"), Some("deployed"));
    assert!(source_files(z).contains(&"opt/sdk/lib/libz.so.1.3.1".to_string()), "{:?}", source_files(z));

    std::fs::remove_file(root.join("opt/sdk/lib/pkgconfig/zlib.pc")).unwrap();
    let control = scan(&root);
    let z = component(&control.cdx, "pkg:generic/zlib@1.3.1").expect("control: the scanner reads the string");
    assert_eq!(prop(z, "waybill:sbom-tier"), Some("analyzed"));
}
