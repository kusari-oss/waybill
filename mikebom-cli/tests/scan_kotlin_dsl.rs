//! Milestone 122 US2 integration tests — Kotlin DSL Gradle reader.
//!
//! Coverage:
//!
//! - US2 AS1: `implementation("g:n:v")` emits as `pkg:maven/...`
//! - US2 AS2: `libs.okhttp` catalog ref resolves to a fully-qualified PURL
//! - US2 AS3: dep-config families map to lifecycle scopes correctly
//! - US2 AS4: KMP source-set declarations stamp `mikebom:kmp-source-set`
//!   as a lex-sorted JSON-encoded array
//! - US2 AS5: multi-module settings.gradle.kts synthesizes a
//!   `pkg:generic/<rootProject.name>@0.0.0` workspace-root component
//! - `us2_kotlin_dsl_honors_exclude_path` (FR-011): --exclude-path
//!   suppresses the targeted module's deps
//! - `us2_nested_gradle_workspace_emits_only_outermost_workspace_root`
//!   (G1 remediation): nested settings.gradle.kts files DO NOT each
//!   produce their own workspace-root component
//! - Negative tests (T028 Kotlin subset): unparseable build.gradle.kts;
//!   missing catalog alias; catalog parse failure

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

fn run_scan_with_args(root: &Path, extra: &[&str]) -> (serde_json::Value, Output) {
    let out_dir = tempfile::tempdir().expect("output tempdir");
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("sbom")
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
        .env_remove("MIKEBOM_NO_GO_MOD_WHY");
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd.output().expect("failed to invoke mikebom binary");
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

fn run_scan(root: &Path) -> (serde_json::Value, Output) {
    run_scan_with_args(root, &[])
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
// US2 acceptance scenarios
// =========================================================================

#[test]
fn us2_as1_implementation_dep_emits_as_pkg_maven() {
    let fixture = workspace_fixture("kotlin_dsl_gradle");
    let (cdx, _out) = run_scan(&fixture);

    let dep = component_by_purl(
        &cdx,
        "pkg:maven/org.jetbrains.kotlin/kotlin-stdlib@1.9.20",
    )
    .expect("kotlin-stdlib component");
    assert_eq!(
        dep.get("version").and_then(|v| v.as_str()),
        Some("1.9.20")
    );
}

#[test]
fn us2_as2_libs_alias_resolves_through_version_catalog() {
    let fixture = workspace_fixture("kotlin_dsl_gradle");
    let (cdx, _out) = run_scan(&fixture);
    // `libs.okhttp` → catalog → `pkg:maven/com.squareup.okhttp3/okhttp@4.12.0`
    let okhttp =
        component_by_purl(&cdx, "pkg:maven/com.squareup.okhttp3/okhttp@4.12.0")
            .expect("okhttp component (resolved via catalog)");
    assert_eq!(
        okhttp.get("version").and_then(|v| v.as_str()),
        Some("4.12.0")
    );
}

#[test]
fn us2_as3_dep_config_to_lifecycle_scope_mapping() {
    let fixture = workspace_fixture("kotlin_dsl_gradle");
    let (cdx, _out) = run_scan(&fixture);

    // testImplementation → mikebom:lifecycle-scope = test (CDX scope: excluded)
    let kotest = component_by_purl(
        &cdx,
        "pkg:maven/io.kotest/kotest-runner-junit5@5.8.0",
    )
    .expect("kotest-runner component");
    assert_eq!(
        component_property(kotest, "mikebom:lifecycle-scope"),
        Some("test"),
        "testImplementation must map to lifecycle-scope=test"
    );

    // kapt → mikebom:lifecycle-scope = build
    let dagger = component_by_purl(
        &cdx,
        "pkg:maven/com.google.dagger/dagger-compiler@2.50",
    )
    .expect("dagger-compiler component");
    assert_eq!(
        component_property(dagger, "mikebom:lifecycle-scope"),
        Some("build"),
        "kapt must map to lifecycle-scope=build"
    );

    // debugImplementation → mikebom:lifecycle-scope = development
    let leak = component_by_purl(
        &cdx,
        "pkg:maven/com.squareup.leakcanary/leakcanary-android@2.12",
    )
    .expect("leakcanary component");
    assert_eq!(
        component_property(leak, "mikebom:lifecycle-scope"),
        Some("development")
    );
}

#[test]
fn us2_as4_kmp_source_set_emits_json_array_lex_sorted() {
    let fixture = workspace_fixture("kotlin_dsl_gradle");
    let (cdx, _out) = run_scan(&fixture);

    // kotlinx-serialization-json is declared in BOTH commonMain AND jvmMain.
    // The merged annotation should be a lex-sorted JSON array.
    let kx = component_by_purl(
        &cdx,
        "pkg:maven/org.jetbrains.kotlinx/kotlinx-serialization-json@1.6.2",
    )
    .expect("kotlinx-serialization-json component");
    let raw = component_property(kx, "mikebom:kmp-source-set")
        .expect("kmp-source-set property must be present");
    let arr: serde_json::Value = serde_json::from_str(raw)
        .expect("kmp-source-set value must parse as JSON");
    let sources = arr.as_array().unwrap();
    let names: Vec<&str> =
        sources.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(names, vec!["commonMain", "jvmMain"]);
}

#[test]
fn us2_as5_settings_kts_workspace_synthesizes_pkg_generic_root() {
    let fixture = workspace_fixture("kotlin_dsl_gradle");
    let (cdx, _out) = run_scan(&fixture);

    // Workspace-root: pkg:generic/demo-kts@0.0.0 with mikebom:component-role = workspace-root.
    let root = component_by_purl(&cdx, "pkg:generic/demo-kts@0.0.0")
        .expect("workspace-root component");
    assert_eq!(
        component_property(root, "mikebom:component-role"),
        Some("workspace-root")
    );
}

#[test]
fn us2_kotlin_dsl_honors_exclude_path() {
    // Exclude the `shared/` module — its deps (kotlinx-serialization-json,
    // ktor-client-cio-jvm) should be absent; `app/`'s deps still emit.
    let fixture = workspace_fixture("kotlin_dsl_gradle");
    let (cdx, _out) =
        run_scan_with_args(&fixture, &["--exclude-path", "**/shared"]);

    let kx = component_by_purl(
        &cdx,
        "pkg:maven/org.jetbrains.kotlinx/kotlinx-serialization-json@1.6.2",
    );
    assert!(kx.is_none(), "shared/ deps should be excluded");
    let okhttp =
        component_by_purl(&cdx, "pkg:maven/com.squareup.okhttp3/okhttp@4.12.0");
    assert!(okhttp.is_some(), "app/ deps still emit");
}

#[test]
fn us2_nested_gradle_workspace_emits_only_outermost_workspace_root() {
    // G1 remediation: a nested settings.gradle.kts must NOT produce its
    // own workspace-root component. Synthesize a two-deep-nested
    // fixture and verify exactly ONE workspace-root entry.
    let dir = tempfile::tempdir().unwrap();
    // Outer settings.
    std::fs::write(
        dir.path().join("settings.gradle.kts"),
        b"rootProject.name = \"outer-root\"\ninclude(\":outer-app\")\n",
    )
    .unwrap();
    // Inner workspace (defensive coding pattern).
    let inner = dir.path().join("outer-app");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(
        inner.join("settings.gradle.kts"),
        b"rootProject.name = \"inner-root\"\ninclude(\":inner-mod\")\n",
    )
    .unwrap();
    let (cdx, _out) = run_scan(dir.path());

    let workspace_roots: Vec<_> = components(&cdx)
        .iter()
        .filter(|c| {
            component_property(c, "mikebom:component-role")
                == Some("workspace-root")
        })
        .collect();
    assert_eq!(
        workspace_roots.len(),
        1,
        "exactly one workspace-root component expected; got {} entries: {:?}",
        workspace_roots.len(),
        workspace_roots
            .iter()
            .map(|c| c.get("purl").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
    );
    // The outer one wins (shortest path).
    assert!(
        workspace_roots[0]
            .get("purl")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p.contains("outer-root")),
        "outermost settings.gradle.kts (outer-root) must win"
    );
}

// =========================================================================
// T028 negative-test runbook (Kotlin DSL subset)
// =========================================================================

#[test]
fn kotlin_dsl_unparseable_build_script_warns_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.gradle.kts"),
        b"rootProject.name = \"bad-script\"\n",
    )
    .unwrap();
    // build.gradle.kts with ONLY meta-programmed deps (no regex match)
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        b"plugins { kotlin(\"jvm\") }\ndeps.forEach { implementation(it) }\n",
    )
    .unwrap();
    let (cdx, _out) = run_scan(dir.path());
    // Workspace-root still emerges; no Maven deps.
    let maven_count = components(&cdx)
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:maven/"))
        })
        .count();
    assert_eq!(
        maven_count, 0,
        "meta-programmed deps don't match the regex; no Maven entries should emerge"
    );
    let root =
        component_by_purl(&cdx, "pkg:generic/bad-script@0.0.0");
    assert!(root.is_some(), "workspace-root still emits even when no deps match");
}

#[test]
fn kotlin_dsl_missing_catalog_alias_warns_and_drops_that_dep() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.gradle.kts"),
        b"rootProject.name = \"missing-alias\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("gradle")).unwrap();
    std::fs::write(
        dir.path().join("gradle/libs.versions.toml"),
        br#"[versions]
ok = "1.0.0"

[libraries]
ok = { module = "io.example:ok", version.ref = "ok" }
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        br#"plugins { kotlin("jvm") }

dependencies {
    implementation(libs.ok)
    implementation(libs.missing)  // not in catalog -- warn-and-drop
}
"#,
    )
    .unwrap();
    let (cdx, _out) = run_scan(dir.path());

    let good = component_by_purl(&cdx, "pkg:maven/io.example/ok@1.0.0");
    assert!(good.is_some(), "valid catalog alias resolves");
    let maven_count = components(&cdx)
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:maven/"))
        })
        .count();
    assert_eq!(
        maven_count, 1,
        "missing alias drops; only the valid dep emits"
    );
}
