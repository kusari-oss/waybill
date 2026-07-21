//! Integration tests for the Gem/Ruby ecosystem (milestone 003 US5).

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::fixture_path;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join("gem")
        .join(sub)
}

fn scan_path(path: &Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn gem_purls(sbom: &serde_json::Value) -> Vec<String> {
    sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter_map(|c| {
            let p = c["purl"].as_str()?;
            if p.starts_with("pkg:gem/") {
                Some(p.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn scan_gem_fixture_emits_canonical_purls() {
    let sbom = scan_path(&fixture("simple-bundle"));
    let purls = gem_purls(&sbom);
    // The fixture declares ~15 gems across GEM + GIT + PATH sections.
    assert!(
        purls.len() >= 15,
        "expected ≥15 gem components, got {}: {purls:?}",
        purls.len(),
    );
    // Direct deps from the DEPENDENCIES block must be present.
    for needle in ["activesupport", "rails", "my-gem", "rspec"] {
        assert!(
            purls.iter().any(|p| p.starts_with(&format!("pkg:gem/{needle}@"))),
            "expected {needle} in gem purls: {purls:?}",
        );
    }
    // Canonical PURL shape.
    for p in &purls {
        assert!(p.starts_with("pkg:gem/"), "non-canonical gem PURL: {p}");
        assert!(p.contains('@'), "gem PURL missing version: {p}");
    }
}

#[test]
fn scan_gem_emits_transitive_dep_edges_from_lockfile() {
    // Gemfile.lock's indent-6 lines encode the per-gem dep graph.
    // Milestone 003 US5 gem.rs now captures those; verify the edges
    // show up in CycloneDX `dependencies[]`.
    let sbom = scan_path(&fixture("simple-bundle"));
    let deps = sbom["dependencies"]
        .as_array()
        .expect("dependencies array");
    let gem_deps: Vec<_> = deps
        .iter()
        .filter(|d| {
            d["ref"]
                .as_str()
                .is_some_and(|s| s.starts_with("pkg:gem/"))
        })
        .collect();
    assert!(
        gem_deps.len() >= 15,
        "expected ≥15 gem dependency records, got {}",
        gem_deps.len(),
    );
    let ref_targets = |needle: &str| -> Vec<String> {
        gem_deps
            .iter()
            .find(|d| {
                d["ref"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(&format!("pkg:gem/{needle}@")))
            })
            .and_then(|d| d["dependsOn"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    // activesupport declares 9 transitive deps in the fixture lockfile.
    let active = ref_targets("activesupport");
    assert!(
        active.iter().any(|t| t.contains("pkg:gem/concurrent-ruby@")),
        "activesupport → concurrent-ruby edge missing: {active:?}",
    );
    assert!(
        active.iter().any(|t| t.contains("pkg:gem/i18n@")),
        "activesupport → i18n edge missing: {active:?}",
    );
    // i18n → concurrent-ruby edge (chained transitive).
    let i18n = ref_targets("i18n");
    assert!(
        i18n.iter().any(|t| t.contains("pkg:gem/concurrent-ruby@")),
        "i18n → concurrent-ruby edge missing: {i18n:?}",
    );
    // rspec chain: rspec-core → rspec-support.
    let rspec_core = ref_targets("rspec-core");
    assert!(
        rspec_core.iter().any(|t| t.contains("pkg:gem/rspec-support@")),
        "rspec-core → rspec-support edge missing: {rspec_core:?}",
    );
}

#[test]
fn scan_gem_git_and_path_entries_tagged_with_source_type() {
    let sbom = scan_path(&fixture("simple-bundle"));
    let components = sbom["components"].as_array().expect("components array");
    let rails = components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:gem/rails@"))
        })
        .expect("rails component present");
    let my_gem = components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:gem/my-gem@"))
        })
        .expect("my-gem component present");
    let rails_src = rails["properties"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["name"].as_str() == Some("waybill:source-type")))
        .and_then(|p| p["value"].as_str())
        .unwrap_or("");
    let my_gem_src = my_gem["properties"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["name"].as_str() == Some("waybill:source-type")))
        .and_then(|p| p["value"].as_str())
        .unwrap_or("");
    assert_eq!(rails_src, "git");
    assert_eq!(my_gem_src, "path");
}

// ---------------------------------------------------------------------------
// Milestone 051 — gem dev/test group classification
// ---------------------------------------------------------------------------

fn scan_args(path: &Path, extra: &[&str]) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let mut cmd = Command::new(bin);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd.output().expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn gem_named<'a>(sbom: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
}

fn has_dev_property(component: &serde_json::Value) -> bool {
    // Milestone 052/part-2: native CDX `scope: "excluded"` is the
    // primary signal; the new `waybill:lifecycle-scope` property
    // carries the finer dev/build/test distinction. Either signal
    // proves the component is non-Runtime-scoped.
    if component["scope"].as_str() == Some("excluded") {
        return true;
    }
    component["properties"]
        .as_array()
        .map(|props| {
            props.iter().any(|p| {
                p["name"].as_str() == Some("waybill:lifecycle-scope")
            })
        })
        .unwrap_or(false)
}

#[test]
fn scan_gem_gemfile_groups_are_tagged() {
    // FR-004 / FR-005: Gemfile groups drive dev classification when
    // the lockfile has no group annotations (canonical Bundler).
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Gemfile"),
        r#"
source "https://rubygems.org"
gem "rack", "~> 3.0"

group :development do
  gem "pry", "0.14.2"
end
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Gemfile.lock"),
        r#"GEM
  remote: https://rubygems.org/
  specs:
    rack (3.0.8)
    pry (0.14.2)

DEPENDENCIES
  rack (~> 3.0)
  pry

BUNDLED WITH
   2.4.10
"#,
    )
    .unwrap();
    // Default (post-052): pry present + tagged with non-Runtime scope.
    let sbom = scan_args(dir.path(), &[]);
    let pry = gem_named(&sbom, "pry").expect("pry present in default mode (post-052)");
    assert!(
        has_dev_property(pry),
        "pry (development group) must carry lifecycle-scope tag: {pry:?}",
    );
    assert!(
        gem_named(&sbom, "rack").is_some(),
        "rack (default group) must be retained",
    );
    // --exclude-scope dev,build,test: pry absent.
    let sbom_strict = scan_args(dir.path(), &["--exclude-scope", "dev,build,test"]);
    assert!(
        gem_named(&sbom_strict, "pry").is_none(),
        "pry (development group) must be dropped under --exclude-scope dev,build,test",
    );
    assert!(
        gem_named(&sbom_strict, "rack").is_some(),
        "rack (default group) must survive --exclude-scope",
    );
}

#[test]
fn scan_gem_gemspec_dev_deps_are_tagged() {
    // FR-004 (gemspec source): library projects with `*.gemspec`
    // declaring `add_development_dependency` get correct
    // classification even when no Gemfile groups are present.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("foo.gemspec"),
        r#"
Gem::Specification.new do |s|
  s.name = "foo"
  s.version = "0.1.0"
  s.add_dependency "activesupport", "~> 7.0"
  s.add_development_dependency "rspec", "~> 3.0"
end
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Gemfile"),
        "source \"https://rubygems.org\"\ngemspec\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Gemfile.lock"),
        r#"GEM
  remote: https://rubygems.org/
  specs:
    activesupport (7.1.3)
    rspec (3.13.0)

DEPENDENCIES
  activesupport
  rspec

BUNDLED WITH
   2.4.10
"#,
    )
    .unwrap();
    // Default (post-052): rspec present + tagged with non-Runtime scope.
    let sbom = scan_args(dir.path(), &[]);
    let rspec = gem_named(&sbom, "rspec").expect("rspec present in default mode (post-052)");
    assert!(
        has_dev_property(rspec),
        "rspec (gemspec dev-dep) must carry lifecycle-scope tag: {rspec:?}",
    );
    assert!(
        gem_named(&sbom, "activesupport").is_some(),
        "activesupport (gemspec runtime-dep) must be retained",
    );
    // --exclude-scope dev,build,test: rspec absent.
    let sbom_strict = scan_args(dir.path(), &["--exclude-scope", "dev,build,test"]);
    assert!(
        gem_named(&sbom_strict, "rspec").is_none(),
        "rspec (gemspec dev-dep) must be dropped under --exclude-scope dev,build,test",
    );
    assert!(
        gem_named(&sbom_strict, "activesupport").is_some(),
        "activesupport must survive --exclude-scope",
    );
}

