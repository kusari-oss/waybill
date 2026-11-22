//! Spec 1645 (#1662): one library, one component, when the curated
//! embedded-version scanner and an installed-library record (pkg-config,
//! CMake package version) name or version it differently.
//!
//! Contract `specs/1645-split-library-identity/contracts/wire.md`. Each case
//! writes a synthetic tree into a fixed-name directory, scans it in all three
//! formats, and checks the components, `waybill sbom parity-check` and
//! unique `bom-ref`s. File and record names are `waybill-fixture-*` except
//! where the library's own name is what is under test (`ncurses`,
//! `ncursesw`, `LLVM`, `expat`): the merge is decided by those names.
//!
//! `WAYBILL_LIBRARY_IDENTITY_BIN` runs a case against another binary, such
//! as `main`'s, for the controls (`measurements/control.md`).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::process::Command;

use common::installed_lib::elf_shared_lib;
use serde_json::Value;

const NCURSES: &[u8] = b"ncurses 6.6.20251230\0";
const LLVM: &[u8] = b"LLVM version 23.0.0git\0";
const EXPAT: &[u8] = b"expat_2.7.5\0";
const ZLIB: &[u8] = b" deflate 1.3.1 Copyright 1995-2024 Jean-loup Gailly and Mark Adler \0";

// ---------------------------------------------------------------------
// Harness (T002)
// ---------------------------------------------------------------------

struct Scan {
    cdx: Value,
    stderr: String,
    _dir: tempfile::TempDir,
}

fn binary() -> String {
    std::env::var("WAYBILL_LIBRARY_IDENTITY_BIN").unwrap_or_else(|_| common::bin().to_string())
}

/// The scanned directory has a fixed name, so the document's root does not
/// vary between runs.
const ROOT: &str = "waybill-fixture-root";

/// `.rodata` bytes: a leading string, then `banner`, NUL-preceded.
fn lib(banner: &[u8]) -> Vec<u8> {
    let mut r = b"waybill-fixture-leading\0".to_vec();
    r.extend_from_slice(banner);
    elf_shared_lib(&r)
}

fn tree(files: &[(&str, Vec<u8>)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, bytes) in files {
        let p = dir.path().join(ROOT).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    dir
}

fn pc(version: &str, extra: &str) -> Vec<u8> {
    format!("Version: {version}\n{extra}").into_bytes()
}

fn cmake_version(version: &str) -> Vec<u8> {
    format!("set(PACKAGE_VERSION \"{version}\")\n").into_bytes()
}

fn scan_with_log(dir: tempfile::TempDir, filter: &str) -> Scan {
    let out_dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let files = [
        out_dir.path().join("waybill.cdx.json"),
        out_dir.path().join("waybill.spdx.json"),
        out_dir.path().join("waybill.spdx3.json"),
    ];
    let out = Command::new(binary())
        .env("HOME", home.path())
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", filter)
        .env("NO_COLOR", "1")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(dir.path().join(ROOT))
        .args(["--no-deep-hash", "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", files[0].display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", files[1].display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", files[2].display()))
        .output()
        .unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    let cdx: Value = serde_json::from_slice(&std::fs::read(&files[0]).unwrap()).unwrap();
    parity(&out_dir);
    unique_bom_refs(&cdx);
    Scan { cdx, stderr: String::from_utf8_lossy(&out.stderr).into_owned(), _dir: dir }
}

fn scan3(dir: tempfile::TempDir) -> Scan {
    scan_with_log(dir, "warn")
}

fn census(dir: tempfile::TempDir) -> Scan {
    scan_with_log(dir, "waybill::version_census=debug")
}

/// Every CycloneDX component, nested ones and `metadata.component` included.
fn cdx_components(cdx: &Value) -> Vec<&Value> {
    let mut out = vec![&cdx["metadata"]["component"]];
    let mut stack: Vec<&Value> = cdx["components"].as_array().into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c["components"].as_array().into_iter().flatten());
    }
    out
}

