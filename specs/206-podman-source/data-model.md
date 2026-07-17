# Data Model: Scan Local Podman Images

**Date**: 2026-07-17
**Purpose**: Document the new module + types + emitter delta + parity catalog row. Reuses `oci_pull::tarball::assemble_docker_save_tarball` verbatim (no new tarball type).

## E1: `PodmanSourceError` enum (NEW)

**Location**: `mikebom-cli/src/scan_fs/podman_source.rs` (top of new file).

**Shape**:

```rust
#[derive(Debug, thiserror::Error)]
pub enum PodmanSourceError {
    #[error("podman storage root not found at `{path}`; reason: {reason}")]
    StorageRootUnreachable { path: PathBuf, reason: String },

    #[error("podman storage driver `{driver}` not supported (m206 supports `overlay` only)")]
    UnsupportedDriver { driver: String },

    #[error("no podman image matched reference `{image_ref}` in storage index at `{path}`")]
    ImageNotFound { image_ref: String, path: PathBuf },

    #[error("podman image `{id}` OCI manifest at `{path}` is corrupted: {reason}")]
    CorruptedManifest { id: String, path: PathBuf, reason: String },

    #[error("podman image `{id}` layer digest verification failed: expected {expected}, computed {actual}")]
    LayerDigestMismatch { id: String, expected: String, actual: String },

    #[error("podman host architecture `{host}` does not match any variant in multi-arch image `{image_ref}`; available: {available:?}")]
    NoArchMatch { image_ref: String, host: String, available: Vec<String> },

    #[error("podman source I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
```

**Variant semantics**:

| Variant | Trigger | Recovery |
|---|---|---|
| `StorageRootUnreachable` | `<graphroot>` missing/unreadable after config lookup + default fallback | Caller falls back to next `--image-src` entry (FR-007) |
| `UnsupportedDriver` | Detected `vfs` or `btrfs` at `<graphroot>` | Caller falls back to next `--image-src` entry (FR-007); WARN log fires |
| `ImageNotFound` | Image ref doesn't match any tag/digest/short-ID in `images.json` | Caller falls back to next `--image-src` entry (FR-007) |
| `CorruptedManifest` | Per-image OCI manifest parse fails | Non-recoverable for this scan target; error propagated |
| `LayerDigestMismatch` | Layer content SHA-256 doesn't match manifest-declared digest | Non-recoverable; error propagated (integrity guarantee) |
| `NoArchMatch` | Multi-arch image cached but no variant matches host arch | Non-recoverable; user must specify arch or fix host (FR-011) |
| `IoError` | Any other `std::io::Error` from filesystem read | Non-recoverable; error propagated |

**Validation rules**:
- Every variant is safe to `format!()` via `Display` — no panics.
- `path` fields use `PathBuf` (not `String`) — filesystem path type safety.
- Public visibility (`pub enum`) — the CLI dispatch at `scan_cmd.rs::resolve_image_ref` needs to match on variants for the `--image-src` fallback ladder.

## E2: `PodmanImageRef` newtype (NEW)

**Location**: `mikebom-cli/src/scan_fs/podman_source.rs` (adjacent to `PodmanSourceError`).

**Shape**:

```rust
/// Parsed forms of the operator's `--image <ref>` argument as it
/// resolves against podman's image index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodmanImageRef {
    /// `nginx:1.27.0` or `docker.io/library/nginx:1.27.0`.
    Tagged { repo: String, tag: String },
    /// `nginx@sha256:abc…`.
    Digest { repo: String, digest: String },
    /// `abcdef123456` (full 64-hex or 12-hex short prefix).
    ImageId { id_prefix: String },
}

impl PodmanImageRef {
    /// Parse a raw operator-supplied string. Cases handled per FR-003.
    pub fn parse(raw: &str) -> Result<Self, PodmanSourceError> { … }
}
```

**Validation rules**:
- Tag form: matches `<repo>(:<tag>)?` — default tag `latest` if `:tag` absent (per Docker convention).
- Digest form: matches `<repo>@sha256:<64-hex>`.
- ImageId form: 12-64 hex chars (podman accepts short IDs as prefix matches).
- Empty input → `PodmanSourceError::ImageNotFound { image_ref: "" }`.

## E3: `resolve_and_pack` public entry point (NEW)

**Location**: `mikebom-cli/src/scan_fs/podman_source.rs`.

**Signature** (F5-remediated — dropped dead `storage_root` param):

```rust
pub fn resolve_and_pack(
    image_ref: &str,
    out_tarball: &Path,
) -> Result<(), PodmanSourceError>
```

