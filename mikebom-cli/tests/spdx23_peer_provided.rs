//! Milestone 178: SPDX 2.3 `PROVIDED_DEPENDENCY_OF` for npm peer deps
//! (Principle V native-first migration).
//!
//! Covers:
//!
//! * **US1 (P1)**: full-mode SPDX 2.3 emits `PROVIDED_DEPENDENCY_OF`
//!   reversed-direction for npm peer edges.
//! * **US2 (P1)**: basic-mode collapses peer edges to natural-direction
//!   `DEPENDS_ON` (pre-178 behavior preserved, m228 escape hatch
//!   respected).
//! * **US3 (P2)**: `mikebom:peer-edge-targets` annotation retained in
//!   both compat modes with byte-identical value; FR-007 bidirectional
//!   invariant (every annotation entry ↔ every emitted PROVIDED edge).
//!
//! Mirrors m175 (`design_tier_advisory.rs`) and m177
//! (`reachability_signal.rs`) integration-test scaffolding:
//! `assert_cmd`-based release-independent subprocess with
//! `apply_fake_home_env` for HOME isolation.

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;

/// Scan `path` under `--offline` with the given compat mode, return
/// parsed SPDX 2.3 SBOM. `compat` = `None` → default (Full); `Some("full")`
/// or `Some("basic")` → explicit flag value.
fn scan_spdx23(path: &Path, compat: Option<&str>) -> serde_json::Value {
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");

    let mut cmd = Command::new(bin());
    common::normalize::apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    if let Some(mode) = compat {
        cmd.arg("--spdx2-relationship-compat").arg(mode);
    }

    let output = cmd.output().expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed (exit={:?}): stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

/// Extract the value of the `mikebom:peer-edge-targets` annotation on
/// the Package whose `name` matches. Returns `None` if no such
/// annotation or Package.
fn peer_edge_annotation(sbom: &serde_json::Value, package_name: &str) -> Option<String> {
    let packages = sbom["packages"].as_array()?;
    let pkg = packages
        .iter()
        .find(|p| p["name"].as_str() == Some(package_name))?;
    let annotations = pkg["annotations"].as_array()?;
    for anno in annotations {
        let comment = anno["comment"].as_str()?;
        // Envelope: {schema, field, value}. Filter by field.
        let envelope: serde_json::Value = serde_json::from_str(comment).ok()?;
        if envelope["field"].as_str() == Some("mikebom:peer-edge-targets") {
            return Some(comment.to_string());
        }
    }
    None
}

/// Synthesize a minimal npm fixture with a peer edge. Per /speckit-analyze
/// U1: keep `provided-pkg` out of the ROOT `packages[""].dependencies`
/// so m147's "regular section wins over peer" Vacant precedence
/// (`package_lock.rs:220`) doesn't suppress the peer classification.
/// The peer walker resolves `provided-pkg` from the top-level
/// `node_modules/provided-pkg` per its walk-up algorithm.
fn write_npm_peer_fixture(dir: &Path) {
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"m178-peer-demo","version":"0.1.0","dependencies":{"consumer-pkg":"^1.0.0"}}"#,
    )
    .expect("write package.json");
    // Root packages[""] deliberately declares ONLY consumer-pkg —
    // provided-pkg is peer-only from consumer-pkg's perspective, so
    // m147's peer walker will resolve it via node_modules/provided-pkg
    // and add the annotation entry.
    let lock = r#"{
  "name": "m178-peer-demo",
  "version": "0.1.0",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {
      "name": "m178-peer-demo",
      "version": "0.1.0",
      "dependencies": {
        "consumer-pkg": "^1.0.0"
      }
    },
    "node_modules/consumer-pkg": {
      "version": "1.2.3",
      "peerDependencies": {
        "provided-pkg": "^2.0.0"
      }
    },
    "node_modules/provided-pkg": {
      "version": "2.0.0"
    }
  }
}"#;
    std::fs::write(dir.join("package-lock.json"), lock).expect("write package-lock.json");
}

