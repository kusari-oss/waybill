# Implementation Plan: Emit `mikebom:image-extraction-completeness` Document-Scope Annotation

**Branch**: `204-helm-completeness-annotation` | **Date**: 2026-07-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/204-helm-completeness-annotation/spec.md`

## Summary

Wire the already-populated `ScanDiagnostics.helm_extraction_mode: Option<HelmExtractionMode>` through the emitter pipeline so all three formats (CDX 1.6, SPDX 2.3, SPDX 3) surface a document-scope `mikebom:image-extraction-completeness = "partial"|"full"` annotation. Emitter-only work — the reader-side plumbing landed in m188 (T023) and m203 wired the `Rendered` branch. m204 replicates the m161 `go_workspace_mode` C112 plumbing pattern verbatim: (1) add `helm_extraction_mode` field to `ScanResult` at `mikebom-cli/src/scan_fs/mod.rs:76`, (2) mirror it from `scan_result.diagnostics.helm_extraction_mode` at line 352, (3) destructure in `scan_cmd.rs:2515`, (4) thread through `ScanArtifacts` at `generate/mod.rs:51`, (5) emit in `cyclonedx/metadata.rs::build_metadata` alongside C112, (6) emit in `spdx/annotations.rs::annotate_document` alongside C112, (7) emit in `spdx/v3_annotations.rs` alongside C112, (8) register new catalog row **C123** (`SymmetricEqual`) at `parity/extractors/mod.rs`. **Zero new Cargo dependencies. ~200 LOC across ~7 files.**

Reconnaissance findings (per m199-m202 lesson):
- `HelmExtractionMode` enum + `ScanDiagnostics.helm_extraction_mode: Option<HelmExtractionMode>` verified at `scan_fs/package_db/mod.rs:420, 427`.
- `helm::read` sets `diagnostics.helm_extraction_mode = Some(extraction_mode)` verified at `scan_fs/package_db/helm.rs:464` (post-m203).
- `ScanResult` at `scan_fs/mod.rs:76` currently mirrors `go_transitive_coverage`, `go_transitive_fallback_count`, `go_cache_warming`, `go_workspace_mode`, `divergence_records` from diagnostics — the pattern is well-worn (5 sibling fields).
- `ScanArtifacts` at `generate/mod.rs:51` similarly threads all doc-scope signals as `Option<&…>` borrows.
- `build_metadata` in `cyclonedx/metadata.rs:45` is the ~20-arg one that C112 emits into at line 561; m204 adds one more arg AND updates ~7 test call sites (the `build_metadata("myapp", …)` invocations that literally enumerate every arg — seen at lines 1198, 1219, 1228, 1240, 1251, 1260, 1296).
- `annotate_document` in `spdx/annotations.rs:395` takes `&ScanArtifacts` — clean single-line new-field addition.
- `v3_annotations.rs` analogous; C112 emission at line 599.
- Parity catalog: next free row ID is **C123** (C122 is highest used per `grep row_id: "C1[0-9]+"`).
- `MikebomAnnotationCommentV1` envelope + `push()` helper in `annotations.rs` (m071) — reused verbatim.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–203; no nightly).
**Primary Dependencies**: Existing only — `serde`/`serde_json` (annotation values + m071 envelope construction), `tracing`, `anyhow`. **Zero new Cargo dependencies.** No subprocess calls. No network access. No filesystem writes beyond emitted SBOM output.
**Storage**: N/A — all state in-process per scan; the annotation flows through the emitter pipeline once per scan and is written into the emitted SBOM. Matches every emitter milestone since 002.
**Testing**: New integration tests appended to `mikebom-cli/tests/helm_reader.rs`. US1 (unrendered → `"partial"`) runs in default CI. US2 (rendered → `"full"`) gated behind `MIKEBOM_HELM_INTEGRATION=1` per m188/m203 precedent. US3 byte-identity gate: assert non-Helm scan goldens are unchanged (`git diff --stat mikebom-cli/tests/fixtures/` post-implementation shows drift only on helm goldens). Existing m071 CDX ↔ SPDX 2.3 ↔ SPDX 3 parity test suite catches three-way format equivalence automatically once C123 is registered.
**Target Platform**: Same as mikebom itself. No new host requirements.
**Project Type**: Reader-driven emitter plumbing. ~200 LOC total across 7 files: 1 struct-field addition (ScanResult), 1 struct-field addition (ScanArtifacts), 1 destructure update (scan_cmd.rs), 3 emitter branches (CDX + SPDX 2.3 + SPDX 3), 1 catalog row addition (~30 LOC across cdx.rs + spdx2.rs + spdx3.rs + mod.rs), 3-5 integration tests, plus ~7 build_metadata test-callsite fixups.
**Performance Goals**: No perf regression beyond SC-006 (`./scripts/pre-pr.sh` wall-clock delta ≤ 5s vs baseline). All new work is a single field lookup + one-off `push!()` per format — negligible.
**Constraints**: (a) zero new Cargo deps; (b) non-Helm-scan byte-identity per FR-004 / SC-004 (annotation gated on `Option::is_some`); (c) helm-scan golden regen ONLY on fixtures that already scan a helm chart (m188 helm reader tests); (d) parity via C123 (`SymmetricEqual`); (e) emitted value is a pure function of `HelmExtractionMode` variant per FR-005.
**Scale/Scope**: 7 source-file edits (one crate: mikebom-cli). No changes to mikebom-common. No changes to mikebom-ebpf. No changes to the helm reader itself (already sets diagnostics; m204 only consumes).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. All new Rust code. No subprocess calls, no C dependencies, no `unsafe`.
- **II. eBPF-Only Observation** — ✅ N/A. Emitter-side extension, not a discovery mechanism.
- **III. Fail Closed** — ✅ PASS. The annotation is deterministic and safe: if the field is `Some(_)` it emits; if `None` it omits. No new failure modes introduced. Emission itself cannot fail beyond existing JSON serialization guarantees.
- **IV. Type-Driven Correctness** — ✅ PASS. Value is derived from the `HelmExtractionMode` enum (`Unrendered` → `"partial"`, `Rendered` → `"full"`) via a small `as_wire_str()` method — same pattern as m161's `WorkspaceMode::as_wire_str()`. No stringly-typed boundaries.
- **V. Specification Compliance** — ✅ PASS. Constitution Principle V audit:
  - **CDX 1.6**: no native construct for "coverage confidence at document scope for image-ref extraction fidelity". `metadata.properties[]` is the m071-approved carrier for `mikebom:*` document-scope signals. Same slot as C110, C111, C112, C118, C119, C122.
  - **SPDX 2.3**: no native construct. Doc-scope `Annotation` with `MikebomAnnotationCommentV1` envelope — same pattern as C110/C112.
  - **SPDX 3.0.1**: no native construct. `Annotation` element in JSON-LD graph — same as C110/C112.
  - Audit precedent from m188 addendum in `docs/reference/sbom-format-mapping.md` §Milestone 188 (referenced by spec Assumption). No new precedent needed.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes stay in `mikebom-cli`. No new modules.
- **VII. Test Isolation** — ✅ PASS. US1 (`"partial"`) runs in default CI (no helm binary needed — the Chart.yaml gate triggers the reader). US2 (`"full"`) gated behind `MIKEBOM_HELM_INTEGRATION=1`. US3 byte-identity via existing golden-drift-audit workflow.
- **VIII. Completeness** — ✅ PASS. This IS the completeness signal — it tells the consumer whether they got the reduced-fidelity or the full-fidelity image-ref set. Directly serves Principle VIII / Principle X.
- **IX. Accuracy** — ✅ PASS. Annotation value is a pure function of the actual extraction path (Rendered vs Unrendered). Per FR-005, `--helm-render` that FALLS BACK to unrendered emits `"partial"` — the value reflects reality, not operator intent.
- **X. Transparency** — ✅ PASS. This is the document-scope operator-facing transparency signal m188 always intended. Complements the per-component `mikebom:image-ref-unresolved` markers by giving a single doc-scope tell.
- **XI. Enrichment (DX)** — ✅ PASS. Zero operator-facing surface changes. Zero new CLI flags. Zero new env vars. Purely emission-time metadata addition.
- **XII. External Data Source Enrichment** — ✅ N/A. No external data source involvement.
- **Strict Boundary §5 (file-tier)** — ✅ N/A. Not touching file-tier plumbing.

**Result**: All principles PASS. No violations. No Complexity Tracking entries needed.

**Post-Phase-1 re-check**: N/A — Phase 1 introduces no new entities beyond what's above (1 new struct field on each of `ScanResult` and `ScanArtifacts`, 1 new method on `HelmExtractionMode`, 1 new catalog row entry). Constitution gate remains PASS post-design.

## Project Structure

### Documentation (this feature)

```text
specs/204-helm-completeness-annotation/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 3 mechanical decisions
├── data-model.md        # Phase 1 output — new fields + emitter branch shapes
├── quickstart.md        # Phase 1 output — 4 reproducers
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

