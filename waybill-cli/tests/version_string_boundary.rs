//! Spec 1643 (#1568, #1641): an embedded version string is found when a
//! byte other than NUL precedes it, for the three curated patterns whose
//! match is fixed at both ends (zlib, expat, ncurses).
//!
//! Contract `specs/1643-version-string-boundary/contracts/wire.md`. Each
//! case writes one synthetic ELF shared library, with the banner in
//! `.rodata`, into its own temp directory and scans it in all three
//! formats. Each checks the curated component, `waybill sbom parity-check`
//! and unique `bom-ref`s.
//!
//! `WAYBILL_VERSION_BOUNDARY_BIN` runs a case against another binary, such
//! as `main`'s, for the controls (`measurements/control.md`) and for B-1's
//! golden (`WAYBILL_UPDATE_VERSION_BOUNDARY_GOLDENS=1`).

#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::installed_lib::elf_shared_lib;
use corpus_harness_195::layer2_golden::{mask_nondeterministic, masked_difference};
use serde_json::Value;

const ZLIB: &[u8] = b" deflate 1.3.2 Copyright 1995-2026 Jean-loup Gailly and Mark Adler \0";
const ZLIB_NO_SPACE: &[u8] = b"deflate 1.3.2 Copyright 1995-2026 Jean-loup Gailly and Mark Adler \0";
const NCURSES: &[u8] = b"ncurses 6.6.20251230\0";
const EXPAT: &[u8] = b"expat_2.7.5\0";
const OPENSSL: &[u8] = b"OpenSSL 3.0.11\0";

const ZLIB_V: &str = "pkg:generic/zlib@1.3.2";
const NCURSES_V: &str = "pkg:generic/ncurses@6.6";
const EXPAT_V: &str = "pkg:generic/expat@2.7.5";
const OPENSSL_V: &str = "pkg:generic/openssl@3.0.11";

// ---------------------------------------------------------------------
// Harness (T001)
// ---------------------------------------------------------------------

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    stderr: String,
    dir: tempfile::TempDir,
}

fn binary() -> String {
    std::env::var("WAYBILL_VERSION_BOUNDARY_BIN").unwrap_or_else(|_| common::bin().to_string())
}

/// `.rodata` holding a leading string, then `preceding` (if any), then
/// `literal`.
fn rodata(preceding: Option<u8>, literal: &[u8]) -> Vec<u8> {
    let mut r = b"waybill-fixture-leading\0".to_vec();
    r.extend(preceding);
    r.extend_from_slice(literal);
    r
}

/// The scanned directory has a fixed name, so the document's root (named
/// after it) does not vary between runs.
const ROOT: &str = "waybill-fixture-root";

/// A fresh temp directory whose `ROOT` subdirectory holds
/// `elf_shared_lib(bytes)` as `file_name`.
fn lib_dir(bytes: &[u8], file_name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(ROOT)).unwrap();
    std::fs::write(dir.path().join(ROOT).join(file_name), elf_shared_lib(bytes)).unwrap();
    dir
}

/// All three formats in one invocation, with `RUST_LOG=<filter>` and the
/// log captured.
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
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    let s = Scan {
        cdx: read(&files[0]),
        spdx23: read(&files[1]),
        spdx3: read(&files[2]),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        dir,
    };
    parity(&out_dir, &s);
    unique_bom_refs(&s.cdx);
    s
}

fn scan3(dir: tempfile::TempDir) -> Scan {
    scan_with_log(dir, "warn")
}

