//! Spec 1536 (#1567): a native binary that only a Python package's `RECORD`
//! claims still goes through binary analysis. It is reported once, nested
//! under the package that owns it, and what it identifies nests under it.
//!
//! The fixtures are synthetic: a hand-built Mach-O dylib carrying an embedded
//! zlib version string and `LC_LOAD_DYLIB` entries, inside a
//! `waybill-fixture-*` package laid out as an unpacked wheel or under
//! `usr/lib/python3.12/site-packages/`. Nothing is fetched.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;

use std::io::Write as _;

use common::installed_lib::{components, elf_shared_lib, macho_dylib, purls_with_prefix, scan};

const ZLIB: &[u8] = b"\0 deflate 1.3.1 Copyright 1995-2024 Jean-loup Gailly and Mark Adler \0";
const EXT_LIB: &str = "/usr/lib/libwaybill-fixture-ext.dylib";

fn plant(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// T002 control: with no `.dist-info`, the fixture library yields both
/// signals the owned-binary tests rely on. Without this, a nested-zlib
/// assertion that fails could mean the fixture never fires.
#[test]
fn control_unclaimed_fixture_library_is_identified_and_linked() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "waybill_fixture_pkg/.dylibs/libwaybill-fixture-z.dylib", &macho_dylib(&[EXT_LIB], ZLIB));
    let docs = scan(dir.path());
    assert_eq!(purls_with_prefix(&docs.cdx, "pkg:generic/zlib@"), vec!["pkg:generic/zlib@1.3.1"]);
    let linkage: Vec<String> = purls_with_prefix(&docs.cdx, "pkg:generic/")
        .into_iter()
        .filter(|p| p.contains("libwaybill-fixture-ext"))
        .collect();
    assert_eq!(linkage.len(), 1, "one linkage component for the load: {linkage:?}");
    let binaries = components(&docs.cdx)
        .into_iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.contains("file-sha256=")))
        .count();
    assert_eq!(binaries, 1, "one file-level component for the library");
}

// ---------------------------------------------------------------------
// US1: a vendored library identified under its package
// ---------------------------------------------------------------------

const PKG: &str = "pkg:pypi/waybill-fixture-pkg@1.2.3";
const C230: &str = "waybill:python-owned-binaries";

/// Where a layout puts `site-packages` under the scan root.
#[derive(Clone, Copy, Debug)]
enum Layout {
    /// An unpacked wheel: its `.dist-info` at the scan root (#1529).
    Wheel,
    /// An installed tree: `usr/lib/python3.12/site-packages/`.
    Site,
}

impl Layout {
    fn site(self, root: &Path) -> std::path::PathBuf {
        match self {
            Layout::Wheel => root.to_path_buf(),
            Layout::Site => root.join("usr/lib/python3.12/site-packages"),
        }
    }
}

/// `waybill-fixture-pkg` 1.2.3 in `layout`, holding `files` (path relative
/// to site-packages, bytes). Each file is listed in `RECORD` unless
/// `unlisted` names it.
fn package(root: &Path, layout: Layout, files: &[(&str, Vec<u8>)], unlisted: &[&str]) -> std::path::PathBuf {
    let site = layout.site(root);
    let di = site.join("waybill_fixture_pkg-1.2.3.dist-info");
    std::fs::create_dir_all(&di).unwrap();
    std::fs::write(di.join("METADATA"), "Metadata-Version: 2.1\nName: waybill-fixture-pkg\nVersion: 1.2.3\n").unwrap();
    let mut record = String::from("waybill_fixture_pkg-1.2.3.dist-info/METADATA,,\n");
    for (rel, bytes) in files {
        plant(&site, rel, bytes);
        if !unlisted.contains(rel) {
            record.push_str(&format!("{rel},,\n"));
        }
    }
    std::fs::write(di.join("RECORD"), record).unwrap();
    site
}

const LIB: &str = "waybill_fixture_pkg/.dylibs/libwaybill-fixture-z.dylib";

fn zlib_library() -> Vec<u8> {
    macho_dylib(&[EXT_LIB], ZLIB)
}

