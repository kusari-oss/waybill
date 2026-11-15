//! Spec 1532 (#1563): installed files attributed to their package through
//! CMake exported targets. Synthetic prefixes in a tempdir; nothing is
//! fetched.

#![allow(clippy::unwrap_used)]

mod common;

use common::installed_lib::{components, elf_shared_lib, scan};
use sha2::{Digest, Sha256};

fn put_bytes(root: &std::path::Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// Research R5 (T002): a binary file component's `file-sha256` qualifier is
/// the SHA-256 of the whole file, so reusing it for evidence is sound.
#[test]
fn a_binary_components_file_sha256_is_the_whole_file_digest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    let bytes = elf_shared_lib(b"\0waybill-fixture\0");
    put_bytes(&root, "opt/sdk/lib/libwaybill-fixture-h.so.1", &bytes);
    let want = format!("{:x}", Sha256::digest(&bytes));
    let docs = scan(&root);
    let purl = components(&docs.cdx)
        .into_iter()
        .find(|c| c["name"] == "libwaybill-fixture-h.so.1")
        .and_then(|c| c["purl"].as_str().map(str::to_string))
        .expect("the shared library is a file component");
    assert!(purl.contains(&format!("file-sha256={want}")), "{purl} vs {want}");
}

use common::installed_lib::{component, coverage, dpkg_owning, prop, row_all_formats, source_files, write, Docs};
use serde_json::Value;

const MAIN_P: &str = include_str!("../src/scan_fs/package_db/installed_lib/testdata/WaybillFixturePTargets.cmake.txt");
const RELEASE_P: &str =
    include_str!("../src/scan_fs/package_db/installed_lib/testdata/WaybillFixturePTargets-release.cmake.txt");

/// A main targets file for a package directory `depth` levels below the
/// install prefix: `_IMPORT_PREFIX` is the directory raised `depth` times.
fn main_file(depth: usize) -> String {
    let mut s = String::from("get_filename_component(_IMPORT_PREFIX \"${CMAKE_CURRENT_LIST_FILE}\" PATH)\n");
    for _ in 0..depth {
        s.push_str("get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n");
    }
    s
}

fn release_file(locations: &[&str]) -> String {
    locations
        .iter()
        .enumerate()
        .map(|(i, l)| format!("set_target_properties(wf::t{i} PROPERTIES\n  IMPORTED_LOCATION_RELEASE \"{l}\"\n  )\n"))
        .collect()
}

fn exported(docs: &Docs) -> Value {
    coverage(docs).expect("C225 present")["exported-targets"].clone()
}

