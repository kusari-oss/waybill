//! #1626: a root override whose PURL equals a manifest main module's.
//!
//! FR-011 (`root_selector::apply_main_module_drop_or_demote`) drops such a
//! module because it IS the subject. Under an override the root's ref is
//! `<name>@<version>`, not the PURL, so the module's edges were left keyed
//! by a PURL that named nothing. In CycloneDX they dangled in
//! `dependencies[]`; SPDX 2.3 and SPDX 3 dropped them. Either way the
//! root reached none of its dependencies.
//!
//! Each case checks, in all three formats, that the root reaches every
//! package and that no edge names an endpoint the document lacks.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

fn write_cargo_fixture(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nwaybill-fixture-dep = \"2.0.0\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.rs"), "").unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        r#"version = 3

[[package]]
name = "waybill-fixture-app"
version = "1.0.0"
dependencies = [
 "waybill-fixture-dep",
]

[[package]]
name = "waybill-fixture-dep"
version = "2.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0000000000000000000000000000000000000000000000000000000000000000"
dependencies = [
 "waybill-fixture-leaf",
]

[[package]]
name = "waybill-fixture-leaf"
version = "3.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "1111111111111111111111111111111111111111111111111111111111111111"

[[package]]
name = "waybill-fixture-stray"
version = "4.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "2222222222222222222222222222222222222222222222222222222222222222"
"#,
    )
    .unwrap();
}

fn write_uv_fixture(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"waybill-fixture-uvapp\"\nversion = \"1.0.0\"\n\
         dependencies = [\"waybill-fixture-pydep>=2\"]\n",
    )
    .unwrap();
    std::fs::write(
        root.join("uv.lock"),
        r#"version = 1
requires-python = ">=3.10"

[[package]]
name = "waybill-fixture-pydep"
version = "2.0.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.invalid/waybill-fixture-pydep-2.0.0.tar.gz", hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000", size = 1 }

[[package]]
name = "waybill-fixture-uvapp"
version = "1.0.0"
source = { editable = "." }
dependencies = [
    { name = "waybill-fixture-pydep" },
]
"#,
    )
    .unwrap();
}

fn scan(root: &Path, format: &str, extra: &[&str]) -> serde_json::Value {
    let out = root.join(format!("out.{format}.json"));
    let output = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["sbom", "scan", "--path"])
        .arg(root)
        .args(["--format", format, "--no-deep-hash", "--offline", "--output"])
        .arg(&out)
        .args(extra)
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        .env_remove("WAYBILL_EXCLUDE_PATH")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap()
}

/// A document's dependency graph, by element `name@version` (the root and
/// a demoted main module can share a name): the root's key, the edges,
/// every other package's key, and the endpoints no element defines.
struct Graph {
    root: String,
    edges: HashMap<String, BTreeSet<String>>,
    packages: BTreeSet<String>,
    undefined_endpoints: BTreeSet<String>,
}

impl Graph {
    fn unreachable(&self) -> BTreeSet<String> {
        let mut seen = BTreeSet::from([self.root.clone()]);
        let mut queue = vec![self.root.clone()];
        while let Some(n) = queue.pop() {
            for t in self.edges.get(&n).into_iter().flatten() {
                if seen.insert(t.clone()) {
                    queue.push(t.clone());
                }
            }
        }
        self.packages.difference(&seen).cloned().collect()
    }
}

fn key(element: &serde_json::Value, version_field: &str) -> String {
    format!(
        "{}@{}",
        element["name"].as_str().unwrap_or(""),
        element[version_field].as_str().unwrap_or("")
    )
}

fn is_file_tier_cdx(c: &serde_json::Value) -> bool {
    c["bom-ref"].as_str().unwrap_or("").starts_with("pkg:generic/file-tier")
}