/// The component whose PURL starts with `prefix`, and the PURL of the
/// component it is nested in (`None` at the top level).
fn find_with_parent<'a>(cdx: &'a serde_json::Value, prefix: &str) -> Vec<(&'a serde_json::Value, Option<String>)> {
    fn walk<'a>(v: &'a serde_json::Value, parent: Option<String>, prefix: &str, out: &mut Vec<(&'a serde_json::Value, Option<String>)>) {
        for c in v["components"].as_array().into_iter().flatten() {
            if c["purl"].as_str().is_some_and(|p| p.starts_with(prefix)) {
                out.push((c, parent.clone()));
            }
            walk(c, c["purl"].as_str().map(str::to_string), prefix, out);
        }
    }
    let mut out = Vec::new();
    walk(cdx, None, prefix, &mut out);
    out
}

fn bom_ref_of<'a>(cdx: &'a serde_json::Value, prefix: &str) -> &'a str {
    let found = find_with_parent(cdx, prefix);
    assert_eq!(found.len(), 1, "exactly one {prefix}: {found:?}");
    found[0].0["bom-ref"].as_str().unwrap()
}

fn depends_on_refs(cdx: &serde_json::Value, bom_ref: &str) -> Vec<String> {
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["ref"] == bom_ref)
        .and_then(|d| d["dependsOn"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// The package's own component: the document root in an unpacked wheel, a
/// component in an installed tree.
fn package_ref(cdx: &serde_json::Value) -> String {
    if cdx["metadata"]["component"]["purl"] == PKG {
        return cdx["metadata"]["component"]["bom-ref"].as_str().unwrap().to_string();
    }
    bom_ref_of(cdx, PKG).to_string()
}

fn c230(docs: &common::installed_lib::Docs) -> Option<serde_json::Value> {
    common::installed_lib::doc_field_all_formats(docs, C230)
}

/// US1 scenarios 1 and 2, and contract C-1, C-2 and C-4.
#[test]
fn us1_owned_library_nests_under_its_package_with_its_identification() {
    for layout in [Layout::Wheel, Layout::Site] {
        let dir = tempfile::tempdir().unwrap();
        package(dir.path(), layout, &[(LIB, zlib_library())], &[]);
        let docs = scan(dir.path());
        let cdx = &docs.cdx;

        let bins = find_with_parent(cdx, "pkg:generic/libwaybill-fixture-z.dylib");
        assert_eq!(bins.len(), 1, "{layout:?}: the library appears once (C-1): {bins:?}");
        match layout {
            // The package is the document root; the root's contents are
            // `components[]`.
            Layout::Wheel => {
                assert_eq!(cdx["metadata"]["component"]["purl"], PKG, "root unchanged");
                assert_eq!(bins[0].1, None);
            }
            Layout::Site => assert_eq!(bins[0].1.as_deref(), Some(PKG), "nested under the package"),
        }
        let zlib = find_with_parent(cdx, "pkg:generic/zlib@");
        assert_eq!(zlib.len(), 1, "{layout:?}: {zlib:?}");
        assert_eq!(zlib[0].0["purl"], "pkg:generic/zlib@1.3.1");
        assert!(
            zlib[0].1.as_deref().is_some_and(|p| p.starts_with("pkg:generic/libwaybill-fixture-z.dylib")),
            "{layout:?}: zlib nested under the library"
        );
        let link = find_with_parent(cdx, "pkg:generic/%2Fusr%2Flib%2Flibwaybill-fixture-ext.dylib");
        assert_eq!(link.len(), 1, "{layout:?}: one linkage component");
        assert_eq!(link[0].1, None, "{layout:?}: linkage stays top-level (C-4)");
        // C-2: no file-tier component for the library's path.
        let file_tier = components(cdx)
            .into_iter()
            .filter(|c| c["type"] == "file" && c["name"].as_str().is_some_and(|n| n.contains("libwaybill-fixture-z")))
            .count();
        assert_eq!(file_tier, 0, "{layout:?}: no file-tier duplicate");
        assert_eq!(c230(&docs), Some(serde_json::json!({"analysed": 1, "identified": 1})), "{layout:?}");
    }
}

/// US1 scenario 3 and FR-004: package → library → {zlib, linked library}.
#[test]
fn us1_edges_form_a_chain_from_the_package() {
    for layout in [Layout::Wheel, Layout::Site] {
        let dir = tempfile::tempdir().unwrap();
        package(dir.path(), layout, &[(LIB, zlib_library())], &[]);
        let docs = scan(dir.path());
        let cdx = &docs.cdx;
        let lib = bom_ref_of(cdx, "pkg:generic/libwaybill-fixture-z.dylib");
        assert!(depends_on_refs(cdx, &package_ref(cdx)).iter().any(|d| d == lib), "{layout:?}: package → library");
        let mut from_lib = depends_on_refs(cdx, lib);
        from_lib.sort();
        let zlib = bom_ref_of(cdx, "pkg:generic/zlib@").to_string();
        let ext = bom_ref_of(cdx, "pkg:generic/%2Fusr%2Flib%2Flibwaybill-fixture-ext.dylib").to_string();
        let mut want = vec![zlib, ext];
        want.sort();
        assert_eq!(from_lib, want, "{layout:?}: library → its identification and its linkage");
        // Contract C-3: every dependency ref is a bom-ref in the document.
        let refs: std::collections::BTreeSet<String> =
            components(cdx).into_iter().filter_map(|c| c["bom-ref"].as_str().map(str::to_string)).collect();
        let root = cdx["metadata"]["component"]["bom-ref"].as_str().unwrap();
        for d in cdx["dependencies"].as_array().unwrap() {
            let r = d["ref"].as_str().unwrap();
            assert!(r == root || refs.contains(r), "{layout:?}: ref {r}");
            for t in d["dependsOn"].as_array().unwrap() {
                assert!(refs.contains(t.as_str().unwrap()), "{layout:?}: dependsOn {t}");
            }
        }
    }
}

/// US1 scenario 4: a library that links its sibling gets an edge to it, and
/// the sibling is not an external dependency.
#[test]
fn us1_internal_link_is_an_edge_not_a_linkage_component() {
    let dir = tempfile::tempdir().unwrap();
    let a = "waybill_fixture_pkg/_core.so";
    let b = "waybill_fixture_pkg/.dylibs/libwaybill-fixture-b.dylib";
    package(
        dir.path(),
        Layout::Site,
        &[(a, macho_dylib(&["@loader_path/.dylibs/libwaybill-fixture-b.dylib"], b"\0a\0")), (b, macho_dylib(&[], b"\0b\0"))],
        &[],
    );
    let docs = scan(dir.path());
    let cdx = &docs.cdx;
    let b_ref = bom_ref_of(cdx, "pkg:generic/libwaybill-fixture-b.dylib");
    assert!(depends_on_refs(cdx, bom_ref_of(cdx, "pkg:generic/_core.so")).iter().any(|d| d == b_ref));
    let externals: Vec<String> = purls_with_prefix(cdx, "pkg:generic/%40loader_path");
    assert!(externals.is_empty(), "no linkage component for the sibling: {externals:?}");
}

/// US1 scenario 5: a `RECORD` listing only Python files changes nothing.
#[test]
fn us1_python_only_record_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    package(dir.path(), Layout::Site, &[("waybill_fixture_pkg/__init__.py", b"x = 1\n".to_vec())], &[]);
    let docs = scan(dir.path());
    assert_eq!(c230(&docs), None);
    assert!(purls_with_prefix(&docs.cdx, "pkg:generic/").iter().all(|p| !p.contains("file-sha256=")));
}

/// US1 scenario 6 and clarification 4: an owned library nothing is
/// identified in still appears once, nested; linkage is not identification.
#[test]
fn us1_unidentified_owned_library_still_appears_once() {
    let dir = tempfile::tempdir().unwrap();
    package(dir.path(), Layout::Site, &[(LIB, macho_dylib(&[EXT_LIB], b"\0nothing here\0"))], &[]);
    let docs = scan(dir.path());
    let bins = find_with_parent(&docs.cdx, "pkg:generic/libwaybill-fixture-z.dylib");
    assert_eq!(bins.len(), 1);
    assert_eq!(bins[0].1.as_deref(), Some(PKG));
    assert_eq!(c230(&docs), Some(serde_json::json!({"analysed": 1, "identified": 0})));
}

/// FR-006: both layouts give the same components and edges below the
/// package, the root aside.
#[test]
fn us1_both_layouts_give_the_same_tree() {
    type Shape = (Vec<(String, Option<String>)>, Vec<(String, String)>);
    fn shape(cdx: &serde_json::Value) -> Shape {
        let strip = |r: &str| r.split('#').next().unwrap().to_string();
        let mut nodes: Vec<(String, Option<String>)> = find_with_parent(cdx, "pkg:generic/")
            .into_iter()
            .map(|(c, p)| (c["purl"].as_str().unwrap().to_string(), p.filter(|p| p != PKG)))
            .collect();
        nodes.sort();
        let mut edges: Vec<(String, String)> = cdx["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["ref"].as_str().is_some_and(|r| r.starts_with("pkg:generic/")))
            .flat_map(|d| {
                let from = strip(d["ref"].as_str().unwrap());
                d["dependsOn"].as_array().unwrap().iter().map(move |t| (from.clone(), strip(t.as_str().unwrap())))
            })
            .collect();
        edges.sort();
        (nodes, edges)
    }
    let shapes: Vec<_> = [Layout::Wheel, Layout::Site]
        .into_iter()
        .map(|layout| {
            let dir = tempfile::tempdir().unwrap();
            package(dir.path(), layout, &[(LIB, zlib_library())], &[]);
            shape(&scan(dir.path()).cdx)
        })
        .collect();
    assert!(!shapes[0].0.is_empty() && !shapes[0].1.is_empty(), "control: a tree to compare");
    assert_eq!(shapes[0], shapes[1]);
}

/// SC-006: the documents are conformant, and identifiers are unique.
#[test]
fn us1_documents_are_conformant() {
    let dir = tempfile::tempdir().unwrap();
    package(dir.path(), Layout::Site, &[(LIB, zlib_library())], &[]);
    let docs = scan(dir.path());
    assert_conformant(&docs, "us1");
}

fn assert_conformant(docs: &common::installed_lib::Docs, label: &str) {
    let errors = common::cdx_schema::cdx_validation_errors(&docs.cdx);
    assert!(errors.is_empty(), "{label}: CycloneDX schema: {errors:?}");
    let mut ids: Vec<&str> = docs.spdx23["packages"].as_array().unwrap().iter().filter_map(|p| p["SPDXID"].as_str()).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(n, ids.len(), "{label}: SPDX 2.3 SPDXIDs unique");
    let mut iris: Vec<&str> = docs.spdx3["@graph"].as_array().unwrap().iter().filter_map(|e| e["spdxId"].as_str()).collect();
    let n = iris.len();
    iris.sort_unstable();
    iris.dedup();
    assert_eq!(n, iris.len(), "{label}: SPDX 3 spdxIds unique");
    let validator = common::workspace_root().join(".venv/spdx3-validate/bin/spdx3-validate");
    if !validator.exists() {
        assert!(
            std::env::var("WAYBILL_REQUIRE_SPDX3_VALIDATOR").ok().as_deref() != Some("1"),
            "spdx3-validate not found at {} and WAYBILL_REQUIRE_SPDX3_VALIDATOR=1",
            validator.display()
        );
        eprintln!("[python_owned_binaries] spdx3-validate not installed; skipping (CI requires it)");
        return;
    }
    let out = tempfile::tempdir().unwrap();
    let doc = out.path().join("d.spdx3.json");
    std::fs::write(&doc, serde_json::to_vec(&docs.spdx3).unwrap()).unwrap();
    let o = common::spdx3_validate::command(&validator).arg("--quiet").arg("-j").arg(&doc).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success() && !text.contains("Violation of type"), "{label}: spdx3-validate:\n{text}");
}

// ---------------------------------------------------------------------
// T011: edge cases
// ---------------------------------------------------------------------

/// A second package, `waybill-fixture-other` 2.0.0, in `site`.
fn other_package(site: &Path, files: &[(&str, Vec<u8>)]) {
    let di = site.join("waybill_fixture_other-2.0.0.dist-info");
    std::fs::create_dir_all(&di).unwrap();
    std::fs::write(di.join("METADATA"), "Metadata-Version: 2.1\nName: waybill-fixture-other\nVersion: 2.0.0\n").unwrap();
    let mut record = String::new();
    for (rel, bytes) in files {
        plant(site, rel, bytes);
        record.push_str(&format!("{rel},,\n"));
    }
    std::fs::write(di.join("RECORD"), record).unwrap();
}

/// V4: two packages vendor a byte-identical library. Each has its own
/// nested copy, with its identification, and every identifier is distinct
/// in all three formats.
#[test]
fn t011_one_library_vendored_by_two_packages() {
    let dir = tempfile::tempdir().unwrap();
    let site = package(dir.path(), Layout::Site, &[(LIB, zlib_library())], &[]);
    other_package(&site, &[("waybill_fixture_other/.dylibs/libwaybill-fixture-z.dylib", zlib_library())]);
    let docs = scan(dir.path());
    let bins = find_with_parent(&docs.cdx, "pkg:generic/libwaybill-fixture-z.dylib");
    let mut parents: Vec<Option<String>> = bins.iter().map(|(_, p)| p.clone()).collect();
    parents.sort();
    assert_eq!(
        parents,
        vec![Some("pkg:pypi/waybill-fixture-other@2.0.0".to_string()), Some(PKG.to_string())],
        "one copy under each package"
    );
    let zlib = find_with_parent(&docs.cdx, "pkg:generic/zlib@");
    assert_eq!(zlib.len(), 2, "the identification under each copy: {zlib:?}");
    assert_eq!(c230(&docs), Some(serde_json::json!({"analysed": 2, "identified": 2})));
    // SPDX is a graph: one element for the library, contained by both owners.
    let pkgs = docs.spdx23["packages"].as_array().unwrap();
    let lib_ids: Vec<&str> = pkgs
        .iter()
        .filter(|p| p["name"].as_str().is_some_and(|n| n.contains("libwaybill-fixture-z")))
        .filter_map(|p| p["SPDXID"].as_str())
        .collect();
    assert_eq!(lib_ids.len(), 1, "one SPDX 2.3 package for the library");
    let owners: std::collections::BTreeSet<&str> = docs.spdx23["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["relationshipType"] == "CONTAINS" && r["relatedSpdxElement"] == lib_ids[0])
        .filter_map(|r| r["spdxElementId"].as_str())
        .collect();
    assert_eq!(owners.len(), 2, "both packages CONTAINS the library: {owners:?}");
    let contains3 = docs.spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["relationshipType"] == "contains")
        .filter(|e| e["to"].as_array().is_some_and(|t| t.iter().any(|x| x.as_str().is_some_and(|x| x.ends_with(&lib_iri_suffix(&docs))))))
        .count();
    assert_eq!(contains3, 2, "both packages contain the library in SPDX 3");
    assert_conformant(&docs, "t011 two packages");
}

/// The IRI of the SPDX 3 element for the vendored library.
fn lib_iri_suffix(docs: &common::installed_lib::Docs) -> String {
    docs.spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"].as_str().is_some_and(|n| n.contains("libwaybill-fixture-z")))
        .and_then(|e| e["spdxId"].as_str())
        .unwrap()
        .to_string()
}

/// A minimal ELF64 shared library with one allocated section `name` of type
/// `sh_type` holding `data`. The shape of `elf_shared_lib`, with the section
/// named.
fn elf_with_section(name: &[u8], sh_type: u32, data: &[u8]) -> Vec<u8> {
    let mut shstrtab = vec![0u8];
    let name_off = shstrtab.len() as u32;
    shstrtab.extend_from_slice(name);
    shstrtab.push(0);
    let shstr_off = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".shstrtab\0");
    let data_off = 64u64;
    let pad = 2048usize.saturating_sub(data.len());
    let strtab_off = data_off + data.len() as u64 + pad as u64;
    let shoff = strtab_off + shstrtab.len() as u64;
    let mut f = Vec::new();
    f.extend_from_slice(b"\x7fELF");
    f.extend_from_slice(&[2, 1, 1, 0]);
    f.extend_from_slice(&[0u8; 8]);
    f.extend_from_slice(&3u16.to_le_bytes()); // ET_DYN
    f.extend_from_slice(&62u16.to_le_bytes());
    f.extend_from_slice(&1u32.to_le_bytes());
    f.extend_from_slice(&0u64.to_le_bytes());
    f.extend_from_slice(&0u64.to_le_bytes());
    f.extend_from_slice(&shoff.to_le_bytes());
    f.extend_from_slice(&0u32.to_le_bytes());
    for v in [64u16, 0, 0, 64, 3, 2] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    f.extend_from_slice(data);
    f.extend(std::iter::repeat_n(0u8, pad));
    f.extend_from_slice(&shstrtab);
    let mut sh = |name: u32, ty: u32, flags: u64, off: u64, size: u64| {
        f.extend_from_slice(&name.to_le_bytes());
        f.extend_from_slice(&ty.to_le_bytes());
        f.extend_from_slice(&flags.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
        f.extend_from_slice(&off.to_le_bytes());
        f.extend_from_slice(&size.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&4u64.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
    };
    sh(0, 0, 0, 0, 0);
    sh(name_off, sh_type, 2, data_off, data.len() as u64);
    sh(shstr_off, 3, 0, strtab_off, shstrtab.len() as u64);
    f
}

/// An ELF whose `.note.package` (systemd's package-note format) names an
/// rpm package.
fn elf_with_package_note(name: &str, version: &str) -> Vec<u8> {
    let desc = format!(r#"{{"type":"rpm","name":"{name}","version":"{version}","distro":"fedora"}}"#);
    let mut note = Vec::new();
    note.extend_from_slice(&4u32.to_le_bytes());
    note.extend_from_slice(&(desc.len() as u32).to_le_bytes());
    note.extend_from_slice(&0xcafe_1a7e_u32.to_le_bytes());
    note.extend_from_slice(b"FDO\0");
    note.extend_from_slice(desc.as_bytes());
    while note.len() % 4 != 0 {
        note.push(0);
    }
    elf_with_section(b".note.package", 7, &note)
}

/// A minimal ELF64 with a `.dep-v0` section, as cargo-auditable writes.
/// `crates` is `(name, version)`; the first is the root. Adapted from
/// `cargo_binary` in `closure_binary_contents.rs`.
fn cargo_auditable_elf(crates: &[(&str, &str)]) -> Vec<u8> {
    let packages: Vec<serde_json::Value> = crates
        .iter()
        .enumerate()
        .map(|(i, (name, version))| {
            let mut p = serde_json::json!({ "name": name, "version": version, "source": "crates.io", "kind": "runtime" });
            if i == 0 {
                p["root"] = serde_json::Value::Bool(true);
                p["dependencies"] = serde_json::json!((1..crates.len()).collect::<Vec<_>>());
            }
            p
        })
        .collect();
    let json = serde_json::json!({ "packages": packages }).to_string();
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(json.as_bytes()).unwrap();
    elf_with_section(b".dep-v0", 1, &enc.finish().unwrap())
}

/// FR-001's other signals: an ELF package note and a bundled source tree
/// (spec 1534), each nested under the owned binary that carries it.
#[test]
fn t008_note_and_bundled_tree_nest_under_the_owned_binary() {
    let dir = tempfile::tempdir().unwrap();
    let noted = "waybill_fixture_pkg/libwaybill-fixture-noted.so";
    let bundled = "waybill_fixture_pkg/libwaybill-fixture-bundled.so";
    package(
        dir.path(),
        Layout::Site,
        &[
            (noted, elf_with_package_note("waybill-fixture-noted", "4.5.6")),
            (bundled, elf_shared_lib(b"\0/w/app/third_party/waybill-fixture-fmt/src/format.cc\0")),
        ],
        &[],
    );
    let docs = scan(dir.path());
    let note = find_with_parent(&docs.cdx, "pkg:rpm/fedora/waybill-fixture-noted@4.5.6");
    assert_eq!(note.len(), 1, "{note:?}");
    assert!(note[0].1.as_deref().is_some_and(|p| p.starts_with("pkg:generic/libwaybill-fixture-noted.so")));
    let fmt: Vec<_> = find_with_parent(&docs.cdx, "pkg:generic/")
        .into_iter()
        .filter(|(c, _)| c["name"] == "waybill-fixture-fmt")
        .collect();
    assert_eq!(fmt.len(), 1, "{fmt:?}");
    assert!(fmt[0].1.as_deref().is_some_and(|p| p.starts_with("pkg:generic/libwaybill-fixture-bundled.so")));
    for b in ["libwaybill-fixture-noted.so", "libwaybill-fixture-bundled.so"] {
        let bins = find_with_parent(&docs.cdx, &format!("pkg:generic/{b}"));
        assert_eq!(bins[0].1.as_deref(), Some(PKG), "{b} under the package");
    }
    assert_eq!(c230(&docs), Some(serde_json::json!({"analysed": 2, "identified": 2})));
    assert_conformant(&docs, "t008 signals");
}

/// A `RECORD` entry outside the package (`../../../bin/<tool>`): a Rust
/// tool shipped in a wheel. It is owned, and its crates nest under it.
#[test]
fn t011_script_entry_with_cargo_auditable_crates() {
    let dir = tempfile::tempdir().unwrap();
    let site = package(dir.path(), Layout::Site, &[], &[]);
    let tool = cargo_auditable_elf(&[("waybill-fixture-tool", "0.4.0"), ("waybill-fixture-crate", "1.0.2")]);
    plant(dir.path(), "usr/bin/waybill-fixture-tool", &tool);
    let rec = site.join("waybill_fixture_pkg-1.2.3.dist-info/RECORD");
    let mut body = std::fs::read_to_string(&rec).unwrap();
    body.push_str("../../../bin/waybill-fixture-tool,,\n");
    std::fs::write(&rec, body).unwrap();
    let docs = scan(dir.path());
    let bins = find_with_parent(&docs.cdx, "pkg:generic/waybill-fixture-tool");
    assert_eq!(bins.len(), 1, "{bins:?}");
    assert_eq!(bins[0].1.as_deref(), Some(PKG));
    let krate = find_with_parent(&docs.cdx, "pkg:cargo/waybill-fixture-crate@1.0.2");
    assert_eq!(krate.len(), 1, "{krate:?}");
    assert!(krate[0].1.as_deref().is_some_and(|p| p.starts_with("pkg:generic/waybill-fixture-tool")));
    assert_conformant(&docs, "t011 script entry");
}

/// FR-010: one file in two `RECORD`s belongs to the `.dist-info` that
/// sorts first (`waybill_fixture_other-2.0.0` < `waybill_fixture_pkg-1.2.3`).
#[test]
fn t011_one_file_in_two_records_goes_to_the_first_sorted_owner() {
    let dir = tempfile::tempdir().unwrap();
    let site = package(dir.path(), Layout::Site, &[(LIB, zlib_library())], &[]);
    other_package(&site, &[]);
    let rec = site.join("waybill_fixture_other-2.0.0.dist-info/RECORD");
    std::fs::write(&rec, format!("{LIB},,\n")).unwrap();
    let docs = scan(dir.path());
    let bins = find_with_parent(&docs.cdx, "pkg:generic/libwaybill-fixture-z.dylib");
    assert_eq!(bins.len(), 1, "{bins:?}");
    assert_eq!(bins[0].1.as_deref(), Some("pkg:pypi/waybill-fixture-other@2.0.0"));
}

/// V2: a `.dist-info` whose `METADATA` yields no package. The library is
/// still analysed, at the top level, and not counted.
#[test]
fn t011_owner_without_a_component_is_analysed_but_not_nested() {
    let dir = tempfile::tempdir().unwrap();
    let site = package(dir.path(), Layout::Site, &[(LIB, zlib_library())], &[]);
    std::fs::write(site.join("waybill_fixture_pkg-1.2.3.dist-info/METADATA"), "not metadata\n").unwrap();
    let docs = scan(dir.path());
    assert!(purls_with_prefix(&docs.cdx, "pkg:pypi/").is_empty(), "control: no pip component");
    let bins = find_with_parent(&docs.cdx, "pkg:generic/libwaybill-fixture-z.dylib");
    assert_eq!(bins.len(), 1);
    assert_eq!(bins[0].1, None);
    assert_eq!(purls_with_prefix(&docs.cdx, "pkg:generic/zlib@"), vec!["pkg:generic/zlib@1.3.1"], "still analysed");
    assert_eq!(c230(&docs), None);
}

/// A binary naming its own package (here a bundled tree called
/// `waybill-fixture-pkg`) is nested under the binary, not merged into the
/// `pkg:pypi` component.
#[test]
fn t011_self_identifying_binary_is_nested_not_merged() {
    let dir = tempfile::tempdir().unwrap();
    package(
        dir.path(),
        Layout::Site,
        &[("waybill_fixture_pkg/_native.so", elf_shared_lib(b"\0/w/src/third_party/waybill-fixture-pkg/core.c\0"))],
        &[],
    );
    let docs = scan(dir.path());
    let own: Vec<_> = find_with_parent(&docs.cdx, "pkg:generic/")
        .into_iter()
        .filter(|(c, _)| c["name"] == "waybill-fixture-pkg")
        .collect();
    assert_eq!(own.len(), 1, "{own:?}");
    assert!(own[0].1.as_deref().is_some_and(|p| p.starts_with("pkg:generic/_native.so")));
    assert_eq!(find_with_parent(&docs.cdx, PKG).len(), 1, "the pypi component is still one component");
}
