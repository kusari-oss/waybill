# Implementation Plan: Pluggable fingerprint corpus v2

**Branch**: `110-pluggable-corpus-v2` | **Date**: 2026-06-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/Users/mlieberman/Projects/mikebom/specs/110-pluggable-corpus-v2/spec.md`

## Summary

Replace milestone-108's single-symbol-set fingerprint corpus with a multi-indicator schema (symbols + version strings + Build-IDs + ABI markers + ecosystem-alias PURLs + CPE candidates) and a pluggable fetch mechanism that allows mikebom-cli to consume corpora from one or more configured sources, each optionally authenticated. The matcher fuses indicators per-record into `high` / `medium` confidence buckets; below-medium fused matches are suppressed (per the 2026-06-03 clarification). The milestone-108 v1 record shape continues to load via a single-indicator compatibility path with a fixed 0.70 confidence baseline mapping to `medium`. Cache layout extends milestone-108's `~/.cache/mikebom/fingerprints/` with a per-source subdirectory and a 24-hour TTL. All cryptographic verification reuses the milestone-089/108 sigstore stack; no new production crates.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–109; no nightly required for this user-space-only work).

**Primary Dependencies** (existing only — **no new production crates**):
- `reqwest = "0.12"` (workspace; `rustls-tls` + `blocking`) — corpus archive HTTP fetch with optional `Authorization` header, milestone-108 reuse.
- `tar = "0.4"` + `flate2` (workspace) — archive extraction, milestone-108 reuse.
- `serde` / `serde_json` (workspace) — v2 record (de)serialization + JSON-LD round-tripping.
- `sigstore = "0.11"` (`mikebom-cli` direct dep, milestone-089) — cosign keyless OIDC verification of archive signatures.
- `sha2` + `data-encoding` (workspace) — cache directory keying on pinned content SHA.
- `tracing` (info/warn logs for fetch + matcher), `anyhow` (CLI error propagation), `thiserror` (matcher error enum).
- `clap` (workspace) — new flags for source configuration via derive; `Vec<String>` for repeatable `--fingerprints-source URL[:credential-env-var-name]` syntax.
- `jsonschema = "0.46"` — existing dev-dep already validates SPDX 2.3 + 3 schemas; **decision** (research item R5): keep validation dev-only via fixture tests rather than gating production on per-archive schema validation; production code uses `serde_json::Deserializer` with the v2 struct's `#[serde(deny_unknown_fields)]` for strict-shape rejection at deserialization time.

