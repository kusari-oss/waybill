# Tasks: A must-pass registry for invariant cells whose Pass is load-bearing

**Feature**: 1217-must-pass-registry | **Issues**: #1227, closes #1230 | **Refs**: #1216, #1155
**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/must-pass-registry.md](./contracts/must-pass-registry.md),
[quickstart.md](./quickstart.md)

Tests are not optional here. The spec's own argument is that a registry which
cannot be shown to fail is the same vacuity one level up (FR-014), so the
mutation that proves it has teeth lands in the same phase as the mechanism.

**Before starting**: read the draft inlined in #1227 — it is a reference, not a
patch, and [research.md](./research.md) R8 tabulates the five places the
clarifications changed it. Deriving from the tree beats applying the draft.

**Paths** are relative to the repository root. `H/` abbreviates
`waybill-cli/tests/corpus_harness_195/` below only in prose; every task gives
the full path.

**Amended 2026-10-18 after `/speckit.analyze`**: T027 and T038 are new, so
T027–T051 shifted by one and then by two. The three `Carries feature 1156's …`
notes name **feature 1156's** task IDs and did not shift; Phase 2's gained
1156's T031 (the seeded file), which the plan had attributed to Phase 3 while
this file seeded it in Phase 2.

---

## Phase 1: Setup & Foundational

**Blocking.** Nothing reads the registry yet; this phase only makes it
loadable.

- [ ] T001 Create `waybill-cli/tests/corpus_harness_195/invariants/must_pass.rs` with `MustPass { invariant: String, target: String, reason: String, issue: Option<u32> }` and `MustPassFile { #[serde(default)] must_pass: Vec<MustPass> }`, both `#[derive(Debug, Clone, serde::Deserialize)]`, mirroring `declarations.rs:13-32` field for field minus `kind` and `count_at_declaration` (data-model.md)
- [ ] T002 Write the module header in `waybill-cli/tests/corpus_harness_195/invariants/must_pass.rs` stating four things a later reader will otherwise undo: the mirror relationship to `invariant-declarations.toml`, that a registry which cannot fail is the same vacuity one level up, that the pass runs under `Scope::SingleTarget` **unlike** `unfalsifiable_invariants` and why (research R2, FR-008a — and T038 is the test that holds it, so name the test here), and a pointer to `specs/1217-must-pass-registry/contracts/must-pass-registry.md`
- [ ] T003 Add `must_pass_path()`, `load_at(path: &Path) -> Result<MustPassFile, String>` and `load() -> MustPassFile` to `waybill-cli/tests/corpus_harness_195/invariants/must_pass.rs`. `load_at` returns `Err` naming the path on a read or parse failure; `load()` resolves `must_pass_path()` and panics with that error. Do **not** copy `declarations.rs:41-46`'s panic-inside-load shape — FR-011 needs the missing-file and malformed-file cases testable without deleting or corrupting a committed file (research R3)
- [ ] T004 Add `pub struct Unmet { invariant: String, target: String, observed: CellOutcome, reason: String }` to `waybill-cli/tests/corpus_harness_195/invariants/must_pass.rs` — a record, not a formatted string, because the grid needs to know which cells are registered and `lane_failures()` needs to print them (data-model.md, research R1)
- [ ] T005 Add `pub mod must_pass;` to the module list in `waybill-cli/tests/corpus_harness_195/invariants/mod.rs:24-36`, in alphabetical position among the `pub mod` entries (after `goldens`)
- [ ] T006 Add the tests module scaffold to `waybill-cli/tests/corpus_harness_195/invariants/must_pass.rs` with `#[cfg(test)]` and `#[cfg_attr(test, allow(clippy::unwrap_used))]` — `waybill-cli` denies `clippy::unwrap_used` including inside `#[cfg(test)]`, and `--all-targets` enforces it

**Checkpoint**: `cargo clippy --workspace --all-targets` is clean. No behaviour change yet.

---

## Phase 2: User Story 1 — A load-bearing Pass cannot go quiet (Priority: P1)

