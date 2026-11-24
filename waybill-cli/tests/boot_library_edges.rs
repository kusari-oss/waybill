//! Spec 1653 (#1688) — under `--nix-closure`, GHC boot libraries carry the
//! dependency edges their compiler release records, or the document says
//! they don't.
//!
//! Contract `specs/1653-boot-library-edges/contracts/wire.md`. Every scan
//! drives the stub `nix` over milestone 966's fixtures
//! (`fixtures/nix_closure_boot_versions/`), whose closure compiler is GHC
//! 9.10.3, and checks `waybill sbom parity-check`. The project declares
//! `base`, `process` and `Win32`; the positive control adds `ghc-prim`, so
//! 9.10.3's `base` → `ghc-prim` has both ends in the document.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

const BOOT_CONFIG: &str = "self: super: {\n  base = null;\n  process = null;\n  Win32 = null;\n  stm = null;\n  os-string = null;\n  ghc-prim = null;\n}\n";
const C218: &str = "waybill:boot-library-compiler";
const C245: &str = "waybill:boot-library-dependencies";
const ROOT: &[&str] = &["--root-name", "waybill-fixture-root"];

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_boot_versions")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(dir().join(name)).unwrap()
}

/// The project, copied, with `ghc-prim` added to its `build-depends`.
fn project_with_ghc_prim() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    let dst = t.path().join("project");
    std::fs::create_dir_all(&dst).unwrap();
    for e in std::fs::read_dir(dir().join("project")).unwrap() {
        let e = e.unwrap();
        let mut text = std::fs::read_to_string(e.path()).unwrap();
        if e.file_name().to_string_lossy().ends_with(".cabal") {
            text = text.replace("build-depends:    base,", "build-depends:    base, ghc-prim,");
            assert!(text.contains("ghc-prim"), "the fixture's build-depends line changed");
        }
        std::fs::write(dst.join(e.file_name()), text).unwrap();
    }
    t
}

struct Run {
    scan: Scan,
    _out: tempfile::TempDir,
}

fn scan_with(project: &Path, flags: &[&str], store: Option<PathBuf>) -> Run {
    let packages = read("hackage-packages.nix");
    let closure = read("closure.json");
    let out = tempfile::tempdir().unwrap();
    let mut argv = vec!["--no-deps-dev", "--no-clearly-defined"];
    argv.extend_from_slice(flags);
    let scan = NixStub {
        project: project.to_path_buf(),
        closure: &closure,
        package_set: Some((&packages, BOOT_CONFIG)),
        store_dir: store,
        ..Default::default()
    }
    .scan_to(out.path(), "waybill", &argv);
    parity(out.path());
    Run { scan, _out: out }
}

fn closure_flags() -> Vec<&'static str> {
    let mut v = vec!["--nix-closure"];
    v.extend_from_slice(ROOT);
    v
}

/// The parity gaps a closure scan already has on `main` (measured on this
/// fixture at `91d84d1`): the dependency-edge rows B1 and B2, whose CycloneDX
/// count differs structurally from SPDX's, and E1, which CycloneDX carries
/// natively in `compositions[]`. Tracked separately; this spec adds none.
const PRE_EXISTING_GAPS: &[&str] = &["B1", "B2", "E1"];

/// `waybill sbom parity-check` over the three documents.
///
/// A row's `universal_parity` field says whether the catalog expects the row
/// in every format, not whether it passed. The verdict is only in
/// `summary.parity_gaps`, and the per-row report carries presence and counts.
/// So: every universal row that any format carries must be present, with the
/// same count, in all three, apart from the gaps `main` already has.
fn parity(out_dir: &Path) {
    let out = Command::new(common::bin())
        .args(["sbom", "parity-check", "--format", "json", "--scan-dir"])
        .arg(out_dir)
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let formats = ["cdx", "spdx23", "spdx3"];
    let gaps: Vec<&Value> = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["universal_parity"] == true)
        .filter(|r| formats.iter().any(|f| r[*f]["kind"] == "present"))
        .filter(|r| {
            let kinds: BTreeSet<String> = formats.iter().map(|f| r[*f]["kind"].to_string()).collect();
            let counts: BTreeSet<String> = formats.iter().map(|f| r[*f]["value_count"].to_string()).collect();
            kinds.len() > 1 || counts.len() > 1
        })
        .filter(|r| !PRE_EXISTING_GAPS.contains(&r["row_id"].as_str().unwrap_or("")))
        .collect();
    assert!(gaps.is_empty(), "parity gaps: {gaps:#?}");
    let reported = report["summary"]["parity_gaps"].as_u64().unwrap() as usize;
    assert!(reported <= PRE_EXISTING_GAPS.len(), "parity-check reports {reported} gaps: {:#?}", report["summary"]);
}

