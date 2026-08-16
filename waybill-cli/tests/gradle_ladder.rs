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

/// Return the value of the named per-component `properties[]` entry
/// on the CDX component whose `purl == target_purl`. `None` when the
/// component is absent OR the property is absent on that component.
fn component_property(
    json: &serde_json::Value,
    target_purl: &str,
    name: &str,
) -> Option<String> {
    let component = json["components"]
        .as_array()?
        .iter()
        .find(|c| c["purl"].as_str() == Some(target_purl))?;
    component["properties"]
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
fn without_gradle_resolve_transitive_edge_absent() {
    // FR-009 non-regression + US3 shape check: without
    // `--gradle-resolve`, the m235 static parser (US3, Phase 5) still
    // emits the fixture's DIRECT deps (declared in build.gradle) but
    // the SUBPROCESS-only transitive edge from direct → transitive
    // MUST be absent. That transitive edge is only recoverable via
    // US1's real dependency-tree call (or US2 cache hit).
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = fixture();

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    // Point cache reader at a definitely-absent path so US2 fails
    // cleanly regardless of the test host's real ~/.gradle state.
    cmd.env(
        "WAYBILL_TEST_GRADLE_CACHE",
        workdir.path().join("nonexistent-cache").to_str().unwrap(),
    );
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
    let direct = "pkg:maven/com.example.waybillfixture/direct@1.0.0";
    let transitive = "pkg:maven/com.example.waybillfixture/transitive@0.5.0";
    // The direct-declared dep MUST appear (US3 emits it).
    let purls = maven_purls(&json);
    assert!(
        purls.iter().any(|p| p == direct),
        "expected US3-emitted direct dep; got: {purls:?}"
    );
    // The TRANSITIVE dep MUST NOT appear — it's only encoded in
    // the mock gradlew's ASCII-tree output, which US3 doesn't
    // invoke.
    assert!(
        !purls.iter().any(|p| p == transitive),
        "transitive dep should NOT appear without --gradle-resolve; got: {purls:?}"
    );
    // And no edge from direct → transitive.
    assert!(
        !has_edge(&json, direct, transitive),
        "transitive edge should NOT appear without --gradle-resolve"
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

    // FR-003 / FR-015: on subprocess timeout, ladder degrades
    // gracefully. Since the fixture's build.gradle declares no
    // dependencies, US3 succeeds with 0 components; no lockfile
    // either. The tier annotation is either absent (no component
    // source contributed) or reflects a fallback tier — both
    // acceptable; the acceptance criterion is "scan doesn't hang
    // and exits cleanly," not a specific tier value.
    let tier = doc_scope_property(&json, "waybill:gradle-resolution-tier");
    assert!(
        matches!(tier.as_deref(), None | Some("static") | Some("lockfile-only")),
        "expected None/static/lockfile-only tier after subprocess timeout; got {tier:?}"
    );

    // No fixture components should appear (the mock never emitted
    // any; the fixture's build.gradle declares no deps).
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
// US4 (C147) — subprocess timeout records
// `waybill:gradle-fallback-reason = "subprocess:timeout"` at doc scope.
// -----------------------------------------------------------

#[test]
fn c147_subprocess_timeout_emits_fallback_reason_annotation() {
    // Same fixture as sc005 — mock gradlew sleeps 15s; --gradle-resolve
    // with --gradle-timeout-secs 3 kills the subprocess, the ladder
    // degrades, and the fallback_history captures (Subprocess, Timeout).
    // C147 aggregates that into `subprocess:timeout` at doc scope
    // (excluding pure operator-opt-out reasons).

    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("gradle")
        .join("timeout_wrapper");

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

    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan MUST succeed even when subprocess times out. stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");

    let reason = doc_scope_property(&json, "waybill:gradle-fallback-reason");
    // Sorted BTreeSet iteration on the tier enum: Subprocess (declared
    // first) < Cache. Timeout is real; MissingTool comes from the cache
    // reader having no gradle-caches dir. Both survive the opt-out
    // filter. Result: `subprocess:timeout,cache:missing-tool`.
    assert_eq!(
        reason.as_deref(),
        Some("subprocess:timeout,cache:missing-tool"),
        "expected C147 doc-scope waybill:gradle-fallback-reason with sorted timeout+missing-tool; got {reason:?}"
    );
}

// -----------------------------------------------------------
// US4 (C147) — cold-clone scan aggregates cache-miss + partial
// static-miss into `cache:missing-tool,static:no-source-files` and
// filters `subprocess:operator-opt-out` (the default no-flag path).
// -----------------------------------------------------------

#[test]
fn c147_aggregates_multi_subproject_fallbacks_and_filters_opt_out() {
    // The cold-clone US3 fixture: no --gradle-resolve, so US1 records
    // `OperatorOptOut` — which C147 explicitly filters. US2 records
    // `MissingTool` (no cache set). US3 succeeds for app/ + core/ but
    // fails at the settings-only root with `NoSourceFiles`. Both
    // survivors of the opt-out filter aggregate into the annotation.

    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("gradle")
        .join("cold_clone_static_only");

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
        "scan failed. stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");

    let reason = doc_scope_property(&json, "waybill:gradle-fallback-reason");
    // Multi-subproject fixture: US2 cache fails (MissingTool) for
    // every project; US3 succeeds for app/ + core/ (they have
    // build.gradle) but fails at the root (settings.gradle only).
    // C147 aggregates + sorts + dedups — expected value is exactly
    // `cache:missing-tool,static:no-source-files`. Crucially, this
    // MUST NOT include `subprocess:operator-opt-out` — that's the
    // whole point of the filter.
    assert_eq!(
        reason.as_deref(),
        Some("cache:missing-tool,static:no-source-files"),
        "expected sorted-joined cache+static fallbacks (opt-out filtered); got {reason:?}"
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

// -----------------------------------------------------------
// US3 (Phase 5) — cold-clone with no wrapper / no cache / no
// lockfile emits direct-only components via static parser AND
// tier annotation reflects `static`.
// -----------------------------------------------------------

#[test]
fn us3_cold_clone_static_emits_direct_components_and_static_tier() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("gradle")
        .join("cold_clone_static_only");
    assert!(
        fixture_path.is_dir(),
        "fixture missing at {}",
        fixture_path.display()
    );

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    // Point the cache reader at a definitely-absent path so US2
    // fails cleanly. Without this the reader falls back to the
    // real ~/.gradle/caches which the test host may have populated.
    cmd.env(
        "WAYBILL_TEST_GRADLE_CACHE",
        workdir.path().join("nonexistent-cache").to_str().unwrap(),
    );
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
        "--include-declared-deps",
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
    let app_runtime = "pkg:maven/com.example.waybillfixture/app-runtime-dep@1.0.0";
    let app_test = "pkg:maven/com.example.waybillfixture/app-test-dep@2.0.0";
    let core_api = "pkg:maven/com.example.waybillfixture/core-api-dep@3.0.0";

    // Both subprojects' direct deps must appear. The walker visits
    // `app/` and `core/` independently; each triggers US3 for its
    // own build.gradle.kts.
    assert!(
        purls.iter().any(|p| p == app_runtime),
        "expected app subproject's runtime dep; got: {purls:?}"
    );
    assert!(
        purls.iter().any(|p| p == app_test),
        "expected app subproject's test dep; got: {purls:?}"
    );
    assert!(
        purls.iter().any(|p| p == core_api),
        "expected core subproject's api dep; got: {purls:?}"
    );

    // FR-006: doc-scope tier annotation MUST be `static`.
    let tier = doc_scope_property(&json, "waybill:gradle-resolution-tier");
    assert_eq!(
        tier.as_deref(),
        Some("static"),
        "expected `static` tier when US3 succeeded; got {tier:?}"
    );
}

// -----------------------------------------------------------
// US4 follow-on (C149) — no_wrapper_warm_cache fixture emits
// `waybill:cache-freshness` per-component on cache-tier components.
// -----------------------------------------------------------

#[test]
fn c149_warm_cache_emits_cache_freshness_annotation() {
    // Same fixture + env-override as us2_warm_cache_produces_transitive_edge.
    // The mock cache tree's .pom mtimes are set at fixture-materialization
    // time (i.e., NOW) — build.gradle is committed to git so its mtime
    // is at checkout time (older). Cache-freshness comparison: newest
    // .pom > build.gradle → `fresh`.

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

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.env("WAYBILL_TEST_GRADLE_CACHE", cache_path.to_str().unwrap());
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

    // C149 present on both the direct seed AND the transitive leaf
    // (both came from cache-tier resolution).
    let root_freshness = component_property(
        &json,
        "pkg:maven/com.example.waybillfixture/cache-root@1.0.0",
        "waybill:cache-freshness",
    );
    assert!(
        matches!(root_freshness.as_deref(), Some("fresh") | Some("stale")),
        "expected C149 waybill:cache-freshness on cache-root; got {root_freshness:?}"
    );
    let leaf_freshness = component_property(
        &json,
        "pkg:maven/com.example.waybillfixture/cache-leaf@2.0.0",
        "waybill:cache-freshness",
    );
    assert!(
        matches!(leaf_freshness.as_deref(), Some("fresh") | Some("stale")),
        "expected C149 waybill:cache-freshness on cache-leaf; got {leaf_freshness:?}"
    );
    // Both components come from the same cache resolution — freshness
    // MUST agree.
    assert_eq!(
        root_freshness, leaf_freshness,
        "C149 freshness should be identical across components from the same project"
    );
}

// -----------------------------------------------------------
// US4 follow-on (C148) — mixed_tier fixture emits per-component
// `waybill:gradle-subproject-tier` tagging each component with the
// ladder tier that produced it (`subprocess` vs `lockfile-only`).
// -----------------------------------------------------------

#[test]
fn c148_mixed_tier_fixture_tags_components_with_subproject_tier() {
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

    // Wrapper-project component came from US1 subprocess.
    let wrapper_tier = component_property(
        &json,
        "pkg:maven/com.example.waybillfixture/wrapper-direct@1.0.0",
        "waybill:gradle-subproject-tier",
    );
    assert_eq!(
        wrapper_tier.as_deref(),
        Some("subprocess"),
        "wrapper-direct should carry C148 subproject-tier=subprocess; got {wrapper_tier:?}"
    );

    // Lockfile-project component came from the m106 lockfile pass.
    let lockfile_tier = component_property(
        &json,
        "pkg:maven/com.example.waybillfixture/mixed-lockfile-dep@2.0.0",
        "waybill:gradle-subproject-tier",
    );
    assert_eq!(
        lockfile_tier.as_deref(),
        Some("lockfile-only"),
        "mixed-lockfile-dep should carry C148 subproject-tier=lockfile-only; got {lockfile_tier:?}"
    );
}