/// The US1 prefix (tasks T006): a package with CMake-generated targets
/// files (shared library, static archive, executable), a lower-case
/// package naming a shipped and a missing file whose shared library also
/// matches its `.pc`, and an `Exports` package.
fn us1_tree(root: &std::path::Path, with_targets: bool) {
    write(root, "opt/sdk/lib/cmake/WaybillFixtureP/WaybillFixturePConfigVersion.cmake", "set(PACKAGE_VERSION \"4.5.6\")\n");
    put_bytes(root, "opt/sdk/lib/libops.a", b"!<arch>\n");
    put_bytes(root, "opt/sdk/lib/libp.4.5.6.dylib", &elf_shared_lib(b"\0wfp\0"));
    put_bytes(root, "opt/sdk/bin/tool", b"#!/bin/sh\n");
    write(root, "opt/sdk/lib/cmake/waybill-fixture-q/waybill-fixture-q-config-version.cmake", "set(PACKAGE_VERSION \"2.0\")\n");
    write(root, "opt/sdk/lib/pkgconfig/waybill-fixture-q.pc", "Version: 2.0\n");
    put_bytes(root, "opt/sdk/lib/libwaybill-fixture-q.so.2", &elf_shared_lib(b"\0wfq\0"));
    write(root, "opt/sdk/lib/cmake/WaybillFixtureE/WaybillFixtureEConfigVersion.cmake", "set(PACKAGE_VERSION \"3.0\")\n");
    put_bytes(root, "opt/sdk/lib/libwaybill-fixture-e-core.a", b"!<arch>\n");
    if with_targets {
        write(root, "opt/sdk/lib/cmake/WaybillFixtureP/WaybillFixturePTargets.cmake", MAIN_P);
        write(root, "opt/sdk/lib/cmake/WaybillFixtureP/WaybillFixturePTargets-release.cmake", RELEASE_P);
        write(root, "opt/sdk/lib/cmake/waybill-fixture-q/waybill-fixture-q-targets.cmake", &main_file(3));
        write(
            root,
            "opt/sdk/lib/cmake/waybill-fixture-q/waybill-fixture-q-targets-release.cmake",
            &release_file(&["${_IMPORT_PREFIX}/lib/libwaybill-fixture-q.so.2", "${_IMPORT_PREFIX}/lib/libnot-shipped.so"]),
        );
        write(root, "opt/sdk/lib/cmake/WaybillFixtureE/WaybillFixtureEExports.cmake", &main_file(3));
        write(
            root,
            "opt/sdk/lib/cmake/WaybillFixtureE/WaybillFixtureEExports-release.cmake",
            &release_file(&["${_IMPORT_PREFIX}/lib/libwaybill-fixture-e-core.a"]),
        );
    }
}

fn occurrences(c: &Value) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = c["evidence"]["occurrences"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| {
            // The digest is inside `additionalContext`, a JSON string.
            let ctx: Value = o["additionalContext"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
            (o["location"].as_str().unwrap_or("").to_string(), ctx["sha256"].as_str().unwrap_or("").to_string())
        })
        .collect();
    v.sort();
    v
}

/// US1 scenarios 1-5, SC-001 and FR-005a.
#[test]
fn us1_exported_files_are_attributed_to_their_package() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    us1_tree(&root, true);
    let docs = scan(&root);

    let p = component(&docs.cdx, "pkg:generic/waybillfixturep@4.5.6").expect("P");
    let p_files = source_files(p);
    for f in ["opt/sdk/lib/libops.a", "opt/sdk/lib/libp.4.5.6.dylib", "opt/sdk/bin/tool"] {
        assert!(p_files.contains(&f.to_string()), "{f} in {p_files:?}");
    }
    let q = component(&docs.cdx, "pkg:generic/waybill-fixture-q@2.0").expect("Q");
    let q_files = source_files(q);
    assert_eq!(
        q_files.iter().filter(|f| f.ends_with("libwaybill-fixture-q.so.2")).count(),
        1,
        "listed once though both the .pc name match and the targets name it: {q_files:?}"
    );
    assert!(!q_files.iter().any(|f| f.contains("not-shipped")));
    let e = component(&docs.cdx, "pkg:generic/waybillfixturee@3.0").expect("E");
    assert!(source_files(e).contains(&"opt/sdk/lib/libwaybill-fixture-e-core.a".to_string()));

    // Occurrences carry a hash; the shared library's is its binary
    // component's file-sha256 (FR-005a).
    let occ = occurrences(p);
    let lib = occ.iter().find(|(l, _)| l.ends_with("libp.4.5.6.dylib")).expect("occurrence");
    assert!(lib.1.len() == 64 && lib.1.chars().all(|c| c.is_ascii_hexdigit()), "a real digest: {lib:?}");
    let bin_purl = components(&docs.cdx)
        .into_iter()
        .find(|c| c["name"] == "libp.4.5.6.dylib")
        .and_then(|c| c["purl"].as_str().map(str::to_string))
        .expect("binary component");
    assert!(bin_purl.contains(&format!("file-sha256={}", lib.1)), "{} vs {bin_purl}", lib.1);
    assert!(occ.iter().all(|(_, h)| !h.is_empty()), "every attributed file is hashed: {occ:?}");

    // FR-005a (analysis U1): reuse is observable, not just digest-equal.
    let line = docs.stderr.lines().find(|l| l.contains("hash cache: seeded=")).expect("hash cache log line");
    let reused: usize = line
        .split("reused=")
        .nth(1)
        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .expect("reused count");
    assert!(reused >= 1, "an attributed shared library reused its binary hash: {line}");

    // C225 (FR-007), identical in all three formats.
    assert_eq!(
        exported(&docs),
        serde_json::json!({"attached":5,"locations":6,"no-package-version":0,"not-shipped":1,"owned-by-os-package":0,"read":6,"unresolved-location":0})
    );
    let c18 = row_all_formats(&docs, "C18");
    assert!(c18.iter().any(|v| v.contains("libops.a")), "C18 carries the archive in all formats");
}

