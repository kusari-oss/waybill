---
description: "Implementation plan — milestone 031 OCI registry image scan (anonymous-pull MVP, feature-gated)"
status: plan
milestone: 031
---

# Plan: OCI registry image scan

## Architecture

Pure additive feature behind a default-off Cargo feature gate.

```
                        ┌─ existing path ────────────────────────────┐
                        │                                            │
  --image foo.tar  ────►│  scan_fs::docker_image::extract(path)  ─► rootfs scan
                        │                                            │
  --image alpine:3.19 ►─┘                                            │
        │                                                             │
        │ (feature off)                                                │
        └─► friendly error: "enable --features oci-registry"          │
                                                                       │
        │ (feature on)                                                 │
        ▼                                                              │
  scan_fs::oci_pull::pull_to_tarball(ref, host_arch)                  │
                  │                                                    │
                  ▼                                                    │
  TempDir{ image.tar (docker-save format) }  ──► extract(path) ───────┘
```

The integration seam is `docker_image::extract(path)` which is
unchanged. The new module produces a TempDir whose `image.tar`
gets passed in. Layer-merging, rootfs construction, and SBOM
emission all reuse the existing milestone-pre-031 path verbatim.

## Reuse inventory

These existing items handle the work:

- `reqwest = "0.12"` with `default-features = false, features = ["json", "rustls-tls"]`
  in `Cargo.toml:15` — already in workspace deps. Pure-Rust TLS
  via rustls. **No additional HTTP/TLS deps needed.**
- `tokio = "1" full` in `Cargo.toml:20` — already in workspace.
  Async runtime ready.
- `tempfile = "3"` — already in workspace deps for tarball
  extraction.
- `flate2 = "1"` (mikebom-cli/Cargo.toml:48) — already in
  workspace for tar handling. Reused for gzip-layer
  decompression.
- `tar = "0.4"` — already in workspace. Reused for tarball
  assembly.
- `serde / serde_json` — already in workspace.
- `scan_fs::docker_image::extract(archive_path: &Path) -> ExtractedImage`
  — unchanged integration seam.
- Existing `--image` CLI arg → upgrades from "must be a path" to
  "path OR ref".

## Crate choice: oci-client

Selected: **`oci-client`** (deislabs-maintained, the rebrand of
`oci-distribution`).

Rationale:
- Pure-Rust (Principle I clean — verified pre-spec via crates.io
  metadata; FR-005 enforces audit at PR time).
- Async/tokio-native — matches mikebom's existing async paths.
- Active maintenance.
- Used by oras-rs, krustlet, and a handful of other projects in
  production — battle-tested for the public-pull happy path.
- Built-in `Reference` parsing (registry/repo/tag/digest).
- Built-in digest verification on layer fetch.
- Supports manifest-list resolution (defers to 031.y).
- Supports auth (defers to 031.x).

Rejected alternatives:
- **Roll our own on `reqwest` + `oci-spec`**: reasonable but
  doubles the maintenance surface and re-implements digest
  verification, manifest-list selection, etc. that `oci-client`
  has tested. Not worth it.
- **`bollard` (Docker daemon)**: covered in spec — wrong
  abstraction (daemon-bound, not registry-bound).
- **Shell out to `skopeo`**: option C from the scoping
  conversation; user picked D for the no-external-tool UX.

## Touched files

| File | Change | LOC |
|---|---|---|
| `mikebom-cli/Cargo.toml` | new `oci-registry` feature; new gated `oci-client` dep + transitive | +8 |
| `mikebom-cli/src/scan_fs/oci_pull.rs` | NEW module — `pull_to_tarball` + ref-detection + platform-mapping helpers + 6+ inline tests | +280 |
| `mikebom-cli/src/scan_fs/mod.rs` | gated `pub mod oci_pull;` declaration | +3 |
| `mikebom-cli/src/cli/scan_cmd.rs` | upgrade `--image` dispatch with ref-vs-path detection + feature-off friendly error | +40 |
| `mikebom-cli/tests/oci_registry_smoke.rs` | NEW network-gated end-to-end smoke test | +60 |
| `docs/user-guide/cli-reference.md` (if exists) | add `--image <ref>` documentation + `--features oci-registry` mention | +15 |

