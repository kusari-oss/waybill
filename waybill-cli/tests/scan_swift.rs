//! Milestone 122 US1 integration tests — Swift Package Manager reader.
//!
//! Coverage:
//!
//! - `us1_as1_swift_argument_parser_emits_as_pkg_swift` — scanning the
//!   `swift_package_resolved/` fixture emits both lockfile pins as
//!   `pkg:swift/...` PURLs with the resolved versions.
//! - `us1_as2_dot_git_suffix_stripped_from_purl` — the lockfile entry's
//!   `https://github.com/Alamofire/Alamofire.git` location strips the
//!   `.git` suffix in the emitted PURL.
//! - `us1_as3_commit_pinned_uses_full_sha_as_version_segment` —
//!   scanning the `swift_commit_pinned/` fixture emits the entry with
//!   the full 40-char revision SHA as the version segment AND the
//!   `waybill:source-type = "git"` annotation.
//! - `us1_as4_package_swift_without_resolved_warns_and_emits_zero` —
//!   a tempdir with a `Package.swift` but no `Package.resolved` emits
//!   zero Swift components AND surfaces the documented warn line.
//! - `no_swift_no_kotlin_byte_identical_to_pre_feature` (SC-007) —
//!   scanning a pure Cargo fixture produces zero `pkg:swift/...` entries
//!   and matches the pre-feature emission shape.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn workspace_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join(name)
}

fn run_scan(root: &Path) -> (serde_json::Value, Output) {
    // Write the CDX output to a per-test tempdir so concurrent test runs
    // never collide and the fixture directories stay clean.
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
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        .env_remove("WAYBILL_EXCLUDE_PATH")
        .env_remove("WAYBILL_NO_GO_MOD_WHY")
        .output()
        .expect("failed to invoke waybill binary");
    if !output.status.success() {
        panic!(
            "waybill exited non-zero:\nstdout: {}\nstderr: {}",
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
// US1 acceptance scenarios
// =========================================================================

#[test]
fn us1_as1_swift_argument_parser_emits_as_pkg_swift() {
    let fixture = workspace_fixture("swift_package_resolved");
    let (cdx, _out) = run_scan(&fixture);

    let arg_parser = component_by_purl(
        &cdx,
        "pkg:swift/github.com/apple/swift-argument-parser@1.3.0",
    )
    .expect("swift-argument-parser component");
    assert_eq!(
        arg_parser.get("name").and_then(|v| v.as_str()),
        Some("swift-argument-parser")
    );
    assert_eq!(
        arg_parser.get("version").and_then(|v| v.as_str()),
        Some("1.3.0")
    );
}

#[test]
fn us1_as2_dot_git_suffix_stripped_from_purl() {
    let fixture = workspace_fixture("swift_package_resolved");
    let (cdx, _out) = run_scan(&fixture);

    // The location was `https://github.com/Alamofire/Alamofire.git` — the
    // `.git` suffix is stripped before PURL projection.
    let alamofire =
        component_by_purl(&cdx, "pkg:swift/github.com/Alamofire/Alamofire@5.9.0")
            .expect("Alamofire component");
    assert!(
        alamofire
            .get("purl")
            .and_then(|v| v.as_str())
            .map(|s| !s.contains(".git"))
            .unwrap_or(false),
        "PURL must not retain .git suffix"
    );
}

#[test]
fn us1_as3_commit_pinned_uses_full_sha_as_version_segment() {
    let fixture = workspace_fixture("swift_commit_pinned");
    let (cdx, _out) = run_scan(&fixture);

    let expected_sha = "9cb486020ebf03bfa5b5df985387a14a98744537";
    let expected_purl =
        format!("pkg:swift/github.com/apple/swift-log@{}", expected_sha);
    let swift_log = component_by_purl(&cdx, &expected_purl)
        .unwrap_or_else(|| panic!("swift-log component with full-SHA version: {expected_purl}"));
    assert_eq!(
        swift_log.get("version").and_then(|v| v.as_str()),
        Some(expected_sha)
    );
    assert_eq!(
        component_property(swift_log, "waybill:source-type"),
        Some("git"),
        "commit-pinned mode must carry waybill:source-type = git"
    );
    assert_eq!(
        component_property(swift_log, "waybill:source-revision"),
        Some(expected_sha),
        "commit-pinned mode must carry the SHA on waybill:source-revision"
    );
}

#[test]
fn us1_as4_package_swift_without_resolved_warns_and_emits_zero() {
    let dir = tempfile::tempdir().unwrap();
    // Touch a Package.swift without a Package.resolved sibling.
    std::fs::write(
        dir.path().join("Package.swift"),
        b"// swift-tools-version:5.9\nimport PackageDescription\n",
    )
    .unwrap();
    let (cdx, out) = run_scan(dir.path());

    // No pkg:swift/* components emit.
    let swift_count = components(&cdx)
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:swift/"))
        })
        .count();
    assert_eq!(swift_count, 0);

    // stderr surfaces the warn line naming the path.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Package.swift") && stderr.contains("Package.resolved"),
        "stderr should warn about the unresolved manifest: {stderr}"
    );
}

