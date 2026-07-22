//! Milestone 216 — integration tests for Gemfile-only Ruby app
//! main-module emission (closes waybill#629 discovered during m215's
//! `--split` real-world validation on ~/Projects/iac).
//!
//! Seven scenarios in priority order:
//!   T012 gemfile_only_dir_emits_pkg_generic_main_module   — happy path
//!   T013 iac_reproducer_pattern_split_mode                — split-mode reproducer
//!   T014 gemfile_without_lock_still_emits_main_module     — FR-006
//!   T014a workspaces_detected_annotation_includes_ruby_apps  — SC-005
//!   T014b ruby_app_sub_sbom_passes_split_manifest_v1_schema  — SC-006
//!   T015 single_sbom_scan_promotes_gemfile_app_over_synthetic_placeholder — SC-002/003
//!   T016 gemspec_present_wins_over_gemfile                — FR-007

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use tempfile::tempdir;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn m216_gemfile_application_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gemfile_application")
}

fn waybill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_waybill"))
}

fn split_manifest_schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts/split-manifest-v1.schema.json")
}

/// Cached split-manifest v1 schema validator (m215 dev-dep).
fn split_manifest_validator() -> &'static jsonschema::Validator {
    static CELL: OnceLock<jsonschema::Validator> = OnceLock::new();
    CELL.get_or_init(|| {
        let raw = std::fs::read_to_string(split_manifest_schema_path())
            .expect("read split-manifest v1 schema");
        let schema: serde_json::Value =
            serde_json::from_str(&raw).expect("parse split-manifest v1 schema");
        jsonschema::validator_for(&schema).expect("compile split-manifest v1 schema")
    })
}

/// Run a scan in an isolated $HOME so per-host caches don't leak.
/// Returns (ok, stdout, stderr).
fn run_scan(
    path: &PathBuf,
    extra_args: &[&str],
) -> (bool, String, String) {
    let home = tempdir().expect("home tempdir");
    let output = Command::new(waybill_bin())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .args(extra_args)
        .arg("--offline")
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path())
        .env("CARGO_HOME", home.path().join(".cargo"))
        .env("GOMODCACHE", home.path().join("go-mod"))
        .env("M2_REPO", home.path().join(".m2"))
        .current_dir(workspace_root())
        .output()
        .expect("spawn waybill");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn list_cdxs(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read output_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".cdx.json"))
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

// ============ T012 — happy path ============

#[test]
fn gemfile_only_dir_emits_pkg_generic_main_module() {
    let out_file = tempdir().expect("out tempdir");
    let out_path = out_file.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &m216_gemfile_application_fixture(),
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "scan failed:\n{stderr}");

    let text = std::fs::read_to_string(&out_path).expect("read cdx");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse cdx");
    let purl = v
        .pointer("/metadata/component/purl")
        .and_then(|s| s.as_str())
        .expect("root PURL present");
    assert!(
        purl.starts_with("pkg:generic/gemfile_application@"),
        "expected pkg:generic/gemfile_application@..., got {purl}"
    );
    let props = v
        .pointer("/metadata/component/properties")
        .and_then(|s| s.as_array())
        .expect("root properties array");
    let has = |k: &str, val: &str| {
        props.iter().any(|p| {
            p["name"].as_str() == Some(k) && p["value"].as_str() == Some(val)
        })
    };
    assert!(
        has("waybill:component-role", "main-module"),
        "expected waybill:component-role = main-module on root: {props:?}"
    );
    assert!(
        has("waybill:package-shape", "application"),
        "expected waybill:package-shape = application on root: {props:?}"
    );
}

// ============ T013 — split-mode reproducer pattern ============

/// Build a mini-monorepo tempdir with 2 sibling Gemfile-only subdirs
/// plus 1 npm subdir (control). Returns the tempdir handle (drop kills
/// the dir) and its scan-root path.
fn build_multi_ecosystem_fixture() -> (tempfile::TempDir, PathBuf) {
    let scratch = tempdir().expect("fixture tempdir");
    let root = scratch.path().to_path_buf();
    // Ruby app 1
    let app1 = root.join("service-alpha");
    std::fs::create_dir_all(&app1).expect("mkdir app1");
    std::fs::write(app1.join("Gemfile"), b"source 'https://rubygems.org'\n").unwrap();
    std::fs::write(
        app1.join("Gemfile.lock"),
        b"GEM\n  remote: https://rubygems.org/\n  specs:\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n\nBUNDLED WITH\n   2.5.3\n",
    )
    .unwrap();
    // Ruby app 2
    let app2 = root.join("service-beta");
    std::fs::create_dir_all(&app2).expect("mkdir app2");
    std::fs::write(app2.join("Gemfile"), b"source 'https://rubygems.org'\n").unwrap();
    std::fs::write(
        app2.join("Gemfile.lock"),
        b"GEM\n  remote: https://rubygems.org/\n  specs:\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n\nBUNDLED WITH\n   2.5.3\n",
    )
    .unwrap();
    // npm control
    let npm = root.join("web-ui");
    std::fs::create_dir_all(&npm).expect("mkdir npm");
    std::fs::write(
        npm.join("package.json"),
        b"{\"name\": \"web-ui\", \"version\": \"1.0.0\"}\n",
    )
    .unwrap();
    std::fs::write(
        npm.join("package-lock.json"),
        b"{\n  \"name\": \"web-ui\",\n  \"version\": \"1.0.0\",\n  \"lockfileVersion\": 3,\n  \"requires\": true,\n  \"packages\": {\n    \"\": {\"name\": \"web-ui\", \"version\": \"1.0.0\"}\n  }\n}\n",
    )
    .unwrap();
    (scratch, root)
}

