//! Spec 1646 (#1665): a binary match that merges into another reader's
//! component stays visible on it, and a version read from a binary is
//! labelled binary analysis.
//!
//! Contract `specs/1646-merged-binary-evidence/contracts/wire.md`. Each case
//! writes a synthetic tree into a fixed-name directory, scans it in all three
//! formats, and checks:
//! - the CycloneDX `evidence.identity[0].methods`;
//! - the SPDX 2.3 and SPDX 3 D1 `evidence.identity` payloads;
//! - `waybill sbom parity-check`, unique `bom-ref`s, and the CycloneDX 1.6
//!   schema.
//!
//! File and record names are `waybill-fixture-*` except where the library's
//! own banner or name is under test.
//!
//! `WAYBILL_MERGED_EVIDENCE_BIN` runs a case against another binary, such as
//! `main`'s, for the controls (`measurements/control.md`).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::process::Command;

use common::installed_lib::{elf_shared_lib, elf_shared_lib_with_dynsym};
use serde_json::Value;

const NCURSES: &[u8] = b"ncurses 6.6.20251230\0";
const LLVM: &[u8] = b"LLVM version 23.0.0git\0";
const EXPAT: &[u8] = b"expat_2.7.5\0";
const ZLIB: &[u8] = b" deflate 1.3.1 Copyright 1995-2024 Jean-loup Gailly and Mark Adler \0";
/// The bundled zlib fingerprint's symbols (`symbol_fingerprint.rs`, 8 of 10
/// required).
const ZLIB_SYMBOLS: [&str; 10] = [
    "deflate", "inflate", "deflateInit_", "inflateInit_", "deflateEnd", "inflateEnd", "crc32", "adler32", "compress",
    "uncompress",
];

// ---------------------------------------------------------------------
// Harness (T002)
// ---------------------------------------------------------------------

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    stderr: String,
    _dir: tempfile::TempDir,
}

fn binary() -> String {
    std::env::var("WAYBILL_MERGED_EVIDENCE_BIN").unwrap_or_else(|_| common::bin().to_string())
}

const ROOT: &str = "waybill-fixture-root";

fn region(banner: &[u8]) -> Vec<u8> {
    let mut r = b"waybill-fixture-leading\0".to_vec();
    r.extend_from_slice(banner);
    r
}

fn lib(banner: &[u8]) -> Vec<u8> {
    elf_shared_lib(&region(banner))
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

fn pc(version: &str) -> Vec<u8> {
    format!("Version: {version}\n").into_bytes()
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
    let read = |p: &std::path::Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    let (cdx, spdx23, spdx3) = (read(&files[0]), read(&files[1]), read(&files[2]));
    parity(&out_dir);
    unique_bom_refs(&cdx);
    let errors = common::cdx_schema::cdx_validation_errors(&cdx);
    assert!(errors.is_empty(), "CycloneDX 1.6 schema: {errors:#?}");
    Scan { cdx, spdx23, spdx3, stderr: String::from_utf8_lossy(&out.stderr).into_owned(), _dir: dir }
}

fn scan3(dir: tempfile::TempDir) -> Scan {
    scan_with_log(dir, "warn")
}

fn cdx_components(cdx: &Value) -> Vec<&Value> {
    let mut out = vec![&cdx["metadata"]["component"]];
    let mut stack: Vec<&Value> = cdx["components"].as_array().into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c["components"].as_array().into_iter().flatten());
    }
    out
}

/// The one CycloneDX component whose PURL (qualifiers dropped) is `purl`.
fn component<'a>(s: &'a Scan, purl: &str) -> &'a Value {
    let found: Vec<&Value> = cdx_components(&s.cdx)
        .into_iter()
        .filter(|c| c["purl"].as_str().map(|p| p.split('?').next().unwrap_or(p)) == Some(purl))
        .collect();
    assert_eq!(found.len(), 1, "{purl}: {found:#?}");
    found[0]
}

fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

/// `(technique, confidence)` of each CycloneDX identity method.
fn cdx_methods(s: &Scan, purl: &str) -> Vec<(String, f64)> {
    component(s, purl)["evidence"]["identity"][0]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["technique"].as_str().unwrap().to_string(), m["confidence"].as_f64().unwrap()))
        .collect()
}

