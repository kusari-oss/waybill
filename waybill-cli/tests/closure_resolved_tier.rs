//! Spec 1650 (#1681) — under `--nix-closure`, a component the closure
//! resolved is not labelled `design`.
//!
//! Contract `specs/1650-closure-resolved-tier/contracts/wire.md`. Every scan
//! drives the stub `nix` over milestone 966's fixtures
//! (`fixtures/nix_closure_boot_versions/`). They reproduce #1681 exactly:
//! with `--root-name`, the evaluated package `waybill-fixture-app` carries C203
//! `exempt-evaluated-package` and C214 `absent`, and is the root's only
//! direct dependency. The project declares `base` and `process`, which the
//! closure's GHC 9.10.3 ships, so they carry C218.
//!
//! | case | component | before | after |
//! |---|---|---|---|
//! | P2 | `waybill-fixture-app` (C203, C214 `absent`) | `design` | `build` |
//! | P3 | `waybill-fixture-extra`, a second repository package the closure does not evaluate | `design` | `design` |
//! | B1 | `base@<9.10.3's>`, `process` (declared, C218 `ghc-9.10.3`) | `design` | `build` |
//! | B2 | `stm` (undeclared, synthesised by the closure, C218 `ghc-9.10.3`) | `build` | `build` |
//! | B3 | `Win32` (no version from any source, no C218) | `design` | `design` |
//! | B4 | `base@<9.8.4's>` (bootstrap `ghc-binary-9.8.4`) | `build` | `build` |
//!
//! Every scan is checked with `waybill sbom parity-check`. Row C5
//! (`waybill:sbom-tier`) is `SymmetricEqual`, so a tier that differs between
//! the formats is a parity gap.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

const BOOT_CONFIG: &str = "self: super: {\n  base = null;\n  process = null;\n  Win32 = null;\n  stm = null;\n  os-string = null;\n}\n";
const TIER: &str = "waybill:sbom-tier";
const C203: &str = "waybill:scope-classification-source";
const C218: &str = "waybill:boot-library-compiler";
const ROOT: &[&str] = &["--root-name", "waybill-fixture-root"];

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_boot_versions")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(dir().join(name)).unwrap()
}

/// The committed boot-library table's version of `lib` for a GHC `release`,
/// read independently of the binary, as milestone 966's tests do.
fn boot_version(release: &str, lib: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv");
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|f| f.len() > 2 && f[0] == release && f[1] == lib)
        .map(|f| f[2].to_string())
        .unwrap_or_else(|| panic!("no {lib} for {release} in the table"))
}

struct Run {
    scan: Scan,
    _out: tempfile::TempDir,
}

/// Scan `project` with the closure tier, in three formats, and check
/// parity. `bin` overrides the binary (the control).
fn scan_project(project: &Path, flags: &[&str], bin: Option<PathBuf>) -> Run {
    let packages = read("hackage-packages.nix");
    let closure = read("closure.json");
    let out = tempfile::tempdir().unwrap();
    let mut argv = vec!["--no-deps-dev", "--no-clearly-defined"];
    argv.extend_from_slice(flags);
    let scan = NixStub {
        project: project.to_path_buf(),
        closure: &closure,
        package_set: Some((&packages, BOOT_CONFIG)),
        bin,
        ..Default::default()
    }
    .scan_to(out.path(), "waybill", &argv);
    parity(out.path());
    unique_bom_refs(&scan.cdx);
    Run { scan, _out: out }
}

fn scan(flags: &[&str]) -> Run {
    scan_project(&dir().join("project"), flags, None)
}

fn closure_flags(extra: &[&str]) -> Vec<&'static str> {
    let mut v = vec!["--nix-closure"];
    for e in extra {
        v.push(Box::leak(e.to_string().into_boxed_str()));
    }
    v
}

/// `waybill sbom parity-check` over the three documents; a row is a gap only
/// if some format carries a value.
fn parity(out_dir: &Path) {
    let out = Command::new(common::bin())
        .args(["sbom", "parity-check", "--format", "json", "--scan-dir"])
        .arg(out_dir)
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

/// Every CycloneDX component, nested ones and `metadata.component` included.
fn components(cdx: &Value) -> Vec<&Value> {
    let mut out = vec![&cdx["metadata"]["component"]];
    let mut stack: Vec<&Value> = cdx["components"].as_array().into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c["components"].as_array().into_iter().flatten());
    }
    out
}

fn unique_bom_refs(cdx: &Value) {
    let mut seen = BTreeSet::new();
    let dups: Vec<&str> = components(cdx)
        .into_iter()
        .skip(1)
        .filter_map(|c| c["bom-ref"].as_str())
        .filter(|r| !seen.insert(*r))
        .collect();
    assert!(dups.is_empty(), "duplicate bom-refs: {dups:?}");
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

/// The one component with this PURL.
fn one<'a>(cdx: &'a Value, purl: &str) -> &'a Value {
    let hits: Vec<&Value> = components(cdx).into_iter().filter(|c| c["purl"] == purl).collect();
    assert_eq!(hits.len(), 1, "{purl}: {hits:#?}");
    hits[0]
}