**Goal**: a cell recorded as must-pass fails the lane if it reports anything
but `Pass`, and the failure says which cell, what it reports now, and what was
written down about why it mattered.

**Independent test**: with the four Pants I8 cells recorded, the hermetic lane
is green; forcing one of them to report `NotApplicable` fails the lane and
names it. Delivered alone this phase closes the hole for every cell anyone
records.

**Carries feature 1156's T026-T029, T031 and T035.**

- [ ] T007 [US1] Implement `validate(file: &MustPassFile, declarations: &DeclarationsFile) -> Vec<String>` in `waybill-cli/tests/corpus_harness_195/invariants/must_pass.rs` with all seven rules from `contracts/must-pass-registry.md`: live invariant, known target, **target not in `goldens::GOLDENS_ARE_FILTERED`**, non-empty trimmed reason, no duplicate `(invariant, target)`, ascending sort, and no cell present in both files. Model the message style on `declarations.rs:51-105`; each message names its own rule
- [ ] T008 [US1] Implement `unmet(cells: &[Cell], file: &MustPassFile) -> Vec<Unmet>` in the same file, mirroring the shape of `unfalsifiable_invariants(cells)` at `waybill-cli/tests/corpus_harness_195/invariants/mod.rs:319`. For each entry, find the cell among `cells`; **absent means skipped**, not unmet (FR-009); present and not `Pass` means an `Unmet`. Do NOT add a third argument to `verdict()` at `mod.rs:293` — it stays the two-input eight-row state machine its own tests pin (FR-008)
- [ ] T009 [US1] Create `waybill-cli/tests/corpus_harness_195/invariant-must-pass.toml` seeded with exactly the four Pants I8 cells, sorted by `(invariant, target)`: `pants-clojure-polyglot`, `pants-example-django`, `pants-example-jvm`, `pants-example-python`, each `issue = 1216`. Start from the draft reasons in #1227 — they were written against the measured corpus — and apply FR-013a: each reason states what would have to be true for a `NotApplicable` on **that** target, which is not the same sentence four times (see the table in spec.md "Why this exists")
- [ ] T010 [US1] Write the header comment of `waybill-cli/tests/corpus_harness_195/invariant-must-pass.toml`: the mirror relationship to the declarations file, the per-cell blind spot between `verdict()` and the all-targets guard, the sort rule and why, that `reason` answers "what would a `NotApplicable` here mean?" and that "it passes today" is rejected by review (FR-003), and why the file is not named `invariant-expectations.toml` (FR-002 — the terminology gate reserves both words)
- [ ] T011 [US1] Wire the registry into the hermetic lane in `invariants::tests::golden_matrix_is_evaluable_and_gated` at `waybill-cli/tests/corpus_harness_195/invariants/mod.rs:347-378`: load it, run `validate` beside `declarations::validate` **before** the report is built, and include `unmet`'s records in the assertion's failure list
- [ ] T012 [US1] Wire the registry into the corpus lane in `waybill-cli/tests/public_corpus.rs:95-126`, beside the existing `declarations::load()` / `validate()` pair, with the same validate-before-compare ordering. Extend the panic text that currently points a reader at `invariant-declarations.toml` to name both files
- [ ] T013 [US1] Test in `must_pass.rs`: `the_committed_must_pass_file_is_valid` — `validate(&load(), &declarations::load())` returns no errors, mirroring `declarations.rs:118-122`
- [ ] T014 [US1] Test in `must_pass.rs`: the committed file holds exactly the four seeded cells, named literally, no more and no fewer (FR-013). Assert the set, not the count — a count passes if someone swaps one target for another
- [ ] T015 [US1] Test in `must_pass.rs`: `a_registered_cell_forced_to_not_applicable_is_unmet_and_names_the_cell` — build cells where a registered cell reports `NotApplicable`, assert `unmet` returns it and that the rendered message contains the invariant, the target, the observed outcome and the entry's reason (FR-007, SC-002, feature 1156's deferred SC-011). **This is the only evidence the registry does anything**
- [ ] T016 [P] [US1] Test in `must_pass.rs`: a registered cell reporting `Fail`, and separately `Error`, is also unmet (SC-003)
- [ ] T017 [P] [US1] Test in `must_pass.rs`: entries whose target is absent from the `cells` slice are skipped, not reported unmet — build a single-target cell set and assert the other three entries are silent (FR-009)

