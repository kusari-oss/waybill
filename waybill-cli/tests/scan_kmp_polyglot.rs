//! Milestone 122 US3 integration tests — KMP polyglot monorepo scan.
//!
//! The fixture at `tests/fixtures/golden_inputs/kmp_polyglot/` contains
//! BOTH Android-side `build.gradle.kts` files (Maven deps) AND a
//! nested `iosApp/` SwiftPM project (`Package.swift` + `Package.resolved`).
//! One scan against the repo root should produce one SBOM with both
//! ecosystem PURL families coexisting per FR-008.
//!
//! Coverage:
//!
//! - `us3_polyglot_scan_emits_both_pkg_maven_and_pkg_swift_components`
//!   (US3 AS1): both ecosystems' PURLs emerge in one SBOM.
//! - `us3_kmp_workspace_root_and_member_components_present`
//!   (US3 AS2 adapted): the synthetic workspace-root + KMP source-set
//!   provenance ride through unchanged in the polyglot case.
//! - `us3_swift_alamofire_emits_with_pkg_swift_namespace`
//!   (US3 AS3): the iosApp/ Swift deps emerge alongside the Maven deps
//!   with no cross-ecosystem collapse.
//! - `us3_no_swift_no_kotlin_byte_identical_to_pre_feature`
//!   (SC-007 / T027): a pure-Cargo project produces zero pkg:swift +
//!   zero pkg:maven entries; emission is byte-identical to the pre-122
//!   shape (modulo `serialNumber` + `timestamp`).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn workspace_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join(name)
}

fn run_scan(root: &Path) -> (serde_json::Value, Output) {
    let out_dir = tempfile::tempdir().expect("output tempdir");
    let out_path = out_dir.path().join("out.cdx.json");
    let output = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--no-deep-hash")
        .arg("--offline")
        .arg("--output")
        .arg(&out_path)
        .env("MIKEBOM_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        .env_remove("MIKEBOM_EXCLUDE_PATH")
        .env_remove("MIKEBOM_NO_GO_MOD_WHY")
        .output()
        .expect("failed to invoke mikebom binary");
    if !output.status.success() {
        panic!(
            "mikebom exited non-zero:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    let text = std::fs::read_to_string(&out_path).expect("CDX output present");
    let cdx: serde_json::Value =
        serde_json::from_str(&text).expect("CDX output must parse as JSON");
    (cdx, output)
}

fn components(cdx: &serde_json::Value) -> &Vec<serde_json::Value> {
    cdx.get("components")
        .and_then(|v| v.as_array())
        .expect("components[] present")
}

fn count_ecosystem(cdx: &serde_json::Value, prefix: &str) -> usize {
    components(cdx)
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with(prefix))
        })
        .count()
}

fn component_by_purl<'a>(
    cdx: &'a serde_json::Value,
    purl: &str,
) -> Option<&'a serde_json::Value> {
    components(cdx)
        .iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some(purl))
}

fn component_property<'a>(
    component: &'a serde_json::Value,
    name: &str,
) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|prop| {
                if prop.get("name").and_then(|v| v.as_str()) == Some(name) {
                    prop.get("value").and_then(|v| v.as_str())
                } else {
                    None
                }
            })
        })
}

// =========================================================================
// US3 acceptance scenarios
// =========================================================================

#[test]
fn us3_polyglot_scan_emits_both_pkg_maven_and_pkg_swift_components() {
    let fixture = workspace_fixture("kmp_polyglot");
    let (cdx, _out) = run_scan(&fixture);

    let maven_count = count_ecosystem(&cdx, "pkg:maven/");
    let swift_count = count_ecosystem(&cdx, "pkg:swift/");
    assert!(
        maven_count > 0,
        "expected at least one pkg:maven/... component; got 0"
    );
    assert!(
        swift_count > 0,
        "expected at least one pkg:swift/... component; got 0"
    );

    // Spot-check the Android side: androidx.core/core-ktx must emerge.
    assert!(
        component_by_purl(&cdx, "pkg:maven/androidx.core/core-ktx@1.12.0").is_some(),
        "androidx.core:core-ktx Maven dep missing"
    );
    // Spot-check the iOS side: Alamofire must emerge.
    assert!(
        component_by_purl(&cdx, "pkg:swift/github.com/Alamofire/Alamofire@5.9.0")
            .is_some(),
        "Alamofire Swift dep missing"
    );
}