Total Rust source: ~390 LOC across 5 files (the smoke test counts
but only runs opt-in).

## Phasing

Three atomic commits in dependency order.

### Commit 1: `031/feature-gate-and-deps`
- Add `oci-registry` feature to `mikebom-cli/Cargo.toml`.
- Add `oci-client` (and any required helpers) as `optional = true`
  deps gated to the feature.
- Run `cargo tree -p mikebom --features oci-registry` and
  document the dep delta in commit message. **Verify Principle I**
  (no C/sys deps surface).
- Add a stub `mikebom-cli/src/scan_fs/oci_pull.rs` (gated module)
  with just the public-fn signature + `unimplemented!()` body so
  the workspace build matrix passes for both feature-on and
  feature-off cases.
- `pre-pr.sh` clean both ways:
  `./scripts/pre-pr.sh` (default) clean.
  `cargo +stable clippy --workspace --all-targets --features oci-registry`
  clean.

### Commit 2: `031/cli-dispatch-and-pull`
- Implement `oci_pull::pull_to_tarball` end-to-end:
  - Reference parsing via `oci_client::Reference`.
  - Platform-mapping helper (`std::env::consts::ARCH` → OCI name).
  - Manifest fetch + manifest-list platform selection.
  - Layer fetch (gzip decompression on the way in).
  - Tarball assembly (`docker save`-format: `manifest.json` +
    per-layer `<sha>.tar` files).
  - TempDir return.
  - Error mapping: each anyhow context names the failure
    point (parse / manifest / layer / decompress / tarball-write).
- CLI dispatch in `scan_cmd.rs`:
  - Detect `--image <arg>`: file-exists → tarball path; else
    if feature on → call oci_pull::pull_to_tarball; else →
    friendly error.
  - 401/403 from registry → "auth not yet supported" error.
  - zstd layer media type → "zstd not yet supported" error.
- 6+ inline tests in `oci_pull.rs::tests`:
  - `ref_detection_distinguishes_path_from_ref`
  - `platform_mapping_x86_64_to_amd64`
  - `platform_mapping_aarch64_to_arm64`
  - `platform_mapping_unknown_arch_returns_error`
  - `tarball_assembly_produces_extract_compatible_layout`
    (hand-built fake layers + manifest → assert
    `docker_image::extract` round-trips).
  - `auth_error_returns_friendly_message`.
- `pre-pr.sh` clean both ways.

### Commit 3: `031/smoke-test-and-docs`
- New `mikebom-cli/tests/oci_registry_smoke.rs` gated on
  `#[cfg(feature = "oci-registry")]` AND
  `MIKEBOM_OCI_NETWORK_TESTS=1` env var. Pulls
  `gcr.io/distroless/static-debian12:latest` (stable, public,
  small) and asserts the pipeline succeeds end-to-end.
- Update `docs/user-guide/cli-reference.md` (or wherever the
  `--image` flag is documented) with the new ref-shape +
  `--features oci-registry` build instructions.
- `pre-pr.sh` clean.

Per FR-013, each commit's `pre-pr.sh` is clean both ways
(default profile + feature-on profile).

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Phase 1 (recon + baseline) | done | T001 done in scoping |
| Phase 2 (Cargo + feature gate + dep audit) | ½ day | dep audit is the careful step |
| Phase 3 (oci_pull module + CLI dispatch + 6 inline tests) | 1.5 days | tarball-assembly logic is the meatiest part |
| Phase 4 (smoke test + docs) | ½ day | network-gated test + docs |
| Phase 5 (verify + PR) | ½ day | both build profiles + smoke test |
| **Total** | **~3 days** | sub-scoped milestone — auth + multi-arch deferred to 031.x / 031.y. |

## Risks

