//! Milestone 966 (#966) — GHC boot libraries take the version of the
//! compiler the derivation closure names.
//!
//! Every scan here drives a stub `nix` over fixture closures
//! (`fixtures/nix_closure_boot_versions/`); nothing is evaluated or built.
//!
//! The main closure (`closure.json`) is a project built by `ghc-9.10.3`
//! declaring `base`, `process` and `Win32`, with three confirmed members:
//! `waybill-fixture-lib` and `waybill-fixture-dep` built by `ghc-9.10.3`, and
//! `waybill-fixture-boottool` built by the bootstrap `ghc-binary-9.8.4`. Both
//! of the latter declare `base`, which the two compilers ship at different
//! versions, and `stm`, which they ship at the same version.
//!
//! Boot libraries appear in the fixtures by name only. Every version asserted
//! below is read from the committed table at run time, so no committed
//! fixture carries a boot library's version.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

const C151: &str = "waybill:unresolved-reason";
const C170: &str = "waybill:haskell-version-unresolved-reason";
const C171: &str = "waybill:nixpkgs-candidate-compilers";
const C174: &str = "waybill:nixpkgs-haskell-resolution";
const C203: &str = "waybill:scope-classification-source";
const C218: &str = "waybill:boot-library-compiler";
const C219: &str = "waybill:ghc-boot-libraries";
const TIER: &str = "waybill:sbom-tier";

const DEP: &str = "pkg:hackage/waybill-fixture-dep@1.2.0";
const BOOTTOOL: &str = "pkg:hackage/waybill-fixture-boottool@2.0.0";
const LIB: &str = "pkg:hackage/waybill-fixture-lib@1.0.0";

/// The union rule's input: every candidate configuration nulls these, as the
/// real 9.10+ configurations null `os-string`.
const BOOT_CONFIG: &str = "self: super: {\n  base = null;\n  process = null;\n  Win32 = null;\n  stm = null;\n  os-string = null;\n}\n";

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_boot_versions")
}

fn read(name: &str) -> String {
    let p = dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The committed table, parsed independently of the binary under test.
fn table() -> BTreeMap<(String, String), (String, String)> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv");
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("ghc\t"))
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            ((f[0].to_string(), f[1].to_string()), (f[2].to_string(), f[4].to_string()))
        })
        .collect()
}

fn version(release: &str, lib: &str) -> String {
    table()
        .get(&(release.to_string(), lib.to_string()))
        .unwrap_or_else(|| panic!("the table has no {lib} for {release}"))
        .0
        .clone()
}

fn table_date() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv");
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("# observed:").map(|d| d.trim().to_string()))
        .unwrap()
}

fn scan_with(project: &str, closure: &str, flags: &[&str]) -> Scan {
    let packages = read("hackage-packages.nix");
    NixStub {
        project: dir().join(project),
        closure,
        package_set: Some((&packages, BOOT_CONFIG)),
        ..Default::default()
    }
    .scan(flags)
}

const CLOSURE_FLAGS: &[&str] = &["--nix-closure", "--no-deps-dev", "--no-clearly-defined"];
const NO_CLOSURE_FLAGS: &[&str] = &["--no-deps-dev", "--no-clearly-defined"];

fn main_scan() -> Scan {
    scan_with("project", &read("closure.json"), CLOSURE_FLAGS)
}

/// The same closure built by a compiler the table does not have. The
/// bootstrap tool is moved to it too, so exactly one compiler is named.
fn absent_closure() -> String {
    read("closure.json")
        .replace("aaa-ghc-9.10.3", "aaa-ghc-99.0.1")
        .replace("bbb-ghc-binary-9.8.4", "aaa-ghc-99.0.1")
}

fn closure_966() -> String {
    read("closure-966.json").replace("@PROCESS_9_6_6@", &version("9.6.6", "process"))
}

// ---- document helpers ----

fn components(cdx: &Value) -> Vec<&Value> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
        if let Some(cs) = v["components"].as_array() {
            for c in cs {
                out.push(c);
                walk(c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(cdx, &mut out);
    out
}

fn named<'a>(cdx: &'a Value, name: &str) -> Vec<&'a Value> {
    components(cdx).into_iter().filter(|c| c["name"] == name).collect()
}

