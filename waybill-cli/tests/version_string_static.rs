//! Spec 1647 (#1664): a statically linked liblzma is versioned from the
//! version it embeds when its anchor symbol is defined in the ELF static
//! symbol table (`.symtab`), not only in `.dynsym`.
//!
//! Contract `specs/1647-static-liblzma-anchor/contracts/wire.md`, cases
//! S1–S8. Each case writes one synthetic ELF file into a fixed-name
//! directory, scans it in all three formats, and checks the curated
//! component, `waybill sbom parity-check`, unique `bom-ref`s and the
//! `liblzma_static` census event. Every path is a `waybill-fixture-*` name.
//!
//! `WAYBILL_VERSION_STATIC_BIN` runs a case against another binary, such
//! as `main`'s, for the controls (`measurements/control.md`).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::process::Command;

use common::installed_lib::{elf_with_symtab, elf_with_tables};
use serde_json::Value;

const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const XZ_MAGIC: &[u8] = b"\xFD7zXZ\x00";

// ---------------------------------------------------------------------
// Harness (T003)
// ---------------------------------------------------------------------

struct Scan {
    cdx: Value,
    stderr: String,
    _dir: tempfile::TempDir,
}

fn binary() -> String {
    std::env::var("WAYBILL_VERSION_STATIC_BIN").unwrap_or_else(|_| common::bin().to_string())
}

/// The scanned directory has a fixed name, so the document's root does not
/// vary between runs.
const ROOT: &str = "waybill-fixture-root";

/// `.rodata` bytes: a leading string, then `parts` joined.
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

fn scan_with(bin: &str, dir: &tempfile::TempDir) -> (Value, [Vec<u8>; 3], String) {
    let out_dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let files = [
        out_dir.path().join("waybill.cdx.json"),
        out_dir.path().join("waybill.spdx.json"),
        out_dir.path().join("waybill.spdx3.json"),
    ];
    let out = Command::new(bin)
        .env("HOME", home.path())
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "waybill::version_census=debug")
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
    let raw = files.clone().map(|f| std::fs::read(f).unwrap());
    let cdx: Value = serde_json::from_slice(&raw[0]).unwrap();
    common::parity::assert_parity(out_dir.path());
    unique_bom_refs(&cdx);
    (cdx, raw, String::from_utf8_lossy(&out.stderr).into_owned())
}

fn scan(dir: tempfile::TempDir) -> Scan {
    let (cdx, _, stderr) = scan_with(&binary(), &dir);
    Scan { cdx, stderr, _dir: dir }
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

/// Every `liblzma_static` census event, parsed. The field is the last on
/// its line, so the JSON is the rest of the line.
fn static_events(s: &Scan) -> Vec<Value> {
    s.stderr
        .lines()
        .filter(|l| l.contains("waybill::version_census"))
        .filter_map(|l| l.split_once(" liblzma_static=").map(|(_, j)| serde_json::from_str(j.trim()).unwrap()))
        .collect()
}

/// The one `liblzma_static` event of a single-file case.
fn one_event(s: &Scan) -> Value {
    let v = static_events(s);
    assert_eq!(v.len(), 1, "events: {v:#?}\n{}", s.stderr);
    v.into_iter().next().unwrap()
}

/// `library`'s entry in a rule's verdict list (`current`, `rule_a`, `rule_b`).
fn entry<'a>(event: &'a Value, rule: &str, library: &str) -> Option<&'a Value> {
    event[rule].as_array()?.iter().find(|e| e["library"] == library)
}

// ---------------------------------------------------------------------
// T002: the helper writes what it claims
// ---------------------------------------------------------------------

/// Without this, a helper that wrote every symbol as undefined, or wrote a
/// `.dynsym` too, would make the cases pass for the wrong reason.
#[test]
fn helper_writes_symtab_only_with_definedness() {
    use object::{Object, ObjectSymbol};
    let bytes = elf_with_symtab(ET_EXEC, b"x\0", &["lzma_version_string"], &["lzma_code"]);
    let file = object::File::parse(&*bytes).unwrap();
    let syms: Vec<(String, bool)> =
        file.symbols().map(|s| (s.name().unwrap().to_string(), s.is_undefined())).collect();
    assert_eq!(syms, vec![("lzma_version_string".to_string(), false), ("lzma_code".to_string(), true)]);
    assert_eq!(file.dynamic_symbols().count(), 0);
    assert_eq!(file.kind(), object::ObjectKind::Executable);

    let both = elf_with_tables(ET_DYN, b"x\0", Some((&["lzma_version_string"], &[])), (&["lzma_version_string"], &[]));
    let file = object::File::parse(&*both).unwrap();
    assert_eq!(file.symbols().filter(|s| !s.is_undefined()).count(), 1);
    assert_eq!(file.dynamic_symbols().filter(|s| !s.is_undefined()).count(), 1);
}

