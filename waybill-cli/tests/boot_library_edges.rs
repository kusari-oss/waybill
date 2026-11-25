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

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

const BOOT_CONFIG: &str = "self: super: {\n  base = null;\n  process = null;\n  Win32 = null;\n  stm = null;\n  os-string = null;\n  ghc-prim = null;\n}\n";
const C218: &str = "waybill:boot-library-compiler";
const C247: &str = "waybill:boot-library-dependencies";
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
    common::parity::assert_parity(out.path());
    Run { scan, _out: out }
}

fn closure_flags() -> Vec<&'static str> {
    let mut v = vec!["--nix-closure"];
    v.extend_from_slice(ROOT);
    v
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

/// Every boot library carries C247 with its source and nothing absent
/// (spec 1655 FR-006): 9.10.3's `base` reaches `ghc-internal`, which spec
/// 1653 could only list as absent.
#[test]
fn every_boot_library_carries_c245() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    let boots: Vec<&Value> = components(&run.scan.cdx).into_iter().filter(|c| prop(c, C218).is_some()).collect();
    assert!(!boots.is_empty());
    for c in &boots {
        let v: Value = serde_json::from_str(&prop(c, C247).unwrap_or_else(|| panic!("no C247 on {c}"))).unwrap();
        assert!(v.get("source").is_some(), "{v}");
        assert!(v.get("absent").is_none(), "{}: {v}", c["purl"]);
    }
    let (base, _) = base_and_prim(&run.scan.cdx);
    let internal = boot_libraries(&run.scan.cdx)
        .get(&("ghc-9.10.3".to_string(), "ghc-internal".to_string()))
        .cloned()
        .expect("ghc-internal is a component");
    assert!(cdx_edges(&run.scan.cdx).contains(&(base, internal)));
}

/// An edge from a boot library of release R reaches the version R ships
/// (spec 1655 FR-005). Edges may cross C218 labels, because one version is
/// one component labelled with the project compiler; they may never reach a
/// version R does not ship.
#[test]
fn an_edge_reaches_the_version_its_release_ships() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    let boots = boot_libraries(&run.scan.cdx);
    // The precondition that makes this test able to fail: both compilers are
    // present, and the bootstrap one's `base` shares a name with a 9.10.3
    // component it must not reach (`ghc-prim@0.12.0`). A name-only lookup
    // would.
    let compilers: BTreeSet<&str> = boots.keys().map(|(c, _)| c.as_str()).collect();
    assert_eq!(compilers, BTreeSet::from(["ghc-9.10.3", "ghc-binary-9.8.4"]), "{boots:?}");
    let edges = cdx_edges(&run.scan.cdx);
    let base_98 = "pkg:hackage/base@4.19.2.0".to_string();
    assert!(edges.contains(&(base_98.clone(), "pkg:hackage/ghc-prim@0.11.0".to_string())));
    assert!(!edges.contains(&(base_98.clone(), "pkg:hackage/ghc-prim@0.12.0".to_string())));
    // The 9.8.4 `base` reaches `rts@1.0.2`, which both releases ship and the
    // document holds once, under the project compiler's label.
    assert!(edges.contains(&(base_98, "pkg:hackage/rts@1.0.2".to_string())));
    assert_eq!(boots.get(&("ghc-9.10.3".to_string(), "rts".to_string())).map(String::as_str), Some("pkg:hackage/rts@1.0.2"));
    assert!(!boots.contains_key(&("ghc-binary-9.8.4".to_string(), "rts".to_string())));
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
        components(cdx).into_iter().filter(|c| prop(c, C218).is_some()).map(|c| (c["purl"].as_str().unwrap().to_string(), prop(c, C247))).collect()
    };
    assert_eq!(c245(&plain.scan.cdx), c245(&with_store.scan.cdx));
}

