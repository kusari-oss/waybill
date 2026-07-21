//! Go transitive-parity regression test — milestone 083 (issue #111).
//!
//! Fixture: kubernetes-sigs/cri-tools @ v1.32.0 (commit `b5cf674`).
//! Manifest + lockfile only per spec FR-002 + Q1. go.mod + go.sum
//! committed at the tagged release.

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "go";

/// **Cache-empty baseline** — pinned at the CI-reproducible state where
/// `$GOMODCACHE` is empty. Waybill's go reader has a 5-step ladder per
/// milestones 055 + 091 (`go mod graph` / `$GOMODCACHE` / proxy /
/// **go.sum flat fallback** / no-edges-fallback). With `--offline` and
/// an empty cache, step 5 (the milestone-091 go.sum-driven flat
/// fallback) claims every go.sum module steps 1–3 missed and augments
/// the main-module's `depends` list with flat root → transitive edges.
/// This recovers ~78 transitive edges that were dropped pre-091 (count
/// rose from 31 → 109 on the cri-tools fixture).
///
/// Real-world output on a developer's box with a populated module
/// cache will be 260+ edges (full per-transitive parent-child topology
/// from step 2); we pin the 109-edge offline-cache-empty count because
/// that's what CI sees and what `WAYBILL_REQUIRE_TRANSITIVE_PARITY=1`
/// must reproduce.
///
/// Closed by milestone 091 (go.sum-fallback step 5):
/// - Pre-091: 31 edges (direct-deps only — main-module → ~24 direct
///   deps from go.mod's non-`// indirect` require lines + ~7
///   inter-transitive cache hits).
/// - Post-091: 109 edges (~24 direct deps + ~85 root → transitive
///   edges synthesized from go.sum's flat closure via step 5).
/// - Post-194 US1 (issue #571): 110 edges (+1: main-module →
///   pkg:golang/stdlib@v* edge closing the stdlib-orphan gap).
const EXPECTED_WAYBILL_EDGE_COUNT: usize = 110;

const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // Direct deps from go.mod `require` block — synthesized into edges
    // from the main-module PURL by `build_main_module_entry`.
    (
        "pkg:golang/sigs.k8s.io/cri-tools",
        "pkg:golang/github.com/distribution/reference",
    ),
    (
        "pkg:golang/sigs.k8s.io/cri-tools",
        "pkg:golang/github.com/google/uuid",
    ),
    (
        "pkg:golang/sigs.k8s.io/cri-tools",
        "pkg:golang/github.com/onsi/ginkgo/v2",
    ),
    // Milestone 091 invariant — step-5 go.sum-fallback edge: a
    // transitive dep that was NOT a direct dep in cri-tools' go.mod
    // and was previously dropped in offline+cache-empty mode.
    // beorn7/perks is a transitive of prometheus libraries, not a
    // direct cri-tools dep — it's only reachable via go.sum.
    // Pre-091 waybill emitted no edge to this component; post-091
    // step 5 augments main-module's depends list.
    (
        "pkg:golang/sigs.k8s.io/cri-tools",
        "pkg:golang/github.com/beorn7/perks",
    ),
];

fn fixture() -> PathBuf {
    fixture_path(FIXTURE_SUBPATH)
}

#[test]
fn fixture_present() {
    let f = fixture();
    assert!(f.join("go.mod").exists(), "missing go.mod at {}", f.display());
    assert!(f.join("go.sum").exists(), "missing go.sum at {}", f.display());
}

#[test]
fn transitive_edges_match_baseline() {
    let mikebom_edges = run_mikebom(&fixture());
    assert_eq!(
        mikebom_edges.len(),
        EXPECTED_WAYBILL_EDGE_COUNT,
        "waybill edge count drifted from the alpha.24 baseline."
    );
    let edge_set: std::collections::HashSet<(String, String)> = mikebom_edges
        .iter()
        .map(|e| (strip_version(&e.from).to_string(), strip_version(&e.to).to_string()))
        .collect();
    for (from_prefix, to_prefix) in EXPECTED_REPRESENTATIVE_EDGES {
        assert!(
            edge_set.contains(&(from_prefix.to_string(), to_prefix.to_string())),
            "expected representative edge missing: {from_prefix} → {to_prefix}"
        );
    }
}