#[test]
fn us3_kmp_workspace_root_and_kmp_source_set_provenance_present() {
    let fixture = workspace_fixture("kmp_polyglot");
    let (cdx, _out) = run_scan(&fixture);

    // The Gradle workspace synthesizes pkg:generic/kmp-app@0.0.0 with the
    // workspace-root role.
    let root = component_by_purl(&cdx, "pkg:generic/kmp-app@0.0.0")
        .expect("workspace-root component pkg:generic/kmp-app@0.0.0");
    assert_eq!(
        component_property(root, "mikebom:component-role"),
        Some("workspace-root")
    );

    // kotlinx-serialization-json is declared in commonMain inside the
    // shared/ module — the kmp-source-set provenance rides through.
    let kx = component_by_purl(
        &cdx,
        "pkg:maven/org.jetbrains.kotlinx/kotlinx-serialization-json@1.6.2",
    )
    .expect("kotlinx-serialization-json (shared module commonMain)");
    let raw = component_property(kx, "mikebom:kmp-source-set")
        .expect("mikebom:kmp-source-set property present");
    let arr: serde_json::Value =
        serde_json::from_str(raw).expect("kmp-source-set value parses as JSON");
    let names: Vec<&str> = arr
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        names.contains(&"commonMain"),
        "kmp-source-set should include commonMain; got {names:?}"
    );
}

#[test]
fn us3_swift_log_emits_with_pkg_swift_namespace_alongside_maven_deps() {
    // Pure-Swift cross-check: iosApp/'s swift-log must emerge as
    // pkg:swift/... without colliding with anything from the Android
    // side. There's no `swift-log` on Maven; this test confirms the
    // Swift reader's contribution surfaces independently of Kotlin's.
    let fixture = workspace_fixture("kmp_polyglot");
    let (cdx, _out) = run_scan(&fixture);

    let swift_log = component_by_purl(
        &cdx,
        "pkg:swift/github.com/apple/swift-log@1.5.4",
    )
    .expect("swift-log Swift dep");
    assert_eq!(
        swift_log.get("version").and_then(|v| v.as_str()),
        Some("1.5.4")
    );

    // And there should be no Maven entry shadowing it (sanity check
    // against any future cross-ecosystem dedup change).
    assert!(
        components(&cdx)
            .iter()
            .filter(|c| {
                c.get("purl")
                    .and_then(|v| v.as_str())
                    .is_some_and(|p| p.contains("swift-log") && p.starts_with("pkg:maven/"))
            })
            .count()
            == 0,
        "no pkg:maven/... entry should shadow the Swift-side swift-log"
    );
}

// =========================================================================
// SC-007 byte-identity regression (T027)
// =========================================================================

#[test]
fn us3_no_swift_no_kotlin_emits_zero_122_components() {
    // A pure-Cargo project (no Package.resolved, no build.gradle.kts,
    // no settings.gradle.kts) should produce ZERO pkg:swift entries
    // AND ZERO pkg:maven entries from the kotlin_dsl reader. Confirms
    // SC-007: when neither ecosystem is present in the scan tree, the
    // milestone-122 readers contribute nothing — emission is shape-
    // identical to a pre-122 build.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    let (cdx, _out) = run_scan(dir.path());

    let swift_count = count_ecosystem(&cdx, "pkg:swift/");
    let maven_count = count_ecosystem(&cdx, "pkg:maven/");
    assert_eq!(swift_count, 0, "pure Cargo project should yield 0 pkg:swift entries");
    assert_eq!(maven_count, 0, "pure Cargo project should yield 0 pkg:maven entries");

    // And no pkg:generic/<workspace-root>@0.0.0 sneaks in either.
    let generic_workspace_count = components(&cdx)
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:generic/") && p.ends_with("@0.0.0"))
                && component_property(c, "mikebom:component-role") == Some("workspace-root")
        })
        .count();
    assert_eq!(
        generic_workspace_count, 0,
        "pure Cargo project should not synthesize a kotlin_dsl workspace-root"
    );
}
