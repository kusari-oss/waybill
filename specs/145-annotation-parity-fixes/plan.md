# Implementation Plan: Annotation-emission parity fixes from sbom-conformance audit (2026-06-26)

**Branch**: `145-annotation-parity-fixes` | **Date**: 2026-06-27 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/145-annotation-parity-fixes/spec.md`

## Summary

Three annotation-emission parity fixes surfaced by the 2026-06-26 sbom-conformance harness audit. ~3,424 combined CFI findings, distributed:

| Story | Issue | Surface | Expected reduction |
|---|---|---|---|
| US1 (P1) | `mikebom:file-paths` emits a stringified array (`Value::String("[\"path\"]")`) instead of native array | `mikebom-cli/src/scan_fs/file_tier/mod.rs:232-234` | 3,112 findings |
| US2 (P1) | `mikebom:lifecycle-scope` missing from SPDX 3 output for non-Runtime scopes | `mikebom-cli/src/generate/spdx/v3_annotations.rs` (add an emission branch mirroring `annotations.rs:227-236`) | 261 findings |
| US3 (P2) | `mikebom:source-files` value drift between CDX and SPDX 3 on Maven deps in image scans | Double-emission: field-derived path AND Maven-reader-stamped `extra_annotations` key (`maven.rs:2244`); fixture-reproduction needed to pin the per-emitter dedup difference | 51 findings |

US1 is a one-line fix in the file-tier component constructor; US2 is an additive emission path that already exists in the SPDX 2.3 sibling; US3 needs a fixture-reproduction step to confirm the exact dedup-order difference between CDX and SPDX 3 emitters. Phase 0 research §C below has substantially narrowed US3's diagnosis (down to "double-emission of the same key, different winners per emitter") — the implement phase will run the polyglot-builder-image fixture to confirm and apply the fix.

Total code surface estimate: ~10 LOC reader/emitter changes + ~150-200 LOC test additions + golden refresh for file-paths shape change. No new Cargo dependencies. The architectural lesson — `mikebom:source-files` is stamped in BOTH `extra_annotations` AND `evidence.source_file_paths`, with no dedup-key guard in the emitter — is the underlying cause of US3 and warrants a small invariant check (or doc-comment) at the call sites so future readers don't recreate the same trap.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–144; no nightly required).
**Primary Dependencies**: Existing only — `serde`/`serde_json` (Value construction + emission), `tracing`, `anyhow`, `thiserror`. Reuses milestone-133's `file_tier` module, milestone-005-era `evidence.source_file_paths` field, milestone-049/052's `LifecycleScope` newtype, milestone-071's parity catalog (C18 source-files, C42 lifecycle-scope, C92 file-paths — all `Directionality::SymmetricEqual`). **No new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; no caches, no persistence (matches every milestone since 002).
**Testing**: `cargo +stable test --workspace`. New tests are in-file `#[cfg(test)] mod tests` plus the existing parity-catalog regression test infrastructure (`mikebom-cli/tests/`). For US3, the implement phase uses the existing `polyglot-builder-image` fixture (or a stripped-down equivalent) to reproduce the drift; no new fixtures committed.
**Target Platform**: Linux x86_64 + macOS arm64 + Windows (matches CI lanes; the file-paths/lifecycle-scope/source-files changes are pure emission code, no platform-specific behavior).
**Project Type**: CLI / library — touches one crate (`mikebom-cli`); no kernel-space changes; no workspace topology change.
**Performance Goals**: No new performance budget. US1's fix removes one `serde_json::to_string` round-trip per file-tier component (slight improvement). US2's emission adds one annotation per non-Runtime-scoped component on SPDX 3 output (negligible). US3's fix may add a dedup check per component on emit (negligible).
**Constraints**:
- **Constitution V (standards-native > `mikebom:*`)** — no new `mikebom:*` annotations introduced; the milestone fixes how three EXISTING annotations are emitted. Compliance audit complete in spec FR-011.
- **Constitution IV (Type-Driven Correctness)** — `Value::String` vs `Value::Array` distinction is a textbook example of why typed wrappers matter; US1's fix is precisely about respecting the type distinction. No new `.unwrap()` in production paths.
- **Constitution X (Transparency)** — US3's fix preserves both source-of-truth signals (field-derived AND Maven-stamped) at the component level; only the emitter-side dedup behavior changes.
- **No subprocess calls** — all three fixes are pure-Rust emitter changes.
- **Pre-PR gate** — `./scripts/pre-pr.sh` (clippy `-D warnings` + `cargo test --workspace`) MUST exit 0 before PR open. The pre-existing `sbomqs_parity` env-only failure documented in milestone 144 T001 still applies; CI on identical commits will validate.
**Scale/Scope**: ~3,424 findings expected to clear (3,112 + 261 + 51). The largest single change is the file-paths shape — affects every file-tier component on every image-fixture scan. Local file-tier scans (e.g., `mikebom sbom scan --path .` on a non-image source tree with file-tier emission enabled) similarly benefit; the harness-finding count is dominated by image scans.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ PASS | No new C dependencies; no new Cargo deps at all. |
| II | eBPF-Only Observation | ✅ N/A | Emission-layer changes only; eBPF discovery untouched. |
| III | Fail Closed | ✅ PASS | No change to scan-failure semantics. |
| IV | Type-Driven Correctness | ✅ IMPROVES | US1 fixes a `Value::String`/`Value::Array` mismatch — exactly the kind of distinction Principle IV exists to protect. |
| V | Specification Compliance | ✅ PASS | **All three target annotations are EXISTING `mikebom:*` properties** (introduced at milestones 133 / 049-052 / 005-era respectively, with their own Principle-V audits at time of introduction). This milestone fixes their emission, NOT their existence — no new properties introduced. Spec FR-011 records the audit. |
| VI | Three-Crate Architecture | ✅ PASS | All changes in `mikebom-cli`. No new crates. |
| VII | Test Isolation | ✅ PASS | All new tests are pure-logic unit/integration tests; no eBPF privilege requirements. |
| VIII | Completeness | ✅ NEUTRAL | No change to component-discovery or component-inclusion. |
| IX | Accuracy | ✅ IMPROVES | Three SBOM-shape inaccuracies fixed (1 value-shape, 1 missing emission, 1 per-emitter drift). |
| X | Transparency | ✅ PASS | All three annotations retain their structured-property channel; only how they're encoded changes. The `mikebom:file-paths` shape change is observable to consumers (intentional — the wire shape becomes self-describing as an array instead of a stringified array). |
| XI | Enrichment | ✅ N/A | No enrichment-source changes. |
| XII | External Data Source Enrichment | ✅ N/A | No external source involvement. |
| SB-1 | No lockfile-based discovery | ✅ N/A | |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ PASS | Existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention preserved on new tests. |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | US1 changes the VALUE shape of a file-tier annotation, not the dedup invariant. |

