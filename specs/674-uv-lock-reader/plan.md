# Implementation Plan: m674 — uv.lock reader for the UV Python package manager

**Branch**: `674-uv-lock-reader` | **Date**: 2026-09-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/674-uv-lock-reader/spec.md`

## Summary

Add a new `uv/` package_db reader that parses Astral's `uv.lock`
TOML-format lockfiles and emits components with the same PURL +
hash + annotation shape as the m223 Pants PEX reader. Two ingest
paths: (a) `<scan_root>/uv.lock` at repo root (standalone uv-managed
projects — the fastest-growing Python packaging ecosystem in 2025),
and (b) opportunistic fallback parsing of `.lock` files that were
discovered by the m673 Pants pipeline but FAILED the m223 PEX-JSON
parse (recovers Pants monorepos using uv as the resolver backend —
observed empirically at `lablup/backend.ai` in the m673 sweep, where
9 uv-shape lockfiles today emit 0 components).

Zero new Cargo dependencies. Wire-format-compatible with m223 output
(same PURL rules, same annotations, same reconciler-tier signal) so
downstream extractors + parity catalog rows apply unchanged.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–673; no nightly required for this user-space-only reader).
**Primary Dependencies**: Existing only — `toml = "0.8"` (workspace dep, already used by cargo/pip/pants config parsers + m672 pants map + m673 unchanged), `serde` + `serde_derive` (workspace), `waybill_common::types::purl::Purl` + `encode_purl_segment` (workspace type — same PURL construction m223 uses), `waybill_common::types::hash::{ContentHash, HashAlgorithm}` (SHA-256 hash emission — same shape m138 composer / m140 elixir / m141 erlang readers use), `tracing`, `anyhow`, `thiserror`. Reuses `pip::normalize_pypi_name_for_purl` at `waybill-cli/src/scan_fs/package_db/pip/mod.rs:99` for cross-reader identity consistency (FR-015). **Zero new Cargo dependencies.** No subprocess calls. No network access.
**Storage**: N/A — all state is in-process for the duration of a single scan (matches every reader milestone since m002).
**Testing**: `cargo test -p waybill` — integration tests via synthetic `tempfile::tempdir()` fixtures + committed small deterministic uv.lock captures under `waybill-cli/tests/fixtures/uv_lock/` (per m223 committed-fixture pattern). Unit tests inline in `uv/lockfile.rs` for the per-source-variant PURL construction rules.
**Target Platform**: Linux + macOS + Windows (portability envelope unchanged — pure Rust, no OS-specific syscalls).
**Project Type**: New reader crate-submodule at `waybill-cli/src/scan_fs/package_db/uv/`. Matches m223 shape (single `uv/` module with `mod.rs` orchestrator + `lockfile.rs` parser + `source_variant.rs` for the 6-variant enum).
**Performance Goals**: ≤ 10 ms overhead on repos with no `uv.lock` (fast-path stat check); ≤ 100 ms per parsed lockfile on real shapes (per SC-006 + SC-007). `serde` deserialization from TOML scales linearly with file size; typical uv.lock ranges from ~5 KB (small deps) to ~500 KB (large transitive closures) — well within budget.
**Constraints**: SC-004 byte-identity gate — pre-m674 scans of repos with no uv.lock MUST produce byte-identical SBOMs to post-m674. m223 + m672 + m673 integration tests MUST pass unchanged.
**Scale/Scope**: Real-world observed shapes: `meilisearch/meilisearch-python` (53 packages, ~150 KB uv.lock), `lablup/backend.ai` (9 lockfiles × ~50-100 packages each = ~500-900 total pypi with transitive closure).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I — Pure Rust, Zero C

✅ **PASS**. Zero new Cargo dependencies. Deserialization uses existing `toml = "0.8"` + `serde` — both already in the dep graph.

### Principle II — eBPF-Only Observation

N/A. This milestone touches the `sbom scan` code path only. `waybill-ebpf` is untouched.

### Principle III — Fail Closed

✅ **PASS**. m223-style fail-open on per-file parse failure (WARN + skip; scan continues). Matches every other package_db reader's contract. New readers inherit that policy; not a new deviation.

### Principle IV — Type-Driven Correctness

✅ **PASS**. `UvSource` 6-variant enum drives per-variant PURL construction via exhaustive `match` — impossible to add a new source variant without touching every callsite. `UvLockfile` / `UvPackage` structs use `#[derive(Deserialize)]` with strict field types; shape drift fails at the parse boundary, not at emit time.

### Principle V — Specification Compliance

✅ **PASS**. Reader extension only; no CDX/SPDX/SPDX3 wire-format additions. Emitted components use the same PURL shape as m223 Pants + m670 pip — same standards-native fields, same annotations. New per-component annotation `waybill:python-lockfile-format=uv` (FR-011) is added to the parity catalog with `SymmetricEqual` directionality; extractors follow the m670 C154/C155 pattern.

### Principle VI — Three-Crate Architecture

✅ **PASS**. All changes stay inside `waybill-cli`. No `waybill-common` or `waybill-ebpf` touches.

### Principle VII — Test Isolation

✅ **PASS**. Every new integration test uses `tempfile::tempdir()` OR reads from committed small deterministic fixtures under `waybill-cli/tests/fixtures/uv_lock/` (per m223 committed-fixture pattern). Every synthetic package name uses the `waybill-fixture-*` prefix per memory `feedback_fixture_synthetic_package_names`.