Prior draft included a `storage_root: Option<&Path>` param earmarked as a Phase-2 escape-hatch hook. It was only ever called with `None` in m206 MVP → dead code from day 1. Deferred: re-add the param + a `--podman-storage-root` CLI flag together in a future milestone if operators surface a need.

**Body flow** (5 phases):

1. **Discover storage root** (R2): read `containers.conf` → detect `graphroot` OR fall back to default per rootless/rootful. Return `StorageRootUnreachable` on failure.
2. **Detect storage driver** (R3): check directory presence at `<graphroot>/overlay/` vs `vfs/` vs `btrfs/`. Non-overlay → `UnsupportedDriver`.
3. **Resolve image ref**: parse via `PodmanImageRef::parse`, load `<graphroot>/overlay-images/images.json`, match against the ref's tag/digest/short-ID form. Return `ImageNotFound` on miss.
4. **Load OCI content** via the F1-remediated multi-arch helper `resolve_manifest_for_host_arch(graphroot, image_id)`:
   - First try `serde_json::from_slice::<oci_spec::image::ImageIndex>(&<graphroot>/overlay-images/<image-id>/manifest bytes)`. On success (multi-arch case), filter `manifests[]` by `platform.architecture == host_oci_arch` AND `platform.os == host_os`; 0 matches → `NoArchMatch { image_ref, host, available }`; 1 match → recurse: read `<graphroot>/overlay-images/<matched-digest>/manifest`, parse as `ImageManifest`, return. ≥2 matches → return first + WARN.
   - On ImageIndex parse failure, fall back to `ImageManifest` parse (single-arch case).
   - `host_oci_arch` is computed via an `ARCH_ALIAS` helper mapping Rust `std::env::consts::ARCH` names to OCI-canonical strings ("x86_64" → "amd64", "aarch64" → "arm64", "arm" → "arm"; unknown → passthrough).
   - Also load `<graphroot>/overlay-images/<image-id>/config` (OCI ImageConfiguration).
5. **Re-tar + assemble**: for each layer in the resolved manifest's layer chain, resolve `<graphroot>/overlay/<layer-id>/diff/` (map manifest digest → c/storage internal layer ID via `<graphroot>/overlay-layers/layers.json`), walk with `walkdir` + write into a `tar::Builder<GzEncoder<Vec<u8>>>` with `HeaderMode::Deterministic`, verify the compressed digest matches the manifest declaration (per FR-012 integrity), then hand `[PulledLayer { blob, digest, media_type }]` + `config_bytes` + `image_ref` + `out_tarball` to `oci_pull::tarball::assemble_docker_save_tarball` (reused verbatim).

**Post-condition on success**: `out_tarball` is a docker-save-format `.tar` (or `.tar.gz` per convention — inspect the assembler's output shape) that `docker_image::extract` consumes with zero modifications.

## E4: `ImageSource::Podman` variant (MODIFIED enum)

**Location**: `mikebom-cli/src/cli/scan_cmd.rs:54-62`.

**Pre-m206 state**:

```rust
pub enum ImageSource {
    Docker,
    Remote,
}
```

**Post-m206 state**:

```rust
pub enum ImageSource {
    Docker,
    /// Milestone 206 (#440) — local podman image cache. Filesystem-only
    /// (no daemon/REST API). Requires the target image be pre-pulled
    /// via `podman pull` or `podman build`. Rootless preferred; rootful
    /// supported when mikebom has read access to `/var/lib/containers/
    /// storage/`. Linux-only per spec Assumption 1.
    Podman,
    Remote,
}
```

**Side effects**:
- Default value string at `scan_cmd.rs:234` bumps from `"docker,remote"` to `"docker,podman,remote"` per FR-006.
- Every match on `ImageSource` gets a new arm — clippy will flag missed match sites at compile time.

## E5: Dispatch branch in `resolve_image_ref` (MODIFIED)

**Location**: `mikebom-cli/src/cli/scan_cmd.rs:1908+` (existing dispatch loop over `args.image_src`).

**New branch** (analogous to Docker branch at lines 1910-1948):

```rust
ImageSource::Podman => {
    let tarball_path = temp_tarball_dir.path().join("podman.tar");
    match scan_fs::podman_source::resolve_and_pack(arg_str, &tarball_path, None) {
        Ok(()) => {
            selected_source = Some(ImageSource::Podman);
            break tarball_path;
        }
        Err(e) => {
            tracing::warn!(
                image_ref = %arg_str,
                error = %e,
                "podman source failed; trying next --image-src entry"
            );
            continue;
        }
    }
}
```

**Success side effect**: `selected_source = Some(ImageSource::Podman)` flows into the emitter pipeline via `ScanResult.image_source: Option<ImageSource>` for the C124 annotation (E7 below).

## E6: `ScanResult.image_source` + `ScanArtifacts.image_source` fields (NEW)

**Locations**:
- `mikebom-cli/src/scan_fs/mod.rs::ScanResult` — new field (mirrors m204 helm_extraction_mode pattern verbatim).
- `mikebom-cli/src/generate/mod.rs::ScanArtifacts` — new borrow field.

**Shape**:

```rust
// ScanResult:
/// Milestone 206 (#440): winning `--image-src` entry for the scan.
/// `None` when scanning `--path` (not an image); `Some(ImageSource::
/// <variant>)` when an image source was selected. Drives the C124
/// `mikebom:image-source` document-scope annotation, but only for
/// Podman per FR-005 byte-identity guardrail (docker/remote scans
/// emit no annotation).
pub image_source: Option<crate::cli::scan_cmd::ImageSource>,

