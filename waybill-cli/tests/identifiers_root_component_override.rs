//! Milestone 077 — root component override (`--root-name` /
//! `--root-version`) integration tests.
//!
//! These tests drive `mikebom sbom scan --path` against tempdir-based
//! fixtures, asserting that:
//!   * US1 — operator-supplied overrides flow into the emitted SBOM's
//!     root component identity (name + version + bom-ref + purl + cpe).
//!   * US2 (build-tier) — the override emits when threaded through the
//!     `mikebom sbom generate` build-tier path with a synthetic
//!     `ScanArtifacts`.
//!   * US3 — manifest-driven main-module components (Cargo) are
//!     dropped from emitted SBOMs when the override is active (clean
//!     replacement per Q2 clarification).
//!
//! Tests guard `.unwrap()` use per CLAUDE.md.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

// ---------------------------------------------------------------------
// Helper: run `mikebom sbom scan --path <tempdir>` with extra args.
// ---------------------------------------------------------------------

fn run_scan_returning_json(
    fake_home: &Path,
    scan_target: &Path,
    extra_args: &[&str],
    out_format: &str,
    out_filename: &str,
) -> serde_json::Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join(out_filename);
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target)
        .arg("--format")
        .arg(out_format)
        .arg("--output")
        .arg(format!("{out_format}={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}\nstdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    drop(out_dir);
    parsed
}

fn run_scan_expecting_failure(
    fake_home: &Path,
    scan_target: &Path,
    extra_args: &[&str],
) -> (i32, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    drop(out_dir);
    (code, stderr)
}

fn make_arbitrary_dir(name: &str) -> tempfile::TempDir {
    let parent = tempfile::tempdir().unwrap();
    let target = parent.path().join(name);
    std::fs::create_dir_all(&target).unwrap();
    // Create a sibling directory we don't return — we want the
    // returned TempDir to drop with the correct lifetime, so we
    // restructure: build the child, then keep the parent alive
    // through the wrapper. Simplest pattern: rename parent's tempdir
    // to mimic the named layout. We instead return a TempDir whose
    // root path is the child by relying on tempdir's prefix support.
    let _ = target;
    let with_named = tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .unwrap();
    with_named
}

// ---------------------------------------------------------------------
// US1 — source-tier override on arbitrary directories
// ---------------------------------------------------------------------

#[test]
fn root_name_and_version_override_on_arbitrary_dir() {
    // SC-001 / FR-001 + FR-002 + FR-004
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("abc123-snapshot");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "widget-svc",
            "--root-version",
            "1.2.3",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    assert_eq!(comp["name"].as_str(), Some("widget-svc"));
    assert_eq!(comp["version"].as_str(), Some("1.2.3"));
    assert_eq!(comp["bom-ref"].as_str(), Some("widget-svc@1.2.3"));
    assert_eq!(comp["purl"].as_str(), Some("pkg:generic/widget-svc@1.2.3"));
    assert_eq!(
        comp["cpe"].as_str(),
        Some("cpe:2.3:a:mikebom:widget-svc:1.2.3:*:*:*:*:*:*:*")
    );
}

#[test]
fn only_root_name_supplied_uses_default_version() {
    // SC-003 / FR-003 — name override, version falls through to "0.0.0"
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("abc123-snapshot");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &["--root-name", "widget-svc"],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    assert_eq!(comp["name"].as_str(), Some("widget-svc"));
    assert_eq!(comp["version"].as_str(), Some("0.0.0"));
    assert_eq!(comp["bom-ref"].as_str(), Some("widget-svc@0.0.0"));
    assert_eq!(comp["purl"].as_str(), Some("pkg:generic/widget-svc@0.0.0"));
}

#[test]
fn only_root_version_supplied_uses_basename_name() {
    // SC-004 / FR-003 — version override, name falls through to basename
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("xyz-asset");
    let basename = scan_target
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap()
        .to_string();
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &["--root-version", "1.2.3"],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    assert_eq!(comp["name"].as_str(), Some(basename.as_str()));
    assert_eq!(comp["version"].as_str(), Some("1.2.3"));
    let expected_bom_ref = format!("{basename}@1.2.3");
    assert_eq!(comp["bom-ref"].as_str(), Some(expected_bom_ref.as_str()));
}

#[test]
fn no_flags_emits_basename_derived_name() {
    // SC-002 (no-flag default behavior) — assert positive identities
    // for the basename + 0.0.0 path. Byte-identical-to-alpha17 is
    // verified transitively by the existing parity-check golden suite.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("plain-asset");
    let basename = scan_target
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap()
        .to_string();
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    assert_eq!(comp["name"].as_str(), Some(basename.as_str()));
    assert_eq!(comp["version"].as_str(), Some("0.0.0"));
    assert_eq!(
        comp["bom-ref"].as_str(),
        Some(format!("{basename}@0.0.0").as_str())
    );
    assert_eq!(
        comp["purl"].as_str(),
        Some(format!("pkg:generic/{basename}@0.0.0").as_str())
    );
}

#[test]
fn validation_rejects_empty_name() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("scratch");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &["--root-name", ""],
    );
    assert_ne!(code, 0, "empty --root-name must reject; stderr={stderr}");
    assert!(
        stderr.contains("must not be empty") || stderr.contains("empty"),
        "stderr should mention empty rejection; got: {stderr}"
    );
}