fn tier(cdx: &Value, purl: &str) -> Option<String> {
    prop(one(cdx, purl), TIER)
}

fn hackage(name: &str, version: &str) -> String {
    format!("pkg:hackage/{name}@{version}")
}

/// The CycloneDX dependency graph, by `bom-ref`.
fn edges(cdx: &Value) -> HashMap<String, Vec<String>> {
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            let to = d["dependsOn"].as_array().into_iter().flatten();
            (d["ref"].as_str().unwrap().to_string(), to.filter_map(|t| t.as_str().map(str::to_string)).collect())
        })
        .collect()
}

/// How many components are reachable from the root, skipping those `skip`
/// rejects (and everything reachable only through them).
fn reachable(cdx: &Value, skip: impl Fn(&Value) -> bool) -> usize {
    let by_ref: BTreeMap<&str, &Value> =
        components(cdx).into_iter().filter_map(|c| c["bom-ref"].as_str().map(|r| (r, c))).collect();
    let g = edges(cdx);
    let root = cdx["metadata"]["component"]["bom-ref"].as_str().unwrap().to_string();
    let mut seen: BTreeSet<String> = BTreeSet::from([root.clone()]);
    let mut todo = vec![root];
    while let Some(n) = todo.pop() {
        for t in g.get(&n).into_iter().flatten() {
            let rejected = by_ref.get(t.as_str()).is_some_and(|c| skip(c));
            if !rejected && seen.insert(t.clone()) {
                todo.push(t.clone());
            }
        }
    }
    seen.len() - 1
}

fn app() -> String {
    hackage("waybill-fixture-app", "0.1.0")
}

/// The project with a second package the closure does not evaluate (P3).
fn project_with_extra() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    let dst = t.path().join("project");
    std::fs::create_dir_all(&dst).unwrap();
    for e in std::fs::read_dir(dir().join("project")).unwrap() {
        let e = e.unwrap();
        std::fs::copy(e.path(), dst.join(e.file_name())).unwrap();
    }
    let extra = dst.join("extra");
    std::fs::create_dir_all(&extra).unwrap();
    std::fs::write(
        extra.join("waybill-fixture-extra.cabal"),
        "cabal-version: 2.4\nname: waybill-fixture-extra\nversion: 0.2.0\n\nlibrary\n    build-depends: base\n    default-language: Haskell2010\n",
    )
    .unwrap();
    t
}

// ---------------------------------------------------------------------
// T004: the project's own package (User Story 1)
// ---------------------------------------------------------------------

/// P2: the evaluated package is `build`, although its outputs were not
/// examined (C214 `absent`, moat's case). C214 does not decide a member's
/// tier; C203 says the closure evaluated it.
#[test]
fn p2_evaluated_package_is_build_whatever_c214_says() {
    let r = scan(&closure_flags(ROOT));
    let c = one(&r.scan.cdx, &app());
    assert_eq!(prop(c, C203).as_deref(), Some("exempt-evaluated-package"));
    assert_eq!(prop(c, "waybill:closure-contents").as_deref(), Some("absent"));
    assert_eq!(prop(c, TIER).as_deref(), Some("build"));
}

/// P3: a repository package the closure does not evaluate keeps `design`.
#[test]
fn p3_unevaluated_repository_package_stays_design() {
    let t = project_with_extra();
    let r = scan_project(&t.path().join("project"), &closure_flags(ROOT), None);
    let extra = hackage("waybill-fixture-extra", "0.2.0");
    assert_eq!(prop(one(&r.scan.cdx, &extra), C203), None);
    assert_eq!(tier(&r.scan.cdx, &extra).as_deref(), Some("design"));
    assert_eq!(tier(&r.scan.cdx, &app()).as_deref(), Some("build"));
}

/// R1: the root's direct dependency is no longer `design`, and dropping
/// `design` components loses only those with no closure evidence.
#[test]
fn r1_the_graph_does_not_hang_off_design() {
    let r = scan(&closure_flags(ROOT));
    let cdx = &r.scan.cdx;
    let root = cdx["metadata"]["component"]["bom-ref"].as_str().unwrap();
    let direct = edges(cdx).get(root).cloned().unwrap_or_default();
    assert_eq!(direct, vec![app()]);
    let is_design = |c: &Value| prop(c, TIER).as_deref() == Some("design");
    let all = reachable(cdx, |_| false);
    let without_design = reachable(cdx, is_design);
    let design_reachable: usize = components(cdx)
        .into_iter()
        .filter(|c| is_design(c))
        .count();
    assert!(all > 0);
    // Only `Win32` (B3) is `design` here, and it is a leaf.
    assert_eq!(design_reachable, 1, "{:?}", components(cdx).iter().filter(|c| is_design(c)).map(|c| &c["purl"]).collect::<Vec<_>>());
    assert_eq!(without_design, all - 1);
}