fn components(cdx: &Value) -> Vec<&Value> {
    let mut out = vec![&cdx["metadata"]["component"]];
    let mut stack: Vec<&Value> = cdx["components"].as_array().into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c["components"].as_array().into_iter().flatten());
    }
    out
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

/// Boot libraries by (compiler, name) -> PURL.
fn boot_libraries(cdx: &Value) -> BTreeMap<(String, String), String> {
    components(cdx)
        .into_iter()
        .filter_map(|c| {
            let compiler = prop(c, C218)?;
            Some(((compiler, c["name"].as_str()?.to_string()), c["purl"].as_str()?.to_string()))
        })
        .collect()
}

fn cdx_edges(cdx: &Value) -> BTreeSet<(String, String)> {
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|d| {
            let from = d["ref"].as_str().unwrap().to_string();
            d["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |t| t.as_str().map(|t| (from.clone(), t.to_string())))
        })
        .collect()
}

/// SPDX 2.3: `DEPENDS_ON` as (from PURL, to PURL).
fn spdx23_edges(doc: &Value) -> BTreeSet<(String, String)> {
    let purl_of: BTreeMap<&str, &str> = doc["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let purl = p["externalRefs"]
                .as_array()?
                .iter()
                .find(|r| r["referenceType"] == "purl")?["referenceLocator"]
                .as_str()?;
            Some((p["SPDXID"].as_str()?, purl))
        })
        .collect();
    doc["relationships"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["relationshipType"] == "DEPENDS_ON")
        .filter_map(|r| {
            let a = purl_of.get(r["spdxElementId"].as_str()?)?;
            let b = purl_of.get(r["relatedSpdxElement"].as_str()?)?;
            Some((a.to_string(), b.to_string()))
        })
        .collect()
}

/// SPDX 3: `dependsOn` relationships as (from PURL, to PURL).
fn spdx3_edges(doc: &Value) -> BTreeSet<(String, String)> {
    let graph = doc["@graph"].as_array().unwrap();
    let purl_of: BTreeMap<&str, &str> = graph
        .iter()
        .filter_map(|e| Some((e["spdxId"].as_str()?, e["software_packageUrl"].as_str()?)))
        .collect();
    graph
        .iter()
        .filter(|e| e["type"] == "Relationship" && e["relationshipType"] == "dependsOn")
        .flat_map(|e| {
            let from = e["from"].as_str().and_then(|f| purl_of.get(f)).map(|s| s.to_string());
            e["to"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| purl_of.get(t.as_str()?).map(|s| s.to_string()))
                .filter_map(move |to| from.clone().map(|f| (f, to)))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn base_and_prim(cdx: &Value) -> (String, String) {
    let boots = boot_libraries(cdx);
    let get = |n: &str| boots.get(&("ghc-9.10.3".to_string(), n.to_string())).cloned().unwrap_or_else(|| panic!("no 9.10.3 {n}: {boots:#?}"));
    (get("base"), get("ghc-prim"))
}

// ---------------------------------------------------------------------
// US1: the edges, in three formats (FR-002, SC-004)
// ---------------------------------------------------------------------

/// SC-004's positive control: 9.10.3's `base` → `ghc-prim`, which the
/// release's own package database records.
#[test]
fn base_depends_on_ghc_prim_in_every_format() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    let (base, prim) = base_and_prim(&run.scan.cdx);
    let edge = (base.clone(), prim.clone());
    assert!(cdx_edges(&run.scan.cdx).contains(&edge), "CycloneDX: {base} -> {prim}");
    assert!(spdx23_edges(&run.scan.spdx23).contains(&edge), "SPDX 2.3: {base} -> {prim}");
    assert!(spdx3_edges(&run.scan.spdx3).contains(&edge), "SPDX 3: {base} -> {prim}");
}

/// Every boot library carries C245; 9.10.3's `base` names `ghc-internal`,
/// which is not a component, as absent.
#[test]
fn every_boot_library_carries_c245() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    let boots: Vec<&Value> = components(&run.scan.cdx).into_iter().filter(|c| prop(c, C218).is_some()).collect();
    assert!(!boots.is_empty());
    for c in &boots {
        let v: Value = serde_json::from_str(&prop(c, C245).unwrap_or_else(|| panic!("no C245 on {c}"))).unwrap();
        assert!(v.get("source").is_some() || v.get("reason").is_some(), "{v}");
    }
    let (base, _) = base_and_prim(&run.scan.cdx);
    let base_c = components(&run.scan.cdx).into_iter().find(|c| c["purl"] == base.as_str()).unwrap();
    let v: Value = serde_json::from_str(&prop(base_c, C245).unwrap()).unwrap();
    assert_eq!(v["absent"], serde_json::json!(["ghc-internal"]), "{v}");
}

/// Edges never cross compilers: the bootstrap compiler's boot libraries only
/// reach their own.
#[test]
fn edges_stay_within_a_compiler() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    let boots = boot_libraries(&run.scan.cdx);
    // The precondition that makes this test able to fail: both compilers are
    // present, and the bootstrap one's `base` shares a name with a 9.10.3
    // component it must not reach (`ghc-prim`). A name-only lookup would.
    let compilers: BTreeSet<&str> = boots.keys().map(|(c, _)| c.as_str()).collect();
    assert_eq!(compilers, BTreeSet::from(["ghc-9.10.3", "ghc-binary-9.8.4"]), "{boots:?}");
    assert!(boots.contains_key(&("ghc-binary-9.8.4".to_string(), "base".to_string())));
    let compiler_of: BTreeMap<&str, &str> = boots.iter().map(|((c, _), p)| (p.as_str(), c.as_str())).collect();
    for (a, b) in cdx_edges(&run.scan.cdx) {
        if let (Some(ca), Some(cb)) = (compiler_of.get(a.as_str()), compiler_of.get(b.as_str())) {
            assert_eq!(ca, cb, "{a} -> {b} crosses compilers");
        }
    }
}