**Checkpoint**: `cargo test --test public_corpus golden_matrix_is_evaluable_and_gated -- --nocapture` is green with four entries recorded, and T015 proves the trap fires. SC-010 holds: all four seeded cells report `Pass`, not `NotApplicable`.

---

## Phase 3: User Story 2 — The registry cannot itself be the vacuity (Priority: P2)

**Goal**: the registry cannot be left looking like a gate while checking
nothing — not by naming a cell that does not exist, not by a blank reason, not
by overlapping the declarations file, not by being deleted or corrupted, and
not by holding an entry no lane ever evaluates.

**Independent test**: violate each rule on its own and get a named failure;
delete the file and the lane fails rather than reading as "no cells are
load-bearing".

**Carries feature 1156's T036-T038.**

- [ ] T018 [P] [US2] Test in `must_pass.rs`: rule 1 — an entry naming `I999` is rejected with a message containing `I999`, not silently skipped
- [ ] T019 [P] [US2] Test in `must_pass.rs`: rule 2 — an entry naming `no-such-target` is rejected
- [ ] T020 [P] [US2] Test in `must_pass.rs`: rule 3 — an entry naming `pants-example-javascript`, whose goldens are filtered (`goldens.rs:67`), is rejected with a message saying the always-run lane cannot evaluate it. This is the rule clarification 1 added; without it FR-010 would be true by construction (research R4)
- [ ] T021 [P] [US2] Test in `must_pass.rs`: rule 4 — a whitespace-only reason is rejected
- [ ] T022 [P] [US2] Test in `must_pass.rs`: rule 5 — two entries for one cell are rejected
- [ ] T023 [P] [US2] Test in `must_pass.rs`: rule 6 — entries out of ascending order are rejected
- [ ] T024 [P] [US2] Test in `must_pass.rs`: rule 7 — a cell present in both the registry and a synthetic `DeclarationsFile` is rejected, with the message naming the cell and both files
- [ ] T025 [US2] Test in `must_pass.rs`: an invalid registry produces validation errors while `unmet` is never consulted — demonstrate the ordering rather than asserting it in prose (FR-004, SC-005). Drive it through the same sequence the lanes use, so a future reordering of the lane breaks this test
- [ ] T026 [US2] Test in `must_pass.rs`: `load_at` on a path inside a `tempfile::tempdir()` that does not exist returns `Err` naming the path (FR-011, SC-006). `tempfile` is already a dev-dependency at `waybill-cli/Cargo.toml:214`
- [ ] T027 [US2] Test in `must_pass.rs`: `load_at` on a file holding malformed TOML returns `Err` naming the path — the *unreadable* half of FR-011's "missing or unreadable", and the other half of the `Err` path T003 builds. Write the bad bytes into a `tempfile::tempdir()`; the committed file is never perturbed
- [ ] T028 [US2] Test in `must_pass.rs`: `load_at` on an empty file returns `Ok` with zero entries, and `validate` accepts it. An empty registry is the honest state of the world; a missing one is a removed gate (FR-011)
- [ ] T029 [US2] Test in `must_pass.rs`: every entry in the committed registry is present among the cells the always-run lane evaluates — build the hermetic matrix over `goldens::load_all()` and assert each entry's `(invariant, target)` appears (FR-010, SC-008). This catches what rule 3 cannot: a target whose goldens are simply missing from disk
- [ ] T030 [US2] Line-stability pair in `must_pass.rs`, mirroring `line_stability.rs:12-107`: a `render(&MustPassFile) -> String` helper over the parsed model, an insert that sorts into the **middle** (`pants-example-golang` lands between django and jvm — an append at either end passes vacuously), an assertion that every other entry's rendered block survives verbatim, and a removal that restores the rendering exactly (FR-015, SC-007)

