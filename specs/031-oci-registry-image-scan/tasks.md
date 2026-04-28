---
description: "Task list — milestone 031 OCI registry image scan (anonymous-pull MVP, feature-gated)"
---

# Tasks: OCI Registry Image Scan — Tighter Spec

**Input**: Design documents from `/specs/031-oci-registry-image-scan/`
**Prerequisites**: spec.md (✅), plan.md (✅), checklists/requirements.md (✅)

**Tests**: 6+ inline tests in `oci_pull.rs::tests` covering ref
detection, platform mapping, tarball assembly, error paths +
1 network-gated end-to-end smoke test.

**Organization**: Single user story (US1, P1). Three atomic commits.

## Path Conventions

- Touches `mikebom-cli/Cargo.toml` (new feature + new dep entry).
- Touches `mikebom-cli/src/scan_fs/oci_pull.rs` (new module),
  `mikebom-cli/src/scan_fs/mod.rs` (gated module decl),
  `mikebom-cli/src/cli/scan_cmd.rs` (dispatch).
- Adds `mikebom-cli/tests/oci_registry_smoke.rs`.
- Updates `docs/user-guide/cli-reference.md` (or wherever the
  --image flag is documented).
- Does NOT touch `mikebom-common/`, `mikebom-cli/src/parity/`,
  any `mikebom-cli/src/scan_fs/binary/*.rs`, any
  `mikebom-cli/src/scan_fs/package_db/*.rs`,
  `mikebom-cli/src/generate/`, `mikebom-cli/src/resolve/`,
  or `mikebom-cli/src/cli/` outside `scan_cmd.rs`.

---

## Phase 1: Setup + baseline

- [X] T001 Recon done. Confirmed:
      - `reqwest 0.12 + rustls-tls` in `Cargo.toml:15` (pure-Rust TLS).
      - `tokio = "1" full` in `Cargo.toml:20`.
      - `flate2 = "1"` and `tar = "0.4"` already in workspace.
      - `scan_fs::docker_image::extract(path) → ExtractedImage`
        is the integration seam; takes a tarball file path.
      - `--image` CLI dispatch lives in `scan_cmd.rs:429`.
      - `oci-client` chosen as the OCI distribution client crate
        (deislabs-maintained successor of `oci-distribution`,
        pure-Rust, async, has built-in `Reference` parsing +
        digest verification).
- [ ] T002 Snapshot baseline: `./scripts/pre-pr.sh 2>&1 | tee /tmp/baseline-031.txt | grep -cE '^test [a-z_:]+ \.\.\. ok' > /tmp/baseline-031-count.txt`. Confirms default-profile test count for regression checks.

---

## Phase 2: Commit 1 — `031/feature-gate-and-deps`

**Goal**: Cargo feature `oci-registry` (default off) gating the
`oci-client` dep, with a stub module that builds clean both ways.

- [ ] T003 [US1] Edit `mikebom-cli/Cargo.toml`:
      - Add `[features]` section entry: `oci-registry = ["dep:oci-client"]`.
      - Add `[dependencies]` entry: `oci-client = { version = "...",
        default-features = false, features = ["rustls-tls"], optional = true }`
        (pin the version after consulting the latest stable
        published release).
      - Verify the dep is correctly conditional (only pulled in
        when feature enabled).
- [ ] T004 [US1] Add a stub `mikebom-cli/src/scan_fs/oci_pull.rs`:
      - Module-level `#![cfg(feature = "oci-registry")]` gate.
      - Module doc explaining feature-flag rationale + integration
        seam.
      - Public fn signature `pub fn pull_to_tarball(image_ref: &str,
        target_arch: &str) -> anyhow::Result<tempfile::TempDir>`
        with `unimplemented!("filled in commit 2")` body.
- [ ] T005 [US1] Edit `mikebom-cli/src/scan_fs/mod.rs`: add
      `#[cfg(feature = "oci-registry")] pub mod oci_pull;`.
- [ ] T006 [US1] Run `cargo tree -p mikebom --features oci-registry`
      and inspect output. Verify NO `*-sys` / `*-c` deps surface
      (Principle I check). Document the dep delta in the commit
      message — exact transitive dep list and their licenses.
- [ ] T007 [US1] Verify both build profiles:
      - `./scripts/pre-pr.sh` (default) → clean, test count
        unchanged from T002 baseline.
      - `cargo +stable build -p mikebom --features oci-registry`
        → clean.
      - `cargo +stable clippy --workspace --all-targets --features oci-registry`
        → clean.
- [ ] T008 [US1] Commit: `feat(031/feature-gate-and-deps): add oci-registry Cargo feature + oci-client gated dep`.

---

## Phase 3: Commit 2 — `031/cli-dispatch-and-pull`

**Goal**: end-to-end pull from anonymous public registry working;
CLI dispatch routes `--image <ref>` to the new path; ≥6 inline
tests covering ref detection / platform mapping / tarball
assembly / error paths.

