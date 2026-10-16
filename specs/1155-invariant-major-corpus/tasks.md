---

description: "Task list for the universal semantic-invariant layer"
---

# Tasks: Universal Semantic-Invariant Layer for the Public Corpus

**Input**: Design documents from `/specs/1155-invariant-major-corpus/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included, because the spec requires them as acceptance criteria rather than as optional extras — SC-004 (mutation per invariant), SC-007 (regression verification), SC-009/SC-010 (no vacuous passes, applicability floor) are success criteria, and FR-004/FR-007 describe behaviour only a test can pin.

**Organization**: Grouped by user story. US1 alone is a usable MVP — a diagnostic matrix you can run and read. US2 turns it into a gate.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US4 per spec.md
- Paths are repository-relative, per plan.md's Source Code layout

## Path Conventions

All harness code lives under `waybill-cli/tests/corpus_harness_195/`,
abbreviated **`H/`** below. The catalog is `docs/reference/sbom-invariants.md`.
`target/` is already gitignored, so the run artifact needs no ignore entry.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the module skeleton and the two persisted artifacts so later tasks have somewhere to land.

- [X] T001 Create `H/invariants/mod.rs` with an empty public surface and wire `mod invariants;` into `H/mod.rs`
- [X] T002 Create `H/invariants/checks/mod.rs` as an empty submodule registry, declared from `H/invariants/mod.rs`
- [X] T003 [P] Create the catalog skeleton at `docs/reference/sbom-invariants.md`: title, the five-column table header from `contracts/invariant-catalog.md`, and the numbering-rules section (append-only, retire-never-reuse)
- [X] T004 [P] Create `H/invariant-declarations.toml` with a header comment stating it holds non-passing cells only, sorted by `(invariant, target)`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types, the terminology rename, and the two design decisions research flagged as prerequisites. The rename is here rather than in US3 because deferring it means every new call site in US1/US2 gets written against a field name that will change; US3 keeps the judgement work of deciding *which assertions move*.

- [X] T005 Define `CellOutcome` (`Pass` / `Fail(Vec<Violation>)` / `NotApplicable { reason }` / `Error { message }`) and `Violation { kind, count, example }` in `H/invariants/mod.rs` per `data-model.md`
- [X] T006 Define the `Invariant` struct (number, statement, precondition, kinds, diagnostic, checker fn pointer) and an empty `REGISTRY: &[Invariant]` in `H/invariants/mod.rs`
- [X] T007 Rename `AssertionFailure.invariant_name` → `check_name` in `H/harness.rs` and update all 63 construction sites across `H/harness.rs`, `H/layer1_assertions.rs` and `H/layer2_golden.rs`
- [X] T008 Rename `H/layer1_assertions.rs` → `H/expectations.rs` and rename all 17 `*_layer1` functions to `*_expectations`, leaving `layer0_document_integrity` named as-is per spec Assumption 6
- [X] T009 Rename `CorpusTarget.layer1` → `expectations` in `H/manifest.rs` and update the call site at `waybill-cli/tests/public_corpus.rs:79`
- [X] T010 Add `EcosystemId` (one variant per row of `docs/ecosystems.md`'s coverage matrix — 17 at time of writing) and `reader_ecosystems: &[EcosystemId]` to `CorpusTarget` in `H/manifest.rs`, populated for all 17 targets from each target's actual manifest sources. Leave the existing `ecosystem: Ecosystem` field and `cross_ecosystem_coverage_check` (`manifest.rs:606`) untouched — different axis, see `data-model.md`; a comment at each field states which is which
- [X] T011 Resolve FR-019: restate I3 and I7 so every defect satisfies the precondition of exactly one, then record both statements and preconditions in `docs/reference/sbom-invariants.md`. The measured overlap is that I3 seeds only from already-marked components, so a never-marked population falls through to I7 — see `measurements/findings.md`
- [X] T012 Settle each of the seven invariants' violation kinds against FR-012a and record them in the catalog, resolving I2's axis in particular (candidate: component type, so `duplicate-file-identity` and `duplicate-library-identity` are distinct defects)

**Checkpoint**: `cargo test -p waybill --test public_corpus` compiles and the existing corpus behaviour is unchanged — the renames are mechanical and must not alter any outcome.

---

## Phase 3: User Story 1 — One invariant, every target, one run (Priority: P1)

**Goal**: A maintainer writes a property once and sees its state for every corpus target in a single run. Report-only at the end of this phase: the matrix prints and is written, but does not yet fail the lane.

**Independent Test**: Add an invariant known to hold on some targets and fail on others, run the corpus lane, and confirm the output names that invariant's state for every target including all failures — not just the first.

### Driver

- [X] T013 [US1] Implement the evaluation driver in `H/invariants/mod.rs`: every `REGISTRY` invariant × every `manifest::TARGETS` entry → `Vec<Cell>`, collecting all outcomes rather than short-circuiting (FR-002, FR-004)
- [X] T014 [US1] Wrap each checker call in `std::panic::catch_unwind` so a panicking checker yields `CellOutcome::Error` and the remaining cells still evaluate (FR-007)
- [X] T015 [US1] Compute per-invariant applicability counts (targets whose outcome is not `NotApplicable`) in the driver (FR-015a)

### Checkers — one file each, fully parallel

- [X] T016 [P] [US1] I1 in `H/invariants/checks/file_tier_edges.rs` — no dependency edge targets a file-tier component; precondition is ≥1 file-tier component present
- [X] T017 [P] [US1] I2 in `H/invariants/checks/identity_unique.rs` — `(name, version, type)` identifies at most one component; kinds split by component type per T012
- [X] T018 [P] [US1] I3 in `H/invariants/checks/scope_propagation.rs` — a component reachable only via non-runtime paths carries a non-runtime marker, using the statement settled in T011
- [X] T019 [P] [US1] I4 in `H/invariants/checks/root_edge_parity.rs` — CDX and SPDX agree on the root's direct set. **Must handle SPDX inverse relationship types** (`DEV_/OPTIONAL_/TEST_/BUILD_/PROVIDED_DEPENDENCY_OF`, which place the root in `relatedSpdxElement`); ignoring them fabricates mismatches — `jellyfin-web` alone carries 2655 `DEV_DEPENDENCY_OF`
- [X] T020 [P] [US1] I5 in `H/invariants/checks/completeness_honesty.rs` — a `complete` verdict implies nothing was excluded; precondition is verdict == `complete`
- [X] T021 [P] [US1] I6 in `H/invariants/checks/version_sentinels.rs` — no package appears under several unknown-version spellings (`0.0.0-unknown`, `unresolved`, absent)
- [X] T022 [P] [US1] I7 in `H/invariants/checks/orphan_explained.rs` — every component is reachable from the subject or carries a marker explaining why not, using the statement settled in T011
- [X] T023 [US1] Register all seven in `REGISTRY` in `H/invariants/mod.rs`, each with its `diagnostic` text, and write the matching `### I<n>` diagnostic section for each in `docs/reference/sbom-invariants.md` per `contracts/invariant-catalog.md` — the class of defect it catches and what to do when it fires (FR-005). This is the `suggested_action` prose the per-target layer already carries; write it rather than deferring it to T052's migration