fn one<'a>(cdx: &'a Value, purl: &str) -> &'a Value {
    let found: Vec<&Value> = components(cdx).into_iter().filter(|c| c["purl"] == purl).collect();
    assert_eq!(found.len(), 1, "exactly one {purl}; document holds {:?}", purls(cdx));
    found[0]
}

fn purls(cdx: &Value) -> Vec<String> {
    components(cdx).iter().filter_map(|c| c["purl"].as_str()).map(str::to_string).collect()
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

fn depends_on(cdx: &Value, from: &str) -> BTreeSet<String> {
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["ref"] == from)
        .filter_map(|d| d["dependsOn"].as_array())
        .flatten()
        .filter_map(|t| t.as_str())
        .map(str::to_string)
        .collect()
}

fn envelope(raw: &str, field: &str) -> Option<String> {
    let v: Value = serde_json::from_str(raw).ok()?;
    (v.get("field")?.as_str()? == field).then(|| v.get("value")?.as_str().map(str::to_string))?
}

/// A document-scope row in each format, asserted equal across the three.
fn doc_row(s: &Scan, name: &str) -> Option<String> {
    let cdx = s.cdx["metadata"]["properties"]
        .as_array()
        .and_then(|ps| ps.iter().find(|p| p["name"] == name))
        .and_then(|p| p["value"].as_str())
        .map(str::to_string);
    let spdx23 = s.spdx23["annotations"]
        .as_array()
        .and_then(|a| a.iter().filter_map(|x| x["comment"].as_str()).find_map(|r| envelope(r, name)));
    let spdx3 = s.spdx3["@graph"].as_array().and_then(|g| {
        g.iter()
            .filter(|e| e["type"] == "Annotation")
            .filter(|e| {
                // Document scope: the subject is the SpdxDocument element.
                let subject = e["subject"].as_str().unwrap_or_default();
                g.iter().any(|d| d["type"] == "SpdxDocument" && d["spdxId"] == subject)
            })
            .filter_map(|e| e["statement"].as_str())
            .find_map(|r| envelope(r, name))
    });
    assert_eq!(spdx23, cdx, "{name}: SPDX 2.3 vs CycloneDX");
    assert_eq!(spdx3, cdx, "{name}: SPDX 3 vs CycloneDX");
    cdx
}

/// C218 by PURL in each format, asserted equal across the three (SC-006).
fn c218_by_purl(s: &Scan) -> BTreeMap<String, String> {
    let cdx: BTreeMap<String, String> = components(&s.cdx)
        .into_iter()
        .filter_map(|c| Some((c["purl"].as_str()?.to_string(), prop(c, C218)?)))
        .collect();
    let spdx23: BTreeMap<String, String> = s.spdx23["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let purl = p["externalRefs"]
                .as_array()?
                .iter()
                .find(|r| r["referenceType"] == "purl")?["referenceLocator"]
                .as_str()?
                .to_string();
            let v = p["annotations"]
                .as_array()?
                .iter()
                .filter_map(|a| a["comment"].as_str())
                .find_map(|r| envelope(r, C218))?;
            Some((purl, v))
        })
        .collect();
    let graph = s.spdx3["@graph"].as_array().unwrap();
    let purl_of: BTreeMap<&str, &str> = graph
        .iter()
        .filter_map(|e| Some((e["spdxId"].as_str()?, e["software_packageUrl"].as_str()?)))
        .collect();
    let spdx3: BTreeMap<String, String> = graph
        .iter()
        .filter(|e| e["type"] == "Annotation")
        .filter_map(|e| {
            let v = envelope(e["statement"].as_str()?, C218)?;
            let purl = purl_of.get(e["subject"].as_str()?)?;
            Some((purl.to_string(), v))
        })
        .collect();
    assert_eq!(spdx23, cdx, "C218: SPDX 2.3 vs CycloneDX");
    assert_eq!(spdx3, cdx, "C218: SPDX 3 vs CycloneDX");
    cdx
}