/// SC-001 control: without the targets files, none of that evidence, and
/// no `exported-targets` key.
#[test]
fn sc001_control_without_targets_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    us1_tree(&root, false);
    let docs = scan(&root);
    let p = component(&docs.cdx, "pkg:generic/waybillfixturep@4.5.6").expect("P");
    assert!(!source_files(p).iter().any(|f| f.ends_with("libops.a") || f.ends_with("bin/tool")));
    assert!(coverage(&docs).unwrap().get("exported-targets").is_none());
}

/// US2 / SC-002 (T012): each uncertainty attributes nothing and is counted.
#[test]
fn us2_nothing_is_attributed_without_certainty() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    // Computed version: no package version.
    write(&root, "lib/cmake/waybill-fixture-c/waybill-fixture-c-config-version.cmake", "set(PACKAGE_VERSION ${V})\n");
    write(&root, "lib/cmake/waybill-fixture-c/waybill-fixture-c-targets.cmake", &main_file(3));
    write(&root, "lib/cmake/waybill-fixture-c/waybill-fixture-c-targets-release.cmake", &release_file(&["${_IMPORT_PREFIX}/lib/libwaybill-fixture-c.a"]));
    put_bytes(&root, "lib/libwaybill-fixture-c.a", b"!<arch>\n");
    // `.pc`-only package: the same-name rule attaches it.
    write(&root, "lib/pkgconfig/waybill-fixture-r.pc", "Version: 1.0\n");
    write(&root, "lib/cmake/waybill-fixture-r/waybill-fixture-r-targets.cmake", &main_file(3));
    write(&root, "lib/cmake/waybill-fixture-r/waybill-fixture-r-targets-release.cmake", &release_file(&["${_IMPORT_PREFIX}/lib/libwaybill-fixture-r.a"]));
    put_bytes(&root, "lib/libwaybill-fixture-r.a", b"!<arch>\n");
    // Foreign variables and an escape: unresolved.
    write(&root, "lib/cmake/WaybillFixtureV/WaybillFixtureVConfigVersion.cmake", "set(PACKAGE_VERSION \"1.0\")\n");
    write(&root, "lib/cmake/WaybillFixtureV/WaybillFixtureVTargets.cmake", &main_file(3));
    write(
        &root,
        "lib/cmake/WaybillFixtureV/WaybillFixtureVTargets-release.cmake",
        &release_file(&["${CMAKE_INSTALL_PREFIX}/lib/x.so", "${FOO}/x.so", "${_IMPORT_PREFIX}/../../../../outside.so"]),
    );
    put_bytes(tmp.path(), "outside.so", b"outside\n");
    // A configuration file with no main file: unresolved.
    write(&root, "lib/cmake/WaybillFixtureN/WaybillFixtureNConfigVersion.cmake", "set(PACKAGE_VERSION \"1.0\")\n");
    write(&root, "lib/cmake/WaybillFixtureN/WaybillFixtureNTargets-release.cmake", &release_file(&["${_IMPORT_PREFIX}/lib/libn.a"]));
    put_bytes(&root, "lib/libn.a", b"!<arch>\n");
    // A dpkg-owned targets file: skipped.
    write(&root, "usr/lib/cmake/WaybillFixtureO/WaybillFixtureOConfigVersion.cmake", "set(PACKAGE_VERSION \"1.0\")\n");
    write(&root, "usr/lib/cmake/WaybillFixtureO/WaybillFixtureOTargets.cmake", &main_file(4));
    dpkg_owning(&root, "waybill-fixture-o-dev", &["usr/lib/cmake/WaybillFixtureO/WaybillFixtureOTargets.cmake"]);
    // Two packages naming one file (analysis I1): listed under both.
    for (d, v) in [("WaybillFixtureS1", "1.1"), ("WaybillFixtureS2", "2.2")] {
        write(&root, &format!("lib/cmake/{d}/{d}ConfigVersion.cmake"), &format!("set(PACKAGE_VERSION \"{v}\")\n"));
        write(&root, &format!("lib/cmake/{d}/{d}Targets.cmake"), &main_file(3));
        write(&root, &format!("lib/cmake/{d}/{d}Targets-release.cmake"), &release_file(&["${_IMPORT_PREFIX}/lib/libshared.a"]));
    }
    put_bytes(&root, "lib/libshared.a", b"!<arch>\n");
    let docs = scan(&root);

    let r = component(&docs.cdx, "pkg:generic/waybill-fixture-r@1.0").expect("R");
    assert!(source_files(r).contains(&"lib/libwaybill-fixture-r.a".to_string()), "same-name rule");
    for (purl, file) in [("pkg:generic/waybillfixtures1@1.1", "lib/libshared.a"), ("pkg:generic/waybillfixtures2@2.2", "lib/libshared.a")] {
        assert!(source_files(component(&docs.cdx, purl).expect(purl)).contains(&file.to_string()), "{purl}");
    }
    let v = component(&docs.cdx, "pkg:generic/waybillfixturev@1.0").expect("V");
    assert!(!source_files(v).iter().any(|f| f.contains("outside") || f.ends_with("x.so")));
    let n = component(&docs.cdx, "pkg:generic/waybillfixturen@1.0").expect("N");
    assert!(!source_files(n).iter().any(|f| f.ends_with("libn.a")));
    assert!(!components(&docs.cdx).iter().any(|c| c["purl"].as_str().is_some_and(|p| p.contains("waybill-fixture-c@"))));

    assert_eq!(
        exported(&docs),
        serde_json::json!({
            "attached": 3, "locations": 8, "no-package-version": 1, "not-shipped": 0,
            "owned-by-os-package": 1, "read": 11, "unresolved-location": 4
        })
    );
    let _ = prop;
}