fn scan_lib(preceding: Option<u8>, literal: &[u8], file_name: &str) -> Scan {
    scan3(lib_dir(&rodata(preceding, literal), file_name))
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

/// Sorted PURLs of the components named `slug`.
fn curated(cdx: &Value, slug: &str) -> Vec<String> {
    let mut v: Vec<String> = cdx_components(cdx)
        .into_iter()
        .filter(|c| c["name"] == slug)
        .filter_map(|c| c["purl"].as_str().map(|p| p.split('?').next().unwrap_or(p).to_string()))
        .collect();
    v.sort();
    v.dedup();
    v
}

fn has(s: &Scan, slug: &str, purl: &str) -> bool {
    curated(&s.cdx, slug).iter().any(|p| p == purl)
}

/// `waybill sbom parity-check` over the scan's three documents. A row is a
/// gap only if some format carries a value: a row absent in two formats and
/// `restricted` in the third (A5 on a document with no package author) is a
/// gap to the tool on `main` too, for every document without that field, and
/// says nothing about the formats disagreeing.
fn parity(out_dir: &tempfile::TempDir, s: &Scan) {
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
    assert!(gaps.is_empty(), "parity gaps for {}: {gaps:#?}", s.dir.path().display());
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

fn masked(v: &Value, dir: &Path) -> Value {
    let s = serde_json::to_string(&mask_nondeterministic(v)).unwrap();
    let d = serde_json::to_string(&dir.display().to_string()).unwrap();
    let d = &d[1..d.len() - 1];
    serde_json::from_str(&s.replace(d, "/TMP")).unwrap()
}

// ---------------------------------------------------------------------
// User Story 3 / FR-006: B-1 is byte-identical to `main` (T002)
// ---------------------------------------------------------------------

const GOLDEN_ENV: &str = "WAYBILL_UPDATE_VERSION_BOUNDARY_GOLDENS";

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/version_string_boundary/b1-golden")
}

#[test]
fn b1_nul_preceded_banners_are_byte_identical_to_main() {
    let mut r = b"waybill-fixture-leading\0".to_vec();
    r.extend_from_slice(ZLIB);
    r.extend_from_slice(EXPAT);
    r.extend_from_slice(NCURSES);
    let s = scan3(lib_dir(&r, "libwaybill-fixture-bundle.so.1"));
    assert!(has(&s, "zlib", ZLIB_V) && has(&s, "expat", EXPAT_V) && has(&s, "ncurses", NCURSES_V));
    let dir = golden_dir();
    let docs = [("cdx.json", &s.cdx), ("spdx-2.3.json", &s.spdx23), ("spdx-3.json", &s.spdx3)];
    if std::env::var_os(GOLDEN_ENV).is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, doc) in docs {
            let text = serde_json::to_string_pretty(&masked(doc, s.dir.path())).unwrap() + "\n";
            std::fs::write(dir.join(name), text).unwrap();
        }
        return;
    }
    for (name, doc) in docs {
        let want: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap();
        let got = masked(doc, s.dir.path());
        assert!(got == want, "B-1 {name} differs from main's golden; {}", masked_difference(&want, &got).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------
// T004: census positive controls (C-1, C-2)
// ---------------------------------------------------------------------

/// The `waybill::version_census` `hit` events of a scan. The needles are raw
/// JSON (`"library":"ncurses"`), so a Debug-escaped field (`\"library\"`)
/// cannot satisfy them. The field is matched with its leading space: the
/// same target also carries specs 1644 and 1645's events, and spec 1645's
/// `curated_version_change` names its library too.
fn census_events(s: &Scan) -> Vec<&str> {
    s.stderr.lines().filter(|l| l.contains("waybill::version_census") && l.contains(" hit=")).collect()
}

#[test]
fn c1_census_reports_a_non_identifier_byte() {
    let s = scan_with_log(
        lib_dir(&rodata(Some(b'-'), NCURSES), "libwaybill-fixture-ncurses.so.6"),
        "waybill::version_census=debug",
    );
    let hits: Vec<&str> = census_events(&s)
        .into_iter()
        .filter(|l| l.contains(r#""library":"ncurses""#))
        .collect();
    assert_eq!(hits.len(), 1, "census events: {hits:?}\nlog:\n{}", s.stderr);
    for needle in [
        r#""preceding":"other_non_identifier""#,
        r#""accepted_main":false"#,
        r#""accepted_new":true"#,
    ] {
        assert!(hits[0].contains(needle), "{needle} missing from {}", hits[0]);
    }
}

#[test]
fn c2_census_reports_whitespace_separately() {
    let s = scan_with_log(
        lib_dir(&rodata(Some(b' '), NCURSES), "libwaybill-fixture-ncurses.so.6"),
        "waybill::version_census=debug",
    );
    let hits: Vec<&str> = census_events(&s)
        .into_iter()
        .filter(|l| l.contains(r#""library":"ncurses""#))
        .collect();
    assert_eq!(hits.len(), 1, "census events: {hits:?}\nlog:\n{}", s.stderr);
    for needle in [r#""preceding":"whitespace""#, r#""accepted_new":true"#] {
        assert!(hits[0].contains(needle), "{needle} missing from {}", hits[0]);
    }
}

// ---------------------------------------------------------------------
// User Story 1: anchored patterns behind a non-identifier byte (T006)
// ---------------------------------------------------------------------

const Z: &str = "libwaybill-fixture-z.so.1";
const N: &str = "libwaybill-fixture-ncurses.so.6";
const E: &str = "libwaybill-fixture-expat.so.1";

#[test]
fn z1_zlib_behind_nul() {
    assert!(has(&scan_lib(Some(0), ZLIB, Z), "zlib", ZLIB_V));
}

#[test]
fn z2_zlib_behind_dash() {
    let s = scan_lib(Some(b'-'), ZLIB, Z);
    assert!(has(&s, "zlib", ZLIB_V), "zlib: {:?}", curated(&s.cdx, "zlib"));
}

#[test]
fn z3_zlib_behind_0xff() {
    let s = scan_lib(Some(0xff), ZLIB, Z);
    assert!(has(&s, "zlib", ZLIB_V), "zlib: {:?}", curated(&s.cdx, "zlib"));
}

#[test]
fn z6_zlib_without_its_leading_space_behind_nul() {
    assert!(has(&scan_lib(Some(0), ZLIB_NO_SPACE, Z), "zlib", ZLIB_V));
}

#[test]
fn n1_ncurses_behind_nul() {
    assert!(has(&scan_lib(Some(0), NCURSES, N), "ncurses", NCURSES_V));
}

#[test]
fn n2_ncurses_behind_dash() {
    let s = scan_lib(Some(b'-'), NCURSES, N);
    assert!(has(&s, "ncurses", NCURSES_V), "ncurses: {:?}", curated(&s.cdx, "ncurses"));
}

#[test]
fn n3_ncurses_behind_0xff() {
    let s = scan_lib(Some(0xff), NCURSES, N);
    assert!(has(&s, "ncurses", NCURSES_V), "ncurses: {:?}", curated(&s.cdx, "ncurses"));
}

#[test]
fn e1_expat_behind_nul() {
    assert!(has(&scan_lib(Some(0), EXPAT, E), "expat", EXPAT_V));
}

#[test]
fn e2_expat_behind_dash_and_0xff() {
    for p in [b'-', 0xff] {
        let s = scan_lib(Some(p), EXPAT, E);
        assert!(has(&s, "expat", EXPAT_V), "byte {p:#04x}: expat {:?}", curated(&s.cdx, "expat"));
    }
}

// ---------------------------------------------------------------------
// User Story 2: a name inside other text is not a match (T009)
// ---------------------------------------------------------------------

#[test]
fn z4_zlib_behind_a_letter_is_not_versioned() {
    let s = scan_lib(Some(b'x'), ZLIB, Z);
    assert!(!has(&s, "zlib", ZLIB_V), "zlib: {:?}", curated(&s.cdx, "zlib"));
}

#[test]
fn z5_zlib_behind_a_digit_or_underscore_is_not_versioned() {
    for &p in b"9_" {
        let s = scan_lib(Some(p), ZLIB, Z);
        assert!(!has(&s, "zlib", ZLIB_V), "byte {p:#04x}: zlib {:?}", curated(&s.cdx, "zlib"));
    }
}

#[test]
fn z7_zlib_without_its_leading_space_keeps_the_nul_rule() {
    let s = scan_lib(Some(b'-'), ZLIB_NO_SPACE, Z);
    assert!(!has(&s, "zlib", ZLIB_V), "zlib: {:?}", curated(&s.cdx, "zlib"));
}

#[test]
fn n4_ncurses_behind_an_identifier_character_is_not_found() {
    for &p in b"l9_" {
        let s = scan_lib(Some(p), NCURSES, N);
        assert!(curated(&s.cdx, "ncurses").is_empty(), "byte {p:#04x}: ncurses {:?}", curated(&s.cdx, "ncurses"));
    }
}

#[test]
fn e3_expat_behind_an_identifier_character_is_not_found() {
    for &p in b"b_" {
        let s = scan_lib(Some(p), EXPAT, E);
        assert!(curated(&s.cdx, "expat").is_empty(), "byte {p:#04x}: expat {:?}", curated(&s.cdx, "expat"));
    }
}

#[test]
fn u1_openssl_behind_dash_is_not_found() {
    let s = scan_lib(Some(b'-'), OPENSSL, "libwaybill-fixture-ssl.so.3");
    assert!(!has(&s, "openssl", OPENSSL_V), "openssl: {:?}", curated(&s.cdx, "openssl"));
}

#[test]
fn u2_openssl_behind_nul_is_found() {
    let s = scan_lib(Some(0), OPENSSL, "libwaybill-fixture-ssl.so.3");
    assert!(has(&s, "openssl", OPENSSL_V), "openssl: {:?}", curated(&s.cdx, "openssl"));
}