No `contracts/` sub-directory — the entire annotation contract is inherited from m071's parity envelope + m188's `mikebom:image-extraction-completeness` naming decision. m204 references those verbatim.

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/
└── mod.rs                                              # MODIFIED — add `helm_extraction_mode:
                                                       #   Option<HelmExtractionMode>` field to
                                                       #   `ScanResult` at line 76, mirror from
                                                       #   diagnostics at line 352.

mikebom-cli/src/scan_fs/package_db/
└── mod.rs                                              # UNCHANGED — `ScanDiagnostics.helm_extraction_mode`
                                                       #   already exists (m188 landed).

mikebom-cli/src/generate/
├── mod.rs                                              # MODIFIED — add `helm_extraction_mode:
│                                                       #   Option<HelmExtractionMode>` field to
│                                                       #   `ScanArtifacts` alongside `go_workspace_mode`.
├── cyclonedx/metadata.rs                               # MODIFIED — `build_metadata` gains one arg
│                                                       #   `helm_extraction_mode: Option<HelmExtractionMode>`.
│                                                       #   Emit branch immediately after C112 emission
│                                                       #   (line 569). Update ~7 test call sites.
└── spdx/
    ├── annotations.rs                                  # MODIFIED — `annotate_document` gains one
    │                                                   #   branch immediately after C112 emission
    │                                                   #   (line 639), reading `artifacts.helm_extraction_mode`.
    └── v3_annotations.rs                               # MODIFIED — analogous branch immediately
                                                       #   after C112 emission (line 601).