### Report

- [X] T024 [US1] Console table in `H/invariants/report.rs`: the grid, with failures and declarations ahead of passes (FR-014)
- [X] T025 [US1] JSON artifact at `target/invariants/run-<waybill-sha>.json` from `H/invariants/report.rs`, with `cells` sorted by `(invariant, target)` so two runs diff cleanly (FR-014a, SC-003)
- [X] T026 [US1] Include `applicability`, `ecosystems_covered` and `ecosystems_uncovered` fields in the artifact shape now, leaving the ecosystem values empty until US4 fills them
- [X] T027 [US1] Wire the driver and report into `waybill-cli/tests/public_corpus.rs` alongside layer 0 and the per-target expectations, **report-only** — it must not fail the lane in this phase

### Tests for US1

- [X] T028 [P] [US1] Unit test in `H/invariants/mod.rs` tests: a driver run over two synthetic failing cells reports both, proving FR-004's no-short-circuit behaviour
- [X] T029 [P] [US1] Unit test: `NotApplicable` is never reported as `Pass` for a document whose precondition is unmet, for each of the seven invariants (SC-009)
- [X] T030 [P] [US1] Unit test: a deliberately panicking checker yields `Error` for its cell while every other cell is still evaluated (FR-007)
- [X] T031 [US1] **Mutation check, seven cases (SC-004)**: for each invariant, take a target whose cell passes, perturb the emitted document so the property is violated, and assert the cell goes red with the expected kind. An invariant that cannot be made to fail checks nothing — this is the task that makes the passing cells evidence. Keep both matrix artifacts for one of the seven and record the fail→pass cell diff in `measurements/findings.md` — that diff is SC-003's demonstration
- [X] T032 [US1] Run the full corpus lane and capture the first real matrix over all 17 targets; record the actual cell states in `measurements/findings.md` under a new section. The spec's 12 known failures come from nine Pico pairs, so **8 of the 17 targets have no measured state** and this task is where they get one