**All gates pass. No Complexity Tracking entries required.**

## Project Structure

### Documentation (this feature)

```text
specs/145-annotation-parity-fixes/
├── plan.md              # This file
├── research.md          # Phase 0 output (US3 diagnosis + design decisions)
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── file-paths-shape.md          # Wire-format contract for mikebom:file-paths
│   ├── lifecycle-scope-emission.md  # SPDX 3 emission contract for mikebom:lifecycle-scope
│   └── source-files-dedup.md        # Dedup invariant for double-emission of mikebom:source-files
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify
```

### Source Code (repository root)

Touched files (all in `mikebom-cli`):

```text
mikebom-cli/
├── Cargo.toml                                              # No change (zero new deps)
├── src/
│   ├── scan_fs/
│   │   ├── file_tier/mod.rs                                # PRIMARY (US1) — line 233 value-shape fix; line 405 unit-test update
│   │   └── package_db/maven.rs                             # US3 — may need to STOP stamping mikebom:source-files into extra_annotations (or align with field-derived shape)
│   └── generate/
│       └── spdx/
│           ├── v3_annotations.rs                           # US2 — add lifecycle-scope emission branch mirroring annotations.rs:227-236
│           ├── annotations.rs                              # READ-ONLY reference for US2's mirror
│           └── (potentially) packages.rs / v3_packages.rs   # US3 — may need dedup-key check at the per-emitter source-files emission site
└── tests/
    ├── (existing parity-catalog tests under tests/)        # Will pick up the C18 / C42 / C92 row corrections automatically
    └── fixtures/golden/                                    # Refresh affected goldens (file-tier components in image-scan goldens; SPDX 3 goldens for lifecycle-scope components)
```

**Structure Decision**: Single existing crate (`mikebom-cli`). No new modules. US1 is one-line + one-test-update. US2 is one-new-branch-in-existing-function. US3 needs fixture-reproduction (Phase 0 §C documented the substantive narrowing; implement phase will reproduce and patch).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

Not applicable — all gates pass.
