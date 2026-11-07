# Implementation Plan: C209 promises a sum a consumer cannot check

**Branch**: `1376-count-emitted-components` | **Date**: 2026-11-07 | **Spec**: [spec.md](./spec.md)
**Issue**: #1385

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1376**; it fixes issue **#1385**.

## Summary

Milestone 1374 promised that C209's five counts *"sum to `.components |
length` exactly, with no filtering rule the reader has to know"*.

**The counts are right. The sentence is wrong.** All three formats emit
exactly the same number of things — CDX 199 + 1 in `metadata.component`,
SPDX 2.3 200 packages, SPDX 3 197 Packages + 3 Files — and the row says 200
in each. A consumer following the documented check gets 199 and concludes the
row is broken.

**Approach**: correct the claim in three places, state one expression per
format, and add an integration test over a fixture that exhibits the failure.
No count changes; no emitted byte changes.

Five findings shaped the plan:

1. **This is not a counting defect**, which is how #1385 was filed. Measuring
   the three formats side by side overturned it.
2. **Two independent reasons** `.components | length` is short: CDX promotes
   a single main-module, SPDX 3 emits files as `File` nodes.
3. **The CDX rule needs three clauses**, each load-bearing: drop one and a
   previously-correct shape over-counts.
4. **The fixture already exists, four times over.** The shape was in-tree and
   nothing ever asserted against it.
5. **The `debug_assert` is true and its comment is false**, and the
   documentation was written from the comment.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: none. Three comment/doc edits and one integration test.
**Storage**: N/A.
**Testing**: `./scripts/pre-pr.sh` via `scripts/gate.sh`, plus one new integration test scanning an existing fixture without `--root-name`.
**Target Platform**: unchanged — this emits nothing new.
**Performance Goals**: N/A.
**Constraints**: no count may change (FR-002); no emitted document may change (FR-006, SC-004); the expression must hold for all three root shapes and all three formats (FR-001).
**Scale/Scope**: one documentation row, two source comments, one test.

## Constitution Check

*Constitution v4.0.2. Evaluated before Phase 0 and re-evaluated after Phase 1 — identical both times.*

| Principle | Verdict | Basis |
| --- | --- | --- |
| **I. Pure Rust, statically linked** | ✅ PASS | No dependency change. |
| **IV. Type-driven correctness** | ✅ PASS | No type change. The `debug_assert` is kept; its description is corrected to match what it compares. |
| **V. Specification compliance** | ✅ PASS | Nothing emitted changes, so no conformance surface moves. |
| **VIII. Completeness** | ✅ N/A | No component population changes. |
| **IX. Accuracy** | ✅ PASS | The feature is this principle turned on waybill's own documentation: a correct number described falsely. |
| **X. Transparency** | ✅ PASS | A consumer gets a check that works instead of one that fails on correct documents. |
| **XII. External enrichment** | ✅ N/A | No external source. |
| **Strict Boundaries 1–5** | ✅ N/A | No trace mode, no proxy, no C, no `.unwrap()`, no file-tier change. |

**Result: PASS.**

## Project Structure

```text
specs/1376-count-emitted-components/
├── spec.md                      # 6 FR, 6 SC, 2 clarifications
├── plan.md                      # this file
├── research.md                  # R1–R7 + the issue correction
├── data-model.md                # the two populations, RootKind, the expressions
├── contracts/documentation.md   # the sentence before and after
├── quickstart.md                # four controls, and why the usual ones are useless here
├── checklists/requirements.md
└── tasks.md                     # /speckit.tasks — NOT created here
```

### Source and docs

```text
docs/reference/sbom-format-mapping.md     # the C209 row's claim
waybill-cli/src/cli/scan_cmd.rs
  ├── CpeSynthesisCoverage::total()       # doc comment
  └── cpe_coverage()                      # the debug_assert's comment
waybill-cli/tests/…                       # one new integration test
```

**Structure Decision**: the existing layout. Nothing moves.

## Phase ordering

1. **Baseline** — capture a pre-change document from the fixture that
   exhibits the failure, so SC-004's byte-identity has something to compare
   against.
2. **The test first, and seen to fail.** Write it against the *old* claim's
   expression and confirm it goes red on `pnpm_v9_scope`.
3. **The source comments**, which is where the false description originates.
4. **The documentation row**, which was written from those comments.
5. **Verification** — the three root shapes, the three formats, byte-identity.

**The test precedes the wording on purpose.** The wording is the deliverable,
but the wording is also unexecutable; the test is the only artifact that can
be shown wrong. Writing it second would mean writing it to match prose that
nothing had checked — which is exactly how the original claim was produced.

## Risks

| Risk | Mitigation |
| --- | --- |
| **An implementer "fixes" the counts** | FR-002 and quickstart's *What is deliberately not checked* both say the numbers are right. This is the mistake #1385 made in its first form |
| A fix handles only the promoted case | Control 2 exercises all three root shapes; the other two over-count by 1 if the adjustment is unconditional |
| The new test is written green and never seen red | Control 4 requires it be run against the old expression first |
| The corrected comment drifts from the assertion again | The comment is required to describe the comparison on the line below it and nothing more — the precise failure being undone |
| Someone adds a promoted-root fixture in a third ecosystem and assumes it is covered | R6 records that `npm` and `hackage` are tested and `cargo`/`pants_go` produce synthetic roots |
| `pants-example-javascript` is read as a counter-example | It is the filtered golden (#1380); the contract names it so it is not rediscovered as a defect |

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| None | — | — |

*Two simpler fixes were considered and rejected. **Emitting the root twice in
CDX** would make `.components | length` work, at the cost of changing
document shape for a reporting convenience. **Adding a `total` field to the
row** would make the check format-independent but would only prove the row is
internally consistent — not that it describes the document, which is the
property that was claimed and the one a consumer wants.*

---

**Phase 0 complete** — `research.md`, 7 findings, no unresolved unknowns.
**Phase 1 complete** — `data-model.md`, `contracts/documentation.md`, `quickstart.md`, agent context updated.
**Next**: `/speckit.tasks`.
