---
description: "Add direct OCI-registry image scanning behind a default-off `oci-registry` feature flag — `mikebom sbom scan --image alpine:3.19` pulls the image manifest + layers, writes a docker-save-equivalent tarball, and routes through the existing extraction path. Anonymous public registries only in this milestone; auth + multi-arch deferred to 031.x follow-ons."
status: spec
milestone: 031
---

# Spec: OCI registry image scan (anonymous pull, feature-gated)

## Background

Today `mikebom sbom scan --image <foo.tar>` accepts only a
`docker save`-format tarball. The user has to extract the image
themselves (`docker save image:tag -o image.tar`), then point
mikebom at it. Both `syft` and `trivy` accept image references
directly (`syft alpine:3.19`) — the registry pull is built in.
Closing this UX gap is the goal.

The integration seam is small: the existing pipeline already does
the hard work. `mikebom-cli/src/scan_fs/docker_image.rs::extract`
takes a tarball path, walks the OCI image-spec tarball layout
(manifest.json + layer-tar files), reverses the layer overlay, and
hands a rootfs to the standard scan_fs walker. The missing step is
turning an image reference (`alpine:3.19`) into a tarball at a
tempdir path. That's it.

The dep landscape is favorable: `reqwest 0.12` with `rustls-tls` is
already in `Cargo.toml:15` (no native-tls / no C / Principle I
clean), `tokio = "1" full` is already in `Cargo.toml:20`. The
incremental dep cost is just an OCI-distribution-spec client crate
(likely `oci-client`, the deislabs-maintained successor to
`oci-distribution` — pure-Rust, async, supports public + auth'd
registries) plus its transitive deps. We gate this behind a
default-off `oci-registry` Cargo feature so the workspace stays
lean by default — same pattern as milestone 020's
`ebpf-tracing` feature gate.

This milestone is **deliberately sub-scoped** to the anonymous-
public-registry happy path so it ships in ~3 days. Auth (Docker
keychain + cred helpers) and multi-arch resolution are explicitly
deferred to 031.x follow-ons documented in Out of Scope below.

## User story (US1, P1)

**As an SBOM consumer who has been trained on the syft/trivy UX**
of `<tool> <image-ref>`, I want
`mikebom sbom scan --image alpine:3.19` to work without manually
running `docker save` first, so that mikebom feels like a
first-class peer of the established SBOM tools for the most common
container-scanning use case (scanning a public image by reference).

**Why P1 (not P2):** UX gap. Today every "let me try mikebom on a
public image" attempt requires the user to context-switch into
`docker save`, which (a) requires a Docker daemon, defeating
mikebom's daemon-free posture; (b) is slow; (c) is a friction
point that has been called out in real-world adoption. Closing
this gap unlocks parity with syft/trivy for the demo path.

### Independent test

After implementation:
- `cargo +stable build -p mikebom --features oci-registry` builds
  clean. Default `cargo +stable build -p mikebom` is unchanged
  (no new transitive deps in the dep tree).
- `cargo +stable test -p mikebom --features oci-registry oci_pull`
  exercises new inline parser + ref-detection tests.
- `cargo +stable test -p mikebom oci_registry` (default profile)
  passes the "feature off → friendly error" test.
- A network-gated smoke test:
  `mikebom sbom scan --image gcr.io/distroless/static-debian12:latest`
  succeeds end-to-end and emits a CDX with the expected components.
  Runs only on a dedicated network-allowed CI job (off by default
  per the existing CI lane discipline).

## Acceptance scenarios

**Scenario 1: Anonymous public-registry pull**
```
Given: a built mikebom with `--features oci-registry` enabled,
       network access to docker.io / gcr.io, and an image
       reference `alpine:3.19`
When:  `mikebom sbom scan --image alpine:3.19` runs
Then:  mikebom pulls the manifest + each layer blob, writes a
       docker-save-format tarball to a temp directory, calls
       `docker_image::extract` on it, and proceeds with the
       existing rootfs scan + SBOM emission. Output is byte-
       identical to running the same scan against a manually-
       produced `docker save alpine:3.19 -o /tmp/alpine.tar`
       tarball, modulo the source-path field.
```

**Scenario 2: Path vs ref auto-detection**
```
Given: `--image foo.tar` where `foo.tar` exists on disk
When:  `mikebom sbom scan --image foo.tar` runs
Then:  mikebom routes to the existing tarball-extraction path
       unchanged. NO registry pull is attempted. Existing
       milestone-pre-031 behavior is preserved exactly.

Given: `--image alpine:3.19` (no file at that path)
When:  `mikebom sbom scan --image alpine:3.19` runs with
       --features oci-registry enabled
Then:  mikebom routes to the new registry-pull path. The
       reference is parsed via `oci-client::Reference`. Failure
       to parse → clear error.
```

