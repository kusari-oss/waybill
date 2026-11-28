//! Spec 1667: Haskell path values have the right shape.
//!
//! - US1 (#1463): C200 `waybill:hpack-read-outcome` names manifests relative
//!   to the scan root, in all three formats, with no host prefix.
//! - US2 (#1691): `--split`'s `source_dir` for a Haskell subproject is its
//!   directory, so `--split=directory` groups two packages that share one.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const C200: &str = "waybill:hpack-read-outcome";

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn pkg(root: &Path, dir: &str, name: &str) {
    let d = if dir.is_empty() { root.to_path_buf() } else { root.join(dir) };
    write(
        &d.join("package.yaml"),
        &format!("name: {name}\nversion: 0.1.0\nlibrary:\n  dependencies: [base]\n"),
    );
}

fn waybill(home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_waybill"));
    c.env("HOME", home).env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    c
}

/// Every string in C200's value that names a file, with its key.
fn c200_paths(v: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (k, xs) in v.as_object().unwrap() {
        for e in xs.as_array().unwrap() {
            match e {
                Value::String(s) => out.push((k.clone(), s.clone())),
                Value::Object(o) => {
                    for q in ["file", "manifest"] {
                        if let Some(s) = o.get(q).and_then(Value::as_str) {
                            out.push((format!("{k}.{q}"), s.to_string()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

#[test]
fn c200_has_no_host_path() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("waybill-fixture-root");
    write(
        &root.join("package.yaml"),
        "name: waybill-fixture-root\nversion: 0.1.0\nlibrary:\n  dependencies: !include \"deps.yaml\"\n",
    );
    write(&root.join("deps.yaml"), "[base]\n");
    write(
        &root.join("server/package.yaml"),
        "name: waybill-fixture-server\nversion: 0.1.0\nlibrary:\n  dependencies: !include \"../../outside.yaml\"\n",
    );
    write(&t.path().join("outside.yaml"), "[base]\n");

    let out = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let f = |n: &str| out.path().join(n);
    let o = waybill(home.path())
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(&root)
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

    // No format carries either spelling of the tempdir.
    let prefixes: Vec<String> = [t.path().to_path_buf(), std::fs::canonicalize(t.path()).unwrap()]
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    for name in ["waybill.cdx.json", "waybill.spdx.json", "waybill.spdx3.json"] {
        let text = std::fs::read_to_string(f(name)).unwrap();
        for p in &prefixes {
            assert!(!text.contains(p.as_str()), "{name} names the host path {p}");
        }
    }

    let doc: Value = serde_json::from_slice(&std::fs::read(f("waybill.cdx.json")).unwrap()).unwrap();
    let raw = doc["metadata"]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == C200)
        .expect("C200 is emitted")["value"]
        .as_str()
        .unwrap()
        .to_string();
    let v: Value = serde_json::from_str(&raw).unwrap();
    let paths = c200_paths(&v);
    assert!(!paths.is_empty(), "{v}");
    for (k, p) in &paths {
        assert!(!p.starts_with('/') && !p.starts_with(".."), "{k}: {p}");
    }
    assert_eq!(v["manifests-seen"], serde_json::json!(["package.yaml", "server/package.yaml"]), "{v}");

    // Research R3: an include above the root is refused, not read, so it
    // reaches C200 only as the entry that names it, with relative paths.
    let unresolved = v["includes-unresolved"].as_array().expect("the outside include is unresolved");
    assert_eq!(unresolved.len(), 1, "{v}");
    assert_eq!(unresolved[0]["reason"], "outside-root");
    assert_eq!(unresolved[0]["manifest"], "server/package.yaml");
    assert_eq!(unresolved[0]["file"], "server/package.yaml");
    assert_eq!(v["includes-resolved"][0]["manifest"], "package.yaml", "{v}");
}

/// The T002 tree: a root package, `local-packages/{a,c}`, and
/// `local-packages/d` holding a `package.yaml` beside a hand-written `.cabal`.
fn split_tree() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("waybill-fixture-tree");
    pkg(&root, "", "waybill-fixture-root");
    pkg(&root, "local-packages/a", "waybill-fixture-a");
    pkg(&root, "local-packages/c", "waybill-fixture-c");
    pkg(&root, "local-packages/d", "waybill-fixture-d");
    write(
        &root.join("local-packages/d/waybill-fixture-d2.cabal"),
        "cabal-version: 2.4\nname: waybill-fixture-d2\nversion: 0.1.0\nlibrary\n  build-depends: base\n",
    );
    (t, root)
}

/// `(subproject_id, source_dir)` per entry, plus the sub-SBOM files written.
fn split(root: &Path, mode: &str) -> (Vec<(String, String)>, usize) {
    let out = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let o = waybill(home.path())
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(root)
        .args(["--no-deep-hash", "--format", "cyclonedx-json", &format!("--split={mode}"), "--output-dir"])
        .arg(out.path())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let m: Value =
        serde_json::from_slice(&std::fs::read(out.path().join("split-manifest.json")).unwrap()).unwrap();
    let entries = m["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["subproject_id"].as_str().unwrap().to_string(), e["source_dir"].as_str().unwrap().to_string()))
        .collect();
    let sboms = std::fs::read_dir(out.path())
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".cdx.json"))
        .count();
    (entries, sboms)
}

#[test]
fn split_source_dir_is_a_directory() {
    let (_t, root) = split_tree();
    let (entries, _) = split(&root, "workspace");
    let dirs: std::collections::BTreeMap<String, String> = entries.into_iter().collect();
    assert_eq!(dirs["waybill-fixture-root.hackage"], "");
    assert_eq!(dirs["waybill-fixture-a.hackage"], "local-packages/a");
    assert_eq!(dirs["waybill-fixture-c.hackage"], "local-packages/c");
    assert_eq!(dirs["waybill-fixture-d.hackage"], "local-packages/d");
    assert_eq!(dirs["waybill-fixture-d2.hackage"], "local-packages/d");
    for d in dirs.values() {
        assert!(d.is_empty() || root.join(d).is_dir(), "{d} is not a directory");
    }
}

#[test]
fn two_main_modules_in_one_dir_group() {
    let (_t, root) = split_tree();
    let (_, workspace) = split(&root, "workspace");
    let (entries, directory) = split(&root, "directory");
    assert_eq!(workspace, 5, "one sub-SBOM per package");
    assert_eq!(directory, 4, "d and d2 share local-packages/d: {entries:?}");
}
