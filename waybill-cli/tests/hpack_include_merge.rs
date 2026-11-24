//! Spec 1649 (#1673, #1674): hpack `!include` and YAML merge keys are read
//! as hpack reads them.
//!
//! Contract `specs/1649-hpack-include-merge/contracts/wire.md`, cases M1–M5,
//! I1–I6, U1–U5 and N1. Each case writes a synthetic `waybill-fixture-*`
//! tree, scans it in all three formats, and checks the package's dependency
//! edges, C200 (`waybill:hpack-read-outcome`), `waybill sbom parity-check`
//! and unique `bom-ref`s. Every expected dependency set is what hpack 0.38.3
//! renders for the same tree (spec Background, research R1 and R3).
//!
//! `WAYBILL_HPACK_MAIN_BIN` runs the byte-identity control against another
//! binary, such as `main`'s (`measurements/control.md`).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

// ---------------------------------------------------------------------
// Harness (T002)
// ---------------------------------------------------------------------

/// The scanned directory has a fixed name, so the document's root does not
/// vary between runs.
const ROOT: &str = "waybill-fixture-root";

struct Scan {
    cdx: Value,
    stderr: String,
    _dir: tempfile::TempDir,
}

/// A fresh temp directory whose `ROOT` holds each `(relative path, text)`.
/// A path may climb out of `ROOT` with `../` (U2 puts a file above it).
fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, text) in files {
        let p = dir.path().join(ROOT).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
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
    parity(&out_dir);
    unique_bom_refs(&serde_json::from_slice(&raw[0]).unwrap());
    (raw, String::from_utf8_lossy(&out.stderr).into_owned())
}

fn scan(dir: tempfile::TempDir) -> Scan {
    let (raw, stderr) = scan_with(common::bin(), &dir);
    Scan { cdx: serde_json::from_slice(&raw[0]).unwrap(), stderr, _dir: dir }
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

fn hackage_name(purl: &str) -> Option<&str> {
    let rest = purl.strip_prefix("pkg:hackage/")?;
    Some(rest.split(['@', '?']).next().unwrap_or(rest))
}

/// The `pkg:hackage` names `package`'s component depends on, from the
/// CycloneDX dependency edges.
fn deps_of(s: &Scan, package: &str) -> BTreeSet<String> {
    let comps = cdx_components(&s.cdx);
    let refs: Vec<&str> = comps
        .iter()
        .filter(|c| c["purl"].as_str().and_then(hackage_name) == Some(package))
        .filter_map(|c| c["bom-ref"].as_str())
        .collect();
    assert!(!refs.is_empty(), "no component for {package}: {:#}", s.cdx["components"]);
    let purl_of = |r: &str| -> String {
        comps.iter().find(|c| c["bom-ref"] == r).and_then(|c| c["purl"].as_str()).unwrap_or(r).to_string()
    };
    s.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| refs.iter().any(|r| d["ref"] == *r))
        .flat_map(|d| d["dependsOn"].as_array().into_iter().flatten())
        .filter_map(|t| t.as_str())
        .filter_map(|t| hackage_name(&purl_of(t)).map(str::to_string))
        .collect()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

/// C200, parsed.
fn c200(s: &Scan) -> Value {
    let v = s.cdx["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == "waybill:hpack-read-outcome")
        .and_then(|p| p["value"].as_str())
        .unwrap_or_else(|| panic!("no C200: {}", s.stderr));
    serde_json::from_str(v).unwrap()
}

/// `read`, `partial` or `unread` for the manifest whose path ends in `rel`.
fn outcome(s: &Scan, rel: &str) -> &'static str {
    let c = c200(s);
    let ends = |v: &Value| v.as_str().is_some_and(|p| p.ends_with(rel));
    if c["manifests-partial"].as_array().into_iter().flatten().any(|e| ends(&e["file"])) {
        "partial"
    } else if c["manifests-unread"].as_array().into_iter().flatten().any(|e| ends(&e["file"])) {
        "unread"
    } else if c["manifests-read"].as_array().into_iter().flatten().any(ends) {
        "read"
    } else {
        panic!("{rel} not in C200: {c:#}")
    }
}

/// The C200 include entries of `kind` (`includes-resolved` or
/// `includes-unresolved`), as `(include, reason)`.
fn includes(s: &Scan, kind: &str) -> Vec<(String, String)> {
    c200(s)[kind]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            assert!(e["manifest"].as_str().is_some_and(|m| m.ends_with("package.yaml")), "{e}");
            (e["include"].as_str().unwrap_or("").to_string(), e["reason"].as_str().unwrap_or("").to_string())
        })
        .collect()
}

