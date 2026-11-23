//! Spec 1644 (#1641): bzip2, GMP and xz (liblzma) are versioned from the
//! bare version they embed, only when a second anchor in the same binary
//! names the library.
//!
//! Contract `specs/1644-bare-version-anchors/contracts/wire.md`. Each case
//! writes synthetic shared libraries into a fixed-name directory, scans it
//! in all three formats, and checks the curated component, `waybill sbom
//! parity-check` and unique `bom-ref`s. The anchors and versions are the
//! upstream literals (research R2); every path is a `waybill-fixture-*` name.
//!
//! `WAYBILL_VERSION_ANCHORED_BIN` runs a case against another binary, such
//! as `main`'s, for the controls (`measurements/control.md`).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::process::Command;

use common::installed_lib::{elf_shared_lib, elf_shared_lib_with_dynsym, macho_dylib_with_symbols};
use serde_json::Value;

const BZIP2_ANCHOR: &[u8] = b"\n\nbzip2/libbzip2: internal error number %d.\n\0";
const BZIP2_VERSION: &[u8] = b"1.0.8, 13-Jul-2019\0";
const GMP_ANCHOR: &[u8] = b"GNU MP: Cannot allocate memory (size=%lu)\n\0";

// ---------------------------------------------------------------------
// Harness (T003)
// ---------------------------------------------------------------------

struct Scan {
    cdx: Value,
    stderr: String,
    _dir: tempfile::TempDir,
}

fn binary() -> String {
    std::env::var("WAYBILL_VERSION_ANCHORED_BIN").unwrap_or_else(|_| common::bin().to_string())
}

/// The scanned directory has a fixed name, so the document's root does not
/// vary between runs.
const ROOT: &str = "waybill-fixture-root";

/// `.rodata` (or `__cstring`) bytes: a leading string, then `parts` joined.
fn region(parts: &[&[u8]]) -> Vec<u8> {
    let mut r = b"waybill-fixture-leading\0".to_vec();
    for p in parts {
        r.extend_from_slice(p);
    }
    r
}

