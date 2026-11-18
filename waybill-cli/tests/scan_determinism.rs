//! Spec 1542 (#1600): a scan of a fixed input gives the same document every
//! time, and no identification loses the file it came from.
//!
//! Contract `specs/1542-deterministic-binary-merge/contracts/wire.md`. Each
//! fixture is scanned at least [`SCANS`] times: the old code varied in 4 of
//! 12 scans of the trace's reproduction, so fewer scans could pass by luck.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;
#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use std::collections::BTreeMap;
use std::path::Path;

use common::installed_lib::{elf_shared_lib, scan_with, Docs};
use corpus_harness_195::layer2_golden::mask_nondeterministic;
use serde_json::Value;

const SCANS: usize = 20;

// ---------------------------------------------------------------------
// Repeat and compare (T002)
// ---------------------------------------------------------------------

fn scan_n(root: &Path, n: usize, extra: &[&str]) -> Vec<Docs> {
    (0..n).map(|_| scan_with("--path", root, extra)).collect()
}

/// The three documents, masked the way the corpus goldens are.
fn masked(d: &Docs) -> [String; 3] {
    [&d.cdx, &d.spdx23, &d.spdx3].map(|v| serde_json::to_string_pretty(&mask_nondeterministic(v)).unwrap())
}

fn completeness_reason(d: &Docs) -> String {
    d.cdx["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == "waybill:graph-completeness-reason")
        .and_then(|p| p["value"].as_str())
        .unwrap_or("-")
        .to_string()
}

/// Every value in a JSON document, as a sorted multiset of leaf paths with
/// array indices removed: equal for two documents that differ only in order.
fn order_insensitive(v: &Value) -> Vec<String> {
    fn walk(v: &Value, path: &str, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => m.iter().for_each(|(k, x)| walk(x, &format!("{path}/{k}"), out)),
            Value::Array(a) => a.iter().for_each(|x| walk(x, &format!("{path}/[]"), out)),
            leaf => out.push(format!("{path}={leaf}")),
        }
    }
    let mut out = Vec::new();
    walk(v, "", &mut out);
    out.sort();
    out
}

/// The first JSON path at which `a` and `b` differ, with both values.
fn first_difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                (Some(p), Some(q)) => first_difference(p, q, &format!("{path}/{k}")),
                _ => Some(format!("{path}/{k}: present in one only")),
            })
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (p, q))| first_difference(p, q, &format!("{path}[{i}]"))),
        _ if a == b => None,
        _ => Some(format!("{path}: {} | {}", truncate(a), truncate(b))),
    }
}

fn truncate(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 1200 { format!("{}…", &s[..1200]) } else { s }
}

/// One masked document per format, byte for byte with array order
/// included, and one completeness reason. A failure names the scans in
/// each variant and whether they differ only in order.
fn assert_repeatable(docs: &[Docs]) {
    let reasons: BTreeMap<String, Vec<usize>> = docs.iter().enumerate().fold(BTreeMap::new(), |mut m, (i, d)| {
        m.entry(completeness_reason(d)).or_default().push(i);
        m
    });
    assert_eq!(reasons.len(), 1, "the completeness reason varies across {} scans: {reasons:?}", docs.len());
    for (f, name) in ["CycloneDX", "SPDX 2.3", "SPDX 3"].iter().enumerate() {
        let mut variants: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, d) in docs.iter().enumerate() {
            variants.entry(masked(d)[f].clone()).or_default().push(i);
        }
        if variants.len() > 1 {
            let parsed: Vec<Value> = variants.keys().map(|s| serde_json::from_str(s).unwrap()).collect();
            let order_only = parsed.windows(2).all(|w| order_insensitive(&w[0]) == order_insensitive(&w[1]));
            let kind = if order_only { "only in array order" } else { "in content" };
            panic!(
                "{name}: {} variants across {} scans, differing {kind}; scans per variant: {:?}; first difference: {}",
                variants.len(),
                docs.len(),
                variants.values().collect::<Vec<_>>(),
                first_difference(&parsed[0], &parsed[1], "").unwrap_or_default()
            );
        }
    }
}