// ---------------------------------------------------------------------
// T007: S1–S8 (contracts/wire.md)
// ---------------------------------------------------------------------

const FILE: &str = "bin/waybill-fixture-xzstatic";
const GMP_ANCHOR: &[u8] = b"GNU MP: Cannot allocate memory (size=%lu)\n\0";

/// The tree for one case of contracts/wire.md.
fn case(id: &str) -> tempfile::TempDir {
    let one = region(&[b"5.4.5\0"]);
    let bytes = match id {
        "S1" => elf_with_symtab(ET_EXEC, &one, &["lzma_version_string"], &[]),
        "S2" => elf_with_symtab(ET_EXEC, &one, &["lzma_version_number"], &[]),
        "S3" => elf_with_symtab(ET_EXEC, &one, &[], &["lzma_version_string"]),
        "S4" => elf_with_symtab(ET_EXEC, &region(&[b"5.4.5\0", b"5.2.5\0"]), &["lzma_version_string"], &[]),
        "S5" => elf_with_symtab(ET_EXEC, &region(&[GMP_ANCHOR, b"5.4.5\0"]), &["lzma_version_string"], &[]),
        "S6" => elf_with_tables(ET_DYN, &one, Some((&["lzma_version_string"], &[])), (&["lzma_version_string"], &[])),
        "S7" => elf_with_symtab(ET_EXEC, &one, &["waybill_fixture_main"], &[]),
        "S8" => elf_with_symtab(ET_EXEC, &region(&[XZ_MAGIC, b"5.4.5\0"]), &[], &[]),
        other => panic!("no case {other}"),
    };
    tree(&[(FILE, bytes)])
}

/// The case's census event shows liblzma versioned at 5.4.5 by rule A, which
/// is now production's verdict (`current`).
fn assert_rule_a_versions(s: &Scan) {
    let e = one_event(s);
    assert_eq!(e["rule_a"], e["current"], "{e:#}");
    let a = entry(&e, "rule_a", "liblzma").unwrap_or_else(|| panic!("no rule_a liblzma: {e:#}"));
    assert_eq!((a["verdict"].as_str(), a["version"].as_str()), (Some("versioned"), Some("5.4.5")), "{e:#}");
    assert!(e["anchors"].as_array().unwrap().iter().any(|a| a["table"] == "symtab"), "{e:#}");
    assert_eq!(e["elf_type"], "exec");
}

/// S1: `lzma_version_string` defined in `.symtab` versions liblzma.
#[test]
fn s1_symtab_lzma_version_string_versions_liblzma() {
    let s = scan(case("S1"));
    assert_rule_a_versions(&s);
    assert_eq!(curated(&s, "liblzma"), vec!["pkg:generic/liblzma@5.4.5"]);
}

/// S2: the same for `lzma_version_number`.
#[test]
fn s2_symtab_lzma_version_number_versions_liblzma() {
    let s = scan(case("S2"));
    assert_rule_a_versions(&s);
    assert_eq!(curated(&s, "liblzma"), vec!["pkg:generic/liblzma@5.4.5"]);
}

/// S3: an importer is never anchored, from either table.
#[test]
fn s3_undefined_symtab_symbol_never_anchors() {
    let s = scan(case("S3"));
    assert!(curated(&s, "liblzma").is_empty());
    assert!(static_events(&s).is_empty(), "{}", s.stderr);
}

/// T012: a dynamically linked program that imports liblzma lists the
/// symbol as undefined in `.dynsym` and in `.symtab`. Neither table anchors.
#[test]
fn importer_undefined_in_both_tables_never_anchors() {
    let bytes = elf_with_tables(
        ET_DYN,
        &region(&[b"5.4.5\0"]),
        Some((&[], &["lzma_version_string"])),
        (&[], &["lzma_version_string", "lzma_version_number"]),
    );
    let s = scan(tree(&[(FILE, bytes)]));
    assert!(curated(&s, "liblzma").is_empty());
    assert!(static_events(&s).is_empty(), "{}", s.stderr);
    assert!(!s.stderr.contains(" anchored="), "{}", s.stderr);
}