- **R1: oci-client API shape vs. expectations.** Some methods may
  be sync, some async. Need to confirm at recon-deep time what
  the public API looks like. Mitigation: `cargo doc -p oci-client
  --open` early in commit 2, before writing the module body.
  Easy to adjust the wrapper.

- **R2: Transitive deps audit failure.** `oci-client` brings
  in `bytes`, `base64`, possibly others. If any pull in a C-bound
  crate (sys or `*-sys`), Principle I says we can't proceed
  without justifying it. Mitigation: commit 1 is a "deps audit"
  commit. If it fails the audit, scope-pivot to a `reqwest`-only
  custom client (more work, but no new top-level deps).

- **R3: Rustls vs native-tls in the transitive graph.**
  `reqwest` is already in the workspace with `rustls-tls`. If
  `oci-client`'s default features pull `native-tls` somehow,
  we have to pin its features to match. Mitigation: pin
  `oci-client = { version = "...", default-features = false,
  features = ["rustls-tls"] }` (or whatever its rustls feature
  name is) in commit 1.

- **R4: Async-to-sync via block_on inside a binary that may
  itself be in tokio context.** If the user invokes `mikebom`
  from inside another tokio runtime (e.g. embedded usage),
  `Runtime::new()::block_on` would panic with "Cannot start a
  runtime from within a runtime". Mitigation: not a real
  concern for the CLI binary path (CLI's main is sync). If it
  ever becomes a concern, switch to `tokio::task::block_in_place`
  or detect existing runtime via `tokio::runtime::Handle::try_current`.
  Out of scope for milestone 031.

- **R5: Fake-layer fixtures for the tarball-assembly test.**
  The test needs to construct fake `tar.gz` bytes that mimic a
  real OCI layer, then assert that the assembled
  `docker save`-format tarball is parseable by
  `docker_image::extract`. Construction is a few KB of
  hand-crafted bytes. Mitigation: `tar::Builder` + `flate2::write::GzEncoder`
  produce these in 30-50 LOC.

## Constitution alignment

- **Principle I (Pure Rust, Zero C):** verified pre-spec via
  crates.io metadata for `oci-client`. Commit 1 enforces with
  `cargo tree` audit. ✓
- **Principle IV (no `.unwrap()` in production):** new code
  uses `?` throughout, returns `anyhow::Result`. ✓
- **Principle VI (Three-Crate Architecture):** untouched. The
  `oci-client` dep lives only in `mikebom-cli`'s
  feature-gated section. ✓
- **Principle VIII (Completeness):** new code path produces the
  same SBOM that `docker save → mikebom --image foo.tar` would.
  No completeness regression. ✓
- **Principle X (Transparency):** generation context already
  flags `container-image-scan`; that field is unchanged.
  Future enrichment of the generation context with the source
  ref (e.g., `mikebom:scan-source = "alpine:3.19"`) is a
  follow-on. ✓
- **Principle XII (External Data Source Enrichment):** pulling
  the scan target from a registry IS retrieving the artifact-
  of-interest, distinct from enrichment-via-deps.dev. The
  constitution's external-data principle governs enrichment
  data, not the scan target itself. ✓
- **Per-commit verification:** FR-013 enforced.
- **Recon-first:** every claim grounded in
  `mikebom-cli/Cargo.toml:15` (reqwest), `Cargo.toml:20`
  (tokio), `mikebom-cli/src/scan_fs/docker_image.rs:70`
  (extract entry point), `mikebom-cli/src/cli/scan_cmd.rs:429`
  (current --image dispatch).

## What this milestone does NOT do

- Does not handle authenticated registry pulls (deferred to 031.x).
- Does not handle multi-platform selection beyond host-arch
  default (deferred to 031.y).
- Does not cache pulled layers (deferred to 031.z).
- Does not support zstd-compressed layers (returns clear error).
- Does not stream layer extraction (full pull-then-scan).
- Does not validate OCI image-spec compliance beyond
  oci-client's built-in digest checks.
- Does not change CLI args beyond extending `--image`'s
  accepted shape.
- Does not change the SBOM output (no new annotations / catalog
  rows / parity extractors).
