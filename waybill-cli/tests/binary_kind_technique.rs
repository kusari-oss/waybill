//! Spec 1648 (#1668): which binary evidence kinds report the CycloneDX
//! technique `binary-analysis`. `dynamic-linkage` and `embedded-source-path`
//! do; the manifests a toolchain embeds (cargo-auditable, ELF package notes,
//! Go BuildInfo) keep `manifest-analysis`.
//!
//! Contract `specs/1648-binary-kind-technique/contracts/wire.md`, cases
//! K1–K7. Each case writes synthetic `waybill-fixture-*` inputs, scans them
//! in all three formats, and checks the technique, `waybill sbom
//! parity-check`, unique `bom-ref`s and the census.
//!
//! `WAYBILL_BINARY_KIND_MAIN_BIN` runs the byte-identity control against
//! another binary, such as `main`'s (`measurements/control.md`).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::io::Write as _;
use std::process::Command;

use common::installed_lib::{elf_shared_lib, macho_dylib};
use serde_json::Value;

// ---------------------------------------------------------------------
// Harness (T003)
// ---------------------------------------------------------------------

/// The scanned directory has a fixed name, so the document's root does not
/// vary between runs.
const ROOT: &str = "waybill-fixture-root";

struct Scan {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    stderr: String,
    _dir: tempfile::TempDir,
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

/// The three documents' bytes and the log of one scan by `bin`.
fn scan_with(bin: &str, dir: &tempfile::TempDir) -> ([Vec<u8>; 3], String) {
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
    common::parity::assert_parity(out_dir.path());
    unique_bom_refs(&serde_json::from_slice(&raw[0]).unwrap());
    (raw, String::from_utf8_lossy(&out.stderr).into_owned())
}

fn scan(dir: tempfile::TempDir) -> Scan {
    let (raw, stderr) = scan_with(common::bin(), &dir);
    let parse = |b: &[u8]| serde_json::from_slice(b).unwrap();
    Scan { cdx: parse(&raw[0]), spdx23: parse(&raw[1]), spdx3: parse(&raw[2]), stderr, _dir: dir }
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

fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

/// The CycloneDX components whose `waybill:evidence-kind` is `kind`.
fn of_kind<'a>(s: &'a Scan, kind: &str) -> Vec<&'a Value> {
    cdx_components(&s.cdx).into_iter().filter(|c| prop(c, "waybill:evidence-kind") == Some(kind)).collect()
}

