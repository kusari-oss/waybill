# Implementation Plan: Scan Local Podman Images

**Branch**: `206-podman-source` | **Date**: 2026-07-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/206-podman-source/spec.md`

## Summary

Add a `Podman` variant to the existing `ImageSource` enum at `mikebom-cli/src/cli/scan_cmd.rs:54-62`, wire it into the `--image-src` dispatch at `scan_cmd.rs:1908`, and implement `mikebom-cli/src/scan_fs/podman_source.rs` as a NEW filesystem-only module that:

1. Reads podman's `c/storage` layout (`<graphroot>/overlay-images/`, `<graphroot>/overlay-layers/`, `<graphroot>/overlay/`) directly — no daemon or REST API.
2. Resolves the operator's `--image <ref>` against podman's image index (`overlay-images/images.json`).
3. Loads OCI manifest + config for the target image.
4. For each layer in the manifest's layer chain, reads the pre-unpacked `overlay/<layer-id>/diff/` directory and re-tars it (podman defaults to unpacking blobs; the compressed `.tar.gz` is not preserved on disk).
5. Assembles a docker-save-format tarball via the existing `oci_pull::tarball::assemble_docker_save_tarball` helper at `scan_fs/oci_pull/tarball.rs:66`.
6. Feeds the tarball to the existing `scan_fs::docker_image::extract` pipeline — the rootfs scan pipeline downstream is completely unchanged (FR-008 structural equivalence).

Default `--image-src` string bumps from `"docker,remote"` to `"docker,podman,remote"` per FR-006 (docker-first preserves byte-identity; podman-before-remote so local wins over network).

Adds a **new** conditional document-scope annotation `mikebom:image-source = "podman"` emitted ONLY when the winning source is podman (FR-014). Docker/registry scans emit no such annotation — preserving pre-m206 byte-identity for docker + registry goldens per FR-005 / SC-005. Registered as parity catalog row C124 with a KEEP-NO-NATIVE audit per Constitution Principle V.

Reconnaissance findings (per m199-m205 lesson):
- `ImageSource` enum at `scan_cmd.rs:54-62` — 2 variants (Docker, Remote); add Podman as a 3rd.
- Dispatch iteration at `scan_cmd.rs:1908` — need Podman branch analogous to Docker branch at 1910-1948.
- Default value literal `"docker,remote"` at `scan_cmd.rs:234` — bump to `"docker,podman,remote"`.
- `oci_pull::tarball::assemble_docker_save_tarball` at `oci_pull/tarball.rs:66` — signature: `(config_bytes: &[u8], layers: &[PulledLayer], image_ref: &str, out_path: &Path) -> Result<()>`. Reusable verbatim.
- `docker_image::extract` at `docker_image.rs:96` returns `ExtractedImage` with rootfs — unchanged.
- Docker-source integration test at `mikebom-cli/tests/scan_image_docker_daemon.rs` — uses REAL docker daemon (line 86: `docker pull alpine:3.19`). m206's US1 test follows the same pattern, gated behind `MIKEBOM_PODMAN_INTEGRATION=1` per m188/m203/m205 precedent.
- **No existing `mikebom:image-source` annotation** — m206 adds a fresh one.
- Podman uses c/storage library (github.com/containers/storage). Storage layout stable across podman v4+ (per issue #440 assumption).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–205; no nightly).
**Primary Dependencies**: Existing only —
- `oci-spec = "0.9"` (workspace, `features = ["distribution", "image"]` — already used by m031's `oci_pull` for `ImageManifest` + `ImageConfiguration` parsing).
- `tar = "0.4"` (workspace — layer re-tar for the assembler input).
- `flate2` (workspace — gzip-compress each re-tarred layer).
- `serde_json` (workspace — parse `images.json`, `layers.json`, per-image `manifest`).
- `sha2` + `data-encoding` (workspace — verify layer digests match the manifest).
- `tempfile` (workspace — scratch directory for the assembled tarball).
- `walkdir` (workspace — traverse `overlay/<layer-id>/diff/` for re-tar).
- `tracing` / `anyhow` / `thiserror`.

**Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; scratch tarball lives in a `tempfile::tempdir()` for the duration of the scan and is dropped at return.
**Testing**: New integration test at `mikebom-cli/tests/scan_image_podman_source.rs` (mirrors `scan_image_docker_daemon.rs`), gated behind `MIKEBOM_PODMAN_INTEGRATION=1`. Requires `podman` on `$PATH` + a locally-cached image to exist (test fixture setup runs `podman pull alpine:3.19` if not present, matching m031 docker-daemon test pattern). Unit tests in `podman_source.rs::tests` for the `c/storage` layout parser (image lookup by tag/digest/short-ID, layers.json chain resolution) using synthetic fixture directories built with `tempfile::tempdir()` — no podman binary needed.
**Target Platform**: Linux for the podman-scan feature. macOS/Windows operators get a clean error naming the `podman machine` VM-isolation limitation per spec Assumption 1. This constrains US1's integration test to Linux CI only — matches m173/m203 precedent for OS-specific integration tests.
**Project Type**: New source module + CLI wiring + emitter annotation. ~400 LOC total: ~250 LOC in the new `podman_source.rs` (storage-layout parser + tarball-assembly bridge), ~30 LOC of CLI wiring (`ImageSource::Podman` + dispatch branch + default-value bump), ~40 LOC of annotation emission (metadata.rs + spdx annotations + parity catalog row C124), ~100 LOC integration tests.
**Performance Goals**: Podman-source scan wall-clock should be comparable to docker-source scan of the same image (both extract the same rootfs; podman's pre-unpacked layers make re-tar CPU-bound rather than IO-bound). No explicit SC target; SC-006 covers the aggregate `./scripts/pre-pr.sh` delta.
**Constraints**: (a) zero new Cargo deps; (b) filesystem-only per FR-009 (no daemon/REST API); (c) `--offline` propagates unchanged per FR-013; (d) non-podman goldens byte-identical per FR-005/SC-005 (conditional annotation gated on `source == Podman`); (e) macOS/Windows operators get clear error per spec Assumption 1; (f) Linux-only integration test per constitution VII test-isolation.
**Scale/Scope**: 1 new source module (~250 LOC) + 1 CLI wiring extension + 1 new emitter branch + 1 parity catalog row + 1 new integration test file + 4 parity infra edits (cdx.rs, spdx2.rs, spdx3.rs, mod.rs for C124). No changes to mikebom-common. No changes to mikebom-ebpf. No changes to `docker_image::extract` (reused verbatim). No changes to `oci_pull::tarball::assemble_docker_save_tarball` (reused verbatim).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. All new Rust code. No subprocess calls (FR-009 filesystem-only). Reuses existing crates for tar/gzip/JSON parsing — no C deps introduced.
- **II. eBPF-Only Observation** — ✅ N/A. Image-source discovery is a filesystem-read mechanism, not build-time discovery. Principle II §2 permits static filesystem parsing as an enrichment/source path.
- **III. Fail Closed** — ✅ PASS. Missing podman storage → fall back to next `--image-src` entry per FR-007. Unsupported storage driver (`vfs`/`btrfs`) → WARN + fall back per spec Edge Case. Permission-denied on rootful storage → error naming the specific permission problem + suggested `sudo` workaround per US2 acceptance scenario. Every failure path is explicit + operator-observable + doesn't corrupt the scan.
- **IV. Type-Driven Correctness** — ✅ PASS. `ImageSource::Podman` is a typed enum variant (not a string). New `PodmanImageId` newtype wraps the c/storage internal ID; `PodmanImageRef` newtype covers the tag/digest/short-ID input forms. Storage-layout errors (`PodmanStorageError` enum) carry structured variants (missing-storage-root, unsupported-driver, image-not-found, corrupted-manifest).
- **V. Specification Compliance** — ⚠️ CONDITIONAL PASS. New `mikebom:image-source` annotation per FR-014 requires a Principle V native-first audit. Preliminary check:
  - **CDX 1.6**: no native `metadata.tools.image-source` or equivalent field expressing "which local tool cached this image." `metadata.tools[]` names the SBOM-producing tool (mikebom), not the image-caching tool.
  - **SPDX 2.3**: no native construct. `creationInfo.tools[]` names the SBOM-producing tool, not the image cache.
  - **SPDX 3**: no native construct. `SoftwareArtifact.software_downloadLocation` names WHERE the artifact was fetched from — semantically different from "which local tool serves this image" (docker vs podman is orthogonal to registry URL). Similarly `Element.originatedBy` is upstream-supplier metadata, not local-tool metadata.
  - **Rejected alternatives**: (1) CDX `metadata.properties[]` with a differently-named property — this IS such a property, naming is the only degree of freedom. (2) SPDX `CreationInfo.comment` free-text — loses machine-parseability. (3) SPDX 3 `Element.description` — free-text, wrong grain.
  - **Verdict**: `KEEP-NO-NATIVE` for the `mikebom:image-source` annotation. Catalog row C124.
  - **Byte-identity guardrail**: annotation is conditional (emit only when source == Podman) so docker/registry goldens don't drift per FR-005 / SC-005.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes stay in `mikebom-cli`.
- **VII. Test Isolation** — ✅ PASS. Unit tests use synthetic tempdir fixtures (no podman needed). Integration test (US1 end-to-end) gated behind `MIKEBOM_PODMAN_INTEGRATION=1` — Linux-only + real podman required. Matches m188/m203/m205 precedent.
- **VIII. Completeness** — ✅ PASS. Directly serves Principle VIII: podman users currently get zero inventory. m206 closes that discovery gap.
- **IX. Accuracy** — ✅ PASS. The extracted rootfs IS the actual rootfs podman would present to a running container — same content mikebom would scan if the operator ran `podman save > out.tar` + `mikebom scan --image out.tar` today. Structural equivalence per FR-008.
- **X. Transparency** — ✅ PASS. `mikebom:image-source = "podman"` gives consumers a machine-readable provenance signal. WARN logs on every fallback path per FR-007.
- **XI. Enrichment (DX)** — ✅ PASS. `--image <ref>` "just works" post-m206 for podman operators (SC-002). Zero new CLI flags for the common case — the operator's existing invocation gains podman support transparently. Explicit `--image-src podman` opt-in remains for operators who want to force the source.
- **XII. External Data Source Enrichment** — ✅ N/A. Podman is a local tool + local filesystem, not an external data source.
- **Strict Boundary §5 (file-tier)** — ✅ N/A. Not touching file-tier plumbing.

**Result**: All principles PASS or N/A. §V is CONDITIONAL-PASS pending the audit outcome documented above → resolved as KEEP-NO-NATIVE with C124 registration. No Complexity Tracking entries needed.

**Post-Phase-1 re-check**: N/A — Phase 1 introduces no new entities beyond what's above. Constitution gate remains PASS.

## Project Structure

### Documentation (this feature)

```text
specs/206-podman-source/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 4 mechanical decisions
├── data-model.md        # Phase 1 output — PodmanImageRef, PodmanStorageError, storage-layout types
├── contracts/
│   └── podman-storage-layout.md  # Phase 1 output — c/storage overlay layout mikebom parses
├── quickstart.md        # Phase 1 output — 4 reproducers
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/
├── podman_source.rs                                    # NEW — the entirety of m206's discovery logic:
│                                                        #
│                                                        # Public API (~30 LOC):
│                                                        #   pub fn resolve_and_pack(
│                                                        #     image_ref: &str,
│                                                        #     out_tarball: &Path,
│                                                        #     storage_root: Option<&Path>,
│                                                        #   ) -> Result<(), PodmanSourceError>
│                                                        #
│                                                        # Body sections (~220 LOC):
│                                                        #   - PodmanSourceError enum (7 variants)
│                                                        #   - PodmanImageRef parser (tag/digest/short-ID)
│                                                        #   - discover_storage_root() —
│                                                        #     rootless default → rootful fallback →
│                                                        #     honor containers.conf `graphroot` override
│                                                        #   - parse_images_index(<graphroot>/overlay-images/images.json)
│                                                        #   - parse_layers_index(<graphroot>/overlay-layers/layers.json)
│                                                        #   - resolve_image_ref() — match tag/digest/
│                                                        #     short-ID against index
│                                                        #   - load_image_manifest(<graphroot>/overlay-images/<id>/manifest)
│                                                        #   - load_image_config(<graphroot>/overlay-images/<id>/config)
│                                                        #   - re-tar each layer's overlay/<layer-id>/diff/
│                                                        #     directory into a temp .tar.gz
│                                                        #   - assemble docker-save tarball via
│                                                        #     oci_pull::tarball::assemble_docker_save_tarball
│                                                        #     (reused verbatim)
│                                                        #
│                                                        # Unit tests (~200 LOC):
│                                                        #   - synthetic fixture builder helpers
│                                                        #   - image-ref parsing (all 3 forms)
│                                                        #   - storage-root discovery (rootless/rootful/
│                                                        #     override)
│                                                        #   - image-index lookup by tag/digest/short-ID
│                                                        #   - layer chain resolution from layers.json
│                                                        #   - PodmanSourceError Display strings
│                                                        #     (needed for FR-007 WARN log matching)
├── mod.rs                                              # MODIFIED — `pub mod podman_source;` (single-line add)
└── docker_image.rs                                     # UNCHANGED — reused verbatim