/// A fresh temp directory whose `ROOT` holds each `(relative path, bytes)`.
fn tree(files: &[(&str, Vec<u8>)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, bytes) in files {
        let p = dir.path().join(ROOT).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    dir
}

fn elf(rodata: &[u8], defined: &[&str], undefined: &[&str]) -> Vec<u8> {
    if defined.is_empty() && undefined.is_empty() {
        elf_shared_lib(rodata)
    } else {
        elf_shared_lib_with_dynsym(rodata, defined, undefined)
    }
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

/// Sorted PURLs (qualifiers dropped) of the components named `slug`.
fn curated(s: &Scan, slug: &str) -> Vec<String> {
    let mut v: Vec<String> = cdx_components(&s.cdx)
        .into_iter()
        .filter(|c| c["name"] == slug)
        .filter_map(|c| c["purl"].as_str().map(|p| p.split('?').next().unwrap_or(p).to_string()))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// `waybill sbom parity-check` over the three documents; a row is a gap
/// only if some format carries a value (as in spec 1643's harness).
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

/// The `anchored` census events for `slug`. The needles are raw JSON, so a
/// Debug-escaped field cannot satisfy them. The field is matched with its
/// leading space: `unanchored=` also contains `anchored=`.
fn anchored<'a>(s: &'a Scan, slug: &str) -> Vec<&'a str> {
    let lib = format!(r#""library":"{slug}""#);
    s.stderr
        .lines()
        .filter(|l| l.contains("waybill::version_census") && l.contains(" anchored=") && l.contains(&lib))
        .collect()
}

fn all_anchored(s: &Scan) -> usize {
    s.stderr.lines().filter(|l| l.contains("waybill::version_census") && l.contains(" anchored=")).count()
}

/// The serialized component with this exact PURL (qualifiers dropped), for
/// checking which CPEs it carries wherever the format puts them.
fn component_text(s: &Scan, purl: &str) -> String {
    let c = cdx_components(&s.cdx)
        .into_iter()
        .find(|c| c["purl"].as_str().map(|p| p.split('?').next().unwrap_or(p)) == Some(purl))
        .unwrap_or_else(|| panic!("no component {purl}"));
    c.to_string()
}

// ---------------------------------------------------------------------
// T002: the helpers write what they claim
// ---------------------------------------------------------------------

/// Without this, a helper that wrote every symbol as undefined would make
/// N-3 and N-9 pass for the wrong reason.
#[test]
fn helpers_mark_symbols_defined_and_undefined() {
    use object::{Object, ObjectSymbol};
    let e = elf_shared_lib_with_dynsym(b"x\0", &["waybill_fixture_def"], &["waybill_fixture_imp"]);
    let f = object::File::parse(&*e).unwrap();
    let elf: Vec<(String, bool)> = f
        .dynamic_symbols()
        .filter_map(|s| s.name().ok().filter(|n| !n.is_empty()).map(|n| (n.to_string(), s.is_undefined())))
        .collect();
    assert_eq!(elf, [("waybill_fixture_def".to_string(), false), ("waybill_fixture_imp".to_string(), true)]);

    let m = macho_dylib_with_symbols(b"x\0", &["waybill_fixture_def"], &["waybill_fixture_imp"]);
    let f = object::File::parse(&*m).unwrap();
    let macho: Vec<(String, bool, bool)> = f
        .symbols()
        .filter_map(|s| s.name().ok().map(|n| (n.to_string(), s.is_global(), s.is_undefined())))
        .collect();
    assert_eq!(
        macho,
        [("_waybill_fixture_def".to_string(), true, false), ("_waybill_fixture_imp".to_string(), true, true)]
    );
}

// ---------------------------------------------------------------------
// T005: only a DEFINED anchor symbol anchors, on ELF and Mach-O
// ---------------------------------------------------------------------

#[test]
fn defined_anchor_symbol_anchors_elf_and_macho() {
    let r = region(&[b"5.8.1\0"]);
    // Spec 1647 adds the table and binding each symbol anchor was read from.
    for (name, bytes, anchor) in [
        (
            "libwaybill-fixture-lzma.so.5",
            elf(&r, &["lzma_version_string"], &[]),
            r#"{"binding":"global","kind":"symbol","table":"dynsym","value":"lzma_version_string"}"#,
        ),
        (
            "libwaybill-fixture-lzma.5.dylib",
            macho_dylib_with_symbols(&r, &["lzma_version_string"], &[]),
            r#"{"binding":"global","kind":"symbol","table":"macho-symtab","value":"lzma_version_string"}"#,
        ),
    ] {
        let s = census(tree(&[(name, bytes)]));
        let ev = anchored(&s, "liblzma");
        assert_eq!(ev.len(), 1, "{name}: {ev:?}\n{}", s.stderr);
        assert!(ev[0].contains(anchor), "{name}: {}", ev[0]);
    }
}

#[test]
fn imported_or_unlisted_symbol_anchors_nothing() {
    let r = region(&[b"5.8.1\0"]);
    for (name, bytes) in [
        ("libwaybill-fixture-a.so.1", elf(&r, &[], &["lzma_version_string"])),
        ("libwaybill-fixture-b.so.1", elf(&r, &["lzma_code"], &[])),
        ("libwaybill-fixture-a.1.dylib", macho_dylib_with_symbols(&r, &[], &["lzma_version_string"])),
        ("libwaybill-fixture-b.1.dylib", macho_dylib_with_symbols(&r, &["lzma_code"], &[])),
    ] {
        let s = census(tree(&[(name, bytes)]));
        assert_eq!(all_anchored(&s), 0, "{name}:\n{}", s.stderr);
    }
}

// ---------------------------------------------------------------------
// T007: census positive control (C-1) and its negative (C-2)
// ---------------------------------------------------------------------

#[test]
fn c1_census_reports_an_anchored_library() {
    let s = census(tree(&[("libwaybill-fixture-gmp.so.10", elf(&region(&[GMP_ANCHOR, b"6.3.0\0"]), &[], &[]))]));
    let ev = anchored(&s, "gmp");
    assert_eq!(ev.len(), 1, "{ev:?}\n{}", s.stderr);
    for needle in [r#""kind":"text""#, r#""value":"GNU MP:""#, r#""verdict":"versioned""#, r#""version":"6.3.0""#] {
        assert!(ev[0].contains(needle), "{needle} missing from {}", ev[0]);
    }
}

/// FR-006's census: bzip2's date form in a binary without its anchor is
/// reported as `unanchored`, and never emitted.
#[test]
fn c3_census_reports_bzip2_form_without_anchor() {
    let s = census(tree(&[("libwaybill-fixture-bz2.so.1", elf(&region(&[BZIP2_VERSION]), &[], &[]))]));
    let ev: Vec<&str> = s.stderr.lines().filter(|l| l.contains("unanchored=")).collect();
    assert_eq!(ev.len(), 1, "{}", s.stderr);
    assert!(ev[0].contains(r#""value":"1.0.8, 13-Jul-2019""#), "{}", ev[0]);
    none(&s, "bzip2");
}

/// G-1 (census): GMP's TheRock layout, `6.3.0\0` straight after a binary
/// table in `.rodata` (`measurements/census.md`), is a candidate.
#[test]
fn g1_census_gmp_after_binary_data() {
    let s = census(tree(&[(
        "libwaybill-fixture-gmp.so.10",
        elf(&region(&[GMP_ANCHOR, b"\x9d\x07\x89\xff", b"6.3.0\0"]), &[], &[]),
    )]));
    let ev = anchored(&s, "gmp");
    assert_eq!(ev.len(), 1, "{}", s.stderr);
    for needle in [r#""verdict":"versioned""#, r#""preceding":"binary""#] {
        assert!(ev[0].contains(needle), "{needle} missing from {}", ev[0]);
    }
}

#[test]
fn c2_no_anchor_no_census_event() {
    let s = census(tree(&[("libwaybill-fixture-plain.so.1", elf(&region(&[b"6.3.0\0", BZIP2_VERSION]), &[], &[]))]));
    assert_eq!(all_anchored(&s), 0, "{}", s.stderr);
}

// ---------------------------------------------------------------------
// User Story 1: anchored libraries are versioned (A-1 to A-5)
// ---------------------------------------------------------------------

#[test]
fn a1_bzip2() {
    let s = scan3(tree(&[("libwaybill-fixture-bz2.so.1", elf(&region(&[BZIP2_ANCHOR, BZIP2_VERSION]), &[], &[]))]));
    assert_eq!(curated(&s, "bzip2"), ["pkg:generic/bzip2@1.0.8"]);
}

#[test]
fn a2_gmp() {
    let s = scan3(tree(&[("libwaybill-fixture-gmp.so.10", elf(&region(&[GMP_ANCHOR, b"6.3.0\0"]), &[], &[]))]));
    assert_eq!(curated(&s, "gmp"), ["pkg:generic/gmp@6.3.0"]);
}

#[test]
fn a3_liblzma() {
    let s = scan3(tree(&[(
        "libwaybill-fixture-lzma.so.5",
        elf(&region(&[b"5.8.1\0"]), &["lzma_version_string"], &[]),
    )]));
    assert_eq!(curated(&s, "liblzma"), ["pkg:generic/liblzma@5.8.1"]);
}

#[test]
fn a4_liblzma_prerelease() {
    let s = scan3(tree(&[(
        "libwaybill-fixture-lzma.so.5",
        elf(&region(&[b"5.7.1alpha\0"]), &["lzma_version_number"], &[]),
    )]));
    assert_eq!(curated(&s, "liblzma"), ["pkg:generic/liblzma@5.7.1alpha"]);
}

#[test]
fn a5_liblzma_macho() {
    let s = scan3(tree(&[(
        "libwaybill-fixture-lzma.5.dylib",
        macho_dylib_with_symbols(&region(&[b"5.8.1\0"]), &["lzma_version_string"], &[]),
    )]));
    assert_eq!(curated(&s, "liblzma"), ["pkg:generic/liblzma@5.8.1"]);
}

/// T012: each versioned library carries the curated table's CPEs
/// (contracts/wire.md).
#[test]
fn a_cpe_candidates() {
    let s = scan3(tree(&[
        ("libwaybill-fixture-bz2.so.1", elf(&region(&[BZIP2_ANCHOR, BZIP2_VERSION]), &[], &[])),
        ("libwaybill-fixture-gmp.so.10", elf(&region(&[GMP_ANCHOR, b"6.3.0\0"]), &[], &[])),
        ("libwaybill-fixture-lzma.so.5", elf(&region(&[b"5.8.1\0"]), &["lzma_version_string"], &[])),
    ]));
    for (purl, want) in [
        ("pkg:generic/bzip2@1.0.8", &["cpe:2.3:a:bzip:bzip2:1.0.8"][..]),
        ("pkg:generic/gmp@6.3.0", &["cpe:2.3:a:gmplib:gmp:6.3.0"][..]),
        ("pkg:generic/liblzma@5.8.1", &["cpe:2.3:a:tukaani:xz:5.8.1", "cpe:2.3:a:xz_project:xz:5.8.1"][..]),
    ] {
        let text = component_text(&s, purl);
        for cpe in want {
            assert!(text.contains(cpe), "{purl} lacks {cpe}: {text}");
        }
    }
}

/// G-1: the same layout gives the versioned component.
#[test]
fn g1_gmp_after_binary_data() {
    let s = scan3(tree(&[(
        "libwaybill-fixture-gmp.so.10",
        elf(&region(&[GMP_ANCHOR, b"\x9d\x07\x89\xff", b"6.3.0\0"]), &[], &[]),
    )]));
    assert_eq!(curated(&s, "gmp"), ["pkg:generic/gmp@6.3.0"]);
}

// ---------------------------------------------------------------------
// User Story 2: a bare version is never a component by itself (N-1 to N-9)
// ---------------------------------------------------------------------

/// Nothing named `slug` in the document.
fn none(s: &Scan, slug: &str) {
    assert!(curated(s, slug).is_empty(), "{slug}: {:?}", curated(s, slug));
}

#[test]
fn n1_bzip2_without_anchor() {
    let s = scan3(tree(&[("libwaybill-fixture-bz2.so.1", elf(&region(&[BZIP2_VERSION]), &[], &[]))]));
    none(&s, "bzip2");
}

#[test]
fn n2_gmp_without_anchor() {
    let s = scan3(tree(&[("libwaybill-fixture-gmp.so.10", elf(&region(&[b"6.3.0\0"]), &[], &[]))]));
    none(&s, "gmp");
}

#[test]
fn n3_liblzma_imported_symbol() {
    let s = scan3(tree(&[(
        "libwaybill-fixture-lzma.so.5",
        elf(&region(&[b"5.8.1\0"]), &[], &["lzma_version_string"]),
    )]));
    none(&s, "liblzma");
}

#[test]
fn n4_gmp_two_candidates_is_ambiguous() {
    let s = census(tree(&[(
        "libwaybill-fixture-gmp.so.10",
        elf(&region(&[GMP_ANCHOR, b"6.3.0\0", b"6.2.1\0"]), &[], &[]),
    )]));
    none(&s, "gmp");
    let ev = anchored(&s, "gmp");
    assert_eq!(ev.len(), 1, "{}", s.stderr);
    for needle in [r#""verdict":"ambiguous""#, r#""value":"6.3.0""#, r#""value":"6.2.1""#] {
        assert!(ev[0].contains(needle), "{needle} missing from {}", ev[0]);
    }
}

#[test]
fn n5_shared_candidate_versions_neither() {
    let s = census(tree(&[(
        "libwaybill-fixture-both.so.1",
        elf(&region(&[GMP_ANCHOR, b"5.8.1\0"]), &["lzma_version_string"], &[]),
    )]));
    none(&s, "gmp");
    none(&s, "liblzma");
    for (slug, with) in [("gmp", "liblzma"), ("liblzma", "gmp")] {
        let ev = anchored(&s, slug);
        assert_eq!(ev.len(), 1, "{slug}: {}", s.stderr);
        let want = format!(r#""with":"{with}""#);
        assert!(ev[0].contains(r#""verdict":"ambiguous-shared""#) && ev[0].contains(&want), "{}", ev[0]);
    }
}

#[test]
fn n6_version_inside_a_longer_string() {
    let s = census(tree(&[(
        "libwaybill-fixture-gmp.so.10",
        elf(&region(&[GMP_ANCHOR, b"requires 6.3.0 or later\0"]), &[], &[]),
    )]));
    none(&s, "gmp");
    let ev = anchored(&s, "gmp");
    assert_eq!(ev.len(), 1, "{}", s.stderr);
    assert!(ev[0].contains(r#""verdict":"no-candidate""#), "{}", ev[0]);
}

#[test]
fn n7_liblzma_snapshot_suffix() {
    let s = census(tree(&[(
        "libwaybill-fixture-lzma.so.5",
        elf(&region(&[b"4.999.8beta-21-g1d92\0"]), &["lzma_version_string"], &[]),
    )]));
    none(&s, "liblzma");
    let ev = anchored(&s, "liblzma");
    assert_eq!(ev.len(), 1, "{}", s.stderr);
    assert!(ev[0].contains(r#""verdict":"no-candidate""#), "{}", ev[0]);
}

#[test]
fn n8_bzip2_needs_its_date_form() {
    let s = scan3(tree(&[("libwaybill-fixture-bz2.so.1", elf(&region(&[BZIP2_ANCHOR, b"1.0.8\0"]), &[], &[]))]));
    none(&s, "bzip2");
}

#[test]
fn n9_liblzma_macho_imported_symbol() {
    let s = scan3(tree(&[(
        "libwaybill-fixture-lzma.5.dylib",
        macho_dylib_with_symbols(&region(&[b"5.8.1\0"]), &[], &["lzma_version_string"]),
    )]));
    none(&s, "liblzma");
}

// ---------------------------------------------------------------------
// User Story 3: one library, one component (P-1, P-2)
// ---------------------------------------------------------------------

fn lzma_with_pc(pc_version: &str) -> Scan {
    scan3(tree(&[
        ("lib/libwaybill-fixture-lzma.so.5", elf(&region(&[b"5.8.1\0"]), &["lzma_version_string"], &[])),
        ("lib/pkgconfig/liblzma.pc", format!("Version: {pc_version}\n").into_bytes()),
    ]))
}

#[test]
fn p1_agreeing_pc_file_gives_one_component() {
    let s = lzma_with_pc("5.8.1");
    assert_eq!(curated(&s, "liblzma"), ["pkg:generic/liblzma@5.8.1"]);
    let c: Vec<&Value> = cdx_components(&s.cdx)
        .into_iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:generic/liblzma@5.8.1")))
        .collect();
    assert_eq!(c.len(), 1, "{c:#?}");
    // The one component carries the embedded match too, not only the `.pc`
    // file: without this the case passes on `main`, where only the `.pc`
    // file names the library.
    let text = component_text(&s, "pkg:generic/liblzma@5.8.1");
    assert!(text.contains("libwaybill-fixture-lzma.so.5"), "the library file is not on the component: {text}");
}

#[test]
fn p2_disagreeing_pc_file_keeps_both_versions() {
    let s = lzma_with_pc("5.8.0");
    assert_eq!(curated(&s, "liblzma"), ["pkg:generic/liblzma@5.8.0", "pkg:generic/liblzma@5.8.1"]);
}

