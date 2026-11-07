# Tasks: `unaddressable` counts patches and tarballs as software

**Feature**: `1374-exclude-members-from-coverage` | **Issue**: #1380
**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/annotations.md](./contracts/annotations.md), [quickstart.md](./quickstart.md)

**Tests**: written first where they can fail for the right reason. Milestone
1373's tests pin the `/v2` shape and the three-count sum, so those must be
made to fail before they are made to pass.

---

## Phase 1: Setup and baseline

- [X] T001 Capture the pre-change document by scanning the pinned moat clone with `--nix-closure --no-clearly-defined`, keeping it for SC-004 and SC-005. **Copy the clone before scanning** — evaluating a flake in place can re-lock it, which broke 11 tests on 2026-11-04 — and verify with `md5 -q` on `flake.lock` before and after
- [X] T002 [P] Record the pre-change per-component `{purl, cpe, cpes}` snapshot in `measurements/README.md` as the **set** SC-004 diffs against, not a count: a count can stay equal while membership changes
- [X] T003 [P] Record the pre-change C209 value for every corpus target from the committed goldens, confirming `contracts/annotations.md`'s table against the tree rather than trusting it
- [X] T004 [P] Record the count of **tracked** golden files carrying `waybill:cpe-coverage` — **33** local and **48** corpus. A bare `grep -rl` reports 66 local; half are `*.actual.json` diagnostics, and counting them made a complete regeneration look half-applied in milestone 1373

---

## Phase 2: Foundational — the classifier

**Blocking.** Both user stories read this classification.

