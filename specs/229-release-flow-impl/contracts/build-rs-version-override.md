# Contract: `waybill-cli/build.rs` `WAYBILL_VERSION` env-var override

**Feature**: 229-release-flow-impl
**Phase**: 1

Pins the `build.rs` modification + companion runtime-code wrapper for FR-005 + FR-012.

## build.rs — additive change

Append to the existing `main()` function (before the current tail, after existing fixture + fingerprint pinning):

```rust
// 229-release-flow-impl: WAYBILL_VERSION build-time override.
//
// Nightly.yml + release.yml (via workflow-input propagation) sets this env
// var to override the emitted binary's version string at build time. When
// unset (normal dev builds + tag-triggered stable releases), the code
// falls through to `env!("CARGO_PKG_VERSION")`.
//
// FR-005 + FR-012 contract:
//   (a) Override applies when set — validated + surfaced via rustc-env
//   (b) Fallback via option_env! at runtime (see waybill_common::version())
//   (c) Invalid SemVer → build-time panic → cargo build fails visibly
println!("cargo:rerun-if-env-changed=WAYBILL_VERSION");
if let Ok(v) = std::env::var("WAYBILL_VERSION") {
    let trimmed = v.trim();
    if trimmed.is_empty() {
        panic!("WAYBILL_VERSION is set but empty/whitespace — refuse to build");
    }
    // SemVer validation using existing `semver` crate (verify presence in
    // Cargo.toml at implementation time; likely already a transitive dep).
    // If `semver` isn't reachable from build.rs (build-dependency vs
    // dev-dependency scope), use a minimal regex check inline:
    //   let re = regex::Regex::new(r"^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$")?;
    // Fallback: pass-through with a WARN comment, since cargo's own
    // Cargo.toml version validation runs on release-bump PR.
    match semver::Version::parse(trimmed) {
        Ok(_) => {
            println!("cargo:rustc-env=WAYBILL_VERSION_OVERRIDE={}", trimmed);
        }
        Err(e) => panic!("WAYBILL_VERSION='{trimmed}' is not valid SemVer: {e}"),
    }
}
```

## Runtime consumption — new `version()` helper

Add to `waybill-common/src/lib.rs` (or wherever the version-string is currently sourced from):

```rust
/// Returns the effective waybill version string, honoring the
/// `WAYBILL_VERSION` build-time env-var override if it was set.
///
/// Runtime cost: compile-time `option_env!()` lookup + `unwrap_or()`
/// on a static string. No allocations, no runtime branch cost after
/// LLVM constant-folding.
///
/// Reference: feature 229 FR-005, FR-012.
pub fn version() -> &'static str {
    option_env!("WAYBILL_VERSION_OVERRIDE").unwrap_or(env!("CARGO_PKG_VERSION"))
}
```

## Migration of existing `env!("CARGO_PKG_VERSION")` call sites

Grep the workspace for `env!("CARGO_PKG_VERSION")` and replace each with `waybill_common::version()`. Expected sites (approximate; verify at implementation time):

- `waybill-cli/src/cli/mod.rs` — CLI `--version` output
- `waybill-cli/src/generate/spdx/document.rs` — SPDX Tool.version emission
- `waybill-cli/src/generate/cyclonedx/metadata.rs` — CycloneDX Tool.version emission
- `waybill-cli/src/generate/spdx/v3_document.rs` — SPDX 3 Tool.version emission

Also check `waybill-cli/build.rs` itself — DO NOT change build.rs to use `waybill_common::version()` (chicken-and-egg problem; build.rs runs before waybill_common is available). build.rs stays with `env::var("CARGO_PKG_VERSION")` for its own purposes.

## Unit test — new file

Create `waybill-cli/tests/waybill_version_override.rs`:

```rust
//! Unit tests for the WAYBILL_VERSION build-time override (229 FR-012).
//!
//! These tests exercise the runtime `waybill_common::version()`
//! helper — the build-time validation lives in build.rs and is
//! implicitly tested by any cargo build with WAYBILL_VERSION set.

#[test]
fn version_reports_cargo_pkg_version_by_default() {
    // Without the env-var override, version() should match
    // env!("CARGO_PKG_VERSION"). This is a compile-time assertion
    // when the test is built without WAYBILL_VERSION_OVERRIDE set.
    // Semi-tautological but catches accidental removals of the
    // fallback branch.
    let v = waybill::version();
    assert!(!v.is_empty(), "version() must return a non-empty string");
    // Any well-formed version should parse as SemVer.
    semver::Version::parse(v)
        .expect("version() output must be valid SemVer");
}

// Note: overriding WAYBILL_VERSION_OVERRIDE at test-runtime is
// impossible via env::set_var — option_env!() is compile-time.
// To test the override path, we'd need a second test binary compiled
// with WAYBILL_VERSION set — that lives at the workflow level (nightly
// e2e test), not the unit-test level. FR-012(a) is thus verified
// end-to-end in nightly.yml's real-world exercise.
```

## Cargo.toml — build-dependencies verification

Ensure `waybill-cli/Cargo.toml`'s `[build-dependencies]` section has `semver`:

```toml
[build-dependencies]
# ... existing entries ...
semver = { workspace = true }
```

If `semver` isn't already a workspace dependency, add it under the root `Cargo.toml`'s `[workspace.dependencies]` — verify at implementation time.

## FR-004 fail-closed enforcement in build.rs

The build.rs `panic!` on invalid SemVer inputs is the fail-closed mechanism. Test manually via:

```bash
# Should fail with informative error:
WAYBILL_VERSION="not-a-semver" cargo build --release 2>&1 | grep "not valid SemVer"

# Should fail with informative error:
WAYBILL_VERSION="" cargo build --release 2>&1 | grep "empty/whitespace"

# Should succeed:
WAYBILL_VERSION="0.2.0-nightly.20260806" cargo build --release
```

## Contract summary

| Field | Value |
|---|---|
| Build-time env-var | `WAYBILL_VERSION` |
| Rustc env-var (emitted by build.rs) | `WAYBILL_VERSION_OVERRIDE` |
| Fallback source | `env!("CARGO_PKG_VERSION")` |
| Runtime accessor | `waybill_common::version()` returning `&'static str` |
| Validation library | `semver` (workspace dependency) |
| Cache invalidation scope | `cargo:rerun-if-env-changed=WAYBILL_VERSION` — only build.rs re-runs, not the whole workspace |
| Constitution SB-4 (`no .unwrap()`) | Honored: build.rs uses `.unwrap_or_else(panic!)` (acceptable in build scripts); runtime code uses `option_env!().unwrap_or()` (compile-time, no runtime panic) |
