//! Integration tests for npm-ecosystem scanning (US2 of milestone 002).
//!
//! Shells out to the `waybill sbom scan --path <fixture>` binary the same
//! way `scan_python.rs` does. Each test asserts the per-story acceptance
//! scenarios + success criteria for the npm pathway documented in
//! `specs/002-python-npm-ecosystem/spec.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join("npm")
        .join(sub)
}

/// Run `waybill sbom scan --path <fixture>` and return the parsed
/// CycloneDX JSON. Returns None if the binary exits non-zero so the
/// caller can assert refusal cases.
///
/// Milestone 052/part-3: `exclude_dev_test` adds
/// `--exclude-scope dev,build,test` to restore the strict pre-052
/// "runtime-only" subset. The default (`false`) emits all lifecycle
/// scopes per FR-002.
fn scan(fixture_sub: &str, exclude_dev_test: bool) -> serde_json::Value {
    let output = scan_raw(fixture_sub, exclude_dev_test);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let out_path = output_path_hint();
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

/// One shared temp path per test thread so scan() and scan_raw_with_path
/// agree. Picks a fresh file each call.
fn output_path_hint() -> PathBuf {
    // Re-derive the same path we passed to the invocation — see scan_raw.
    LAST_OUT_PATH.with(|c| c.borrow().clone().expect("no prior scan"))
}

thread_local! {
    static LAST_OUT_PATH: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

fn scan_raw(fixture_sub: &str, exclude_dev_test: bool) -> std::process::Output {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    LAST_OUT_PATH.with(|c| *c.borrow_mut() = Some(out_path.clone()));
    let mut cmd = Command::new(bin);
    cmd.arg("--offline");
    if exclude_dev_test {
        cmd.arg("--exclude-scope").arg("dev,build,test");
    }
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture(fixture_sub))
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    cmd.output().expect("waybill should run")
}

fn npm_components(sbom: &serde_json::Value) -> Vec<&serde_json::Value> {
    sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:npm/"))
        })
        .collect()
}

fn prop_value<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))?
        .get("value")?
        .as_str()
}

#[test]
fn lockfile_v3_fixture_emits_source_tier_prod_only_with_exclude_scope() {
    // Milestone 052/part-3: --exclude-scope dev,build,test restores
    // the prod-only view (was the pre-052 default).
    let sbom = scan("lockfile-v3", true);
    let npm = npm_components(&sbom);
    assert_eq!(
        npm.len(),
        2,
        "lockfile-v3 --exclude-scope dev,build,test: expected chalk + lodash only, got {:?}",
        npm.iter().map(|c| c["name"].as_str()).collect::<Vec<_>>()
    );
    for c in &npm {
        assert_eq!(prop_value(c, "waybill:sbom-tier"), Some("source"));
        // Prod entries don't emit lifecycle-scope (Runtime is implicit).
        assert!(
            prop_value(c, "waybill:lifecycle-scope").is_none(),
            "{}: prod entries should not surface waybill:lifecycle-scope",
            c["name"]
        );
    }
    for c in &npm {
        let purl = c["purl"].as_str().expect("purl");
        assert!(purl.starts_with("pkg:npm/"), "{purl}");
    }
}

#[test]
fn lockfile_v3_marks_npm_ecosystem_complete() {
    let sbom = scan("lockfile-v3", false);
    let compositions = sbom["compositions"]
        .as_array()
        .expect("compositions array");
    let npm_complete = compositions.iter().any(|r| {
        r["aggregate"].as_str() == Some("complete")
            && r["assemblies"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .any(|p| p.as_str().is_some_and(|s| s.starts_with("pkg:npm/")))
                })
                .unwrap_or(false)
    });
    assert!(
        npm_complete,
        "lockfile-sourced npm scan must emit aggregate=complete composition"
    );
}