- [X] T005 Add `ExcludedMember` and `ExcludedFile` to `FeedAddressability` in `waybill-cli/src/cli/scan_cmd.rs` as **variants on the closed enum**, not a filter ahead of it. A filter leaves the enum describing a population it no longer covers, so the exhaustive match stops proving every component was considered (#892)
- [X] T006 (FR-001) Extend `classify_addressability` with the exclusion tests **ahead of** the CPE test, per `data-model.md` §1: membership first, then file-tier, then the existing three arms
- [X] T007 [P] Unit-test in `scan_cmd.rs` that a component carrying `waybill:closure-member-role` classifies `ExcludedMember` **even when it carries a CPE** (FR-003). No member carries one today — 0 of 539 on darwin, 0 of 1,087 on linux — so this test must construct one; the guard it protects is the synthesizer's, which could change
- [X] T008 [P] Unit-test in `scan_cmd.rs` that a member carrying an **OSV-addressable** PURL classifies `ExcludedMember`, not `AddressableWithoutCpe`. This case is real: `pkg:hackage/time` with role `fragment` on the linux closure
- [X] T009 [P] Unit-test in `scan_cmd.rs` that a file-tier component classifies `ExcludedFile`. **The in-memory signal is `extra_annotations[COMPONENT_TIER_KEY] == "file"`** — not `type`, which `ResolvedComponent` does not have, and not "no PURL", because a file-tier component carries `pkg:generic/file-tier?content-sha256=…` in memory and would otherwise classify `Unaddressable`. The emitter drops that PURL; `partition.py` correctly uses the document signal instead, and the two must not be "aligned"
- [X] T010 [P] Unit-test in `scan_cmd.rs` that an ordinary component is unaffected: `pkg:generic/openssl@3.0.0` still `Unaddressable`, `pkg:hackage/aeson@2.2.4.1` still `AddressableWithoutCpe`, a CPE-bearing component still `WithCandidates`

---

## Phase 3: User Story 1 — an operator sizes the CPE gap (P1)

**Goal**: `unaddressable` counts software and nothing else.

**Independent test**: scan the pinned closure, read C209, confirm the count
matches a hand count of components that are software, carry no CPE, and whose
PURL type no feed indexes.

- [X] T011 [US1] **Make the existing tests fail first.** `waybill-cli/tests/closure_component_cpes.rs` asserts the literal `waybill-cpe-coverage/v2`, and `scan_cmd.rs`'s unit tests assert the three-count sum. Update both to the `/v3` shape **before** changing the struct, and confirm they fail; a test that survives this edit unchanged was not asserting the shape
- [X] T012 [US1] Add `excluded_members` and `excluded_files` to `CpeSynthesisCoverage` in `waybill-cli/src/cli/scan_cmd.rs`, bump `wire()` to `waybill-cpe-coverage/v3`, and extend `total()` to five (FR-008)
- [X] T013 [US1] **Split the function** per `data-model.md` §3: `fill_missing_cpes(&mut [..])` synthesizes and returns `()`; a new `cpe_coverage(&[..]) -> CpeSynthesisCoverage` counts. Then **move the counting call** to after `scan_cmd.rs:5315`, where file-tier components have joined the vector (FR-009). **Synthesis stays at 4907** — it mutates `c.cpes` and later passes read them — so only the counting moves, which is what keeps `with_candidates` meaning the same thing
- [X] T013a [US1] Update the **10 unit-test call sites** in `scan_cmd.rs` that do `let cov = fill_missing_cpes(..)` to synthesize and then count as two lines, **keeping each original assertion unchanged**. A signature break invites the quickest repair rather than the one that preserves what each test meant
- [X] T014 [US1] Assert that the five counts **sum to the emitted component count** (FR-002, SC-002) — an assertion, not a comment, and over the vector as it stands after the file-tier extend rather than over a filtered subset
- [X] T015 [US1] Update the `tracing::info!` that currently names three fields so it names five; it would otherwise log a shape that no longer exists
- [X] T016 [US1] Verify against the real closure that the row reads **78 / 166 / 194 / 539 / 0 = 977** on aarch64-darwin, recording it in `measurements/README.md`. A different split is a finding, not a refresh — these figures were computed from the document before any code changed

---

## Phase 4: User Story 2 — the row still adds up on its own (P2)

**Goal**: the sum is checkable against the document with no filtering rule.

- [X] T017 [US2] Run `measurements/partition.py` against the post-change document and confirm **AGREES**. It currently reports `DISAGREES` against a `/v2` golden, so it already has teeth; a run that cannot report `DISAGREES` is not a check (SC-001)
- [X] T018 [P] [US2] Confirm the five counts sum to `.components | length` on every corpus target after the refresh, **excluding the two whose golden is not the whole document**: `image-postgres16` (no C209 at all, #1372) and `pants-example-javascript` (filtered to `pkg:npm/*` by `corpus_harness_195/js_filter.rs`, #760 / feature 675 FR-008, so the row counts more than the golden retains). 15 of 15 applicable targets close (SC-006)
- [X] T019 [P] [US2] Record in `measurements/README.md` that `excluded_members` equals C204's `member-roles` total on any document carrying both: 539 on darwin, 1,087 on linux (FR-007)

---

## Phase 5: Polish and verification

- [X] T020 Verify **`with_candidates` is unchanged** on every corpus target, and that **no member carries a CPE** on any of them — quickstart Control 4, the structural claim behind FR-004
- [X] T021 Verify `addressable_without_cpe` is unchanged **except** on `nix-closure-moat`, where it falls by exactly 1 (`pkg:hackage/time`, role `fragment`). Any other movement is a finding (SC-003)
- [X] T022 Verify **no component's `{purl, cpe, cpes}` changed**, diffing T002's snapshot as a set rather than a count (FR-006, SC-004) — quickstart Control 3
- [X] T023 Verify that the only differing JSON path between pre- and post-change documents is the `waybill:cpe-coverage` record, compared structurally in all three formats — **including on a project with no closure** (FR-005, SC-005), where `excluded_members` is 0 but `excluded_files` may not be. Nothing is byte-identical: C209 is on every document. **Build the pre-change binary at the merge base** — milestone 1373 reported 42 spurious differences from a baseline that was one commit stale, and nearly chased them
- [X] T024 **Confirm the parity extractors need no change** rather than assuming it. They key on the row label, not the payload, so C209's three rows should pass untouched — verified by `cargo test --lib parity::` in the **lib** target, where that gate lives. **`holistic_parity` is not a test name**; `cargo test --lib holistic_parity` reports `0 passed; N filtered out`, indistinguishable from a pass. The real gates are `every_catalog_row_has_an_extractor`, `the_closure_rows_extract_equal_sets_from_all_three_formats`, `extractors_table_is_sorted_by_row_id` and `extractors_have_unique_labels`
- [X] T025 **Mutation-check the suite**, recording which tests go red in `measurements/README.md`. Make the member test a no-op → T016/T017 must fail. Make the file-tier test a no-op → T018's sum must fail. Put the exclusion tests **after** the CPE test → T007 must fail. Drop the sum assertion → T014's test must fail. **Revert each mutation with `cp` from a backup, never `git checkout`**: on an uncommitted file that deletes the work under test, and milestone 1373's gate then passed with two tasks missing
- [X] T026 [P] Update `docs/reference/sbom-format-mapping.md`'s C209 row to the `/v3` payload, stating that the sum now closes against the raw component count and that `unaddressable` being much smaller than `/v2`'s is the correction
- [X] T027 Regenerate the **33 local goldens** with `WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`, remembering that **six** test files write them — `cdx_regression`, `spdx_regression`, `spdx3_regression`, `pkg_alias_binding_us1`, `oci_pull_backward_compat`, `optional_dep_classification` — and that regenerating only the three `*_regression` ones leaves the rest stale
- [X] T028 Verify the local golden diff contains **only** C209 lines, using a normalized sorted diff so array reordering cannot masquerade as a hit. Then check the **complement**: which tracked golden carrying C209 did *not* move? The answer must be none — a count can be short for two different reasons and only one is a bug
- [X] T029 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189). Assert on the positive signal and the per-target `test result: ok. N passed; 0 failed` lines, never on the absence of the word FAILED. **Enumerate the `m1374_*` tests the run executed** and compare against what this file says should exist; a missing test is invisible in a pass count
- [X] T030 Refresh the corpus goldens through `public-corpus.yml` workflow dispatch with `regen_goldens=true` — never locally. Expect **16 of 17** targets to change, and verify the run's reported scanned commit is this branch's tip rather than assuming it
- [X] T031 Account for **every** differing corpus file from the artifact rather than from the run summary (#1286: a clean summary once hid one target's stale goldens), and keep `image-postgres16` out of this feature's diff — it carries no C209 at all, for #1372's unrelated drift
- [X] T032 [P] Update issue #1380 with the delivered figures for **both** hosts if they differ from the predicted 78/166/194/539/0 and 6/162/360/1087/0

---

## Dependencies

```text
Phase 1 (T001-T004)  baseline — before any code moves
        │
Phase 2 (T005-T010)  the classifier — blocks both stories
        │
Phase 3 (T011-T016)  US1: the counts, the bump, the move   ← MVP
        │
Phase 4 (T017-T019)  US2: the sum property                 ← reads US1's fields
        │
Phase 5 (T020-T032)  verification and goldens
```

**T012 and T013 must land together.** Moving the computation while it still
produces three fields would emit `/v2` numbers computed over a vector that now
includes file-tier components — a state worse than either endpoint, because
the sum would then match nothing.

**US2 depends on US1** because it checks a property of fields US1 defines.

## Parallel opportunities

- **T002, T003, T004** — three independent measurement outputs.
- **T007–T010** — four unit tests, same file, independent cases.
- **T018, T019** — two different documents.
- **T026, T032** — a docs file and an issue.

## MVP

**Phase 1 + Phase 2 + Phase 3** (T001–T016). That delivers the corrected
count, which is the whole operator-facing value and the input #1381 needs.
US2 makes the row self-checking; it changes no figure.

## Task count

| phase | tasks |
| --- | --- |
| Setup / baseline | 4 |
| Foundational | 6 |
| US1 (P1) | 7 |
| US2 (P2) | 3 |
| Polish | 13 |
| **total** | **33** |
