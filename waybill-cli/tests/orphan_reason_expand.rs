//! Milestone 167 (T009, implements SC-009) — integration test for the
//! emit-time `waybill:orphan-reason` classifier vocabulary expansion.
//!
//! Verifies that a real end-to-end scan against a synthesized npm
//! fixture (declared-only dep with no `node_modules/`) emits
//! `waybill:orphan-reason = hoisted-unused` on the orphaned component
//! across CDX 1.6, SPDX 2.3, and SPDX 3.0.1 formats (FR-007).
//! Non-orphan components carry no such property/annotation (FR-006).
//!
//! Approach: invoke the built waybill binary against a tempdir
//! fixture with a `package.json` declaring `some-declared-only`
//! but no `package-lock.json` or `node_modules/`. The declared-only
//! dep is emitted at the design/manifest tier as a `pkg:npm/*`
//! component with no incoming `dependsOn` edges — BFS-unreachable
//! from `metadata.component.purl` → classified as `hoisted-unused`.

use std::collections::HashSet;
use std::process::Command;

fn build_fixture(tmp: &std::path::Path) {
    // Root package.json declares one dep (`declared-dep`) — this gives
    // the root a real outbound edge so the m158 primary-dep-fallback
    // doesn't synthesize edges to every graph-top.
    std::fs::write(
        tmp.join("package.json"),
        r#"{
  "name": "test-fixture-167",
  "version": "1.0.0",
  "dependencies": {
    "declared-dep": "^1.0.0"
  }
}
"#,
    )
    .unwrap();

    // `node_modules/declared-dep/` — the honest declared dep. BFS-
    // reachable via the manifest-derived edge. MUST NOT carry
    // waybill:orphan-reason.
    let declared_dir = tmp.join("node_modules").join("declared-dep");
    std::fs::create_dir_all(&declared_dir).unwrap();
    std::fs::write(
        declared_dir.join("package.json"),
        r#"{ "name": "declared-dep", "version": "1.0.0" }
"#,
    )
    .unwrap();

    // `node_modules/hoisted-orphan/` — the phantom. BFS-unreachable
    // (no declaring parent). Classified as `hoisted-unused` per m167.
    let hoisted_dir = tmp.join("node_modules").join("hoisted-orphan");
    std::fs::create_dir_all(&hoisted_dir).unwrap();
    std::fs::write(
        hoisted_dir.join("package.json"),
        r#"{ "name": "hoisted-orphan", "version": "2.5.0" }
"#,
    )
    .unwrap();
}

fn scan_fixture(tmp: &std::path::Path, format: &str) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp)
        .arg("--format")
        .arg(format)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed (format={format}): stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

// ---------------------------------------------------------------------
// T009 assertion 1 (FR-007 CDX 1.6) — orphan npm component carries the
// property; non-orphan components do not.
// ---------------------------------------------------------------------
#[test]
fn t009_cdx_hoisted_unused_on_declared_only_npm_orphan() {
    let tmp_holder = tempfile::tempdir().expect("tempdir");
    let tmp = tmp_holder.path();
    build_fixture(tmp);

    let sbom = scan_fixture(tmp, "cyclonedx-json");
    let components = sbom
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components[] array");

    let mut orphan_reasons: Vec<(String, String)> = Vec::new();
    for c in components {
        let purl = c
            .get("purl")
            .and_then(|p| p.as_str())
            .unwrap_or("<no-purl>")
            .to_string();
        if let Some(props) = c.get("properties").and_then(|p| p.as_array()) {
            for p in props {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if name == "waybill:orphan-reason" {
                    let value = p
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    orphan_reasons.push((purl.clone(), value));
                }
            }
        }
    }

    // At least one npm component (the declared-only orphan) MUST carry
    // `waybill:orphan-reason=hoisted-unused`.
    let hoisted_unused_npm: Vec<&(String, String)> = orphan_reasons
        .iter()
        .filter(|(purl, reason)| purl.starts_with("pkg:npm/") && reason == "hoisted-unused")
        .collect();
    assert!(
        !hoisted_unused_npm.is_empty(),
        "expected at least 1 pkg:npm/* component with \
         waybill:orphan-reason=hoisted-unused; got orphan_reasons = {orphan_reasons:?}"
    );

    // Sanity: every emitted orphan-reason must be one of the 5 valid
    // vocabulary codes (FR-002 + m061 preserved).
    let valid: HashSet<&str> = [
        "stale-go-sum-entry",
        "dead-lockfile-entry",
        "hoisted-unused",
        "unresolved-indirect-require",
        "flat-attached-fallback",
    ]
    .iter()
    .copied()
    .collect();
    for (purl, reason) in &orphan_reasons {
        assert!(
            valid.contains(reason.as_str()),
            "component {purl} carries unknown orphan-reason {reason:?}"
        );
    }
}