/// SC-004 (T015): ten scans are identical; a tree with only #1531 files has
/// the #1531 C225 shape, byte for byte.
#[test]
fn sc004_byte_identity_and_unchanged_shape_without_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    us1_tree(&root, true);
    let mask = |d: &Docs| {
        let mut c = d.cdx.clone();
        c["serialNumber"] = Value::Null;
        (c, d.spdx23.clone(), d.spdx3.clone())
    };
    let first = mask(&scan(&root));
    for i in 1..10 {
        assert!(mask(&scan(&root)) == first, "run {i} differs");
    }

    let only = tmp.path().join("only1531");
    write(&only, "lib/pkgconfig/waybill-fixture-z.pc", "Version: 1.0\n");
    let docs = scan(&only);
    let raw = docs.cdx["metadata"]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "waybill:installed-library-coverage")
        .and_then(|p| p["value"].as_str())
        .unwrap()
        .to_string();
    assert_eq!(
        raw,
        r#"{"cmake-version":{"emitted":0,"read":0},"pkg-config":{"emitted":1,"read":1},"skipped":{"computed-version":0,"malformed":0,"no-version":0,"owned-by-os-package":0,"unrecognised-location":0,"unresolved-version":0},"unresolved-requires":0,"version-disagreements":0}"#
    );
}