/// FR-007: without `--nix-closure` there is no C247 and no boot-library edge
/// source.
#[test]
fn without_the_closure_nothing_changes() {
    let run = scan_with(&dir().join("project"), ROOT, None);
    for c in components(&run.scan.cdx) {
        assert!(prop(c, C247).is_none(), "C247 without --nix-closure: {c}");
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

/// Spec 1655: every dependency is now a component, so the reason is gone.
/// The stub stays `partial` for its other reason (`hackage` transitive
/// edges), so the `compositions[]` check below still has work to do. The
/// reason's positive control is the unit test
/// `a_version_the_table_lacks_stays_absent` (research R6).
#[test]
fn boot_library_edges_are_complete() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    assert_eq!(doc_prop(&run.scan.cdx, "waybill:graph-completeness").as_deref(), Some("partial"));
    let reason = doc_prop(&run.scan.cdx, "waybill:graph-completeness-reason").unwrap_or_default();
    assert!(!reason.contains("boot-library-edges-incomplete"), "{reason}");
    assert!(reason.contains("transitive-edges-unresolvable: hackage"), "{reason}");
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


/// Spec 1654 (#1695, R4b) — the SPDX side of a member CycloneDX marks
/// `scope: excluded`: every one receives a `BUILD_DEPENDENCY_OF` in SPDX 2.3,
/// and none a plain `DEPENDS_ON`. Unit tests set `artifact_presence` by hand,
/// so this is what fails if pass 3 runs before the closure role pass sets it.
#[test]
fn edges_into_excluded_members_are_build_dependencies_in_spdx23() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    let excluded: BTreeSet<String> = components(&run.scan.cdx)
        .into_iter()
        .filter(|c| c["scope"] == "excluded")
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect();
    assert!(!excluded.is_empty(), "positive control: the fixture must exclude a member");
    let spdx = &run.scan.spdx23;
    let purl_of: BTreeMap<&str, &str> = spdx["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| {
            let purl = p["externalRefs"]
                .as_array()?
                .iter()
                .find(|r| r["referenceType"] == "purl")?["referenceLocator"]
                .as_str()?;
            Some((p["SPDXID"].as_str()?, purl))
        })
        .collect();
    let mut build_into: BTreeSet<String> = BTreeSet::new();
    for r in spdx["relationships"].as_array().unwrap() {
        let ty = r["relationshipType"].as_str().unwrap_or("");
        // BUILD_DEPENDENCY_OF reads dependency -> dependent; DEPENDS_ON the reverse.
        let (dependency, kind) = match ty {
            "BUILD_DEPENDENCY_OF" => (r["spdxElementId"].as_str(), "build"),
            "DEPENDS_ON" => (r["relatedSpdxElement"].as_str(), "runtime"),
            _ => continue,
        };
        let Some(purl) = dependency.and_then(|id| purl_of.get(id)) else { continue };
        if !excluded.contains(*purl) {
            continue;
        }
        assert_eq!(kind, "build", "a plain DEPENDS_ON into the excluded member {purl}: {r}");
        build_into.insert(purl.to_string());
    }
    assert!(!build_into.is_empty(), "no BUILD_DEPENDENCY_OF into any of {excluded:?}");
}

// ---------------------------------------------------------------------
// Spec 1655 (#1696): the reachable boot libraries
// ---------------------------------------------------------------------

/// The boot libraries present before spec 1655 (`measurements/stub-before.md`).
/// `ghc-prim@0.12.0` is present only when the project declares it.
const PRESENT_BEFORE: &[&str] = &[
    "pkg:hackage/base@4.20.2.0",
    "pkg:hackage/process@1.6.26.1",
    "pkg:hackage/stm@2.5.3.1",
    "pkg:hackage/base@4.19.2.0",
];
const DECLARED_PRIM: &str = "pkg:hackage/ghc-prim@0.12.0";