- [ ] T009 [US1] Open `cargo doc -p oci-client --open` to confirm
      the public API shape. Note specifically:
      - `oci_client::Reference::try_from(&str)` (parse).
      - `oci_client::Client::new(...)` constructor.
      - `Client::pull_manifest_and_config` /
        `Client::pull_image_manifest` shape and async signature.
      - `Client::pull_blob` / `pull_layer` shape.
      - `RegistryAuth::Anonymous` for the anon path.
      - Manifest-list resolution helpers.
      - Adjust the FR-002 fn body design to match the actual API.
- [ ] T010 [US1] Implement `oci_pull::pull_to_tarball`:
      ```rust
      pub fn pull_to_tarball(
          image_ref: &str,
          target_arch: &str,
      ) -> anyhow::Result<tempfile::TempDir> {
          let runtime = tokio::runtime::Builder::new_current_thread()
              .enable_all()
              .build()?;
          runtime.block_on(async {
              let reference = oci_client::Reference::try_from(image_ref)?;
              let client = oci_client::Client::new(default_config());
              let auth = oci_client::secrets::RegistryAuth::Anonymous;
              // pull manifest, resolve manifest list to platform
              //   matching target_arch, fail if none
              // pull config + layer blobs, gunzipping each layer
              // assemble docker-save-format tarball:
              //   - manifest.json (JSON array with one entry,
              //     fields: Config, RepoTags, Layers)
              //   - <digest>.json (config blob)
              //   - <digest>/layer.tar (one per layer, decompressed)
              // write to tempfile::tempdir()/image.tar
              // return TempDir
              ...
          })
      }
      ```
      Each step: `?`-propagates with `anyhow::Context::context(...)`
      naming the failure point.
- [ ] T011 [US1] Implement helper fns:
      - `fn detect_image_arg_kind(arg: &Path) -> ImageArgKind`
        returning `Path | OciRef | Invalid`.
      - `fn host_oci_arch() -> anyhow::Result<&'static str>`
        mapping `std::env::consts::ARCH` to OCI platform name
        (`x86_64`→`amd64`, `aarch64`→`arm64`, `arm`→`arm`,
        `riscv64`→`riscv64`, others → error listing supported).
      - `fn manifest_for_platform(manifest_list, target_arch) ->
        anyhow::Result<Manifest>` selecting the platform-specific
        manifest from a manifest list. Single manifest → returned
        directly. Image with manifest list but no matching arch →
        error listing the available platforms.
- [ ] T012 [US1] Edit `mikebom-cli/src/cli/scan_cmd.rs::scan`'s
      `--image` dispatch (around line 429):
      ```rust
      if let Some(image_arg) = args.image.as_ref() {
          if image_arg.is_file() {
              // existing tarball-extract path — unchanged
          } else {
              #[cfg(feature = "oci-registry")]
              {
                  let target_arch = scan_fs::oci_pull::host_oci_arch()?;
                  let pulled_dir =
                      scan_fs::oci_pull::pull_to_tarball(
                          image_arg.to_str().context("...")?,
                          target_arch,
                      )?;
                  let tarball_path = pulled_dir.path().join("image.tar");
                  let extracted = scan_fs::docker_image::extract(&tarball_path)?;
                  // pulled_dir's TempDir keeps tarball alive through scan
                  ...
              }
              #[cfg(not(feature = "oci-registry"))]
              {
                  anyhow::bail!(
                      "OCI registry image references require --features oci-registry. \
                       Either install with `cargo install mikebom --features oci-registry` \
                       or pre-extract the image with \
                       `docker save {} -o image.tar` and pass `--image image.tar`.",
                      image_arg.display()
                  );
              }
          }
      }
      ```
- [ ] T013 [US1] Add 6 inline tests in `oci_pull.rs::tests`:
      - `host_oci_arch_maps_known_arches` — table-test for x86_64
        / aarch64 / arm / riscv64.
      - `host_oci_arch_returns_error_for_unknown_arch` — synthetic
        test forcing an unknown ARCH constant.
      - `tarball_assembly_produces_extract_compatible_layout` —
        hand-build fake manifest + 2 fake gzipped layer-tar
        blobs → `assemble_docker_save_tarball` → assert
        `docker_image::extract` round-trips and the rootfs has
        the expected files.
      - `manifest_for_platform_picks_amd64_from_manifest_list` —
        synthetic manifest list with linux/amd64 + linux/arm64
        entries, target_arch="amd64" → returns the amd64
        manifest digest.
      - `manifest_for_platform_errors_when_target_unavailable` —
        manifest list with only linux/arm64, target="amd64" →
        error message lists the available platforms.
      - `bridge_async_runtime_block_on_works` — smoke test for
        the runtime construction.
      The actual network pull is NOT covered here (would require
      a registry mock; covered by the smoke test in Commit 3).
- [ ] T014 [US1] Verify both build profiles:
      - `./scripts/pre-pr.sh` (default) → clean.
      - `cargo +stable test -p mikebom --features oci-registry oci_pull` → all 6 new tests pass.
      - `cargo +stable clippy --workspace --all-targets --features oci-registry` → clean.