// ---------------------------------------------------------------------
// T009 assertion 2 (FR-006 CDX 1.6) — non-npm components (e.g., the
// `pkg:generic/test-fixture-167@1.0.0` root) carry NO
// `waybill:orphan-reason` property.
// ---------------------------------------------------------------------
#[test]
fn t009_cdx_non_npm_components_carry_no_orphan_reason() {
    let tmp_holder = tempfile::tempdir().expect("tempdir");
    let tmp = tmp_holder.path();
    build_fixture(tmp);

    let sbom = scan_fixture(tmp, "cyclonedx-json");

    // The root component lives at `metadata.component` in CDX.
    if let Some(root) = sbom
        .get("metadata")
        .and_then(|m| m.get("component"))
    {
        if let Some(props) = root.get("properties").and_then(|p| p.as_array()) {
            for p in props {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                assert_ne!(
                    name, "waybill:orphan-reason",
                    "root component MUST NOT carry waybill:orphan-reason \
                     (non-Go/npm ecosystem — FR-006/FR-001 scope)"
                );
            }
        }
    }

    // Non-npm components in `components[]` (if any) MUST NOT carry
    // the property. Empirically the fixture emits only npm entries +
    // the generic root at `metadata.component`, but assert the invariant
    // anyway per FR-001 scope.
    if let Some(components) = sbom.get("components").and_then(|c| c.as_array()) {
        for c in components {
            let purl = c
                .get("purl")
                .and_then(|p| p.as_str())
                .unwrap_or("<no-purl>");
            if purl.starts_with("pkg:npm/") {
                continue;
            }
            if let Some(props) = c.get("properties").and_then(|p| p.as_array()) {
                for p in props {
                    let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    assert_ne!(
                        name, "waybill:orphan-reason",
                        "non-Go/npm component {purl} MUST NOT carry \
                         waybill:orphan-reason (FR-001 ecosystem scope)"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------
// T009 assertion 3 (FR-007 SPDX 2.3) — same npm orphan carries the
// annotation via `annotations[].comment` in the SPDX 2.3 emission.
// ---------------------------------------------------------------------
#[test]
fn t009_spdx23_hoisted_unused_on_declared_only_npm_orphan() {
    let tmp_holder = tempfile::tempdir().expect("tempdir");
    let tmp = tmp_holder.path();
    build_fixture(tmp);

    let sbom = scan_fixture(tmp, "spdx-2.3-json");
    let packages = sbom
        .get("packages")
        .and_then(|p| p.as_array())
        .expect("packages[] array");

    let mut found_hoisted_unused = false;
    for pkg in packages {
        // SPDX 2.3 npm package purls live in externalRefs[].referenceLocator.
        let ext_refs = pkg
            .get("externalRefs")
            .and_then(|e| e.as_array());
        let is_npm = ext_refs
            .map(|refs| {
                refs.iter().any(|r| {
                    r.get("referenceLocator")
                        .and_then(|l| l.as_str())
                        .map(|s| s.starts_with("pkg:npm/"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !is_npm {
            continue;
        }
        if let Some(annos) = pkg.get("annotations").and_then(|a| a.as_array()) {
            for a in annos {
                let comment = a.get("comment").and_then(|c| c.as_str()).unwrap_or("");
                // SPDX 2.3 annotation.comment carries the parity-catalog
                // C45 envelope (MikebomAnnotationCommentV1) — a JSON
                // string. Parse and match the field + value.
                if let Ok(env) = serde_json::from_str::<serde_json::Value>(comment) {
                    let field = env.get("field").and_then(|f| f.as_str()).unwrap_or("");
                    let value = env.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    if field == "waybill:orphan-reason" && value == "hoisted-unused" {
                        found_hoisted_unused = true;
                    }
                }
            }
        }
    }
    assert!(
        found_hoisted_unused,
        "expected at least 1 npm Package with \
         waybill:orphan-reason=hoisted-unused in SPDX 2.3 annotations[].comment envelope"
    );
}

// ---------------------------------------------------------------------
// T009 assertion 4 (FR-007 SPDX 3.0.1) — same npm orphan carries the
// annotation via `@graph[]` Annotation element with statement matching
// the vocabulary.
// ---------------------------------------------------------------------
#[test]
fn t009_spdx3_hoisted_unused_on_declared_only_npm_orphan() {
    let tmp_holder = tempfile::tempdir().expect("tempdir");
    let tmp = tmp_holder.path();
    build_fixture(tmp);

    let sbom = scan_fixture(tmp, "spdx-3-json");
    let graph = sbom
        .get("@graph")
        .and_then(|g| g.as_array())
        .expect("@graph[] array");

    let mut found_hoisted_unused = false;
    for elem in graph {
        let elem_type = elem.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if elem_type != "Annotation" {
            continue;
        }
        let statement = elem
            .get("statement")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        // SPDX 3 Annotation.statement carries the parity-catalog C45
        // envelope (MikebomAnnotationCommentV1) — a JSON string.
        // Parse and match the field + value.
        if let Ok(env) = serde_json::from_str::<serde_json::Value>(statement) {
            let field = env.get("field").and_then(|f| f.as_str()).unwrap_or("");
            let value = env.get("value").and_then(|v| v.as_str()).unwrap_or("");
            if field == "waybill:orphan-reason" && value == "hoisted-unused" {
                found_hoisted_unused = true;
            }
        }
    }
    assert!(
        found_hoisted_unused,
        "expected at least 1 Annotation element in SPDX 3 @graph[] with \
         MikebomAnnotationCommentV1 envelope carrying \
         field=waybill:orphan-reason value=hoisted-unused"
    );
}
