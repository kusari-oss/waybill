# Implementation Plan: SPDX 3 duplicate-Annotation-spdxId dedup fix

**Branch**: `166-spdx3-annotation-dedup` | **Date**: 2026-07-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/166-spdx3-annotation-dedup/spec.md`

## Summary

Fix the SPDX 3 annotation merge point at `mikebom-cli/src/generate/spdx/v3_document.rs:754-820` where 4 annotation builders' outputs are merged into `@graph[]` sorted by `spdxId` but WITHOUT deduplication. When two builder call paths produce elements with the same `hash(subject_iri | field)` — as milestone-165's audit found on Kubernetes (`anno-GJJZ6XAC7UZOZO57` = `mikebom:graph-completeness=partial` emitted twice) and ArgoCD (`anno-YNFF6NBSSKSMJZF2` same pattern) — both land in `@graph[]` with identical `spdxId`, violating SPDX 3.0.1's `Annotation.statement` max-1-per-subject cardinality. Result: whole document fails `spdx3-validate` validation on 0.04% of annotations.

**Fix approach**: dedup by `spdxId` via a `BTreeMap<String, Value>` at merge time, keyed by the annotation's `spdxId` string. LAST-writer-wins (matches Rust's `BTreeMap::insert` returning `Some(previous)` semantics — natural pattern, easy to log the dropped duplicate). Add FR-007 tracing log emitting `spdx3_annotation_duplicates_dropped=<N>` per emission. Zero other changes.

**Empirical target**: post-166 `spdx3-validate` returns exit code 0 on both Kubernetes + ArgoCD SBOMs (regenerated at same commit SHAs as milestone-165 audit). All milestone-090 fixture SPDX 3 goldens continue passing conformance CI gate.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–165; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `serde_json` (already used for SPDX 3 emission), `std::collections::BTreeMap` (stdlib), `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan.
**Testing**: 5+ unit tests (SC-008), 1 integration test (SC-009), reuse existing `spdx3_conformance.rs` (m078) + `spdx3_annotation_fidelity.rs` (m145) as regression guards.
**Target Platform**: All mikebom-supported hosts (Linux, macOS, Windows).
**Project Type**: Rust CLI (mikebom-cli) — single-crate scope; two files touched: `v3_document.rs` (merge point) + optional helper in `v3_annotations.rs` (dedup utility).
**Performance Goals**: Zero measurable overhead. Dedup is a single pass over ~5000 annotations per scan (podman-desktop scale) with O(N log N) BTreeMap operations. Sub-millisecond.
**Constraints**: Constitution Principle IX (Accuracy — emitted documents must satisfy their own schema); Principle X (Transparency — FR-007 log surfaces potential redundant-emitter bugs); SC-005 dual-side byte-identity for CDX + SPDX 2.3.
**Scale/Scope**: podman-desktop = 2748 components + 4477 annotations pre-166. Kubernetes = 4477 annotations with 2 duplicates. ArgoCD = fewer annotations with 1 duplicate. Milestone-090 fixtures = smaller scale, likely 0 duplicates per fixture (empirical — TBD in Phase 3).

## Constitution Check

**GATE**: Pass before Phase 0 research. Re-check after Phase 1 design.

Constitution v1.5.0 principles evaluated against milestone 166's scope:

- **I. Pure Rust, Zero C**: PASS — Rust stable only, no new crates, no FFI.
- **II. Deterministic Scan Output**: PASS — same input produces same output. Dedup is deterministic single-pass with fixed builder ordering. LAST-writer-wins is stable given the deterministic ordering already established (m017 T013b + `sort_by_spdx_id` post-processing).
- **III. Attestation-First**: N/A — no attestation code touched.
- **IV. No `.unwrap()` in Production**: PASS — dedup helper uses `BTreeMap::insert` which returns `Option<Value>` (no unwrap needed); the `spdxId` extraction uses existing `.as_str().unwrap_or("")` pattern from `v3_document.rs:815`.
- **V. Specification Compliance (standards-native precedence)**: PASS — reuses existing SPDX 3 emission infrastructure. No new `mikebom:*` annotations. FR-010 explicitly documents this.
- **VI. Three-Crate Architecture**: PASS — only `mikebom-cli` touched.
- **VII. eBPF-Only Observation**: N/A — user-space code path.
- **VIII. Completeness — Never Silently Drop**: PARTIAL — dedup DROPS duplicate annotations, but this is CORRECT behavior (duplicates violate SPDX 3 spec). FR-007's tracing log surfaces the count so the drop is observable, not silent. Constitution intent (never silently drop DATA) is preserved — dropped duplicates carry no unique information.
- **IX. Accuracy — No Fake Versions**: PASS — the whole point of this milestone. Post-fix, emitted SPDX 3 documents validate clean against `spdx3-validate`.
- **X. Transparency — Explicit Signals**: PASS — FR-007 tracing log at info level; matches milestone-157/158/159/160/161/162/163/164/165 observability convention.
- **XI. Every Scan Produces an SBOM**: PASS — no scan-termination path added; dedup is a post-processing pass.
- **XII. Ecosystem Coverage**: N/A — no ecosystem code touched.

**Strict Boundaries** (v1.5.0):

- §1 (deterministic PURL): N/A.
- §2 (workspace layout): PASS.
- §3 (constitution amendment process): N/A.
- §4 (single source of truth): PASS — `spdxId` is the single truth for annotation identity.
- §5 (no duplicate file-tier components): PASS — file-tier code path unchanged.

**Verdict**: All 12 principles + 5 boundaries clear. Principle VIII is nominally PARTIAL but the "never silently drop" spirit is preserved via FR-007 observability — dropped duplicates are counted and logged, not hidden. Reviewers can trace behavior. No Complexity Tracking entries required.

## Project Structure

### Documentation (this feature)

```text
specs/166-spdx3-annotation-dedup/
├── plan.md              # This file
├── research.md          # Phase 0 output — dedup posture + BTreeMap vs HashMap decision
├── data-model.md        # Phase 1 output — dedup helper signature + call-site diff
├── quickstart.md        # Phase 1 output — how to reproduce the bug + verify the fix
├── contracts/
│   └── README.md        # Empty stub — no new external contracts
├── checklists/
│   └── requirements.md  # /speckit.specify output
└── tasks.md             # /speckit.tasks output (NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── generate/
│       └── spdx/
│           ├── v3_annotations.rs     # ← EDITED (T003): new dedup helper `dedup_annotations_by_spdx_id`
│           └── v3_document.rs        # ← EDITED (T004): call dedup helper at merge point + tally + FR-007 log
└── tests/
    └── spdx3_annotation_dedup.rs     # ← NEW (T012): SC-009 integration test — synthesized duplicate scenario
```

**Structure Decision**: Single-crate scope. Only `mikebom-cli` touched. Two files edited (dedup helper + call site), one new integration test file. No new modules, no restructuring. This is the smallest possible surface for a bug fix of this scope — matches milestone-087 cargo-fix + milestone-164 pnpm-v9 fix precedents.

## Complexity Tracking

No entries required. All Constitution gates pass without justification. This is a targeted single-symptom fix reusing existing SPDX 3 emission infrastructure.