**Scenario 3: Feature-off friendly error**
```
Given: a mikebom binary built WITHOUT --features oci-registry
       (the default profile)
When:  `mikebom sbom scan --image alpine:3.19` runs (image
       arg looks like a ref, no file at that path)
Then:  mikebom exits non-zero with a clear error message:
       "OCI registry image references require --features oci-registry.
        Either install with `cargo install mikebom --features oci-registry`
        or pre-extract the image with `docker save alpine:3.19 -o
        image.tar` and pass `--image image.tar`."
       NO panic. NO confusing path-not-found error.
```

**Scenario 4: Registry pull failure**
```
Given: --features oci-registry on, an image ref that doesn't
       resolve (typo, non-existent registry, network down)
When:  the pull fails
Then:  clear error message naming the failure mode (manifest
       fetch, layer fetch, network timeout, etc.), exit
       non-zero, no panic, partial tempdir cleaned up.
```

**Scenario 5: Multi-platform manifest list (deferred to 031.x but
behavior under 031)**
```
Given: an image ref like `alpine:3.19` whose registry returns
       a manifest LIST (image index) with linux/amd64,
       linux/arm64, linux/arm/v7 variants
When:  mikebom pulls under milestone 031 (no platform flag,
       host arch only)
Then:  mikebom selects the manifest matching the host's arch
       (`std::env::consts::ARCH` mapped to OCI's `amd64` /
       `arm64` / etc.). If no matching platform exists, error
       with the available platforms listed for the user.
```

## Edge cases

- **Image ref without a tag** (e.g. `alpine`): default to `latest`
  per the OCI distribution-spec convention. `oci-client::Reference`
  handles this.

- **Image ref with a digest** (e.g. `alpine@sha256:...`): pull
  the exact digest. No tag-resolution needed.