**Checkpoint**: seven rules demonstrated independently, missing-versus-malformed-versus-empty settled by test, and no entry can hide from both lanes.

---

## Phase 4: User Story 3 — The operator sees which cell broke, in the matrix (Priority: P3)

**Goal**: a reader of the matrix sees which cells carry a must-pass commitment,
and sees a breach among the other lane failures.

**Independent test**: read the rendered grid and see the registered cells
marked while green; force one to `NotApplicable` and the mark changes and the
failure appears in the failure list.

**Carries feature 1156's T030.**

- [ ] T031 [US3] Add the registry as a **required parameter** to `build_scoped`, `build` and `build_for_target` in `waybill-cli/tests/corpus_harness_195/invariants/report.rs:121-155`, and store `unmet`'s records on `MatrixReport`. Not a `with_must_pass()` setter: a caller who forgets a setter gets a green report, which is this feature's own failure mode reproduced in its API (research R1). Compute the records for **every** scope — do not wrap the call in the `match scope` that guards `unfalsifiable` three lines above (`report.rs:144-147`), which is the exact shape FR-008a forbids
- [ ] T032 [US3] Extend `MatrixReport::lane_failures()` at `waybill-cli/tests/corpus_harness_195/invariants/report.rs:158-179` to append one line per unmet entry, naming the invariant, the target, the observed outcome and the reason, sorted with the existing failures by the same `sort()` (FR-007, FR-012)
- [ ] T033 [US3] Mark registered cells in the grid in `MatrixReport::console()` at `waybill-cli/tests/corpus_harness_195/invariants/report.rs:206-223`: render a registered cell as `"  *{glyph}"` — two spaces, the mark, the cell's own outcome glyph — which keeps the four-column width the header's `" {:>3}"` sets and keeps the outcome legible. Mark **every** registered cell, met or not (FR-012, clarification 2)
- [ ] T034 [US3] Extend the legend at `waybill-cli/tests/corpus_harness_195/invariants/report.rs:199` with `* must-pass`, and add the unmet entries to `to_json()` at `:256-274` so the per-run artifact carries them
- [ ] T035 [US3] Remove the direct `unmet` calls added to the two lanes in T011 and T012, now that the report carries them — move the responsibility, do not leave two code paths that can disagree about what failed
- [ ] T036 [US3] Test in `report.rs`: an unmet entry appears in `lane_failures()` and in `console()`'s rendered failure list, alongside a verdict failure, sorted among them (SC-009)
- [ ] T037 [US3] Test in `report.rs`: a registered cell reporting `Pass` renders differently from an unregistered passing cell, and a registered cell reporting `Fail` still renders its own outcome glyph — the mark says the cell was committed to, not what it did (SC-016, US3 scenarios 2 and 3)
- [ ] T038 [US3] Test in `report.rs`: `build_for_target` — the `Scope::SingleTarget` constructor the corpus lane uses — reports an unmet entry in `lane_failures()` (FR-008a, SC-017). This is the only thing standing between the corpus lane and plan.md's first risk row: `unfalsifiable` is scope-gated three lines above `unmet` in `build_scoped`, the corpus lane is network-gated so the pre-PR gate never exercises it, and after T035 the report is the sole enforcement path. Hermetic — build the cells by hand, no network

**Checkpoint**: the grid distinguishes a load-bearing pass from an incidental one before anything breaks, and the per-target constructor still enforces the registry.

---

## Phase 5: Anchor presence — FR-019, closes #1230

**Goal**: the component each seeded reason leans on cannot leave the document
unnoticed. Independent of the registry — it could ship alone — but it is what
stops the registry recording a commitment it cannot keep: on three of the four
seeded targets the anchor can vanish while the cell still reports `Pass`
(research M1, M3).

