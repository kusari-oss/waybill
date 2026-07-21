//! Integration tests for the milestone 116 PR-A `waybill:produces-binaries`
//! Cargo extractor + auto-alias binder.
//!
//! Covers:
//!   - T015: source-tier emission shape (multi-source + library-only +
//!     workspace per-member + union-merge)
//!   - T016: cross-tier auto-alias resolution (single + collision)
//!   - T017: operator-`--pkg-alias` precedence rule (FR-004)
//!
//! Fixtures live in-tree under
//! `waybill-cli/tests/fixtures/produces_binaries/cargo/` (not in the
//! milestone-090 external fixture repo; these are tiny synthetic projects).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("produces_binaries")
        .join("cargo")
        .join(sub)
}

fn run_scan(path: &Path, out_path: &Path) -> Output {
    let bin = env!("CARGO_BIN_EXE_waybill");
    Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run")
}

fn read_sbom(path: &Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn produces_binaries_for_purl(sbom: &serde_json::Value, purl: &str) -> Option<Vec<String>> {
    // Single-main-module scans promote the main-module to
    // `metadata.component`; multi-module scans emit through `components[]`.
    // Search both locations.
    let mut candidates: Vec<&serde_json::Value> = Vec::new();
    if let Some(c) = sbom.get("metadata").and_then(|m| m.get("component")) {
        candidates.push(c);
    }
    if let Some(arr) = sbom.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            candidates.push(c);
        }
    }
    for c in candidates {
        if c.get("purl").and_then(|v| v.as_str()) == Some(purl) {
            let Some(props) = c.get("properties").and_then(|v| v.as_array()) else {
                return Some(Vec::new());
            };
            for p in props {
                if p.get("name").and_then(|v| v.as_str()) == Some("waybill:produces-binaries") {
                    let v = p.get("value").and_then(|v| v.as_str())?;
                    let arr: Vec<String> = serde_json::from_str(v).ok()?;
                    return Some(arr);
                }
            }
            return Some(Vec::new());
        }
    }
    None
}

// ============================================================================
// T015 — source-tier emission shape
// ============================================================================

#[test]
fn multi_source_emits_all_three_cargo_binary_sources() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("multi-source"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    let produced =
        produces_binaries_for_purl(&sbom, "pkg:cargo/fixture-baz@1.0.0").expect(
            "fixture-baz main-module component should be present and carry produces-binaries",
        );
    // Expected: [[bin]] table entry (`fixture-baz-alt`), default-binary
    // inference from src/main.rs (`fixture-baz`), and implicit
    // src/bin/fixture-baz-helper.rs. Lex-sorted, deduped, normalized.
    // Note: fixture-baz-alt is BOTH an explicit [[bin]] and listed by
    // the src/bin/*.rs walk; the dedupe collapses it to a single entry.
    assert_eq!(
        produced,
        vec![
            "fixture-baz".to_string(),
            "fixture-baz-alt".to_string(),
            "fixture-baz-helper".to_string(),
        ]
    );
}

#[test]
fn library_only_omits_produces_binaries_property() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("library-only"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    let produced = produces_binaries_for_purl(&sbom, "pkg:cargo/fixture-libonly@1.0.0");
    // Either the component is absent (no main module emitted for pure
    // libs in some configurations) OR it's present with no property.
    // In both cases the produces-binaries declaration MUST be absent.
    match produced {
        None => {} // component not emitted; vacuously correct
        Some(v) => assert!(
            v.is_empty(),
            "library-only crate must NOT carry produces-binaries; got {v:?}"
        ),
    }
}

#[test]
fn workspace_emits_per_member_declarations_not_consolidated() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("workspace"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    // Per spec clarification Q1: declarations land on EACH workspace
    // member's main-module component, not consolidated onto the root.
    let crate_a = produces_binaries_for_purl(&sbom, "pkg:cargo/crate-a@1.0.0")
        .expect("crate-a main-module component should be present");
    let crate_b = produces_binaries_for_purl(&sbom, "pkg:cargo/crate-b@1.0.0")
        .expect("crate-b main-module component should be present");
    assert_eq!(crate_a, vec!["crate-a".to_string()]);
    assert_eq!(crate_b, vec!["crate-b".to_string()]);
}