/// SC-002a: a compiler output present in the store, whose package database
/// says something else, changes nothing — scans read only the table.
#[test]
fn a_present_store_output_changes_nothing() {
    let p = project_with_ghc_prim();
    let plain = scan_with(&p.path().join("project"), &closure_flags(), None);
    let store = tempfile::tempdir().unwrap();
    let db = store.path().join("waybill-fixture-hash-ghc-9.10.3/lib/ghc-9.10.3/lib/package.conf.d");
    std::fs::create_dir_all(&db).unwrap();
    std::fs::write(db.join("base-4.20.2.0.conf"), "name: base\nversion: 4.20.2.0\ndepends: rts\n").unwrap();
    let with_store = scan_with(&p.path().join("project"), &closure_flags(), Some(store.path().to_path_buf()));
    assert_eq!(cdx_edges(&plain.scan.cdx), cdx_edges(&with_store.scan.cdx));
    let c245 = |cdx: &Value| -> BTreeMap<String, Option<String>> {
        components(cdx).into_iter().filter(|c| prop(c, C218).is_some()).map(|c| (c["purl"].as_str().unwrap().to_string(), prop(c, C245))).collect()
    };
    assert_eq!(c245(&plain.scan.cdx), c245(&with_store.scan.cdx));
}

/// FR-007: without `--nix-closure` there is no C245 and no boot-library edge
/// source.
#[test]
fn without_the_closure_nothing_changes() {
    let run = scan_with(&dir().join("project"), ROOT, None);
    for c in components(&run.scan.cdx) {
        assert!(prop(c, C245).is_none(), "C245 without --nix-closure: {c}");
    }
}

// ---------------------------------------------------------------------
// US2: the verdict (FR-005)
// ---------------------------------------------------------------------

fn doc_prop(cdx: &Value, name: &str) -> Option<String> {
    cdx["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

/// `rts` and `ghc-internal` are never components, so some boot library's
/// edges are incomplete and the document is not `complete`.
#[test]
fn incomplete_boot_library_edges_are_reported() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    assert_eq!(doc_prop(&run.scan.cdx, "waybill:graph-completeness").as_deref(), Some("partial"));
    let reason = doc_prop(&run.scan.cdx, "waybill:graph-completeness-reason").unwrap_or_default();
    assert!(reason.contains("boot-library-edges-incomplete:"), "{reason}");
    let root_ref = run.scan.cdx["metadata"]["component"]["bom-ref"].clone();
    for c in run.scan.cdx["compositions"].as_array().into_iter().flatten() {
        if c["aggregate"] != "complete" {
            continue;
        }
        if c.get("assemblies").is_none() && c["dependencies"] == serde_json::json!([root_ref]) {
            continue;
        }
        let names_hackage = ["assemblies", "dependencies"].iter().any(|k| {
            c[*k].as_array().into_iter().flatten().any(|a| a.as_str().is_some_and(|s| s.starts_with("pkg:hackage/")))
        });
        assert!(!names_hackage, "a complete record names hackage: {c}");
    }
}

