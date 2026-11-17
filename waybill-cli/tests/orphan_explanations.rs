//! Spec 1541 (#1593): every unreachable component explains itself, and the
//! classifier, the document and invariant I7 agree on which are unreachable.
//!
//! Contract `specs/1541-explain-therock-orphans/contracts/wire.md`. Every
//! fixture is synthetic and nothing is fetched. I-A is asserted with the
//! corpus harness's own I7 predicates, never a re-derived walk.
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::path::Path;

use common::installed_lib::{macho_dylib, scan_with, write, Docs};
use corpus_harness_195::invariants::checks::orphan_explained::{
    cdx_components_with_containers, is_explained, reachable_with_containment,
};
use corpus_harness_195::invariants::checks::{cdx_component_ref, cdx_prop, cdx_subject_ref};
use serde_json::{json, Value};

const C45: &str = "waybill:orphan-reason";

// ---------------------------------------------------------------------
// Invariants
// ---------------------------------------------------------------------

/// A document-scope value, unparsed, from each format.
fn doc_raw(docs: &Docs, field: &str) -> Option<String> {
    let cdx = docs.cdx["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == field)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string);
    let spdx = |doc: &Value| -> Option<String> {
        doc["annotations"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(doc["@graph"].as_array().into_iter().flatten().filter(|e| e["type"] == "Annotation"))
            .find_map(|a| {
                let env = a["comment"].as_str().or_else(|| a["statement"].as_str())?;
                let env: Value = serde_json::from_str(env).ok()?;
                (env["field"] == field).then(|| env["value"].as_str().map(str::to_string))?
            })
    };
    assert_eq!(cdx, spdx(&docs.spdx23), "{field}: CycloneDX and SPDX 2.3 disagree");
    assert_eq!(cdx, spdx(&docs.spdx3), "{field}: CycloneDX and SPDX 3 disagree");
    cdx
}

/// The orphan count the completeness reason states, 0 when it states none.
fn stated_orphans(docs: &Docs) -> usize {
    let Some(reason) = doc_raw(docs, "waybill:graph-completeness-reason") else { return 0 };
    assert!(
        !reason.contains("multi-ecosystem-partial-root"),
        "fixture design: the reason splits its count: {reason}"
    );
    reason
        .split(';')
        .find_map(|code| code.trim().strip_prefix("orphaned-components-detected: "))
        .map(|rest| rest.split_whitespace().next().unwrap().parse().unwrap())
        .unwrap_or(0)
}

/// The I7 population: components, nested ones included, other than the
/// subject and file-tier inventory, that nothing reaches.
fn unreachable(cdx: &Value) -> Vec<&Value> {
    let reachable = reachable_with_containment(cdx);
    let subject = cdx_subject_ref(cdx);
    let mut out: Vec<&Value> = cdx_components_with_containers(cdx)
        .into_iter()
        .map(|(c, _)| c)
        .filter(|c| cdx_component_ref(c).is_some_and(|r| Some(r) != subject && !reachable.contains(r)))
        .filter(|c| cdx_prop(c, "waybill:component-tier") != Some("file"))
        .collect();
    out.sort_by_key(|c| cdx_component_ref(c).unwrap_or_default().to_string());
    out.dedup_by_key(|c| cdx_component_ref(c).unwrap_or_default().to_string());
    out
}

/// I-A and I-B (contract): one unreachable set, in every format.
fn assert_one_answer(docs: &Docs) {
    let cdx = &docs.cdx;
    let reachable = reachable_with_containment(cdx);
    for (c, _) in cdx_components_with_containers(cdx) {
        if cdx_component_ref(c).is_some_and(|r| reachable.contains(r)) {
            assert_eq!(cdx_prop(c, C45), None, "I-A: reachable {} carries C45", c["bom-ref"]);
        }
    }
    let orphans = unreachable(cdx);
    for c in &orphans {
        assert!(is_explained(c), "I-A: unreachable {} carries no marker", c["bom-ref"]);
    }
    assert_eq!(
        stated_orphans(docs),
        orphans.len(),
        "I-A: completeness count vs the I7 population {:?}",
        orphans.iter().map(|c| &c["bom-ref"]).collect::<Vec<_>>()
    );
    common::installed_lib::row_all_formats(docs, "C45");
    doc_raw(docs, "waybill:graph-completeness");
}

fn marker<'a>(docs: &'a Docs, purl: &str) -> Option<&'a str> {
    let found: Vec<&Value> = cdx_components_with_containers(&docs.cdx)
        .into_iter()
        .map(|(c, _)| c)
        .filter(|c| c["purl"] == purl)
        .collect();
    assert_eq!(found.len(), 1, "exactly one {purl}");
    cdx_prop(found[0], C45)
}

// ---------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------

/// An npm project at `root/dir` named `name`, depending on `deps`. Its lock
/// also lists `strays`, which nothing depends on.
fn npm_project(root: &Path, dir: &str, name: &str, deps: &[&str], strays: &[&str]) {
    let wanted: serde_json::Map<String, Value> =
        deps.iter().map(|d| (d.to_string(), json!("^1.0.0"))).collect();
    write(
        root,
        &format!("{dir}/package.json"),
        &json!({ "name": name, "version": "1.0.0", "dependencies": wanted }).to_string(),
    );
    let mut packages = serde_json::Map::new();
    packages.insert(
        String::new(),
        json!({ "name": name, "version": "1.0.0", "dependencies": wanted }),
    );
    for p in deps.iter().chain(strays) {
        packages.insert(
            format!("node_modules/{p}"),
            json!({
                "version": "1.0.0",
                "resolved": format!("https://registry.npmjs.org/{p}/-/{p}-1.0.0.tgz"),
                "integrity": "sha512-fake"
            }),
        );
    }
    write(
        root,
        &format!("{dir}/package-lock.json"),
        &json!({ "name": name, "version": "1.0.0", "lockfileVersion": 3, "requires": true, "packages": packages })
            .to_string(),
    );
}