// ---------------------------------------------------------------------
// The trees (contracts/wire.md)
// ---------------------------------------------------------------------

fn case(id: &str) -> tempfile::TempDir {
    let m = |name: &str, body: &str| format!("name: {name}\nversion: 0.1.0\n{body}");
    match id {
        "M1" => tree(&[("package.yaml", &m("waybill-fixture-m1", "_a: &a\n  dependencies: [base, containers]\nlibrary:\n  <<: *a\n  source-dirs: src\n"))]),
        "M2" => tree(&[("package.yaml", &m("waybill-fixture-m2", "_a: &a\n  dependencies: [base]\n_b: &b\n  ghc-options: -Wall\nlibrary:\n  <<: *a\n  <<: *b\n  source-dirs: src\n"))]),
        "M3" => tree(&[("package.yaml", &m("waybill-fixture-m3", "_a: &a\n  dependencies: [base]\n_b: &b\n  dependencies: [text]\nlibrary:\n  <<: *a\n  <<: *b\n  source-dirs: src\n"))]),
        "M4" => tree(&[
            ("p/package.yaml", &m("waybill-fixture-m4a", "_a: &a\n  dependencies: [base]\nlibrary:\n  <<: *a\n  dependencies: [text]\n  source-dirs: src\n")),
            ("q/package.yaml", &m("waybill-fixture-m4b", "_a: &a\n  dependencies: [base]\nlibrary:\n  dependencies: [text]\n  <<: *a\n  source-dirs: src\n")),
        ]),
        "M5" => tree(&[("package.yaml", &m("waybill-fixture-m5", "_s: &s [base]\nlibrary:\n  <<: *s\n  dependencies: [text]\n  source-dirs: src\n"))]),
        "I1" => tree(&[
            ("package.yaml", &m("waybill-fixture-i1", "_common: !include \"common.yaml\"\nlibrary:\n  dependencies: *deps\n  source-dirs: src\n")),
            ("common.yaml", "deps: &deps [base, containers]\n"),
        ]),
        "I2" => tree(&[
            ("package.yaml", &m("waybill-fixture-i2", "<<: !include \"shared/a.yaml\"\nlibrary:\n  source-dirs: src\n")),
            ("shared/a.yaml", "<<: !include \"b.yaml\"\nghc-options: -Wall\n"),
            ("shared/b.yaml", "dependencies: [text]\n"),
        ]),
        "I3" => tree(&[
            ("package.yaml", &m("waybill-fixture-i3", "_d: &d [base, text]\nlibrary: !include \"lib.yaml\"\n")),
            ("lib.yaml", "dependencies: *d\nsource-dirs: src\n"),
        ]),
        "I4" => tree(&[
            ("package.yaml", &m("waybill-fixture-i4", "_a: &d [base]\n_i: !include \"inc.yaml\"\nlibrary:\n  dependencies: *d\n  source-dirs: src\n")),
            ("inc.yaml", "x: &d [text]\n"),
        ]),
        "I5" => tree(&[
            ("pkg/package.yaml", &m("waybill-fixture-i5", "_common/lib: !include \"../waybill-fixture-common.yaml\"\nlibrary:\n  <<: *lib\n")),
            ("waybill-fixture-common.yaml", "lib: &lib\n  dependencies: [base, bytestring]\n  source-dirs: src\n"),
        ]),
        "I6" => tree(&[
            ("package.yaml", &m("waybill-fixture-i6", "_c: !include shared.yaml\nlibrary:\n  dependencies: *deps\n  source-dirs: src\n")),
            ("shared.yaml", "x: &deps [base]\n"),
        ]),
        "U1" => tree(&[("package.yaml", &m("waybill-fixture-u1", "_c: !include \"missing.yaml\"\nlibrary:\n  dependencies: [base]\n  source-dirs: src\n"))]),
        "U2" => tree(&[
            ("p/package.yaml", &m("waybill-fixture-u2", "<<: !include \"../../outside.yaml\"\nlibrary:\n  dependencies: [base]\n  source-dirs: src\n")),
            ("../outside.yaml", "dependencies: [waybill-fixture-outside]\n"),
        ]),
        "U3" => tree(&[
            ("package.yaml", &m("waybill-fixture-u3", "<<: !include \"a.yaml\"\nlibrary:\n  dependencies: [base]\n  source-dirs: src\n")),
            ("a.yaml", "<<: !include \"b.yaml\"\nghc-options: -Wall\n"),
            ("b.yaml", "<<: !include \"a.yaml\"\nghc-options: -O2\n"),
        ]),
        "U4" => {
            let mut files: Vec<(String, String)> = vec![(
                "package.yaml".into(),
                m("waybill-fixture-u4", "<<: !include \"c1.yaml\"\nlibrary:\n  dependencies: [base]\n  source-dirs: src\n"),
            )];
            for i in 1..=9 {
                files.push((format!("c{i}.yaml"), format!("<<: !include \"c{}.yaml\"\nx{i}: {i}\n", i + 1)));
            }
            files.push(("c10.yaml".into(), "x10: 10\n".into()));
            let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
            tree(&refs)
        }
        "U5" => tree(&[
            ("package.yaml", &m("waybill-fixture-u5", "_c: {x: !include \"shared.yaml\"}\nlibrary:\n  dependencies: [base]\n  source-dirs: src\n")),
            ("shared.yaml", "y: 1\n"),
        ]),
        "L1" => tree(&[("package.yaml", &m("waybill-fixture-l1", "library:\n  source-dirs: src\n  dependencies:\n    - QuickCheck >= 2\n    - quickcheck < 3\n    - Text\n    - text\n"))]),
        "L2" => tree(&[("package.yaml", &m("waybill-fixture-l2", "library:\n  source-dirs: src\n  dependencies:\n    - waybill-fixture-x:sub\n    - waybill-fixture-x:other\n  build-tools:\n    - hspec-discover:hspec-discover\n"))]),
        "L3" => tree(&[("package.yaml", &m("waybill-fixture-l3", "library:\n  source-dirs: src\n  when:\n    condition: true\n    dependencies: [unix]\n  dependencies: [base]\n"))]),
        "N1" => tree(&[("package.yaml", &m("waybill-fixture-n1", "dependencies: [base]\nlibrary:\n  dependencies: [text]\n  source-dirs: src\n"))]),
        other => panic!("no case {other}"),
    }
}