- [ ] T039 [P] Assert the presence of `pkg:generic/java17?pants-namespace=jvm`, `pkg:generic/java21?pants-namespace=jvm` and `pkg:generic/pants-2.30?pants-namespace=python` in `pants_clojure_polyglot_expectations` at `waybill-cli/tests/corpus_harness_195/expectations.rs:786`, using the `cdx_has_component_purl` shape already used at `:988`. That entry's registry reason cites the four-anchor case as what the cell exercises, so all four must be guarded — and **write down what guards the fourth**, because two checks in this same function read like anchor coverage and neither is a presence assertion: (a) `resolve-anchor-reaches-pants` at `:852-873` resolves `pants-2.31` to a `bom-ref` and requires an edge to `pkg:pypi/pantsbuild-pants@2.31`, so it *does* fail when the anchor is absent, but under a `check_name` and a `suggested_action` about pypi dependency-name matching (#1100) — a reader whose anchor vanished is sent to the wrong investigation; (b) `ownership-covers-both-namespaces` at `:877-888` asserts the resolve-ownership annotation equals `["jvm:java17", "jvm:java21", "python:pants-2.30", "python:pants-2.31"]` — all four anchors by name, and still not a presence check, because the annotation comes from `pants.toml`'s resolves and every anchor component can leave with it green. The measurement probe missed (b) entirely: it matches PURLs, not ownership names
- [ ] T040 [P] Assert the presence of `pkg:generic/python-default?pants-namespace=python` in `pants_example_django_expectations` at `waybill-cli/tests/corpus_harness_195/expectations.rs:677`. Give the failure a `suggested_action` naming the Pants resolve classifier, and state in a comment that `pants-resolve-annotation-present` in the same function does **not** cover this — 34 components satisfy it on this target (research M3)
- [ ] T041 [P] Assert the presence of `pkg:generic/python-default?pants-namespace=python` in `pants_example_python_expectations` at `waybill-cli/tests/corpus_harness_195/expectations.rs:631`, same shape, same comment about the near-miss (11 components satisfy the annotation check here)
- [ ] T042 Prove the assertions bite: a test that removes the anchor component from a loaded golden and asserts the target's expectation function now returns `Err`. Cover **all seven** anchors, not only the five added — SC-014 says *any one* anchor, and the two already covered are the two nobody is editing, so they are the two nobody would otherwise check: removing `pants-2.31` must fail as `resolve-anchor-reaches-pants` (`expectations.rs:866`) and `jvm-default` as `jvm-default-owned` (`expectations.rs:987`), while the five new ones fail under their own `check_name`s. An assertion that cannot fail is the same trap one level further out — and these five are being added precisely because the existing ones passed over the gap
- [ ] T043 Re-run `python3 specs/1217-must-pass-registry/measurements/versionless-populations.py` and confirm the coverage table prints no `NO` rows (SC-014, first clause). The probe matches an anchor's PURL against the *text* of `expectations.rs`, so a `yes` row proves the PURL is named, not that the assertion asserts presence — T039's finding (a) is exactly how a `yes` can mean a reachability check. T042 is the clause that proves the assertions bite; neither task substitutes for the other

**Checkpoint**: both halves hold — the registry catches a cell that stops applying, the expectations catch the anchor leaving, and each is proven by a mutation rather than by a grep.

---

## Phase 6: Polish & cross-cutting