fn hackage(name: &str, version: &str) -> String {
    format!("pkg:hackage/{name}@{version}")
}

// ---- US1 ----

#[test]
fn design_tier_base_takes_the_project_compilers_version() {
    let s = main_scan();
    let purl = hackage("base", &version("9.10.3", "base"));
    let base = one(&s.cdx, &purl);
    assert_eq!(base["version"], version("9.10.3", "base"));
    assert_eq!(prop(base, C218).as_deref(), Some("ghc-9.10.3"));
    assert_eq!(prop(base, TIER).as_deref(), Some("design"), "the tier says where it was declared");
    for gone in [C170, C171, C151] {
        assert_eq!(prop(base, gone), None, "{gone} on a versioned boot library: {base}");
    }
    // `process` too: every boot library the compiler ships.
    let process = one(&s.cdx, &hackage("process", &version("9.10.3", "process")));
    assert_eq!(prop(process, C218).as_deref(), Some("ghc-9.10.3"));
    // SC-006: all three formats name the compiler.
    let c218 = c218_by_purl(&s);
    assert_eq!(c218.get(&purl).map(String::as_str), Some("ghc-9.10.3"));
}

#[test]
fn bootstrap_member_edge_points_at_its_own_compilers_base() {
    let s = main_scan();
    let (b10, b8) = (version("9.10.3", "base"), version("9.8.4", "base"));
    assert_ne!(b10, b8, "the fixture needs a library the two compilers ship differently");
    let (p10, p8) = (hackage("base", &b10), hackage("base", &b8));
    assert_eq!(prop(one(&s.cdx, &p10), C218).as_deref(), Some("ghc-9.10.3"));
    assert_eq!(prop(one(&s.cdx, &p8), C218).as_deref(), Some("ghc-binary-9.8.4"));

    let boottool = depends_on(&s.cdx, BOOTTOOL);
    assert!(boottool.contains(&p8), "{BOOTTOOL} -> {boottool:?}");
    assert!(!boottool.contains(&p10), "{BOOTTOOL} -> {boottool:?}");
    let dep = depends_on(&s.cdx, DEP);
    assert!(dep.contains(&p10), "{DEP} -> {dep:?}");
    assert!(!dep.contains(&p8), "{DEP} -> {dep:?}");
    // Nothing but the bootstrap-built member reaches the bootstrap's base.
    let into_p8: Vec<&str> = s.cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["dependsOn"].as_array().is_some_and(|t| t.iter().any(|x| x == &p8)))
        .filter_map(|d| d["ref"].as_str())
        .collect();
    assert_eq!(into_p8, vec![BOOTTOOL]);
}

#[test]
fn a_library_both_compilers_ship_identically_is_one_component() {
    let s = main_scan();
    let v = version("9.10.3", "stm");
    assert_eq!(v, version("9.8.4", "stm"), "the fixture needs a library both ship identically");
    let stm = named(&s.cdx, "stm");
    assert_eq!(stm.len(), 1, "one stm: {stm:?}");
    let purl = hackage("stm", &v);
    assert_eq!(stm[0]["purl"], purl.as_str());
    assert_eq!(prop(stm[0], C218).as_deref(), Some("ghc-9.10.3"), "a shared version names the project compiler");
    assert!(depends_on(&s.cdx, DEP).contains(&purl));
    assert!(depends_on(&s.cdx, BOOTTOOL).contains(&purl));
}

#[test]
fn win32_is_never_versioned_on_linux() {
    let s = main_scan();
    let win32 = named(&s.cdx, "Win32");
    assert_eq!(win32.len(), 1, "{win32:?}");
    assert_eq!(win32[0]["purl"], "pkg:hackage/Win32");
    assert!(win32[0]["version"].as_str().unwrap_or_default().is_empty());
    assert_eq!(prop(win32[0], C218), None);
    // It stays compiler-supplied, never resolved from the package set.
    assert_eq!(prop(win32[0], C170).as_deref(), Some("compiler-supplied"));
}

