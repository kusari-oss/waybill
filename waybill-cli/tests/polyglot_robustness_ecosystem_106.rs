//! Milestone 106 — SC-006 polyglot-robustness regression test.
//!
//! Build a temp fixture containing well-formed manifests from all four
//! new ecosystems (uv, Bun, Gradle, NuGet) AND a deliberately-malformed
//! file from each ecosystem. Scan it once and assert:
//!
//! 1. The scan exits 0 (no abort across ecosystems).
//! 2. At least one expected component emerges from EACH of the four
//!    well-formed manifests.
//! 3. The malformed files don't abort their reader and don't poison
//!    sibling readers.
//!
//! Mirrors `tests/polyglot_legacy_lockfile_robustness.rs` (milestone
//! 105's npm v1 regression). Locks in the SC-006 polyglot-safety
//! guarantee against regressions; complements
//! `offline_mode_audit_ecosystem_106.rs` (offline-only audit) by
//! exercising the actual runtime behavior end-to-end.

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir parent");
    }
    std::fs::write(path, contents).expect("write file");
}

fn build_fixture(root: &Path) {
    // ── uv (well-formed) ──────────────────────────────────────────
    write(
        &root.join("python/pyproject.toml"),
        r#"[project]
name = "waybill-polyglot-py"
version = "0.1.0"
"#,
    );
    write(
        &root.join("python/uv.lock"),
        r#"version = 1

[[package]]
name = "waybill-polyglot-uv-pkg"
version = "1.2.3"
"#,
    );

    // ── uv (malformed: invalid TOML) ──────────────────────────────
    write(
        &root.join("python-bad/pyproject.toml"),
        r#"[project]
name = "waybill-polyglot-py-bad"
"#,
    );
    write(
        &root.join("python-bad/uv.lock"),
        "this is { not } valid [[[ toml",
    );

    // ── Bun (well-formed) ─────────────────────────────────────────
    write(
        &root.join("bun-app/package.json"),
        r#"{
  "name": "waybill-polyglot-bun",
  "version": "0.1.0"
}
"#,
    );
    write(
        &root.join("bun-app/bun.lock"),
        r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "waybill-polyglot-bun" } },
  "packages": {
    "waybill-polyglot-bun-pkg": ["waybill-polyglot-bun-pkg@4.5.6", "sha512-aaa"]
  }
}
"#,
    );

    // ── Bun (malformed: broken JSONC) ─────────────────────────────
    write(
        &root.join("bun-app-bad/package.json"),
        r#"{"name":"bun-bad","version":"0.0.1"}"#,
    );
    write(
        &root.join("bun-app-bad/bun.lock"),
        r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "packages": { unterminated string
"#,
    );

    // ── Gradle (well-formed) ──────────────────────────────────────
    write(
        &root.join("jvm/gradle.lockfile"),
        r#"# This is a Gradle generated file for dependency locking.
com.waybill.polyglot:gradle-pkg:7.8.9=compileClasspath,runtimeClasspath
"#,
    );

    // ── Gradle (malformed: garbage on every non-comment line) ─────
    write(
        &root.join("jvm-bad/gradle.lockfile"),
        r#"# garbage follows
totally not a coord
also not
"#,
    );

    // ── NuGet (well-formed) ───────────────────────────────────────
    write(
        &root.join("dotnet/App.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="MikebomFixture.PolyglotNuget" Version="10.11.12" />
  </ItemGroup>
</Project>
"#,
    );

    // ── NuGet (malformed: not XML) ────────────────────────────────
    write(
        &root.join("dotnet-bad/App.csproj"),
        "this file is not xml at all",
    );
}

#[test]
fn well_formed_manifests_emit_components_despite_neighboring_malformed_files() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fixture_root = workdir.path().join("fixture");
    build_fixture(&fixture_root);

    let out_path = workdir.path().join("sbom.cdx.json");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_root.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan unexpectedly failed: status={:?}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let components = json["components"]
        .as_array()
        .expect("components[] present");
    let purls: Vec<String> = components
        .iter()
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();

    // Each well-formed manifest MUST contribute its representative
    // component despite the sibling malformed files in their own
    // ecosystems.
    let expected = [
        "pkg:pypi/waybill-polyglot-uv-pkg@1.2.3",
        "pkg:npm/waybill-polyglot-bun-pkg@4.5.6",
        "pkg:maven/com.waybill.polyglot/gradle-pkg@7.8.9",
        "pkg:nuget/MikebomFixture.PolyglotNuget@10.11.12",
    ];
    for purl in expected {
        assert!(
            purls.iter().any(|p| p == purl),
            "expected `{purl}` in SBOM; got purls: {purls:#?}",
        );
    }
}