mikebom-cli/src/cli/
└── scan_cmd.rs                                         # MODIFIED — the CLI wiring:
                                                        #
                                                        # Line 54-62: add ImageSource::Podman variant
                                                        # to the existing enum.
                                                        #
                                                        # Line 234: bump default_value from
                                                        # "docker,remote" to "docker,podman,remote".
                                                        #
                                                        # Line 1908+ (resolve_image_ref dispatch loop):
                                                        # add Podman branch analogous to the Docker
                                                        # branch at 1910-1948. Calls
                                                        # scan_fs::podman_source::resolve_and_pack(...)
                                                        # → passes resulting tarball path to the
                                                        # existing scan_fs::docker_image::extract call
                                                        # at line 2429 (unchanged).
                                                        #
                                                        # Line 2429+ post-extract: emit the
                                                        # mikebom:image-source diagnostic (see below).

mikebom-cli/src/scan_fs/
└── mod.rs                                              # MODIFIED — thread a new field
                                                        # `ScanResult.image_source: Option<ImageSource>`
                                                        # (or equivalent String type) through the
                                                        # ScanResult struct so the emitter can read it.
                                                        # 3 fields touched at declaration + construction
                                                        # + mirror sites, mirroring the m204
                                                        # helm_extraction_mode plumbing pattern.