#[test]
fn no_versionless_twin_remains() {
    for s in [main_scan(), scan_with("closure_only", &read("closure.json"), CLOSURE_FLAGS)] {
        let c218 = c218_by_purl(&s);
        assert!(!c218.is_empty(), "CONTROL: something was versioned");
        for purl in c218.keys() {
            let bare = purl.split('@').next().unwrap();
            assert!(
                !purls(&s.cdx).iter().any(|p| p == bare),
                "{bare} beside {purl} (FR-010)"
            );
        }
        // #980: no edge endpoint is missing from the components.
        let refs: BTreeSet<String> = components(&s.cdx)
            .iter()
            .filter_map(|c| c["bom-ref"].as_str().map(str::to_string))
            .chain(s.cdx["metadata"]["component"]["bom-ref"].as_str().map(str::to_string))
            .collect();
        for d in s.cdx["dependencies"].as_array().unwrap() {
            let from = d["ref"].as_str().unwrap();
            assert!(refs.contains(from), "dangling ref {from}");
            for t in d["dependsOn"].as_array().into_iter().flatten() {
                assert!(refs.contains(t.as_str().unwrap()), "dangling target {t} from {from}");
            }
        }
    }
}

#[test]
fn closure_only_scan_versions_boot_libraries() {
    let s = scan_with("closure_only", &read("closure.json"), CLOSURE_FLAGS);
    assert_eq!(doc_row(&s, C174), None, "the shellcheck shape: the m926 pass declined");
    let purl = hackage("base", &version("9.10.3", "base"));
    let base = one(&s.cdx, &purl);
    assert_eq!(prop(base, C218).as_deref(), Some("ghc-9.10.3"));
    assert_eq!(prop(base, C170), None);
    assert!(depends_on(&s.cdx, LIB).contains(&purl), "{LIB} -> {:?}", depends_on(&s.cdx, LIB));
}

// ---- US2 ----

#[test]
fn os_string_is_not_compiler_supplied_on_966() {
    let s = scan_with("os_string_966", &closure_966(), CLOSURE_FLAGS);
    let os = named(&s.cdx, "os-string");
    assert_eq!(os.len(), 1, "{os:?}");
    // Resolved from the pinned package set, as any declared dependency is.
    assert_eq!(os[0]["purl"], "pkg:hackage/os-string@0.0.0.966");
    assert_eq!(prop(os[0], C170), None);
    assert_eq!(prop(os[0], C218), None, "9.6.6 does not ship it");
    // CONTROL: the same scan versions what 9.6.6 does ship.
    let process = named(&s.cdx, "process");
    assert_eq!(process.len(), 1, "{process:?}");
    assert_eq!(process[0]["version"], version("9.6.6", "process"));
    assert_eq!(prop(process[0], C218).as_deref(), Some("ghc-9.6.6"));
}

#[test]
fn the_scope_exemption_uses_the_exact_set() {
    let s = scan_with("os_string_966", &closure_966(), CLOSURE_FLAGS);
    let os = named(&s.cdx, "os-string");
    assert!(!os.is_empty(), "no os-string component");
    for c in &os {
        assert_ne!(
            prop(c, C203).as_deref(),
            Some("exempt-compiler-supplied"),
            "os-string is exempt as compiler-supplied on 9.6.6: {c}"
        );
    }
    // CONTROL: os-string is a closure member, so the scope rule did examine
    // it rather than skip it.
    assert!(os.iter().any(|c| prop(c, C203).is_some()), "{os:?}");
    // CONTROL: the exemption still fires on a library 9.6.6 ships, reached
    // through the same build-tooling path.
    let process = named(&s.cdx, "process");
    assert_eq!(prop(process[0], C203).as_deref(), Some("exempt-compiler-supplied"), "{}", process[0]);
}