/// What spec 1655 added, measured in `measurements/stub-before.md` with
/// `shared_purls.py`, with ghc-prim declared. Names and versions together.
const ADDED_WITH_PRIM: &[&str] = &[
    "array@0.5.8.0", "bytestring@0.12.2.0", "deepseq@1.5.0.0", "directory@1.3.8.5", "exceptions@0.10.9",
    "filepath@1.5.4.0", "ghc-bignum@1.3", "ghc-boot-th@9.10.3", "ghc-internal@9.1003.0", "mtl@2.3.1",
    "os-string@2.0.7", "pretty@1.1.3.6", "rts@1.0.2", "template-haskell@2.22.0.0", "time@1.12.2",
    "transformers@0.6.1.1", "unix@2.8.7.0", "ghc-prim@0.11.0",
];

fn added(cdx: &Value, prim_declared: bool) -> BTreeSet<String> {
    boot_libraries(cdx)
        .into_values()
        .filter(|p| !PRESENT_BEFORE.contains(&p.as_str()))
        .filter(|p| !(prim_declared && p == DECLARED_PRIM))
        .collect()
}

fn want(extra: &[&str]) -> BTreeSet<String> {
    ADDED_WITH_PRIM.iter().chain(extra).map(|s| format!("pkg:hackage/{s}")).collect()
}

/// SC-002 on the stub: exactly the measured PURLs, 18 with ghc-prim and 19
/// without it (`ghc-prim@0.12.0` is then added, not declared).
#[test]
fn added_purls_match_the_measurement() {
    let p = project_with_ghc_prim();
    let with_prim = scan_with(&p.path().join("project"), &closure_flags(), None);
    assert_eq!(added(&with_prim.scan.cdx, true), want(&[]));
    let plain = scan_with(&dir().join("project"), &closure_flags(), None);
    assert_eq!(added(&plain.scan.cdx, false), want(&["ghc-prim@0.12.0"]));
}

/// FR-007: no boot library is an orphan.
#[test]
fn no_added_boot_library_is_an_orphan() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    for c in components(&run.scan.cdx).into_iter().filter(|c| prop(c, C218).is_some()) {
        assert!(prop(c, "waybill:orphan-reason").is_none(), "{} is an orphan", c["purl"]);
    }
}

/// FR-002 and FR-004: an added library has exactly the shape of the boot
/// library the closure synthesised before spec 1655 (the stub's 9.8.4
/// `base`): a Hackage PURL, no CPE, no CycloneDX scope, `build` tier, and the
/// `waybill:*` keys C218, C247 and the tier only. Versions are pinned by
/// `added_purls_match_the_measurement`, so no table is read here.
#[test]
fn added_libraries_have_their_peers_shape() {
    let p = project_with_ghc_prim();
    let run = scan_with(&p.path().join("project"), &closure_flags(), None);
    let peer_keys = |c: &Value| -> BTreeSet<String> {
        c["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["name"].as_str())
            .filter(|n| n.starts_with("waybill:"))
            .map(str::to_string)
            .collect()
    };
    let all = components(&run.scan.cdx);
    let peer = all.iter().find(|c| c["purl"] == "pkg:hackage/base@4.19.2.0").expect("the 9.8.4 base");
    let want_keys = BTreeSet::from([C218.to_string(), C247.to_string(), "waybill:sbom-tier".to_string()]);
    assert_eq!(peer_keys(peer), want_keys, "the peer's shape changed");
    let added = added(&run.scan.cdx, true);
    assert!(!added.is_empty());
    for c in all.iter().filter(|c| c["purl"].as_str().is_some_and(|p| added.contains(p))) {
        assert!(c["purl"].as_str().unwrap().starts_with("pkg:hackage/"));
        assert!(c.get("cpe").is_none(), "{}", c["purl"]);
        assert!(c.get("scope").is_none(), "{}", c["purl"]);
        assert_eq!(prop(c, "waybill:sbom-tier").as_deref(), Some("build"));
        assert_eq!(peer_keys(c), want_keys, "{}", c["purl"]);
    }
}