// ScanArtifacts (borrow):
pub image_source: Option<&'a crate::cli::scan_cmd::ImageSource>,
```

**Note on visibility**: `ImageSource` is defined in `cli::scan_cmd` (private CLI module). If the emitter reads it via `crate::cli::scan_cmd::ImageSource`, that's fine for `mikebom-cli` internal use. Alternative: promote `ImageSource` to `mikebom-cli::types::ImageSource` (module tidying). For m206 MVP, use the internal path — refactor is a separate concern.

**Plumbing chain**: same 8-hop pattern as m204:
1. Reader (scan_cmd.rs dispatch) sets `selected_source`.
2. Threaded into `ScanResult.image_source`.
3. Destructured in `scan_cmd.rs::execute_scan` (or wherever ScanResult is unpacked pre-emission).
4. `ScanArtifacts.image_source: image_source.as_ref()` at construction.
5. Emitters consume `artifacts.image_source`.

## E7: Emitter branches for C124 (MODIFIED)

**Locations** (3 formats, mirror m204 verbatim):
- `mikebom-cli/src/generate/cyclonedx/metadata.rs` — after C123 helm-image-extraction-completeness branch.
- `mikebom-cli/src/generate/spdx/annotations.rs` — after C123 in `annotate_document`.
- `mikebom-cli/src/generate/spdx/v3_annotations.rs` — after C123.

**CDX branch example**:

```rust
// Milestone 206 (#440): C124 doc-scope image-source annotation.
// Emitted iff image_source is Some(Podman) — docker/remote scans
// emit no annotation per FR-005 byte-identity for pre-m206 goldens.
if artifacts.image_source == Some(&ImageSource::Podman) {
    properties.push(json!({
        "name": "mikebom:image-source",
        "value": "podman",
    }));
}
```

**SPDX 2.3 + SPDX 3**: same conditional check + `push(&mut out, "mikebom:image-source", json!("podman"))`.

## E8: Parity catalog row C124 (NEW)

**Locations** (4 files, mirror m204 registration):
- `mikebom-cli/src/parity/extractors/cdx.rs`: `cdx_anno!(c124_cdx, "mikebom:image-source", document);`
- `mikebom-cli/src/parity/extractors/spdx2.rs`: `spdx23_anno!(c124_spdx23, "mikebom:image-source", document);`
- `mikebom-cli/src/parity/extractors/spdx3.rs`: `spdx3_anno!(c124_spdx3, "mikebom:image-source", document);`
- `mikebom-cli/src/parity/extractors/mod.rs`: register `ParityExtractor { row_id: "C124", label: "mikebom:image-source", cdx: c124_cdx, spdx23: c124_spdx23, spdx3: c124_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false }` — inserted AFTER C123 in numerical order (m205 lesson: EXTRACTORS array must be numerically sorted).

## Cross-cutting: FR-005 byte-identity guardrail

**Guarantee**: `mikebom:image-source` annotation is conditional (emit only when `image_source == Some(Podman)`). Pre-m206 goldens for docker-source + registry-source scans emit nothing at the C124 slot → they're byte-identical post-m206.

**Enforcement**: SC-005 assertion via `git diff --stat mikebom-cli/tests/fixtures/` post-implementation. Drift is expected ONLY on newly-added podman-source golden fixtures (none in MVP — the US1 integration test uses `alpine:3.19` pulled at test time, no committed goldens).
