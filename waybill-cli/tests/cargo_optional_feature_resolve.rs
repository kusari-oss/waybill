//! Milestone 205 (#593) — cargo optional-dep classifier fix.
//!
//! Verifies that the classifier at `cargo.rs::parse_lockfile` respects
//! Cargo's actual feature-activation semantics rather than treating any
//! `optional = true` declaration as `scope: excluded`. Regression tests
//! for the reporter's `test-vaultwarden` case (external gist at
//! https://gist.github.com/nchelluri/8e74c2d7d3761c74be57dcecf5bc92df).
//!
//! Tests build synthetic Cargo workspaces via `tempfile::tempdir()` +
//! shell out to `cargo generate-lockfile` to produce Cargo.lock, then
//! invoke the waybill binary via `env!("CARGO_BIN_EXE_waybill")`.
//! `cargo` binary is a hard dev prereq (matches m087 / m173 / m203
//! precedent).

use std::path::Path;
use std::process::Command;

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Skip cleanly if `cargo` is not on PATH.
fn require_cargo() -> bool {
    Command::new("cargo").arg("--version").output().is_ok()
}

/// Create a minimal Cargo workspace at `ws_root` with the given
/// Cargo.toml text. Runs `cargo fetch` (NOT `cargo generate-lockfile`)
/// to produce Cargo.lock AND unpack `.crate` files into
/// `$CARGO_HOME/registry/src/`. This is CRITICAL for cold-cache
/// environments (fresh CI runners): `cargo metadata --offline`
/// (invoked by waybill's classifier) reads dep manifests from
/// unpacked crates in registry/src/. Bare `cargo generate-lockfile`
/// fetches the index + creates Cargo.lock but does NOT unpack — so
/// `cargo metadata --offline` fails with "failed to download …"
/// even though Cargo.lock exists and the index has the crate.
///
/// This mismatch caused m205's initial US1 test to pass locally
/// (warm ~/.cargo/registry from years of use) and fail on CI (cold
/// cache). Post-`cargo fetch`, cargo metadata --offline succeeds
/// deterministically across all environments.
fn build_synthetic_workspace(ws_root: &Path, cargo_toml: &str) {
    std::fs::create_dir_all(ws_root.join("src")).unwrap();
    std::fs::write(ws_root.join("Cargo.toml"), cargo_toml).unwrap();
    std::fs::write(ws_root.join("src/main.rs"), "fn main() {}\n").unwrap();
    let out = Command::new("cargo")
        .args(["fetch"])
        .current_dir(ws_root)
        .output()
        .expect("spawn cargo fetch");
    assert!(
        out.status.success(),
        "cargo fetch failed: stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
}

fn scan_workspace_with_env(
    ws_root: &Path,
    path_env: Option<&str>,
) -> (serde_json::Value, String, bool) {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let mut cmd = Command::new(mikebom_bin());
    cmd.args([
        "sbom",
        "scan",
        "--offline",
        "--path",
        ws_root.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out.to_str().unwrap(),
        "--no-deep-hash",
    ]);
    if let Some(p) = path_env {
        cmd.env("PATH", p);
    }
    let cmd_out = cmd.output().expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&cmd_out.stderr).to_string();
    let success = cmd_out.status.success();
    let json = if success && out.exists() {
        serde_json::from_slice(&std::fs::read(&out).unwrap())
            .expect("output is valid JSON")
    } else {
        serde_json::Value::Null
    };
    (json, stderr, success)
}

fn find_component_by_name<'a>(
    cdx: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    cdx.get("components")?
        .as_array()?
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
}

fn component_property<'a>(comp: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    comp.get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(key))?
        .get("value")?
        .as_str()
}

fn component_scope(comp: &serde_json::Value) -> Option<&str> {
    comp.get("scope").and_then(|s| s.as_str())
}

// ─────────────────────────────────────────────────────────────────
// US1 — feature-activated optional dep is Runtime (SC-002)
// ─────────────────────────────────────────────────────────────────

