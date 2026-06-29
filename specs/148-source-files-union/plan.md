# Implementation Plan: source-files cross-emitter divergence — union evidence across same-PURL entries

**Branch**: `148-source-files-union` | **Date**: 2026-06-28 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/148-source-files-union/spec.md`

## Summary

Close the last cluster of cross-format `mikebom:source-files` value divergence on the polyglot-builder-image audit corpus (51 → 0). The Maven nested-JAR walker intentionally creates two `PackageDbEntry` instances for the same Maven coord when it appears both standalone (`parent_purl = None`) AND vendored inside a fat-jar (`parent_purl = Some(...)`). The deduplicator at `mikebom-cli/src/resolve/deduplicator.rs:34-46` correctly preserves both entries (CDX nested-components model requires it). The bug: each entry carries its own single-path `evidence.source_file_paths` Vec, so per-emitter iteration-order differences produce a harness-visible divergence on the otherwise-same PURL.

**Fix**: a post-dedup canonicalization pass that, for every PURL appearing on multiple `ResolvedComponent` instances, replaces each instance's `evidence.source_file_paths` Vec with the alphabetically-sorted **union** of paths observed across all same-PURL entries. The pass is keyed on `Purl::as_str()` (full canonical PURL string including ecosystem segment) per FR-003 — preserving cross-ecosystem isolation. It's idempotent (FR-004), preserves every other field (FR-005), and preserves the dep-graph topology that the deduplicator's `parent_purl` group-key intentionally retains (FR-006).

Phase 0 research (§A through §D below) confirmed three load-bearing assumptions:
1. **No existing post-dedup pass touches `evidence.source_file_paths`** — `synthesize_cpes` at line 754-756 only writes `c.cpes`; `maybe_suppress_scan_target_coord` at line 763-764 returns a new `scan_target_coord` value; `tag_main_modules_with_workspace_root` at line 773 only writes the `mikebom:is-workspace-root` extra_annotation. Order-of-operations is not load-bearing; the union pass can land anywhere after `deduplicate()` and before `ScanResult` construction.
2. **The deduplicator's within-group merge at lines 74-78 uses insertion-order `.contains()` semantics** — not alphabetically sorted. For PURLs that survive as a single entry post-dedup (the common case), the cross-PURL union is identity and the within-group merge result reaches emit unchanged. For PURLs with multi-entry survival (cross-`parent_purl` shape), the cross-PURL union OVERWRITES the within-group result with the alphabetically-sorted superset — both unions are doing the same logical operation (set union), the cross-PURL pass is just more inclusive.
3. **`Purl::as_str()` includes the ecosystem segment** (verified at `mikebom-common/src/types/purl.rs`), so cross-ecosystem isolation (Edge Case 7) is automatic.

Total code surface estimate: ~15 LOC for the new pass in `mikebom-cli/src/resolve/deduplicator.rs` (or a sibling `source_files_union.rs` module — research §B decision below), ~60 LOC of unit tests (SC-004 + SC-005 + a same-PURL-different-`parent_purl` regression test), ~80 LOC of in-tree integration test (SC-003: synthetic Maven fixture asserting cross-format `mikebom:source-files` invariance). Zero new Cargo dependencies. No changes to any per-format emitter, no changes to any ecosystem reader, no new annotation. One additive function call at `scan_fs/mod.rs:750-751`.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–147; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `std::collections::HashMap` + `std::collections::BTreeSet` (both pervasive in the codebase, no new use). The pass operates on `mikebom_common::resolution::ResolvedComponent` (already a workspace type). **No new Cargo dependencies.**
**Storage**: N/A — purely in-process per-scan transformation; no persistence.
**Testing**: `cargo +stable test --workspace`. New unit tests in `mikebom-cli/src/resolve/deduplicator.rs#mod tests` (or in a new `source_files_union.rs#mod tests` if research §B picks the sibling-module placement). New in-tree integration test at `mikebom-cli/tests/source_files_purl_union_md148.rs` covering the SC-003 cross-format byte-equality assertion on a synthetic Maven nested-coord fixture.
**Target Platform**: Linux x86_64 + macOS arm64 + Windows (CI lanes). Pure-Rust pure-data-transform; no platform-specific behavior.
**Project Type**: Library/post-processor — touches `mikebom-cli`'s post-dedup pipeline. Downstream consumers (CDX builder, SPDX 2.3 emitter, SPDX 3 emitter) consume the `ResolvedComponent.evidence.source_file_paths` field transparently with **zero call-site changes**.
**Performance Goals**: One-pass O(N) over the post-dedup `Vec<ResolvedComponent>` (N = component count, typically 100s–10Ks). Two HashMap+BTreeSet builds (one for path-collection, one for write-back). Cost is comparable to the existing `synthesize_cpes` loop at lines 754-756; no measurable perf impact on any realistic scan.
**Constraints**:
- **Constitution V (standards-native > `mikebom:*`)**: This milestone introduces NO new `mikebom:*` annotation (FR-008). The fix is a value-canonicalization operation on the in-process `evidence.source_file_paths` field that all three emitters already consume. The existing C18 parity-catalog row's audit is unchanged. Constitution V is satisfied vacuously.
- **Constitution IV (Type-Driven Correctness)**: The pass operates entirely on already-typed values (`Purl` newtype for keying, `String` for path values which is the existing field type). No new `.unwrap()` in production paths; tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per existing convention.
- **No subprocess calls**: pure-Rust HashMap + BTreeSet operations.
- **Pre-PR gate**: `./scripts/pre-pr.sh` (clippy `-D warnings` + `cargo test --workspace`) MUST exit 0 before PR open. The pre-existing local `sbomqs_parity` env-only failure documented in milestone 144 T001 still applies; CI on a clean runner validates.
**Scale/Scope**: 51 polyglot-builder-image Maven cases → 0 post-fix. Across the full mikebom test fixture set, the union pass is a no-op for the overwhelming majority of components (single-entry PURLs); only components with multi-entry shape (Maven nested-coords, theoretical Cargo workspace vendor-cases, theoretical Go vendor-cases) experience the canonical-union write-back.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ PASS | No new Cargo dependencies; uses stdlib `HashMap` + `BTreeSet`. |
| II | eBPF-Only Observation | ✅ N/A | Pure post-dedup canonicalization; eBPF discovery untouched. |
| III | Fail Closed | ✅ PASS | No change to scan-failure semantics; the union pass is an idempotent transformation on an existing Vec. |
| IV | Type-Driven Correctness | ✅ PASS | Operates on existing typed values; no new `.unwrap()` in production paths. |
| V | Specification Compliance | ✅ PASS (no new annotation) | Per FR-008. The fix changes VALUES of the existing `mikebom:source-files` annotation but introduces no new key. The existing C18 row's audit is unchanged. **Constitution V satisfied vacuously** — no annotation added → no audit needed. |
| VI | Three-Crate Architecture | ✅ PASS | All change in `mikebom-cli`. |
| VII | Test Isolation | ✅ PASS | New tests are pure-logic unit tests + an integration test that runs the binary against a synthetic fixture; no eBPF privilege requirements. |
| VIII | Completeness | ✅ IMPROVES | The cross-format `mikebom:source-files` divergence was effectively a transparency gap — auditors couldn't trust the field to be stable across formats. Closing the divergence improves the downstream completeness signal. |
| IX | Accuracy | ✅ IMPROVES | After the union pass, every consumer that reads `mikebom:source-files` for a given PURL gets the SAME set of observed paths regardless of format — more accurate than the pre-148 first-match-wins divergent behavior. |
| X | Transparency | ✅ PASS | The path superset is more informative than the per-entry single path; auditors get the complete observed-paths picture for any same-PURL multi-entry component. |
| XI | Enrichment | ✅ N/A | No enrichment-source changes. |
| XII | External Data Source Enrichment | ✅ N/A | No external sources consulted. |
| SB-1 | No lockfile-based discovery | ✅ N/A | The pass operates on already-discovered components. |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ PASS | New tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`. |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | File-tier components already aggregate paths via `file_tier/walker.rs::push_path`; the union pass is idempotent for them. |

**All gates pass.** Principle V is satisfied vacuously (no new annotation introduced); no Complexity Tracking entry needed.

## Project Structure

### Documentation (this feature)

```text
specs/148-source-files-union/
├── plan.md              # This file
├── research.md          # Phase 0 output (placement decision + idempotence-strategy + cross-ecosystem coverage audit + perf cost sketch)
├── data-model.md        # Phase 1 output (no new types — describes the existing ResolvedComponent.evidence.source_file_paths semantic + pre/post behavior table)
├── quickstart.md        # Phase 1 output (operator-facing verification — re-run polyglot-builder-image harness; expect 51 → 0)
├── contracts/
│   └── source-files-union.md   # Pure-function contract for the union pass
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify (all items ✅)
```

### Source Code (repository root)

Touched files (narrow scope):

```text
mikebom-cli/
├── (no Cargo.toml change)
└── src/
    ├── resolve/
    │   └── deduplicator.rs                      # OR a new sibling source_files_union.rs (research §B decision)
    └── scan_fs/
        └── mod.rs                               # ONE additive call at line ~751 invoking the union pass