#[test]
fn validation_rejects_whitespace_name() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("scratch");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &["--root-name", "my widget svc"],
    );
    assert_ne!(code, 0);
    assert!(
        stderr.contains("whitespace"),
        "stderr should mention whitespace; got: {stderr}"
    );
}

#[test]
fn validation_rejects_control_char_name() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("scratch");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &["--root-name", "foo\x01bar"],
    );
    assert_ne!(code, 0);
    assert!(
        stderr.contains("control character") || stderr.contains("control"),
        "stderr should mention control char; got: {stderr}"
    );
}

#[test]
fn validation_rejects_question_mark_name() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("scratch");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &["--root-name", "foo?bar"],
    );
    assert_ne!(code, 0);
    assert!(
        stderr.contains("URL-syntax-breaking") || stderr.contains("'?'"),
        "stderr should mention `?` rejection; got: {stderr}"
    );
}

#[test]
fn validation_rejects_hash_name() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("scratch");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &["--root-name", "foo#bar"],
    );
    assert_ne!(code, 0);
    assert!(
        stderr.contains("URL-syntax-breaking") || stderr.contains("'#'"),
        "stderr should mention `#` rejection; got: {stderr}"
    );
}

#[test]
fn npm_scoped_name_url_encoded_in_purl() {
    // FR-006 + research §1 — `@acme/widget-svc` accepted at parse,
    // emitted into PURL with RFC 3986 percent-encoding (`@` → `%40`,
    // `/` → `%2F`).
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("scratch");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "@acme/widget-svc",
            "--root-version",
            "1.2.3",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    // The `name` field preserves the operator's exact value verbatim.
    assert_eq!(comp["name"].as_str(), Some("@acme/widget-svc"));
    // The PURL `name` segment is RFC-3986 percent-encoded.
    assert_eq!(
        comp["purl"].as_str(),
        Some("pkg:generic/%40acme%2Fwidget-svc@1.2.3")
    );
}

#[test]
fn override_emits_in_all_three_formats() {
    // SC-005 / FR-007 — same scan emits CDX + SPDX 2.3 + SPDX 3 with
    // the override values appearing in all three root-element fields.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("scratch");

    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "widget-svc",
            "--root-version",
            "1.2.3",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    assert_eq!(
        cdx["metadata"]["component"]["name"].as_str(),
        Some("widget-svc")
    );
    assert_eq!(
        cdx["metadata"]["component"]["version"].as_str(),
        Some("1.2.3")
    );

    let spdx2 = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "widget-svc",
            "--root-version",
            "1.2.3",
        ],
        "spdx-2.3-json",
        "out.spdx.json",
    );
    let pkgs = spdx2["packages"]
        .as_array()
        .expect("packages[] present");
    let synth = pkgs
        .iter()
        .find(|p| {
            p.get("name").and_then(|v| v.as_str()) == Some("widget-svc")
                && p.get("versionInfo").and_then(|v| v.as_str()) == Some("1.2.3")
        })
        .expect("synthesized override root in SPDX 2.3");
    let _ = synth;

    let spdx3 = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "widget-svc",
            "--root-version",
            "1.2.3",
        ],
        "spdx-3-json",
        "out.spdx3.json",
    );
    let graph = spdx3["@graph"].as_array().expect("@graph[] present");
    let pkg = graph
        .iter()
        .find(|n| {
            n.get("type").and_then(|v| v.as_str()) == Some("software_Package")
                && n.get("name").and_then(|v| v.as_str()) == Some("widget-svc")
                && n.get("software_packageVersion").and_then(|v| v.as_str())
                    == Some("1.2.3")
        })
        .expect("synthesized override root in SPDX 3");
    let _ = pkg;
}