// ---------------------------------------------------------------------
// T008: merge keys (User Story 1)
// ---------------------------------------------------------------------

#[test]
fn m1_single_merge_keeps_merged_dependencies() {
    let s = scan(case("M1"));
    assert_eq!(deps_of(&s, "waybill-fixture-m1"), set(&["base", "containers"]));
    assert_eq!(outcome(&s, "package.yaml"), "read");
}

#[test]
fn m2_two_merge_keys_are_read() {
    let s = scan(case("M2"));
    assert_eq!(outcome(&s, "package.yaml"), "read");
    assert_eq!(deps_of(&s, "waybill-fixture-m2"), set(&["base"]));
}

#[test]
fn m3_first_merge_wins() {
    let s = scan(case("M3"));
    assert_eq!(deps_of(&s, "waybill-fixture-m3"), set(&["base"]));
}

#[test]
fn m4_explicit_beats_merge_in_either_order() {
    let s = scan(case("M4"));
    assert_eq!(deps_of(&s, "waybill-fixture-m4a"), set(&["text"]));
    assert_eq!(deps_of(&s, "waybill-fixture-m4b"), set(&["text"]));
}

/// M5: a merge of a sequence contributes nothing, as in hpack.
#[test]
fn m5_non_mapping_merge_is_ignored() {
    let s = scan(case("M5"));
    assert_eq!(deps_of(&s, "waybill-fixture-m5"), set(&["text"]));
    assert_eq!(outcome(&s, "package.yaml"), "read");
}

// ---------------------------------------------------------------------
// T012: includes (User Story 2)
// ---------------------------------------------------------------------

#[test]
fn i1_anchor_from_an_included_file() {
    let s = scan(case("I1"));
    assert_eq!(deps_of(&s, "waybill-fixture-i1"), set(&["base", "containers"]));
    assert_eq!(outcome(&s, "package.yaml"), "read");
    assert_eq!(includes(&s, "includes-resolved"), vec![("common.yaml".to_string(), String::new())]);
}

#[test]
fn i2_nested_include_resolves_relative_to_the_including_file() {
    let s = scan(case("I2"));
    assert_eq!(deps_of(&s, "waybill-fixture-i2"), set(&["text"]));
    assert_eq!(outcome(&s, "package.yaml"), "read");
}

#[test]
fn i3_included_file_uses_the_includers_anchor() {
    let s = scan(case("I3"));
    assert_eq!(deps_of(&s, "waybill-fixture-i3"), set(&["base", "text"]));
}

#[test]
fn i4_later_anchor_definition_from_an_include_wins() {
    let s = scan(case("I4"));
    assert_eq!(deps_of(&s, "waybill-fixture-i4"), set(&["text"]));
}

/// I5: the corpus's commonest form, a key with a slash and a path one
/// directory up (`_common/lib: !include "../package-common.yaml"`).
#[test]
fn i5_slash_key_and_parent_path() {
    let s = scan(case("I5"));
    assert_eq!(deps_of(&s, "waybill-fixture-i5"), set(&["base", "bytestring"]));
    assert_eq!(outcome(&s, "pkg/package.yaml"), "read");
}