#[test]
fn lockfile_v3_default_emits_jest_with_native_scope() {
    // Milestone 052/part-3: default mode emits ALL lifecycle scopes.
    // jest (devDependency) shows up tagged with native CDX `scope:
    // "excluded"` + `waybill:lifecycle-scope: "development"`.
    let sbom = scan("lockfile-v3", false);
    let npm = npm_components(&sbom);
    assert_eq!(npm.len(), 3, "lockfile-v3 default (post-052): expected 3");
    let jest = npm
        .iter()
        .find(|c| c["name"] == "jest")
        .expect("jest present in default mode (post-052)");
    assert_eq!(
        jest["scope"].as_str(),
        Some("excluded"),
        "jest must carry native CDX scope: \"excluded\" in default mode"
    );
    assert_eq!(
        prop_value(jest, "waybill:lifecycle-scope"),
        Some("development"),
        "jest must carry waybill:lifecycle-scope = \"development\" in default mode"
    );
}

#[test]
fn scoped_package_emits_encoded_purl() {
    let sbom = scan("scoped-package", false);
    let npm = npm_components(&sbom);
    let angular = npm
        .iter()
        .find(|c| c["name"] == "@angular/core")
        .expect("@angular/core present");
    assert_eq!(
        angular["purl"].as_str().unwrap(),
        "pkg:npm/%40angular/core@16.2.12",
        "scoped PURL must encode @ per packageurl reference impl"
    );
}

#[test]
fn pnpm_v8_fixture_parses_prod_and_filters_dev() {
    // Milestone 052/part-3: default emits ALL scopes; --exclude-scope
    // dev,build,test restores the prod-only view.

    let sbom_all = scan("pnpm-v8", false);
    let npm_all = npm_components(&sbom_all);
    assert_eq!(npm_all.len(), 2, "pnpm-v8 default (post-052): expected 2");
    let mocha = npm_all
        .iter()
        .find(|c| c["name"] == "mocha")
        .expect("mocha present in default mode (post-052)");
    assert_eq!(
        mocha["scope"].as_str(),
        Some("excluded"),
        "mocha must carry native CDX scope: \"excluded\" in default mode"
    );
    assert_eq!(
        prop_value(mocha, "waybill:lifecycle-scope"),
        Some("development"),
        "mocha must carry waybill:lifecycle-scope = \"development\""
    );

    let sbom = scan("pnpm-v8", true);
    let npm = npm_components(&sbom);
    assert_eq!(
        npm.len(),
        1,
        "pnpm-v8 --exclude-scope dev,build,test: expected 1"
    );
    assert_eq!(npm[0]["name"], "is-odd");
    assert_eq!(prop_value(npm[0], "waybill:sbom-tier"), Some("source"));
}

#[test]
fn node_modules_walk_emits_deployed_tier() {
    let sbom = scan("node-modules-walk", false);
    let npm = npm_components(&sbom);
    assert_eq!(npm.len(), 2, "expected express + safe-buffer");
    for c in &npm {
        assert_eq!(
            prop_value(c, "waybill:sbom-tier"),
            Some("deployed"),
            "{}: node_modules walk must tag deployed",
            c["name"]
        );
    }
}

