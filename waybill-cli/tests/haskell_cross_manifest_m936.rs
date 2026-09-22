//! Issue #936 — two defects in the `.cabal` reader, both reproduced on a real
//! public project (cabal + hpack + Nix library with a nested example).
//!
//! 1. **A dependency declared in two manifests kept only one manifest's
//!    constraint.** The library required `base >=4.11 && <4.22`; the example
//!    required `base >=4.14 && <4.15`. One component was emitted carrying only
//!    the example's — which a consumer reads as the library's requirement.
//!    That is a wrong answer to a compatibility or vulnerability question, not
//!    merely an incomplete one.
//!
//!    The cause was not a missing array union, as it first appeared. The emit
//!    loop keyed design-tier components on PURL alone, so the SECOND component
//!    was discarded whole — constraint and manifest path together — before any
//!    union pass could see it. `cabal_paths` is sorted, so the file that sorts
//!    first silently defined the dependency for the whole repository.
//!
//! 2. **A `build-depends:` on a local package emitted a versionless phantom.**
//!    The example depends on the library; the library already emits as a main
//!    module with a real version. The self-reference filter checked only the
//!    manifest's OWN name, so the sibling reference produced a second,
//!    versionless component for the same package — the sole orphan in the
//!    graph, dragging `waybill:graph-completeness` to `partial` for a reason
//!    that was an artifact of the scan rather than a property of the project.
//!
//! The fixture mirrors that shape with synthetic package names.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

/// Root library + a nested example that depends on it. `waybill-fixture-dep`
/// is declared by BOTH, with DIFFERENT and non-overlapping constraints — the
/// property that made the loss visible. No lockfile, so everything is
/// design-tier and the m191 reconciler never fires.
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(
        r.join("waybill-fixture-lib.cabal"),
        "name: waybill-fixture-lib\nversion: 0.1\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends:\n      \
         waybill-fixture-dep >=4.11 && <4.22\n    , waybill-fixture-other >=1.0\n",
    ).unwrap();
    let ex = r.join("examples").join("readme");
    std::fs::create_dir_all(&ex).unwrap();
    std::fs::write(
        ex.join("readme.cabal"),
        "name: waybill-fixture-readme\nversion: 0.1.0.0\nlicense: BSD-3-Clause\n\n\
         executable readme\n  build-depends:       \
         waybill-fixture-dep >=4.14 && <4.15, waybill-fixture-lib\n",
    ).unwrap();
    d
}

fn scan(root: &Path) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("m936-{}-{seq}.json", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", "cyclonedx-json", "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// All values of `prop` on the component named `name`, parsed from the
/// JSON-array-in-string wire shape and unioned.
fn prop_values(v: &serde_json::Value, name: &str, prop: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for c in v["components"].as_array().unwrap() {
        if c["name"].as_str() != Some(name) { continue; }
        for p in c["properties"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            if p["name"].as_str() != Some(prop) { continue; }
            let raw = p["value"].as_str().unwrap_or_default();
            match serde_json::from_str::<Vec<String>>(raw) {
                Ok(items) => out.extend(items),
                Err(_) => { out.insert(raw.to_string()); }
            }
        }
    }
    out
}

fn refs_named(v: &serde_json::Value, name: &str) -> Vec<String> {
    v["components"].as_array().unwrap().iter()
        .filter(|c| c["name"].as_str() == Some(name))
        .map(|c| c["bom-ref"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Components with no inbound dependency edge and which are not the root.
fn orphans(v: &serde_json::Value) -> BTreeSet<String> {
    let all: BTreeSet<String> = v["components"].as_array().unwrap().iter()
        .map(|c| c["bom-ref"].as_str().unwrap_or_default().to_string()).collect();
    let root = v["metadata"]["component"]["bom-ref"].as_str().unwrap_or_default().to_string();
    let mut reached = BTreeSet::new();
    for d in v["dependencies"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        for t in d["dependsOn"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            reached.insert(t.as_str().unwrap_or_default().to_string());
        }
    }
    all.into_iter().filter(|r| r != &root && !reached.contains(r)).collect()
}

/// Defect 1 — both constraints survive, not whichever manifest sorts first.
#[test]
fn a_dep_declared_in_two_manifests_keeps_both_constraints_m936() {
    let d = fixture();
    let rep = scan(d.path());
    let ranges = prop_values(&rep, "waybill-fixture-dep", "waybill:requirement-ranges");

    assert!(ranges.contains(">=4.11 && <4.22"),
        "the LIBRARY's constraint `>=4.11 && <4.22` is missing. Only the \
         example's survived, so a consumer reads the example's much narrower \
         bound as the library's requirement. ranges={ranges:?}");
    assert!(ranges.contains(">=4.14 && <4.15"),
        "the EXAMPLE's constraint `>=4.14 && <4.15` is missing. ranges={ranges:?}");
    assert_eq!(ranges.len(), 2,
        "expected exactly the two declared constraints; ranges={ranges:?}");
}

/// Defect 1, second half — the manifest paths union too (m148's contract).
#[test]
fn a_dep_declared_in_two_manifests_cites_both_manifests_m936() {
    let d = fixture();
    let rep = scan(d.path());
    let files = prop_values(&rep, "waybill-fixture-dep", "waybill:source-files");

    let has = |needle: &str| files.iter().any(|f| f.ends_with(needle));
    assert!(has("waybill-fixture-lib.cabal"),
        "the library manifest is not cited as a source of this dependency, so \
         the surviving constraint cannot be attributed. files={files:?}");
    assert!(has("readme.cabal"),
        "the example manifest is not cited. files={files:?}");
}

/// Defect 2 — a `build-depends:` on a sibling package resolves to that
/// package's main module rather than minting a versionless second component.
#[test]
fn a_build_depends_on_a_local_package_is_not_a_phantom_m936() {
    let d = fixture();
    let rep = scan(d.path());
    let refs = refs_named(&rep, "waybill-fixture-lib");

    assert_eq!(refs.len(), 1,
        "the local package was emitted more than once. The versionless copy \
         comes from the example's `build-depends:` on it and is a phantom: the \
         package already emits as a main module carrying its real version. \
         refs={refs:?}");
    assert!(refs[0].contains("@0.1"),
        "the surviving component is the versionless phantom rather than the \
         real main module. refs={refs:?}");
}

/// Defect 2, consequence — the phantom was the sole orphan, so removing it
/// removes an incompleteness the project does not actually have.
#[test]
fn the_local_package_phantom_leaves_no_orphan_m936() {
    let d = fixture();
    let rep = scan(d.path());
    let orph = orphans(&rep);
    assert!(orph.is_empty(),
        "orphaned components remain, degrading graph-completeness for a reason \
         that is an artifact of the scan rather than a property of the \
         project: {orph:?}");
}

/// Guard against over-correcting defect 1. Keying per (PURL, manifest) must
/// not start emitting one component per STANZA within a single manifest — a
/// dependency named in both a library and a test-suite stanza of the same file
/// is one declaration site, and the pre-existing per-manifest union owns it.
#[test]
fn one_manifest_naming_a_dep_twice_still_yields_one_component_m936() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("solo.cabal"),
        "name: waybill-fixture-solo\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-dep >=1.0\n\n\
         test-suite spec\n  type: exitcode-stdio-1.0\n  \
         build-depends: waybill-fixture-dep >=1.0\n",
    ).unwrap();
    let rep = scan(d.path());

    let refs = refs_named(&rep, "waybill-fixture-dep");
    assert_eq!(refs.len(), 1,
        "a dependency named in two stanzas of ONE manifest split into multiple \
         components. refs={refs:?}");
}