#[test]
fn cross_tool_parity_check() {
    if let Some(reason) = maybe_skip(&["trivy", "syft"]) {
        eprintln!("transitive_parity_go::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let waybill = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&waybill, &trivy, &syft);
    eprintln!("\n=== go audit (kubernetes-sigs/cri-tools @ v1.32.0) ===");
    eprintln!(
        "edge counts: waybill={} trivy={} syft={}",
        waybill.len(),
        trivy.len(),
        syft.len()
    );
    eprintln!("diff:\n{}", format_edge_diff(&diff));
}

fn strip_version(purl: &str) -> &str {
    match purl.rfind('@') {
        Some(i) => &purl[..i],
        None => purl,
    }
}

// ============================================================
// Milestone 112 (T011) — C60 `waybill:build-inclusion` cross-format
// consistency on the cri-tools fixture.
//
// Offline + cache-empty, step 5 (the milestone-091 go.sum flat
// fallback) claims every module steps 1–3 missed — those components
// carry `waybill:resolver-step: go-sum-fallback` and therefore the
// milestone-112 `waybill:build-inclusion: unknown` marker. The
// marked PURL set must be non-empty and IDENTICAL across CDX 1.6,
// SPDX 2.3, and SPDX 3 (catalog row C60, SymmetricEqual).
// ============================================================

#[test]
fn build_inclusion_marker_cross_format_consistency() {
    use std::process::Command;

    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let spdx23_path = tmp.path().join("out.spdx.json");
    let spdx3_path = tmp.path().join("out.spdx3.json");
    let fake_home = tempfile::tempdir().expect("fake-home");
    let mut cmd = Command::new(bin);
    apply_fake_home_env(&mut cmd, fake_home.path());
    let output = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture())
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.to_string_lossy()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.to_string_lossy()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_path.to_string_lossy()))
        .arg("--no-deep-hash")
        .output()
        .expect("waybill invokes");
    assert!(
        output.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parse = |p: &std::path::Path| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(p).expect("read output")).expect("parse JSON")
    };
    let cdx = parse(&cdx_path);
    let spdx23 = parse(&spdx23_path);
    let spdx3 = parse(&spdx3_path);

    let cdx_marked = cdx_marked_purls(&cdx, "waybill:build-inclusion", "unknown");
    let spdx23_marked = spdx23_marked_purls(&spdx23, "waybill:build-inclusion", "unknown");
    let spdx3_marked = spdx3_marked_purls(&spdx3, "waybill:build-inclusion", "unknown");

    assert!(
        !cdx_marked.is_empty(),
        "offline + cache-empty cri-tools scan must produce go-sum-fallback \
         components carrying waybill:build-inclusion: unknown"
    );
    assert_eq!(
        cdx_marked, spdx23_marked,
        "C60 marked-component set must match between CDX and SPDX 2.3"
    );
    assert_eq!(
        cdx_marked, spdx3_marked,
        "C60 marked-component set must match between CDX and SPDX 3"
    );
}

// ============================================================
// Shared cross-format extraction helpers (C60/C61/C62) — given an
// annotation key + value, return the PURL set of components carrying
// it in each format's native carrier.
// ============================================================

fn is_annotation_envelope(comment: &str, field: &str, value: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(comment).is_ok_and(|env| {
        env["field"].as_str() == Some(field) && env["value"].as_str() == Some(value)
    })
}

/// CDX: PURLs of components carrying the property.
fn cdx_marked_purls(
    cdx: &serde_json::Value,
    field: &str,
    value: &str,
) -> std::collections::BTreeSet<String> {
    cdx["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter(|c| {
            c["properties"].as_array().into_iter().flatten().any(|p| {
                p["name"].as_str() == Some(field) && p["value"].as_str() == Some(value)
            })
        })
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect()
}