mikebom-cli/src/generate/
├── mod.rs                                              # MODIFIED — add `image_source` field to
│                                                        # ScanArtifacts alongside helm_extraction_mode.
├── cyclonedx/metadata.rs                               # MODIFIED — one emit branch immediately after
│                                                        # C123 helm-image-extraction-completeness.
│                                                        # `if let Some(src) = image_source { if src
│                                                        # == ImageSource::Podman { push properties
│                                                        # entry mikebom:image-source = "podman" }}`
├── cyclonedx/builder.rs                                # MODIFIED — mirror the m204 CDX builder
│                                                        # helm_extraction_mode plumbing pattern.
└── spdx/
    ├── annotations.rs                                  # MODIFIED — one branch mirroring m204 for
    │                                                    # SPDX 2.3 annotate_document.
    └── v3_annotations.rs                               # MODIFIED — one branch for SPDX 3.

mikebom-cli/src/parity/extractors/
├── mod.rs                                              # MODIFIED — register C124 ParityExtractor
│                                                        # entry after C123 (numerical order).
├── cdx.rs                                              # MODIFIED — cdx_anno! for c124_cdx.
├── spdx2.rs                                            # MODIFIED — spdx23_anno! for c124_spdx23.
└── spdx3.rs                                            # MODIFIED — spdx3_anno! for c124_spdx3.