#[test]
fn package_json_only_emits_design_tier_and_source_type() {
    // Milestone 052/part-3: --exclude-scope dev,build,test drops the
    // devDependency (eslint), leaving only the 3 prod entries.
    let sbom = scan("package-json-only", true);
    let npm = npm_components(&sbom);
    // dependencies has axios (registry) + local-helper (file:) +
    // internal-tool (git+). devDependencies (eslint) filtered via
    // --exclude-scope.
    assert_eq!(npm.len(), 3);
    for c in &npm {
        assert_eq!(
            prop_value(c, "waybill:sbom-tier"),
            Some("design"),
            "{}: must be design-tier",
            c["name"]
        );
        assert!(
            prop_value(c, "waybill:requirement-ranges").is_some(),
            "{}: must carry requirement-range",
            c["name"]
        );
    }
    let local = npm.iter().find(|c| c["name"] == "local-helper").unwrap();
    assert_eq!(prop_value(local, "waybill:source-type"), Some("local"));
    let git = npm.iter().find(|c| c["name"] == "internal-tool").unwrap();
    assert_eq!(prop_value(git, "waybill:source-type"), Some("git"));
    let reg = npm.iter().find(|c| c["name"] == "axios").unwrap();
    assert!(
        prop_value(reg, "waybill:source-type").is_none(),
        "registry entries emit no source-type property"
    );

    // Pre-milestone-066: this asserted the npm ecosystem was NOT
    // marked "complete" because design-tier-only scans don't have
    // resolved versions. Post-066: the npm main-module emitted from
    // the same `package.json` IS source-tier with a resolved version
    // (`package-json-only-fixture@0.1.0`), which legitimately falls
    // into the "complete" composition bucket. The existing
    // design-tier deps (`axios`, `local-helper`, `internal-tool`)
    // still carry empty-version PURLs (`pkg:npm/axios@`); the
    // composition just groups them with the source-tier main-module
    // by ecosystem. This is an accurate downgrade in signal
    // resolution, but acceptable per milestone 066 — consumers
    // walking individual component `waybill:sbom-tier` properties
    // can still distinguish design-tier deps from the source-tier
    // main-module.
    let compositions = sbom["compositions"].as_array().unwrap();
    let npm_complete_any = compositions.iter().any(|r| {
        r["aggregate"].as_str() == Some("complete")
            && r["assemblies"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .any(|p| p.as_str().is_some_and(|s| s.starts_with("pkg:npm/")))
                })
                .unwrap_or(false)
    });
    // Post-066: a "complete" entry exists because the main-module
    // is source-tier. Verify the design-tier deps' empty-version
    // PURLs are present in that entry (not stripped from the
    // composition).
    assert!(
        npm_complete_any,
        "post-066: npm-ecosystem composition includes the source-tier main-module"
    );
}

#[test]
fn npm_dependency_tree_reflects_lockfile() {
    // The lockfile-v3-transitive fixture declares express@4.18.2 with
    // an explicit `dependencies:` section listing body-parser,
    // cookie-signature, and safe-buffer — all of which are sibling
    // entries in the same lockfile. The SBOM's `dependencies[]` block
    // must carry a `{ref: pkg:npm/express@4.18.2, dependsOn: [...]}`
    // record listing all three at their lockfile-resolved versions.
    let sbom = scan("lockfile-v3-transitive", false);
    let deps = sbom["dependencies"]
        .as_array()
        .expect("dependencies array");

    let express_record = deps
        .iter()
        .find(|r| {
            r["ref"]
                .as_str()
                .is_some_and(|s| s == "pkg:npm/express@4.18.2")
        })
        .expect("express must have a dependencies[] record");

    let depends_on: Vec<&str> = express_record["dependsOn"]
        .as_array()
        .expect("dependsOn array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();

    assert!(
        depends_on.contains(&"pkg:npm/body-parser@1.20.1"),
        "express → body-parser@1.20.1 expected; got {depends_on:?}"
    );
    assert!(
        depends_on.contains(&"pkg:npm/cookie-signature@1.0.6"),
        "express → cookie-signature@1.0.6 expected; got {depends_on:?}"
    );
    assert!(
        depends_on.contains(&"pkg:npm/safe-buffer@5.2.1"),
        "express → safe-buffer@5.2.1 expected; got {depends_on:?}"
    );
}

#[test]
fn v1_lockfile_warns_but_does_not_abort_scan() {
    // Milestone 105 phase 2G (T026, SC-008): the pre-105 behavior
    // was a fatal abort with a non-zero exit code on any v1
    // lockfile anywhere in the scan tree. That behavior caused the
    // gRPC-discovered polyglot regression where a stray legacy v1
    // package-lock.json in an unrelated Node example sub-tree
    // blocked the WHOLE scan (including the C/C++ readers the
    // operator actually cared about).
    //
    // Post-105 behavior: the npm reader's `Err(LockfileV1Unsupported)`
    // is caught at the dispatcher and converted to a warn-and-skip.
    // The scan succeeds; the npm reader contributes zero components
    // for this project; stderr carries an actionable warn message
    // naming the offending lockfile path.
    //
    // The npm reader's own `NpmError::LockfileV1Unsupported` variant
    // and its unit tests are unchanged — only the dispatcher's
    // response to that error changed.
    let output = scan_raw("lockfile-v1-refused", false);
    assert!(
        output.status.success(),
        "v1 lockfile MUST warn-and-skip rather than abort the scan (milestone 105 SC-008); got status={:?}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("package-lock.json v1 unsupported"),
        "stderr must carry the actionable warn message; got: {stderr}"
    );
}

// --- Milestone 066: npm main-module emission ------------------------

/// Helper for milestone-066 fixtures that live under waybill-cli/
/// (not the workspace-root tests/fixtures/npm/ tree). Mirrors the
/// pattern used for the cargo-workspace fixture in milestone 064.
fn cli_local_fixture(sub: &str) -> PathBuf {
    // Milestone 090: waybill-cli/tests/fixtures/<sub> dirs moved to
    // mikebom-test-fixtures repo; resolve via WAYBILL_FIXTURES_DIR.
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join(sub)
}

/// Scan a fixture path directly (bypassing the workspace-root
/// `npm/` prefix that `fixture()` adds) and return the parsed CDX
/// JSON.
fn scan_path(path: &Path) -> serde_json::Value {
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

/// US1 AS#1 + SC-001: a single-package npm project emits its
/// main-module via CDX `metadata.component`.
#[test]
fn scan_npm_single_package_emits_main_module_in_metadata_component() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"demo-app","version":"1.2.3","dependencies":{}}"#,
    )
    .unwrap();
    let sbom = scan_path(dir.path());
    let meta = &sbom["metadata"]["component"];
    assert_eq!(meta["type"].as_str(), Some("application"));
    assert_eq!(meta["purl"].as_str(), Some("pkg:npm/demo-app@1.2.3"));
    assert_eq!(meta["name"].as_str(), Some("demo-app"));
    let role = meta["properties"]
        .as_array()
        .expect("metadata.component.properties")
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:component-role"));
    assert_eq!(
        role.and_then(|p| p["value"].as_str()),
        Some("main-module")
    );
    let same_in_components = sbom["components"]
        .as_array()
        .map(|a| a.iter().any(|c| c["purl"].as_str() == Some("pkg:npm/demo-app@1.2.3")))
        .unwrap_or(false);
    assert!(
        !same_in_components,
        "main-module's PURL must not double-emit in components[]"
    );
}