- [ ] T044 Delete `i8_applies_and_passes_on_each_named_pants_target` at `waybill-cli/tests/corpus_harness_195/invariants/mutation_tests.rs:452-466` (FR-017). Keep `PANTS_I8_TARGETS` at `:424` and the two mutation tests at `:476` and `:493` — they perturb the document and assert the cell goes red, which tests the checker, not the registry
- [ ] T045 Update the I8 section header of `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml:283-299`, which names the test T044 deletes as what holds those four cells. Name the registry instead; a comment pointing at a deleted test is worse than no comment (FR-017)
- [ ] T046 [P] Update the doc comment at `waybill-cli/tests/corpus_harness_195/invariants/no_new_io.rs:28-30`, which enumerates the permitted repository-file reads ("the catalog, the declarations file, the ecosystems doc and the committed goldens"), to name the registry. `std::fs::read` is already permitted so the gate passes either way — the comment is the thing that would be wrong (FR-016, feature 1156's T042)
- [ ] T047 [P] Mark feature 1156's SC-010 and SC-011 as met in `specs/1156-version-absence-marker/spec.md:358-371`, pointing at this feature, and update the deferral note at `specs/1156-version-absence-marker/tasks.md:113-131` plus the eleven `[DEFERRED — next spec]` task markers so the record says where they landed (FR-018)
- [ ] T048 [P] Add a short companion-files note to the preamble of `docs/reference/sbom-invariants.md:13-25`, beside the invariant/expectation table: the declarations file records cells known to be broken, the registry records cells whose `Pass` is load-bearing, a cell is in at most one, and neither is an invariant — which is why neither borrows the word
- [ ] T049 [P] Add a CHANGELOG.md entry under the test-infrastructure heading: the registry, the anchor assertions (#1230), and the stopgap's removal
- [ ] T050 Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` and confirm `GATE_RESULT=pass` plus the per-target `test result: ok. N passed; 0 failed` lines. No `+toolchain` argument — the pin in `rust-toolchain.toml` is what makes the gate match CI (#1189), and the constitution's Pre-PR Verification table still prints the pre-#1189 `+stable` form, which is a stale clause pending its own amendment, not an instruction to follow here (plan.md Constitution Check)
- [ ] T051 Confirm every success criterion by name: SC-001 through SC-017. SC-011 (no network) and SC-013 (clippy + test) are automatic under the gate, so name them here or nobody checks them deliberately; SC-014 is T042 plus T043's probe output; SC-017 is T038; SC-012 is that `verdict()`'s signature and its eight-row tests are untouched — verify by `git diff` on `mod.rs:293-315`, not by assertion

---

## Dependencies

```text
Phase 1 (T001-T006)  ── blocking for everything
      │
      ├─ Phase 2 / US1 (T007-T017)  ── the mechanism + its teeth
      │        │
      │        ├─ Phase 3 / US2 (T018-T030)   needs validate() + load_at() from US1
      │        └─ Phase 4 / US3 (T031-T038)   needs unmet() from US1; T035 removes US1's
      │                                        direct calls, which is why T038 exists
      │
      └─ Phase 5 (T039-T043)  ── independent of all of the above; could ship alone

Phase 6 (T044-T051)  ── T044/T045 need Phase 2 (the registry must exist before
                        the stopgap it replaces is deleted); T050/T051 last
```

**Story independence**: US1 stands alone. US2 extends US1's validator with
demonstrations. US3 moves US1's lane calls into the report — the only
cross-story edit, which is why T035 is explicit about moving rather than
duplicating and why T038 pins what the move puts at risk. Phase 5 touches a
different file entirely and shares no code.

## Parallel execution

- **T016, T017** after T008 — different test functions, same file, independent assertions.
- **T018-T024** together — seven rule tests, one per rule, no shared state.
- **T039, T040, T041** together — three different expectation functions.
- **T046, T047, T048, T049** together — four different files, all documentation.

Tasks in the same file are marked `[P]` only where they add independent
functions; two tasks editing the same function are never both `[P]`.

## Implementation strategy

**MVP is Phase 1 + Phase 2.** That is a working registry with the four cells
recorded and the mutation proving the trap fires — the hole closed for every
cell anyone chooses to record. Phase 3 hardens the file against being
malformed, Phase 4 makes a breach diagnosable at a glance and pins the
per-target path, Phase 5 closes the complementary gap the planning measurement
found.

**Suggested commit split** (each independently green, per the project's
one-change-per-commit posture):

1. Phases 1-2 — the registry and its teeth.
2. Phase 3 — the validation demonstrations.
3. Phase 4 — the report, the grid, and the scope test.
4. Phase 5 — the anchor assertions (#1230).
5. Phase 6 — reconciliation, docs, changelog.

Run the gate on each in isolation rather than once at the end; a bisect
through this branch should land on a commit that builds.