/// T1: `--tier design-only` no longer emits what the closure resolved; the
/// filter runs again after the re-tier (#1360).
#[test]
fn t1_tier_design_only_excludes_the_retiered_components() {
    let r = scan(&closure_flags(&["--root-name", "waybill-fixture-root", "--tier", "design-only"]));
    let purls: BTreeSet<String> = components(&r.scan.cdx)
        .into_iter()
        .skip(1)
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect();
    assert!(!purls.contains(&app()), "{purls:?}");
    assert!(!purls.contains(&hackage("base", &boot_version("9.10.3", "base"))), "{purls:?}");
    assert!(purls.contains("pkg:hackage/Win32"), "{purls:?}");
}

// ---------------------------------------------------------------------
// T007: boot libraries (User Story 2)
// ---------------------------------------------------------------------

/// B1: a boot library the manifest declares, versioned from the closure's
/// compiler, is `build` with C218 naming that compiler.
#[test]
fn b1_declared_boot_library_is_build() {
    let r = scan(&closure_flags(&[]));
    for lib in ["base", "process"] {
        let p = hackage(lib, &boot_version("9.10.3", lib));
        let c = one(&r.scan.cdx, &p);
        assert_eq!(prop(c, C218).as_deref(), Some("ghc-9.10.3"), "{p}");
        assert_eq!(prop(c, TIER).as_deref(), Some("build"), "{p}");
    }
}

/// B2: one the manifest does not declare, synthesised by the closure, stays
/// `build`.
#[test]
fn b2_synthesised_boot_library_stays_build() {
    let r = scan(&closure_flags(&[]));
    let p = hackage("stm", &boot_version("9.10.3", "stm"));
    assert_eq!(prop(one(&r.scan.cdx, &p), C218).as_deref(), Some("ghc-9.10.3"));
    assert_eq!(tier(&r.scan.cdx, &p).as_deref(), Some("build"));
}

/// B3: a component with no version from any source, and no C218, keeps
/// `design`.
#[test]
fn b3_unresolved_component_stays_design() {
    let r = scan(&closure_flags(&[]));
    let c = one(&r.scan.cdx, "pkg:hackage/Win32");
    assert_eq!(prop(c, C218), None);
    assert_eq!(prop(c, TIER).as_deref(), Some("design"));
}

/// B4: the bootstrap compiler's boot libraries are already `build`, and stay
/// so.
#[test]
fn b4_bootstrap_boot_library_stays_build() {
    let r = scan(&closure_flags(&[]));
    let p = hackage("base", &boot_version("9.8.4", "base"));
    assert_eq!(prop(one(&r.scan.cdx, &p), C218).as_deref(), Some("ghc-binary-9.8.4"));
    assert_eq!(tier(&r.scan.cdx, &p).as_deref(), Some("build"));
}

// ---------------------------------------------------------------------
// T012: nothing else changes (User Story 4)
// ---------------------------------------------------------------------

/// N1: without `--nix-closure` the documents equal the control binary's
/// (`WAYBILL_TIER_MAIN_BIN`, `main`'s), version-4 UUIDs masked. With
/// `WAYBILL_TIER_EXPECT_DIFF=1` it runs the closure scan instead and requires
/// a difference: the positive control.
#[test]
#[ignore = "control: needs WAYBILL_TIER_MAIN_BIN"]
fn n1_control_byte_identical_to_main_without_the_closure() {
    let other = PathBuf::from(std::env::var("WAYBILL_TIER_MAIN_BIN").unwrap());
    let expect_diff = std::env::var("WAYBILL_TIER_EXPECT_DIFF").is_ok();
    let flags: Vec<&str> = if expect_diff { closure_flags(ROOT) } else { ROOT.to_vec() };
    let ours = scan_project(&dir().join("project"), &flags, None);
    let theirs = scan_project(&dir().join("project"), &flags, Some(other));
    let re = regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}").unwrap();
    let mask = |v: &Value| re.replace_all(&v.to_string(), "UUID").into_owned();
    let same = [
        (&ours.scan.cdx, &theirs.scan.cdx),
        (&ours.scan.spdx23, &theirs.scan.spdx23),
        (&ours.scan.spdx3, &theirs.scan.spdx3),
    ]
    .iter()
    .all(|(a, b)| mask(a) == mask(b));
    assert_eq!(same, !expect_diff, "expect_diff={expect_diff}");
}