#[test]
fn i6_unquoted_include_path() {
    let s = scan(case("I6"));
    assert_eq!(deps_of(&s, "waybill-fixture-i6"), set(&["base"]));
}

// ---------------------------------------------------------------------
// T015: includes that cannot be resolved (User Story 3)
// ---------------------------------------------------------------------

fn assert_unresolved(id: &str, rel: &str, reason: &str) -> Scan {
    let s = scan(case(id));
    assert_eq!(outcome(&s, rel), "partial", "{id}");
    let un = includes(&s, "includes-unresolved");
    assert!(un.iter().any(|(_, r)| r == reason), "{id}: {un:?}");
    s
}

#[test]
fn u1_missing_include_is_partial() {
    let s = assert_unresolved("U1", "package.yaml", "missing");
    assert_eq!(deps_of(&s, "waybill-fixture-u1"), set(&["base"]));
}

#[test]
fn u2_include_outside_the_root_is_not_read() {
    let s = assert_unresolved("U2", "p/package.yaml", "outside-root");
    assert_eq!(deps_of(&s, "waybill-fixture-u2"), set(&["base"]), "the outside file was read");
}

#[test]
fn u3_include_cycle_terminates_partial() {
    assert_unresolved("U3", "package.yaml", "cycle");
}

#[test]
fn u4_include_chain_too_deep() {
    assert_unresolved("U4", "package.yaml", "too-deep");
}

#[test]
fn u5_include_in_flow_syntax_is_unsupported() {
    assert_unresolved("U5", "package.yaml", "unsupported-position");
}

// ---------------------------------------------------------------------
// T018a: the lowering keeps hpack's names (FR-009, FR-010)
// ---------------------------------------------------------------------

/// L1: names keep their declared spelling, and last-wins is per exact
/// spelling (`measurements/probe_lowering.md`). Asserted on the components:
/// the main module's edges fold a case-differing pair to one name for the
/// `.cabal` reader too, a shared edge-resolution defect filed separately.
#[test]
fn l1_names_keep_their_declared_spelling() {
    let s = scan(case("L1"));
    let names: BTreeSet<String> = cdx_components(&s.cdx)
        .into_iter()
        .filter_map(|c| c["purl"].as_str().and_then(hackage_name))
        .filter(|n| *n != "waybill-fixture-l1")
        .map(str::to_string)
        .collect();
    assert_eq!(names, set(&["QuickCheck", "Text", "quickcheck", "text"]));
}

/// L2: `package:qualifier` names the package, for a library and a build tool
/// alike (#1684).
#[test]
fn l2_qualified_names_name_the_package() {
    let s = scan(case("L2"));
    let names: BTreeSet<String> = cdx_components(&s.cdx)
        .into_iter()
        .filter_map(|c| c["purl"].as_str().and_then(hackage_name))
        .map(str::to_string)
        .collect();
    assert!(names.iter().all(|n| !n.contains(':')), "{names:?}");
    assert!(names.contains("waybill-fixture-x") && names.contains("hspec-discover"), "{names:?}");
}

#[test]
fn l3_when_as_one_mapping() {
    let s = scan(case("L3"));
    assert_eq!(outcome(&s, "package.yaml"), "read");
    assert_eq!(deps_of(&s, "waybill-fixture-l3"), set(&["base", "unix"]));
}

// ---------------------------------------------------------------------
// N1 and the control
// ---------------------------------------------------------------------

/// N1: a manifest with neither construct reads as before.
#[test]
fn n1_plain_manifest_reads() {
    let s = scan(case("N1"));
    assert_eq!(deps_of(&s, "waybill-fixture-n1"), set(&["base", "text"]));
    assert_eq!(outcome(&s, "package.yaml"), "read");
    assert!(c200(&s).get("includes-resolved").is_none() && c200(&s).get("includes-unresolved").is_none());
}

/// Control (T011, T014): this binary and another
/// (`WAYBILL_HPACK_MAIN_BIN`, `main`'s) give identical documents, random
/// UUIDs masked, for each case in `WAYBILL_HPACK_IDENTICAL` (default N1).
#[test]
#[ignore = "control: needs WAYBILL_HPACK_MAIN_BIN"]
fn control_byte_identical_to_main() {
    let other = std::env::var("WAYBILL_HPACK_MAIN_BIN").unwrap();
    let ids = std::env::var("WAYBILL_HPACK_IDENTICAL").unwrap_or_else(|_| "N1".into());
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
