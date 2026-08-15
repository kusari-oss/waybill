//! Milestone 235 US1 subprocess resolver — end-to-end integration test.
//!
//! Companion to the unit tests in
//! `scan_fs::package_db::gradle::subprocess::tests` (which exercise the
//! ASCII-tree parser + PURL construction directly). This test invokes
//! the `waybill sbom scan --path <fixture> --gradle-resolve` binary
//! against the `wrapper_single_subproject/` fixture and asserts the
//! emitted CDX contains:
//!
//! - A component for the direct dep (`waybillfixture:direct@1.0.0`)
//! - A component for the mock's transitive dep
//!   (`waybillfixture:transitive@0.5.0`)
//! - A component for the test-scope dep
//!   (`waybillfixture:test-only@2.0.0`)
//! - A `dependencies[]` edge from direct → transitive
//!
//! The fixture's `./gradlew` is a bash mock (no JDK required); it
//! emits canned ASCII-tree output for the configurations the m235
//! subprocess resolver requests by default (runtimeClasspath +
//! testRuntimeClasspath per clarify Q1).
//!
//! Golden fixture pinning (T019) is deferred to a follow-on PR;
//! structural assertions here cover SC-001's "transitive edge
//! present" acceptance criterion.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("gradle")
        .join("wrapper_single_subproject")
}

fn run_scan_with_gradle_resolve() -> serde_json::Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = fixture();
    assert!(
        fixture_path.is_dir(),
        "fixture missing at {}",
        fixture_path.display()
    );

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--gradle-resolve",
        "--gradle-timeout-secs",
        "30",
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "gradle-resolve scan failed:\n  stdout={}\n  stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