/// The SPDX 2.3 D1 `evidence.identity` payload of the package with `purl`.
fn spdx23_d1(s: &Scan, purl: &str) -> Value {
    let pkg = s.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["externalRefs"].as_array().into_iter().flatten().any(|r| {
                r["referenceType"] == "purl"
                    && r["referenceLocator"].as_str().map(|l| l.split('?').next().unwrap_or(l)) == Some(purl)
            })
        })
        .unwrap_or_else(|| panic!("no SPDX 2.3 package {purl}"));
    let env: Vec<Value> = pkg["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| serde_json::from_str::<Value>(a["comment"].as_str()?).ok())
        .filter(|e| e["field"] == "evidence.identity")
        .collect();
    assert_eq!(env.len(), 1, "{purl}: {env:#?}");
    env[0]["value"].clone()
}

/// The SPDX 3 D1 `evidence.identity` payload of the package with `purl`.
fn spdx3_d1(s: &Scan, purl: &str) -> Value {
    let graph = s.spdx3["@graph"].as_array().unwrap();
    let id = graph
        .iter()
        .find(|e| {
            e["type"] == "software_Package"
                && e["software_packageUrl"].as_str().map(|l| l.split('?').next().unwrap_or(l)) == Some(purl)
        })
        .and_then(|e| e["spdxId"].as_str())
        .unwrap_or_else(|| panic!("no SPDX 3 package {purl}"));
    let env: Vec<Value> = graph
        .iter()
        .filter(|e| e["type"] == "Annotation" && e["subject"] == id)
        .filter_map(|e| serde_json::from_str::<Value>(e["statement"].as_str()?).ok())
        .filter(|e| e["field"] == "evidence.identity")
        .collect();
    assert_eq!(env.len(), 1, "{purl}: {env:#?}");
    env[0]["value"].clone()
}

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

/// The census events with this field, matched with its leading space.
fn events<'a>(s: &'a Scan, field: &str) -> Vec<&'a str> {
    let needle = format!(" {field}=");
    s.stderr.lines().filter(|l| l.contains("waybill::version_census") && l.contains(&needle)).collect()
}

/// A merged component: CycloneDX lists the record's method then the binary
/// match's; both SPDX formats keep the flat keys and add `methods`.
fn assert_merged(s: &Scan, purl: &str) {
    assert_eq!(
        cdx_methods(s, purl),
        [("manifest-analysis".to_string(), 0.85), ("binary-analysis".to_string(), 0.85)],
        "{purl}"
    );
    let want = serde_json::json!({
        "technique": "package-database",
        "confidence": 0.85,
        "methods": [
            {"technique": "package-database", "confidence": 0.85},
            {"technique": "binary-analysis", "confidence": 0.85}
        ]
    });
    assert_eq!(spdx23_d1(s, purl), want, "{purl} SPDX 2.3");
    assert_eq!(spdx3_d1(s, purl), want, "{purl} SPDX 3");
}

/// An unmerged binary match: one `binary-analysis` method, no `methods`.
fn assert_binary_only(s: &Scan, purl: &str) {
    assert_eq!(cdx_methods(s, purl), [("binary-analysis".to_string(), 0.85)], "{purl}");
    let want = serde_json::json!({"technique": "binary-analysis", "confidence": 0.85});
    assert_eq!(spdx23_d1(s, purl), want, "{purl} SPDX 2.3");
    assert_eq!(spdx3_d1(s, purl), want, "{purl} SPDX 3");
}

// ---------------------------------------------------------------------
// Trees (contracts/wire.md)
// ---------------------------------------------------------------------

fn b1_tree() -> tempfile::TempDir {
    tree(&[("lib/libncursesw.so.6", lib(NCURSES)), ("lib/pkgconfig/ncursesw.pc", pc("6.6.20251230"))])
}

fn zlib_lib() -> Vec<u8> {
    elf_shared_lib_with_dynsym(&region(ZLIB), &ZLIB_SYMBOLS, &[])
}

// ---------------------------------------------------------------------
// Cases that change (red on the log-only commit)
// ---------------------------------------------------------------------

/// B-1: banner plus `.pc`.
#[test]
#[ignore = "spec 1646: red until T011"]
fn b1_banner_and_pc() {
    let s = scan3(b1_tree());
    assert_merged(&s, "pkg:generic/ncurses@6.6.20251230");
}