/// Count `relationships[]` entries with the given relationshipType.
fn count_rels(sbom: &serde_json::Value, kind: &str) -> usize {
    sbom["relationships"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|r| r["relationshipType"].as_str() == Some(kind))
                .count()
        })
        .unwrap_or(0)
}

/// Return the `(spdxElementId, relatedSpdxElement)` tuples for
/// relationships of the given type.
fn rels_of_kind(sbom: &serde_json::Value, kind: &str) -> Vec<(String, String)> {
    sbom["relationships"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|r| r["relationshipType"].as_str() == Some(kind))
                .filter_map(|r| {
                    let src = r["spdxElementId"].as_str()?.to_string();
                    let tgt = r["relatedSpdxElement"].as_str()?.to_string();
                    Some((src, tgt))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Look up the SPDXID of the Package whose `name` matches.
fn spdxid_of_package(sbom: &serde_json::Value, package_name: &str) -> Option<String> {
    sbom["packages"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(package_name))
        .and_then(|p| p["SPDXID"].as_str().map(String::from))
}

// -----------------------------------------------------------------------
// US1 (P1) — Full-mode SPDX 2.3 emits `PROVIDED_DEPENDENCY_OF`.
// -----------------------------------------------------------------------

/// SC-001 — full-mode scan of npm-peer-fixture emits ≥1
/// `PROVIDED_DEPENDENCY_OF` relationship.
#[test]
fn t001_us1_full_mode_emits_provided_dependency_of() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_npm_peer_fixture(tmp.path());

    // Explicitly-full (should be identical to default; assert both).
    let sbom_full = scan_spdx23(tmp.path(), Some("full"));
    let sbom_default = scan_spdx23(tmp.path(), None);

    let full_count = count_rels(&sbom_full, "PROVIDED_DEPENDENCY_OF");
    let default_count = count_rels(&sbom_default, "PROVIDED_DEPENDENCY_OF");
    assert!(
        full_count >= 1,
        "SC-001: expected ≥1 PROVIDED_DEPENDENCY_OF under --spdx2-relationship-compat=full; got {full_count}. \
         peer-edge-targets on consumer-pkg: {:?}",
        peer_edge_annotation(&sbom_full, "consumer-pkg")
    );
    assert_eq!(
        default_count, full_count,
        "SC-001: default compat mode MUST equal explicit full mode (m228 default is Full)"
    );
}

/// SC-001 direction detail — the `PROVIDED_DEPENDENCY_OF` edge is
/// reversed relative to the internal `A DependsOn B` where B is in
/// A's peer-edge-targets. SPDX form: `B PROVIDED_DEPENDENCY_OF A`,
/// which serializes as `spdxElementId=B, relatedSpdxElement=A`.
#[test]
fn t002_us1_reversed_direction() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_npm_peer_fixture(tmp.path());

    let sbom = scan_spdx23(tmp.path(), None);

    let provided_edges = rels_of_kind(&sbom, "PROVIDED_DEPENDENCY_OF");
    assert!(
        !provided_edges.is_empty(),
        "US1: expected ≥1 PROVIDED_DEPENDENCY_OF edge; got 0. peer-edge-targets on consumer-pkg: {:?}",
        peer_edge_annotation(&sbom, "consumer-pkg")
    );

    let consumer_spdxid = spdxid_of_package(&sbom, "consumer-pkg")
        .expect("US1: consumer-pkg Package MUST exist in emitted SBOM");
    let provided_spdxid = spdxid_of_package(&sbom, "provided-pkg")
        .expect("US1: provided-pkg Package MUST exist in emitted SBOM");

    let expected_edge = (provided_spdxid.clone(), consumer_spdxid.clone());
    assert!(
        provided_edges.contains(&expected_edge),
        "US1 reversed-direction: expected edge (spdxElementId={provided_spdxid}, relatedSpdxElement={consumer_spdxid}) \
         which reads as `provided-pkg PROVIDED_DEPENDENCY_OF consumer-pkg` \
         (`provided-pkg is a provided dep of consumer-pkg`); got edges {provided_edges:?}"
    );
}

// -----------------------------------------------------------------------
// US2 (P1) — Basic-mode preserves `DEPENDS_ON`.
// -----------------------------------------------------------------------

/// SC-002 — basic-mode scan produces ZERO `PROVIDED_DEPENDENCY_OF`
/// relationships. Peer edges collapse to `DEPENDS_ON`.
#[test]
fn t003_us2_basic_mode_collapses_to_depends_on() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_npm_peer_fixture(tmp.path());

    let sbom = scan_spdx23(tmp.path(), Some("basic"));

    let provided_count = count_rels(&sbom, "PROVIDED_DEPENDENCY_OF");
    assert_eq!(
        provided_count, 0,
        "SC-002: basic mode MUST emit 0 PROVIDED_DEPENDENCY_OF; got {provided_count}"
    );

    // Peer edge must still exist somewhere — as natural-direction DEPENDS_ON.
    let depends_edges = rels_of_kind(&sbom, "DEPENDS_ON");
    assert!(
        !depends_edges.is_empty(),
        "SC-002: basic mode MUST retain peer edges as DEPENDS_ON; got 0 edges"
    );
}