mikebom-cli/src/parity/extractors/
├── mod.rs                                              # MODIFIED — register C123 ParityExtractor
│                                                       #   entry alongside C112 (line 472).
├── cdx.rs                                              # MODIFIED — add `cdx_anno!(c123_cdx,
│                                                       #   "mikebom:image-extraction-completeness",
│                                                       #   document);` alongside C112 (line 814).
├── spdx2.rs                                            # MODIFIED — add `spdx23_anno!(c123_spdx23,
│                                                       #   "mikebom:image-extraction-completeness",
│                                                       #   document);` alongside C112 (line 583).
└── spdx3.rs                                            # MODIFIED — add `spdx3_anno!(c123_spdx3,
                                                       #   "mikebom:image-extraction-completeness",
                                                       #   document);` alongside C112 (line 643).

mikebom-cli/src/cli/
└── scan_cmd.rs                                         # MODIFIED — add `helm_extraction_mode`
                                                       #   to the `ScanResult` destructure at
                                                       #   line 2515, thread into `ScanArtifacts`
                                                       #   at line 3187 alongside `go_workspace_mode`.

mikebom-cli/tests/
└── helm_reader.rs                                      # MODIFIED — add 3 new integration tests:
                                                       #   - m204_us1_partial_annotation_on_unrendered_scan
                                                       #     (asserts CDX, SPDX 2.3, SPDX 3 all carry
                                                       #      "partial" for a default helm-chart scan)
                                                       #   - m204_us2_full_annotation_on_rendered_scan
                                                       #     (gated MIKEBOM_HELM_INTEGRATION=1;
                                                       #      asserts "full" when --helm-render succeeds)
                                                       #   - m204_us3_no_annotation_on_non_helm_scan
                                                       #     (byte-identity guard; asserts absence)
```

**Structure Decision**: 7 source file edits + 1 test file extension + zero fixture additions. Test coverage covers all three P1 stories via one CI-gated test each. Zero fixture files needed — tests construct chart directories inline via `tempfile::tempdir()` per m188/m203 precedent.

## Complexity Tracking

No constitution violations. All principles pass on first check. The plumbing pattern is precedented five times over (`go_transitive_coverage`, `go_transitive_fallback_count`, `go_cache_warming`, `go_workspace_mode`, `divergence_records`); no new architectural choices needed.