- **Registry hostnames**: `alpine:3.19` defaults to `docker.io`
  (which itself redirects to `registry-1.docker.io/library/alpine`
  per Docker Hub's `/library/` convention). `gcr.io/foo/bar:1`
  and `ghcr.io/foo/bar:1` use their own registry. `oci-client::Reference`
  parsing handles these.

- **Compressed layers**: most registries use gzip-compressed layer
  blobs (`application/vnd.oci.image.layer.v1.tar+gzip`). The
  existing `docker_image::extract` path expects plain `tar` inside
  the tarball, so the registry-pull layer needs to **decompress
  gzipped layers before writing** them into the docker-save-
  format tarball. zstd-compressed layers (newer OCI media type)
  out of scope for milestone 031 — return a clear error if
  encountered.

- **Layer digest verification**: the OCI distribution spec carries
  a `digest` field per layer descriptor. mikebom MUST verify the
  downloaded blob's SHA-256 matches before trusting it. `oci-client`
  does this internally; we just need to ensure the verification
  path is taken (not bypassed via raw HTTP).

- **Anonymous-only scope**: this milestone covers PUBLIC anonymous
  pulls only. Authenticated registries (private Docker Hub repos,
  GHCR private packages, ECR, GCR with private images) are out of
  scope — return a clear error pointing users at the deferred
  031.x auth milestone.

- **Multi-arch manifest lists**: this milestone uses host-arch
  selection only (`std::env::consts::ARCH` → OCI arch name).
  `--image-platform linux/arm64` flag is deferred to 031.y.

- **Async runtime**: `oci-client` is async/tokio. mikebom's CLI
  scan path is currently synchronous. Use `tokio::runtime::Runtime::new()`
  + `block_on(...)` inside the new `oci_pull::pull_to_tarball`
  helper to bridge async-to-sync at the module boundary. Keeps
  the rest of the CLI path unchanged.

## Functional requirements

- **FR-001**: `mikebom-cli/Cargo.toml` gains a new feature
  `oci-registry` (default off) gating one or more new dep entries:
  - `oci-client = "0.x"` (deislabs-maintained successor of
    `oci-distribution`; pure-Rust async OCI distribution client)
  - any of its transitive deps that aren't already in the workspace.
  Audit `cargo tree -p mikebom --features oci-registry` to
  verify no C-bound transitive deps surface (Principle I).

- **FR-002**: New module `mikebom-cli/src/scan_fs/oci_pull.rs`
  guarded by `#[cfg(feature = "oci-registry")]`. Exposes one
  public entry point:
  ```rust
  pub fn pull_to_tarball(
      image_ref: &str,
      target_arch: &str,
  ) -> anyhow::Result<tempfile::TempDir>;
  ```
  Returns a TempDir containing a `docker save`-format tarball
  named `image.tar` that can be passed to
  `docker_image::extract`. The TempDir handle ensures cleanup
  on drop.

- **FR-003**: `oci_pull::pull_to_tarball` walks: parse ref →
  fetch manifest → resolve to platform-specific manifest if it's
  a manifest list → fetch the config blob + each layer blob →
  decompress gzipped layer blobs → assemble a docker-save-format
  tarball (manifest.json + per-layer tar files) → return TempDir.
  Async work bridged via `tokio::runtime::Runtime::new()` +
  `block_on(...)` at the module boundary.

- **FR-004**: `mikebom-cli/src/cli/scan_cmd.rs` gains ref-vs-path
  detection on the `--image` argument. The detection logic:
  ```text
  if file at path exists  → existing tarball extraction path
  else if feature off    → friendly error per Scenario 3
  else if parses as OCI ref → call oci_pull::pull_to_tarball
  else                   → error "neither a file nor a parseable image reference"
  ```

- **FR-005**: When the new registry-pull path runs, it produces a
  TempDir whose `image.tar` is then handed to
  `scan_fs::docker_image::extract` exactly the same as the
  existing path. The TempDir lifetime extends through the scan
  to keep the tarball alive.

- **FR-006**: Registry pull failures (manifest 404, layer
  fetch error, gzip decompress error, layer-digest mismatch,
  unparseable ref, etc.) propagate as `anyhow::Error` with
  context naming the failure point. No panics. The TempDir is
  cleaned up on drop regardless.

- **FR-007**: Authenticated registries → return a clear error:
  `"image registry returned 401/403; authenticated pulls are
  not yet supported in milestone 031 (tracked as 031.x — see
  spec)"`. NO partial-credential leakage in error messages.

- **FR-008**: Multi-arch manifest lists → select the manifest
  matching `std::env::consts::ARCH` mapped to the OCI platform
  name (`x86_64` → `amd64`, `aarch64` → `arm64`, etc.). If no
  matching platform exists, error listing the available platforms.

- **FR-009**: zstd-compressed layers → return a clear error:
  `"zstd-compressed image layers are not yet supported in
  milestone 031 (gzip layers fully supported); image's layer
  N has media type X"`.

- **FR-010**: Inline tests in `oci_pull.rs::tests`:
  - Ref-detection: paths, valid refs, invalid refs.
  - Platform-name mapping: `x86_64` → `amd64`, `aarch64` → `arm64`,
    unknown host arch → error.
  - Tarball-assembly: hand-build a fake manifest + hand-build
    fake layer-tar bytes, run the assembly logic, assert the
    resulting tarball is parseable by `docker_image::extract`.
  - The actual network pull is NOT covered by inline tests
    (would require a registry mock — that's FR-011).

- **FR-011**: A network-gated smoke test in
  `mikebom-cli/tests/oci_registry_smoke.rs` (gated on
  `#[cfg(feature = "oci-registry")]` AND a `MIKEBOM_OCI_NETWORK_TESTS=1`
  env var) pulls a known-stable public image
  (`gcr.io/distroless/static-debian12:latest`) and verifies the
  end-to-end pipeline. Skipped on the default Linux + macOS CI
  lanes. May get its own `lint-and-test-oci-network` job in a
  follow-on; for milestone 031 it ships as opt-in only.

- **FR-012**: `docs/reference/sbom-format-mapping.md` is
  unchanged — this milestone doesn't introduce new annotations.

- **FR-013**: Per-commit `./scripts/pre-pr.sh` clean across the
  three commits (Cargo + feature gate / CLI dispatch + oci_pull /
  tests + docs).

## Success criteria

- **SC-001**: `./scripts/pre-pr.sh` clean on default profile.
  `cargo +stable test -p mikebom --features oci-registry` clean.
  `cargo +stable clippy --workspace --all-targets --features oci-registry`
  clean.

- **SC-002**: Default `cargo tree -p mikebom` is unchanged from
  pre-milestone. The `oci-client` and any transitive deps live
  ONLY in the feature-gated graph.

- **SC-003**: `git diff main..HEAD --
  mikebom-cli/src/scan_fs/binary/ mikebom-cli/src/scan_fs/package_db/`
  is empty. The binary scanners + package-db readers are
  untouched. This is purely an input-source change.

- **SC-004**: `git diff main..HEAD --
  mikebom-common/ mikebom-cli/src/parity/`
  is empty. No SBOM-output schema changes; no new annotations.

- **SC-005**: Audit: `cargo tree -p mikebom --features oci-registry
  --no-default-features 2>&1 | grep -E '\\-(sys|c)'` produces no
  output indicating C-bound deps. Principle I clean.

- **SC-006**: All 3 standard CI lanes green on the default
  profile (no oci-registry feature). The feature-on CI path is
  added in 031.x or as a separate PR if needed.

- **SC-007**: 27-golden regen produces zero diff (no fixture
  exercises the registry-pull path; tarball-extraction is
  unchanged).

- **SC-008**: `mikebom sbom scan --image gcr.io/distroless/static-debian12:latest`
  built with `--features oci-registry` succeeds end-to-end
  against a real registry (manual verification — runs of this
  command on developer + CI lanes will be the gate).

## Clarifications

- **Default-off feature gate**: matches milestone 020's
  `ebpf-tracing` precedent. The 80% of users who scan local
  rootfs / source trees / pre-extracted tarballs don't take on
  the OCI client + transitive dep weight. Users who want
  registry pulls opt in via `cargo install mikebom --features
  oci-registry`.

- **Pull THEN scan, not stream-as-you-scan**: the implementation
  pulls the full image to disk first (via tempdir tarball) and
  THEN scans. Streaming-as-you-scan would be a meaningful
  refactor of the docker_image::extract path (which today is
  built around `tar::Archive::new` over a fully-present tarball).
  Not worth doing in milestone 031.

- **Anonymous-only**: deliberate. Auth handling (Docker keychain
  parsing, cred helpers, bearer tokens, ECR's
  `aws ecr get-login-password`-style credentials) is multiple
  days of careful engineering. Defer to 031.x as a focused
  milestone after we've shipped the happy path.

- **Host-arch only**: `--image-platform linux/arm64` flag deferred
  to 031.y. Same milestone-cadence discipline.

- **No layer caching**: each scan re-pulls the layers from the
  registry. Caching would help for repeat scans but is meaningful
  state-management work (cache key = manifest digest + layer
  digests + cache-eviction policy). Defer to 031.z.

- **No image-spec validation**: mikebom doesn't verify that the
  pulled image conforms to the OCI image-spec
  (e.g. config blob has a valid platform, layer digests aren't
  duplicated). `oci-client` does basic digest verification;
  beyond that we trust the registry.

- **Async-to-sync via `tokio::Runtime`**: the CLI scan path stays
  synchronous. The async work is contained inside
  `oci_pull::pull_to_tarball` via a runtime created at function
  entry and dropped at exit. This avoids cascading async
  through the rest of the CLI. Costs an extra runtime
  instantiation per pull (~ms-level overhead, negligible against
  layer download time).

## Out of scope — deferred to 031.x / 031.y / 031.z follow-ons

### 031.x — Authenticated pulls (highest priority follow-on)

Pulling from private registries needs:
- `~/.docker/config.json` parsing (`auths.<registry>.auth`
  base64 user:pass; `auths.<registry>.identitytoken` bearer;
  `credsStore: <helper>` → invoke
  `docker-credential-<helper> get` subprocess).
- macOS keychain helper subprocess
  (`docker-credential-osxkeychain`) — common in dev environments.
- Bearer-token flows (registry returns `401 Bearer realm=...`,
  client fetches a scoped token, retries).
- ECR-specific (`aws ecr get-login-password` integration; or
  AWS SDK).
- GHCR-specific (PAT-as-password convention).

Scope estimate: 3-4 days. Separate milestone.

### 031.y — Multi-platform `--image-platform` flag

Adds `--image-platform <linux/arch>` CLI flag to override the
default host-arch selection. Useful for scanning an arm64 image
on an amd64 host. Scope: ~1 day. Separate milestone.

### 031.z — Layer caching

Disk-cache pulled layers keyed on layer digest. Enables fast
repeat-scans of the same image. Cache-eviction policy needed
(LRU? size cap? TTL?). Scope: ~2 days. Separate milestone.

### Out of scope entirely (no current plan)

- **zstd-compressed layers**: mikebom returns a clear "not yet
  supported" error. The vast majority of registry-published
  images use gzip; zstd is opt-in via the OCI media type
  `application/vnd.oci.image.layer.v1.tar+zstd` and not the
  default. Add when a real-world image surfaces the need.

- **Read-from-Docker-daemon socket**: alternative to
  registry pulls (`docker-credential-helper`-style). Lower
  priority since rootless / k8s / containerd-only environments
  don't have a Docker daemon. Defer indefinitely.

- **OCI image distribution v2 protocol extensions** (e.g.
  cross-repo blob mounting). Not needed for read-only scan use
  case.

- **Streaming layer extraction**: scan layers as they're
  downloaded, never write a full tarball to disk. Significant
  pipeline refactor; scan-time savings only meaningful for
  very large images. Defer.

- **OCI signature verification** (cosign / notary v2). Out of
  scope — mikebom records SBOM data; signing is a separate
  workflow.