**Checkpoint**: running the corpus lane prints a 7 × 17 matrix and writes the artifact. US1 is independently useful here — it answers "what is this defect's blast radius" without being a gate yet.

---

## Phase 4: User Story 2 — A carve-out is a visible row, not a sentence (Priority: P1)

**Goal**: Deferred cells are declared with written reasons; acquiring a new gap silently becomes impossible.

**Independent Test**: Declare one failing cell with a reason and confirm the lane passes; remove the reason and confirm the lane fails naming that cell.

### Catalog gate

- [X] T033 [US2] Implement the catalog parser in `H/invariants/catalog.rs`, reading the table from `docs/reference/sbom-invariants.md` in the style of `catalog::parse_mapping_doc`
- [X] T034 [US2] Bidirectional gate test in `H/invariants/catalog.rs`: active rows without registry entries fail, registry entries without rows fail, a registry entry for a `Retired` row fails, duplicate or non-ascending IDs fail, an active row with empty kinds or precondition fails, and an active row with no `### I<n>` diagnostic section fails (FR-008, FR-001c, FR-005, FR-012a, mirroring `parity/extractors/mod.rs:922`)
- [X] T035 [US2] Assert kind-set agreement: a checker may only emit kinds its catalog row lists

### Declarations

- [X] T036 [US2] Define `Declaration` and the TOML loader in `H/invariants/declarations.rs` per `contracts/declarations.md`
- [X] T037 [US2] Validate declarations on load: unknown or retired invariant, unknown target, duplicate `(invariant, target)`, and a kind absent from the catalog row all fail the lane
- [X] T038 [US2] Implement the cell verdict state machine in `H/invariants/mod.rs` — the full eight-row (outcome × declaration) table in `data-model.md`, including stale-declaration detection for both `Pass` and `NotApplicable` (FR-009, FR-011)
- [X] T039 [US2] Implement the kind-pinned comparison: a drifted count is accepted, a changed kind or an additional kind fails (FR-012, FR-012a)
- [X] T040 [US2] Implement the FR-006a lane rule: an invariant that is `NotApplicable` on every target fails the lane as unfalsifiable
- [X] T041 [US2] Flip `waybill-cli/tests/public_corpus.rs` from report-only to gating, so undeclared failures fail the lane
- [X] T042 [US2] Populate `H/invariant-declarations.toml` from T032's real matrix: one entry per failing cell with a written reason, cross-referencing #1206, #1207, #1208, #1209 and #1211 where applicable (FR-018)

### Tests for US2