/// Sorted PURLs (qualifiers dropped) of the components named `name`.
fn named(s: &Scan, name: &str) -> Vec<String> {
    let mut v: Vec<String> = cdx_components(&s.cdx)
        .into_iter()
        .filter(|c| c["name"] == name)
        .filter_map(|c| c["purl"].as_str().map(|p| p.split('?').next().unwrap_or(p).to_string()))
        .collect();
    v.sort();
    v.dedup();
    v
}

fn component<'a>(s: &'a Scan, purl: &str) -> &'a Value {
    cdx_components(&s.cdx)
        .into_iter()
        .find(|c| c["purl"].as_str().map(|p| p.split('?').next().unwrap_or(p)) == Some(purl))
        .unwrap_or_else(|| panic!("no component {purl}"))
}

fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

/// `waybill sbom parity-check` over the three documents; a row is a gap only
/// if some format carries a value (as in specs 1643 and 1644).
fn parity(out_dir: &tempfile::TempDir) {
    let out = Command::new(common::bin())
        .args(["sbom", "parity-check", "--format", "json", "--scan-dir"])
        .arg(out_dir.path())
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let gaps: Vec<&Value> = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["universal_parity"] == false)
        .filter(|r| ["cdx", "spdx23", "spdx3"].iter().any(|f| r[*f]["kind"] == "present"))
        .collect();
    assert!(gaps.is_empty(), "parity gaps: {gaps:#?}");
}

fn unique_bom_refs(cdx: &Value) {
    let mut seen = BTreeSet::new();
    let dups: Vec<&str> = cdx_components(cdx)
        .into_iter()
        .skip(1)
        .filter_map(|c| c["bom-ref"].as_str())
        .filter(|r| !seen.insert(*r))
        .collect();
    assert!(dups.is_empty(), "duplicate bom-refs: {dups:?}");
}

/// The census events with this field. The field is matched with its leading
/// space, so one field name inside another cannot satisfy it (spec 1644's
/// `unanchored=` lesson).
fn events<'a>(s: &'a Scan, field: &str) -> Vec<&'a str> {
    let needle = format!(" {field}=");
    s.stderr.lines().filter(|l| l.contains("waybill::version_census") && l.contains(&needle)).collect()
}

/// The PURLs (qualifiers dropped) the component with `purl` depends on.
fn depends_on(s: &Scan, purl: &str) -> Vec<String> {
    let by_ref: std::collections::BTreeMap<&str, &str> = cdx_components(&s.cdx)
        .into_iter()
        .filter_map(|c| Some((c["bom-ref"].as_str()?, c["purl"].as_str()?)))
        .collect();
    let me = component(s, purl)["bom-ref"].as_str().unwrap();
    let mut v: Vec<String> = s.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["ref"] == me)
        .flat_map(|d| d["dependsOn"].as_array().into_iter().flatten())
        .filter_map(|r| r.as_str())
        .map(|r| by_ref.get(r).copied().unwrap_or(r).split('?').next().unwrap_or(r).to_string())
        .collect();
    v.sort();
    v
}

// ---------------------------------------------------------------------
// Trees (contracts/wire.md)
// ---------------------------------------------------------------------

fn ncurses_tree(pc_version: &str, extra: &[(&str, Vec<u8>)]) -> tempfile::TempDir {
    let mut files = vec![
        ("lib/libncursesw.so.6", lib(NCURSES)),
        ("lib/pkgconfig/ncursesw.pc", pc(pc_version, "")),
    ];
    files.extend(extra.iter().cloned());
    tree(&files)
}

fn llvm_tree() -> tempfile::TempDir {
    tree(&[
        ("lib/libLLVM.so", lib(LLVM)),
        ("lib/cmake/llvm/LLVMConfigVersion.cmake", cmake_version("23.0.0git")),
    ])
}