mikebom-cli/
└── tests/
    ├── source_files_purl_union_md148.rs        # NEW — SC-003 in-tree integration test (synthetic Maven nested-coord fixture)
    └── fixtures/
        └── source_files_union/                  # NEW — synthetic fixture with one Maven coord appearing both standalone + nested
            ├── (minimal pom.xml + nested fat-jar synth structure)
            └── README.md                        # Fixture-shape documentation
mikebom-cli/
└── tests/
    └── fixtures/
        └── golden/                              # Maven-bearing CDX/SPDX 2.3/SPDX 3 goldens may refresh (existing maven fixture: pom-three-deps)
```

**Structure Decision**: Single-file change at `mikebom-cli/src/resolve/deduplicator.rs` (research §B picks the in-deduplicator-module placement over a sibling source_files_union.rs module — the pass is conceptually a post-dedup canonicalization step belonging to the deduplicator's domain). One additive call at `scan_fs/mod.rs:751` (immediately after the existing `let mut components = deduplicate(components);`). Synthetic test fixture under `mikebom-cli/tests/fixtures/source_files_union/`. Zero new modules, zero signature changes to public APIs, zero call-site changes elsewhere.

## Complexity Tracking

*Not applicable.* All Constitution gates pass cleanly. No new annotation introduced (Principle V satisfied vacuously per FR-008). No new dependencies. No structural deviation from existing post-dedup pass pattern.
