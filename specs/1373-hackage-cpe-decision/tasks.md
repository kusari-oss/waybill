# Tasks: what `eligible_unmapped` is actually counting

**Feature**: `1373-hackage-cpe-decision` | **Issue**: #1375
**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/annotations.md](./contracts/annotations.md), [quickstart.md](./quickstart.md)

**Tests**: written first where they can fail for the right reason. The row's
shape is asserted by existing tests that pin `/v1`, so those must be made to
fail before they are made to pass.

---

## Phase 1: Setup and baseline

**Measure before changing.** 114 golden files move, and a diff that size hides
a wrong implementation unless the expectation is pinned first.

- [X] T001 Confirm `specs/1373-hackage-cpe-decision/measurements/README.md` records the host, closure pin and waybill commit, and append a "T00x baseline" section for the figures T002–T005 capture
- [X] T002 Capture the pre-change document by scanning the pinned moat clone with `--nix-closure --no-deps-dev --no-clearly-defined`, keeping it for SC-003 and SC-004 comparison. **Copy the clone before scanning**; evaluating a flake in place can re-lock it, which broke 11 tests on 2026-11-04. Verify with `md5 -q` on `flake.lock` before and after
- [X] T003 [P] Record the pre-change per-component `{purl, cpe, cpes}` snapshot from T002's document in `measurements/`, which is what SC-004 diffs against — not a count, the actual set, because a count can stay equal while membership changes
- [X] T004 [P] Record in `measurements/README.md` the pre-change C209 value for **every** corpus target, from the committed goldens rather than from a scan; `contracts/annotations.md` has the table, and this task is to confirm it against the tree rather than trust it
- [X] T005 [P] Record the count of **tracked** golden files carrying `waybill:cpe-coverage` — **33** local and **48** corpus — so that a file count after regeneration that differs is visible as a finding rather than absorbed. A bare `grep -rl` reports 66 local, which is what this task originally expected; half the hits are `*.actual.json` diagnostics a failing test writes, not goldens. Counting them makes a complete regeneration look half-applied

---

## Phase 2: Foundational — the classifier

**Blocking.** Every user story reads this classification.

