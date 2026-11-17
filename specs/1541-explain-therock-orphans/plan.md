# Implementation Plan: Every unreachable component on TheRock explains itself

**Branch**: `1541-explain-therock-orphans` | **Date**: 2026-11-17 | **Spec**: [spec.md](spec.md)

## Summary

#1593. On TheRock's full source tree, at the code `main` now carries, 846 components are unreachable and **841 carry no marker** (`measurements/source-trace.md`). The FR-001 trace found four causes:

1. **Two reachability answers.** The orphan classifier calls the completeness function with a default root and unprepared inputs, while the emitters use the operator's root and anchored inputs. On TheRock this left npm components the document calls unreachable uncoded. **Fix:** one entry point, `graph_completeness::for_document`, called by the three emitters and the classifier alike (R1).
2. **No rule for binaries.** 787 components from binary analysis. **Fix:** two codes below every existing one: `test-data-binary` (572 on TheRock) and `unowned-binary` (215, mostly committed GPU code objects) (R3, clarification Q2).
3. **A uv reader gap.** An editable lock root with a dynamic version is skipped, which strands its dependency groups (35 PyPI components). **Fix:** emit the root versionless with C151, and wire its runtime, optional and development edges, including every forked version (R4).
4. **Small populations** (cargo, npm, conan, vcpkg; two generic). Expected to take existing codes once (1) lands; the rest are traced one by one (R5).

**The SDK** (645 unexplained at v0.17.0, 644 of them nested): containment counts as reachability, in `for_document` and in the I7 check (R2, clarification Q1). The re-measurement at `main` is T001 (run 44173368036).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.

**Primary Dependencies**: Existing only: `generate::graph_completeness`, `generate::orphan_reason`, `generate::root_selector`, `scan_fs::package_db::pip::uv_lock`, `serde_json`, `tracing`. **Zero new Cargo dependencies** (FR-010).

**Storage**: N/A.

**Testing**:
- **Unit:** `for_document` against each emitter's former preparation; containment edges; both new codes and the directory list; the uv reader's editable root and groups.
- **Integration:** a new `waybill-cli/tests/orphan_explanations.rs` with cases O-1 to O-12, in three formats, asserting I-A to I-C. The I7 harness check gains nested components.
- **Mutations:** each rule.
- **CI:**
  - `measure-source` and `measure-full` at the branch against `main`, probed with `probe_i7_on_document`;
  - the public corpus (invariant gate) and the quality corpus, each against a `main` control.

**Target Platform**: as today. **Project Type**: CLI.

**Performance Goals**: SC-007, at most 1.5× `main` on the full source tree.

**Constraints**: deterministic; no network; synthetic fixtures; TheRock in CI only.

**Scale/Scope**: TheRock source has 4,348 components; 846 are unreachable at `main`.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust | Pass. No new crates. |
| III. Fail Closed | Pass. Every new code applies only to the population it names. A component no rule covers stays unexplained and is reported by I7, never forced into a code. |
| IV. Type-Driven | Pass. The new codes extend the closed `OrphanReasonCode` enum, and `for_document` replaces four hand-kept copies of one preparation. |
| V. Spec Compliance | Pass. Two C45 values are registered in the catalog row first. Containment uses the formats' native nesting and `CONTAINS`, and adds no new row. |
| VII. Test Isolation | Pass. Synthetic fixtures. TheRock CI documents are probed locally; TheRock is never scanned locally. One file was read through the GitHub API. |
| VIII/IX/X | Pass. The aim is that every unreachable component on TheRock says why. A missing edge is fixed as an edge, not hidden behind a reason (R4). |

No violations.

## Project Structure

```text
specs/1541-explain-therock-orphans/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/wire.md           # O-1…O-12, I-A…I-C
├── measurements/source-trace.md
└── tasks.md
waybill-cli/src/generate/graph_completeness/mod.rs     # for_document; containment edges
waybill-cli/src/generate/cyclonedx/builder.rs          # call for_document
waybill-cli/src/generate/spdx/document.rs              # call for_document
waybill-cli/src/generate/spdx/v3_document.rs           # call for_document
waybill-cli/src/generate/orphan_reason.rs              # call for_document; two new codes
waybill-cli/src/cli/scan_cmd.rs                        # pass the real root override and version
waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs      # editable root; group edges; forks
waybill-cli/tests/orphan_explanations.rs               # O-1…O-12
waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs  # nested components
waybill-cli/tests/public_corpus.rs                     # probe_i7_on_document
docs/reference/sbom-format-mapping.md                  # C45 values; containment note
docs/reference/sbom-invariants.md                      # I7: containment
```

## Complexity Tracking

None.
