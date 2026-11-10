//! #1401 — a `--path` scan names its target after the scanned directory,
//! however the directory was spelled.
//!
//! Measured before the fix: `--path .`, `--path ./`, `--path ..` and
//! `--path sub/..` all named the target `filesystem-scan`, because
//! `Path::file_name()` is `None` when the last component is `.` or `..`.
//! The name reaches the placeholder subject (`pkg:generic/<name>@0.0.0`),
//! its `bom-ref` and CPE, and the SPDX 2.3 document namespace, so the
//! same directory produced two different documents depending on the
//! working directory the scan ran from.
//!
//! Every assertion is on the emitted documents.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

const DIR: &str = "waybill-fixture-path-target-name";

struct Docs {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    raw: String,
}

/// A directory with no manifest, so the root-selection ladder falls
/// through to the placeholder named after the target.
fn fixture() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join(DIR);
    std::fs::create_dir_all(dir.join("sub")).expect("create fixture");
    std::fs::write(dir.join("README.txt"), "waybill fixture\n").expect("write fixture");
    (tmp, dir)
}

fn scan(cwd: &Path, path: &str) -> Docs {
    let out = tempfile::tempdir().expect("output tempdir");
    let home = tempfile::tempdir().expect("home tempdir");
    let file = |name: &str| out.path().join(name);
    let output = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["sbom", "scan", "--offline", "--path", path])
        .args(["--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", file("cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", file("spdx23.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", file("spdx3.json").display()))
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path())
        .current_dir(cwd)
        .output()
        .expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan --path {path} from {} failed: {}",
        cwd.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let read = |name: &str| std::fs::read_to_string(file(name)).expect("read output");
    let parse = |raw: &str| serde_json::from_str::<Value>(raw).expect("parse output");
    let (cdx, spdx23, spdx3) = (read("cdx.json"), read("spdx23.json"), read("spdx3.json"));
    Docs {
        cdx: parse(&cdx),
        spdx23: parse(&spdx23),
        spdx3: parse(&spdx3),
        raw: [cdx, spdx23, spdx3].concat(),
    }
}

/// The target-derived identity of each format, as one comparable value.
fn identity(docs: &Docs) -> Value {
    let spdx23_root = docs.spdx23["documentDescribes"][0].clone();
    let spdx23_pkg = docs.spdx23["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .find(|p| p["SPDXID"] == spdx23_root)
        .expect("described package")
        .clone();
    let graph = docs.spdx3["@graph"].as_array().expect("@graph");
    let root_iri = graph
        .iter()
        .find(|e| e["type"] == "SpdxDocument")
        .expect("SpdxDocument")["rootElement"][0]
        .clone();
    let spdx3_pkg = graph
        .iter()
        .find(|e| e["spdxId"] == root_iri)
        .expect("root element");
    serde_json::json!({
        "cdx": docs.cdx["metadata"]["component"],
        "cdx_dependency_ref": docs.cdx["dependencies"][0]["ref"],
        "spdx23_name": docs.spdx23["name"],
        "spdx23_namespace": docs.spdx23["documentNamespace"],
        "spdx23_package": [spdx23_pkg["name"], spdx23_pkg["externalRefs"]],
        "spdx3_package": [spdx3_pkg["name"], spdx3_pkg["software_packageUrl"]],
    })
}

fn assert_named_after_dir(docs: &Docs, name: &str, spelled: &str) {
    let component = &docs.cdx["metadata"]["component"];
    assert_eq!(component["name"], name, "--path {spelled}: {component}");
    assert_eq!(
        component["purl"],
        format!("pkg:generic/{name}@0.0.0"),
        "--path {spelled}: {component}"
    );
}

#[test]
fn dot_spellings_name_the_directory_like_an_absolute_path() {
    let (_tmp, dir) = fixture();
    let absolute = scan(&dir, dir.to_str().expect("utf-8 tempdir"));
    assert_named_after_dir(&absolute, DIR, "<absolute>");
    let expected = identity(&absolute);

    let sub = dir.join("sub");
    for (cwd, spelled) in [(&dir, "."), (&dir, "./"), (&sub, ".."), (&dir, "sub/..")] {
        let docs = scan(cwd, spelled);
        assert_named_after_dir(&docs, DIR, spelled);
        assert_eq!(identity(&docs), expected, "--path {spelled}");
    }
}

#[test]
fn resolving_the_name_does_not_put_the_path_in_the_document() {
    let (_tmp, dir) = fixture();
    let docs = scan(&dir, ".");
    let parent = dir.parent().expect("parent").to_str().expect("utf-8");
    let canonical = dir.canonicalize().expect("canonicalize");
    let canonical_parent = canonical.parent().expect("parent").to_str().expect("utf-8");
    for leaked in [parent, canonical_parent] {
        assert!(
            !docs.raw.contains(leaked),
            "documents contain the scanned path's parent {leaked}"
        );
    }
}

/// The name the operator typed is kept: a symlink is named for the link,
/// as it was before #1401. `.` has no typed name, and the working
/// directory a process inherits is the physical one, so scanning a
/// symlinked directory from inside it names the link's target.
#[cfg(unix)]
#[test]
fn a_symlink_is_named_for_the_link_and_dot_inside_it_for_the_target() {
    let (tmp, dir) = fixture();
    let link = tmp.path().join("waybill-fixture-link");
    std::os::unix::fs::symlink(&dir, &link).expect("symlink");

    let docs = scan(tmp.path(), "waybill-fixture-link");
    assert_named_after_dir(&docs, "waybill-fixture-link", "waybill-fixture-link");

    let docs = scan(&link, ".");
    assert_named_after_dir(&docs, DIR, ". (inside the link)");
}