/// SPDX 2.3: PURLs (externalRefs purl locator) of packages with the
/// annotation envelope.
fn spdx23_marked_purls(
    spdx23: &serde_json::Value,
    field: &str,
    value: &str,
) -> std::collections::BTreeSet<String> {
    spdx23["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .filter(|p| {
            p["annotations"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a["comment"].as_str())
                .any(|c| is_annotation_envelope(c, field, value))
        })
        .filter_map(|p| {
            p["externalRefs"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["referenceType"].as_str() == Some("purl"))
                .and_then(|r| r["referenceLocator"].as_str())
                .map(String::from)
        })
        .collect()
}

/// SPDX 3: Annotation elements → subject spdxId → package PURL.
fn spdx3_marked_purls(
    spdx3: &serde_json::Value,
    field: &str,
    value: &str,
) -> std::collections::BTreeSet<String> {
    let graph = spdx3["@graph"].as_array().expect("@graph");
    let marked_ids: std::collections::BTreeSet<&str> = graph
        .iter()
        .filter(|e| e["type"].as_str() == Some("Annotation"))
        .filter(|e| {
            e["statement"]
                .as_str()
                .is_some_and(|c| is_annotation_envelope(c, field, value))
        })
        .filter_map(|e| e["subject"].as_str())
        .collect();
    graph
        .iter()
        .filter(|e| e["type"].as_str() == Some("software_Package"))
        .filter(|e| e["spdxId"].as_str().is_some_and(|id| marked_ids.contains(id)))
        .filter_map(|e| e["software_packageUrl"].as_str().map(String::from))
        .collect()
}

// ============================================================
// Milestone 112 (T019) — C61 `waybill:build-inclusion-derivation` +
// C62 `waybill:lifecycle-scope-derivation` cross-format consistency.
//
// The derivation discriminators are only emitted when the `go mod
// why -m` classification runs, so this test injects a stub `go`
// toolchain (same pattern as `go_build_inclusion.rs`) that answers
// not-needed for one module and test-only for another, then asserts
// the marked PURL sets are non-empty and IDENTICAL across CDX 1.6,
// SPDX 2.3, and SPDX 3 (catalog rows C61/C62, SymmetricEqual).
// ============================================================

#[cfg(unix)]
mod derivation_parity {
    use super::{apply_fake_home_env, cdx_marked_purls, spdx23_marked_purls, spdx3_marked_purls};
    use std::path::Path;
    use std::process::Command;

    const STUB_GO: &str = r##"#!/bin/sh
case "$1" in
  version) echo "go version go1.22.0 stub/test"; exit 0 ;;
  list)    exit 0 ;;
  mod)
    case "$2" in
      graph) exit 1 ;;
      why)
        shift 4   # drop: mod why -m -vendor
        for m in "$@"; do
          echo "# $m"
          case "$m" in
            *never-linked*)
              echo "(main module does not need module $m)" ;;
            *test-only*)
              echo "example.com/derivparity"
              echo "example.com/derivparity.test"
              echo "$m" ;;
            *)
              echo "example.com/derivparity"
              echo "$m" ;;
          esac
        done
        exit 0 ;;
    esac ;;