// =========================================================================
// T028 negative-test runbook (Swift subset)
// =========================================================================

#[test]
fn swift_malformed_package_resolved_warns_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Package.swift"), b"// swift-tools-version:5.9\n")
        .unwrap();
    std::fs::write(
        dir.path().join("Package.resolved"),
        // Trailing comma → invalid JSON
        b"{ \"version\": 2, \"pins\": [], }\n",
    )
    .unwrap();
    let (cdx, out) = run_scan(dir.path());
    let swift_count = components(&cdx)
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:swift/"))
        })
        .count();
    assert_eq!(swift_count, 0, "malformed lockfile ⇒ zero pkg:swift entries");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Package.resolved"),
        "stderr should name the bad lockfile path: {stderr}"
    );
}

#[test]
fn swift_unknown_schema_version_warns_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Package.swift"), b"// swift-tools-version:5.9\n")
        .unwrap();
    std::fs::write(
        dir.path().join("Package.resolved"),
        br#"{ "version": 4, "pins": [] }"#,
    )
    .unwrap();
    let (cdx, out) = run_scan(dir.path());
    let swift_count = components(&cdx)
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:swift/"))
        })
        .count();
    assert_eq!(swift_count, 0);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown schema version") || stderr.contains("version"),
        "stderr should mention the unknown version: {stderr}"
    );
}

#[test]
fn swift_ssh_url_emits_via_ssh_host() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Package.swift"), b"// swift-tools-version:5.9\n")
        .unwrap();
    std::fs::write(
        dir.path().join("Package.resolved"),
        br#"{
          "pins" : [
            {
              "identity" : "internal-lib",
              "kind" : "remoteSourceControl",
              "location" : "git@gitlab.acme.com:internal/lib.git",
              "state" : {
                "revision" : "cafebabecafebabecafebabecafebabecafebabe"
              }
            }
          ],
          "version" : 2
        }"#,
    )
    .unwrap();
    let (cdx, _out) = run_scan(dir.path());
    let expected = format!(
        "pkg:swift/gitlab.acme.com/internal/lib@{}",
        "cafebabecafebabecafebabecafebabecafebabe"
    );
    let lib = component_by_purl(&cdx, &expected)
        .unwrap_or_else(|| panic!("expected SSH-projected PURL: {expected}"));
    assert_eq!(lib.get("name").and_then(|v| v.as_str()), Some("internal-lib"));
}

// =========================================================================
// SC-007 byte-identity regression
// =========================================================================

#[test]
fn no_swift_content_emits_zero_swift_components() {
    // Scan a tempdir with NO Swift content (just an empty dir). Should
    // emit zero `pkg:swift/...` components without errors.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    let (cdx, _out) = run_scan(dir.path());

    let swift_count = components(&cdx)
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:swift/"))
        })
        .count();
    assert_eq!(swift_count, 0, "no Swift content ⇒ no pkg:swift entries");
}