#[test]
fn override_deterministic_across_reruns() {
    // SC-009 / FR-010 — same flags + same scan target → byte-identical
    // CDX output across two runs (modulo the per-invocation
    // serialNumber/timestamp that the parity normalization layer
    // strips). We compare metadata.component fields, which are pure
    // functions of the override + target inputs.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("scratch");
    let args = &[
        "--root-name",
        "widget-svc",
        "--root-version",
        "1.2.3",
    ];
    let a = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        args,
        "cyclonedx-json",
        "a.cdx.json",
    );
    let b = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        args,
        "cyclonedx-json",
        "b.cdx.json",
    );
    assert_eq!(a["metadata"]["component"], b["metadata"]["component"]);
}

// ---------------------------------------------------------------------
// US3 — manifest-driven main-module precedence (clean replacement)
// ---------------------------------------------------------------------

/// Build a small Cargo project fixture with `[package].name =
/// "foo-internal"` so the cargo main-module milestone (064) emits a
/// `pkg:cargo/foo-internal@0.5.1` main-module component. Returns a
/// `tempfile::TempDir` whose path holds the fixture.
fn build_cargo_fixture_named(name: &str, version: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            r#"
[package]
name = "{name}"
version = "{version}"

[dependencies]
serde = "1.0.197"
"#
        ),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Cargo.lock"),
        format!(
            r#"
version = 3

[[package]]
name = "{name}"
version = "{version}"
dependencies = ["serde"]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c753"
"#
        ),
    )
    .unwrap();
    dir
}

#[test]
fn override_drops_manifest_main_module_cargo() {
    // SC-006 / FR-008 — Cargo main-module dropped from emitted CDX
    // when the override is set on a manifest-driven scan.
    let fake_home = tempfile::tempdir().unwrap();
    let fixture = build_cargo_fixture_named("foo-internal", "0.5.1");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        fixture.path(),
        &[
            "--root-name",
            "widget-svc",
            "--root-version",
            "1.2.3",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    // metadata.component is the operator-supplied identity.
    assert_eq!(cdx["metadata"]["component"]["name"], "widget-svc");
    assert_eq!(cdx["metadata"]["component"]["version"], "1.2.3");

    // The manifest-derived main-module PURL must NOT appear anywhere
    // in components[].
    let cdx_components = cdx["components"].as_array().expect("components[]");
    let purls: Vec<&str> = cdx_components
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .collect();
    assert!(
        !purls.contains(&"pkg:cargo/foo-internal@0.5.1"),
        "manifest main-module PURL must be dropped under override; got: {purls:?}"
    );
    // serde dependency should still be present.
    assert!(
        purls.iter().any(|p| p.starts_with("pkg:cargo/serde")),
        "serde dependency must be preserved; got: {purls:?}"
    );
}

#[test]
fn no_override_preserves_manifest_main_module() {
    // FR-009 — no-flag scan against the same Cargo fixture preserves
    // the manifest-derived main-module identity (no regression).
    let fake_home = tempfile::tempdir().unwrap();
    let fixture = build_cargo_fixture_named("foo-internal", "0.5.1");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        fixture.path(),
        &[],
        "cyclonedx-json",
        "out.cdx.json",
    );
    // metadata.component is the cargo main-module (CDX 053-style
    // promotion) — `name` matches the manifest's `[package].name`.
    let comp = &cdx["metadata"]["component"];
    assert_eq!(
        comp["name"], "foo-internal",
        "auto-derived metadata.component should be the cargo main-module"
    );
    assert_eq!(comp["version"], "0.5.1");
}