- [ ] T015 [US1] Commit: `feat(031/cli-dispatch-and-pull): wire OCI registry image pull behind --image <ref>`.

---

## Phase 4: Commit 3 — `031/smoke-test-and-docs`

**Goal**: opt-in network-gated end-to-end smoke test + user-facing
docs for `--features oci-registry`.

- [ ] T016 [US1] Create
      `mikebom-cli/tests/oci_registry_smoke.rs`:
      ```rust
      #![cfg(feature = "oci-registry")]
      // Network-gated: only runs when MIKEBOM_OCI_NETWORK_TESTS=1.
      // Default + ebpf + macOS CI lanes skip these. A future
      // milestone can add a dedicated `lint-and-test-oci-network`
      // job that flips the env var on.

      #[test]
      fn pulls_distroless_static_and_emits_sbom() {
          if std::env::var("MIKEBOM_OCI_NETWORK_TESTS").ok().as_deref() != Some("1") {
              return; // skipped on default lanes
          }
          let output = std::process::Command::new(env!("CARGO_BIN_EXE_mikebom"))
              .arg("sbom").arg("scan")
              .arg("--image").arg("gcr.io/distroless/static-debian12:latest")
              .arg("--format").arg("cyclonedx-json")
              .arg("--output").arg("-") // stdout
              .output()
              .expect("mikebom should run");
          assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
          let sbom: serde_json::Value = serde_json::from_slice(&output.stdout)
              .expect("stdout must be valid CDX JSON");
          assert!(sbom["components"].as_array().is_some_and(|c| !c.is_empty()),
              "distroless image should yield ≥1 component");
      }
      ```
- [ ] T017 [US1] Update user-facing docs (whichever file is the
      live `--image` reference):
      - Document the new `--image <ref>` shape.
      - Document `--features oci-registry` requirement.
      - Note current scope (anonymous public pulls only) and
        link to spec for deferred items (auth → 031.x; multi-arch
        flag → 031.y).
- [ ] T018 [US1] Verify:
      - `./scripts/pre-pr.sh` (default profile) clean.
      - `cargo +stable build -p mikebom --features oci-registry` clean.
      - Manual: `cargo run -p mikebom --features oci-registry --
        sbom scan --image gcr.io/distroless/static-debian12:latest`
        succeeds end-to-end on a network-connected dev machine.
- [ ] T019 [US1] Commit: `feat(031/smoke-test-and-docs): add network-gated end-to-end smoke test + cli-reference doc updates`.

---

## Phase 5: Verification

- [ ] T020 SC-001 verification: pre-pr clean default; pre-pr
      clean feature-on; clippy clean both ways.
- [ ] T021 SC-002 verification: `cargo tree -p mikebom` (default)
      identical to pre-milestone tree (no new top-level deps).
      `cargo tree -p mikebom --features oci-registry` shows the
      new deps cleanly contained.
- [ ] T022 SC-003 verification: `git diff main..HEAD --
      mikebom-cli/src/scan_fs/binary/ mikebom-cli/src/scan_fs/package_db/`
      empty.
- [ ] T023 SC-004 verification: `git diff main..HEAD --
      mikebom-common/ mikebom-cli/src/parity/` empty.
- [ ] T024 SC-005 verification: `cargo tree -p mikebom --features
      oci-registry --no-default-features 2>&1 | grep -E '\\-(sys|c)$'`
      empty. Principle I clean.
- [ ] T025 SC-007 verification: 27-golden regen
      (`MIKEBOM_UPDATE_*_GOLDENS=1`) zero diff.
- [ ] T026 SC-008 verification: manual end-to-end smoke test
      against `gcr.io/distroless/static-debian12:latest`.
- [ ] T027 Push branch; observe all 3 standard CI lanes
      (default-profile only) green (SC-006).
- [ ] T028 Author the PR description: 3-commit summary,
      sub-scope rationale (anon-only / host-arch only),
      pointers to 031.x / 031.y / 031.z deferred follow-ons,
      dep audit attestation.

---

## Dependency graph

```text
T001-T002 (recon + baseline, recon done)
   │
   ↓
T003-T008 [Commit 1: feature gate + dep audit + stub]
   │
   ↓
T009-T015 [Commit 2: oci_pull module + CLI dispatch + 6 inline tests]
   │
   ↓
T016-T019 [Commit 3: network-gated smoke test + docs]
   │
   ↓
T020-T028 (verification + PR)
```

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Phase 1 (baseline) | 5 min | T001 done; just snapshot |
| Phase 2 (feature gate + dep audit) | ½ day | dep audit is the careful step |
| Phase 3 (oci_pull + CLI dispatch + tests) | 1.5 days | tarball-assembly logic is the bulk |
| Phase 4 (smoke test + docs) | ½ day | network-gated test infrastructure |
| Phase 5 (verify + PR) | ½ day | dual-profile verification |
| **Total** | **~3 days** | sub-scoped milestone — auth + multi-arch deferred. |