- [X] T043 [P] [US2] Unit test: a declared cell that now passes fails the lane as stale (FR-011)
- [X] T044 [P] [US2] Unit test: an undeclared failure fails; the same failure with a declaration of matching kind passes even when the count has drifted (FR-012)
- [X] T045 [P] [US2] Unit test: a second violation kind appearing under an existing declaration fails the lane
- [X] T046 [P] [US2] Unit test: a newly added corpus target is evaluated against every invariant and must pass or be declared — it cannot inherit an exemption (FR-013)
- [X] T047 [P] [US2] Unit test: a declaration naming a retired or unknown invariant fails rather than being skipped (FR-001c)
- [X] T048 [P] [US2] Unit test: an invariant whose every cell is `NotApplicable` fails the lane (SC-010)
- [X] T049 [US2] Line-stability check (FR-014c): adding and then removing a declaration leaves every other entry's bytes unchanged, verified by diffing the file across both operations
- [X] T050 [US2] Regression verification (SC-007): move a passing cell to failing without declaring it and confirm the lane fails naming the invariant, the target and the observation

**Checkpoint**: the matrix is now a gate. A scoped fix is a declarations diff, and the five issues from this investigation are recorded as rows rather than prose.

---

## Phase 5: User Story 3 — Universal properties leave the per-target functions (Priority: P2)

**Goal**: A reviewer can tell from a check's name whether it asserts a property of any SBOM or an expectation about one target.

**Independent Test**: Take one assertion duplicated across two or more expectation functions, move it to the catalog, and confirm both targets' outcomes are unchanged while the expectation functions shrink.

- [X] T051 [US3] Audit all 17 functions in `H/expectations.rs` and classify every assertion as a universal property or a target-specific expectation; record the classification as a table in `measurements/findings.md`
- [X] T052 [US3] Migrate each assertion appearing in two or more expectation functions into the catalog as a new invariant, allocating numbers append-only and **preserving its `suggested_action` prose verbatim** (FR-005, FR-020) — **migration set is empty, and that is the finding.** All three repeated names are target-specific on inspection; `pants-resolve-annotation-present` looks universal but cannot be generalised without its precondition collapsing into its conclusion. Measured and recorded in `measurements/findings.md`. The universal properties that *were* in the file are layer 0's three, which already run for every target
- [X] T053 [US3] Verify the originating targets' outcomes are unchanged after each migration, comparing matrix artifacts before and after — no migration happened (T052), so nothing to compare. The before/after artifact comparison itself is exercised by `mutation_tests::a_matrix_diff_shows_which_targets_a_change_reached`, which is SC-003's demonstration
- [X] T054 [US3] Confirm genuinely target-specific expectations stay in `H/expectations.rs` — `go-cobra`'s offline `partial` verdict and the nixpkgs moat checks among them — and add a short comment at each explaining why it is an expectation and not an invariant
- [X] T055 [US3] Verify FR-020a holds: every check in the harness is identifiable as invariant or expectation from its name alone, without reading its body

---

## Phase 6: User Story 4 — The matrix states what it cannot see (Priority: P3)

**Goal**: A green matrix is not mistaken for broad confidence.

**Independent Test**: Run the lane and confirm the output names at least one ecosystem waybill has a reader for that no corpus target exercises.

- [X] T056 [US4] Parse the ecosystem universe from the coverage matrix in `docs/ecosystems.md` in `H/invariants/report.rs`, reusing T033's table-parsing approach, and gate it bidirectionally against `EcosystemId`: a matrix row with no variant fails, a variant with no row fails. Without that gate a doc rename quietly shrinks the covered set instead of reddening the lane
- [X] T057 [US4] Compute covered and uncovered sets from `CorpusTarget.reader_ecosystems` against that universe and fill the artifact fields stubbed in T026 (FR-015)
- [X] T058 [US4] Print the coverage statement in the console report beneath the matrix, so a green grid always carries its caveat
- [X] T059 [US4] Verify SC-005: the uncovered set is non-empty and names `gem`, which is why #1211 was unobservable — no invariant over the existing 17 targets could have caught a gem defect

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T060 [P] Follow `quickstart.md` end to end by adding a throwaway invariant, declaring its failures, then removing it; fix any step the document gets wrong
- [X] T061 [P] Cross-link the two catalogs: reference `docs/reference/sbom-invariants.md` from `docs/reference/sbom-format-mapping.md` and vice versa, so the gates are discoverable together
- [X] T062 Run the mandatory pre-PR gate per `CLAUDE.md` — `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo "EXIT=$?"` — and assert on the `>>> all pre-PR checks passed.` line plus the per-target `test result: ok.` lines, not on the absence of the word FAILED
- [X] T063 Confirm `scripts/pre-pr.sh` is unchanged and the matrix still runs only in the dispatched corpus lane (spec Assumption 3)
- [X] T064 Confirm no emitted SBOM changed: no golden regeneration should be required by this feature at any point

