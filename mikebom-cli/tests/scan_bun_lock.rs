//! Integration test for the bun.lock reader (milestone 106 US2, issue #278).
//!
//! Companion to the unit tests in `scan_fs::package_db::npm::bun_lock::tests`
//! (which exercise `parse_bun_lock` directly). This test invokes the
//! `mikebom sbom scan --path <fixture>` binary against the in-repo
//! `bun_lock/basic/` fixture to verify the dispatcher integration —
//! `npm::read` actually calls `bun_lock::read_bun_lock`, the JSONC
//! comment stripper handles the top-of-file `// bun: lockfileVersion: 1`
//! marker correctly, and the emitted SBOM contains the expected
//! `pkg:npm/...` components including the URL-encoded scoped package.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn basic_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("bun_lock")
        .join("basic")
}

#[test]
fn bun_lock_basic_fixture_emits_npm_components() {
    let fixture = basic_fixture();
    assert!(
        fixture.is_dir(),
        "bun_lock/basic fixture missing at {}",
        fixture.display()
    );
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("MIKEBOM_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn mikebom");
    assert!(
        output.status.success(),
        "bun.lock scan unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");

    let components = json["components"]
        .as_array()
        .expect("components array present");
    let npm_purls: Vec<&str> = components
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:npm/"))
        .collect();

    // Both fixture packages MUST appear, including the scoped name
    // with URL-encoded `@` (per the PURL spec). The names are
    // deliberately synthetic (`mikebom-fixture-*`) so the fixture
    // never collides with real-world CVE advisories — Inspector +
    // dependency policy gates don't whack-a-mole us on package
    // version churn (the fixture is a parser exercise, not a real
    // build dep).
    assert!(
        npm_purls.contains(&"pkg:npm/mikebom-fixture-lib@1.2.3"),
        "expected mikebom-fixture-lib in output; got: {npm_purls:?}",
    );
    assert!(
        npm_purls.contains(&"pkg:npm/%40mikebom-fixture/types-pkg@4.5.6"),
        "expected URL-encoded @mikebom-fixture/types-pkg in output; got: {npm_purls:?}",
    );
}

/// Milestone 189 (#555) — regression test for Fable5's 2026-07-13 audit
/// finding: bun.lock with trailing commas (which every real Bun 1.2+
/// output has) MUST parse cleanly rather than silently truncating to
/// manifest-tier only. Pre-m189 bug produced 282 components instead of
/// 1584 on a typical bun project.
#[test]
fn bun_lock_with_trailing_commas_parses_and_emits_components() {
    // Fabricate a bun.lock with trailing commas at multiple nesting
    // levels — matches the shape Fable5 observed in real-world files.
    let bun_lock_content = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": {
    "": {
      "name": "m189-trailing-comma-fixture",
      "dependencies": {
        "mikebom-fixture-lib": "1.2.3",
        "@mikebom-fixture/types-pkg": "4.5.6",
      },
    },
  },
  "packages": {
    "mikebom-fixture-lib": ["mikebom-fixture-lib@1.2.3", "sha512-abc"],
    "@mikebom-fixture/types-pkg": ["@mikebom-fixture/types-pkg@4.5.6", "sha512-def"],
  },
}
"#;

    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let scan_root = workdir.path().join("project");
    std::fs::create_dir_all(&scan_root).expect("mkdir project");
    std::fs::write(scan_root.join("bun.lock"), bun_lock_content).expect("write bun.lock");
    // Sibling package.json so the manifest-tier reader also sees the deps.
    let pkg_json = r#"{
  "name": "m189-trailing-comma-fixture",
  "version": "0.1.0",
  "dependencies": {
    "mikebom-fixture-lib": "1.2.3",
    "@mikebom-fixture/types-pkg": "4.5.6"
  }
}
"#;
    std::fs::write(scan_root.join("package.json"), pkg_json).expect("write package.json");
    let out_path = workdir.path().join("sbom.cdx.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("MIKEBOM_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        scan_root.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn mikebom");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "bun.lock scan with trailing commas failed: stderr={stderr}"
    );

    // Critical: verify mikebom did NOT emit a bun.lock parse-failure
    // WARN (that'd mean we're still silently falling back to
    // manifest-tier).
    assert!(
        !stderr.contains("bun.lock JSONC parse failed"),
        "trailing-comma bun.lock triggered the m189-fixed parse-failure WARN: stderr={stderr}"
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let components = json["components"]
        .as_array()
        .expect("components array present");
    let npm_purls: Vec<&str> = components
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:npm/"))
        .collect();
    assert!(
        npm_purls.contains(&"pkg:npm/mikebom-fixture-lib@1.2.3"),
        "expected mikebom-fixture-lib from bun.lock in output; got: {npm_purls:?}"
    );
    assert!(
        npm_purls.contains(&"pkg:npm/%40mikebom-fixture/types-pkg@4.5.6"),
        "expected scoped fixture package from bun.lock in output; got: {npm_purls:?}"
    );
}