/// S4: two candidates, so no version (spec 1644 FR-005).
#[test]
fn s4_two_candidates_are_ambiguous() {
    let s = scan(case("S4"));
    assert!(curated(&s, "liblzma").is_empty());
    let e = one_event(&s);
    assert_eq!(entry(&e, "rule_a", "liblzma").unwrap()["verdict"], "ambiguous", "{e:#}");
}

/// S5: on `main` GMP alone is anchored and takes `5.4.5`; with the static
/// anchor both libraries accept it, so neither keeps it.
#[test]
fn s5_shared_candidate_versions_neither() {
    let s = scan(case("S5"));
    let e = one_event(&s);
    assert_eq!(entry(&e, "current", "gmp").unwrap()["verdict"], "ambiguous-shared", "{e:#}");
    assert_eq!(entry(&e, "current", "liblzma").unwrap()["verdict"], "ambiguous-shared", "{e:#}");
    assert!(curated(&s, "liblzma").is_empty());
    assert!(curated(&s, "gmp").is_empty(), "gmp kept a shared version");
}

/// S6: a shared liblzma with the symbol in both tables: as on `main`.
#[test]
fn s6_both_tables_as_main() {
    let s = scan(case("S6"));
    assert_eq!(curated(&s, "liblzma"), vec!["pkg:generic/liblzma@5.4.5"]);
    assert!(static_events(&s).is_empty(), "{}", s.stderr);
}

/// S7: a `.symtab` with only unrelated names: nothing, and no event.
#[test]
fn s7_unrelated_symtab_changes_nothing() {
    let s = scan(case("S7"));
    assert!(curated(&s, "liblzma").is_empty());
    assert!(static_events(&s).is_empty(), "{}", s.stderr);
}

/// S8: a stripped program with the xz header magic: no component and no
/// event. FR-004 was rejected: 7-Zip carries the magic with one bare
/// `13.2.0` (`measurements/census.md`).
#[test]
fn s8_magic_alone_gives_no_component() {
    let s = scan(case("S8"));
    assert!(curated(&s, "liblzma").is_empty());
    assert!(static_events(&s).is_empty(), "{}", s.stderr);
}

/// The documents with every random (version 4) UUID masked: the CycloneDX
/// serial number and SPDX namespaces are drawn fresh on every run, so no two
/// runs are byte-identical without this.
fn masked(doc: &[u8]) -> String {
    let re = regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}").unwrap();
    re.replace_all(&String::from_utf8_lossy(doc), "UUID").into_owned()
}

/// Control (T007): this binary and another (`WAYBILL_VERSION_STATIC_MAIN_BIN`,
/// `main`'s) give byte-identical documents, random UUIDs masked, for each case listed in
/// `WAYBILL_VERSION_STATIC_IDENTICAL` (default: the cases whose verdict never
/// changes). Ignored unless the other binary is given.
#[test]
#[ignore = "control: needs WAYBILL_VERSION_STATIC_MAIN_BIN"]
fn control_byte_identical_to_main() {
    let other = std::env::var("WAYBILL_VERSION_STATIC_MAIN_BIN").unwrap();
    let ids = std::env::var("WAYBILL_VERSION_STATIC_IDENTICAL").unwrap_or_else(|_| "S3,S4,S6,S7,S8".into());
    for id in ids.split(',') {
        let dir = case(id);
        let (_, ours, _) = scan_with(&binary(), &dir);
        let (_, theirs, _) = scan_with(&other, &dir);
        for (i, f) in ["cdx", "spdx-2.3", "spdx-3"].iter().enumerate() {
            if masked(&ours[i]) != masked(&theirs[i]) {
                if let Ok(out) = std::env::var("WAYBILL_VERSION_STATIC_DIFF_DIR") {
                    std::fs::write(format!("{out}/{id}.{f}.ours.json"), &ours[i]).unwrap();
                    std::fs::write(format!("{out}/{id}.{f}.theirs.json"), &theirs[i]).unwrap();
                }
                panic!("{id}: {f} differs from {other}");
            }
        }
    }
}