#[test]
fn without_a_closure_the_union_rule_still_applies() {
    let s = scan_with("os_string_966", &closure_966(), NO_CLOSURE_FLAGS);
    let os = named(&s.cdx, "os-string");
    assert_eq!(os.len(), 1, "{os:?}");
    assert_eq!(os[0]["purl"], "pkg:hackage/os-string");
    assert_eq!(prop(os[0], C170).as_deref(), Some("compiler-supplied"));
    assert!(c218_by_purl(&s).is_empty(), "no C218 without a closure");
    assert_eq!(doc_row(&s, C219), None);

    let s = scan_with("project", &read("closure.json"), NO_CLOSURE_FLAGS);
    let base = named(&s.cdx, "base");
    assert_eq!(base.len(), 1);
    assert_eq!(base[0]["purl"], "pkg:hackage/base");
    assert_eq!(prop(base[0], C170).as_deref(), Some("compiler-supplied"));
}

// ---- US3 ----

#[test]
fn an_unknown_compiler_changes_nothing_and_says_so() {
    let s = scan_with("project", &absent_closure(), CLOSURE_FLAGS);
    assert!(c218_by_purl(&s).is_empty(), "no version from a compiler the table lacks");
    let base = named(&s.cdx, "base");
    assert_eq!(base.len(), 1, "{base:?}");
    assert_eq!(base[0]["purl"], "pkg:hackage/base");
    assert_eq!(prop(base[0], C170).as_deref(), Some("compiler-supplied"));
    assert_eq!(
        doc_row(&s, C219).as_deref(),
        Some(
            format!(
                r#"{{"compilers":{{"ghc-99.0.1":"absent-from-table"}},"project":"ghc-99.0.1","schema":1,"system":"x86_64-linux","table":"{}"}}"#,
                table_date()
            )
            .as_str()
        )
    );
}

#[test]
fn c219_names_both_compilers_and_the_table_date() {
    let s = main_scan();
    let v: Value = serde_json::from_str(&doc_row(&s, C219).expect("C219 on a closure scan")).unwrap();
    assert_eq!(v["compilers"]["ghc-9.10.3"], "in-table");
    assert_eq!(v["compilers"]["ghc-binary-9.8.4"], "in-table");
    assert_eq!(v["compilers"].as_object().unwrap().len(), 2);
    assert_eq!(v["project"], "ghc-9.10.3");
    assert_eq!(v["system"], "x86_64-linux");
    assert_eq!(v["table"], table_date().as_str());
    assert_eq!(v["schema"], 1);
}

#[test]
fn no_c219_without_a_closure() {
    let s = scan_with("project", &read("closure.json"), NO_CLOSURE_FLAGS);
    assert_eq!(doc_row(&s, C219), None);
    // CONTROL: the scan ran the Haskell pass, so absence is the gate.
    assert!(doc_row(&s, C174).is_some());
}

#[test]
fn no_c219_on_a_closure_without_haskell() {
    // A closure with no GHC anywhere, scanned from a project with no
    // Haskell manifest.
    let closure = read("closure.json")
        .replace("/nix/store/aaa-ghc-9.10.3", "/nix/store/aaa-gcc-14.2.0")
        .replace("/nix/store/bbb-ghc-binary-9.8.4", "/nix/store/aaa-gcc-14.2.0")
        .replace("\"setupCompilerEnvironmentPhase\": \"runHook preSetupCompilerEnvironment\",", "");
    let s = scan_with("closure_only", &closure, CLOSURE_FLAGS);
    // CONTROL: the closure tier ran.
    assert!(doc_row(&s, "waybill:nix-closure").is_some(), "C184 absent: the closure did not run");
    assert_eq!(doc_row(&s, C219), None);
}

#[test]
fn c219_present_on_a_closure_only_scan() {
    let s = scan_with("closure_only", &read("closure.json"), CLOSURE_FLAGS);
    assert_eq!(doc_row(&s, C174), None);
    let v: Value = serde_json::from_str(&doc_row(&s, C219).expect("C219 on a closure-only scan")).unwrap();
    let keys: BTreeSet<String> = v["compilers"].as_object().unwrap().keys().cloned().collect();
    let c218: BTreeSet<String> = c218_by_purl(&s).into_values().collect();
    assert_eq!(keys, c218, "C219's compilers are the document's C218 values");
}
