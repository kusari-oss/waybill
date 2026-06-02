# Implementation Plan: External symbol-fingerprint corpus

**Branch**: `108-fingerprint-corpus` | **Date**: 2026-06-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/108-fingerprint-corpus/spec.md`

## Summary

Move the hand-curated `FINGERPRINTS` const out of `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs` into a new sibling repo `kusari-sandbox/mikebom-fingerprints`. mikebom-cli pulls a SHA-pinned snapshot at scan time (build-time-embedded SHA is the default; `--fingerprints-rev <sha>` is the runtime override), caches per-host at `~/.cache/mikebom/fingerprints/<sha>/`, and stamps the corpus SHA as a `mikebom:fingerprint-corpus-sha` annotation on every fingerprint-derived component.

Six concrete deliverables:

1. **New sibling repo** `kusari-sandbox/mikebom-fingerprints` (public, Apache-2.0). Seeded with the existing 7 libraries from `symbol_fingerprint.rs::FINGERPRINTS` converted to `corpus/<library>.json` + `corpus/index.json`. Repo carries its own CI for schema validation + per-record `min_symbols` floor check.
2. **Build-time SHA embed** in `mikebom-cli`. A new constant `FINGERPRINTS_CORPUS_SHA` is set from `mikebom-cli/Cargo.toml`'s `[package.metadata.fingerprints]` section, exposed at compile time as an `env!()` value via `build.rs`.
3. **External-corpus loader** in `mikebom-cli/src/scan_fs/binary/fingerprints/`. Cache-first read from `~/.cache/mikebom/fingerprints/<sha>/`; cache-miss fetch via GitHub's archive-download API using the existing workspace `reqwest` + `tar` + `flate2`. Fallback to the bundled in-source `FINGERPRINTS` when external corpus disabled, network unreachable, or cache empty under `--offline`.
4. **CLI flags + subcommands**: `--fingerprints-corpus` (opt-in flag on `sbom scan`), `--fingerprints-rev <sha>` (runtime override), `mikebom fingerprints fetch [--corpus-rev <sha>]` (air-gapped pre-fetch), `mikebom fingerprints cache-clear [--keep-rev <sha>]` (cleanup).
5. **SBOM annotation** wiring. Every fingerprint-matched component carries `mikebom:fingerprint-corpus-sha = <12-hex>` (or the `bundled` sentinel when bundled fallback fired). Threads through the existing `extra_annotations` mechanism — no new emission paths.
6. **Bundled-defaults preservation**. The in-source `FINGERPRINTS` const stays as the fallback corpus. The 7 libraries match exactly what the seeded sibling repo ships on day 1; existing 33 byte-identity goldens pass byte-identically when the external corpus is OFF (the SC-003 no-regression contract).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–107; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — no new Cargo additions. `reqwest = "0.12"` (workspace; `rustls-tls` + `blocking` features already enabled; used for the corpus tarball fetch), `tar = "0.4"` (workspace; reused from milestone-002 layer extraction), `flate2` (workspace; gzip), `serde`/`serde_json` (corpus record (de)serialization), `sha2` + `data-encoding` (cache directory key validation), `tracing`, `anyhow`, `thiserror`, `clap` (the new flags via derive). Build-time embed of the corpus SHA uses `env!()` driven by a `build.rs`-set env var sourced from a per-crate `Cargo.toml` `[package.metadata.fingerprints]` entry — no new build dependencies.
**Storage**: Per-host cache at `~/.cache/mikebom/fingerprints/<sha>/` (same pattern as milestone 090's `~/.cache/mikebom/fixtures/<sha>/`). On-disk layout mirrors the sibling repo's `corpus/` directory verbatim for transparency + Docker-COPY-friendliness. No databases, no daemons.
**Testing**: Existing `cargo test --workspace` framework. New per-module unit tests (`fingerprints::{loader, cache, fetch}::tests`) + end-to-end integration tests at `mikebom-cli/tests/scan_fingerprint_corpus*.rs`. Network-touching tests are gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1` so the standard CI path stays offline.
**Target Platform**: Linux primary (where most operator scans happen); macOS + Windows compile + unit-test (the cache layout is platform-agnostic; the fetch path uses `reqwest::blocking` which works cross-platform via rustls-tls).
**Project Type**: cli — extends `mikebom-cli/src/scan_fs/binary/` with a new `fingerprints/` sub-module + adds the two `fingerprints fetch` / `fingerprints cache-clear` subcommands.
**Performance Goals**: SC-004 — initial corpus fetch ≤5s on a typical broadband connection; cache-hit adds <100ms over the bundled-corpus path (the cache load is a single JSON-parse + a per-binary HashSet lookup, same big-O as today's bundled match).
**Constraints**: Constitution XII opt-in (default OFF); FR-014 single-network-operation surface (the corpus fetch is the ONLY allowed network call; no deps.dev / CD lookups added by this milestone); FR-015 bundled-defaults preserved as fallback; cross-platform.
**Scale/Scope**: Day 1: 7 libraries (the existing in-source corpus, lifted verbatim into JSON). Target: 100+ libraries within 12 months of the sibling repo opening. Corpus tarball size at 7 libraries: ~5 KB; at 100 libraries: ~75 KB (well within the SC-004 5-second fetch budget).

All clarifications resolved in the spec's 2026-06-02 session — no `NEEDS CLARIFICATION` markers remain.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| **I. Pure Rust, Zero C** | ✅ Pass | All new code in Rust. No FFI, no `bindgen`, no C build scripts. |
| **II. eBPF-Only Observation** | ✅ Pass — established `scan_fs/binary/` extension | This milestone extends the binary analysis pathway (`scan_fs/binary/`), which is the established static-binary-analysis sister-mode to the eBPF trace. The boundary "no lockfile-based dependency discovery" applies to the trace pipeline; binary analysis has been the canonical fallback since milestone 004. |
| **III. Fail Closed** | ✅ Pass | When the external corpus is unreachable, mikebom warns + falls back to the bundled 7-library corpus + stamps the `bundled` sentinel. Component-identification is NOT a fail-closed concern (the binary still gets a `pkg:generic/<basename>?file-sha256=<hex>` placeholder when nothing matches); the corpus is purely additive identification. |
| **IV. Type-Driven Correctness** | ✅ Pass | New types: `FingerprintCorpus` (the in-memory collection), `FingerprintRecord` (per-library entry, validated at load time), `CorpusSha` (typed newtype with `Display` for 12-hex truncation), `CorpusSource` enum (`Bundled` / `Cached { sha }` / `Fetched { sha }`). No `.unwrap()` in production paths; test modules use the established `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard. |
| **V. Specification Compliance** | ✅ Pass | All emitted PURLs continue to flow through the existing `mikebom_common::types::purl::Purl` validation. The new `mikebom:fingerprint-corpus-sha` annotation is documented under the existing `mikebom:source-mechanism: "symbol-fingerprint"` family — parity-bridging annotation for "which corpus version produced this match"; no native CDX/SPDX field carries this. Added to `docs/reference/sbom-format-mapping.md`'s C-row catalog (next available row number; assigned in tasks.md). |
| **VI. Three-Crate Architecture** | ✅ Pass | All new code lands in `mikebom-cli`. No new crates in the workspace; the sibling repo is independent of the Cargo workspace. |
| **VII. Test Isolation** | ✅ Pass | All new tests are pure-Rust unit + integration — no `root` / `CAP_BPF` required. Network-touching tests gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1` so the standard CI lane (`cargo +stable test --workspace`) stays offline. |
| **VIII. Completeness** | ✅ Pass | Adding the external corpus monotonically increases identification completeness. The bundled fallback ensures no regression when the external corpus is disabled or unreachable. |
| **IX. Accuracy** | ✅ Pass | Per-record `min_symbols` floor (Q3 clarification) is the false-positive guard. Curators tune per library based on API stability. Two-record collisions emit BOTH components per FR-013 (no silent dedup); consumers see the duplicates and triage. |
| **X. Transparency** | ✅ Pass | `mikebom:fingerprint-corpus-sha` annotation on every match (FR-005). `bundled` sentinel when the fallback fired. `mikebom:source-mechanism: "symbol-fingerprint"` distinguishes heuristic identification from deterministic paths (BuildInfo / cargo-auditable / `.note.package`). |
| **XI. Enrichment** | ✅ Pass — corpus IS enrichment | The corpus enriches binary-walker-emitted components from `pkg:generic/<basename>?file-sha256=<hex>` placeholders into real library PURLs. Same enrichment posture as the existing deps.dev license / VCS lookups. |
| **XII. External Data Source Enrichment** | ✅ Pass — by design | The external corpus is opt-in (FR-002), cache-friendly (FR-003 + FR-004), source-attributed (FR-005 SHA annotation). Constitution XII's three rules are explicitly the design constraints. |

### Strict Boundaries audit

| Boundary | Status | Notes |
|---|---|---|
| **1. No lockfile-based dependency discovery** | ✅ Pass — `scan_fs/binary/` is binary-analysis-mode | The corpus is a per-library symbol-set + PURL claim, NOT a lockfile. Binary analysis has been the canonical sister-mode since milestone 004. |
| **2. No MITM proxy** | ✅ Pass | `reqwest` with `rustls-tls` (workspace default), direct HTTPS to GitHub. No intermediate proxy. |
| **3. No C code** | ✅ Pass — Rust-only, no new transitive C deps | The milestone reuses workspace `reqwest` + `tar` + `flate2` which are already in mikebom's dep closure. |
| **4. No `.unwrap()` in production** | ✅ Pass — test modules guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` | |

**Verdict**: No gate violations. No Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/108-fingerprint-corpus/
├── plan.md              # This file (/speckit.plan output)
├── spec.md              # /speckit.specify + /speckit.clarify output (clarifications resolved 2026-06-02)
├── research.md          # Phase 0 output (this command)
├── data-model.md        # Phase 1 output (this command)
├── quickstart.md        # Phase 1 output (this command)
├── contracts/           # Phase 1 output (this command) — per-component contracts
│   ├── corpus-schema.md
│   ├── cache-layout.md
│   ├── fetch-protocol.md
│   ├── cli-surface.md
│   └── sibling-repo-bootstrap.md
├── checklists/
│   └── requirements.md  # /speckit.specify validation checklist (all items pass)
└── tasks.md             # /speckit.tasks output (NOT created by /speckit.plan)
```

### Source code (repository root)

```text
mikebom-cli/
├── Cargo.toml                                  # MODIFIED: add [package.metadata.fingerprints] section with corpus_sha pin
├── build.rs                                    # MODIFIED: parse the metadata + emit MIKEBOM_FINGERPRINTS_CORPUS_SHA=<sha> env var
└── src/
    ├── cli/
    │   ├── mod.rs                              # MODIFIED: register the `fingerprints` subcommand group
    │   └── fingerprints_cmd.rs                 # NEW: `fingerprints fetch` + `fingerprints cache-clear`
    └── scan_fs/
        └── binary/
            ├── mod.rs                          # MODIFIED: route through the new corpus loader
            ├── symbol_fingerprint.rs           # MODIFIED: FINGERPRINTS const stays as bundled fallback; scan() consumes a CorpusSource
            └── fingerprints/
                ├── mod.rs                      # NEW: public surface (load_corpus, CorpusSource, etc.)
                ├── loader.rs                   # NEW: cache-first JSON loader
                ├── cache.rs                    # NEW: per-host cache layout management
                ├── fetch.rs                    # NEW: GitHub-archive tarball fetch + gunzip + tar-extract
                ├── record.rs                   # NEW: FingerprintRecord struct + schema validation
                └── source_sha.rs               # NEW: CorpusSha newtype + build-time-embedded SHA resolution

mikebom-cli/tests/
├── scan_fingerprint_corpus_bundled.rs          # NEW: bundled-fallback integration test (SC-003 no-regression)
├── scan_fingerprint_corpus_external.rs         # NEW: external-corpus integration test (gated MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1)
└── offline_mode_audit_ecosystem_108.rs         # NEW: SC-008 FR-014 audit — fingerprints/fetch.rs contains the ONLY allowed reqwest call

docs/
└── reference/
    └── sbom-format-mapping.md                  # MODIFIED: new C-row for `mikebom:fingerprint-corpus-sha`
```

### Sibling repository (NEW, separate from this Cargo workspace)

```text
kusari-sandbox/mikebom-fingerprints (new public Apache-2.0 repo)
├── README.md
├── CONTRIBUTING.md
├── LICENSE                                     # Apache-2.0
├── corpus/
│   ├── index.json                              # Aggregated index enumerating per-library files
│   ├── openssl.json                            # Seeded from in-source FINGERPRINTS[0]
│   ├── zlib.json                               # Seeded from FINGERPRINTS[1]
│   ├── libcurl.json                            # FINGERPRINTS[2]
│   ├── sqlite.json                             # FINGERPRINTS[3]
│   ├── pcre.json                               # FINGERPRINTS[4]
│   ├── pcre2.json                              # FINGERPRINTS[5]
│   └── gnutls.json                             # FINGERPRINTS[6]
├── schema/
│   └── fingerprint-record.v1.json              # JSON Schema for the per-library file
└── .github/workflows/validate-corpus.yml       # CI: schema + min_symbols floor + prefix-distinctiveness
```

**Structure Decision**: extend the existing `mikebom-cli/src/scan_fs/binary/` module collection. The new `fingerprints/` sub-module groups corpus loader, cache machinery, fetch logic, and record schema into a cohesive boundary — same pattern as `yocto/` from milestone 107. The bundled `FINGERPRINTS` const stays in `symbol_fingerprint.rs` for the fallback path; the new sub-module exposes a `CorpusSource` enum that `symbol_fingerprint::scan` consumes.

The sibling repo is standalone (no Rust code; just JSON + CI). Its CI runs schema validation; mikebom-cli does NOT re-validate at scan time (it trusts SHA-pinned snapshots per FR-010's "PRs to the sibling repo are the review point").

## Implementation phases (sub-PRs)

Six sub-PRs, mirroring the established multi-PR rhythm. Phase 1 (sibling-repo bootstrap) is separate from mikebom-cli and must merge first; Phases 2–5 are mikebom-cli PRs that build on each other; Phase 6 is the alpha.44 release cut.

| Phase | PR title (proposed) | Closes | Files touched |
|---|---|---|---|
| 1. Sibling-repo bootstrap | (no PR in mikebom workspace) | — | New repo `kusari-sandbox/mikebom-fingerprints` seeded with 7 libraries + CI + schema. Merges first; its initial commit's SHA is the seed `corpus_sha` pin in Phase 2. |
| 2. mikebom-cli foundation | `feat(fingerprints): add external corpus loader + cache + bundled fallback` | (new issue from #208) | `scan_fs/binary/fingerprints/*` (loader/cache/record/source_sha; fetch is a stub returning `Bundled`); `Cargo.toml` + `build.rs` metadata pin; MODIFIED `symbol_fingerprint.rs::scan` signature. Bundled-fallback path works end-to-end; external path is wired but stub. |
| 3. Network fetch + offline behavior | `feat(fingerprints): wire GitHub-archive fetch + cache miss handling` | (same issue) | NEW `fingerprints/fetch.rs`; `--offline` integration; first integration test with the network-gated env var. |
| 4. CLI subcommands + SBOM annotation | `feat(fingerprints): add fingerprints fetch/cache-clear subcommands + mikebom:fingerprint-corpus-sha annotation` | (same issue) | NEW `cli/fingerprints_cmd.rs`; MODIFIED `symbol_fingerprint.rs` to stamp the SHA annotation. |
| 5. Polish | `docs+test: milestone 108 polish — sbom-format-mapping C-row + FR-014 audit + SC-003 no-regression test` | (same issue) | `docs/reference/sbom-format-mapping.md`, `tests/offline_mode_audit_ecosystem_108.rs`, `tests/scan_fingerprint_corpus_bundled.rs`. |
| 6. Release | `release: bump workspace to v0.1.0-alpha.44 + regen 33 byte-identity goldens` | — | `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, 33 goldens (deltas expected to be version-bump-only — external corpus is OFF by default in golden fixtures). |

## Phase 0: Outline & Research

No `NEEDS CLARIFICATION` items remain in the spec — all three resolved in the 2026-06-02 clarification session. `research.md` captures the validation of the four design assumptions backing this plan:

- R1: GitHub archive-download API as the fetch transport (vs `git2` library or shell-out to `git`)
- R2: Cache-directory layout consistency with milestone 090's `mikebom-test-fixtures` precedent
- R3: Build-time `env!()` embed via `build.rs` + `Cargo.toml [package.metadata]` (vs runtime-config-file alternative)
- R4: JSON Schema validation in sibling-repo CI (vs mikebom-cli-side validation at scan time)

**Output**: `specs/108-fingerprint-corpus/research.md` (written by this command).

## Phase 1: Design & Contracts

Phase 1 artifacts:

- `data-model.md` — `FingerprintRecord`, `FingerprintCorpus`, `CorpusSha`, `CorpusSource`, `IndexEntry`, cache directory entity. Field-by-field schema for each.
- `contracts/corpus-schema.md` — JSON Schema for `corpus/<library>.json` and `corpus/index.json`. The contract the sibling-repo CI validates against.
- `contracts/cache-layout.md` — per-host cache directory shape, validation rules, atomic write semantics.
- `contracts/fetch-protocol.md` — GitHub archive-download API usage, error handling, retry behavior.
- `contracts/cli-surface.md` — exact flag + subcommand syntax (`--fingerprints-corpus`, `--fingerprints-rev`, `mikebom fingerprints fetch`, `mikebom fingerprints cache-clear`).
- `contracts/sibling-repo-bootstrap.md` — seeded content + CI shape for `kusari-sandbox/mikebom-fingerprints`.
- `quickstart.md` — operator-facing instructions for opt-in scans, air-gapped pre-fetch, hermetic-build pinning.

The Phase 1 artifacts are written by this `/speckit.plan` invocation.

## Complexity Tracking

*Empty — Constitution Check passes with no violations requiring justification.*