fn cdx_graph(d: &serde_json::Value) -> Graph {
    let root_ref = d["metadata"]["component"]["bom-ref"].as_str().unwrap().to_string();
    let mut name_of: HashMap<String, String> = HashMap::new();
    name_of.insert(root_ref.clone(), key(&d["metadata"]["component"], "version"));
    let mut packages = BTreeSet::new();
    for c in d["components"].as_array().unwrap() {
        if is_file_tier_cdx(c) {
            continue;
        }
        let r = c["bom-ref"].as_str().unwrap().to_string();
        let n = key(c, "version");
        packages.insert(n.clone());
        name_of.insert(r, n);
    }
    let mut edges: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut undefined_endpoints = BTreeSet::new();
    for dep in d["dependencies"].as_array().into_iter().flatten() {
        let from = dep["ref"].as_str().unwrap();
        let Some(from_name) = name_of.get(from) else {
            undefined_endpoints.insert(from.to_string());
            continue;
        };
        for t in dep["dependsOn"].as_array().into_iter().flatten() {
            let t = t.as_str().unwrap();
            if t.starts_with("pkg:generic/file-tier") {
                continue;
            }
            match name_of.get(t) {
                Some(n) => {
                    edges.entry(from_name.clone()).or_default().insert(n.clone());
                }
                None => {
                    undefined_endpoints.insert(t.to_string());
                }
            }
        }
    }
    Graph { root: name_of[&root_ref].clone(), edges, packages, undefined_endpoints }
}

fn spdx23_graph(d: &serde_json::Value) -> Graph {
    let mut name_of: HashMap<String, String> = HashMap::new();
    for p in d["packages"].as_array().unwrap() {
        name_of.insert(p["SPDXID"].as_str().unwrap().to_string(), key(p, "versionInfo"));
    }
    let root_id = d["documentDescribes"][0].as_str().unwrap().to_string();
    let root = name_of[&root_id].clone();
    let file_ids: BTreeSet<String> = d["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| f["SPDXID"].as_str().unwrap().to_string())
        .collect();
    let packages = name_of
        .iter()
        .filter(|(id, _)| **id != root_id)
        .map(|(_, n)| n.clone())
        .collect();
    let mut edges: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut undefined_endpoints = BTreeSet::new();
    for r in d["relationships"].as_array().unwrap() {
        let (a, b) = (
            r["spdxElementId"].as_str().unwrap(),
            r["relatedSpdxElement"].as_str().unwrap(),
        );
        let (from, to) = match r["relationshipType"].as_str().unwrap() {
            "DEPENDS_ON" | "CONTAINS" => (a, b),
            "DEPENDENCY_OF" | "DEV_DEPENDENCY_OF" | "OPTIONAL_DEPENDENCY_OF"
            | "BUILD_DEPENDENCY_OF" => (b, a),
            _ => continue,
        };
        if file_ids.contains(from) || file_ids.contains(to) {
            continue;
        }
        for x in [from, to] {
            if !name_of.contains_key(x) {
                undefined_endpoints.insert(x.to_string());
            }
        }
        if let (Some(f), Some(t)) = (name_of.get(from), name_of.get(to)) {
            edges.entry(f.clone()).or_default().insert(t.clone());
        }
    }
    Graph { root, edges, packages, undefined_endpoints }
}

fn spdx3_graph(d: &serde_json::Value) -> Graph {
    let graph = d["@graph"].as_array().unwrap();
    let mut name_of: HashMap<String, String> = HashMap::new();
    for e in graph {
        if e["type"] == "software_Package" {
            name_of.insert(
                e["spdxId"].as_str().unwrap().to_string(),
                key(e, "software_packageVersion"),
            );
        }
    }
    let defined: BTreeSet<String> = graph
        .iter()
        .filter_map(|e| e["spdxId"].as_str().map(str::to_string))
        .collect();
    let doc = graph.iter().find(|e| e["type"] == "SpdxDocument").unwrap();
    let root_iri = doc["rootElement"][0].as_str().unwrap().to_string();
    let root = name_of[&root_iri].clone();
    let packages = name_of
        .iter()
        .filter(|(id, _)| **id != root_iri)
        .map(|(_, n)| n.clone())
        .collect();
    let mut edges: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut undefined_endpoints = BTreeSet::new();
    for e in graph {
        let t = e["type"].as_str().unwrap_or("");
        if t != "Relationship" && t != "LifecycleScopedRelationship" {
            continue;
        }
        if !matches!(e["relationshipType"].as_str(), Some("dependsOn" | "contains")) {
            continue;
        }
        let from = e["from"].as_str().unwrap();
        for to in e["to"].as_array().unwrap() {
            let to = to.as_str().unwrap();
            for x in [from, to] {
                if !defined.contains(x) && !x.ends_with("NoAssertionElement") {
                    undefined_endpoints.insert(x.to_string());
                }
            }
            if let (Some(f), Some(t)) = (name_of.get(from), name_of.get(to)) {
                edges.entry(f.clone()).or_default().insert(t.clone());
            }
        }
    }
    Graph { root, edges, packages, undefined_endpoints }
}

/// The root's (name, version) in each format.
fn root_identities(
    cdx: &serde_json::Value,
    spdx23: &serde_json::Value,
    spdx3: &serde_json::Value,
) -> [(String, String); 3] {
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
    let m = &cdx["metadata"]["component"];
    let rid = &spdx23["documentDescribes"][0];
    let p23 = spdx23["packages"].as_array().unwrap().iter().find(|p| &p["SPDXID"] == rid).unwrap();
    let g = spdx3["@graph"].as_array().unwrap();
    let doc = g.iter().find(|e| e["type"] == "SpdxDocument").unwrap();
    let p3 = g.iter().find(|e| e["spdxId"] == doc["rootElement"][0]).unwrap();
    [
        (s(&m["name"]), s(&m["version"])),
        (s(&p23["name"]), s(&p23["versionInfo"])),
        (s(&p3["name"]), s(&p3["software_packageVersion"])),
    ]
}

/// Scan in all three formats and assert, in each, that the root's direct
/// dependencies are `direct`, that it reaches every package except
/// `unreachable`, that every edge endpoint is defined, and that the formats
/// name the root alike. Returns the CycloneDX root PURL.
///
/// The cargo fixture's `waybill-fixture-stray` is a lock entry nothing
/// depends on. A root with no edges of its own opens the root fallback
/// (`generate/root_edges.rs`), which attaches every top-level component,
/// the stray one included; so `direct` tells a root that inherited the
/// subject's edges from one that fell back.
fn assert_connected(
    root: &Path,
    extra: &[&str],
    direct: &[&str],
    unreachable: &[&str],
) -> String {
    let cdx = scan(root, "cyclonedx-json", extra);
    let spdx23 = scan(root, "spdx-2.3-json", extra);
    let spdx3 = scan(root, "spdx-3-json", extra);
    let [c, s2, s3] = root_identities(&cdx, &spdx23, &spdx3);
    assert_eq!((&c, &c), (&s2, &s3), "the formats must name the root alike");
    let graphs = [
        ("cyclonedx", cdx_graph(&cdx)),
        ("spdx-2.3", spdx23_graph(&spdx23)),
        ("spdx-3", spdx3_graph(&spdx3)),
    ];
    let direct: BTreeSet<String> = direct.iter().map(|s| s.to_string()).collect();
    let unreachable: BTreeSet<String> = unreachable.iter().map(|s| s.to_string()).collect();
    for (format, g) in &graphs {
        assert_eq!(
            g.edges.get(&g.root).cloned().unwrap_or_default(),
            direct,
            "{format}: the root's direct dependencies"
        );
        assert_eq!(
            g.unreachable(),
            unreachable,
            "{format}: the root `{}` must reach every other package; edges {:?}",
            g.root,
            g.edges
        );
        assert_eq!(
            g.undefined_endpoints,
            BTreeSet::new(),
            "{format}: every edge endpoint must be an element of the document"
        );
    }
    // FR-011: no component asserts the subject's own coordinate.
    let root_purl = cdx["metadata"]["component"]["purl"].as_str().unwrap().to_string();
    let same: Vec<&str> = cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["purl"].as_str() == Some(root_purl.as_str()))
        .filter_map(|c| c["bom-ref"].as_str())
        .collect();
    assert!(same.is_empty(), "components carrying the root's PURL {root_purl}: {same:?}");
    root_purl
}