// ---------------------------------------------------------------------
// Fixtures (T003)
// ---------------------------------------------------------------------

/// An ELF shared library whose `.rodata` holds `paths` and a `tag` that
/// makes its bytes distinct.
fn lib(paths: &[&str], tag: &str) -> Vec<u8> {
    let mut rodata = format!("\0{tag}\0").into_bytes();
    for p in paths {
        rodata.extend_from_slice(p.as_bytes());
        rodata.push(0);
    }
    elf_shared_lib(&rodata)
}

/// A symlink chain `dir/base` → `base.<v1>` → … → `base.<vN>`, the last a
/// regular file holding `bytes`: a shared library's soname links.
fn chain(root: &Path, dir: &str, base: &str, versions: &[&str], bytes: &[u8]) {
    let d = root.join(dir);
    std::fs::create_dir_all(&d).unwrap();
    let names: Vec<String> = std::iter::once(base.to_string())
        .chain(versions.iter().map(|v| format!("{base}.{v}")))
        .collect();
    std::fs::write(d.join(names.last().unwrap()), bytes).unwrap();
    for w in names.windows(2) {
        std::os::unix::fs::symlink(&w[1], d.join(&w[0])).unwrap();
    }
}

fn bundled(name: &str) -> String {
    format!("/w/third_party/{name}/src.c")
}

// ---------------------------------------------------------------------
// Checkpoint: the comparison sees a real difference
// ---------------------------------------------------------------------

/// Control: two scans of one tree are equal, and a scan of a different tree
/// is reported as a content difference. Without this, a masking that hid
/// everything would make every repeatability case pass.
#[test]
fn control_the_comparison_detects_a_difference() {
    let a = tempfile::tempdir().unwrap();
    chain(a.path(), "lib", "libwaybill-fixture-c.so", &["1"], &lib(&[&bundled("waybill-fixture-one")], "a"));
    let same = scan_n(a.path(), 2, &[]);
    assert_repeatable(&same);

    let b = tempfile::tempdir().unwrap();
    chain(b.path(), "lib", "libwaybill-fixture-c.so", &["1"], &lib(&[&bundled("waybill-fixture-two")], "b"));
    let mixed = vec![same.into_iter().next().unwrap(), scan_n(b.path(), 1, &[]).remove(0)];
    let caught = std::panic::catch_unwind(|| assert_repeatable(&mixed));
    assert!(caught.is_err(), "two different trees must not compare equal after masking");
}

// ---------------------------------------------------------------------
// User Story 2: no identification loses its file (T004)
// ---------------------------------------------------------------------

use corpus_harness_195::invariants::checks::cdx_component_ref;
use corpus_harness_195::invariants::checks::orphan_explained::cdx_components_with_containers;

