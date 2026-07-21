//! Milestone 175: design-tier component visibility advisory integration tests.
//!
//! Covers:
//!
//! * **US2 (P1)**: advisory-log fires exactly once when the scan detects
//!   ≥1 design-tier component AND the scan produced ≥1 component AND
//!   `WAYBILL_NO_DESIGN_TIER_ADVISORY` is unset. Silent otherwise.
//! * **FR-002 offline-orthogonality (SC-005)**: advisory fires under
//!   `--offline` (unlike the m173 warming advisory).
//! * **FR-009 cross-ecosystem coverage** (per /speckit-analyze C1 remediation):
//!   advisory fires on non-Python design-tier fixtures too (Ruby Gemfile
//!   without Gemfile.lock).
//!
//! Mirrors the m173 (`warm_go_cache.rs`) + m176 (`workspace_visibility.rs`)
//! integration-test scaffolding: `assert_cmd`-based release-independent
//! subprocess with `apply_fake_home_env` for HOME isolation.

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;

/// Load-bearing stable substring per FR-003 + contracts/advisory-log-contract.md.
/// CI dashboards `grep -F` this token to detect design-tier scans.
const ADVISORY_SUBSTRING: &str = "design-tier components detected: ";

/// Env-var suppression name per data-model.md §Entity 2.
const SUPPRESS_ENV_VAR: &str = "WAYBILL_NO_DESIGN_TIER_ADVISORY";

/// Count how many times the stable advisory substring appears in the
/// captured stderr. Equivalent to `grep -cF 'design-tier components detected: '`.
fn advisory_hit_count(stderr: &str) -> usize {
    stderr.matches(ADVISORY_SUBSTRING).count()
}

/// Scan `path` with `--offline`, capturing the emitted CDX SBOM + stderr.
/// Optionally sets the suppression env var to the given truthy value.
fn scan_with_env(path: &Path, suppress_value: Option<&str>) -> String {
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");

    let mut cmd = Command::new(bin());
    common::normalize::apply_fake_home_env(&mut cmd, fake_home.path());
    if let Some(v) = suppress_value {
        cmd.env(SUPPRESS_ENV_VAR, v);
    } else {
        // Explicitly unset the env var so ambient environment doesn't leak
        // suppression into tests that expect the advisory to fire.
        cmd.env_remove(SUPPRESS_ENV_VAR);
    }
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");

    let output = cmd.output().expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed (exit={:?}): stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Synthesize a pip `requirements.txt`-only fixture (constraint-only —
/// no `pyproject.toml`, no lockfile). Every entry becomes a design-tier
/// component per the milestone-068 pip reader.
fn write_pip_requirements_fixture(dir: &Path) {
    std::fs::write(
        dir.join("requirements.txt"),
        "requests>=2.31.0\n\
         click>=8.1.7\n\
         pyyaml>=6.0\n",
    )
    .expect("write requirements.txt");
}

/// Synthesize an npm-with-lockfile fixture — every dep is resolved via
/// `package-lock.json` → source-tier components, zero design-tier.
fn write_npm_with_lockfile_fixture(dir: &Path) {
    std::fs::write(
        dir.join("package.json"),
        "{\"name\": \"clean-npm\", \"version\": \"0.1.0\", \
         \"dependencies\": {\"lodash\": \"^4.17.21\"}}",
    )
    .expect("write package.json");
    std::fs::write(
        dir.join("package-lock.json"),
        "{\"name\": \"clean-npm\", \"version\": \"0.1.0\", \"lockfileVersion\": 3, \
         \"requires\": true, \"packages\": {\
         \"\": {\"name\": \"clean-npm\", \"version\": \"0.1.0\", \
                \"dependencies\": {\"lodash\": \"^4.17.21\"}}, \
         \"node_modules/lodash\": {\"version\": \"4.17.21\", \
                \"resolved\": \"https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz\"}}}",
    )
    .expect("write package-lock.json");
}

/// Synthesize a PHP Composer manifest-only fixture (no `composer.lock`).
/// Every `require:` entry becomes a design-tier component per the m138
/// composer reader's fallback path — exercises FR-009 (non-Python
/// cross-ecosystem coverage).
fn write_composer_manifest_fixture(dir: &Path) {
    std::fs::write(
        dir.join("composer.json"),
        "{\
           \"name\": \"m175/design-tier-test\", \
           \"require\": {\
             \"symfony/console\": \"^6.4\", \
             \"guzzlehttp/guzzle\": \"^7.8\"\
           }\
         }",
    )
    .expect("write composer.json");
}

// -----------------------------------------------------------------------
// US2 T005 — 5 acceptance tests + 1 FR-009 non-Python test (C1 remediation).
// -----------------------------------------------------------------------

/// SC-002 gate — pip `requirements.txt`-only scan fires exactly one
/// advisory. Body carries the exact count + at least one remediation
/// keyword + the docs anchor path.
#[test]
fn t001_advisory_fires_once_on_design_tier_scan() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_pip_requirements_fixture(tmp.path());

    let stderr = scan_with_env(tmp.path(), None);

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 1,
        "FR-002 + SC-002: expected exactly one advisory line matching {ADVISORY_SUBSTRING:?} on a \
         pip requirements-only scan; got {hits}. Full stderr:\n{stderr}"
    );

    // Body must carry a remediation keyword (FR-003).
    let has_remediation = stderr.contains("lockfile") || stderr.contains("venv");
    assert!(
        has_remediation,
        "FR-003: advisory body should carry a remediation keyword (lockfile / venv); full stderr:\n{stderr}"
    );

    // Body must reference the reading-guide anchor.
    assert!(
        stderr.contains("docs/reference/reading-a-waybill-sbom.md"),
        "FR-003: advisory body should reference the reading-guide docs path; full stderr:\n{stderr}"
    );
}