#[test]
fn cargo_typed_root_equal_to_the_main_module_keeps_its_dependencies() {
    let tmp = tempfile::tempdir().unwrap();
    write_cargo_fixture(tmp.path());
    let purl = assert_connected(
        tmp.path(),
        &[
            "--root-name",
            "waybill-fixture-app",
            "--root-version",
            "1.0.0",
            "--root-purl-type",
            "cargo",
        ],
        &["waybill-fixture-dep@2.0.0"],
        &["waybill-fixture-stray@4.0.0"],
    );
    assert_eq!(purl, "pkg:cargo/waybill-fixture-app@1.0.0");
}

#[test]
fn full_root_purl_equal_to_the_main_module_keeps_its_dependencies() {
    let tmp = tempfile::tempdir().unwrap();
    write_cargo_fixture(tmp.path());
    let purl = assert_connected(
        tmp.path(),
        &["--root-purl", "pkg:cargo/waybill-fixture-app@1.0.0"],
        &["waybill-fixture-dep@2.0.0"],
        &["waybill-fixture-stray@4.0.0"],
    );
    assert_eq!(purl, "pkg:cargo/waybill-fixture-app@1.0.0");
}

#[test]
fn pypi_typed_root_equal_to_the_main_module_keeps_its_dependencies() {
    let tmp = tempfile::tempdir().unwrap();
    write_uv_fixture(tmp.path());
    let purl = assert_connected(
        tmp.path(),
        &[
            "--root-name",
            "waybill-fixture-uvapp",
            "--root-version",
            "1.0.0",
            "--root-purl-type",
            "pypi",
        ],
        &["waybill-fixture-pydep@2.0.0"],
        &[],
    );
    assert_eq!(purl, "pkg:pypi/waybill-fixture-uvapp@1.0.0");
}

