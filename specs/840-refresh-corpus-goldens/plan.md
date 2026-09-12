# Implementation Plan: Refresh the public-corpus goldens with verified drift

**Branch**: `840-refresh-corpus-goldens` | **Date**: 2026-09-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/840-refresh-corpus-goldens/spec.md`

## Summary

The nightly public-corpus lane fails on ten of eleven targets. The goldens were last written on 2026-07-21 and roughly 147 merges have landed since, at least one of which rewrites nearly every component. A lane that always fails carries no information, so a genuine regression arriving tomorrow would be invisible.

Research found that **the generation half of this work already exists**: `public-corpus.yml` has a `regen_goldens` dispatch input that runs the harness with the update flag and uploads the regenerated tree as an artifact, and goldens are written already-masked so a `git diff` is largely normalised for free. The work this feature actually adds is the *verification* half the spec insists on — a review-time ordering normaliser (the one gap masking cannot close), per-target attribution of every delta category, and a deliberate mutation proving the lane can still go red.

Approach: dispatch the existing regen against this branch, download the artifact, normalise ordering for review, attribute every delta category against the merge log, commit all targets in one change with the attribution in the PR description, then prove the gate still detects change.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain; no nightly). No production source changes — this feature touches test fixtures, a review-time tool under `xtask`, and documentation.
**Primary Dependencies**: Existing only — `serde_json` (already used by the harness for masked golden serialisation), the m195 harness at `waybill-cli/tests/corpus_harness_195/`, and the `public-corpus.yml` dispatch. **Zero new Cargo dependencies.**
**Storage**: Committed goldens at `waybill-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`. Unchanged layout.
**Testing**: The corpus lane itself is the test. Its verification is `cargo test --test public_corpus` under `WAYBILL_RUN_PUBLIC_CORPUS=1`, dispatched via CI per FR-002.
**Target Platform**: `ubuntu-latest`, because that is where the gate runs and FR-002 requires goldens be generated in the gate's environment class.
**Project Type**: Test-fixture refresh plus a small review-time tool. Not a product feature.
**Performance Goals**: N/A. Nothing here runs in the shipped binary.
**Constraints**: FR-008 — no assertion may be widened, relaxed or disabled to make a target pass. FR-003 — no local golden generation. FR-015 — evidence in the PR, not committed as a document.
**Scale/Scope**: Ten failing targets × three formats = thirty goldens, against an eleventh passing target left untouched.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | PASS. No new dependency; the review-time normaliser uses `serde_json`, already present. Nothing links. |
| **II. eBPF-Only Observation** | N/A. No observation path touched. |
| **III. Fail Closed** | PASS, and strengthened. FR-007 blocks the refresh on an unattributable delta; FR-012 forbids regenerating a golden for a target failing for non-drift reasons. The feature refuses rather than guesses. |
| **IV. Type-Driven Correctness** | PASS. New code is test/tooling only; `clippy::unwrap_used` discipline applies via the workspace gate. |
| **V. Specification Compliance** | PASS. Emitted formats are unchanged — this records what is emitted, it does not alter it. |
| **VI. Three-Crate Architecture** | PASS. Fixtures under `waybill-cli/tests/`, tooling under `xtask/`. No crate boundary moves. |
| **VII. Test Isolation** | PASS. Per-target goldens; no shared mutable state introduced. |
| **VIII–XII** (Completeness, Accuracy, Transparency, Enrichment, External Data) | N/A. No emission or enrichment behaviour changes. |

**Strict Boundaries**: none engaged. No lockfile-based discovery, no MITM, no first-party C, no `.unwrap()` in production paths, no file-tier duplicate change.

**Gate result: PASS.** No violations, so Complexity Tracking is empty.

One note rather than a violation: FR-014 adds a document to `docs/development/`, while #827 is concurrently retiring `docs/design-notes.md` for accretion. These are compatible — #827's objection is to a document that accreted point-in-time journal entries, not to procedures. FR-015 keeps the point-in-time material (the attribution evidence) out of the tree entirely.

## Project Structure

### Documentation (this feature)

```text
specs/840-refresh-corpus-goldens/
├── spec.md              # complete, 3 clarifications integrated
├── plan.md              # this file
├── research.md          # R1–R6, complete
├── data-model.md        # entities: target, golden, normalised diff, attribution
├── quickstart.md        # the FR-014 procedure, drafted here, published to docs/
└── contracts/
    └── xtask-corpus-diff-cli.md   # the review-time normaliser's contract
```

### Source Code (repository root)

```text
waybill-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json
    REGENERATED for every currently-failing target. No layout change.

xtask/src/corpus_diff/            # NEW — review-time normaliser (R3)
    mod.rs                        # sort unordered collections by stable key
    tests.rs                      # ordering-only change must normalise to empty

docs/development/refreshing-corpus-goldens.md   # NEW — the FR-014 procedure
    Sibling of docs/perf/refreshing-the-baseline.md, same hazard class.

.github/workflows/public-corpus.yml             # UNCHANGED
waybill-cli/tests/corpus_harness_195/           # UNCHANGED
```

**Unchanged is the point.** The regen path, the masking and the workflow all already do their job; touching them would risk the gate while adding nothing.

## Phase sequencing

Ordered so that nothing is committed before it can be reviewed:

1. **Establish the definitive failing set.** Dispatch the lane read-only against this branch. FR-001 scopes to "currently failing", so the list is observed, not assumed — the ten-of-eleven figure is from an earlier run and may have moved.
2. **Build the review-time normaliser** (R3, contracts/). It must exist before the diffs are read, or the first thing anyone sees is the unreviewable raw diff this feature exists to avoid. Its own test: an ordering-only change normalises to an empty diff.
3. **Dispatch regen; download the artifact.** No local generation (FR-003).
4. **Normalise and read every per-target diff.** Attribute each delta category against the merge log since `25bfbce` (R4). Anything unattributable stops the refresh for that target (FR-007).
5. **Resolve any non-drift failure** under FR-012 / FR-012a — repair here, or drop from gating with a tracked issue and make the drop visible in lane output.
6. **Commit all targets together** (FR-013), attribution in the PR description (FR-015), commit message referencing the PR (FR-015a).
7. **Prove the teeth** (FR-010 / SC-003): mutate emission, confirm the lane fails and names the target and format, revert.
8. **Verify reproducibility** (FR-011 / SC-004): a second regen against the same tree produces identical goldens.
9. **Write the procedure** to `docs/development/` (FR-014).

Steps 7 and 8 are ordered last deliberately: both are checks on the committed result, and doing them earlier would test something that is about to change.

## Complexity Tracking

No constitutional violations. Table intentionally empty.