/// SC-002 direction detail — under basic mode, peer edge is natural
/// direction: `consumer-pkg DEPENDS_ON provided-pkg`.
#[test]
fn t004_us2_basic_mode_direction_natural() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_npm_peer_fixture(tmp.path());

    let sbom = scan_spdx23(tmp.path(), Some("basic"));

    let consumer_spdxid = spdxid_of_package(&sbom, "consumer-pkg")
        .expect("US2: consumer-pkg Package MUST exist");
    let provided_spdxid = spdxid_of_package(&sbom, "provided-pkg")
        .expect("US2: provided-pkg Package MUST exist");

    let depends_edges = rels_of_kind(&sbom, "DEPENDS_ON");
    let expected_edge = (consumer_spdxid.clone(), provided_spdxid.clone());
    assert!(
        depends_edges.contains(&expected_edge),
        "US2 natural-direction: expected edge (spdxElementId={consumer_spdxid}, relatedSpdxElement={provided_spdxid}) \
         which reads as `consumer-pkg DEPENDS_ON provided-pkg`; got edges {depends_edges:?}"
    );
}

// -----------------------------------------------------------------------
// US3 (P2) — Annotation retained + FR-007 bidirectional invariant.
// -----------------------------------------------------------------------

/// SC-004 core — annotation present on consumer-pkg in BOTH modes.
#[test]
fn t005_us3_annotation_present_both_modes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_npm_peer_fixture(tmp.path());

    let sbom_full = scan_spdx23(tmp.path(), Some("full"));
    let sbom_basic = scan_spdx23(tmp.path(), Some("basic"));

    let anno_full = peer_edge_annotation(&sbom_full, "consumer-pkg");
    let anno_basic = peer_edge_annotation(&sbom_basic, "consumer-pkg");
    assert!(
        anno_full.is_some(),
        "SC-004: mikebom:peer-edge-targets MUST be present on consumer-pkg under full mode"
    );
    assert!(
        anno_basic.is_some(),
        "SC-004: mikebom:peer-edge-targets MUST be present on consumer-pkg under basic mode"
    );
}

/// SC-004 detail — annotation comment field is byte-identical across
/// full and basic compat modes.
#[test]
fn t006_us3_annotation_value_byte_identical_across_modes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_npm_peer_fixture(tmp.path());

    let sbom_full = scan_spdx23(tmp.path(), Some("full"));
    let sbom_basic = scan_spdx23(tmp.path(), Some("basic"));

    let anno_full = peer_edge_annotation(&sbom_full, "consumer-pkg")
        .expect("full mode annotation MUST exist");
    let anno_basic = peer_edge_annotation(&sbom_basic, "consumer-pkg")
        .expect("basic mode annotation MUST exist");
    assert_eq!(
        anno_full, anno_basic,
        "SC-004: annotation comment MUST be byte-identical across compat modes"
    );
}