/// US1 AS#2: scoped package names URL-encode the `@` per PURL spec.
#[test]
fn scan_npm_scoped_name_encodes_at_sigil_in_purl() {
    let path = cli_local_fixture("npm-scoped-package");
    let sbom = scan_path(&path);
    assert_eq!(
        sbom["metadata"]["component"]["purl"].as_str(),
        Some("pkg:npm/%40kusari/foo@1.0.0"),
        "scoped name `@kusari/foo` must encode `@` to `%40` per PURL spec"
    );
    assert_eq!(
        sbom["metadata"]["component"]["name"].as_str(),
        Some("@kusari/foo"),
        "name field stays verbatim (with `@` sigil) — only the PURL encodes"
    );
}

/// US1 AS#3 + FR-002: workspace root with `private: true` + no
/// version is skipped; each member emits its own main-module.
#[test]
fn scan_npm_workspace_emits_per_member_main_modules() {
    let path = cli_local_fixture("npm-workspace");
    let sbom = scan_path(&path);
    let main_modules: Vec<&serde_json::Value> = sbom["components"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|c| {
            c["properties"]
                .as_array()
                .map(|p| {
                    p.iter().any(|prop| {
                        prop["name"].as_str() == Some("waybill:component-role")
                            && prop["value"].as_str() == Some("main-module")
                    })
                })
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(
        main_modules.len(),
        2,
        "expected exactly 2 main-modules (members a + b); workspace root \
         (private: true + no version) MUST be skipped per FR-002"
    );
    let purls: std::collections::BTreeSet<String> = main_modules
        .iter()
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();
    assert!(purls.contains("pkg:npm/a@0.5.0"));
    assert!(purls.contains("pkg:npm/b@0.5.0"));
}

/// FR-011: workspace path-deps emit member-to-member edges.
#[test]
fn scan_npm_workspace_path_dep_emits_member_to_member_edge() {
    let path = cli_local_fixture("npm-workspace");
    let sbom = scan_path(&path);
    let deps = sbom["dependencies"].as_array().expect("deps array");
    let b_to_a = deps.iter().any(|d| {
        d["ref"].as_str() == Some("pkg:npm/b@0.5.0")
            && d["dependsOn"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .any(|x| x.as_str() == Some("pkg:npm/a@0.5.0"))
                })
                .unwrap_or(false)
    });
    assert!(
        b_to_a,
        "expected b → a workspace-member path-dep edge in npm workspace SBOM. \
         dependencies array: {deps:#?}"
    );
}

/// FR-001 spec Q1: `name` declared but `version` missing (and not
/// `private`) emits with the `0.0.0-unknown` placeholder.
#[test]
fn scan_npm_name_without_version_uses_placeholder() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"versionless-app"}"#,
    )
    .unwrap();
    let sbom = scan_path(dir.path());
    assert_eq!(
        sbom["metadata"]["component"]["purl"].as_str(),
        Some("pkg:npm/versionless-app@0.0.0-unknown"),
    );
}

