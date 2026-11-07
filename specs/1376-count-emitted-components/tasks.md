# Tasks: C209 promises a sum a consumer cannot check

**Feature**: `1376-count-emitted-components` | **Issue**: #1385
**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/documentation.md](./contracts/documentation.md), [quickstart.md](./quickstart.md)

**Tests**: required by FR-005, and written **before** the wording. The wording
is the deliverable but cannot be executed; the test is the only artifact that
can be shown wrong.

> **Do not change any count.** The five numbers are correct in every format.
> #1385 was filed claiming otherwise and a reader of its first version would
> "fix" arithmetic that is already right. FR-002, and quickstart's *What is
> deliberately not checked*.

---

## Phase 1: Setup and baseline

- [X] T001 Capture a pre-change document from `waybill-cli/tests/fixtures/pnpm_v9_scope` in all three formats, scanned **without** `--root-name`, as the byte-identity baseline SC-004 compares against
- [X] T002 [P] Record in `measurements/README.md` the pre-change figures that demonstrate the defect: `row=10` against `.components | length = 9` on `pnpm_v9_scope`, and `row=6` against `5` on `closure_role_scope`
- [X] T003 [P] Record the three root shapes and how each is produced — **synthetic** (`cargo`, `pip`, `pants_go`, or any `--root-name` scan), **promoted** (`pnpm_v9_scope`, `sbom_edit`, `closure_role_scope`, `nix_eval`), **duplicated** (see T007's recipe). Note that live `moat` is the only *observed* duplicated case but is a nix-closure corpus clone, **not usable as a test input**

---

## Phase 2: User Story 2 — a test exercises the shape that broke (P2)

**Ordered before US1 although US1 is P1.** US1's deliverable is prose, which
nothing can execute. This test is the only thing that can fail, so it goes
first and the wording is written to match a check that has been run.

**Goal**: the promoted-real-root shape is asserted against, not merely
reachable.

**Independent test**: the test fails when run against the old claim's
expression and passes against the corrected one.

- [X] T004 [US2] Add an integration test in `waybill-cli/tests/root_selection_integration.rs` — the existing home for root-selection behaviour — scanning a single-main-module project **without** `--root-name` and asserting the CDX expression from `data-model.md` §3 agrees with the C209 row. **Synthesize the fixture inline into a `tempfile::tempdir()`**, matching that file's stated convention; do not read `waybill-cli/tests/fixtures/`, which would diverge from it. A single `go.mod` + `main.go` at the scan root produces the promoted shape
- [X] T005 [US2] (SC-005) **Run it against the old expression first and confirm it fails.** `.components | length` returns 9 where the row says 10. A test written green and never seen red is a test of nothing (quickstart Control 4)
- [X] T006 [P] [US2] Extend the test to the **synthetic** shape (`--root-name` passed) and assert the adjustment is **+0**. An unconditional `+1` would break every `--root-name` scan, including all 17 corpus targets
- [X] T007 [P] [US2] Extend the test to the **duplicated** shape and assert **+0** — the clause a naive fix omits. **The recipe matters and is not obvious**: several main modules is *not* sufficient, because a `go.work` using `./a` and `./b` yields a **synthetic** root. One of the modules must be at the **scan root**. A `go.work` using `.` and `./sub`, with a `go.mod` + `main.go` in each, produces `heuristic=repo-root-main-module` and a real root present in `components[]` — verified during analysis
- [X] T008 [US2] (FR-001) Assert the SPDX 2.3 and SPDX 3 expressions on the same scan: `len(packages[])` and `Package + File` nodes. SPDX 3's file-tier entries are the second, independent reason the naive check fails (FR-004, SC-003)

---

## Phase 3: User Story 1 — the documented check works (P1)

**Goal**: a consumer who follows the documentation gets the right answer on
every format and every project shape.

- [X] T009 [US1] Correct the `debug_assert` comment in `cpe_coverage` at `waybill-cli/src/cli/scan_cmd.rs` to say it compares the counts to **the slice** — that nothing was classified twice or not at all — and **explicitly that it does not check the emitted document**. It must describe the comparison on the line below it and nothing more
- [X] T010 [US1] Correct `CpeSynthesisCoverage::total()`'s doc comment at `scan_cmd.rs`, removing "against the document as it stands". State that the sum is over resolved components and point to the C209 row for the per-format expressions
- [X] T011 [US1] (FR-003, SC-006) Correct the **C209 row** in `docs/reference/sbom-format-mapping.md` per `contracts/documentation.md` §2: replace the `.components | length` promise with the three per-format expressions, and **delete the phrase "with no filtering rule the reader has to know"** — there is a rule, it is short, and denying it is what made the claim false
- [X] T012 [US1] Verify the corrected row still satisfies `sbom_format_mapping_coverage.rs`, which parses the map for non-empty entries in each of the three format columns. The prose changes; the columns must not

---

## Phase 4: Polish and verification

- [X] T013 Verify **byte-identity**: every emitted document is unchanged against T001's baseline in all three formats (FR-006, SC-004) — quickstart Control 3. This feature changes prose and adds a test; a differing document means something leaked
- [X] T014 [P] Verify the expression on the three root shapes end to end, per quickstart Control 2: synthetic +0, promoted +1, duplicated +0 (SC-001, SC-002)
- [X] T015 [P] Re-verify against the committed corpus goldens that **15 of 16** agree, and that the sixteenth is `pants-example-javascript` — the **filtered** golden (#1380), not a counter-example. Any other disagreement is a finding
- [X] T016 (SC-005) **Mutation-check**, recording results in `measurements/README.md`. Drop the `waybill:*` test from the expression → T006 must fail. Drop the `bom-ref` test → T007 must fail. Make the adjustment unconditional → T006 must fail. Revert each with `cp` from a backup, **never `git checkout`**: on an uncommitted file that deletes the work under test while the gate still passes
- [X] T017 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189). Assert on the positive signal and the per-target `test result: ok. N passed; 0 failed` lines, and **enumerate the new tests the run executed** rather than reading the total
- [X] T018 [P] Confirm **no golden regenerated**, local or corpus. Nothing emitted changes, so a moved golden is a finding and not a refresh — the inverse of the usual expectation
- [ ] T019 [P] Update issue #1385 with the delivered wording and the test, and confirm its title and body reflect that the counts were always correct

---

## Dependencies

```text
Phase 1 (T001-T003)  baseline + the shapes each fixture produces
        │
Phase 2 (T004-T008)  US2 — the test, seen red first         ← the only executable artifact
        │
Phase 3 (T009-T012)  US1 — the comments, then the doc row   ← the deliverable
        │
Phase 4 (T013-T019)  verification
```

**T009 precedes T011** deliberately: the documentation row was written *from*
the source comment, so correcting the comment first means the row is written
from a corrected description rather than re-derived from the same false one.

**T005 must happen before T009–T011.** Once the wording is corrected it is no
longer possible to demonstrate the old expression failing without reverting.

## Parallel opportunities

- **T002, T003** — two independent measurement records.
- **T006, T007** — two shapes, same test file, independent cases.
- **T014, T015** — different artifacts.
- **T018, T019** — a check and an issue update.

## MVP

**Phase 1 + Phase 2 + Phase 3** (T001–T012). That is the corrected claim and
the test that keeps it honest. Phase 4 verifies it did not cost anything.

## Task count

| phase | tasks |
| --- | --- |
| Setup / baseline | 3 |
| US2 (P2) — the test | 5 |
| US1 (P1) — the corrections | 4 |
| Polish | 7 |
| **total** | **19** |