- [X] T006 Add `OSV_INDEXED_PURL_TYPES` to `waybill-cli/src/cli/scan_cmd.rs` mapping PURL type to OSV ecosystem per `data-model.md` §2, with a doc comment stating it is a **dated observation** and that a row may only be added after re-probing
- [X] T007 Add the `FeedAddressability` enum and its classifier function in `waybill-cli/src/cli/scan_cmd.rs`, as a **closed enum with an exhaustive match** rather than an allowlist with a default arm (#892 survived ~2 years behind an allowlist that fell out of date)
- [X] T008 [P] Unit-test in `scan_cmd.rs` that a component carrying CPEs classifies `WithCandidates` **even when its PURL type is also addressable** — 72 `pkg:pypi` components on the pinned closure are in exactly that position, and counting them twice breaks FR-002's sum
- [X] T009 [P] Unit-test in `scan_cmd.rs` that `pkg:generic` with no CPE classifies `Unaddressable`, and `pkg:hackage` with no CPE classifies `AddressableWithoutCpe`
- [X] T010 [P] Unit-test in `scan_cmd.rs` that a **versionless** `pkg:hackage` PURL still classifies `AddressableWithoutCpe` (FR-008). The 24 such components are in no count today, and FR-008 exists because partitioning only the release-shaped subset is what loses them
- [X] T011 [P] Unit-test in `scan_cmd.rs` that a PURL type absent from `OSV_INDEXED_PURL_TYPES` classifies `Unaddressable` rather than panicking or being skipped — `pkg:github` is the real instance, 1 component on the pinned closure
- [X] T012 Add a test asserting **every value** in `OSV_INDEXED_PURL_TYPES` appears in the committed `measurements/osv-ecosystems-2026-11-07.txt` (FR-003 half a, SC-005). This is the half that stops a row being added from memory
- [X] T012a Record in `measurements/README.md` the probe result backing **every** row of `OSV_INDEXED_PURL_TYPES` — FR-003 half b, SC-005 — showing that ecosystem's advisories returned for a known-vulnerable version. **The table holds exactly three rows**: `hackage`, `pypi`, `npm`. `/speckit.analyze` found a sixteen-row draft of which thirteen satisfied list membership alone; they are cut rather than probed, because measurement showed **zero** components across all 17 corpus targets are classified addressable on an unprobed type — `cargo`, `golang`, `maven` and the rest already carry CPEs and never reach that branch. Cutting them costs no emitted figure
- [X] T012b Keep `measurements/partition.py`'s `PURL_TO_OSV` and `OSV_INDEXED_PURL_TYPES` in step, and assert it. `partition.py` is the instrument for SC-001 (T019, T021) and quickstart Control 2; if the two tables drift, the check and the thing it checks agree by construction and the check is worthless. Trim `partition.py` to the same three rows and add a test comparing the two

---

## Phase 3: User Story 1 — the operator reads the row and acts on it (P1)

**Goal**: the row distinguishes components no feed can address from components
the synthesizer had no arm for.

**Independent test**: scan the pinned closure, read C209, and check the
unreachable count against a hand count from the document.

- [X] T013 [US1] **Make the existing tests fail first.** `waybill-cli/tests/closure_component_cpes.rs:411` asserts `eligible_unmapped >= 1` and line 439 asserts the literal `waybill-cpe-coverage/v1`. Update both to the v2 shape **before** changing the struct, and confirm they fail; a test that passes unchanged across this edit was not asserting the shape
- [X] T014 [US1] Replace `eligible_unmapped` with `addressable_without_cpe` and `unaddressable` on `CpeSynthesisCoverage` in `waybill-cli/src/cli/scan_cmd.rs`, and bump `wire()`'s schema to `waybill-cpe-coverage/v2` (FR-001)
- [X] T015 [US1] Compute the three counts from the classifier over **every** component rather than only those passing `is_release_shaped`, keeping `is_release_shaped` as the gate on *synthesis* exactly as it is (FR-006 — `with_candidates` must not change meaning)
- [X] T016 [US1] Assert in `scan_cmd.rs` that the three counts **sum to the component count** (FR-002, SC-002) — an assertion, not a comment. This is the invariant `eligible_unmapped` never had and the cheapest check that no component was classified twice or not at all
- [X] T017 [US1] Update the `tracing::info!` at `scan_cmd.rs:4908` to log the three new fields; it currently names `eligible_unmapped` and would otherwise log a field that no longer exists
- [X] T018 [US1] Verify against the real closure that the emitted row reads **78 / 166 / 733** summing to 977, recording it in `measurements/README.md`, and that the 24 versionless `pkg:hackage` components fall in `addressable_without_cpe` and in no other count (SC-006). A different split is a finding, not a refresh — the figures were computed from the document before any code changed

---

## Phase 4: User Story 2 — a reader can tell which ecosystems carry the gap (P2)

**Goal**: the figures are attributable to ecosystems and verifiable from the
document.

- [X] T019 [US2] Run `measurements/partition.py` against the post-change document and confirm it prints **AGREES**. It currently prints `DISAGREES` against the v1 row, so it already has teeth; a run that cannot print `DISAGREES` is not a check (SC-001)
- [X] T020 [P] [US2] Record the per-ecosystem composition of each bucket in `measurements/README.md` — expected `with_candidates` 72 pypi + 6 generic, `addressable_without_cpe` 163 hackage + 3 pypi, `unaddressable` 732 generic + 1 github
- [X] T021 [P] [US2] Confirm the C209 row and `partition.py`'s recompute agree on **every** corpus target, not only the closure one, and record any target where they do not

---

## Phase 5: User Story 3 — the no-arm decision is recorded (P3)

**Goal**: the next reader finds the answer where they would look for the arm,
rather than re-running two NVD sweeps to reach it.

- [X] T022 [US3] Record in `waybill-cli/src/generate/cpe.rs`, beside the `match ecosystem` dispatch at line ~170, why there is no `hackage` arm: 171 pairs probed with zero unanswered and **1** hit (`aeson`, 95), while `base`, `Cabal` and `process` carry live HSEC advisories and have **no** dictionary entry — so a blanket arm emits CPEs matching nothing for precisely the packages that matter. Cite `specs/1373-hackage-cpe-decision/measurements/` (FR-007)
- [X] T023 [P] [US3] Add a test asserting `synthesize_cpes` returns empty for a `pkg:hackage` component, so the decision is enforced rather than described and a future arm cannot be added without confronting it
- [X] T024 [P] [US3] Update `docs/reference/sbom-format-mapping.md`'s C209 row to the v2 payload, and state that `unaddressable` being large is the expected shape for a `pkg:generic`-heavy document rather than a defect

---

## Phase 6: Polish and verification

- [X] T025 Verify **no component's `cpes` array changed**, diffing T003's snapshot against the post-change document as a set rather than a count (FR-004, SC-004). quickstart Control 3
- [X] T026 Verify the **only** differing JSON path between pre- and post-change documents is the `waybill:cpe-coverage` record, compared structurally in all three formats (SC-003). Anything else means the change leaked out of the metric
- [X] T027 **Confirm the parity extractors need no change** rather than assuming it. The `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros key on the row label and not the payload, so C209's three rows should pass untouched — but milestone 1372 recorded that an extractor can compensate for a wire shape, so this is verified by running the parity suite in the **lib** target, where that gate lives: `cargo test --lib parity::`. **`holistic_parity` is not a test name** — this task cited one that does not exist, and `cargo test --lib holistic_parity` reports `0 passed; N filtered out`, which is indistinguishable from a pass. The real gates are `every_catalog_row_has_an_extractor`, `the_closure_rows_extract_equal_sets_from_all_three_formats`, `extractors_table_is_sorted_by_row_id` and `extractors_have_unique_labels`
- [X] T028 **Mutation-check the suite**, recording which tests go red in `measurements/README.md`. Classify every component `Unaddressable` → T018/T019 must fail. Drop the sum assertion → T016's test must fail. Return `WithCandidates` for an addressable-without-CPE component → T008 must fail. If the last leaves everything green, the double-count case is not covered
- [X] T029 (FR-005) Regenerate the **33 local goldens** with `WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`, remembering that **six** test files write them — `cdx_regression`, `spdx_regression`, `spdx3_regression`, `pkg_alias_binding_us1`, `oci_pull_backward_compat`, `optional_dep_classification` — and that regenerating only the three `*_regression` ones leaves the rest stale
- [X] T030 Verify the local golden diff contains **only** C209 lines: 33 files changed, every changed line part of a `waybill:cpe-coverage` value, and **zero deletions outside it**. Use a normalized, sorted diff so array reordering cannot masquerade as a hit. **If a local golden did not move, the row is not being emitted where it was** — the inverse alarm to milestone 1372's
- [X] T031 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189). Assert on the positive signal and on the per-target `test result: ok. N passed; 0 failed` lines, never on the absence of the word FAILED
- [X] T032 Refresh the corpus goldens through `public-corpus.yml` workflow dispatch with `regen_goldens=true` — never locally. Expect **16 of 17** targets to change
- [X] T033 Account for **every** differing corpus file from the artifact rather than from the run summary (#1286: a clean summary once hid one target's stale goldens), and keep `image-postgres16` out of this feature's diff. **State the resulting divergence rather than leaving it silent**: after this feature every document carries `waybill-cpe-coverage/v2` except that one, whose committed golden carries no C209 at all. It carries no C209 at all because its golden predates milestone 1367, verified against the milestone-1372 regeneration artifact; its drift is #1372
- [X] T034 [P] Update the issue #1375 thread with the delivered figures if they differ from the predicted 78 / 166 / 733, so the issue does not become stale a second time

---

## Dependencies

```text
Phase 1 (T001-T005)  baseline — must precede any code change
        │
Phase 2 (T006-T012)  the classifier — blocks every story
        │
Phase 3 (T013-T018)  US1: the row's shape        ← MVP
        │
Phase 4 (T019-T021)  US2: attribution            ← reads US1's fields
        │
Phase 5 (T022-T024)  US3: the recorded decision  ← independent of US1/US2
        │
Phase 6 (T025-T034)  verification and goldens
```

**US3 is independent** of US1 and US2 and could ship alone; it is ordered last
only because it is the smallest. **US2 depends on US1** because it reports on
fields US1 defines — the reverse order would mean writing a breakdown of
fields that do not exist.

## Parallel opportunities

- **T003, T004, T005** — three different measurement outputs.
- **T008–T011** — four unit tests, same file, independent cases.
- **T020, T021** — two different documents.
- **T023, T024** — a test and a docs file.

## MVP

**Phase 1 + Phase 2 + Phase 3** (T001–T018). That delivers the corrected row
and its sum invariant, which is the whole of the operator-facing value. US2
makes it attributable and US3 stops the question being re-asked, but neither
changes what the row says.

## Task count

| phase | tasks |
| --- | --- |
| Setup / baseline | 5 |
| Foundational | 9 |
| US1 (P1) | 6 |
| US2 (P2) | 3 |
| US3 (P3) | 3 |
| Polish | 10 |
| **total** | **36** |