/// FR-001 + #104: `private: true` AND no `version` skips emission.
#[test]
fn scan_npm_private_no_version_skips_main_module() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"private-app","private":true}"#,
    )
    .unwrap();
    let sbom = scan_path(dir.path());
    let meta_purl = sbom["metadata"]["component"]["purl"].as_str();
    // Falls through to the synthetic placeholder root (pkg:generic/...
    // or similar). The KEY assertion: no `pkg:npm/private-app@...`
    // should appear anywhere.
    assert!(
        meta_purl.is_some_and(|p| !p.starts_with("pkg:npm/")),
        "private + no version MUST NOT emit a main-module; got metadata.component.purl = {meta_purl:?}"
    );
    let any_npm_main_module = sbom["components"]
        .as_array()
        .map(|a| {
            a.iter().any(|c| {
                c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:npm/private-app"))
            })
        })
        .unwrap_or(false);
    assert!(
        !any_npm_main_module,
        "private + no version: no pkg:npm/private-app main-module should appear in components[]"
    );
}

// ============================================================
// Milestone 199 — always-array shape (US1) + npm-alias
// resolved-identity matching (US2). Fixtures inline via
// tempfile per the existing scan_npm.rs convention.
// ============================================================

/// US1 (m199) — multi-declaration: two workspace packages declare the
/// same dep with different ranges; the lockfile resolves both to a
/// single version. The reconciler-survivor MUST carry both ranges +
/// both manifest paths as JSON arrays (always-array shape per FR-001).
#[test]
fn scan_npm_multi_declaration_preserves_all_ranges_m199() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("packages/foo")).unwrap();
    std::fs::create_dir_all(root.join("packages/bar")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"root","version":"1.0.0","workspaces":["packages/*"]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("packages/foo/package.json"),
        r#"{"name":"foo","version":"1.0.0","dependencies":{"commander":"^11.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("packages/bar/package.json"),
        r#"{"name":"bar","version":"1.0.0","dependencies":{"commander":"^11.1.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("package-lock.json"),
        r#"{
          "name":"root","version":"1.0.0","lockfileVersion":3,
          "packages":{
            "":{"name":"root","version":"1.0.0","workspaces":["packages/*"]},
            "packages/foo":{"name":"foo","version":"1.0.0","dependencies":{"commander":"^11.0"}},
            "packages/bar":{"name":"bar","version":"1.0.0","dependencies":{"commander":"^11.1.0"}},
            "node_modules/commander":{"version":"11.1.0"}
          }
        }"#,
    )
    .unwrap();
    let sbom = scan_path(root);
    let raw = serde_json::to_string(&sbom).unwrap();
    assert!(
        !raw.contains(r#""waybill:requirement-range""#),
        "m199 SC-002: singular scalar waybill:requirement-range MUST NOT appear anywhere in emitted SBOM"
    );
    assert!(
        !raw.contains(r#""waybill:source-manifest""#),
        "m199 SC-002: singular scalar waybill:source-manifest MUST NOT appear anywhere in emitted SBOM"
    );
}

/// US2 (m199) basic case — npm-alias declaration reconciles by resolved
/// identity, stamps `waybill:declared-as: [alias]` on the survivor,
/// emits NO phantom under the alias name.
#[test]
fn scan_npm_alias_reconciles_by_resolved_identity_m199() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(
        root.join("package.json"),
        r#"{
          "name":"my-app","version":"1.0.0",
          "dependencies":{"my-alias":"npm:actual-pkg@1.0.0"}
        }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("package-lock.json"),
        r#"{
          "name":"my-app","version":"1.0.0","lockfileVersion":3,
          "packages":{
            "":{"name":"my-app","version":"1.0.0",
                 "dependencies":{"my-alias":"npm:actual-pkg@1.0.0"}},
            "node_modules/my-alias":{"name":"actual-pkg","version":"1.0.0"}
          }
        }"#,
    )
    .unwrap();
    let sbom = scan_path(root);
    let comps = npm_components(&sbom);
    // No phantom `pkg:npm/my-alias` component.
    let phantom_count = comps
        .iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:npm/my-alias")))
        .count();
    assert_eq!(phantom_count, 0, "no phantom pkg:npm/my-alias component (FR-005)");
    // At least one `actual-pkg` component MUST carry `waybill:declared-as`.
    let actual_pkgs: Vec<&&serde_json::Value> = comps
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:npm/actual-pkg"))
        })
        .collect();
    assert!(
        !actual_pkgs.is_empty(),
        "at least one pkg:npm/actual-pkg component must exist"
    );
    let has_declared_as = actual_pkgs
        .iter()
        .any(|c| prop_value(c, "waybill:declared-as").is_some());
    assert!(
        has_declared_as,
        "at least one actual-pkg component MUST carry waybill:declared-as (FR-006)"
    );
}