/// Extract all `pkg:maven/*` PURLs from an SBOM's `components[]` array
/// AND the top-level metadata.component (waybill's main-module
/// placement varies by ecosystem).
fn maven_purls(json: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = json["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["purl"].as_str())
                .filter(|p| p.starts_with("pkg:maven/"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if let Some(main) = json["metadata"]["component"]["purl"].as_str() {
        if main.starts_with("pkg:maven/") {
            out.push(main.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Return `true` iff there exists an edge with `ref == source_purl`
/// AND `target_purl` present in the `dependsOn` array.
fn has_edge(json: &serde_json::Value, source_purl: &str, target_purl: &str) -> bool {
    let Some(deps) = json["dependencies"].as_array() else {
        return false;
    };
    for dep in deps {
        if dep["ref"].as_str() == Some(source_purl) {
            let Some(depends_on) = dep["dependsOn"].as_array() else {
                continue;
            };
            for t in depends_on {
                if t.as_str() == Some(target_purl) {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract the value of a CDX document-scope `metadata.properties[]`
/// entry with the given name. Returns `None` if the property is absent.
fn doc_scope_property(json: &serde_json::Value, name: &str) -> Option<String> {
    json["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str().map(str::to_string))
}

// -----------------------------------------------------------
// SC-001 — US1 fixture emits transitive edge in CDX
// -----------------------------------------------------------

#[test]
fn us1_wrapper_single_subproject_transitive_edge() {
    let json = run_scan_with_gradle_resolve();

    let purls = maven_purls(&json);
    let direct = "pkg:maven/com.example.waybillfixture/direct@1.0.0";
    let transitive = "pkg:maven/com.example.waybillfixture/transitive@0.5.0";
    let test_only = "pkg:maven/com.example.waybillfixture/test-only@2.0.0";

    assert!(
        purls.iter().any(|p| p == direct),
        "expected direct dep component; got: {purls:?}"
    );
    assert!(
        purls.iter().any(|p| p == transitive),
        "expected transitive dep component (proves ASCII-tree parser recorded child of direct); got: {purls:?}"
    );
    assert!(
        purls.iter().any(|p| p == test_only),
        "expected testRuntimeClasspath dep component (Clarifications Q1 default set); got: {purls:?}"
    );

    // SC-001 acceptance — the transitive edge is emitted in
    // `dependencies[]`. This is the m235 US1 promise: subprocess
    // tier surfaces parent → child relationships that m106 lockfile
    // reading cannot.
    assert!(
        has_edge(&json, direct, transitive),
        "expected dependencies[] edge {direct} -> {transitive}; got: {}",
        serde_json::to_string_pretty(&json["dependencies"]).unwrap_or_default()
    );
}

// -----------------------------------------------------------
// FR-009 non-regression — scan WITHOUT --gradle-resolve emits
// zero m235 components (the fixture has no gradle.lockfile).
// -----------------------------------------------------------

#[test]
fn without_gradle_resolve_the_fixture_produces_no_maven_components() {
    // Verifies the m106-only path: without --gradle-resolve, waybill
    // looks for `gradle.lockfile` (absent in this fixture) and finds
    // nothing. FR-009 non-regression.
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = fixture();

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "no-flag scan failed unexpectedly:\n  stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let purls = maven_purls(&json);
    let fixture_maven: Vec<&String> = purls
        .iter()
        .filter(|p| p.contains("waybillfixture"))
        .collect();
    assert!(
        fixture_maven.is_empty(),
        "expected zero m235-fixture components without --gradle-resolve; got: {fixture_maven:?}"
    );
}

// -----------------------------------------------------------
// FR-006 / SC-004 — US4 tier annotation appears on Gradle-touching
// scans (subprocess variant).
// -----------------------------------------------------------

#[test]
fn us4_tier_annotation_present_on_subprocess_scan() {
    let json = run_scan_with_gradle_resolve();
    // FR-006: every scan touching a Gradle project MUST carry the
    // `waybill:gradle-resolution-tier` document-scope annotation.
    // With `--gradle-resolve` set and the mock wrapper emitting real
    // dependency output, the tier MUST be `subprocess`.
    let tier = doc_scope_property(&json, "waybill:gradle-resolution-tier");
    assert_eq!(
        tier.as_deref(),
        Some("subprocess"),
        "expected doc-scope waybill:gradle-resolution-tier=subprocess; got {tier:?}"
    );
}

// -----------------------------------------------------------
// FR-014 — per-scan INFO log summary lines emit once per scan
// with a `gradle-resolver:` prefix + per-project `<dir>=<tier>` pairs.
// -----------------------------------------------------------

#[test]
fn fr014_summary_log_emits_once_per_scan() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = fixture();

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    // Enable INFO-level tracing so the FR-014 summary line reaches
    // stderr where the test can inspect it. Only the gradle target
    // is enabled to keep the stderr scrubbable.
    cmd.env("RUST_LOG", "waybill::gradle=info");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--gradle-resolve",
        "--gradle-timeout-secs",
        "30",
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan failed:\n  stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let count = stderr.matches("gradle-resolver:").count();
    assert_eq!(
        count, 1,
        "expected exactly ONE `gradle-resolver:` summary log line per scan (FR-014); got {count}. \
         stderr:\n{stderr}"
    );
    // The wrapper_single_subproject fixture has one project directory
    // and the ladder ran (--gradle-resolve set + mock gradlew emits
    // real output) → tier MUST be `subprocess`.
    assert!(
        stderr.contains("subprocess"),
        "expected `subprocess` in FR-014 summary; got:\n{stderr}"
    );
}

// -----------------------------------------------------------
// FR-009 non-regression + tier annotation — no-wrapper-with-lockfile
// fixture emits m106 components AND doc-scope tier=`lockfile-only`.
// -----------------------------------------------------------

fn scan_fixture_no_flag(name: &str) -> serde_json::Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("gradle")
        .join(name);
    assert!(fixture_path.is_dir(), "fixture missing at {}", fixture_path.display());

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

#[test]
fn no_wrapper_with_lockfile_emits_lockfile_only_tier() {
    // Fixture has NO gradlew AND has `gradle.lockfile`. The m106
    // reader emits the lockfile components; the m235 ladder falls
    // through to `LockfileOnly` with the walker recording the
    // lockfile-only entry. Tier annotation MUST be `lockfile-only`.
    let json = scan_fixture_no_flag("no_wrapper_with_lockfile");

    // FR-009: m106 lockfile components MUST appear (byte-identity
    // preserved for pre-m235 scan behavior).
    let purls = maven_purls(&json);
    assert!(
        purls
            .iter()
            .any(|p| p == "pkg:maven/com.example.waybillfixture/lockfile-dep@1.0.0"),
        "expected m106 lockfile-dep component; got: {purls:?}"
    );

    // FR-006: tier annotation MUST be `lockfile-only`.
    let tier = doc_scope_property(&json, "waybill:gradle-resolution-tier");
    assert_eq!(
        tier.as_deref(),
        Some("lockfile-only"),
        "expected `lockfile-only` tier; got {tier:?}"
    );
}

// -----------------------------------------------------------
// FR-007 aggregate — mixed_tier fixture (one subproject with mock
// gradlew, one with lockfile-only) produces `mixed` tier value.
// -----------------------------------------------------------

#[test]
fn mixed_tier_fixture_produces_mixed_tier_annotation() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("gradle")
        .join("mixed_tier");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--gradle-resolve",
        "--gradle-timeout-secs",
        "30",
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan failed:\n  stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");

    // Both subprojects' expected components should appear (FR-009 for
    // the lockfile-only side; US1 subprocess for the wrapper side).
    let purls = maven_purls(&json);
    assert!(
        purls
            .iter()
            .any(|p| p == "pkg:maven/com.example.waybillfixture/wrapper-direct@1.0.0"),
        "expected subprocess-tier wrapper-direct component; got: {purls:?}"
    );
    assert!(
        purls
            .iter()
            .any(|p| p == "pkg:maven/com.example.waybillfixture/mixed-lockfile-dep@2.0.0"),
        "expected lockfile-tier mixed-lockfile-dep component; got: {purls:?}"
    );

    // FR-006 + FR-007: aggregate tier MUST be `mixed` when subprojects
    // resolved via different tiers.
    let tier = doc_scope_property(&json, "waybill:gradle-resolution-tier");
    assert_eq!(
        tier.as_deref(),
        Some("mixed"),
        "expected `mixed` tier annotation when subprojects differ; got {tier:?}"
    );
}

// -----------------------------------------------------------
// SC-005 — subprocess timeout: scan completes without hanging;
// ladder degrades to `LockfileOnly` when subprocess is killed.
// -----------------------------------------------------------

#[test]
fn sc005_subprocess_timeout_degrades_gracefully() {
    // Fixture's mock `./gradlew` sleeps 15s on any `:dependencies`
    // call. With `--gradle-timeout-secs 3` the scan MUST return
    // within a bounded time (5s cap gives generous CI headroom over
    // the 3s timeout + wait-thread + walker overhead) AND fall back
    // to `LockfileOnly` since no components were parsed.

    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("gradle")
        .join("timeout_wrapper");
    assert!(
        fixture_path.is_dir(),
        "fixture missing at {}",
        fixture_path.display()
    );

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--gradle-resolve",
        "--gradle-timeout-secs",
        "3",
        "--no-deep-hash",
    ]);

    let start = std::time::Instant::now();
    let output = cmd.output().expect("spawn waybill");
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "scan MUST succeed even when subprocess times out (fail-closed → LockfileOnly \
         graceful degrade). stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    // Two `:dependencies` invocations at 3s each = ~6s upper bound
    // for subprocess-side wall time (default configs =
    // runtimeClasspath + testRuntimeClasspath per clarify Q1).
    // Extra 4s headroom accommodates walker + CI cold-start jitter.
    // Well under the 15s sleep the mock uses — proves the timeout
    // actually kills the subprocess rather than waiting for its
    // natural exit.
    let upper_bound = std::time::Duration::from_secs(10);
    assert!(
        elapsed < upper_bound,
        "scan MUST complete within {upper_bound:?} of --gradle-timeout-secs 3 \
         (would take 15s+ if timeout didn't kill subprocess); took {elapsed:?}"
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");

    // FR-003 / FR-015: on subprocess timeout, ladder degrades to
    // `LockfileOnly`. Since the fixture has no gradle.lockfile
    // either, the tier annotation MUST reflect the LockfileOnly
    // sentinel (recorded by the walker via the ladder's empty
    // fallback graph).
    let tier = doc_scope_property(&json, "waybill:gradle-resolution-tier");
    assert_eq!(
        tier.as_deref(),
        Some("lockfile-only"),
        "expected `lockfile-only` tier after subprocess timeout \
         (ladder degrades gracefully); got {tier:?}"
    );

    // No fixture components should appear (the mock never emitted any).
    let purls = maven_purls(&json);
    let fixture_maven: Vec<&String> = purls
        .iter()
        .filter(|p| p.contains("waybillfixture"))
        .collect();
    assert!(
        fixture_maven.is_empty(),
        "timed-out subprocess must not contribute components; got: {fixture_maven:?}"
    );
}

// -----------------------------------------------------------
// US2 (Phase 4) — no-wrapper-warm-cache fixture emits cache-tier
// components + transitive edge via US2 cache reader.
// -----------------------------------------------------------

#[test]
fn us2_warm_cache_produces_transitive_edge_and_cache_tier() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("gradle")
        .join("no_wrapper_warm_cache");
    let cache_path = fixture_path.join("gradle-cache");
    assert!(cache_path.is_dir(), "cache fixture missing at {}", cache_path.display());

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    // Point the m235 cache reader at the fixture's fake gradle cache.
    cmd.env("WAYBILL_TEST_GRADLE_CACHE", cache_path.to_str().unwrap());
    // Deliberately DO NOT pass --gradle-resolve. US1 should short-
    // circuit at OperatorOptOut, US2 fires and produces the graph.
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan failed:\n  stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");

    let purls = maven_purls(&json);
    let root = "pkg:maven/com.example.waybillfixture/cache-root@1.0.0";
    let leaf = "pkg:maven/com.example.waybillfixture/cache-leaf@2.0.0";

    // Both direct seed AND transitive dep (parsed from the seed's
    // cached POM) MUST appear.
    assert!(
        purls.iter().any(|p| p == root),
        "expected direct seed component from cache; got: {purls:?}"
    );
    assert!(
        purls.iter().any(|p| p == leaf),
        "expected transitive component from cached POM's <dependencies>; got: {purls:?}"
    );

    // Transitive edge from root → leaf synthesized via scan_fs's
    // depends-resolution.
    assert!(
        has_edge(&json, root, leaf),
        "expected dependencies[] edge {root} -> {leaf}; got: {}",
        serde_json::to_string_pretty(&json["dependencies"]).unwrap_or_default()
    );

    // FR-006: doc-scope tier annotation MUST be `cache`.
    let tier = doc_scope_property(&json, "waybill:gradle-resolution-tier");
    assert_eq!(
        tier.as_deref(),
        Some("cache"),
        "expected `cache` tier annotation when US2 succeeded; got {tier:?}"
    );
}
