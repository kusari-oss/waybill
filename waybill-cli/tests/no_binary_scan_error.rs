//! Milestone 665 T028 — SC-007 error-surface tests for
//! `--no-binary-scan=<MODE>`.
//!
//! Guards three regression axes:
//!
//! 1. **Unrecognized mode** (`--no-binary-scan=xyz`): non-zero exit
//!    code + stderr names the recognized modes. Per FR-009 + contract
//!    `contracts/cli-flag.md` C6 rows 3 and 7.
//! 2. **Bare flag** (`--no-binary-scan` with no `=<MODE>`): non-zero
//!    exit code, message names the missing value. Per FR-001 +
//!    contract row "Bare `--no-binary-scan` (no `=<MODE>`)".
//! 3. **Empty env-var** (`WAYBILL_NO_BINARY_SCAN=` with empty value):
//!    treated as absent → scan succeeds with byte-identity output
//!    (annotation elided). Per contract "Empty string treated as
//!    absent" clause under §Environment variable.
//!
//! Env-var tests acquire `waybill::testing::EnvGuard` per T029 +
//! contract T1 + project memory `reference_podman_test_flake` — the
//! shared save-and-restore RAII guard serializes WAYBILL_NO_BINARY_SCAN
//! mutations with every other env-mutating test in the workspace
//! (fixes the flake class documented by `reference_podman_test_flake`
//! + `reference_m205_cargo_metadata_env_flake`).

use std::process::Command;

use waybill::testing::EnvGuard;

/// FR-009 + C6 row 3: `--no-binary-scan=xyz` must be rejected with a
/// non-zero exit code AND stderr must name the recognized modes so
/// operators know which value(s) to use.
#[test]
fn unrecognized_mode_fails_with_recognized_modes_listed() {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let output = Command::new(bin)
        .args(["sbom", "scan", "--no-binary-scan=xyz", "--path", "."])
        .output()
        .expect("waybill should run");

    assert!(
        !output.status.success(),
        "expected non-zero exit code for `--no-binary-scan=xyz`, got \
         success. stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("xyz"),
        "expected stderr to name the rejected value `xyz`, got:\n{stderr}",
    );
    assert!(
        stderr.contains("go"),
        "expected stderr to name the recognized value `go` so operators \
         know what to pass, got:\n{stderr}",
    );
    assert!(
        stderr.contains("no-binary-scan"),
        "expected stderr to name the offending flag, got:\n{stderr}",
    );
}

/// FR-001: bare `--no-binary-scan` with no value must be rejected —
/// the mode is REQUIRED. clap's `Option<BinaryScanMode>` with `value_enum`
/// derive handles this automatically; this test guards against a
/// future accidental change to `default_value`/`num_args` semantics
/// that would silently accept the bare form.
#[test]
fn bare_flag_without_value_fails() {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let output = Command::new(bin)
        .args(["sbom", "scan", "--no-binary-scan", "--path", "."])
        .output()
        .expect("waybill should run");

    assert!(
        !output.status.success(),
        "expected non-zero exit code for bare `--no-binary-scan`, got \
         success. stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no-binary-scan"),
        "expected stderr to name the flag missing its value, got:\n{stderr}",
    );
    assert!(
        stderr.contains("value"),
        "expected stderr to mention that a value is required, got:\n{stderr}",
    );
}

/// Contract §Environment variable, "Empty string treated as absent":
/// setting `WAYBILL_NO_BINARY_SCAN=""` must NOT engage suppression —
/// the effective mode is `None`, the scan succeeds, and the
/// document-scope annotation is elided (byte-identity default per
/// FR-003).
///
/// Guards against a regression where clap's `env` attribute might
/// treat an empty env-var as an unrecognized value and abort. Uses
/// `EnvGuard` to serialize the WAYBILL_NO_BINARY_SCAN mutation with
/// every other env-mutating test in the workspace per contract T1 +
/// project memory `reference_podman_test_flake`. The RAII drop
/// restores the pre-test env state automatically even on panic —
/// safer than manual save/restore around the test body.
#[test]
fn empty_env_var_treated_as_absent() {
    let mut env = EnvGuard::acquire();
    env.set("WAYBILL_NO_BINARY_SCAN", "");

    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("out.cdx.json");
    let output = Command::new(bin)
        .args([
            "--offline",
            "sbom",
            "scan",
            "--path",
        ])
        .arg(tmp.path())
        .arg("--output")
        .arg(&out_path)
        .arg("--format")
        .arg("cyclonedx-json")
        // Explicit: no `--no-binary-scan` flag; env-var is what's
        // being probed here. EnvGuard::drop restores the prior
        // WAYBILL_NO_BINARY_SCAN state on scope exit — automatic
        // cleanup even on assertion panic.
        .output()
        .expect("waybill should run");

    assert!(
        output.status.success(),
        "empty WAYBILL_NO_BINARY_SCAN must be treated as absent \
         (contract §Environment variable), but scan failed. \
         stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let sbom: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read out"),
    )
    .expect("valid CDX JSON");
    let annotation = sbom["metadata"]["properties"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|p| p["name"].as_str() == Some("waybill:binary-scan-suppressed"))
        });
    assert!(
        annotation.is_none(),
        "FR-003 byte-identity: empty WAYBILL_NO_BINARY_SCAN must \
         elide the suppression annotation, got: {annotation:?}",
    );
}