docs/reference/
└── sbom-format-mapping.md                              # MODIFIED — add C124 row per Principle V
                                                        # audit outcome. KEEP-NO-NATIVE with rejected-
                                                        # alternatives list.

mikebom-cli/tests/
└── scan_image_podman_source.rs                         # NEW — m206 integration test:
                                                        #
                                                        # us1_podman_source_scans_rootless_image
                                                        #   (MIKEBOM_PODMAN_INTEGRATION=1 gated,
                                                        #    #[cfg(target_os = "linux")]) — pulls
                                                        #   alpine:3.19 via `podman pull`, scans via
                                                        #   `mikebom sbom scan --image alpine:3.19
                                                        #   --image-src podman`, asserts:
                                                        #     - exit 0
                                                        #     - components[] has apk-detected entries
                                                        #       (base alpine packages)
                                                        #     - metadata.properties[] contains
                                                        #       mikebom:image-source = "podman"
                                                        #
                                                        # us3_default_order_falls_back_to_podman
                                                        #   (same gate) — pulls alpine via podman
                                                        #   only (docker cache empty), invokes with
                                                        #   default --image-src, asserts scan
                                                        #   succeeds via podman fallback.
                                                        #
                                                        # fr014_non_podman_scan_omits_image_source_annotation
                                                        #   (default CI) — scan a local directory
                                                        #   (not an image); assert emitted SBOM does
                                                        #   NOT contain mikebom:image-source annotation
                                                        #   (byte-identity preserved for non-image scans).
```

**Structure Decision**: 1 new source module + CLI wiring + emitter branches + parity registration + 1 new integration test file + 1 docs update. Zero committed fixture additions — synthetic unit-test fixtures via `tempfile::tempdir()`; US1 integration test uses real podman-cached image (test setup pulls if absent). Zero non-podman-source golden regen (annotation is conditional per FR-005).

## Complexity Tracking

No constitution violations. All principles pass. The plumbing pattern (new source under `scan_fs/`, dispatch in `resolve_image_ref`, tarball assembly reuse, docker_image::extract downstream) mirrors the m031 `oci_pull` module precedent. The emitter annotation follows the m204 `helm_extraction_mode` 8-hop pattern verbatim.
