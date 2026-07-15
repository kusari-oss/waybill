# Implementation Plan: Populate Remaining Public-Corpus Goldens

**Branch**: `196-populate-corpus-goldens` | **Date**: 2026-07-14 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/196-populate-corpus-goldens/spec.md`

## Summary

Mechanical follow-up to m195. Runs the 5 non-cobra corpus targets
through the existing harness in golden-regen mode on the `ubuntu-latest`
platform (Q1 clarification), commits 15 resulting golden files (5
targets × 3 formats), replaces the m195 placeholder Docker Hub digest
for `postgres:16` with the real one, and empirically reconciles any
Layer 1 assertions whose spec-knowledge-derived shape doesn't match
actual mikebom output. Zero new Cargo dependencies. Zero changes to
library code, CLI surface, harness architecture, or emitted SBOM shape.
Purely an artifact-population + assertion-tune milestone.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–195). No changes to compiler surface.
**Primary Dependencies**: Existing only — the m195 harness at `mikebom-cli/tests/corpus_harness_195/` is unchanged. No new Cargo deps. No new bash/shell tooling beyond what m195 shipped (`git`, `docker`, `cargo`).
**Storage**: Same as m195 — `~/.cache/mikebom/corpus/<sha>/<pin>/` runtime cache; goldens land in-repo at `mikebom-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`.
**Testing**: Same as m195 — `cargo test --test public_corpus` gated by `MIKEBOM_RUN_PUBLIC_CORPUS=1`. Regen invocation via `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` env; on-runner via `workflow_dispatch` input surfaced through `public-corpus.yml`.
**Target Platform**: Linux `ubuntu-latest` — per Q1 clarification, goldens are generated exclusively on the CI runner that will subsequently verify them nightly. Cross-platform byte-identity out of scope for this milestone.
**Project Type**: Test-fixture augmentation. No library / CLI / SBOM-shape changes.
**Performance Goals**: Per-target scan wall-clock inherited from m195 SC-005 (< 30 min cold-cache total across 6 targets). Regen mode adds a file-write per format; overhead is milliseconds per target.
**Constraints**: (a) additive-only — the m195 go-cobra goldens MUST remain byte-identical per FR-005; (b) no library code changes; (c) `./scripts/pre-pr.sh` wall-clock delta ≤ 5s vs pre-m196 baseline per SC-006; (d) all golden generation happens on Linux CI runners per Q1 / FR-004a.
**Scale/Scope**: 5 targets × 3 formats = 15 new golden files + 1 manifest entry field-update + 0-5 Layer 1 assertion adjustments (empirical count unknown pre-run).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. Zero code changes; goldens are data files. Layer 1 assertion adjustments (if any) stay pure Rust.
- **II. eBPF-Only Observation** — ✅ N/A.
- **III. Fail Closed** — ✅ PASS. Layer 1 assertion adjustments MUST NOT weaken past the point of catching class-of-bug regressions per FR-003; any weakening flagged for review.
- **IV. Type-Driven Correctness** — ✅ PASS. Same `CorpusTarget` type as m195. Postgres digest field is a `PinnedRef::Digest` variant per m195's typed manifest.
- **V. Specification Compliance** — ✅ PASS. No new `mikebom:*` annotations, no new PURL types, no format extensions. Corpus continues to consume emitted output.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes live in `mikebom-cli/tests/`.
- **VII. Test Isolation** — ✅ PASS. Same test-isolation posture as m195.
- **VIII. Completeness** — ✅ PASS. This milestone increases corpus completeness (1 of 6 targets → 6 of 6).
- **IX. Accuracy** — ✅ PASS. Assertion adjustments MUST match observed mikebom output per FR-003 (Accuracy over aspiration).
- **X. Transparency** — ✅ PASS. Any Layer 1 assertion change carries a doc-comment or scratch-note explaining why, per FR-003.
- **XI. / XII. Enrichment** — ✅ N/A.
- **Strict Boundary §5 (file-tier)** — ✅ PASS. No changes to file-tier emission.

**Result**: All principles PASS. No violations to justify.

**Post-Phase-1 re-check**: N/A here — no new entities or contracts introduced beyond the small workflow-dispatch input; m195's data-model and contracts are entirely reused. The post-Phase-1 re-check trivially passes.

## Project Structure

### Documentation (this feature)

```text
specs/196-populate-corpus-goldens/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 4 mechanical decisions
├── data-model.md        # Phase 1 output — reuses m195 entities; annotates the one mutation
├── quickstart.md        # Phase 1 output — 3-step reproducer (dispatch → download → commit)
├── contracts/
│   └── regen-workflow.md   # NEW — workflow_dispatch input surface for one-shot regen
├── checklists/
│   └── requirements.md
├── scratch/             # Assertion-drift audit findings recorded here
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

### Source Code (repository root)

```text
.github/workflows/
└── public-corpus.yml                            # MODIFIED — add `workflow_dispatch.inputs.regen_goldens`
                                                 # boolean + conditional env injection

mikebom-cli/tests/corpus_harness_195/
├── manifest.rs                                  # MODIFIED — postgres:16 digest field only
└── layer1_assertions.rs                         # POSSIBLY MODIFIED — per-target reconciliation

mikebom-cli/tests/fixtures/public_corpus/
├── rust-ripgrep/{cdx,spdx-2.3,spdx-3}.json     # NEW
├── npm-express/{cdx,spdx-2.3,spdx-3}.json      # NEW
├── python-flask/{cdx,spdx-2.3,spdx-3}.json     # NEW
├── maven-guice/{cdx,spdx-2.3,spdx-3}.json      # NEW
└── image-postgres16/{cdx,spdx-2.3,spdx-3}.json  # NEW
```

**Structure Decision**: In-place augmentation of the m195 test infrastructure. No new files under `mikebom-cli/src/`. All new files are either golden data or trivial modifications to the existing m195 workflow / manifest / assertions. The `contracts/regen-workflow.md` is the one net-new spec artifact and it documents the small addition to `public-corpus.yml`'s dispatch surface.

## Complexity Tracking

No constitution violations. No complexity beyond what m195 already carries.