#[test]
fn scan_gem_production_wins_over_dev() {
    // FR-006: a gem listed as prod by ANY source (Gemfile default
    // group, gemspec add_dependency, or anywhere with empty groups)
    // is treated as production, even if another source classifies
    // it as dev.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Gemfile"),
        r#"
source "https://rubygems.org"
gem "json", "2.7.1"
group :test do
  gem "json"
end
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Gemfile.lock"),
        r#"GEM
  remote: https://rubygems.org/
  specs:
    json (2.7.1)

DEPENDENCIES
  json

BUNDLED WITH
   2.4.10
"#,
    )
    .unwrap();
    let sbom = scan_args(dir.path(), &[]);
    let json = gem_named(&sbom, "json").expect("json must be retained (prod wins)");
    assert!(
        !has_dev_property(json),
        "json must NOT be tagged dev when present in default group: {json:?}",
    );
}

#[test]
fn scan_gem_default_group_is_production_unmarked() {
    // No Gemfile groups, no gemspec — every gem in the lock is
    // production by default. Default scan emits all of them
    // unmarked.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Gemfile"),
        "source \"https://rubygems.org\"\ngem \"rack\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Gemfile.lock"),
        r#"GEM
  remote: https://rubygems.org/
  specs:
    rack (3.0.8)