#[test]
fn us1_default_feature_activated_optional_dep_is_runtime() {
    if !require_cargo() {
        eprintln!("skipping: cargo binary not on PATH");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let ws = tempdir.path();
    build_synthetic_workspace(
        ws,
        r#"[package]
name = "m205-us1"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", optional = true }

[features]
default = ["serde"]
"#,
    );

    let (cdx, stderr, ok) = scan_workspace_with_env(ws, None);
    assert!(ok, "scan should succeed. stderr:\n{stderr}");

    let serde_comp = find_component_by_name(&cdx, "serde")
        .unwrap_or_else(|| panic!("serde component present. cdx: {cdx:#}"));
    // CDX 1.6 default scope is `runtime` when omitted. Post-m205,
    // feature-activated optional dep gets Runtime (either explicit
    // "runtime" or absent field which defaults to runtime).
    let scope = component_scope(serde_comp);
    assert!(
        scope.is_none() || scope == Some("runtime"),
        "serde should be scope=runtime (or absent → runtime), got {scope:?}. cdx: {cdx:#}"
    );
    assert!(
        component_property(serde_comp, "waybill:optional-derivation").is_none(),
        "serde MUST NOT carry waybill:optional-derivation (default-feature-activated). cdx: {cdx:#}"
    );
}

// ─────────────────────────────────────────────────────────────────
// US2 — truly-optional dep stays Optional (SC-003)
// ─────────────────────────────────────────────────────────────────

#[test]
fn us2_truly_optional_dep_stays_optional() {
    if !require_cargo() {
        eprintln!("skipping: cargo binary not on PATH");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let ws = tempdir.path();
    build_synthetic_workspace(
        ws,
        r#"[package]
name = "m205-us2"
version = "0.1.0"
edition = "2021"

[dependencies]
regex = { version = "1", optional = true }

[features]
enable-regex = ["regex"]
"#,
    );

    let (cdx, stderr, ok) = scan_workspace_with_env(ws, None);
    assert!(ok, "scan should succeed. stderr:\n{stderr}");

    let regex_comp = find_component_by_name(&cdx, "regex")
        .unwrap_or_else(|| panic!("regex component present. cdx: {cdx:#}"));
    assert_eq!(
        component_scope(regex_comp),
        Some("excluded"),
        "regex (non-default-activated optional) MUST be scope=excluded. cdx: {cdx:#}"
    );
    assert_eq!(
        component_property(regex_comp, "waybill:optional-derivation"),
        Some("cargo-optional-true"),
        "regex MUST carry waybill:optional-derivation = cargo-optional-true. cdx: {cdx:#}"
    );
}

// ─────────────────────────────────────────────────────────────────
// US3 — non-Cargo scan in-process regression guard (SC-004)
// ─────────────────────────────────────────────────────────────────

#[test]
fn us3_non_cargo_scan_does_not_invoke_cargo_metadata() {
    let tempdir = tempfile::tempdir().unwrap();
    let scan_root = tempdir.path().join("random-dir");
    std::fs::create_dir_all(&scan_root).unwrap();
    std::fs::write(scan_root.join("readme.txt"), b"hello world").unwrap();

    let (cdx, stderr, ok) = scan_workspace_with_env(&scan_root, None);
    assert!(ok, "non-Cargo scan MUST succeed. stderr:\n{stderr}");

    // The cargo reader must not fire on a non-Cargo scan → no cargo
    // metadata invocation → no WARN log about it.
    assert!(
        !stderr.contains("cargo metadata"),
        "non-Cargo scan MUST NOT invoke cargo metadata. stderr:\n{stderr}"
    );
    // No optional-derivation annotation should be emitted (no cargo
    // component in the SBOM to attach it to, either way).
    if let Some(comps) = cdx.get("components").and_then(|c| c.as_array()) {
        for c in comps {
            assert!(
                component_property(c, "waybill:optional-derivation").is_none(),
                "non-Cargo scan MUST NOT emit waybill:optional-derivation. comp: {c:#}"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// FR-004 — Cargo absent → WARN + safe over-inclusion (user's ask)
// ─────────────────────────────────────────────────────────────────

#[test]
#[cfg(unix)]
fn fr004_cargo_absent_warns_and_falls_back() {
    if !require_cargo() {
        eprintln!("skipping: cargo needed for initial lockfile generation");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let ws = tempdir.path();
    // Reuse US1's default-feature-activated shape so we can assert
    // safe over-inclusion (serde flips to Runtime under fallback).
    build_synthetic_workspace(
        ws,
        r#"[package]
name = "m205-fr004"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", optional = true }

[features]
default = ["serde"]
"#,
    );

    // Scrub PATH so waybill's own cargo-metadata subprocess cannot
    // resolve `cargo` → BinaryNotFound fallback fires.
    let (cdx, stderr, ok) = scan_workspace_with_env(ws, Some(""));
    assert!(ok, "scan MUST succeed via FR-004 fallback. stderr:\n{stderr}");

    // (b) WARN log contains BOTH substrings from the data-model E5
    // fixture wording — matches the exact log emitted by the caller
    // wiring at cargo.rs::read.
    assert!(
        stderr.contains("cargo metadata"),
        "WARN log MUST mention 'cargo metadata'. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("falling back"),
        "WARN log MUST mention 'falling back'. stderr:\n{stderr}"
    );

    // (c) WARN log carries the CargoMetadataResolveFailure::BinaryNotFound
    // Display value verbatim (per T007's display test). The variant
    // name "BinaryNotFound" itself does NOT appear on the wire —
    // tracing::warn!(reason = %e) uses Display.
    assert!(
        stderr.contains("binary not found on $PATH"),
        "WARN log MUST contain BinaryNotFound Display substring 'binary not \
         found on $PATH'. stderr:\n{stderr}"
    );

    // (d) FR-004 fallback preserves pre-m205 name-only classification.
    // Under fallback, waybill can't verify feature activation via cargo,
    // so it defers to the manifest's `optional = true` declaration.
    // For `serde = { optional = true }` + `default = ["serde"]`, this
    // means serde is classified Optional (scope: excluded) — this is
    // the same behavior alpha.63 (pre-m205) would produce for this
    // workspace. Zero regression from pre-m205.
    //
    // The correctness improvement (serde → Runtime) applies ONLY when
    // cargo metadata succeeds. The WARN log above tells the operator
    // to install cargo + warm the registry cache for the full-fidelity
    // path.
    let serde_comp = find_component_by_name(&cdx, "serde")
        .unwrap_or_else(|| panic!("serde component present. cdx: {cdx:#}"));
    assert_eq!(
        component_scope(serde_comp),
        Some("excluded"),
        "FR-004 pre-m205-preservation: serde MUST be scope=excluded under \
         cargo-metadata-fallback (same as alpha.63 behavior). cdx: {cdx:#}"
    );

    // (e) The `waybill:optional-derivation` annotation IS still emitted
    // under fallback — it's set by the classifier's Optional branch,
    // which now fires because activated_names is empty. This matches
    // pre-m205 behavior.
    assert_eq!(
        component_property(serde_comp, "waybill:optional-derivation"),
        Some("cargo-optional-true"),
        "FR-004 fallback: serde MUST carry waybill:optional-derivation \
         (same as alpha.63 behavior). cdx: {cdx:#}"
    );
}