/// PURLs of file components (`?file-sha256=`) whose name is `name`, sorted.
fn file_components(d: &Docs, name: &str) -> Vec<String> {
    let mut v: Vec<String> = cdx_components_with_containers(&d.cdx)
        .into_iter()
        .filter_map(|(c, _)| c["purl"].as_str())
        .filter(|p| p.starts_with(&format!("pkg:generic/{name}?file-sha256=")))
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// I-D: every component of `purl` is nested, under a component that exists.
fn assert_nested(d: &Docs, purl: &str) {
    let found: Vec<(&Value, Option<&str>)> = cdx_components_with_containers(&d.cdx)
        .into_iter()
        .filter(|(c, _)| c["purl"] == purl)
        .collect();
    assert!(!found.is_empty(), "control: {purl} is identified");
    for (c, container) in found {
        assert!(container.is_some(), "{purl} is detached at the top level ({})", cdx_component_ref(c).unwrap_or("-"));
    }
}

const G: &str = "libwaybill-fixture-g.so";

/// D-1: the trace's shape. Two symlink chains, one file name at their head,
/// different bytes at their ends.
fn d1_tree(root: &Path) {
    chain(root, "lib/sdk", G, &["3", "3.0.0"], &lib(&[&bundled("waybill-fixture-gotcha")], "three"));
    chain(root, "lib/systems", G, &["2", "2.4.2"], &lib(&[&bundled("waybill-fixture-timemory")], "two"));
}

#[test]
fn d1_symlink_chains_to_different_bytes_stay_apart() {
    let dir = tempfile::tempdir().unwrap();
    d1_tree(dir.path());
    let docs = scan_n(dir.path(), SCANS, &[]);
    for d in &docs {
        assert_eq!(file_components(d, G).len(), 2, "one file component per hash: {:?}", file_components(d, G));
        assert_nested(d, "pkg:generic/waybill-fixture-gotcha");
        assert_nested(d, "pkg:generic/waybill-fixture-timemory");
    }
    assert_repeatable(&docs);
}

/// D-2: one name and the same bytes in two directories are one file.
#[test]
fn d2_one_name_same_bytes_is_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = lib(&[&bundled("waybill-fixture-shared")], "same");
    for d in ["lib/a", "lib/b"] {
        std::fs::create_dir_all(dir.path().join(d)).unwrap();
        std::fs::write(dir.path().join(d).join("libwaybill-fixture-s.so"), &bytes).unwrap();
    }
    let docs = scan_n(dir.path(), SCANS, &[]);
    for d in &docs {
        let files = file_components(d, "libwaybill-fixture-s.so");
        assert_eq!(files.len(), 1, "one file component: {files:?}");
        assert_nested(d, "pkg:generic/waybill-fixture-shared");
    }
    assert_repeatable(&docs);
}

/// D-3: one name, different bytes, no symlinks.
#[test]
fn d3_one_name_different_bytes_is_two_files() {
    let dir = tempfile::tempdir().unwrap();
    for (d, tree) in [("lib/a", "waybill-fixture-x"), ("lib/b", "waybill-fixture-y")] {
        std::fs::create_dir_all(dir.path().join(d)).unwrap();
        std::fs::write(dir.path().join(d).join("libwaybill-fixture-n.so"), lib(&[&bundled(tree)], d)).unwrap();
    }
    let docs = scan_n(dir.path(), SCANS, &[]);
    for d in &docs {
        assert_eq!(file_components(d, "libwaybill-fixture-n.so").len(), 2);
        assert_nested(d, "pkg:generic/waybill-fixture-x");
        assert_nested(d, "pkg:generic/waybill-fixture-y");
    }
    assert_repeatable(&docs);
}

/// D-7: the same bytes under two names are two files (the name is part of
/// a file component's identity), as before this feature.
#[test]
fn d7_same_bytes_different_names_are_two_files() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = lib(&[&bundled("waybill-fixture-twin")], "twin");
    std::fs::create_dir_all(dir.path().join("lib")).unwrap();
    std::fs::write(dir.path().join("lib/libwaybill-fixture-p.so"), &bytes).unwrap();
    std::fs::write(dir.path().join("lib/libwaybill-fixture-q.so"), &bytes).unwrap();
    let docs = scan_n(dir.path(), SCANS, &[]);
    for d in &docs {
        assert_eq!(file_components(d, "libwaybill-fixture-p.so").len(), 1);
        assert_eq!(file_components(d, "libwaybill-fixture-q.so").len(), 1);
        assert_nested(d, "pkg:generic/waybill-fixture-twin");
    }
    assert_repeatable(&docs);
}

// ---------------------------------------------------------------------
// User Story 1: the same answer every scan (T007)
// ---------------------------------------------------------------------

use corpus_harness_195::invariants::checks::cdx_prop;
use corpus_harness_195::invariants::checks::orphan_explained::{is_explained, reachable_with_containment};