// ============================================================================
// T016 — cross-tier auto-alias resolution (single + collision)
// ============================================================================

mod binder_unit {
    //! Unit tests for `SourceSbomContext::binding_for_purl()`'s
    //! milestone-116 auto-alias fallback. Constructs synthetic source
    //! SBOMs entirely in JSON; no scanner invocation needed.
    //!
    //! These are integration-test-positioned but unit-shaped — they
    //! exercise the binder's contract directly without the per-ecosystem
    //! extractor in the loop. Per data-model.md § "Lifecycle" this is the
    //! shape closest to how downstream consumers exercise the contract.

    use waybill::binding::{AliasSource, BindingStrength, SourceSbomContext};
    use serde_json::json;

    fn write_source_sbom(dir: &std::path::Path, sbom: serde_json::Value) -> std::path::PathBuf {
        let p = dir.join("source.cdx.json");
        std::fs::write(&p, sbom.to_string()).expect("write source sbom");
        p
    }

    #[test]
    fn single_candidate_auto_alias_succeeds() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = write_source_sbom(
            tmp.path(),
            json!({
                "bomFormat": "CycloneDX",
                "specVersion": "1.6",
                "components": [
                    {
                        "type": "library",
                        "purl": "pkg:cargo/fixture-baz@1.0.0",
                        "properties": [
                            { "name": "waybill:produces-binaries", "value": "[\"fixture-baz\"]" }
                        ]
                    }
                ]
            }),
        );
        let ctx = SourceSbomContext::load(&src).expect("load source sbom");
        let binding = ctx.binding_for_purl("pkg:generic/fixture-baz");
        assert_eq!(
            binding.alias_source,
            Some(AliasSource::AutomaticFromProducesBinaries),
            "expected automatic-from-produces-binaries alias"
        );
        assert_eq!(
            binding.alias_to.as_ref().map(|p| p.as_str().to_string()),
            Some("pkg:cargo/fixture-baz@1.0.0".to_string())
        );
        assert_ne!(
            binding.reason.as_deref(),
            Some("source-not-found-in-bind-target")
        );
    }

    #[test]
    fn case_and_suffix_tolerance_works() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = write_source_sbom(
            tmp.path(),
            json!({
                "bomFormat": "CycloneDX",
                "specVersion": "1.6",
                "components": [
                    {
                        "type": "library",
                        "purl": "pkg:maven/com.acme/baz@1.0.0",
                        "properties": [
                            { "name": "waybill:produces-binaries", "value": "[\"baz\"]" }
                        ]
                    }
                ]
            }),
        );
        let ctx = SourceSbomContext::load(&src).expect("load source sbom");
        // .jar suffix on image side
        let binding = ctx.binding_for_purl("pkg:generic/baz.jar");
        assert_eq!(
            binding.alias_source,
            Some(AliasSource::AutomaticFromProducesBinaries),
            ".jar suffix should be tolerated"
        );
        // Mixed case on image side (BAZ.EXE → baz)
        let binding = ctx.binding_for_purl("pkg:generic/BAZ.EXE");
        assert_eq!(
            binding.alias_source,
            Some(AliasSource::AutomaticFromProducesBinaries),
            "case + .exe suffix should be tolerated"
        );
    }

    #[test]
    fn multi_candidate_collision_is_weak() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = write_source_sbom(
            tmp.path(),
            json!({
                "bomFormat": "CycloneDX",
                "specVersion": "1.6",
                "components": [
                    {
                        "type": "library",
                        "purl": "pkg:cargo/baz@1.0.0",
                        "properties": [
                            { "name": "waybill:produces-binaries", "value": "[\"baz\"]" }
                        ]
                    },
                    {
                        "type": "library",
                        "purl": "pkg:cargo/other-baz@2.0.0",
                        "properties": [
                            { "name": "waybill:produces-binaries", "value": "[\"baz\"]" }
                        ]
                    }
                ]
            }),
        );
        let ctx = SourceSbomContext::load(&src).expect("load source sbom");
        let binding = ctx.binding_for_purl("pkg:generic/baz");
        assert_eq!(
            binding.alias_source,
            Some(AliasSource::AutomaticFromProducesBinaries)
        );
        assert_eq!(binding.strength, BindingStrength::Weak);
        assert_eq!(
            binding.reason.as_deref(),
            Some("multiple-source-candidates-for-binary-name")
        );
    }

    #[test]
    fn operator_supplied_rhs_takes_exact_path_not_auto_alias() {
        // T017 — FR-004 operator-precedence rule's structural test.
        //
        // When the operator declares `--pkg-alias pkg:generic/baz=
        // pkg:cargo/other-baz@2.0.0`, attach_bindings_to_components
        // (scan_cmd.rs:2317) resolves the image-tier PURL via the alias
        // map FIRST, then calls binding_for_purl on the RHS. The RHS
        // (`pkg:cargo/other-baz@2.0.0`) is in `source_purls` because the
        // operator chose an existing source-tier component, so the
        // exact-match path returns a binding without engaging the auto-
        // alias fallback. The wrapping logic in attach_bindings_to_
        // components then explicitly stamps `alias_source =
        // OperatorSupplied`. This unit-style test verifies the structural
        // half: binding_for_purl(RHS) returns no auto-alias signal.
        //
        // The end-to-end E2E test (operator flag → output SBOM stamped
        // with operator-supplied alias_source) lives in the existing
        // milestone-111 `pkg_alias_binding_us1.rs` test suite, which
        // already covers the full --pkg-alias path; we extend that path
        // structurally here without rebuilding the synthetic-image
        // scaffolding.
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = write_source_sbom(
            tmp.path(),
            json!({
                "bomFormat": "CycloneDX",
                "specVersion": "1.6",
                "components": [
                    {
                        "type": "library",
                        "purl": "pkg:cargo/fixture-baz@1.0.0",
                        "properties": [
                            { "name": "waybill:produces-binaries", "value": "[\"fixture-baz\"]" }
                        ]
                    },
                    {
                        "type": "library",
                        "purl": "pkg:cargo/other-baz@2.0.0"
                    }
                ]
            }),
        );
        let ctx = SourceSbomContext::load(&src).expect("load source sbom");
        // Operator's RHS resolves via exact match — auto-alias does NOT
        // engage (alias_source stays None at this layer; production code
        // stamps OperatorSupplied separately).
        let binding = ctx.binding_for_purl("pkg:cargo/other-baz@2.0.0");
        assert_eq!(
            binding.alias_source, None,
            "binding_for_purl(operator's RHS) must NOT engage auto-alias"
        );
        // Confirm the OTHER PURL — the one declared via produces-binaries
        // — would have hit the auto-alias path IF the operator hadn't
        // pointed elsewhere.
        let auto = ctx.binding_for_purl("pkg:generic/fixture-baz");
        assert_eq!(
            auto.alias_source,
            Some(AliasSource::AutomaticFromProducesBinaries),
            "auto-alias path engages for the LHS image PURL"
        );
        assert_eq!(
            auto.alias_to.as_ref().map(|p| p.as_str().to_string()),
            Some("pkg:cargo/fixture-baz@1.0.0".to_string()),
            "auto-alias resolves to the produces-binaries-declaring source PURL"
        );
    }

    #[test]
    fn no_candidates_falls_back_to_unknown_unchanged() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = write_source_sbom(
            tmp.path(),
            json!({
                "bomFormat": "CycloneDX",
                "specVersion": "1.6",
                "components": [
                    {
                        "type": "library",
                        "purl": "pkg:cargo/somethingelse@1.0.0"
                    }
                ]
            }),
        );
        let ctx = SourceSbomContext::load(&src).expect("load source sbom");
        let binding = ctx.binding_for_purl("pkg:generic/unrelated");
        assert_eq!(binding.alias_source, None);
        assert_eq!(binding.strength, BindingStrength::Unknown);
        assert_eq!(
            binding.reason.as_deref(),
            Some("source-not-found-in-bind-target")
        );
    }
}