fn expat_tree(pc_version: &str) -> tempfile::TempDir {
    tree(&[("lib/libexpat.so.1", lib(EXPAT)), ("lib/pkgconfig/expat.pc", pc(pc_version, ""))])
}

// ---------------------------------------------------------------------
// Cases that change (red on the log-only commit)
// ---------------------------------------------------------------------

/// M-1: the name variant. One component, named and versioned as the
/// library, carrying both readers' files and NVD's CPE form.
#[test]
fn m1_name_variant_is_one_component() {
    let s = scan3(ncurses_tree("6.6.20251230", &[]));
    assert_eq!(named(&s, "ncurses"), ["pkg:generic/ncurses@6.6.20251230"]);
    assert!(named(&s, "ncursesw").is_empty(), "{:?}", named(&s, "ncursesw"));
    let c = component(&s, "pkg:generic/ncurses@6.6.20251230");
    let files = prop(c, "waybill:source-files").unwrap_or_default();
    assert!(files.contains("lib/libncursesw.so.6") && files.contains("lib/pkgconfig/ncursesw.pc"), "{files}");
    // Research R7: the record's mechanism survives; the curated evidence
    // kind does not, as for every curated-plus-record merge (#1665).
    assert_eq!(prop(c, "waybill:source-mechanism"), Some("pkg-config"));
    let text = c.to_string();
    for vendor in ["invisible-island", "ncurses_project", "gnu"] {
        let cpe = format!("cpe:2.3:a:{vendor}:ncurses:6.6:20251230:");
        assert!(text.contains(&cpe), "{cpe} not on {text}");
    }
}

/// M-2: the version suffix. The curated `23.0.0git` equals the CMake
/// record's, so the existing PURL merge gives one component.
#[test]
fn m2_version_suffix_is_one_component() {
    let s = scan3(llvm_tree());
    assert_eq!(named(&s, "llvm"), ["pkg:generic/llvm@23.0.0git"]);
    let files = prop(component(&s, "pkg:generic/llvm@23.0.0git"), "waybill:source-files").unwrap_or_default().to_string();
    assert!(files.contains("lib/libLLVM.so") && files.contains("LLVMConfigVersion.cmake"), "{files}");
}

/// M-3: a record more exact than its banner. The curated `2.7.5` is a dotted
/// prefix of the record's `2.7.5.1`.
#[test]
fn m3_dotted_prefix_takes_the_record_version() {
    let s = scan3(expat_tree("2.7.5.1"));
    assert_eq!(named(&s, "expat"), ["pkg:generic/expat@2.7.5.1"]);
    let files = prop(component(&s, "pkg:generic/expat@2.7.5.1"), "waybill:source-files").unwrap_or_default().to_string();
    assert!(files.contains("lib/libexpat.so.1") && files.contains("expat.pc"), "{files}");
}

/// E-1: an edge to the renamed record follows it.
#[test]
fn e1_requires_follows_the_merge() {
    let s = scan3(ncurses_tree(
        "6.6.20251230",
        &[
            ("lib/libformw.so.6", elf_shared_lib(b"\0waybill-fixture-formw\0")),
            ("lib/pkgconfig/formw.pc", pc("6.6.20251230", "Requires: ncursesw\n")),
        ],
    ));
    let deps = depends_on(&s, "pkg:generic/formw@6.6.20251230");
    assert!(deps.contains(&"pkg:generic/ncurses@6.6.20251230".to_string()), "{deps:?}");
    assert!(!deps.iter().any(|d| d.contains("ncursesw")), "a dangling edge: {deps:?}");
}

// ---------------------------------------------------------------------
// Cases that keep two components (guards; teeth from the mutations)
// ---------------------------------------------------------------------

