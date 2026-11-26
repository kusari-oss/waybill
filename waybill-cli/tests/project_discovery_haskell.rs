//! Spec 1652 (#1676) — `--project-discovery=root-only` keeps the packages the
//! root builds, and records what it dropped.
//!
//! Contracts: `specs/1652-root-only-haskell/contracts/{scope-filter,wire}.md`.
//! Fixtures: `tests/fixtures/project_discovery_haskell/`, written by
//! `specs/1652-root-only-haskell/measurements/make-fixtures.sh`:
//!
//! | tree | shape |
//! |---|---|
//! | f1 | root `package.json`; nested hpack pair, `b` → `a`, unreached |
//! | f2 | root `package.yaml` → `a` → `c`; `d` independent |
//! | f3 | root `cabal.project` `packages: local-packages/*`, no root package; `tools/e` unlisted |
//! | f4 | f3 plus a root `package.json` |
//! | f5 | root `stack.yaml` listing `a`, `b`; `tools/e` unlisted |
//! | f6 | npm root depending on a nested npm package by path |
//!
//! Every scan is offline, with an isolated `HOME`, in all three formats, and is
//! checked with `waybill sbom parity-check`.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const C140: &str = "waybill:project-discovery-mode";
const C244: &str = "waybill:project-discovery-dropped";
const C104: &str = "waybill:graph-completeness";
const C105: &str = "waybill:graph-completeness-reason";
const C200: &str = "waybill:hpack-read-outcome";
const FELL_BACK: &str = "found zero root-level manifests";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project_discovery_haskell")
        .join(name)
}

/// One scan, three documents.
struct Docs {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    log: String,
    _out: tempfile::TempDir,
}

fn scan(tree: &Path, mode: &str) -> Docs {
    let out = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let cdx = out.path().join("waybill.cdx.json");
    let spdx23 = out.path().join("waybill.spdx.json");
    let spdx3 = out.path().join("waybill.spdx3.json");
    let o = Command::new(common::bin())
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", home.path())
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(tree)
        .arg(format!("--project-discovery={mode}"))
        .args([
            "--no-deep-hash",
            "--format",
            "cyclonedx-json",
            "--format",
            "spdx-2.3-json",
            "--format",
            "spdx-3-json",
            "--output",
        ])
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3.display()))
        .output()
        .unwrap();
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&o.stderr),
        String::from_utf8_lossy(&o.stdout)
    );
    assert!(o.status.success(), "scan failed ({mode}): {log}");
    let read = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    common::parity::assert_parity(out.path());
    Docs {
        cdx: read(&cdx),
        spdx23: read(&spdx23),
        spdx3: read(&spdx3),
        log,
        _out: out,
    }
}

fn walk_strings<'a>(v: &'a Value, key: Option<&str>, out: &mut Vec<(Option<String>, &'a str)>) {
    match v {
        Value::String(s) => out.push((key.map(str::to_string), s)),
        Value::Array(a) => a.iter().for_each(|e| walk_strings(e, key, out)),
        Value::Object(o) => o.iter().for_each(|(k, e)| walk_strings(e, Some(k), out)),
        _ => {}
    }
}

/// Every PURL in a document, in any of the three formats, without
/// qualifiers. The scan root's file-tier component is left out.
fn purls(doc: &Value) -> BTreeSet<String> {
    let mut strings = Vec::new();
    walk_strings(doc, None, &mut strings);
    strings
        .into_iter()
        .filter(|(k, s)| {
            s.starts_with("pkg:")
                && matches!(
                    k.as_deref(),
                    Some("purl" | "referenceLocator" | "software_packageUrl")
                )
        })
        .map(|(_, s)| s.split('?').next().unwrap().to_string())
        .filter(|p| !p.starts_with("pkg:generic/file-tier"))
        .collect()
}

fn same_purls_everywhere(d: &Docs) -> BTreeSet<String> {
    let cdx = purls(&d.cdx);
    assert_eq!(cdx, purls(&d.spdx23), "SPDX 2.3 PURLs differ from CycloneDX");
    assert_eq!(cdx, purls(&d.spdx3), "SPDX 3 PURLs differ from CycloneDX");
    cdx
}