/// SC-005 / FR-007 bidirectional invariant — under full mode:
///
/// * every (source_purl, target_purl) tuple in `mikebom:peer-edge-targets`
///   annotations MUST correspond to exactly one PROVIDED_DEPENDENCY_OF
///   edge (accounting for the direction reversal — SPDX
///   `relatedSpdxElement` maps to annotation-source PURL, SPDX
///   `spdxElementId` maps to annotation-target PURL).
/// * conversely, every PROVIDED_DEPENDENCY_OF edge MUST have its
///   (annotation-source, annotation-target) present in some source
///   Package's annotation.
///
/// Assertion: the two sets are byte-equal (as sorted vectors).
#[test]
fn t007_us3_fr007_bidirectional_invariant() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_npm_peer_fixture(tmp.path());

    let sbom = scan_spdx23(tmp.path(), Some("full"));

    // Build SPDXID → PURL map (for going from edge SPDXIDs back to
    // PURLs to compare against annotations, which are PURL-valued).
    let mut spdxid_to_purl: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for pkg in sbom["packages"].as_array().expect("packages array") {
        let Some(spdxid) = pkg["SPDXID"].as_str() else { continue };
        // PURL lives in externalRefs[type=purl].referenceLocator.
        if let Some(refs) = pkg["externalRefs"].as_array() {
            for r in refs {
                if r["referenceType"].as_str() == Some("purl") {
                    if let Some(purl) = r["referenceLocator"].as_str() {
                        spdxid_to_purl.insert(spdxid.to_string(), purl.to_string());
                    }
                }
            }
        }
    }

    // Forward index: (source_purl, target_purl) tuples from annotations.
    let mut anno_pairs: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for pkg in sbom["packages"].as_array().expect("packages array") {
        let Some(annotations) = pkg["annotations"].as_array() else { continue };
        let Some(spdxid) = pkg["SPDXID"].as_str() else { continue };
        let Some(source_purl) = spdxid_to_purl.get(spdxid) else { continue };
        for anno in annotations {
            let Some(comment) = anno["comment"].as_str() else { continue };
            let Ok(envelope) = serde_json::from_str::<serde_json::Value>(comment) else {
                continue;
            };
            if envelope["field"].as_str() != Some("mikebom:peer-edge-targets") {
                continue;
            }
            // Value can be a native JSON array OR a JSON-encoded string.
            let targets: Vec<String> = match &envelope["value"] {
                serde_json::Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                serde_json::Value::String(s) => {
                    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
                }
                _ => continue,
            };
            for target_purl in targets {
                anno_pairs.insert((source_purl.clone(), target_purl));
            }
        }
    }

    // Edge index: (source_purl, target_purl) tuples derived from
    // PROVIDED_DEPENDENCY_OF edges — reversing the direction:
    // SPDX (spdxElementId, relatedSpdxElement) = (target_spdxid, source_spdxid)
    // → PURL tuple (source_purl, target_purl).
    let mut edge_pairs: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for r in sbom["relationships"].as_array().expect("relationships array") {
        if r["relationshipType"].as_str() != Some("PROVIDED_DEPENDENCY_OF") {
            continue;
        }
        let Some(target_spdxid) = r["spdxElementId"].as_str() else { continue };
        let Some(source_spdxid) = r["relatedSpdxElement"].as_str() else { continue };
        let Some(source_purl) = spdxid_to_purl.get(source_spdxid) else { continue };
        let Some(target_purl) = spdxid_to_purl.get(target_spdxid) else { continue };
        edge_pairs.insert((source_purl.clone(), target_purl.clone()));
    }

    assert_eq!(
        anno_pairs, edge_pairs,
        "SC-005 FR-007 bidirectional invariant: annotation-derived peer tuples MUST equal \
         edge-derived peer tuples. anno_pairs={anno_pairs:?}, edge_pairs={edge_pairs:?}"
    );
    assert!(
        !anno_pairs.is_empty(),
        "SC-005: expected ≥1 peer edge in the fixture; both sets are empty — fixture may not \
         be triggering m147 classifier"
    );
}