fn verdicts(s: &Scan) -> Vec<String> {
    events(s, "shared_file")
        .into_iter()
        .filter_map(|l| l.split(r#""verdict":""#).nth(1).and_then(|r| r.split('"').next()).map(str::to_string))
        .collect()
}

/// N-1: a disagreeing record keeps both.
#[test]
fn n1_disagreeing_version_keeps_both() {
    let s = census(ncurses_tree("6.5.20240427", &[]));
    assert_eq!(named(&s, "ncursesw"), ["pkg:generic/ncursesw@6.5.20240427"]);
    assert_eq!(named(&s, "ncurses").len(), 1, "{:?}", named(&s, "ncurses"));
    assert_eq!(verdicts(&s), ["version-disagrees"]);
}

/// N-2: a record for a library that bundles zlib is not zlib.
#[test]
fn n2_bundled_library_is_not_merged() {
    let s = census(tree(&[
        ("lib/libwaybill-fixture-foo.so.1", lib(ZLIB)),
        ("lib/pkgconfig/waybill-fixture-foo.pc", pc("1.3.1", "")),
    ]));
    assert_eq!(named(&s, "zlib"), ["pkg:generic/zlib@1.3.1"]);
    assert_eq!(named(&s, "waybill-fixture-foo"), ["pkg:generic/waybill-fixture-foo@1.3.1"]);
    assert_eq!(verdicts(&s), ["name-unrelated"]);
}

/// N-3: a string prefix that is not at a `.` does not agree.
#[test]
fn n3_string_prefix_does_not_agree() {
    let s = census(expat_tree("2.7.50"));
    assert_eq!(named(&s, "expat"), ["pkg:generic/expat@2.7.5", "pkg:generic/expat@2.7.50"]);
    assert_eq!(verdicts(&s), ["version-disagrees"]);
}

/// N-4, integration form: a second qualifying record reaches the file only
/// through CMake exported targets, and the readers do attach both
/// (`measurements/control.md`). The census says `ambiguous` and nothing
/// merges.
#[test]
fn n4_two_qualifying_records() {
    let s = census(ncurses_tree(
        "6.6.20251230",
        &[
            ("lib/cmake/ncursestw/ncursestw-config-version.cmake", cmake_version("6.6.20251230")),
            (
                "lib/cmake/ncursestw/ncursestw-targets.cmake",
                b"get_filename_component(_IMPORT_PREFIX \"${CMAKE_CURRENT_LIST_FILE}\" PATH)\n\
                  get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n\
                  get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n\
                  get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n"
                    .to_vec(),
            ),
            (
                "lib/cmake/ncursestw/ncursestw-targets-release.cmake",
                b"set_target_properties(wf::t0 PROPERTIES\n  IMPORTED_LOCATION_RELEASE \"${_IMPORT_PREFIX}/lib/libncursesw.so.6\"\n  )\n"
                    .to_vec(),
            ),
        ],
    ));
    assert_eq!(verdicts(&s), ["ambiguous"]);
    assert_eq!(named(&s, "ncursesw"), ["pkg:generic/ncursesw@6.6.20251230"], "ambiguous: nothing merges");
    assert_eq!(named(&s, "ncursestw"), ["pkg:generic/ncursestw@6.6.20251230"]);
}

// ---------------------------------------------------------------------
// Census controls
// ---------------------------------------------------------------------

/// C-1: the census fires on the case it counts.
#[test]
fn c1_census_reports_the_shared_file() {
    let s = census(ncurses_tree("6.6.20251230", &[]));
    let e = events(&s, "shared_file");
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(e[0].contains(r#""verdict":"merge""#), "{}", e[0]);
    assert!(e[0].contains(r#""name":"ncursesw""#) && e[0].contains(r#""mechanism":"pkg-config""#), "{}", e[0]);
}

/// C-2: no record, no event.
#[test]
fn c2_no_record_no_event() {
    let s = census(tree(&[("lib/libwaybill-fixture-ncurses.so.6", lib(NCURSES))]));
    assert!(events(&s, "shared_file").is_empty(), "{:#?}", events(&s, "shared_file"));
    assert_eq!(named(&s, "ncurses").len(), 1);
}