/// TheRock's shape (source-trace, "the classifier"): several projects, so no
/// single main module roots the scan, and a lock entry nothing depends on.
fn two_projects(root: &Path) {
    npm_project(root, "a", "waybill-fixture-a", &["waybill-fixture-dep-a"], &["waybill-fixture-stray"]);
    npm_project(root, "b", "waybill-fixture-b", &["waybill-fixture-dep-b"], &[]);
}

const STRAY: &str = "pkg:npm/waybill-fixture-stray@1.0.0";

fn scan(root: &Path, extra: &[&str]) -> Docs {
    scan_with("--path", root, extra)
}

const ROOT_FLAGS: &[&str] = &["--root-name", "waybill-fixture-root", "--root-version", "1.0.0"];

const ZLIB: &[u8] = b"\0 deflate 1.3.1 Copyright 1995-2024 Jean-loup Gailly and Mark Adler \0";

fn zlib_dylib() -> Vec<u8> {
    macho_dylib(&["/usr/lib/libwaybill-fixture-ext.dylib"], ZLIB)
}

fn plant(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// An installed Python package whose `RECORD` claims a library carrying an
/// embedded zlib: the package nests the library, and the library nests zlib
/// (spec 1536).
fn installed_package_with_zlib(root: &Path) {
    let site = root.join("usr/lib/python3.12/site-packages");
    let di = site.join("waybill_fixture_pkg-1.2.3.dist-info");
    std::fs::create_dir_all(&di).unwrap();
    std::fs::write(di.join("METADATA"), "Metadata-Version: 2.1\nName: waybill-fixture-pkg\nVersion: 1.2.3\n").unwrap();
    let lib = "waybill_fixture_pkg/.dylibs/libwaybill-fixture-z.dylib";
    std::fs::write(
        di.join("RECORD"),
        format!("waybill_fixture_pkg-1.2.3.dist-info/METADATA,,\n{lib},,\n"),
    )
    .unwrap();
    plant(&site, lib, &zlib_dylib());
}

/// `pkg:generic/zlib@1.3.1` and the bom-ref of the component it is nested in.
fn zlib_and_container(cdx: &Value) -> (&Value, Option<&str>) {
    let found: Vec<(&Value, Option<&str>)> = cdx_components_with_containers(cdx)
        .into_iter()
        .filter(|(c, _)| c["purl"] == "pkg:generic/zlib@1.3.1")
        .collect();
    assert_eq!(found.len(), 1, "one zlib: {found:?}");
    found[0]
}

// ---------------------------------------------------------------------
// User Story 2: one reachability answer
// ---------------------------------------------------------------------

/// O-1: with an operator root, the dropped main modules' edges are anchored
/// to it, so nothing else attaches to the root and the stray lock entry is
/// unreachable. The classifier must see that, and explain it.
#[test]
fn o1_an_operator_root_classifies_against_the_documents_graph() {
    let dir = tempfile::tempdir().unwrap();
    two_projects(dir.path());
    let docs = scan(dir.path(), ROOT_FLAGS);
    assert!(
        unreachable(&docs.cdx).iter().any(|c| c["purl"] == STRAY),
        "control: the stray is unreachable in the document"
    );
    assert!(marker(&docs, STRAY).is_some(), "the stray carries C45");
    assert_one_answer(&docs);
}

/// O-2: the same tree without an operator root; the markers agree with the
/// document's reachable set, whatever that set is.
#[test]
fn o2_without_an_operator_root_markers_agree_with_the_document() {
    let dir = tempfile::tempdir().unwrap();
    two_projects(dir.path());
    let docs = scan(dir.path(), &[]);
    assert_one_answer(&docs);
}

/// O-8: a library nested in a binary nested in an installed package is
/// reached through its containers; it carries no orphan marker and the
/// completeness count does not include it.
#[test]
fn o8_a_library_nested_in_a_reachable_binary_is_reachable() {
    let dir = tempfile::tempdir().unwrap();
    installed_package_with_zlib(dir.path());
    let docs = scan(dir.path(), &[]);
    let (zlib, container) = zlib_and_container(&docs.cdx);
    assert!(container.is_some(), "control: zlib is nested");
    assert!(
        reachable_with_containment(&docs.cdx).contains(cdx_component_ref(zlib).unwrap()),
        "zlib is reached through its containers"
    );
    assert_eq!(cdx_prop(zlib, C45), None);
    assert_one_answer(&docs);
}

/// O-9: a library nested in an unreachable binary is unreachable, and is
/// explained by its own population's code. An npm project with edges closes
/// the root fallback, so the installed package holding the binary is not
/// attached to the root.
#[test]
fn o9_a_library_nested_in_an_unreachable_binary_is_explained() {
    let dir = tempfile::tempdir().unwrap();
    npm_project(dir.path(), ".", "waybill-fixture-app", &["waybill-fixture-dep-a"], &[]);
    installed_package_with_zlib(dir.path());
    let docs = scan(dir.path(), &[]);
    let (zlib, container) = zlib_and_container(&docs.cdx);
    let container = container.expect("control: zlib is nested");
    let reachable = reachable_with_containment(&docs.cdx);
    assert!(!reachable.contains(container), "control: the binary is unreachable");
    assert!(!reachable.contains(cdx_component_ref(zlib).unwrap()));
    assert!(cdx_prop(zlib, C45).is_some(), "zlib carries C45");
    assert_one_answer(&docs);
}