/// B-2: banner plus CMake package version.
#[test]
#[ignore = "spec 1646: red until T011"]
fn b2_banner_and_cmake() {
    let s = scan3(tree(&[
        ("lib/libLLVM.so", lib(LLVM)),
        ("lib/cmake/llvm/LLVMConfigVersion.cmake", cmake_version("23.0.0git")),
    ]));
    assert_merged(&s, "pkg:generic/llvm@23.0.0git");
}

/// B-3: a banner alone. C4 and C16 are unchanged.
#[test]
#[ignore = "spec 1646: red until T010"]
fn b3_banner_alone() {
    let s = scan3(tree(&[("libwaybill-fixture-expat.so.1", lib(EXPAT))]));
    assert_binary_only(&s, "pkg:generic/expat@2.7.5");
    let c = component(&s, "pkg:generic/expat@2.7.5");
    assert_eq!(prop(c, "waybill:evidence-kind"), Some("embedded-version-string"));
    assert_eq!(prop(c, "waybill:confidence"), Some("heuristic"));
}

/// F-1: a symbol fingerprint alone (identity, no version).
#[test]
#[ignore = "spec 1646: red until T010"]
fn f1_fingerprint_alone() {
    let s = scan3(tree(&[(
        "libwaybill-fixture-z.so.1",
        elf_shared_lib_with_dynsym(b"\0waybill-fixture\0", &ZLIB_SYMBOLS, &[]),
    )]));
    let zlib: Vec<String> = cdx_components(&s.cdx)
        .into_iter()
        .filter(|c| c["name"] == "zlib")
        .filter_map(|c| c["purl"].as_str().map(|p| p.split('?').next().unwrap_or(p).to_string()))
        .collect();
    assert_eq!(zlib.len(), 1, "{zlib:?}");
    assert_binary_only(&s, &zlib[0]);
}

/// F-2: banner and fingerprint on one binary, plus `.pc`.
#[test]
#[ignore = "spec 1646: red until T011"]
fn f2_fingerprint_banner_and_pc() {
    let s = scan3(tree(&[("lib/libz.so.1", zlib_lib()), ("lib/pkgconfig/zlib.pc", pc("1.3.1"))]));
    assert_merged(&s, "pkg:generic/zlib@1.3.1");
    assert!(
        prop(component(&s, "pkg:generic/zlib@1.3.1"), "waybill:fingerprint-symbols-matched").is_some(),
        "the symbol count is kept"
    );
}

// ---------------------------------------------------------------------
// Guards and controls
// ---------------------------------------------------------------------

/// R-1: a record merged into a record has no binary match; nothing changes.
#[test]
fn r1_record_into_record() {
    let s = scan3(tree(&[
        ("lib/libexpat.so.1", elf_shared_lib(b"\0waybill-fixture\0")),
        ("lib/pkgconfig/expat.pc", pc("2.7.5")),
        ("lib/cmake/expat/expat-config-version.cmake", cmake_version("2.7.5")),
    ]));
    let purl = "pkg:generic/expat@2.7.5";
    assert_eq!(cdx_methods(&s, purl), [("manifest-analysis".to_string(), 0.85)]);
    let want = serde_json::json!({"technique": "package-database", "confidence": 0.85});
    assert_eq!(spdx23_d1(&s, purl), want);
    assert_eq!(spdx3_d1(&s, purl), want);
    assert_eq!(prop(component(&s, purl), "waybill:also-detected-via"), Some(r#"["cmake-package-version"]"#));
}

/// C-1: the census fires on the case it counts.
#[test]
fn c1_census_reports_the_merge() {
    let s = scan_with_log(b1_tree(), "waybill::version_census=debug");
    let m = events(&s, "binary_merge");
    assert_eq!(m.len(), 1, "{m:#?}");
    for needle in [
        r#""site":"S1""#,
        r#""mechanism":"pkg-config""#,
        r#""evidence_kind":"embedded-version-string""#,
        r#""absorbed":["binary-analysis"]"#,
    ] {
        assert!(m[0].contains(needle), "{needle} not in {}", m[0]);
    }
    let t = events(&s, "technique_change");
    assert_eq!(t.len(), 1, "{t:#?}");
    assert!(t[0].contains(r#""new":"binary-analysis""#), "{}", t[0]);
}
