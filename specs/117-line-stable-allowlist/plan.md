# Implementation Plan: Line-stable walker-audit allow-list

**Branch**: `117-line-stable-allowlist` | **Date**: 2026-06-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/117-line-stable-allowlist/spec.md`

## Summary

One-line edit to the `Walker-audit allow-list check` step in `.github/workflows/ci.yml` (lines 68–129) — extend the live-side grep pipeline with a `sed 's/^\([^:]*\):[0-9]*:/\1:/'` step that strips the `:<line>:` column AND apply the identical filter to the committed allow-list before reading it into `$EXPECTED`. The committed allow-list at `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` regenerates with line-number columns removed at PR-ship time. Docs in `CONTRIBUTING.md § Walker-audit CI gate` + `specs/115-walker-audit-ci/contracts/ci-step.md` track the new shape.

**Technical approach**: pure POSIX-shell change (sed + grep + diff already used at the step; no new tools). The line-stripping filter applies symmetrically to both sides so the diff compares apples-to-apples. The bootstrap is total at PR-ship — there's no transition window where the gate accepts both shapes. The existing failure-message contract is preserved bit-for-bit (only the diff content's shape is shorter by one column).

Negative-test methodology mirrors milestone 115: a synthetic file with `fn walk_synthetic_negative_test_DO_NOT_MERGE()` in a non-allow-listed location still fails CI red (FR-004). Positive-test methodology is new: a synthetic ~50-line helper inserted above an existing allow-listed walker passes CI green (FR-009 — the case that motivated the feature).

## Technical Context

**Language/Version**: POSIX shell (bash) inside GitHub Actions YAML; no Rust code change.
**Primary Dependencies**: existing tools — `grep`, `sort`, `diff`, `sed` (all POSIX-mandated; preinstalled on every GitHub Actions runner image). **Zero new tool installations.** Specifically `sed` is the new addition vs milestone 115; it's already used elsewhere in the workflow and on every runner.
**Storage**: same source-tree-committed plain-text file `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`; the file's location and name are unchanged. Only the per-entry shape changes: `<file>:<content>` (this feature) vs `<file>:<line>:<content>` (milestone 115).
**Testing**: two synthetic-PR tests verify the contract end-to-end. (1) Positive: add a ~50-line unrelated helper above an existing allow-listed walker → CI stays green. (2) Negative: add `fn walk_synthetic_negative_test_DO_NOT_MERGE()` in a non-allow-listed location → CI fails red with the unchanged milestone-115 failure-message contract.
**Target Platform**: GitHub Actions `ubuntu-latest` runner (existing `Lint + test (linux-x86_64)` job at line 39 — same slot milestone 115 chose; no re-slotting).
**Project Type**: CI/CD configuration + one tracked data file (the allow-list) + documentation updates.
**Performance Goals**: ≤5 seconds wall time (inherited from milestone-115 SC-002). The new `sed` step adds <10 ms on ~28 lines of input — negligible vs the existing grep + sort + diff pipeline.
**Constraints**: failure-message contract from milestone 115 MUST be preserved bit-for-bit (FR-003); strict-enforcement bootstrap rule (missing/empty allow-list fails CI) inherited from milestone 115 stays in effect (FR-012); single-PR total cutover, no transition mode (Assumptions).
**Scale/Scope**: 1 CI step's `run:` block updated; 1 allow-list file regenerated; 1 CONTRIBUTING.md section updated; 1 spec contract document updated. Diff size estimate: ~20 lines YAML + ~28 lines of allow-list (same count, shorter entries) + ~15 lines docs.

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | N/A | This feature ships YAML + plain text + Markdown; no source code. |
| II. eBPF-Only Observation | N/A | CI infrastructure, not trace/scan path. |
| III. Fail Closed | ✓ | The gate IS a fail-closed mechanism. The milestone-115 strict-enforcement bootstrap rule (missing/empty allow-list fails CI) is explicitly preserved per FR-012. |
| IV. Type-Driven Correctness | N/A | No Rust code. |
| V. Specification Compliance | N/A | No SBOM emission change. |
| VI. Three-Crate Architecture | N/A | No new crates. |
| VII. Test Isolation | ✓ | Two synthetic-PR tests run as ordinary CI invocations; no privilege requirements. |
| VIII. Completeness | N/A | No discovery layer change. |
| IX. Accuracy | N/A | |
| X. Transparency | ✓ | Failure-message contract from milestone 115 is preserved bit-for-bit (FR-003) — operators triaging a red CI see the same diff hunks + same trailing pointer + same shape; only individual entries are shorter by one column. |
| XI. Enrichment | N/A | |
| XII. External Data Source Enrichment | N/A | |
| Strict Boundary 1 (no lockfile-based discovery) | N/A | |
| Strict Boundary 2 (no MITM) | N/A | |
| Strict Boundary 3 (no C code) | ✓ | Shell + Markdown + text only. |
| Strict Boundary 4 (no `.unwrap()` in production) | N/A | |

**Result**: Constitution Check PASSES. No violations. (Most principles are N/A because this is CI infrastructure, not source code — the same posture milestone 115 had.)

## Project Structure

### Documentation (this feature)

```text
specs/117-line-stable-allowlist/
├── plan.md              # This file
├── research.md          # Phase 0 — 4 implementation decisions: filter mechanism (sed vs awk vs Rust); symmetric vs one-sided filtering; bootstrap timing (no transition); failure-message template preservation
├── data-model.md        # Phase 1 — allow-list entry shape + invariants + comparison-pipeline lifecycle
├── quickstart.md        # Phase 1 — "how the gate works after this PR" runbook for triaging red CI + adding a new exception (new regenerate command)
├── contracts/
│   └── ci-step.md       # The updated CI step contract — supersedes milestone-115 contracts/ci-step.md for the `Walker-audit allow-list check` step's pipeline
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
.github/
└── workflows/
    └── ci.yml                                   # MODIFIED — Walker-audit allow-list check step gains a sed filter on both sides + the failure-message header text line is updated to reflect the new pipeline
mikebom-cli/
└── src/
    └── scan_fs/
        └── walk.audit-allowlist.txt             # REGENERATED — 28 entries, new <file>:<content> shape
CONTRIBUTING.md                                   # MODIFIED — § Walker-audit CI gate's regenerate command + paste-able-snippet updated to the new pipeline
specs/
├── 115-walker-audit-ci/
│   └── contracts/
│       └── ci-step.md                           # MODIFIED — "Step ordering" + "Pipeline" sections updated to record the new filter step; supersession note + cross-link to specs/117/
└── 117-line-stable-allowlist/                   # NEW — speckit artifacts for this feature
```

**Structure Decision**: This feature is intentionally minimal in surface area. The CI step's `run:` block is the only behavioral change; the allow-list file's regen is mechanical; the docs update is two small section edits. No new files in production code paths; one new directory under `specs/` for the speckit artifacts.

## Complexity Tracking

No constitution violations. No complexity to justify. The mechanism choice (sed vs awk vs Rust helper) is the one substantive implementation decision; documented in research.md § Decision 1.