/// SC-003 gate — fully-resolved npm scan (with package-lock.json) fires
/// zero advisories. Design-tier count is zero, advisory is suppressed
/// by the FR-002 predicate.
#[test]
fn t002_advisory_silent_on_zero_design_tier() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_npm_with_lockfile_fixture(tmp.path());

    let stderr = scan_with_env(tmp.path(), None);

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 0,
        "SC-003: fully-resolved (lockfile-present) scan MUST emit zero advisories; got {hits}. \
         Full stderr:\n{stderr}"
    );
}

/// SC-004 gate — env-var suppression silences the advisory regardless
/// of design-tier count. Tests both truthy values (`1` + `true`
/// case-insensitive).
#[test]
fn t003_advisory_silent_on_suppression_env_var() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_pip_requirements_fixture(tmp.path());

    for truthy in ["1", "true", "TRUE", "True"] {
        let stderr = scan_with_env(tmp.path(), Some(truthy));
        let hits = advisory_hit_count(&stderr);
        assert_eq!(
            hits, 0,
            "SC-004: {SUPPRESS_ENV_VAR}={truthy:?} MUST silence the advisory on a design-tier scan; \
             got {hits} hit(s). Full stderr:\n{stderr}"
        );
    }

    // Sanity: non-truthy value does NOT suppress.
    let stderr = scan_with_env(tmp.path(), Some("no"));
    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 1,
        "SC-004 negative: {SUPPRESS_ENV_VAR}=\"no\" MUST NOT silence the advisory; got {hits}. \
         Full stderr:\n{stderr}"
    );
}

/// SC-005 gate — advisory fires under `--offline` on a design-tier
/// scan. `scan_with_env` already passes `--offline`, so t001 already
/// covers this; documenting it explicitly here locks in FR-002's
/// offline-orthogonality contract.
#[test]
fn t004_advisory_fires_under_offline() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_pip_requirements_fixture(tmp.path());

    // scan_with_env passes --offline unconditionally.
    let stderr = scan_with_env(tmp.path(), None);

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 1,
        "SC-005 + FR-002: advisory MUST fire under --offline on a design-tier scan \
         (m175 is orthogonal to --offline, unlike m173 warming advisory); got {hits}. \
         Full stderr:\n{stderr}"
    );
}

/// Edge case per spec §Edge Cases — empty scan target (no manifests)
/// emits zero components, so the advisory MUST NOT fire (predicate 2:
/// !components.is_empty()).
#[test]
fn t005_advisory_silent_on_empty_scan_target() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("README.txt"), b"empty scan target\n")
        .expect("write README");

    let stderr = scan_with_env(tmp.path(), None);

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 0,
        "Edge case: empty scan target (zero manifests, zero components) MUST emit zero \
         advisories; got {hits}. Full stderr:\n{stderr}"
    );
}

/// FR-009 gate (per /speckit-analyze C1 remediation) — advisory fires
/// on a non-Python design-tier fixture. PHP `composer.json` without
/// `composer.lock` exercises the m138 composer reader's design-tier
/// fallback path (`emit_design_tier_components`). Verifies FR-009's
/// "same wording pattern" clause holds cross-ecosystem.
#[test]
fn t006_advisory_fires_on_non_python_design_tier() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_composer_manifest_fixture(tmp.path());

    let stderr = scan_with_env(tmp.path(), None);

    let hits = advisory_hit_count(&stderr);
    assert_eq!(
        hits, 1,
        "FR-009: advisory MUST fire on non-Python design-tier scans (Ruby Gemfile without \
         Gemfile.lock exercises the m069 gem reader's constraint-only path); got {hits}. \
         Full stderr:\n{stderr}"
    );

    // Same wording pattern — remediation keyword + docs anchor still present.
    let has_remediation = stderr.contains("lockfile") || stderr.contains("venv");
    assert!(
        has_remediation,
        "FR-009: advisory body wording MUST match cross-ecosystem (remediation keyword present); \
         full stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("docs/reference/reading-a-waybill-sbom.md"),
        "FR-009: advisory body wording MUST match cross-ecosystem (docs anchor present); \
         full stderr:\n{stderr}"
    );
}
