# Implementation Plan: OCI Referrers API SBOM discovery

**Branch**: `186-oci-referrers-sbom` | **Date**: 2026-07-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/186-oci-referrers-sbom/spec.md`

## Summary

Adds an opt-in OCI Distribution Spec v1.1 Referrers API discovery path so mikebom can OPTIONALLY fetch an upstream-published SBOM from the registry instead of scanning the image bytes. Three-mode CLI flag `--sbom-source [scan|referrer|either]` gates the behavior; default `scan` preserves pre-m186 byte-identity.

**Technical approach**: Small additive extensions at four isolated code sites in `mikebom-cli/`:

- **`cli/scan_cmd.rs`** — add `SbomSourceMode` enum + `--sbom-source` `Args`-derive flag; branch the image-scan dispatch on the enum value.
- **`scan_fs/oci_pull/mod.rs`** — new `pub async fn try_fetch_referrer_sbom(...) -> Option<Vec<u8>>` entry point that queries the Referrers endpoint after manifest resolution + filters by media type + fetches the descriptor blob + verifies its SHA-256.
- **`scan_fs/oci_pull/registry.rs`** — new `pub(super) async fn fetch_referrers(...) -> Result<ImageIndex>` method on `RegistryClient` that GETs `/v2/<repo>/referrers/<digest>` with the same auth + TLS + retry semantics as manifest fetches.
- **`scan_fs/oci_pull/referrers.rs`** (new file) — media-type filter + priority ordering + size-cap enforcement helpers colocated for testability.

Zero new production Cargo dependencies. Reuses `oci-spec = "0.9"` for `ImageIndex` parsing (already a workspace dep), `reqwest` for HTTP (workspace), `sha2` for digest verification (already used by `registry.rs::verify_sha256`).

The `either` mode's fall-through path routes to the existing `pull_to_tarball` → docker-save-tarball → scan pipeline unchanged. The `referrer` mode short-circuits before any scan pipeline runs, writing the fetched bytes verbatim to the operator's `--output` path.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–185; no nightly required).

**Primary Dependencies**: Existing only — `oci-spec = "0.9"` (workspace, `features = ["distribution", "image"]` — `ImageIndex` already used for manifest-list resolution at m031; the same type parses Referrers responses per OCI Distribution Spec v1.1); `reqwest = "0.12"` (workspace, `features = ["json", "rustls-tls", "blocking"]`); `sha2` (existing, already used by `registry.rs::verify_sha256`); `serde`/`serde_json` (existing); `tracing` (existing INFO/WARN logs); `anyhow`/`thiserror` (existing). Reuses m034 credential-resolution + m036 layer-cache + m182 TLS configuration verbatim. **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan. Referrer bytes flow through memory (a `Vec<u8>` up to the 100 MiB cap) then to the operator's `--output` path. Optionally cached via the m036 blob cache if the descriptor's digest is already known — but m186 does NOT extend the cache to Referrers-index responses themselves (each `--sbom-source referrer|either` invocation re-queries the endpoint; this preserves freshness for a well-known-mutable registry surface).

**Testing**: `cargo +stable test --workspace` (unit + integration tests), `cargo +stable clippy --workspace --all-targets -- -D warnings` (lint). Integration tests reuse the m182 `wiremock` dev-dep (already in workspace since m055) to mock Referrers endpoint responses + descriptor blobs.

**Target Platform**: Linux + macOS user-space (unchanged from prior milestones).

**Project Type**: CLI (Rust binary + shared common crate). Existing three-crate architecture: `mikebom-cli`, `mikebom-common`, `xtask`.

**Performance Goals**: The Referrers query adds at most ONE HTTP round-trip to the existing image-pull flow (SC-002 pins the 10% overhead ceiling under `either` mode when no referrer is found). Under `referrer` mode when a referrer IS found, m186 skips the image-blob-fetch + docker-save-assembly + scan-pipeline entirely — SC-001 pins the ≥2x speedup target (referrer emit typically <2s vs scan ≥5s for 50 MB image).

**Constraints**: FR-015 + SC-004 byte-identity guard on `--sbom-source scan` (default). The Referrers endpoint MUST NOT be invoked when the flag is `scan` (or unspecified) — zero network activity beyond the existing image-pull scan path. SC-008 zero-new-dep gate (cargo tree line-count identical pre/post).

**Scale/Scope**: 3 user stories (mode variants), 4 code sites, 1 new module (`referrers.rs`), 1 CLI flag. Estimated ~26-30 tasks across 6 phases.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Principle I (Pure Rust, Zero C)** — PASS. Zero new Cargo dependencies. Existing `oci-spec` + `reqwest` + `sha2` + stdlib cover all needs.

**Principle II (eBPF-Only Observation)** — N/A. m186 is user-space registry-fetch work; `mikebom-ebpf` untouched.

**Principle III (Fail Closed)** — PASS. Under `--sbom-source referrer` (US2), every fall-through condition surfaces an actionable error (FR-009). No silent fallbacks in strict mode. Under `either` (US1), fall-throughs are silent by design (that's the mode's contract) but each condition still logs at INFO for auditability.

**Principle IV (Type-Driven Correctness)** — PASS. `SbomSourceMode` enum is compile-time-typed with clap `ValueEnum` derive; no stringly-typed dispatch. Referrers response deserializes into `oci-spec::image::ImageIndex` — no manual JSON walking.

**Principle V (Specification Compliance + Native-first)** — PASS. The Referrers API is native to OCI Distribution Spec v1.1 §Referrers; SBOM media types (`application/spdx+json`, `application/vnd.cyclonedx+json`, `application/vnd.cyclonedx+xml`) are industry-standard identifiers. No `mikebom:*` invention in the wire protocol. The two `mikebom:sbom-source-*` provenance markers appear in mikebom's OWN scan-run log stream, NOT in the emitted SBOM content — preserving the upstream signer's byte-identity contract.

**Principle VI (Three-Crate Architecture)** — PASS. Changes confined to `mikebom-cli/src/`. Zero changes to `mikebom-common`, `mikebom-ebpf`, or `xtask`.

**Principle VII (Test Isolation)** — PASS. New unit tests colocated with `referrers.rs` (media-type filter, priority ordering, size-cap). Integration tests via new `mikebom-cli/tests/oci_referrers_*.rs` files using the m182 wiremock infrastructure (already dev-dep).

**Principle VIII (Completeness)** — PASS. m186 covers the three-mode CLI + all fall-through conditions listed in FR-008. Deferred cases (signed-verification, transcoding, additional media types, multi-referrer emission, artifactType filter) explicitly documented in spec.md.

**Principle IX (Accuracy)** — PASS. Byte-identity emission (FR-006) preserves the upstream signer's accuracy contract. mikebom does NOT synthesize or transform referrer bytes.

**Principle X (Transparency)** — PASS. FR-007 + SC-005 pin the audit-log requirement. Operators consuming mikebom logs can identify referrer-sourced emissions from log content alone.

**Principle XI (Enrichment)** — N/A. No external-data enrichment for m186.

**Principle XII (External Data Source Enrichment)** — N/A. Same as XI.

**Result**: All 12 principles PASS. No violations to justify. No Complexity Tracking table needed.

## Project Structure

### Documentation (this feature)

```text
specs/186-oci-referrers-sbom/
├── plan.md                    # This file
├── research.md                # Phase 0 output (5 decisions)
├── data-model.md              # Phase 1 output (SbomSourceMode enum + Referrers response shape + dispatch matrix)
├── quickstart.md              # Phase 1 output (operator + developer worked examples)
├── contracts/
│   ├── cli-flag.md            # `--sbom-source` semantics + mode-vs-input-type × output-format matrix
│   └── referrers-pipeline.md  # Query → filter → verify → emit contract (fall-through table)
├── checklists/
│   └── requirements.md        # 16/16 PASS from /speckit-specify
├── spec.md                    # Feature specification
└── tasks.md                   # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── cli/
│   │   └── scan_cmd.rs                    # US1/US2/US3 — new `SbomSourceMode` enum + `--sbom-source` flag + dispatch branch
│   └── scan_fs/
│       └── oci_pull/
│           ├── mod.rs                     # US1/US2 — new `try_fetch_referrer_sbom` entry point
│           ├── registry.rs                # US1/US2 — new `RegistryClient::fetch_referrers` method
│           └── referrers.rs               # US1/US2 — NEW FILE — media-type filter + priority + size-cap helpers + unit tests
└── tests/
    ├── oci_referrers_either_mode.rs       # NEW — US1 integration tests (wiremock)
    ├── oci_referrers_strict_mode.rs       # NEW — US2 integration tests (wiremock)
    └── oci_referrers_backward_compat.rs   # NEW — US3 default-scan byte-identity pin

mikebom-cli/tests/fixtures/golden/         # UNCHANGED — no existing fixture uses --sbom-source; SC-004 pins byte-identity
```

**Structure Decision**: Four-file, single-crate scope inside `mikebom-cli/src/`. One new file (`referrers.rs`) colocated with the existing OCI-pull family for module cohesion. Three new integration test files (one per user story) using the m182 wiremock precedent. No cross-crate coordination. Follows the m182 minimal-touch precedent — the m182 `RegistryTlsConfig` threading pattern is unchanged (m186's fetch reuses the same reqwest client under the same TLS config).

## Complexity Tracking

*No violations to justify — all 12 constitution principles PASS.*