**Optional new dev-dep** (research item R7): `wiremock = "0.6"` for hermetic HTTP fixture tests of the auth-fetch path. Alternative: hand-rolled `tokio::net::TcpListener` stub (same pattern as milestone 055's go-mod-proxy stub). Decision deferred to T002 — pick the lighter alternative if the test surface is small enough.

**Storage**: Per-host cache at `~/.cache/mikebom/fingerprints/<source-id>/<pinned-sha>/` where `<source-id>` is a stable hash of the source URL (so multiple sources coexist without filename collisions) and `<pinned-sha>` is the archive's content SHA (matching the milestone-090 + milestone-108 pattern). The 24-hour TTL is implemented as a `last_used.touch` sidecar file whose mtime is checked at scan startup; expiry triggers re-fetch but does NOT delete the cache directory (re-fetch may write the same SHA back, in which case the existing dir is reused).

**Testing**: `cargo +stable test --workspace` (workspace-level integration tests in `mikebom-cli/tests/`); the `transitive_parity_common` helper from milestone 083 + the milestone-090 fixture-repo pattern. New fixture corpora live under `mikebom-cli/tests/fixtures/fingerprints_v2/` and are vendored verbatim (no per-test ingestion). Hermetic auth-fetch tests use the chosen R7 stub.

**Target Platform**: Linux/macOS/Windows. Cache directory uses `dirs::cache_dir()` (workspace dep) consistent with milestone-090 + milestone-100 portability work. No platform-specific code paths.

**Project Type**: cli (extension to `mikebom-cli`). No new crates.

**Performance Goals**:
- SC-003: < 30 seconds end-to-end fetch+verify+cache+scan on archives ≤ 5 MB.
- SC-004: 0 network I/O on second scan within 24 h TTL (verified by network-absence assertion in integration tests).
- Matcher: bounded by O(records × binaries × indicators-per-record) — at ~50 records × ~5 indicators/record × ~10 binaries/typical-scan, well under the existing milestone-099 matcher's runtime envelope.

**Constraints**:
- Pre-PR gate (constitution `Pre-PR Verification`): both `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace` must pass clean.
- Zero `.unwrap()` in production code (constitution principle IV); `mikebom-cli` crate-root deny applies.
- v1 backward compat: existing milestone-108 records continue to load and emit the same `pkg:generic/<name>` components — the OSS-regression CI lane (FR-019, SC-002) enforces this with a re-anchored byte-identity golden.
- Standards-native annotation precedence (constitution principle V, fifth bullet): every proposed `mikebom:*` annotation MUST first audit CDX 1.6 + SPDX 2.3 + SPDX 3.0.1 for a native equivalent. Audit recorded in research.md.

**Scale/Scope**:
- Spec scopes ≥ 10 libraries × ≥ 2 versions = ≥ 20 records for the initial seed under FR-004.
- Matcher should perform within budget on archives up to ~100 records (no lazy-fetch sharding required at this scale per spec § Assumptions).
- v2 schema extension points (additional indicator types, additional alias ecosystems) are forward-compatible at the JSON Schema level so a v2.1 record can ride the same matcher with new indicator kinds gracefully ignored (`additionalProperties: false` is OFF for the `indicators` map; ON elsewhere).

## Constitution Check

*GATE: passes Phase 0 research. Re-verified post-Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | **PASS** | No C code; no new C-linked deps; all proposed crates are existing workspace deps. |
| II. eBPF-Only Observation | **PASS (N/A)** | This milestone operates in the user-space file-walking scan path (no eBPF). The binary files themselves are observed via filesystem walk; the corpus provides ENRICHMENT (per Principle XII) of those already-observed binaries, never INTRODUCES phantom components. The matcher only attributes an identity to a binary that was filesystem-observed; if no binary matches a record, no component is emitted from the record. |
| III. Fail Closed | **PASS** | Discovery (the binary itself) does not fail-open. Per Principle XII constraint #3, enrichment unavailability MUST degrade gracefully with transparency annotations — exactly what FR-010 / FR-011 specify (when no corpus loads, components fall back to the pre-milestone-108 file-SHA-256 baseline; the scan exits successfully). This is consistent with Principle III's intent (don't silently fabricate dependencies) because no fabrication occurs: the binary's file-SHA-256 component is still observed and emitted. |
| IV. Type-Driven Correctness | **PASS (with discipline)** | Existing `Purl`, `Sha256`, etc. newtypes apply. New types introduced: `CorpusRecordV2`, `CorpusSourceId`, `IndicatorKind` (enum), `FusedConfidence` (enum: `High`, `Medium`), `MatchEvidence`. No raw `String` boundaries for PURLs or hashes. `unwrap()` forbidden in production per the `mikebom-cli` crate-root `clippy::unwrap_used` deny. |
| V. Specification Compliance | **PASS (after audit)** | Critical: per the fifth bullet (standards-native precedence), every proposed `mikebom:*` annotation MUST audit CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 for a native equivalent. The audit is research item R1 and lands in research.md; if any of `mikebom:identification-confidence` / `mikebom:indicators-matched` / `mikebom:purl-aliases` / `mikebom:also-detected-via` has a native equivalent, the spec's emission moves to the native field and the audit clause is recorded in `docs/reference/sbom-format-mapping.md`. |
| VI. Three-Crate Architecture | **PASS** | All changes land in `mikebom-cli`. No new crates. |
| VII. Test Isolation | **PASS** | No eBPF involvement. New tests run unprivileged. Auth-fetch hermetic tests live in `mikebom-cli/tests/` under the existing integration-test convention. |
| VIII. Completeness | **PASS** | Matcher MUST process every indicator declared in every loaded record against every binary; minimize false negatives. Records skipped due to per-record validation failure surface as warnings (spec edge-case "Single malformed record"). |
| IX. Accuracy | **PASS** | The 2026-06-03 clarification (suppress emission below `medium`) directly enforces accuracy by preventing low-confidence claims from polluting the SBOM. Multi-record collision handling (FR-014) emits both candidates with cross-references rather than silently picking a winner that could be wrong. |
| X. Transparency | **PASS** | Every fingerprint-derived component carries `confidence` (high/medium), `indicators-matched`, and `also-detected-via` (when applicable) so consumers can assess the identification's authority. Provenance on every record (`extracted_from`) satisfies the principle's annotation requirement. |
| XI. Enrichment | **PASS** | The corpus IS the enrichment source. Native CDX `evidence` constructs preferred where available per the principle V audit; `mikebom:*` properties are the parity-bridge fallback. |
| XII. External Data Source Enrichment | **PASS** | Constraint #1 (no new components from external sources) is honored — components are emitted only when a binary the filesystem-walker already observed matches the corpus. Constraint #2 (provenance annotation) is honored via `extracted_from` per record + `confidence` / `indicators-matched` per emitted component. Constraint #3 (graceful degradation) is FR-010 / FR-011. Constraint #4 (trace remains authoritative) — N/A here since this scan mode is filesystem-observed, not eBPF-traced, but the principle applies identically: filesystem observation remains authoritative, enrichment never introduces phantom binaries. |

**Strict Boundaries**:
1. No lockfile-based discovery — PASS (corpus is not a manifest; components only emit when a filesystem-observed binary matches).
2. No MITM proxy — PASS (no network observation; archives fetched via standard HTTPS).
3. No C code — PASS.
4. No `.unwrap()` in production — PASS (enforced by clippy at crate root).

**No constitution-check violations**. No Complexity Tracking entries required.

## Project Structure

### Documentation (this feature)

```text
specs/110-pluggable-corpus-v2/
├── plan.md              # This file
├── spec.md              # Feature specification (clarified)
├── research.md          # Phase 0 output — 10 research items resolved
├── data-model.md        # Phase 1 output — v2 record schema + matcher types
├── quickstart.md        # Phase 1 output — operator workflow + CI lane setup
├── contracts/
│   ├── corpus-record-v2.schema.json       # Public JSON Schema for v2 records (FR-001, FR-004)
│   ├── fetch-protocol-v2.md               # URL format + Authorization header + signature scheme (FR-020)
│   ├── matcher-api.md                     # Matcher trait surface (Rust API contract within mikebom-cli)
│   └── cli-flags.md                       # New `--fingerprints-source` syntax + env-var convention (FR-006, FR-007)
└── checklists/
    └── requirements.md  # Quality checklist (already created during /speckit.specify)
```

### Source Code (`mikebom-cli` crate; no new crates per constitution principle VI)

```text
mikebom-cli/
├── Cargo.toml                              # No production-dep changes; optional `wiremock` dev-dep per R7
├── src/
│   ├── scan_fs/
│   │   └── binary/
│   │       ├── fingerprints/               # EXTENDED — current milestone-108 module
│   │       │   ├── mod.rs                  # Top-level: extend `Corpus` to hold multi-source records
│   │       │   ├── cache.rs                # EXTENDED — multi-source cache layout + 24h TTL
│   │       │   ├── fetch.rs                # EXTENDED — multi-source fetch + Authorization header
│   │       │   ├── loader.rs               # EXTENDED — v1+v2 record loader with compat path (FR-005)
│   │       │   ├── record.rs               # EXTENDED — v2 record shape + indicator enum (FR-001, FR-002)
│   │       │   ├── source_sha.rs           # EXTENDED — per-source SHA tracking
│   │       │   ├── matcher.rs              # NEW — multi-indicator fusion + collision handling (FR-013–17)
│   │       │   ├── confidence.rs           # NEW — fusion rule + buckets (FR-017)
│   │       │   ├── self_identity.rs        # NEW — self-suppression resolver (FR-015)
│   │       │   ├── source_config.rs        # NEW — corpus source URL + credential parsing
│   │       │   ├── annotations.rs          # NEW — native CDX/SPDX construction per R1 audit
│   │       │   └── tests/                  # NEW — unit-test submodule (per-file in source tree)
│   │       └── symbol_fingerprint.rs       # UNCHANGED — extractor; matcher consumes its output via new API
│   ├── cli/
│   │   └── scan_cmd.rs                     # EXTENDED — new flags: --fingerprints-source (repeatable), --fingerprints-corpus-cache-bypass
│   └── ...                                 # All other files UNCHANGED
└── tests/
    ├── fingerprints_v2_match.rs            # NEW — versioned PURL emission (US1)
    ├── fingerprints_v2_pluggable.rs        # NEW — multi-source + auth + fallback (US2)
    ├── fingerprints_v1_regression.rs       # NEW — OSS continuity (US3, SC-002)
    ├── fingerprints_v2_fusion.rs           # NEW — multi-indicator confidence (US4)
    └── fixtures/
        └── fingerprints_v2/                # NEW — vendored test fixtures
            ├── archives/                   # Pre-built test corpus archives (sigstore-signed)
            │   ├── public-v1.tar.gz        # v1-shape archive mirroring milestone-108 records
            │   ├── private-v2.tar.gz       # v2-shape archive (auth-protected in test harness)
            │   └── conflicting-v2.tar.gz   # Two records claiming same library; exercises FR-014
            └── binaries/                   # Fixture binaries with known indicator signatures
                ├── libopenssl-3.1.4.so.fixture
                ├── libzlib-1.3.1.so.fixture
                ├── libboringssl.so.fixture # Collides with openssl exported symbols
                └── self-identity-cmake/    # Source tree where matcher should self-suppress
```

**Structure Decision**: Single-crate extension to `mikebom-cli` under the existing `scan_fs/binary/fingerprints/` module. No new crates per constitution principle VI. The matcher logic is isolated in new files (`matcher.rs`, `confidence.rs`, `self_identity.rs`, `annotations.rs`) so the existing milestone-108 surface (`cache.rs`, `fetch.rs`, `loader.rs`, `record.rs`, `source_sha.rs`) stays close to its current shape while being extended for multi-source + multi-indicator handling. Test fixtures live in `mikebom-cli/tests/fixtures/fingerprints_v2/` and are vendored verbatim — no per-test ingestion, no network at test time.

## Complexity Tracking

*No constitution-check violations — table omitted per template.*