DEPENDENCIES
  rack

BUNDLED WITH
   2.4.10
"#,
    )
    .unwrap();
    let sbom = scan_args(dir.path(), &[]);
    let rack = gem_named(&sbom, "rack").expect("rack must emit");
    assert!(!has_dev_property(rack), "rack must not be tagged dev");
}

// --- Milestone 069: gem main-module emission ------------------------

fn cli_local_fixture(sub: &str) -> PathBuf {
    // Post-103 migration: gem-source-project moved to the sibling
    // fixtures repo alongside the other build-manifest test projects.
    fixture_path(sub)
}

fn scan_path_format(path: &Path, format: &str) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg(format)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

/// US1 AS#1 + SC-001: top-level *.gemspec emits a main-module in
/// CDX `metadata.component`.
#[test]
fn scan_gem_top_level_gemspec_emits_main_module_in_metadata_component() {
    let path = cli_local_fixture("gem-source-project");
    let cdx = scan_path_format(&path, "cyclonedx-json");
    let meta = &cdx["metadata"]["component"];
    assert_eq!(meta["type"].as_str(), Some("application"));
    assert_eq!(meta["purl"].as_str(), Some("pkg:gem/foo@1.0.0"));
    assert_eq!(meta["name"].as_str(), Some("foo"));
    let role = meta["properties"]
        .as_array()
        .expect("metadata.component.properties")
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:component-role"));
    assert_eq!(
        role.and_then(|p| p["value"].as_str()),
        Some("main-module")
    );
}

/// US1 AS#2: non-literal `s.version = SomeConstant` falls back to
/// `0.0.0-unknown` placeholder per FR-001 + A9.
#[test]
fn scan_gem_non_literal_version_uses_placeholder() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("bar.gemspec"),
        r#"
Gem::Specification.new do |s|
  s.name    = "bar"
  s.version = Bar::VERSION
end
"#,
    )
    .unwrap();
    let cdx = scan_path_format(dir.path(), "cyclonedx-json");
    assert_eq!(
        cdx["metadata"]["component"]["purl"].as_str(),
        Some("pkg:gem/bar@0.0.0-unknown"),
    );
}

/// US1 AS#4 + FR-002 + SC-002: application-style Ruby project
/// (Gemfile + Gemfile.lock, no top-level `*.gemspec`) skips
/// main-module emission.
#[test]
fn scan_gem_application_style_skips_main_module() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'rake', '13.0.0'\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Gemfile.lock"),
        r#"GEM
  remote: https://rubygems.org/
  specs:
    rake (13.0.0)

PLATFORMS
  ruby

DEPENDENCIES
  rake (= 13.0.0)
"#,
    )
    .unwrap();
    let spdx = scan_path_format(dir.path(), "spdx-2.3-json");
    let app_pkgs: Vec<&serde_json::Value> = spdx["packages"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|p| {
            p["primaryPackagePurpose"].as_str() == Some("APPLICATION")
        })
        .collect();
    assert_eq!(
        app_pkgs.len(),
        0,
        "application-style project (Gemfile only, no *.gemspec) MUST NOT emit a main-module per FR-002. Got: {app_pkgs:#?}"
    );
}

/// FR-003: `*.gemspec` files inside install-state paths
/// (`vendor/`, `gems/`, `specifications/`, `.bundle/`) must NOT
/// emit main-modules.
#[test]
fn scan_gem_install_state_paths_skipped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    // Top-level gemspec — should emit.
    std::fs::write(
        root.join("foo.gemspec"),
        r#"
Gem::Specification.new do |s|
  s.name = "foo"
  s.version = "1.0.0"
end
"#,
    )
    .unwrap();
    // vendor/ + gems/ subdirs — must NOT emit per FR-003.
    for skip_parent in ["vendor", "gems"] {
        let dir_path = root.join(skip_parent).join("shadow-1.0.0");
        std::fs::create_dir_all(&dir_path).unwrap();
        std::fs::write(
            dir_path.join("shadow.gemspec"),
            r#"
Gem::Specification.new do |s|
  s.name = "shadow"
  s.version = "9.9.9"
end
"#,
        )
        .unwrap();
    }
    let cdx = scan_path_format(root, "cyclonedx-json");
    assert_eq!(
        cdx["metadata"]["component"]["purl"].as_str(),
        Some("pkg:gem/foo@1.0.0"),
    );
    // Verify no `pkg:gem/shadow` appears anywhere.
    let any_shadow = cdx["components"]
        .as_array()
        .map(|a| {
            a.iter().any(|c| {
                c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:gem/shadow"))
            })
        })
        .unwrap_or(false);
    assert!(
        !any_shadow,
        "shadow.gemspec inside vendor/ or gems/ MUST NOT emit a component per FR-003"
    );
}
