---
description: "Task list for m206 — scan local podman images (issue #440)"
---

# Tasks: Scan Local Podman Images

**Input**: Design documents from `/specs/206-podman-source/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/podman-storage-layout.md ✓, quickstart.md ✓

**Tests**: Tests-included. US1 (P1 MVP) gets a gated integration test + storage-layout parser unit tests. US2 (rootful) + US3 (auto-detection) each get one additional integration test. FR-005 byte-identity has a dedicated in-process regression guard.

**Organization**: 6 phases — setup (baseline recon), foundational (types + parsers + unit tests, no user-visible change), then 3 P1/P2 story phases, then polish. All 3 stories exercise the same underlying `podman_source.rs` module — each phase adds the story-specific integration test that pins its acceptance criteria.

## Format: `[ID] [P?] [Story] Description with file path`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1, US2, US3 mapping to spec.md user stories
- **File paths**: absolute or repo-relative — every task cites exact target

## Phase 1: Setup (Baseline + Recon)

**Purpose**: Establish pre-m206 baseline for SC-005 (byte-identity) and SC-006 (pre-PR delta). Verify plan.md / data-model.md line-numbers are still valid + podman is reachable for the US1 gated test.

- [ ] T001 Verify pre-m206 baseline pre-PR is green: run `./scripts/pre-pr.sh` on branch `206-podman-source` HEAD (post-checkout, pre-implementation) and capture wall-clock time to `/tmp/m206-prepr-baseline.txt` for SC-006 delta measurement.
- [ ] T002 [P] Golden-drift baseline: `git diff --stat main -- mikebom-cli/tests/fixtures/` (expected empty — branch is spec+plan only) → record to `/tmp/m206-golden-baseline.txt`. Post-implementation the diff MUST show ZERO drift for pre-existing goldens (SC-005 assertion).
- [ ] T003 [P] Recon: run quickstart.md `Empirical re-verification at implement time` block. Concretely:
  - `grep -n "pub enum ImageSource\|Docker,\|Remote,\|default_value = \"docker,remote\"" mikebom-cli/src/cli/scan_cmd.rs | head` — expect enum def at 54-62; default value at 234.
  - `grep -n "assemble_docker_save_tarball" mikebom-cli/src/scan_fs/oci_pull/tarball.rs` — confirm assembler helper at line 66.
  - `grep -n "fn extract" mikebom-cli/src/scan_fs/docker_image.rs | head` — confirm extract fn at line 96.
  - `grep -oE 'row_id: "C1[0-9]+"' mikebom-cli/src/parity/extractors/mod.rs | sort -u | tail` — confirm C123 highest → C124 free.
  - Podman availability probe: `command -v podman && podman --version 2>&1 | head`. If absent, tag T010 (US1 gated integration test) as skip-locally + note in the PR body.
  - Record all outputs to `/tmp/m206-recon.txt`.

## Phase 2: Foundational (Prerequisites for ALL user stories)

**Purpose**: Add the storage-layout types + parsers + entry function. NO CLI wiring or emitter changes yet — those land in Phase 3. Every user story's test transitively depends on these.

- [ ] T004 Create `mikebom-cli/src/scan_fs/podman_source.rs` with:
  - Module doc-comment citing issue #440, spec assumptions (Linux-only, overlay-driver-only MVP), and the contracts/podman-storage-layout.md pin.
  - Standard imports: `std::collections::HashMap`, `std::path::{Path, PathBuf}`, `std::io::Write`, `serde` / `serde_json`, `walkdir::WalkDir`, `tar`, `flate2::write::GzEncoder`, `sha2`, `data-encoding::HEXLOWER`, `tempfile`, `tracing`, `thiserror`, `oci_spec::image::{ImageManifest, ImageConfiguration}`.
  - `PodmanSourceError` enum with 7 variants per data-model E1 verbatim (Display strings must match; the FR-007 WARN log and integration-test assertions grep for them).
  - Add `pub mod podman_source;` line to `mikebom-cli/src/scan_fs/mod.rs`.
- [ ] T005 Add `PodmanImageRef` enum + `parse` associated fn to `podman_source.rs` per data-model E2. 3 variants (Tagged, Digest, ImageId); parser handles all 3 forms per FR-003. Default tag `latest` if `:tag` absent.
- [ ] T006 [P] Add storage-layout parser functions to `podman_source.rs`:
  - `discover_storage_root(rootless: bool) -> Result<PathBuf, PodmanSourceError>` — read `containers.conf` (rootful `/etc/containers/storage.conf` or rootless `$HOME/.config/containers/storage.conf`) via `toml = "0.8"` (workspace dep); look for `[storage] graphroot = "..."` key. Fall back to compiled-in defaults per FR-002 / research R2.
  - `parse_images_index(graphroot: &Path) -> Result<Vec<ImageRecord>, PodmanSourceError>` — read `<graphroot>/overlay-images/images.json`, deserialize per contracts/podman-storage-layout.md §Image Index into a private `ImageRecord` struct with `id`, `digest`, `names: Vec<String>`, `layer` fields.
  - `parse_layers_index(graphroot: &Path) -> Result<HashMap<String, LayerRecord>, PodmanSourceError>` — read `<graphroot>/overlay-layers/layers.json`, deserialize into a `HashMap<layer_id, LayerRecord>` where `LayerRecord` has `parent`, `diff_digest`, `compressed_diff_digest` fields.
  - `resolve_image_ref(index: &[ImageRecord], parsed_ref: &PodmanImageRef) -> Result<&ImageRecord, PodmanSourceError>` — match tag/digest/short-ID per FR-003. Short-ID accepts any prefix ≥ 12 chars of `.id`.
  - `resolve_layer_chain(layers: &HashMap<String, LayerRecord>, top_layer: &str) -> Vec<&LayerRecord>` — walk parent chain, return in base-to-top order per contracts §Layer Index chain algorithm.
  - `detect_storage_driver(graphroot: &Path) -> Result<StorageDriver, PodmanSourceError>` — check directory presence at `<graphroot>/overlay/` vs `vfs/` vs `btrfs/`; return `StorageDriver::Overlay` OR `PodmanSourceError::UnsupportedDriver`.
- [ ] T007 [P] Add unit tests to `mikebom-cli/src/scan_fs/podman_source.rs::tests`:
  - `podman_image_ref_parse_tagged_default_latest` — `"alpine"` → `Tagged { repo: "alpine", tag: "latest" }`.
  - `podman_image_ref_parse_tagged_explicit` — `"alpine:3.19"` → `Tagged { repo: "alpine", tag: "3.19" }`.
  - `podman_image_ref_parse_digest` — `"alpine@sha256:abc…64hex"` → `Digest { ... }`.
  - `podman_image_ref_parse_image_id` — `"abcdef123456"` → `ImageId { id_prefix: "abcdef123456" }`.
  - `podman_image_ref_parse_empty_errors` — `""` → `Err(ImageNotFound { image_ref: "" })`.
  - `discover_storage_root_honors_graphroot_override` — create tempdir with fake `storage.conf` setting `graphroot = "/some/path"`; env-mask `$HOME` to the tempdir; assert `discover_storage_root(true)` returns `/some/path`.
  - `discover_storage_root_falls_back_to_default_when_no_config` — tempdir with no `storage.conf`; assert default `$HOME/.local/share/containers/storage/` path returned.
  - `parse_images_index_matches_by_tag` — write a synthetic `images.json` with 2 entries; call `resolve_image_ref` with `Tagged { repo: "alpine", tag: "3.19" }`; assert the correct record is returned.
  - `parse_images_index_matches_by_short_id` — same fixture; ref `ImageId { id_prefix: <first 12 chars of id> }`; assert match.
  - `resolve_layer_chain_returns_base_to_top` — build synthetic `layers.json` with 3-layer chain; assert order.
  - `detect_storage_driver_returns_overlay_when_overlay_dir_present` — synthetic tempdir with `<graphroot>/overlay/`; assert `StorageDriver::Overlay`.
  - `detect_storage_driver_returns_unsupported_when_vfs` — synthetic tempdir with `<graphroot>/vfs/`; assert `UnsupportedDriver { driver: "vfs" }`.
  - `podman_source_error_display_formats_all_variants_m206` — construct all 7 variants + format each; assert Display strings match data-model E1 exactly (needed for T013 stderr assertions).
- [ ] T008 Post-T004/T005/T006/T007 sanity: run `CARGO_TARGET_DIR=/tmp/m206-c cargo +stable check --workspace --tests 2>&1 | tail -20`. Expected: clean compile. Dead-code warnings on `resolve_and_pack` are ACCEPTABLE at this checkpoint (Phase 3 wires it in).

## Phase 3: User Story 1 — Rootless podman image scan (Priority: P1 MVP)

**Story Goal**: An operator on a Linux host with rootless podman can invoke `mikebom sbom scan --image <name>:<tag> --image-src podman` and get a full SBOM (apk/deb/rpm/etc detected from the extracted rootfs) with `mikebom:image-source = "podman"` document-scope annotation.

**Independent Test Criterion**: `podman pull alpine:3.19` + `mikebom sbom scan --image alpine:3.19 --image-src podman --format cyclonedx-json --output out.cdx.json` produces a CDX SBOM where (a) at least 10 `pkg:apk/` components appear (alpine base packages), (b) `metadata.properties[]` contains `mikebom:image-source = "podman"`, (c) exit code 0.

- [ ] T009 [US1] Implement `pub fn resolve_and_pack(image_ref: &str, out_tarball: &Path, storage_root: Option<&Path>) -> Result<(), PodmanSourceError>` in `mikebom-cli/src/scan_fs/podman_source.rs` per data-model E3's 5-phase flow:
  1. Discover storage root (T006 `discover_storage_root`; honor `storage_root` param if `Some(_)`).
  2. Detect storage driver (T006 `detect_storage_driver`).
  3. Parse image index + resolve image ref (T005 + T006).
  4. Load OCI manifest + config from `<graphroot>/overlay-images/<image-id>/{manifest,config}` via `serde_json::from_slice` into `oci_spec::image::ImageManifest` + `ImageConfiguration`.
  5. For each layer in the OCI manifest's `layers[]` chain: locate the c/storage internal layer ID via `layers_json` reverse lookup on `diff-digest` OR `compressed-diff-digest`; walk `<graphroot>/overlay/<layer-id>/diff/` with `walkdir::WalkDir` sorted lexicographically; write into `tar::Builder<GzEncoder<Vec<u8>>>` with `HeaderMode::Deterministic` per contracts/podman-storage-layout.md §Layer Content note on reproducibility; verify computed SHA-256 matches OCI manifest's declared digest per FR-012 → return `PodmanSourceError::LayerDigestMismatch` on mismatch.
  6. Assemble via `crate::scan_fs::oci_pull::tarball::assemble_docker_save_tarball(&config_bytes, &pulled_layers, image_ref, out_tarball)`. Bubble any error.
- [ ] T010 [US1] Add `ImageSource::Podman` variant to `mikebom-cli/src/cli/scan_cmd.rs:54-62` per data-model E4. Include doc-comment citing m206 (#440) + spec Assumption 1 (Linux-only). ALSO update the `default_value` string at scan_cmd.rs:234 from `"docker,remote"` to `"docker,podman,remote"` per FR-006.
- [ ] T011 [US1] Add Podman dispatch branch to `resolve_image_ref` in `mikebom-cli/src/cli/scan_cmd.rs:1908+` per data-model E5. Analogous to the Docker branch at lines 1910-1948. Calls `scan_fs::podman_source::resolve_and_pack(arg_str, &tarball_path, None)`. On success: `selected_source = Some(ImageSource::Podman)`, break with the tarball path. On error: WARN log naming the error, `continue` to next `--image-src` entry.
- [ ] T012 [US1] Wire `ScanResult.image_source` + `ScanArtifacts.image_source` field per data-model E6 (mirrors m204 helm_extraction_mode 8-hop plumbing pattern):
  - Add field to `mikebom-cli/src/scan_fs/mod.rs::ScanResult` struct definition.
  - Add mirror line in `scan_path` function (top-of-function `let mut image_source: Option<...> = None;` + assignment where `selected_source` is threaded up + return via struct literal).
  - Add field to `mikebom-cli/src/generate/mod.rs::ScanArtifacts` struct (borrow type).
  - Hunt for every `ScanArtifacts { ... }` construction site in `mikebom-cli/src/cli/scan_cmd.rs`, `mikebom-cli/src/generate/spdx/{document,mod,packages,relationships}.rs`, `mikebom-cli/src/generate/spdx/v3_document.rs`, `mikebom-cli/src/generate/openvex/mod.rs` and add `image_source: <value>` at each site. Concrete: `grep -c "helm_extraction_mode:" mikebom-cli/src/generate/spdx/*.rs mikebom-cli/src/generate/spdx/v3_*.rs mikebom-cli/src/generate/openvex/mod.rs mikebom-cli/src/cli/scan_cmd.rs` to enumerate all sites (m204's identical pattern).
- [ ] T013 [P] [US1] CDX emit branch: add C124 conditional emission to `mikebom-cli/src/generate/cyclonedx/metadata.rs` immediately after C123 (helm image-extraction-completeness) per data-model E7:
  ```rust
  if artifacts.image_source == Some(&ImageSource::Podman) {
      properties.push(json!({
          "name": "mikebom:image-source",
          "value": "podman",
      }));
  }
  ```
  Also: append `image_source: Option<&ImageSource>` as new arg to `build_metadata` signature; update the ~25 test callsites in metadata.rs to pass `None`. Update production callsite in `builder.rs` to pass `scan_artifacts.image_source`. Extend `CycloneDxBuilder` with `helm_extraction_mode`-analogous field + `with_image_source()` setter.
- [ ] T014 [P] [US1] SPDX 2.3 emit branch in `mikebom-cli/src/generate/spdx/annotations.rs::annotate_document` — mirror of T013 using existing `push(&mut out, "mikebom:image-source", json!("podman"))` helper. Conditional on `artifacts.image_source == Some(&ImageSource::Podman)`.
- [ ] T015 [P] [US1] SPDX 3 emit branch in `mikebom-cli/src/generate/spdx/v3_annotations.rs` — analogous to T014.
- [ ] T016 [P] [US1] Register catalog row C124 across 4 files per data-model E8 (mirror m204 C123 pattern):
  - `mikebom-cli/src/parity/extractors/cdx.rs`: `cdx_anno!(c124_cdx, "mikebom:image-source", document);` immediately after the C123 line.
  - `mikebom-cli/src/parity/extractors/spdx2.rs`: `spdx23_anno!(c124_spdx23, "mikebom:image-source", document);` immediately after C123.
  - `mikebom-cli/src/parity/extractors/spdx3.rs`: `spdx3_anno!(c124_spdx3, "mikebom:image-source", document);` immediately after C123.
- [ ] T017 [US1] Register `ParityExtractor { row_id: "C124", label: "mikebom:image-source", cdx: c124_cdx, spdx23: c124_spdx23, spdx3: c124_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false }` in `mikebom-cli/src/parity/extractors/mod.rs` per data-model E8. **CRITICAL**: insert AFTER C123 in numerical sort order (m205 lesson: EXTRACTORS array must be numerically sorted, else `extractors_table_is_sorted_by_row_id` test fails). Also add `c124_cdx`, `c124_spdx23`, `c124_spdx3` to the 3 import lists at lines ~62/73/84 — CAUTION: mimic m204 pattern using `sed -i.bak -E 's|c123_cdx,|c123_cdx, c124_cdx,|; s|c123_spdx23,|c123_spdx23, c124_spdx23,|; s|c123_spdx3,|c123_spdx3, c124_spdx3,|' mikebom-cli/src/parity/extractors/mod.rs` — verify only the imports changed (not the ParityExtractor body).
- [ ] T018 [US1] Post-Phase-3 sanity: `cargo +stable check --workspace --tests 2>&1 | tail -20`. Expected clean compile. Verify T017's sed didn't accidentally modify the ParityExtractor row bodies (grep for `c124_.*,.*c123_` — should NOT match).
- [ ] T019 [US1] Create `mikebom-cli/tests/scan_image_podman_source.rs` (mirror `mikebom-cli/tests/scan_image_docker_daemon.rs` structure). Include:
  - `#[cfg(target_os = "linux")]` module gate.
  - Helper `require_podman_integration()` → returns `bool` based on `MIKEBOM_PODMAN_INTEGRATION=1` env var check + `podman --version` probe. Skip cleanly with `eprintln!` if either check fails (matches m188/m203/m205 gating pattern).
  - Helper `ensure_alpine_cached()` → runs `podman pull alpine:3.19` if not already cached.
  - `us1_podman_source_scans_rootless_alpine` test:
    - Skip guard via `require_podman_integration()`.
    - Call `ensure_alpine_cached()`.
    - Shell out to mikebom binary via `env!("CARGO_BIN_EXE_mikebom")` with `sbom scan --image alpine:3.19 --image-src podman --format cyclonedx-json --output <tempfile> --offline --no-deep-hash`.
    - Assert:
      - (a) Exit code 0.
      - (b) `.components[]` contains ≥ 10 entries with `.purl` starting with `pkg:apk/` (alpine base pkgs).
      - (c) `.metadata.properties[]` contains `{name: "mikebom:image-source", value: "podman"}`.

## Phase 4: User Story 2 — Rootful podman image scan (Priority: P2)

**Story Goal**: Same as US1 but with root euid and `/var/lib/containers/storage/` as the storage root. Implementation is a no-op — the same code path handles both once T006's `discover_storage_root` detects euid.

**Independent Test Criterion**: `sudo podman pull alpine:3.19` + `sudo mikebom sbom scan --image alpine:3.19 --image-src podman` produces the same SBOM shape as US1.

- [ ] T020 [US2] Add integration test `us2_podman_source_scans_rootful_image` to `mikebom-cli/tests/scan_image_podman_source.rs`:
  - `#[cfg(target_os = "linux")]` + `require_podman_integration()` gate + additional `MIKEBOM_PODMAN_ROOTFUL_INTEGRATION=1` gate (rootful test requires root privileges — skip unless explicitly opted in; matches m188 nightly-lane pattern).
  - Detect `geteuid() == 0` at test entry; skip cleanly if not root.
  - Runs `podman pull` as root then invokes mikebom — same assertions as T019 but against `/var/lib/containers/storage/`.
- [ ] T021 [US2] Add integration test `us2_podman_source_permission_denied_names_actionable_error` (unix-only, no-root guard):
  - Skip if running as root (this test exercises the non-root user hitting rootful storage).
  - Create a synthetic tempdir mimicking `/var/lib/containers/storage/` layout but with `chmod 0700` (unreadable by non-root).
  - Invoke mikebom `--image alpine:3.19 --image-src podman` with the env var `MIKEBOM_PODMAN_STORAGE_ROOT_OVERRIDE=<tempdir>` (or pass the storage_root param path if the CLI exposes an escape hatch — otherwise skip this test and note in code comment that Phase-2 escape-hatch flag would enable this).
  - Assert: exit code non-zero, stderr WARN contains "permission" OR "unreadable" AND mentions the specific path.

## Phase 5: User Story 3 — Auto-detection between docker + podman sources (Priority: P2)

**Story Goal**: Operator runs `mikebom sbom scan --image <name>:<tag>` without `--image-src`. Default order `docker,podman,remote` tries docker first, falls back to podman when docker doesn't have the image, and only reaches remote if neither local tool has it.

**Independent Test Criterion**: With alpine cached in podman but NOT docker, invoking mikebom WITHOUT `--image-src` succeeds via podman fallback (no premature error about docker-not-found).

- [ ] T022 [US3] Add integration test `us3_default_order_falls_back_from_docker_to_podman` to `mikebom-cli/tests/scan_image_podman_source.rs`:
  - Full gate: `#[cfg(target_os = "linux")]` + `require_podman_integration()` + probe for `docker` binary (skip cleanly if docker not installed — the test needs both tools to demonstrate fallback).
  - Setup: `docker rmi alpine:3.19 2>/dev/null || true` + `podman pull alpine:3.19` (idempotent).
  - Invoke mikebom `--image alpine:3.19` (NO `--image-src` flag — use default).
  - Assert:
    - (a) Exit 0.
    - (b) stderr contains "docker source failed" OR "trying next" (WARN fired but scan proceeded).
    - (c) `.metadata.properties[]` contains `mikebom:image-source = "podman"` (winning source).

## Phase 6: Polish & Delivery

**Purpose**: Verification, byte-identity audit, docs mapping-row, PR body.

- [ ] T023 FR-005 byte-identity regression test `fr005_non_image_scan_omits_image_source_annotation` in a NEW file `mikebom-cli/tests/podman_source_byte_identity.rs` (default CI, cross-platform):
  - Scan an existing non-image public_corpus fixture (`mikebom-cli/tests/fixtures/public_corpus/npm-express/`) via the mikebom binary.
  - Assert emitted CDX contains NO `.metadata.properties[]` entry with `.name == "mikebom:image-source"` (image_source is None for --path scans → conditional annotation absent).
- [ ] T024 [P] Add C124 row to `docs/reference/sbom-format-mapping.md` immediately after C123 following the C108 shape:
  - Label: `mikebom:image-source`.
  - Per-emitter mapping (CDX `metadata.properties[]`, SPDX 2.3 doc-scope Annotation with `MikebomAnnotationCommentV1` envelope, SPDX 3 Annotation element).
  - **KEEP-NO-NATIVE** verdict with rejected-alternatives list per Constitution Principle V audit (plan.md §V):
    - CDX `metadata.tools[]` — names SBOM-producing tool, not image-caching tool. Semantic mismatch.
    - SPDX 2.3 `creationInfo.tools[]` — same rejection.
    - SPDX 3 `SoftwareArtifact.software_downloadLocation` — WHERE the artifact was fetched from, not WHICH local tool serves it. Rejected.
    - SPDX 3 `Element.originatedBy` — upstream-supplier metadata, not local-tool metadata. Rejected.
- [ ] T025 [P] Run every existing image-source integration test to confirm zero regression:
  - `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --test scan_image_docker_daemon --no-fail-fast 2>&1 | tail -3` (m031 docker-source; MUST stay green post-m206).
  - `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --test parity_synthetic_drift --test holistic_parity --no-fail-fast 2>&1 | tail -5` (m071 parity suite; must exercise C124 automatically post-registration).
- [ ] T026 Re-run T002 audit post-implementation: `git diff --stat mikebom-cli/tests/fixtures/`. Compare to /tmp/m206-golden-baseline.txt. Assert ZERO drift on pre-existing goldens (SC-005). Only new files should be in the delta (the new `scan_image_podman_source.rs` + `podman_source_byte_identity.rs` — no fixture files, just tests).
- [ ] T027 Run `./scripts/pre-pr.sh` post-implementation. Capture wall-clock time; compute delta vs T001 baseline; MUST be ≤ 10s per SC-006. On failure, enumerate every `^---- .+ stdout ----` line per `feedback_prepr_gate_bails_on_first_failure` memory.
- [ ] T028 [P] (Optional, requires podman + docker on Linux host) Manually execute quickstart.md Reproducers 1, 2, 3, 4 for end-to-end verification. Reproducer 5 (non-overlay driver) optional-optional — only applicable on hosts with vfs/btrfs configured.
- [ ] T029 Draft PR body with `Closes #440` per SC-007. Include:
  - (a) 1-paragraph summary: what podman-source enables + why (reporter's ecosystem: RHEL/Fedora + rootless CI operators had zero inventory previously).
  - (b) Design choice callouts: Strategy B tarball-reuse (research R1), storage-root discovery via `containers.conf` (R2), overlay-only MVP with WARN+fallback for vfs/btrfs (R3), conditional `mikebom:image-source` annotation preserves FR-005 byte-identity (R4).
  - (c) Test coverage: US1 gated + US2 rootful+permission-denied + US3 auto-detect + FR-005 byte-identity guard + C124 parity via m071 suite.
  - (d) Code-diff LOC + files: ~400 LOC across 1 new source module + 1 new integration test file + 4 emitter/parity infra files + CLI wiring + docs mapping row.
  - (e) macOS/Windows scope note: `podman machine` VM introspection deferred to follow-up per spec Assumption 1.
  - (f) Follow-up hooks: `--podman-storage-root <path>` escape hatch (deferred; add if operators surface need), `vfs`/`btrfs` driver support (deferred), podman REST API path (FR-009 deferred).

---

## Dependencies

Sequential within phases; phases mostly sequential across the milestone:

```
Phase 1 (Setup) ── T001, T002, T003 in parallel
     ↓
Phase 2 (Foundational) ── T004 → T005 → T006, T007 in parallel → T008 (sanity)
     ↓
Phase 3 (US1) ── T009 → T010 → T011 → T012 → T013, T014, T015, T016 in parallel → T017 → T018 → T019
     ↓
Phase 4 (US2) ── T020, T021 in parallel (independent of US3)
     ↓
Phase 5 (US3) ── T022
     ↓
Phase 6 (Polish) ── T023 → T024, T025 in parallel → T026 → T027 → T028 → T029
```

**MVP** = Phase 1 + Phase 2 + Phase 3 (US1 only). Delivers: `mikebom sbom scan --image <name>:<tag> --image-src podman` works for rootless Linux operators. US2 (rootful + permission handling) + US3 (auto-detection) add on top; both reuse the US1 code path.

## Parallel opportunities

- **Setup**: T002, T003 read-only.
- **Foundational**: T006 (parser fns) + T007 (unit tests) — different sections of the same file, safe.
- **US1 emitters + parity extractors**: T013, T014, T015, T016 — 4 different files.
- **US2**: T020, T021 — different `#[test]` fns.
- **Polish**: T024, T025 — different concerns; T028 read-only.

## Implementation strategy

Ship as a single PR. Phase 2 (foundational types + parsers) + Phase 3 (US1 CLI wiring + emitter + integration test) form the coherent MVP; US2 + US3 are additive tests that share the same underlying code path (T020-T022 are test-only, no production changes).

**Total task count**: 29 tasks.
**By story**: US1 = 11 tasks (T009-T019), US2 = 2 tasks (T020-T021), US3 = 1 task (T022). Phase 1 = 3, Phase 2 = 5, Phase 6 = 7 = 15 non-story.