esac
exit 1
"##;

    fn write_fixture(root: &Path) {
        std::fs::write(
            root.join("go.mod"),
            "module example.com/derivparity\n\
             go 1.22\n\
             require (\n\
             \tgithub.com/never-linked/fake v9.9.9\n\
             \tgithub.com/test-only/dep v2.0.0\n\
             \tgithub.com/prod/dep v1.0.0\n\
             )\n",
        )
        .expect("write go.mod");
        std::fs::write(
            root.join("go.sum"),
            concat!(
                "github.com/never-linked/fake v9.9.9 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
                "github.com/test-only/dep v2.0.0 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
                "github.com/prod/dep v1.0.0 h1:fakeSHA/k1PS7AGV8HAP9mRGQ==\n",
            ),
        )
        .expect("write go.sum");
    }

    #[test]
    fn derivation_markers_cross_format_consistency() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = tempfile::tempdir().expect("fixture tempdir");
        write_fixture(fixture.path());

        let stub_dir = tempfile::tempdir().expect("stub tempdir");
        let stub_path = stub_dir.path().join("go");
        std::fs::write(&stub_path, STUB_GO).expect("write stub");
        std::fs::set_permissions(&stub_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod stub");

        let out = tempfile::tempdir().expect("output tempdir");
        let cdx_path = out.path().join("out.cdx.json");
        let spdx23_path = out.path().join("out.spdx.json");
        let spdx3_path = out.path().join("out.spdx3.json");
        let fake_home = tempfile::tempdir().expect("fake-home");
        let real_path = std::env::var("PATH").expect("PATH set");

        let bin = env!("CARGO_BIN_EXE_waybill");
        let mut cmd = Command::new(bin);
        apply_fake_home_env(&mut cmd, fake_home.path());
        let output = cmd
            .env("PATH", format!("{}:{real_path}", stub_dir.path().to_string_lossy()))
            // Classification ON — the helper pins it off for golden
            // stability; this test exists to exercise it.
            .env_remove("WAYBILL_NO_GO_MOD_WHY")
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(fixture.path())
            .arg("--format")
            .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
            .arg("--output")
            .arg(format!("cyclonedx-json={}", cdx_path.to_string_lossy()))
            .arg("--output")
            .arg(format!("spdx-2.3-json={}", spdx23_path.to_string_lossy()))
            .arg("--output")
            .arg(format!("spdx-3-json={}", spdx3_path.to_string_lossy()))
            .arg("--no-deep-hash")
            .output()
            .expect("waybill invokes");
        assert!(
            output.status.success(),
            "scan failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let parse = |p: &std::path::Path| -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(p).expect("read output")).expect("parse JSON")
        };
        let cdx = parse(&cdx_path);
        let spdx23 = parse(&spdx23_path);
        let spdx3 = parse(&spdx3_path);

        // C61 — build-inclusion-derivation: go-mod-why.
        let cdx_c61 = cdx_marked_purls(&cdx, "waybill:build-inclusion-derivation", "go-mod-why");
        let spdx23_c61 =
            spdx23_marked_purls(&spdx23, "waybill:build-inclusion-derivation", "go-mod-why");
        let spdx3_c61 =
            spdx3_marked_purls(&spdx3, "waybill:build-inclusion-derivation", "go-mod-why");
        assert!(
            !cdx_c61.is_empty(),
            "stub-classified scan must mark at least one not-needed component \
             with waybill:build-inclusion-derivation: go-mod-why"
        );
        assert_eq!(
            cdx_c61, spdx23_c61,
            "C61 marked-component set must match between CDX and SPDX 2.3"
        );
        assert_eq!(
            cdx_c61, spdx3_c61,
            "C61 marked-component set must match between CDX and SPDX 3"
        );

        // C61 is the REQUIRED companion of C60 `not-needed` — the two
        // marked sets must coincide exactly.
        let cdx_not_needed = cdx_marked_purls(&cdx, "waybill:build-inclusion", "not-needed");
        assert_eq!(
            cdx_c61, cdx_not_needed,
            "every not-needed component must carry the C61 derivation and vice versa"
        );

        // C62 — lifecycle-scope-derivation: go-mod-why.
        let cdx_c62 = cdx_marked_purls(&cdx, "waybill:lifecycle-scope-derivation", "go-mod-why");
        let spdx23_c62 =
            spdx23_marked_purls(&spdx23, "waybill:lifecycle-scope-derivation", "go-mod-why");
        let spdx3_c62 =
            spdx3_marked_purls(&spdx3, "waybill:lifecycle-scope-derivation", "go-mod-why");
        assert!(
            !cdx_c62.is_empty(),
            "stub-classified scan must mark at least one test-only component \
             with waybill:lifecycle-scope-derivation: go-mod-why"
        );
        assert_eq!(
            cdx_c62, spdx23_c62,
            "C62 marked-component set must match between CDX and SPDX 2.3"
        );
        assert_eq!(
            cdx_c62, spdx3_c62,
            "C62 marked-component set must match between CDX and SPDX 3"
        );

        // C61 and C62 populations are disjoint: a component is either
        // not-needed (C61) or test-scoped (C62), never both.
        assert!(
            cdx_c61.is_disjoint(&cdx_c62),
            "not-needed and test-only derivation sets must be disjoint"
        );
    }
}
