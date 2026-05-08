//! Closure-invariant regression test — milestone 084 FR-006 / FR-011.
//!
//! Asserts that every reference field in a mikebom-emitted CDX 1.6
//! document resolves to a declared `bom-ref` in the same document.
//! The CDX 1.6 schema's `refLinkType` is defined as "Descriptor for
//! an element identified by the attribute 'bom-ref' in the same BOM
//! document" — the schema doesn't enforce closure (no JSON-Schema
//! "must resolve" rule), but every reference-field description is a
//! contract that the value identifies an in-document element.
//!
//! Closure set:
//!     S = components[].bom-ref ∪ {metadata.component.bom-ref}
//!
//! Invariant:
//!     dependencies[].ref           ⊆ S
//!     dependencies[].dependsOn[]   ⊆ S
//!     compositions[].assemblies[]  ⊆ S
//!     compositions[].dependencies[] ⊆ S
//!
//! Plus per-user-story sharper assertions (US2 / US3):
//!   - Reverse-walk from any leaf terminates at exactly one node
//!     equal to `metadata.component.bom-ref` (US2).
//!   - The `incomplete_first_party_only` composition's assemblies
//!     and the trailing `complete` dep-completeness composition's
//!     dependencies both anchor on `metadata.component.bom-ref`
//!     (US3).
//!
//! When this test fails, each violation prints the offending field
//! path and ref value so root-cause is one log read away.

use std::collections::HashSet;
use std::process::Command;

mod common;
use common::normalize::apply_fake_home_env;
use common::workspace_root;

/// Per-ecosystem fixture row. Reuses the canonical fixture paths from
/// `common::CASES` in spirit, restricted to the post-053 ecosystems
/// where main-module promotion is in effect (Go, cargo, npm, pip,
/// gem, maven). Apk/deb/rpm are excluded because they exercise the
/// no-manifest fallback path (legitimate `<short-name>@0.0.0`
/// target_ref); FR-004 / US4 covers their byte-identity preservation
/// in a separate test (`cdx_regression`).
struct Fixture {
    label: &'static str,
    fixture_subpath: &'static str,
}

const FIXTURES: &[Fixture] = &[
    Fixture { label: "golang", fixture_subpath: "tests/fixtures/go/simple-module" },
    Fixture { label: "cargo",  fixture_subpath: "tests/fixtures/cargo/lockfile-v3" },
    Fixture { label: "npm",    fixture_subpath: "tests/fixtures/npm/node-modules-walk" },
    Fixture { label: "pip",    fixture_subpath: "tests/fixtures/python/simple-venv" },
    Fixture { label: "gem",    fixture_subpath: "tests/fixtures/gem/simple-bundle" },
    Fixture { label: "maven",  fixture_subpath: "tests/fixtures/maven/pom-three-deps" },
];

/// Run mikebom against a fixture and return the parsed CDX 1.6 JSON.
/// Optionally pass override flags to exercise the milestone-077
/// `--root-name` / `--root-version` path; when both are `None`, the
/// scan runs without overrides (default milestone-053+ main-module
/// emission).
fn run_mikebom_cdx_with_override(
    fixture_subpath: &str,
    root_name: Option<&str>,
    root_version: Option<&str>,
) -> serde_json::Value {
    let fixture = workspace_root().join(fixture_subpath);
    assert!(
        fixture.exists(),
        "fixture path missing: {}",
        fixture.display()
    );
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("mikebom.cdx.json");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let mut cmd = Command::new(bin);
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json");
    if let Some(name) = root_name {
        cmd.arg("--root-name").arg(name);
    }
    if let Some(version) = root_version {
        cmd.arg("--root-version").arg(version);
    }
    let output = cmd.output().expect("failed to invoke mikebom");
    assert!(
        output.status.success(),
        "mikebom failed for {}: stderr={}",
        fixture_subpath,
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out_path).expect("read output");
    serde_json::from_slice(&bytes).expect("parse CDX JSON")
}

fn run_mikebom_cdx(fixture_subpath: &str) -> serde_json::Value {
    run_mikebom_cdx_with_override(fixture_subpath, None, None)
}