#[test]
fn iac_reproducer_pattern_split_mode() {
    let (_scratch, root) = build_multi_ecosystem_fixture();
    let out = tempdir().expect("out tempdir");
    let (ok, _stdout, stderr) = run_scan(
        &root,
        &[
            "--split",
            "--output-dir",
            out.path().to_str().unwrap(),
            "--format",
            "cyclonedx-json",
        ],
    );
    assert!(ok, "split scan failed:\n{stderr}");

    let cdxs = list_cdxs(out.path());
    assert_eq!(
        cdxs.len(),
        3,
        "expected 3 sub-SBOMs (2 Ruby apps + 1 npm), got {}:\n{}",
        cdxs.len(),
        cdxs.iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );

    // Filename convention: 2 files match `<slug>.generic.cdx.json`.
    let generic_count = cdxs
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".generic.cdx.json"))
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        generic_count, 2,
        "expected 2 .generic.cdx.json Ruby-app files, got {generic_count}"
    );

    // Manifest lists 3 entries, 2 with pkg:generic/ root PURLs.
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("split-manifest.json"))
            .expect("read manifest"),
    )
    .expect("parse manifest");
    let entries = manifest["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 3, "manifest must list 3 subprojects");
    let generic_entries = entries
        .iter()
        .filter(|e| {
            e["root_purl"]
                .as_str()
                .map(|p| p.starts_with("pkg:generic/"))
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        generic_entries, 2,
        "expected 2 pkg:generic/ manifest entries, got {generic_entries}"
    );
}

// ============ T014 — no-lock ============

#[test]
fn gemfile_without_lock_still_emits_main_module() {
    let scratch = tempdir().expect("fixture tempdir");
    let root = scratch.path().to_path_buf();
    std::fs::write(root.join("Gemfile"), b"source 'https://rubygems.org'\n").unwrap();
    // Deliberately NO Gemfile.lock.

    let out = tempdir().expect("out tempdir");
    let out_path = out.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &root,
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "no-lock scan failed:\n{stderr}");

    let text = std::fs::read_to_string(&out_path).expect("read cdx");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse cdx");
    let purl = v
        .pointer("/metadata/component/purl")
        .and_then(|s| s.as_str())
        .expect("root PURL present");
    assert!(
        purl.starts_with("pkg:generic/"),
        "expected pkg:generic/ root even without Gemfile.lock, got {purl}"
    );
    let props = v
        .pointer("/metadata/component/properties")
        .and_then(|s| s.as_array())
        .expect("root properties array");
    assert!(
        props.iter().any(|p| {
            p["name"].as_str() == Some("waybill:package-shape")
                && p["value"].as_str() == Some("application")
        }),
        "waybill:package-shape must still be present without lock"
    );
}

// ============ T014a — SC-005 workspaces-detected ============

#[test]
fn workspaces_detected_annotation_includes_ruby_apps() {
    let (_scratch, root) = build_multi_ecosystem_fixture();
    let out = tempdir().expect("out tempdir");
    let out_path = out.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &root,
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "single-SBOM scan failed:\n{stderr}");

    // workspaces-detected annotation lives at metadata.properties[].
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).expect("read cdx"))
            .expect("parse cdx");
    let props = v
        .pointer("/metadata/properties")
        .and_then(|s| s.as_array());
    let workspaces_json = props
        .and_then(|arr| {
            arr.iter()
                .find(|p| p["name"].as_str() == Some("waybill:workspaces-detected"))
                .and_then(|p| p["value"].as_str())
                .map(|s| s.to_string())
        });
    // The annotation is emitted only when >1 workspaces are detected —
    // our 3-subproject fixture qualifies.
    let workspaces_str = workspaces_json
        .expect("waybill:workspaces-detected annotation missing (needed for SC-005)");
    let workspaces: Vec<String> =
        serde_json::from_str(&workspaces_str).expect("parse workspaces JSON array");
    // The Ruby app subdirs should appear alongside the npm subdir.
    assert!(
        workspaces.iter().any(|w| w == "service-alpha"),
        "workspaces-detected must include Ruby app service-alpha, got {workspaces:?}"
    );
    assert!(
        workspaces.iter().any(|w| w == "service-beta"),
        "workspaces-detected must include Ruby app service-beta, got {workspaces:?}"
    );
    assert!(
        workspaces.iter().any(|w| w == "web-ui"),
        "workspaces-detected must include npm control web-ui, got {workspaces:?}"
    );
}

// ============ T014b — SC-006 schema validation ============