### Principle VIII — Completeness

✅ **PASS**. This milestone exists to close two completeness gaps: (a) standalone uv-managed projects emit 0-usable-components today, (b) Pants-with-uv-backend repos emit 0 from the Pants reader (partial rescue via m670 pyproject.toml fallback only).

### Principle IX — Accuracy

✅ **PASS**. FR-015 requires the pypi PURL to match byte-for-byte what the pip reader emits (via shared `normalize_pypi_name_for_purl`). Prevents cross-format drift.

### Principle X — Transparency

✅ **PASS**. FR-012 mandates an INFO-level reader-complete log naming `lockfiles_discovered / parsed_ok / skipped_corrupt / components_emitted`. Matches m223 + m672 + m673 log conventions. FR-011 `waybill:python-lockfile-format` annotation makes the format-origin visible to downstream consumers.

### Principle XI — Enrichment

N/A. This milestone doesn't emit new enrichment; it recovers dropped detection.

### Principle XII — External Data Source Enrichment

N/A. Zero network access.

### Development Workflow / Pre-PR Verification

Standard: `cargo +stable clippy --workspace --all-targets` + `cargo +stable test --workspace` + `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh` all green.

### Strict Boundaries §5 (file-tier duplicates)

N/A. This milestone touches only the package-tier uv reader; no file-tier interaction.

**GATE VERDICT**: ✅ **PASS**. No documented nuances.

### Post-design re-check (2026-09-02, after Phase 1)

Reviewed data-model + contracts + quickstart against every principle. No new violations surfaced. Design choices tightened rather than widened the surface:

- **`UvSource` enum** as exhaustive discriminator: Principle IV strengthened.
- **Reuse of `pip::normalize_pypi_name_for_purl`**: Principle IX shared-identity guarantee.
- **New parity C-row for `waybill:python-lockfile-format`**: Principle V standards-through-parity coverage.
- **Pants FR-002 fallback via hook-into-m673 (research.md §R3)**: single source of truth for discovery; parser dispatch is the only new logic. Tightens Principle IV — cannot double-parse a file.

Final verdict: ✅ **PASS**. Ready for `/speckit.tasks`.

## Project Structure

### Documentation (this feature)

```text
specs/674-uv-lock-reader/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── uv_lockfile_schema.md
│   ├── source_variants.md
│   └── pants_integration.md
├── spec.md              # Feature specification (already done)
├── checklists/
│   └── requirements.md  # Spec quality checklist (already done)
└── tasks.md             # Phase 2 output (`/speckit.tasks` — not this command)
```

### Source Code (repository root)

New module at `waybill-cli/src/scan_fs/package_db/uv/` + 1 new integration test file + minor plumbing edits:

```text
waybill-cli/
├── src/scan_fs/package_db/
│   ├── uv/                              # NEW module
│   │   ├── mod.rs                       # ~80 lines — orchestrator + read() entry
│   │   ├── lockfile.rs                  # ~200 lines — schema deser + per-source PURL/annotation emission
│   │   └── source_variant.rs            # ~50 lines — UvSource enum + variant → PURL helpers
│   ├── mod.rs                           # +5 lines — register uv reader in `read_all` dispatcher
│   ├── pants/mod.rs                     # +10 lines — FR-002 fallback: forward m672-discovered files that failed PEX parse to the uv reader
│   └── pip/mod.rs                       # (unchanged — shared `normalize_pypi_name_for_purl` reused as-is)
├── src/parity/extractors/
│   ├── mod.rs                           # +3 lines — register C157 row
│   ├── cdx.rs                           # +2 lines — c157_cdx macro
│   ├── spdx2.rs                         # +2 lines — c157_spdx23 macro
│   └── spdx3.rs                         # +2 lines — c157_spdx3 macro
├── tests/
│   └── scan_uv_lock_m674.rs             # NEW — 8+ integration tests
├── tests/fixtures/uv_lock/              # NEW — small committed fixtures
│   ├── minimal_uv/                      # 3-package fixture for SC-001
│   ├── multi_source/                    # git + path + editable + registry mix
│   └── pants_uv_backend/                # simulates backend.ai shape
└── docs/reference/sbom-format-mapping.md  # +1 row — C157 waybill:python-lockfile-format
```

No `Cargo.toml` changes. No workflow YAML changes.

**Structure Decision**: New `uv/` module alongside `pants/`, `pip/`, `cargo/`, etc. Matches the existing per-ecosystem-reader convention. `mod.rs` orchestrates discovery + parse dispatch; `lockfile.rs` owns the schema types + parse + per-package emit; `source_variant.rs` isolates the 6-variant `UvSource` enum + per-variant PURL helpers so the emit logic in `lockfile.rs` stays clean. The Pants FR-002 fallback lives inside `pants/mod.rs` (invokes `uv::lockfile::parse` on m672-declared files that failed `pants::lockfile::parse`) — keeps the m673 discovery pipeline as the single source of truth for Pants layouts while letting the uv reader own the format parsing.

## Complexity Tracking

No violations to track. All constitutional principles pass without documented nuance. The one design choice worth calling out — Pants FR-002 fallback approach (hook-into-m673 vs. second-pass) — is settled in `research.md` §R3 in favor of hook-into-m673 (single-source-of-truth discovery + parser dispatch).
