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

/// The orphan count the completeness reason states (0 when it states none),
/// and the ecosystems it reports under `multi-ecosystem-partial-root`, whose
/// unreachable components that count leaves out.
fn stated_orphans(docs: &Docs) -> (usize, Vec<String>) {
    let Some(reason) = doc_raw(docs, "waybill:graph-completeness-reason") else {
        return (0, Vec::new());
    };
    let codes: Vec<&str> = reason.split(';').map(str::trim).collect();
    let count = codes
        .iter()
        .find_map(|c| c.strip_prefix("orphaned-components-detected: "))
        .map(|rest| rest.split_whitespace().next().unwrap().parse().unwrap())
        .unwrap_or(0);
    let partial = codes
        .iter()
        .find_map(|c| c.strip_prefix("multi-ecosystem-partial-root: "))
        .map(|rest| rest.split(',').map(|e| e.trim().to_string()).collect())
        .unwrap_or_default();
    (count, partial)
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
    assert_one_answer_except(docs, &[]);
}

/// [`assert_one_answer`], with `unexplained` named as unreachable components
/// no rule of this feature covers. Each caller says why.
fn assert_one_answer_except(docs: &Docs, unexplained: &[&str]) {
    let cdx = &docs.cdx;
    let reachable = reachable_with_containment(cdx);
    for (c, _) in cdx_components_with_containers(cdx) {
        if cdx_component_ref(c).is_some_and(|r| reachable.contains(r)) {
            assert_eq!(cdx_prop(c, C45), None, "I-A: reachable {} carries C45", c["bom-ref"]);
        }
    }
    let orphans = unreachable(cdx);
    for c in &orphans {
        if unexplained.iter().any(|u| c["purl"] == *u) {
            assert!(!is_explained(c), "{} is explained now; drop the exemption", c["purl"]);
            continue;
        }
        assert!(is_explained(c), "I-A: unreachable {} carries no marker", c["bom-ref"]);
    }
    let (stated, partial) = stated_orphans(docs);
    let counted = orphans
        .iter()
        .filter(|c| {
            let eco = c["purl"].as_str().and_then(|p| p.strip_prefix("pkg:")).and_then(|p| p.split('/').next());
            !eco.is_some_and(|e| partial.iter().any(|x| x == e))
        })
        .count();
    assert_eq!(
        stated,
        counted,
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

const INCLUDE_TESTS: &[&str] = &["--include-test-binaries"];

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
    // The installed package holding the binary is unreachable too, because
    // the fixture puts it behind an unrelated npm app. No rule of this
    // feature covers an installed package, and TheRock has none.
    assert_one_answer_except(&docs, &["pkg:pypi/waybill-fixture-pkg@1.2.3"]);
}

// ---------------------------------------------------------------------
// User Story 1: binaries found by binary analysis (FR-004)
// ---------------------------------------------------------------------

/// An npm project with an edge at the scan root: its main module roots the
/// graph and closes the root fallback, so a loose binary is not attached to
/// the root and is unreachable.
fn app(root: &Path) {
    npm_project(root, ".", "waybill-fixture-app", &["waybill-fixture-dep-a"], &[]);
}

/// Every component binary analysis produced, by PURL, with its C45.
fn analyzed(docs: &Docs) -> Vec<(String, Option<String>)> {
    let mut v: Vec<(String, Option<String>)> = cdx_components_with_containers(&docs.cdx)
        .into_iter()
        .map(|(c, _)| c)
        .filter(|c| cdx_prop(c, "waybill:sbom-tier") == Some("analyzed"))
        .map(|c| (c["purl"].as_str().unwrap_or_default().to_string(), cdx_prop(c, C45).map(str::to_string)))
        .collect();
    v.sort();
    v
}

/// Scan a tree holding the zlib-carrying library at each of `paths`, behind
/// an npm app, and return the documents. Asserts the binary population is
/// unreachable, so a code test cannot pass on a reachable component.
///
/// A directory scan skips binaries under `test`, `tests`, `unittests` and
/// `Inputs` (#1524); a case placing one there passes
/// `--include-test-binaries`, as TheRock's scan in effect did.
fn loose_binaries(paths: &[&str], extra: &[&str]) -> Docs {
    let dir = tempfile::tempdir().unwrap();
    app(dir.path());
    for p in paths {
        plant(dir.path(), p, &zlib_dylib());
    }
    let docs = scan(dir.path(), extra);
    let found = analyzed(&docs);
    assert!(!found.is_empty(), "control: binary analysis produced components");
    let reachable = reachable_with_containment(&docs.cdx);
    for (c, _) in cdx_components_with_containers(&docs.cdx) {
        if cdx_prop(c, "waybill:sbom-tier") == Some("analyzed") {
            assert!(
                !reachable.contains(cdx_component_ref(c).unwrap()),
                "control: {} is unreachable",
                c["purl"]
            );
        }
    }
    docs
}

fn assert_all_coded(docs: &Docs, code: &str) {
    for (purl, got) in analyzed(docs) {
        assert_eq!(got.as_deref(), Some(code), "{purl}");
    }
    assert_one_answer(docs);
}

/// O-3: under a test-data directory.
#[test]
fn o3_a_binary_under_tests_is_test_data() {
    let docs = loose_binaries(&["tests/data/libwaybill-fixture-z.dylib"], INCLUDE_TESTS);
    assert_all_coded(&docs, "test-data-binary");
}

/// O-4: the other spellings TheRock uses. `Inputs` is one of #1524's; the
/// rest are analyzed in a directory scan without any flag.
#[test]
fn o4_the_other_test_data_spellings_are_test_data() {
    let docs = loose_binaries(&["lldb/Inputs/libwaybill-fixture-z.dylib"], INCLUDE_TESTS);
    assert_all_coded(&docs, "test-data-binary");
    for d in ["test_assets", "testsuite", "fixtures", "testdata", "test-data", "test_data", "test-suite", "test-assets"] {
        let docs = loose_binaries(&[&format!("kpack/{d}/libwaybill-fixture-z.dylib")], &[]);
        assert_all_coded(&docs, "test-data-binary");
    }
}

/// O-5: anywhere else, in a source tree or an install tree.
#[test]
fn o5_a_binary_elsewhere_is_unowned() {
    for p in ["prebuilt/libwaybill-fixture-z.dylib", "usr/lib/libwaybill-fixture-z.dylib"] {
        let docs = loose_binaries(&[p], &[]);
        assert_all_coded(&docs, "unowned-binary");
    }
}

/// O-6: every source file must be under a test-data directory. The same
/// library twice, once under `tests/` and once outside, is one set of
/// components with both paths.
#[test]
fn o6_a_binary_partly_outside_test_data_is_unowned() {
    let docs = loose_binaries(
        &["tests/libwaybill-fixture-z.dylib", "prebuilt/libwaybill-fixture-z.dylib"],
        INCLUDE_TESTS,
    );
    let zlib = cdx_components_with_containers(&docs.cdx)
        .into_iter()
        .map(|(c, _)| c)
        .find(|c| c["purl"] == "pkg:generic/zlib@1.3.1")
        .unwrap();
    assert_eq!(
        common::installed_lib::source_files(zlib).len(),
        2,
        "control: one component, both paths"
    );
    assert_all_coded(&docs, "unowned-binary");
}

/// O-13: a binary a package claims is the package's contents. It gets no
/// binary code, because it is not unowned.
#[test]
fn o13_a_claimed_binary_gets_no_binary_code() {
    let dir = tempfile::tempdir().unwrap();
    installed_package_with_zlib(dir.path());
    let docs = scan(dir.path(), &[]);
    for (purl, got) in analyzed(&docs) {
        assert!(
            !matches!(got.as_deref(), Some("unowned-binary") | Some("test-data-binary")),
            "{purl}: {got:?}"
        );
    }
    assert_one_answer(&docs);
}

// ---------------------------------------------------------------------
// User Story 1: lock files (FR-005)
// ---------------------------------------------------------------------

const UV_REGISTRY: &str = r#"source = { registry = "https://pypi.org/simple" }"#;

fn uv_package(name: &str, version: &str) -> String {
    format!("[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n{UV_REGISTRY}\n\n")
}

/// O-10: the project's own entry is editable with a dynamic version, so the
/// lock gives it none. It is emitted versionless and explains why, and its
/// three groups are wired.
#[test]
fn o10_a_versionless_editable_root_is_emitted_and_wired() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "proj/pyproject.toml",
        "[project]\nname = \"waybill-fixture-proj\"\ndynamic = [\"version\"]\n",
    );
    let lock = format!(
        "version = 1\nrequires-python = \">=3.10\"\n\n\
[[package]]\nname = \"waybill-fixture-proj\"\nsource = {{ editable = \".\" }}\n\
dependencies = [{{ name = \"waybill-fixture-run\" }}]\n\n\
[package.optional-dependencies]\ncu12 = [{{ name = \"waybill-fixture-opt\" }}]\n\n\
[package.dev-dependencies]\ndev = [{{ name = \"waybill-fixture-dev\" }}]\n\n{}{}{}",
        uv_package("waybill-fixture-run", "1.0.0"),
        uv_package("waybill-fixture-opt", "1.0.0"),
        uv_package("waybill-fixture-dev", "1.0.0"),
    );
    write(dir.path(), "proj/uv.lock", &lock);
    let docs = scan(dir.path(), &[]);
    let root: Vec<&Value> = cdx_components_with_containers(&docs.cdx)
        .into_iter()
        .map(|(c, _)| c)
        .chain(std::iter::once(&docs.cdx["metadata"]["component"]))
        .filter(|c| c["purl"] == "pkg:pypi/waybill-fixture-proj")
        .collect();
    assert_eq!(root.len(), 1, "the versionless root, once");
    for p in ["pkg:pypi/waybill-fixture-run@1.0.0", "pkg:pypi/waybill-fixture-opt@1.0.0", "pkg:pypi/waybill-fixture-dev@1.0.0"] {
        assert!(!unreachable(&docs.cdx).iter().any(|c| c["purl"] == p), "{p} is reachable");
    }
    assert_one_answer(&docs);
}