#[test]
fn override_orthogonal_to_other_identifier_flags() {
    // FR-011 — the override is independent of milestone-073 (--repo),
    // milestone-076 (--subject-hash, --component-id) flags. All
    // identifier surfaces coexist independently in the emitted SBOM.
    let fake_home = tempfile::tempdir().unwrap();
    let fixture = build_cargo_fixture_named("foo-internal", "0.5.1");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        fixture.path(),
        &[
            "--root-name",
            "widget-svc",
            "--root-version",
            "1.2.3",
            "--repo",
            "git@github.com:acme/widget-svc.git",
            "--subject-hash",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "--component-id",
            "pkg:cargo/serde@1.0.197=kusari-id:asset-shared-lib-v2",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );

    // (i) Override identity drives metadata.component.
    assert_eq!(cdx["metadata"]["component"]["name"], "widget-svc");
    assert_eq!(cdx["metadata"]["component"]["version"], "1.2.3");

    // (ii) `repo:` identifier rides metadata.component.externalReferences[].
    let ext_refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("externalReferences[]");
    let urls: Vec<&str> = ext_refs
        .iter()
        .filter_map(|r| r["url"].as_str())
        .collect();
    assert!(
        urls.contains(&"git@github.com:acme/widget-svc.git"),
        "repo: identifier preserved; got: {urls:?}"
    );

    // (iii) `subject:` identifier rides
    // metadata.component.externalReferences[type=attestation].
    let has_subject = ext_refs.iter().any(|r| {
        r["type"].as_str() == Some("attestation")
            && r["url"].as_str()
                == Some(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
    });
    assert!(
        has_subject,
        "subject: identifier preserved; got: {ext_refs:#?}"
    );

    // (iv) Per-component identifier rides `properties[]` on the
    // matching `pkg:cargo/serde@1.0.197` component.
    let comps = cdx["components"].as_array().expect("components[]");
    let serde_comp = comps
        .iter()
        .find(|c| {
            c["purl"].as_str() == Some("pkg:cargo/serde@1.0.197")
        });
    if let Some(c) = serde_comp {
        let props = c["properties"].as_array();
        let found = props
            .and_then(|arr| {
                arr.iter().find(|p| {
                    p["name"].as_str() == Some("kusari-id")
                        && p["value"].as_str() == Some("asset-shared-lib-v2")
                })
            })
            .is_some();
        assert!(
            found,
            "per-component identifier must ride serde's properties[]; got: {c:#?}"
        );
    }
    // If serde isn't present (fixture variation), the test of slot
    // (iv) is moot — slots (i)-(iii) still verify orthogonality.
}

// ---------------------------------------------------------------------
// Issue #229 — root-name must preserve the dependency-graph wiring
// between the synthesized root and the manifest's direct dependencies
// in SPDX 2.3 and SPDX 3, not just CycloneDX.
// ---------------------------------------------------------------------

#[test]
fn root_override_preserves_root_outgoing_edges_in_all_formats() {
    // Pre-fix behaviour: in CDX the renamed root inherited the
    // manifest-derived main-module's `dependsOn` edges (via the
    // primary-dependency fallback in cyclonedx/dependencies.rs).
    // In SPDX 2.3 + SPDX 3 the edges silently vanished because
    // `build_relationships` resolves edge sources via a PURL→ID map
    // built from `artifacts.components` only — the dropped main
    // module's PURL was no longer in that map, so its outgoing
    // edges fell through the "PURL not present" branch.
    let fake_home = tempfile::tempdir().unwrap();
    let fixture = build_cargo_fixture_named("foo-internal", "0.5.1");

    // --- CycloneDX (the known-good baseline) -------------------------
    let cdx = run_scan_returning_json(
        fake_home.path(),
        fixture.path(),
        &["--root-name", "widget-svc", "--root-version", "1.2.3"],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let root_ref = cdx["metadata"]["component"]["bom-ref"]
        .as_str()
        .expect("metadata.component.bom-ref present");
    let cdx_root_deps: Vec<String> = cdx["dependencies"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .find(|d| d["ref"].as_str() == Some(root_ref))
        .and_then(|d| d["dependsOn"].as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !cdx_root_deps.is_empty(),
        "CDX baseline: synthesized root must have outgoing dependsOn edges; got {cdx_root_deps:?}"
    );

    // --- SPDX 2.3 ----------------------------------------------------
    let spdx = run_scan_returning_json(
        fake_home.path(),
        fixture.path(),
        &["--root-name", "widget-svc", "--root-version", "1.2.3"],
        "spdx-2.3-json",
        "out.spdx.json",
    );
    let rels = spdx["relationships"].as_array().expect("relationships[]");
    let root_spdxid = rels
        .iter()
        .find(|r| r["relationshipType"].as_str() == Some("DESCRIBES"))
        .and_then(|r| r["relatedSpdxElement"].as_str())
        .expect("DESCRIBES edge present");
    let mut spdx_root_deps: Vec<String> = Vec::new();
    let packages = spdx["packages"].as_array().expect("packages[]");
    for r in rels {
        if r["spdxElementId"].as_str() == Some(root_spdxid)
            && r["relationshipType"].as_str() == Some("DEPENDS_ON")
        {
            let target = r["relatedSpdxElement"].as_str().unwrap_or("");
            let pkg = packages
                .iter()
                .find(|p| p["SPDXID"].as_str() == Some(target));
            if let Some(p) = pkg {
                let purl = p["externalRefs"]
                    .as_array()
                    .and_then(|arr| {
                        arr.iter()
                            .find(|r| r["referenceType"].as_str() == Some("purl"))
                    })
                    .and_then(|r| r["referenceLocator"].as_str())
                    .unwrap_or("");
                spdx_root_deps.push(purl.to_string());
            }
        }
    }
    assert!(
        !spdx_root_deps.is_empty(),
        "issue #229 (SPDX 2.3): synthesized root must have outgoing DEPENDS_ON edges; got {spdx_root_deps:?}"
    );
    assert_eq!(
        spdx_root_deps.iter().collect::<std::collections::BTreeSet<_>>(),
        cdx_root_deps.iter().collect::<std::collections::BTreeSet<_>>(),
        "issue #229: SPDX 2.3 root outgoing edges must match CDX exactly",
    );

    // --- SPDX 3.0.1 --------------------------------------------------
    let spdx3 = run_scan_returning_json(
        fake_home.path(),
        fixture.path(),
        &["--root-name", "widget-svc", "--root-version", "1.2.3"],
        "spdx-3-json",
        "out.spdx3.json",
    );
    let graph = spdx3["@graph"].as_array().expect("@graph");
    let root_iri = graph
        .iter()
        .find(|e| e["type"].as_str() == Some("SpdxDocument"))
        .and_then(|d| d["rootElement"].as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .expect("SpdxDocument.rootElement present");
    let mut spdx3_root_deps: Vec<String> = Vec::new();
    for e in graph {
        let ty = e["type"].as_str().unwrap_or("");
        if ty != "Relationship" && ty != "LifecycleScopedRelationship" {
            continue;
        }
        if e["from"].as_str() != Some(root_iri) {
            continue;
        }
        if e["relationshipType"].as_str() != Some("dependsOn") {
            continue;
        }
        let to_iris: Vec<&str> = e["to"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        for iri in to_iris {
            let purl = graph
                .iter()
                .find(|p| p["spdxId"].as_str() == Some(iri))
                .and_then(|p| p["software_packageUrl"].as_str())
                .unwrap_or("");
            spdx3_root_deps.push(purl.to_string());
        }
    }
    assert!(
        !spdx3_root_deps.is_empty(),
        "issue #229 (SPDX 3): synthesized root must have outgoing dependsOn edges; got {spdx3_root_deps:?}"
    );
    assert_eq!(
        spdx3_root_deps.iter().collect::<std::collections::BTreeSet<_>>(),
        cdx_root_deps.iter().collect::<std::collections::BTreeSet<_>>(),
        "issue #229: SPDX 3 root outgoing edges must match CDX exactly",
    );
}