/// Control: an override that does not collide demotes the module and
/// anchors the root to it (milestone 860). It passed before #1626 and
/// must still pass, so the fixture and the checks are not what fails.
#[test]
fn non_colliding_override_is_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    write_cargo_fixture(tmp.path());
    assert_connected(
        tmp.path(),
        &[
            "--root-name",
            "waybill-fixture-app",
            "--root-version",
            "9.9.9",
            "--root-purl-type",
            "cargo",
        ],
        &["waybill-fixture-app@1.0.0"],
        &["waybill-fixture-stray@4.0.0"],
    );
}

/// `--root-purl` sets the subject's name and version through the PURL
/// alone. The graph root's ref was built from the separate name and
/// version fields, `None` under it, so it fell back to the directory name
/// and matched no emitted root: the anchor to the demoted module hung off
/// a ref the document lacked, even with no collision.
#[test]
fn non_colliding_root_purl_keeps_the_root_anchored() {
    let tmp = tempfile::tempdir().unwrap();
    write_cargo_fixture(tmp.path());
    let purl = assert_connected(
        tmp.path(),
        &["--root-purl", "pkg:cargo/waybill-fixture-app@9.9.9"],
        &["waybill-fixture-app@1.0.0"],
        &["waybill-fixture-stray@4.0.0"],
    );
    assert_eq!(purl, "pkg:cargo/waybill-fixture-app@9.9.9");
}

/// An edge INTO the subject: a workspace member that depends on the root
/// crate. With the subject dropped, the edge must point at the root in
/// every format. SPDX 2.3 builds its relationships from the scan's edges
/// through `purl_aliases`, not from the anchored set, so this is the case
/// that exercises its alias; the root's own edges reach it via
/// `generate/root_edges.rs` either way.
#[test]
fn a_member_depending_on_the_subject_points_at_the_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("member/src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "").unwrap();
    std::fs::write(root.join("member/src/lib.rs"), "").unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"waybill-fixture-app\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nwaybill-fixture-dep = \"2.0.0\"\n\n[workspace]\nmembers = [\"member\"]\n",
    )
    .unwrap();
    std::fs::write(
        root.join("member/Cargo.toml"),
        "[package]\nname = \"waybill-fixture-member\"\nversion = \"0.5.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nwaybill-fixture-app = { path = \"..\" }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        r#"version = 3

[[package]]
name = "waybill-fixture-app"
version = "1.0.0"
dependencies = [
 "waybill-fixture-dep",
]

[[package]]
name = "waybill-fixture-dep"
version = "2.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0000000000000000000000000000000000000000000000000000000000000000"

[[package]]
name = "waybill-fixture-member"
version = "0.5.0"
dependencies = [
 "waybill-fixture-app",
]
"#,
    )
    .unwrap();
    let extra = [
        "--root-name",
        "waybill-fixture-app",
        "--root-version",
        "1.0.0",
        "--root-purl-type",
        "cargo",
    ];
    assert_connected(
        root,
        &extra,
        &["waybill-fixture-dep@2.0.0", "waybill-fixture-member@0.5.0"],
        &[],
    );
    let graphs = [
        ("cyclonedx", cdx_graph(&scan(root, "cyclonedx-json", &extra))),
        ("spdx-2.3", spdx23_graph(&scan(root, "spdx-2.3-json", &extra))),
        ("spdx-3", spdx3_graph(&scan(root, "spdx-3-json", &extra))),
    ];
    for (format, g) in &graphs {
        assert_eq!(
            g.edges.get("waybill-fixture-member@0.5.0").cloned().unwrap_or_default(),
            BTreeSet::from([g.root.clone()]),
            "{format}: the member's dependency on the subject is an edge to the root"
        );
    }
}