/// O-11: a dependency listed at three versions under markers gets an edge to
/// each.
#[test]
fn o11_every_forked_version_is_wired() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "pyproject.toml",
        "[project]\nname = \"waybill-fixture-proj\"\nversion = \"1.0.0\"\n",
    );
    let deps: String = ["1.0.0", "2.0.0", "3.0.0"]
        .iter()
        .map(|v| format!("{{ name = \"waybill-fixture-fork\", version = \"{v}\", {UV_REGISTRY}, marker = \"python_full_version == '3.1{}'\" }}", &v[..1]))
        .collect::<Vec<_>>()
        .join(", ");
    let lock = format!(
        "version = 1\nrequires-python = \">=3.10\"\n\n\
[[package]]\nname = \"waybill-fixture-proj\"\nversion = \"1.0.0\"\nsource = {{ editable = \".\" }}\n\
dependencies = [{deps}]\n\n{}{}{}",
        uv_package("waybill-fixture-fork", "1.0.0"),
        uv_package("waybill-fixture-fork", "2.0.0"),
        uv_package("waybill-fixture-fork", "3.0.0"),
    );
    write(dir.path(), "uv.lock", &lock);
    let docs = scan(dir.path(), &[]);
    for v in ["1.0.0", "2.0.0", "3.0.0"] {
        let p = format!("pkg:pypi/waybill-fixture-fork@{v}");
        assert!(!unreachable(&docs.cdx).iter().any(|c| c["purl"] == p.as_str()), "{p} is reachable");
    }
    assert_one_answer(&docs);
}

