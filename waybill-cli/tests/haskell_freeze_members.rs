//! Spec 1668 (#1744): a `cabal.project.freeze` governs every package its
//! sibling `cabal.project` lists, not only one in its own directory.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;
use std::process::Command;

use serde_json::Value;

const FREEZE: &str = "constraints: waybill-fixture-frozen ==4.0.0,\n             waybill-fixture-unused ==1.0.0\n";
const DEPS: &[&str] = &["waybill-fixture-frozen", "waybill-fixture-loose"];

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn cabal(dir: &Path, name: &str, deps: &[&str]) {
    let mut bd = String::from("base");
    for d in deps {
        bd.push_str(", ");
        bd.push_str(d);
    }
    write(
        &dir.join(format!("{name}.cabal")),
        &format!("cabal-version: 2.4\nname: {name}\nversion: 0.1.0\nlibrary\n  build-depends: {bd}\n"),
    );
}

/// A tree with a root freeze file and `cabal.project` = `packages`.
fn tree(packages: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("waybill-fixture-tree");
    write(&root.join("cabal.project"), &format!("packages: {packages}\n"));
    write(&root.join("cabal.project.freeze"), FREEZE);
    (t, root)
}

/// Scan in all three formats, assert parity, return the CycloneDX document.
fn scan(root: &Path) -> Value {
    let out = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let f = |n: &str| out.path().join(n);
    let o = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .env("HOME", home.path())
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(root)
        .args(["--no-deep-hash", "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", f("waybill.cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", f("waybill.spdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", f("waybill.spdx3.json").display()))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    common::parity::assert_parity(out.path());
    serde_json::from_slice(&std::fs::read(f("waybill.cdx.json")).unwrap()).unwrap()
}

/// Every component, nested ones and the metadata component included.
fn components(doc: &Value) -> Vec<Value> {
    fn walk(cs: &[Value], out: &mut Vec<Value>) {
        for c in cs {
            out.push(c.clone());
            if let Some(n) = c["components"].as_array() {
                walk(n, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(doc["components"].as_array().map(Vec::as_slice).unwrap_or(&[]), &mut out);
    if doc["metadata"]["component"].is_object() {
        out.push(doc["metadata"]["component"].clone());
    }
    out
}

fn named(doc: &Value, name: &str) -> Vec<Value> {
    components(doc).into_iter().filter(|c| c["name"] == name).collect()
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name).map(|p| p["value"].as_str().unwrap().to_string())
}

/// Names of the components with an edge to `target`.
fn users(doc: &Value, target: &Value) -> Vec<String> {
    let all = components(doc);
    let name_of = |r: &str| all.iter().find(|c| c["bom-ref"] == r).map(|c| c["name"].as_str().unwrap().to_string());
    let mut out: Vec<String> = doc["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["dependsOn"].as_array().is_some_and(|xs| xs.iter().any(|x| *x == target["bom-ref"])))
        .filter_map(|d| name_of(d["ref"].as_str().unwrap()))
        .collect();
    out.sort();
    out
}

/// SC-001: one pinned component, reached from `member`, no orphan reason.
fn assert_governed(doc: &Value, member: &str) {
    let frozen = named(doc, "waybill-fixture-frozen");
    assert_eq!(frozen.len(), 1, "one frozen component: {frozen:#?}");
    let f = &frozen[0];
    assert_eq!(f["version"], "4.0.0", "{f:#?}");
    assert_eq!(users(doc, f), vec![member.to_string()]);
    assert_eq!(prop(f, "waybill:orphan-reason"), None, "{f:#?}");
    assert!(prop(f, "waybill:source-files").unwrap().contains("cabal.project.freeze"), "{f:#?}");
}

#[test]
fn freeze_governs_subdirectory_member() {
    let (_t, root) = tree("app");
    cabal(&root.join("app"), "waybill-fixture-app", DEPS);
    assert_governed(&scan(&root), "waybill-fixture-app");
}

#[test]
fn freeze_governs_every_packages_form() {
    for packages in ["./app/", "app/waybill-fixture-app.cabal"] {
        let (_t, root) = tree(packages);
        cabal(&root.join("app"), "waybill-fixture-app", DEPS);
        assert_governed(&scan(&root), "waybill-fixture-app");
    }

    let (_t, root) = tree("libs/*");
    cabal(&root.join("libs/x"), "waybill-fixture-x", &[]);
    cabal(&root.join("libs/y"), "waybill-fixture-y", DEPS);
    assert_governed(&scan(&root), "waybill-fixture-y");

    let (_t, root) = tree("app");
    write(
        &root.join("app/package.yaml"),
        "name: waybill-fixture-app\nversion: 0.1.0\nlibrary:\n  dependencies: [base, waybill-fixture-frozen, waybill-fixture-loose]\n",
    );
    assert_governed(&scan(&root), "waybill-fixture-app");
}

/// FR-011: an unpinned dependency is emitted as before; the member's main
/// module gains only `sbom-tier = source`, as the `packages: .` control has.
#[test]
fn unpinned_member_dep_is_unchanged() {
    let (_t, root) = tree("app");
    cabal(&root.join("app"), "waybill-fixture-app", DEPS);
    let doc = scan(&root);
    let loose = named(&doc, "waybill-fixture-loose");
    assert_eq!(loose.len(), 1, "{loose:#?}");
    assert!(loose[0]["version"].is_null(), "{loose:#?}");
    assert_eq!(users(&doc, &loose[0]), vec!["waybill-fixture-app".to_string()]);

    let (_c, control) = tree(".");
    cabal(&control, "waybill-fixture-app", DEPS);
    let tier = |d: &Value| prop(&named(d, "waybill-fixture-app")[0], "waybill:sbom-tier");
    assert_eq!(tier(&doc), tier(&scan(&control)));
    assert_eq!(tier(&doc).as_deref(), Some("source"));
}

#[test]
fn unused_freeze_pin_stays_edgeless() {
    let (_t, root) = tree("app");
    cabal(&root.join("app"), "waybill-fixture-app", DEPS);
    let doc = scan(&root);
    let unused = named(&doc, "waybill-fixture-unused");
    assert_eq!(unused.len(), 1);
    assert_eq!(prop(&unused[0], "waybill:orphan-reason").as_deref(), Some("edgeless-lock-pin"));
}

/// An unlisted directory and a `../` entry are not governed or emitted.
#[test]
fn unlisted_directory_is_not_governed() {
    let (t, root) = tree("app ../outside");
    cabal(&root.join("app"), "waybill-fixture-app", DEPS);
    cabal(&root.join("other"), "waybill-fixture-other", &["waybill-fixture-frozen"]);
    cabal(&t.path().join("outside"), "waybill-fixture-outside", &["waybill-fixture-frozen"]);
    let doc = scan(&root);
    assert!(named(&doc, "waybill-fixture-other").is_empty());
    assert!(named(&doc, "waybill-fixture-outside").is_empty());
    assert_governed(&doc, "waybill-fixture-app");
}