fn hackage(set: &BTreeSet<String>) -> BTreeSet<String> {
    set.iter()
        .filter(|p| p.starts_with("pkg:hackage/"))
        .cloned()
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// A document-scope waybill value: a CycloneDX `metadata.properties[]`
/// entry, or an SPDX annotation envelope carrying the field.
fn doc_value(doc: &Value, key: &str) -> Option<String> {
    if let Some(props) = doc["metadata"]["properties"].as_array() {
        return props
            .iter()
            .find(|p| p["name"] == key)
            .and_then(|p| p["value"].as_str().map(str::to_string));
    }
    let mut strings = Vec::new();
    walk_strings(doc, None, &mut strings);
    strings.into_iter().find_map(|(_, s)| {
        let env: Value = serde_json::from_str(s).ok()?;
        let field = env.get("field").or_else(|| env.get("type"))?;
        if field != key {
            return None;
        }
        match &env["value"] {
            Value::String(v) => Some(v.clone()),
            other => Some(other.to_string()),
        }
    })
}

/// The same value in all three formats.
fn value_everywhere(d: &Docs, key: &str) -> Option<String> {
    let cdx = doc_value(&d.cdx, key);
    assert_eq!(cdx, doc_value(&d.spdx23, key), "{key}: SPDX 2.3 differs");
    assert_eq!(cdx, doc_value(&d.spdx3, key), "{key}: SPDX 3 differs");
    cdx
}

fn edge(cdx: &Value, from_prefix: &str, to: &str) -> bool {
    cdx["dependencies"].as_array().into_iter().flatten().any(|d| {
        d["ref"].as_str().is_some_and(|r| r.starts_with(from_prefix))
            && d["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|t| t.as_str() == Some(to))
    })
}

fn fell_back(d: &Docs) -> bool {
    d.log.contains(FELL_BACK)
}

fn drop_record(d: &Docs) -> Option<Value> {
    value_everywhere(d, C244).map(|s| serde_json::from_str(&s).unwrap())
}

const A: &str = "pkg:hackage/waybill-fixture-a@0.1.0";
const B: &str = "pkg:hackage/waybill-fixture-b@0.1.0";
const C: &str = "pkg:hackage/waybill-fixture-c@0.1.0";
const DD: &str = "pkg:hackage/waybill-fixture-d@0.1.0";
const E: &str = "pkg:hackage/waybill-fixture-e@0.1.0";
const ROOT_HS: &str = "pkg:hackage/waybill-fixture-root@0.1.0";
const BASE: &str = "pkg:hackage/base";
const TEXT: &str = "pkg:hackage/text";

// ---------------- US1: reachability, root detection (FR-002) ----------------

#[test]
fn us1_f2_root_only_keeps_chain_drops_independent() {
    let d = scan(&fixture("f2-hpack-root-chain"), "root-only");
    assert!(!fell_back(&d), "a root package.yaml is root-level: {}", d.log);
    assert_eq!(value_everywhere(&d, C140).as_deref(), Some("root-only"));
    assert_eq!(hackage(&same_purls_everywhere(&d)), set(&[ROOT_HS, A, C, BASE]));
    assert!(edge(&d.cdx, ROOT_HS, A), "root → a");
    assert!(edge(&d.cdx, A, C), "a → c");
}

#[test]
fn us1_f2_strict_keeps_chain() {
    let d = scan(&fixture("f2-hpack-root-chain"), "strict");
    assert!(!fell_back(&d), "{}", d.log);
    assert_eq!(value_everywhere(&d, C140).as_deref(), Some("strict"));
    assert_eq!(hackage(&same_purls_everywhere(&d)), set(&[ROOT_HS, A, C, BASE]));
    assert!(edge(&d.cdx, A, C), "a → c");
}

#[test]
fn us1_f6_npm_nested_by_path_kept() {
    for mode in ["root-only", "strict"] {
        let d = scan(&fixture("f6-npm-path"), mode);
        let p = same_purls_everywhere(&d);
        assert!(p.contains("pkg:npm/waybill-fixture-lib@0.2.0"), "{mode}: {p:?}");
        // The versionless declared component (research R2) rides along too.
        assert!(p.contains("pkg:npm/waybill-fixture-lib"), "{mode}: {p:?}");
        assert!(
            edge(&d.cdx, "pkg:npm/waybill-fixture-root-js", "pkg:npm/waybill-fixture-lib@0.2.0"),
            "{mode}: root → lib"
        );
    }
}

/// Research R1's consequence for every ecosystem: root-level is decided by
/// directory, so a root manifest whose file name milestone 215's list does
/// not know is still root-level. `pubspec.yaml` is the dart reader's
/// main-module evidence path.
#[test]
fn us1_unlisted_manifest_name_is_root_level() {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(
        t.path().join("pubspec.yaml"),
        "name: waybill_fixture_root\nversion: 1.0.0\n",
    )
    .unwrap();
    std::fs::create_dir_all(t.path().join("nested")).unwrap();
    std::fs::write(
        t.path().join("nested/package.json"),
        r#"{"name":"waybill-fixture-nested","version":"1.0.0"}"#,
    )
    .unwrap();
    let d = scan(t.path(), "root-only");
    assert!(!fell_back(&d), "{}", d.log);
    let p = same_purls_everywhere(&d);
    assert!(!p.iter().any(|x| x.starts_with("pkg:npm/")), "nested npm project dropped: {p:?}");
}

// ---------------- US2: declared members (FR-003) ----------------

#[test]
fn us2_f3_cabal_project_sets_scope_without_root_package() {
    let d = scan(&fixture("f3-cabal-project"), "root-only");
    assert!(!fell_back(&d), "a root cabal.project sets the scope: {}", d.log);
    assert_eq!(hackage(&same_purls_everywhere(&d)), set(&[A, B, BASE, TEXT]));
}

#[test]
fn us2_f4_cabal_project_members_join_json_root() {
    let d = scan(&fixture("f4-cabal-project-json"), "root-only");
    let p = same_purls_everywhere(&d);
    assert_eq!(hackage(&p), set(&[A, B, BASE, TEXT]));
    assert!(p.contains("pkg:npm/waybill-fixture-root-js@1.0.0"), "{p:?}");
}

#[test]
fn us2_f5_stack_yaml_sets_scope() {
    let d = scan(&fixture("f5-stack"), "root-only");
    assert!(!fell_back(&d), "a root stack.yaml sets the scope: {}", d.log);
    assert_eq!(hackage(&same_purls_everywhere(&d)), set(&[A, B, BASE, TEXT]));
}

#[test]
fn us2_strict_ignores_declarations() {
    for tree in ["f3-cabal-project", "f5-stack"] {
        let d = scan(&fixture(tree), "strict");
        assert!(fell_back(&d), "{tree}: a declaration does not set strict's scope");
        assert_eq!(value_everywhere(&d, C140), None, "{tree}: no C140 on the fall-back");
    }
    let d = scan(&fixture("f4-cabal-project-json"), "strict");
    assert!(hackage(&same_purls_everywhere(&d)).is_empty());
    let rec = drop_record(&d).expect("strict records its drop");
    // Spec 1662: `tools/e` is outside `packages: local-packages/*`, so it is
    // not a project member and never reaches the scope filter.
    assert_eq!(rec["main-modules"], serde_json::json!([A, B]));
}

/// A declaration at the root is the root project's: its root-level
/// components (here a `flake.lock` input, `["."]`) stay in scope, even with
/// no root package (T032). Two packages, because with one the scan links the
/// flake's inputs to it and the edge alone keeps them (haskell-security-
/// advisories, with six, lost all three of its inputs).
#[test]
fn us2_declaration_only_scope_keeps_root_level_components() {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("cabal.project"), "packages: pkgs/*\n").unwrap();
    for n in ["a", "b"] {
        std::fs::create_dir_all(t.path().join("pkgs").join(n)).unwrap();
        std::fs::write(
            t.path().join("pkgs").join(n).join("package.yaml"),
            format!("name: waybill-fixture-{n}\nversion: 0.1.0\nlibrary:\n  dependencies: [base]\n"),
        )
        .unwrap();
    }
    std::fs::write(
        t.path().join("flake.lock"),
        r#"{"nodes":{"waybill-fixture-input":{"locked":{"lastModified":1700000000,"narHash":"sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","owner":"waybill-fixture-org","repo":"waybill-fixture-input","rev":"0123456789abcdef0123456789abcdef01234567","type":"github"},"original":{"owner":"waybill-fixture-org","repo":"waybill-fixture-input","type":"github"}},"root":{"inputs":{"waybill-fixture-input":"waybill-fixture-input"}}},"root":"root","version":7}"#,
    )
    .unwrap();
    let all = same_purls_everywhere(&scan(t.path(), "all"));
    let input: Vec<&String> = all.iter().filter(|p| p.starts_with("pkg:github/")).collect();
    assert_eq!(input.len(), 1, "control: the flake input is emitted under all: {all:?}");
    let d = scan(t.path(), "root-only");
    assert!(!fell_back(&d), "{}", d.log);
    let scoped = same_purls_everywhere(&d);
    assert!(scoped.contains(input[0]), "the root's flake input stays: {scoped:?}");
    assert!(scoped.contains(A), "{scoped:?}");
}

/// A component with no directory provenance and no edge cannot be attributed
/// to any project, so the scope keeps it: f5's Stack snapshot (T033).
#[test]
fn us2_unattributable_snapshot_component_is_kept() {
    let snapshot = "pkg:generic/stackage-lts-22.0@unspecified";
    let all = same_purls_everywhere(&scan(&fixture("f5-stack"), "all"));
    assert!(all.contains(snapshot), "control: emitted under all: {all:?}");
    for mode in ["root-only", "strict"] {
        let p = same_purls_everywhere(&scan(&fixture("f5-stack"), mode));
        assert!(p.contains(snapshot), "{mode}: {p:?}");
    }
}

// ---------------- US3: the drop is recorded (FR-004 to FR-006) ----------------

/// SC-003's positive control.
#[test]
fn us3_f1_positive_control_records_drop() {
    let d = scan(&fixture("f1-json-root"), "root-only");
    assert_eq!(
        value_everywhere(&d, C244).as_deref(),
        Some(
            r#"{"count":2,"main-modules":["pkg:hackage/waybill-fixture-a@0.1.0","pkg:hackage/waybill-fixture-b@0.1.0"],"reason":"project-discovery-scope"}"#
        )
    );
    assert_eq!(value_everywhere(&d, C104).as_deref(), Some("partial"));
    let reason = value_everywhere(&d, C105).unwrap_or_default();
    assert!(
        reason.contains("project-discovery-scope: 2 project(s) dropped from hackage"),
        "{reason}"
    );
    let c200: Value = serde_json::from_str(&value_everywhere(&d, C200).unwrap()).unwrap();
    let dropped: Vec<&str> = c200["manifests-dropped-by-scope"]
        .as_array()
        .expect("C200 lists the dropped manifests")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(dropped.len(), 2, "{c200}");
    assert!(dropped[0].ends_with("local-packages/a/package.yaml"), "{dropped:?}");
    assert!(dropped[1].ends_with("local-packages/b/package.yaml"), "{dropped:?}");
    let read: BTreeSet<&str> = c200["manifests-read"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(dropped.iter().all(|m| read.contains(m)), "still listed as read");
}

/// No `compositions[]` record claims a hackage component complete, on f1
/// (nothing hackage remains) and f2 (`d` dropped, the rest kept). The record
/// for the root's own dependency list is excluded: it is the same under
/// `all`, and the root's direct dependencies are all present. The `compositions` arm for the new
/// reason is pinned by `a_scope_dropped_ecosystem_is_degraded`.
#[test]
fn us3_cdx_compositions_not_complete_for_dropped_ecosystem() {
    for tree in ["f1-json-root", "f2-hpack-root-chain"] {
        let d = scan(&fixture(tree), "root-only");
        let root_ref = d.cdx["metadata"]["component"]["bom-ref"].clone();
        for c in d.cdx["compositions"].as_array().into_iter().flatten() {
            if c["aggregate"] != "complete" {
                continue;
            }
            // The root's claim about its own dependency list, identical
            // under `all`: the root's direct dependencies are all present.
            if c.get("assemblies").is_none() && c["dependencies"] == serde_json::json!([root_ref]) {
                continue;
            }
            let names_hackage = ["assemblies", "dependencies"].iter().any(|k| {
                c[*k].as_array()
                    .into_iter()
                    .flatten()
                    .any(|a| a.as_str().is_some_and(|s| s.starts_with("pkg:hackage/")))
            });
            assert!(!names_hackage, "{tree}: a complete record names hackage: {c}");
        }
    }
}

#[test]
fn us3_f6_no_drop_no_record() {
    let d = scan(&fixture("f6-npm-path"), "root-only");
    assert_eq!(value_everywhere(&d, C244), None);
    assert!(!value_everywhere(&d, C105).unwrap_or_default().contains("project-discovery-scope"));
}

#[test]
fn us3_f2_drop_one() {
    let d = scan(&fixture("f2-hpack-root-chain"), "root-only");
    let rec = drop_record(&d).expect("d is dropped");
    assert_eq!(rec["count"], 1);
    assert_eq!(rec["main-modules"], serde_json::json!([DD]));
}

/// Spec 1662 (#1685): `tools/e/package.yaml` is outside the root
/// `cabal.project`'s `packages: local-packages/*`. It used to reach root-only
/// and be recorded as a scope drop. It is now not a project member at all, as
/// a `.cabal` there already wasn't (#1032), so it's absent in every mode and
/// root-only has nothing to record. `us3_f2_drop_one` still covers the record.
#[test]
fn us3_f3_unlisted_package_is_not_a_member() {
    for mode in ["all", "root-only"] {
        let d = scan(&fixture("f3-cabal-project"), mode);
        let p = same_purls_everywhere(&d);
        assert!(!p.iter().any(|x| x.contains(E)), "{mode}: tools/e emitted: {p:?}");
        assert!(drop_record(&d).is_none(), "{mode}: nothing for root-only to drop");
    }
}

/// Research R7: C200's paths and the components' evidence paths are joined to
/// different spellings of the root when the scan path is a symlink.
#[test]
fn us3_symlinked_root_c200_matches() {
    let t = tempfile::tempdir().unwrap();
    let link = t.path().join("link");
    std::os::unix::fs::symlink(fixture("f1-json-root"), &link).unwrap();
    let d = scan(&link, "root-only");
    let c200: Value = serde_json::from_str(&value_everywhere(&d, C200).unwrap()).unwrap();
    assert_eq!(
        c200["manifests-dropped-by-scope"].as_array().map(Vec::len),
        Some(2),
        "{c200}"
    );
}

#[test]
fn us3_all_mode_has_none_of_it() {
    let d = scan(&fixture("f1-json-root"), "all");
    assert_eq!(value_everywhere(&d, C140), None);
    assert_eq!(value_everywhere(&d, C244), None);
    assert!(!value_everywhere(&d, C105).unwrap_or_default().contains("project-discovery-scope"));
    let c200: Value = serde_json::from_str(&value_everywhere(&d, C200).unwrap()).unwrap();
    assert!(c200.get("manifests-dropped-by-scope").is_none(), "{c200}");
}

// ---------------- US4: nothing else changes (FR-007) ----------------

#[test]
fn us4_no_root_and_no_declaration_falls_back() {
    let t = tempfile::tempdir().unwrap();
    let pkg = t.path().join("nested/x");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("package.yaml"),
        "name: waybill-fixture-x\nversion: 0.1.0\nlibrary:\n  dependencies: [base]\n",
    )
    .unwrap();
    let scoped = scan(t.path(), "root-only");
    assert!(fell_back(&scoped), "{}", scoped.log);
    assert_eq!(value_everywhere(&scoped, C140), None);
    assert_eq!(value_everywhere(&scoped, C244), None);
    let all = scan(t.path(), "all");
    assert_eq!(same_purls_everywhere(&scoped), same_purls_everywhere(&all));
}