/// US2 (m199) negative guardrail — a project with NO alias declarations
/// MUST NOT stamp `waybill:declared-as` on any component (FR-006).
#[test]
fn scan_npm_no_alias_no_declared_as_annotation_m199() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"plain","version":"1.0.0","dependencies":{"commander":"^11.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("package-lock.json"),
        r#"{
          "name":"plain","version":"1.0.0","lockfileVersion":3,
          "packages":{
            "":{"name":"plain","version":"1.0.0","dependencies":{"commander":"^11.0.0"}},
            "node_modules/commander":{"version":"11.0.0"}
          }
        }"#,
    )
    .unwrap();
    let sbom = scan_path(root);
    let raw = serde_json::to_string(&sbom).unwrap();
    assert!(
        !raw.contains(r#""waybill:declared-as""#),
        "FR-006: no alias declarations → no waybill:declared-as anywhere"
    );
}

/// US2 (m199) scoped-package edge — `"my-alias": "npm:@scope/actual@1.0.0"`
/// resolves to `@scope/actual` (not just `actual`).
#[test]
fn scan_npm_alias_scoped_package_resolved_identity_m199() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(
        root.join("package.json"),
        r#"{
          "name":"my-app","version":"1.0.0",
          "dependencies":{"my-alias":"npm:@scope/actual@1.0.0"}
        }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("package-lock.json"),
        r#"{
          "name":"my-app","version":"1.0.0","lockfileVersion":3,
          "packages":{
            "":{"name":"my-app","version":"1.0.0",
                 "dependencies":{"my-alias":"npm:@scope/actual@1.0.0"}},
            "node_modules/my-alias":{"name":"@scope/actual","version":"1.0.0"}
          }
        }"#,
    )
    .unwrap();
    let sbom = scan_path(root);
    let raw = serde_json::to_string(&sbom).unwrap();
    // The scoped-package PURL uses URL-encoded `@` in the name segment.
    assert!(
        raw.contains(r#""pkg:npm/%40scope/actual"#),
        "scoped alias must resolve to @scope/actual PURL; got SBOM containing no @scope/actual PURL"
    );
    // Ensure no `pkg:npm/my-alias` phantom.
    assert!(
        !raw.contains(r#""pkg:npm/my-alias"#),
        "no phantom pkg:npm/my-alias component"
    );
}