/// Spec 1541's I-A in one scan: no reachable component carries C45, every
/// unreachable one is explained, and the completeness count equals the
/// non-file-tier unreachable population.
fn assert_one_answer(d: &Docs) -> usize {
    let reachable = reachable_with_containment(&d.cdx);
    let subject = d.cdx["metadata"]["component"]["bom-ref"].as_str();
    let mut orphans = 0usize;
    for (c, _) in cdx_components_with_containers(&d.cdx) {
        let Some(r) = cdx_component_ref(c) else { continue };
        if Some(r) == subject || cdx_prop(c, "waybill:component-tier") == Some("file") {
            continue;
        }
        if reachable.contains(r) {
            assert_eq!(cdx_prop(c, "waybill:orphan-reason"), None, "I-A: reachable {r} carries C45");
        } else {
            assert!(is_explained(c), "I-A: unreachable {r} is unexplained");
            orphans += 1;
        }
    }
    let stated = completeness_reason(d)
        .split(';')
        .find_map(|c| c.trim().strip_prefix("orphaned-components-detected: "))
        .map(|r| r.split_whitespace().next().unwrap().parse::<usize>().unwrap())
        .unwrap_or(0);
    assert_eq!(stated, orphans, "I-A: completeness count vs the I7 population");
    orphans
}

/// An npm project at the scan root with an edge: its main module roots the
/// graph and closes the root fallback, so the binaries are unreachable.
fn npm_app(root: &Path) {
    let pkg = serde_json::json!({ "name": "waybill-fixture-app", "version": "1.0.0", "dependencies": { "waybill-fixture-dep": "^1.0.0" } });
    std::fs::write(root.join("package.json"), pkg.to_string()).unwrap();
    let lock = serde_json::json!({
        "name": "waybill-fixture-app", "version": "1.0.0", "lockfileVersion": 3, "requires": true,
        "packages": {
            "": { "name": "waybill-fixture-app", "version": "1.0.0", "dependencies": { "waybill-fixture-dep": "^1.0.0" } },
            "node_modules/waybill-fixture-dep": { "version": "1.0.0", "resolved": "https://registry.npmjs.org/waybill-fixture-dep/-/waybill-fixture-dep-1.0.0.tgz", "integrity": "sha512-fake" }
        }
    });
    std::fs::write(root.join("package-lock.json"), lock.to_string()).unwrap();
}

/// D-4: D-1 behind an npm app. The binaries and what they identify are
/// unreachable; the answer about them is the same in every scan.
#[test]
fn d4_unreachable_binaries_answer_the_same_every_scan() {
    let dir = tempfile::tempdir().unwrap();
    d1_tree(dir.path());
    npm_app(dir.path());
    let docs = scan_n(dir.path(), SCANS, &[]);
    let counts: Vec<usize> = docs.iter().map(assert_one_answer).collect();
    assert!(counts.iter().all(|n| *n > 0), "control: the binaries are unreachable: {counts:?}");
    assert_repeatable(&docs);
}

/// D-6: one Debian package installed at two architectures. Milestone 1064
/// merges qualifier-only variants; the survivor is the same in every scan.
#[test]
fn d6_a_qualifier_only_merge_has_one_survivor() {
    let dir = tempfile::tempdir().unwrap();
    let status = "Package: waybill-fixture-lib\nStatus: install ok installed\nArchitecture: amd64\nMulti-Arch: same\nVersion: 1.0-1\n\n\
                  Package: waybill-fixture-lib\nStatus: install ok installed\nArchitecture: i386\nMulti-Arch: same\nVersion: 1.0-1\n";
    common::installed_lib::write(dir.path(), "var/lib/dpkg/status", status);
    common::installed_lib::write(dir.path(), "etc/os-release", "ID=debian\nVERSION_CODENAME=bookworm\n");
    let docs = scan_n(dir.path(), SCANS, &[]);
    let debs: Vec<String> = cdx_components_with_containers(&docs[0].cdx)
        .into_iter()
        .filter_map(|(c, _)| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:deb/"))
        .map(str::to_string)
        .collect();
    assert!(!debs.is_empty(), "control: the dpkg database is read");
    assert_repeatable(&docs);
}