/// Compute the closure set S = components[].bom-ref ∪ {metadata.component.bom-ref}.
fn declared_refs(cdx: &serde_json::Value) -> HashSet<String> {
    let mut s: HashSet<String> = HashSet::new();
    if let Some(meta) = cdx["metadata"]["component"]["bom-ref"].as_str() {
        s.insert(meta.to_string());
    }
    if let Some(arr) = cdx["components"].as_array() {
        for c in arr {
            if let Some(r) = c["bom-ref"].as_str() {
                s.insert(r.to_string());
            }
        }
    }
    // Services may also declare bom-refs (CDX 1.6 §services). Include
    // for completeness — empty for current mikebom output but
    // future-proof.
    if let Some(arr) = cdx["services"].as_array() {
        for c in arr {
            if let Some(r) = c["bom-ref"].as_str() {
                s.insert(r.to_string());
            }
        }
    }
    s
}

/// US1 / FR-006 — closure invariant: every refLinkType-typed value
/// resolves to a declared bom-ref.
fn assert_closure(cdx: &serde_json::Value, label: &str) {
    let s = declared_refs(cdx);
    let mut violations: Vec<(String, String)> = Vec::new();

    if let Some(deps) = cdx["dependencies"].as_array() {
        for (i, dep) in deps.iter().enumerate() {
            if let Some(r) = dep["ref"].as_str() {
                if !s.contains(r) {
                    violations.push((format!("dependencies[{i}].ref"), r.to_string()));
                }
            }
            if let Some(arr) = dep["dependsOn"].as_array() {
                for (j, d) in arr.iter().enumerate() {
                    if let Some(v) = d.as_str() {
                        if !s.contains(v) {
                            violations.push((
                                format!("dependencies[{i}].dependsOn[{j}]"),
                                v.to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    if let Some(comps) = cdx["compositions"].as_array() {
        for (i, c) in comps.iter().enumerate() {
            for (j, a) in c["assemblies"].as_array().unwrap_or(&vec![]).iter().enumerate() {
                if let Some(v) = a.as_str() {
                    if !s.contains(v) {
                        violations.push((
                            format!("compositions[{i}].assemblies[{j}]"),
                            v.to_string(),
                        ));
                    }
                }
            }
            for (j, d) in c["dependencies"].as_array().unwrap_or(&vec![]).iter().enumerate() {
                if let Some(v) = d.as_str() {
                    if !s.contains(v) {
                        violations.push((
                            format!("compositions[{i}].dependencies[{j}]"),
                            v.to_string(),
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ecosystem {label}: {} closure violations:\n{}",
        violations.len(),
        violations
            .iter()
            .map(|(p, v)| format!("  {p} = {v:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// US2 / SC-002 — reverse-walk: metadata.component.bom-ref is the
/// unique project root. Specifically:
///   1. It appears as the `ref` of at least one dependencies[] entry
///      (so the project is represented in the graph).
///   2. It is NOT a target of any other node's dependsOn[] (no node
///      "depends on" the project — the project is the source, not a
///      sink).
///
/// The pre-fix bug had `metadata.component.bom-ref` (the PURL) as a
/// dependsOn target of the orphan `<short-name>@0.0.0` ref. Post-fix,
/// nothing depends on the project root.
fn assert_reverse_walk_terminates_at_root(cdx: &serde_json::Value, label: &str) {
    let meta_ref = cdx["metadata"]["component"]["bom-ref"]
        .as_str()
        .expect("metadata.component.bom-ref")
        .to_string();

    // Collect (a) every value appearing as a dependencies[].ref, and
    // (b) every value appearing inside any dependencies[].dependsOn[].
    let mut refs_as_source: HashSet<String> = HashSet::new();
    let mut refs_as_target: HashSet<String> = HashSet::new();
    if let Some(deps) = cdx["dependencies"].as_array() {
        for dep in deps {
            if let Some(r) = dep["ref"].as_str() {
                refs_as_source.insert(r.to_string());
            }
            if let Some(arr) = dep["dependsOn"].as_array() {
                for d in arr {
                    if let Some(v) = d.as_str() {
                        refs_as_target.insert(v.to_string());
                    }
                }
            }
        }
    }

    // (1) The project root MUST appear as a dependencies[].ref. If it
    // doesn't, the project itself isn't in the dep graph.
    assert!(
        refs_as_source.contains(&meta_ref),
        "ecosystem {label}: metadata.component.bom-ref {meta_ref:?} does not appear as any dependencies[].ref"
    );

    // (2) The project root MUST NOT be a dependsOn target of any node.
    // Pre-fix, the orphan `<short-name>@0.0.0` had dependsOn = [PURL],
    // making the PURL a target. Post-fix, nothing points at the root.
    assert!(
        !refs_as_target.contains(&meta_ref),
        "ecosystem {label}: metadata.component.bom-ref {meta_ref:?} appears as a dependsOn target of some node — \
         the project root should be the source of edges, not a sink. \
         (This is the pre-fix orphan-bridge symptom: an orphan ref `<name>@0.0.0` has the PURL in its dependsOn[].)"
    );
}

/// US3 / VR-084-004 — the `incomplete_first_party_only` composition's
/// assemblies and the trailing `complete` dep-completeness composition's
/// dependencies both anchor on metadata.component.bom-ref.
fn assert_compositions_anchored_on_root(cdx: &serde_json::Value, label: &str) {
    let meta_ref = cdx["metadata"]["component"]["bom-ref"]
        .as_str()
        .expect("metadata.component.bom-ref")
        .to_string();

    let comps = cdx["compositions"]
        .as_array()
        .expect("compositions array");

    // Find the incomplete_first_party_only composition (when present).
    // Its assemblies[] should contain exactly one entry equal to the
    // root.
    let first_party = comps
        .iter()
        .find(|c| c["aggregate"].as_str() == Some("incomplete_first_party_only"));
    if let Some(c) = first_party {
        let assemblies: Vec<&str> = c["assemblies"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            assemblies.len(),
            1,
            "ecosystem {label}: incomplete_first_party_only.assemblies expected length 1, got {assemblies:?}"
        );
        assert_eq!(
            assemblies[0], meta_ref,
            "ecosystem {label}: incomplete_first_party_only.assemblies[0] {:?} != metadata.component.bom-ref {meta_ref:?}",
            assemblies[0]
        );
    }

    // Find the trailing dep-completeness composition (aggregate=complete,
    // assemblies absent or empty, dependencies = [root]). Distinguished
    // from the inventory complete composition by having only
    // `dependencies` populated.
    let dep_completeness = comps.iter().find(|c| {
        c["aggregate"].as_str() == Some("complete")
            && c["assemblies"]
                .as_array()
                .map(|a| a.is_empty())
                .unwrap_or(true)
            && c["dependencies"]
                .as_array()
                .map(|d| d.len() == 1)
                .unwrap_or(false)
    });
    if let Some(c) = dep_completeness {
        let deps: Vec<&str> = c["dependencies"]
            .as_array()
            .map(|d| d.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            deps.len(),
            1,
            "ecosystem {label}: trailing complete dep-completeness composition expected dependencies length 1, got {deps:?}"
        );
        assert_eq!(
            deps[0], meta_ref,
            "ecosystem {label}: trailing complete composition dependencies[0] {:?} != metadata.component.bom-ref {meta_ref:?}",
            deps[0]
        );
    }
}

#[test]
fn cdx_ref_closure_invariant_holds_per_ecosystem() {
    let mut failures: Vec<String> = Vec::new();
    for fx in FIXTURES {
        // Each ecosystem runs in a catch-unwind so one failure
        // doesn't mask the rest. The `assert!` macros in the
        // helpers panic on violation; we collect the panic message.
        let label = fx.label;
        let path = fx.fixture_subpath;
        let result = std::panic::catch_unwind(|| {
            let cdx = run_mikebom_cdx(path);
            assert_closure(&cdx, label);
            assert_reverse_walk_terminates_at_root(&cdx, label);
            assert_compositions_anchored_on_root(&cdx, label);
        });
        match result {
            Ok(()) => eprintln!("ecosystem {label}: closure invariant + reverse-walk + compositions-anchor PASS"),
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| {
                        payload
                            .downcast_ref::<&'static str>()
                            .map(|s| (*s).to_string())
                    })
                    .unwrap_or_else(|| "<panic with no message>".to_string());
                failures.push(format!("[{label}] {msg}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} ecosystem(s) failed milestone-084 invariants:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Milestone 086 — assert that `--root-name` / `--root-version`
/// override doesn't silently drop dep edges. Pre-086 (alpha.24) the
/// override path's `target_ref` had no outgoing edges because
/// milestone-084 filtered out main-module-PURL-keyed relationships;
/// the `dependencies.rs:78-91` primary-dep fallback synthesized
/// edges only for "components nothing else depends on", losing
/// real direct deps that happened to be transitively depended on
/// by other components. Post-086 the relationships are REWRITTEN
/// (not filtered), preserving every edge under the override identity.
///
/// Test invariant: edge count from the project root MUST match
/// between override and non-override scans of the same fixture.
#[test]
fn override_preserves_direct_edge_count() {
    let mut failures: Vec<String> = Vec::new();

    for fx in FIXTURES {
        let label = fx.label;
        let path = fx.fixture_subpath;
        let result = std::panic::catch_unwind(|| {
            let no_override = run_mikebom_cdx(path);
            let with_override = run_mikebom_cdx_with_override(
                path,
                Some("test-override"),
                Some("9.9.9"),
            );

            // Closure invariant must hold under override too.
            assert_closure(&with_override, &format!("{label}/override"));

            // Project root identity post-override.
            let override_root = with_override["metadata"]["component"]["bom-ref"]
                .as_str()
                .expect("metadata.component.bom-ref under override")
                .to_string();
            assert_eq!(
                override_root, "test-override@9.9.9",
                "ecosystem {label}: override identity not propagated to metadata.component.bom-ref"
            );

            // Edge count from project root: invariant under override.
            let no_override_root = no_override["metadata"]["component"]["bom-ref"]
                .as_str()
                .expect("metadata.component.bom-ref")
                .to_string();
            let no_override_edges = count_dep_edges_from(&no_override, &no_override_root);
            let with_override_edges = count_dep_edges_from(&with_override, &override_root);

            assert_eq!(
                with_override_edges, no_override_edges,
                "ecosystem {label}: override path lost {} edges \
                 (no-override: {}, with-override: {}). \
                 milestone-086 regression — relationships must be REWRITTEN, not filtered.",
                no_override_edges - with_override_edges,
                no_override_edges,
                with_override_edges,
            );
        });
        match result {
            Ok(()) => eprintln!("ecosystem {label}: override-edge-preservation PASS"),
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| {
                        payload
                            .downcast_ref::<&'static str>()
                            .map(|s| (*s).to_string())
                    })
                    .unwrap_or_else(|| "<panic with no message>".to_string());
                failures.push(format!("[{label}] {msg}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} ecosystem(s) failed milestone-086 override-preservation invariant:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

fn count_dep_edges_from(cdx: &serde_json::Value, source_ref: &str) -> usize {
    cdx["dependencies"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .find(|d| d["ref"].as_str() == Some(source_ref))
        .and_then(|d| d["dependsOn"].as_array())
        .map(|a| a.len())
        .unwrap_or(0)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod sanity {
    use super::*;
    use serde_json::json;

    #[test]
    fn declared_refs_includes_metadata_and_components_and_services() {
        let cdx = json!({
            "metadata": { "component": { "bom-ref": "root" } },
            "components": [
                { "bom-ref": "a" },
                { "bom-ref": "b" }
            ],
            "services": [{ "bom-ref": "svc" }]
        });
        let s = declared_refs(&cdx);
        assert!(s.contains("root"));
        assert!(s.contains("a"));
        assert!(s.contains("b"));
        assert!(s.contains("svc"));
        assert_eq!(s.len(), 4);
    }

    #[test]
    #[should_panic(expected = "closure violations")]
    fn assert_closure_detects_dangling_dep_ref() {
        let cdx = json!({
            "metadata": { "component": { "bom-ref": "root" } },
            "components": [{ "bom-ref": "a" }],
            "dependencies": [{ "ref": "ghost", "dependsOn": ["a"] }],
            "compositions": []
        });
        assert_closure(&cdx, "test");
    }

    #[test]
    fn assert_closure_passes_clean_doc() {
        let cdx = json!({
            "metadata": { "component": { "bom-ref": "root" } },
            "components": [{ "bom-ref": "a" }],
            "dependencies": [
                { "ref": "root", "dependsOn": ["a"] },
                { "ref": "a", "dependsOn": [] }
            ],
            "compositions": [
                { "aggregate": "complete", "assemblies": ["a"], "dependencies": ["a"] },
                { "aggregate": "incomplete_first_party_only", "assemblies": ["root"] },
                { "aggregate": "complete", "dependencies": ["root"] }
            ]
        });
        assert_closure(&cdx, "test");
    }
}