/// A CycloneDX component's identity-method techniques, in order.
fn techniques(c: &Value) -> Vec<String> {
    c["evidence"]["identity"][0]["methods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["technique"].as_str().map(str::to_string))
        .collect()
}

/// The SPDX 2.3 D1 `evidence.identity` payload of the package with `purl`
/// (qualifiers ignored). Copied from `tests/merged_binary_evidence.rs`.
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

/// The SPDX 3 D1 payload of the package with `purl`. Copied from
/// `tests/merged_binary_evidence.rs`.
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

/// Every census event named `event`, parsed. Its JSON is the rest of the line.
fn census(s: &Scan, event: &str) -> Vec<Value> {
    let needle = format!(" {event}=");
    s.stderr
        .lines()
        .filter(|l| l.contains("waybill::version_census"))
        .filter_map(|l| l.split_once(needle.as_str()).map(|(_, j)| serde_json::from_str(j.trim()).unwrap()))
        .collect()
}

// ---------------------------------------------------------------------
// Builders. Copied where the original is private to another test file.
// ---------------------------------------------------------------------

/// An ELF64 with one extra section `name`. Copied from
/// `tests/python_owned_binaries.rs::elf_with_section`.
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

/// An ELF whose `.note.package` (systemd's package-note format) names an rpm
/// package. Copied from `tests/python_owned_binaries.rs`.
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

/// An ELF64 with a `.dep-v0` section, as cargo-auditable writes. The first
/// crate is the root. Copied from `tests/python_owned_binaries.rs`.
fn cargo_auditable_elf(crates: &[(&str, &str)]) -> Vec<u8> {
    let packages: Vec<Value> = crates
        .iter()
        .enumerate()
        .map(|(i, (name, version))| {
            let mut p = serde_json::json!({ "name": name, "version": version, "source": "crates.io", "kind": "runtime" });
            if i == 0 {
                p["root"] = Value::Bool(true);
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

fn uvarint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
    out
}

/// A file the Go BuildInfo reader accepts. Copied from
/// `tests/closure_binary_contents.rs::go_binary`.
fn go_binary(main: &str, deps: &[(&str, &str)]) -> Vec<u8> {
    let mut mod_info = format!("path\t{main}/cmd\nmod\t{main}\t(devel)\t\n");
    for (name, version) in deps {
        mod_info.push_str(&format!("dep\t{name}\t{version}\th1:fixture=\n"));
    }
    let mut v = vec![0u8; 2048];
    v[..4].copy_from_slice(b"\x7fELF");
    v.extend_from_slice(b"\xff Go buildinf:");
    v.push(8);
    v.push(0x2);
    v.extend_from_slice(&[0u8; 16]);
    let build = b"go1.22.1";
    v.extend(uvarint(build.len() as u64));
    v.extend_from_slice(build);
    v.extend(uvarint(mod_info.len() as u64));
    v.extend_from_slice(mod_info.as_bytes());
    v
}

/// An ELF shared library whose `.rodata` holds `paths`, NUL-separated: the
/// shape `tests/installed_lib_embedded_paths.rs::binary` uses.
fn elf_with_paths(name: &str, paths: &[&str]) -> Vec<u8> {
    let mut rodata = format!("\0{name}\0").into_bytes();
    for p in paths {
        rodata.extend_from_slice(p.as_bytes());
        rodata.push(0);
    }
    elf_shared_lib(&rodata)
}

// ---------------------------------------------------------------------
// The inputs of K1–K7 (contracts/wire.md)
// ---------------------------------------------------------------------

fn case(id: &str) -> tempfile::TempDir {
    match id {
        // A Mach-O dylib that loads another: waybill's ELF imports name a
        // library only through symbol versioning, so the import is built
        // where `object` reports it from a load command (research, as built).
        "K1" => tree(&[(
            "lib/libwaybill-fixture-app.dylib",
            macho_dylib(&["/usr/lib/libwaybill-fixture-dep.1.dylib"], b"waybill-fixture\0"),
        )]),
        "K2" => tree(&[(
            "out/lib/libwaybill-fixture-bundle.so",
            elf_with_paths("libwaybill-fixture-bundle.so", &["/w/app/third_party/waybill-fixture-fmt/src/format.cc"]),
        )]),
        "K3" => tree(&[(
            "bin/waybill-fixture-rs",
            cargo_auditable_elf(&[("waybill-fixture-rs", "0.1.0"), ("waybill-fixture-dep", "1.2.3")]),
        )]),
        "K4" => tree(&[("bin/waybill-fixture-noted", elf_with_package_note("waybill-fixture-noted", "2.0.1"))]),
        "K5" => tree(&[(
            "bin/waybill-fixture-gobin",
            go_binary("example.com/waybill-fixture-go", &[("example.com/waybill-fixture-godep", "v1.4.0")]),
        )]),
        "K6b" => tree(&[
            (
                "lib/libwaybill-fixture-app.dylib",
                macho_dylib(&["/usr/lib/libwaybill-fixture-dep.1.dylib"], b"waybill-fixture\0"),
            ),
            (
                "lib/libwaybill-fixture-app2.dylib",
                macho_dylib(&["/usr/lib/libwaybill-fixture-dep.1.dylib"], b"waybill-fixture-2\0"),
            ),
        ]),
        "K7" => tree(&[(
            "lib/libwaybill-fixture-z.so.1",
            elf_shared_lib(b"waybill-fixture\0 deflate 1.3.1 Copyright 1995-2024 Jean-loup Gailly and Mark Adler \0"),
        )]),
        other => panic!("no case {other}"),
    }
}

// ---------------------------------------------------------------------
// T008: K1–K7
// ---------------------------------------------------------------------

/// The one component of `kind` in a single-input case, with its PURL
/// (qualifiers dropped).
fn the_one<'a>(s: &'a Scan, kind: &str) -> (&'a Value, String) {
    let v = of_kind(s, kind);
    assert_eq!(v.len(), 1, "{kind}: {v:#?}");
    let purl = v[0]["purl"].as_str().unwrap();
    (v[0], purl.split('?').next().unwrap_or(purl).to_string())
}

/// `technique` in all three formats, as the component's only method.
fn assert_technique(s: &Scan, c: &Value, purl: &str, technique: &str, spdx: &str) {
    assert_eq!(techniques(c), vec![technique.to_string()], "{purl}");
    assert_eq!(spdx23_d1(s, purl)["technique"], spdx, "{purl} SPDX 2.3");
    assert_eq!(spdx3_d1(s, purl)["technique"], spdx, "{purl} SPDX 3");
    assert!(spdx23_d1(s, purl).get("methods").is_none(), "{purl}: one method, no methods array");
}

/// K1: a library named by another binary's imports reports binary analysis.
#[test]
fn k1_dynamic_linkage_is_binary_analysis() {
    let s = scan(case("K1"));
    let (c, purl) = the_one(&s, "dynamic-linkage");
    assert_technique(&s, c, &purl, "binary-analysis", "binary-analysis");
}

/// K2: a package named by embedded source paths reports binary analysis.
#[test]
fn k2_embedded_source_path_is_binary_analysis() {
    let s = scan(case("K2"));
    let (c, purl) = the_one(&s, "embedded-source-path");
    assert_technique(&s, c, &purl, "binary-analysis", "binary-analysis");
}

/// K3: cargo-auditable's embedded manifest keeps manifest analysis.
#[test]
fn k3_cargo_auditable_keeps_manifest_analysis() {
    let s = scan(case("K3"));
    let cs = of_kind(&s, "cargo-auditable");
    assert_eq!(cs.len(), 2, "{cs:#?}");
    for c in cs {
        let purl = c["purl"].as_str().unwrap().split('?').next().unwrap().to_string();
        assert_technique(&s, c, &purl, "manifest-analysis", "package-database");
    }
    assert!(census(&s, "kind_technique_change").is_empty(), "{}", s.stderr);
}

/// K4: an ELF package note keeps manifest analysis.
#[test]
fn k4_elf_package_note_keeps_manifest_analysis() {
    let s = scan(case("K4"));
    let (c, purl) = the_one(&s, "elf-note-package");
    assert_technique(&s, c, &purl, "manifest-analysis", "package-database");
    assert!(census(&s, "kind_technique_change").is_empty(), "{}", s.stderr);
}

/// K5: Go BuildInfo keeps manifest analysis; the census counts its
/// components where the Go binary reader emits them (FR-005).
#[test]
fn k5_go_buildinfo_keeps_manifest_analysis() {
    let s = scan(case("K5"));
    let go: Vec<&Value> = cdx_components(&s.cdx)
        .into_iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:golang/")))
        .collect();
    assert_eq!(go.len(), 2, "stdlib and the dep: {go:#?}");
    for c in &go {
        assert_eq!(techniques(c), vec!["manifest-analysis".to_string()]);
    }
    assert!(census(&s, "kind_technique_change").is_empty(), "{}", s.stderr);
    let mut gb: Vec<String> = census(&s, "binary_derived_kind")
        .into_iter()
        .filter(|e| e["evidence_kind"] == "go-buildinfo")
        .map(|e| e["purl"].as_str().unwrap().to_string())
        .collect();
    gb.sort();
    // The reader also creates the binary's `(devel)` main module, which a
    // later step leaves out of the document: the census counts at creation.
    assert_eq!(
        gb,
        vec![
            "pkg:golang/example.com/waybill-fixture-go@(devel)",
            "pkg:golang/example.com/waybill-fixture-godep@v1.4.0",
            "pkg:golang/stdlib@v1.22.1"
        ],
        "{}",
        s.stderr
    );
    assert!(census(&s, "binary_derived_kind").iter().all(|e| e["evidence_kind"] != "detected-go"
        || e["purl"].as_str().is_some_and(|p| !p.starts_with("pkg:golang/"))), "{}", s.stderr);
}

/// K6 (integration half): one library imported by two binaries becomes one
/// component, which lists one binary-analysis method, not two. The linkage
/// aggregator combines the two imports before the deduplicator runs, so this
/// is not a deduplicator merge (`log_only_census_reports_what_will_change`);
/// the merge rule itself is a deduplicator unit test (T014).
#[test]
fn k6_deduped_dynamic_linkage_lists_one_method() {
    let s = scan(case("K6b"));
    let (c, purl) = the_one(&s, "dynamic-linkage");
    assert_technique(&s, c, &purl, "binary-analysis", "binary-analysis");
}

/// K7: spec 1646's banner match is unchanged.
#[test]
fn k7_banner_match_unchanged() {
    let s = scan(case("K7"));
    let (c, purl) = the_one(&s, "embedded-version-string");
    assert_technique(&s, c, &purl, "binary-analysis", "binary-analysis");
}

/// The census at the wired commit (T012): the log-only events are gone, and
/// the decided kinds carry their technique in the census as in the document.
#[test]
fn wired_census_has_no_log_only_events() {
    for id in ["K1", "K2", "K6b"] {
        let s = scan(case(id));
        assert!(census(&s, "kind_technique_change").is_empty(), "{id}: {}", s.stderr);
        assert!(census(&s, "kind_merge").is_empty(), "{id}: {}", s.stderr);
        let ev = census(&s, "binary_derived_kind");
        assert!(!ev.is_empty() && ev.iter().all(|e| e["technique"] == "binary-analysis"), "{id}: {ev:#?}");
    }
}

/// Control (T008, T013): this binary and another
/// (`WAYBILL_BINARY_KIND_MAIN_BIN`, `main`'s) give identical documents,
/// random UUIDs masked, for each case in `WAYBILL_BINARY_KIND_IDENTICAL`
/// (default: the cases whose output never changes).
#[test]
#[ignore = "control: needs WAYBILL_BINARY_KIND_MAIN_BIN"]
fn control_byte_identical_to_main() {
    let other = std::env::var("WAYBILL_BINARY_KIND_MAIN_BIN").unwrap();
    let ids = std::env::var("WAYBILL_BINARY_KIND_IDENTICAL").unwrap_or_else(|_| "K3,K4,K5,K7".into());
    let re = regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}").unwrap();
    let masked = |d: &[u8]| re.replace_all(&String::from_utf8_lossy(d), "UUID").into_owned();
    for id in ids.split(',') {
        let dir = case(id);
        let (ours, _) = scan_with(common::bin(), &dir);
        let (theirs, _) = scan_with(&other, &dir);
        for (i, f) in ["cdx", "spdx-2.3", "spdx-3"].iter().enumerate() {
            assert!(masked(&ours[i]) == masked(&theirs[i]), "{id}: {f} differs from {other}");
        }
    }
}