#[test]
fn ruby_app_sub_sbom_passes_split_manifest_v1_schema() {
    let (_scratch, root) = build_multi_ecosystem_fixture();
    let out = tempdir().expect("out tempdir");
    let (ok, _stdout, stderr) = run_scan(
        &root,
        &[
            "--split",
            "--output-dir",
            out.path().to_str().unwrap(),
            "--format",
            "cyclonedx-json",
        ],
    );
    assert!(ok, "split scan failed:\n{stderr}");

    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("split-manifest.json"))
            .expect("read manifest"),
    )
    .expect("parse manifest");

    let errors: Vec<String> = split_manifest_validator()
        .iter_errors(&manifest)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "split-manifest.json (with Ruby-app entries) failed v1 schema validation:\n  {}",
        errors.join("\n  ")
    );
}

// ============ T015 — SC-002 + SC-003 heuristic promotion ============

#[test]
fn single_sbom_scan_promotes_gemfile_app_over_synthetic_placeholder() {
    let out_file = tempdir().expect("out tempdir");
    let out_path = out_file.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &m216_gemfile_application_fixture(),
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "single-SBOM scan failed:\n{stderr}");

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).expect("read cdx"))
            .expect("parse cdx");

    // Root PURL matches the Ruby-app main-module identity.
    let purl = v
        .pointer("/metadata/component/purl")
        .and_then(|s| s.as_str())
        .expect("root PURL");
    assert!(
        purl.starts_with("pkg:generic/gemfile_application@"),
        "expected pkg:generic/gemfile_application@..., got {purl}"
    );

    // Root-selection heuristic: m127's count==1 fast path is
    // byte-identity-preserving and emits NO `waybill:root-selection-heuristic`
    // annotation (only the multi-candidate ladder branches emit one).
    // Pre-feature: 0 main-modules → synthetic-placeholder branch fires
    // AND emits the annotation with heuristic="synthetic-placeholder".
    // Post-feature: 1 main-module (the Gemfile app) → fast path fires
    // silently. So SC-003 is satisfied when EITHER (a) the annotation is
    // absent OR (b) the annotation is present with heuristic !=
    // "synthetic-placeholder".
    let props = v
        .pointer("/metadata/properties")
        .and_then(|s| s.as_array())
        .expect("metadata.properties array");
    let heuristic_opt = props
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:root-selection-heuristic"))
        .and_then(|p| p["value"].as_str());
    match heuristic_opt {
        None => {
            // Fast path fired — pre-feature synthetic-placeholder branch
            // was avoided. SC-003 satisfied.
        }
        Some(envelope_str) => {
            let envelope: serde_json::Value =
                serde_json::from_str(envelope_str).expect("envelope JSON");
            let heuristic = envelope
                .pointer("/value/heuristic")
                .and_then(|s| s.as_str())
                .expect("heuristic field in envelope");
            assert_ne!(
                heuristic, "synthetic-placeholder",
                "SC-003: heuristic MUST NOT be synthetic-placeholder; got {heuristic}"
            );
        }
    }
}

// ============ T016 — FR-007 gemspec wins ============

#[test]
fn gemspec_present_wins_over_gemfile() {
    let scratch = tempdir().expect("fixture tempdir");
    let root = scratch.path().to_path_buf();
    // Publish a synthetic gemspec.
    std::fs::write(
        root.join("pubgem.gemspec"),
        b"Gem::Specification.new do |s|\n  s.name = \"pubgem\"\n  s.version = \"1.0.0\"\nend\n",
    )
    .unwrap();
    // Also carry a Gemfile in the same dir.
    std::fs::write(root.join("Gemfile"), b"source 'https://rubygems.org'\n").unwrap();
    std::fs::write(
        root.join("Gemfile.lock"),
        b"GEM\n  remote: https://rubygems.org/\n  specs:\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n\nBUNDLED WITH\n   2.5.3\n",
    )
    .unwrap();

    let out_file = tempdir().expect("out tempdir");
    let out_path = out_file.path().join("waybill.cdx.json");
    let (ok, _stdout, stderr) = run_scan(
        &root,
        &[
            "--format",
            "cyclonedx-json",
            "--output",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(ok, "scan failed:\n{stderr}");

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).expect("read cdx"))
            .expect("parse cdx");
    let purl = v
        .pointer("/metadata/component/purl")
        .and_then(|s| s.as_str())
        .expect("root PURL");
    assert_eq!(
        purl, "pkg:gem/pubgem@1.0.0",
        "FR-007: gemspec-derived pkg:gem/ main-module must win over Gemfile-derived pkg:generic/; got {purl}"
    );
    // Assert exactly ONE main-module across all components (root + components[]).
    let all_purls: Vec<String> = std::iter::once(purl.to_string())
        .chain(
            v["components"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|c| c["purl"].as_str().map(|s| s.to_string())),
        )
        .collect();
    let generic_pubgem = all_purls
        .iter()
        .filter(|p| p.starts_with("pkg:generic/pubgem"))
        .count();
    assert_eq!(
        generic_pubgem, 0,
        "FR-007: NO pkg:generic/pubgem should exist (gemspec wins); got {all_purls:?}"
    );
}