/// O-12: an older version in a `Cargo.lock` that nothing references, beside
/// a reachable newer one, is a dead lock entry (existing code, unchanged).
#[test]
fn o12_an_unreferenced_older_cargo_version_is_a_dead_lock_entry() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname = \"waybill-fixture-app\"\nversion = \"0.1.0\"\n\n[dependencies]\nwaybill-fixture-x = \"2\"\n",
    );
    write(dir.path(), "src/main.rs", "fn main() {}\n");
    let reg = "source = \"registry+https://github.com/rust-lang/crates.io-index\"";
    write(
        dir.path(),
        "Cargo.lock",
        &format!(
            "version = 3\n\n[[package]]\nname = \"waybill-fixture-app\"\nversion = \"0.1.0\"\ndependencies = [\"waybill-fixture-x 2.0.0\"]\n\n\
[[package]]\nname = \"waybill-fixture-x\"\nversion = \"1.0.0\"\n{reg}\n\n\
[[package]]\nname = \"waybill-fixture-x\"\nversion = \"2.0.0\"\n{reg}\n"
        ),
    );
    let docs = scan(dir.path(), &[]);
    assert_eq!(marker(&docs, "pkg:cargo/waybill-fixture-x@1.0.0"), Some("dead-lockfile-entry"));
    assert_eq!(marker(&docs, "pkg:cargo/waybill-fixture-x@2.0.0"), None);
    assert_one_answer(&docs);
}

// ---------------------------------------------------------------------
// User Story 1: #1524's skip in a source tree with a vendored SDK
// ---------------------------------------------------------------------

/// O-14: TheRock's shape. One committed CMake package-version file in an
/// install-looking location used to mark the whole source tree installed,
/// which turned #1524's test-input skip off everywhere. The skip now spares
/// only binaries under that record's prefix.
#[test]
fn o14_a_vendored_sdk_does_not_unskip_the_trees_test_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let sdk = "shared/waybill-fixture-interop/waybill-fixture-kit/lnx";
    write(
        dir.path(),
        &format!("{sdk}/lib/cmake/waybill-fixture-kit/waybill-fixture-kitConfigVersion.cmake"),
        "set(PACKAGE_VERSION \"1.0.0\")\n",
    );
    plant(dir.path(), "llvm/test/Object/Inputs/libwaybill-fixture-z.dylib", &zlib_dylib());
    plant(dir.path(), &format!("{sdk}/lib/waybill-fixture-kit/tests/libwaybill-fixture-kept.dylib"), &zlib_dylib());
    let docs = scan(dir.path(), &[]);
    assert!(
        common::installed_lib::purls_with_prefix(&docs.cdx, "pkg:generic/waybill-fixture-kit@").len() == 1,
        "control: the version file is read as an installed record"
    );
    let files: Vec<String> = cdx_components_with_containers(&docs.cdx)
        .into_iter()
        .map(|(c, _)| c)
        .filter(|c| cdx_prop(c, "waybill:sbom-tier") == Some("analyzed"))
        .flat_map(common::installed_lib::source_files)
        .collect();
    assert!(
        files.iter().all(|f| !f.starts_with("llvm/")),
        "the LLVM test input is skipped: {files:?}"
    );
    assert!(
        files.iter().any(|f| f.contains("waybill-fixture-kit/tests/")),
        "the installed package's own test is kept: {files:?}"
    );
    assert_eq!(doc_raw(&docs, "waybill:binary-scan-skipped-test-inputs").as_deref(), Some("1"));
}