---

## Dependencies

```
Phase 1 Setup
    ↓
Phase 2 Foundational  ← blocks everything; T011 and T012 block T018/T022 and T017 specifically
    ↓
Phase 3 US1 (P1)  ── report-only matrix ── MVP ends here
    ↓
Phase 4 US2 (P1)  ── needs T032's real matrix for T042
    ↓
Phase 5 US3 (P2)  ── needs the catalog + gate to migrate into
    ↓
Phase 6 US4 (P3)  ── needs T010's reader_ecosystems field and T026's stubbed fields
    ↓
Phase 7 Polish
```

**Hard orderings worth calling out**:

- **T011 before T018 and T022.** Writing the I3 and I7 checkers before their overlap is resolved bakes in the misattribution that sent one defect class to the wrong row.
- **T012 before T017.** I2's kinds decide whether a declaration can absorb a second, different duplication defect.
- **T032 before T042.** The declarations file's initial contents cannot be authored from the spec; they must come from a real run, and 8 of 17 targets are unmeasured.
- **T031 before relying on any passing cell.** Until the mutation check runs, the matrix's passes are not evidence.

**Story independence**: US1 stands alone as a diagnostic. US2 depends on US1's driver. US3 and US4 each depend only on Foundational plus US1, not on each other.

---

## Parallel Execution Examples

**Phase 1** — T003 and T004 are different files, independent of T001/T002.

**Phase 3, the checkers** — the largest parallel block in the feature. Seven files, no shared state, each depending only on Phase 2:

```
T016 file_tier_edges.rs     T017 identity_unique.rs
T018 scope_propagation.rs   T019 root_edge_parity.rs
T020 completeness_honesty.rs T021 version_sentinels.rs
T022 orphan_explained.rs
```

T023 (registry) joins them, so it is the barrier.

**Phase 3, tests** — T028, T029, T030 are independent of one another.

**Phase 4, tests** — T043 through T048 are six independent cases over the verdict machine.

**Phase 7** — T060 and T061 are independent.

---

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + Phase 3.** That delivers a runnable 7 × 17
matrix, report-only. It is immediately useful for the question that
motivated the feature — given a defect, which ecosystems have it — and
it is the increment that converts "read 17 documents by hand" into one
run. It is explicitly *not* a gate yet, so it cannot block anyone while
the initial 12-plus failures are still being triaged.

**Second increment = Phase 4.** The matrix becomes a gate, and the
deferred cells become declared rows with reasons. After this, a scoped
fix is a declarations diff and a new gap cannot be acquired silently.
Phases 1–4 together satisfy every P1 requirement.

**Then Phase 5 and Phase 6**, in either order. US3 is cleanup that makes
the harness legible; US4 is the honesty caveat on a green grid. Neither
blocks the other.

**Deliberately out of scope**, per spec Assumption 1: fixing the twelve
failing cells. Those belong to #1206, #1207, #1208, #1209 and #1211.
Landing the visibility mechanism and its fixes together would make it
impossible to attribute any golden movement to one or the other.

**Also out of scope**: the stale "all nine ecosystems Waybill supports"
sentence opening `docs/ecosystems.md`, found during Phase 0 research
while looking for an ecosystem enumeration. T056 parses that document's
table rather than its prose, so the feature is not blocked — but the
sentence is a user-facing docs defect and wants its own issue.
