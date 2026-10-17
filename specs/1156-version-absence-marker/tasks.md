# Tasks: a marker for a version that was never there to find

**Input**: Design documents from `/specs/1156-version-absence-marker/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: REQUIRED, not optional. The spec's central requirement is a
positive control (FR-012) plus mutation evidence (FR-013, SC-004, SC-011).
A test task here is deliverable, not scaffolding.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 from spec.md
- Paths are repository-relative

## Delivery boundary

FR-017 splits this into two independently-green changes. **T004–T019 are
change one** (emission); **T020–T042 are change two** (acceptance). Neither
may depend on the other to be green (FR-018). The boundary is load-bearing
and marked again at the phase heads.

---

## Phase 1: Setup — establish the baseline before changing it

**Purpose**: `mutation_tests.rs` opens with the rule this phase enforces —
assert the baseline explicitly rather than assuming it. Three of this
repo's recorded incidents were a green run that measured nothing.

- [X] T001 Run `cargo test -p waybill --test public_corpus` and record the five I8 failing cells verbatim, confirming `pants-clojure-polyglot` (4), `pants-example-django` (1), `pants-example-jvm` (1), `pants-example-python` (1), `image-postgres16` (1)
- [X] T002 [P] Confirm the corpus goldens are current by checking the latest `public-corpus.yml` nightly is green (`gh run list --workflow=public-corpus.yml --limit 3`); if it is red, stop — FR-015's "diff confined to the marker" is not checkable against drifted goldens
- [X] T003 [P] Record the seven anchors' `bom-ref` values from the four goldens with a property-aware filter, not a substring grep: `jq '.components[] | select(.properties[]? | select(.name == "waybill:component-kind" and .value == "lockfile-resolve")) | ."bom-ref"'`. A plain `grep lockfile-resolve` also matches `npm-express`, where C151's value string contains "no lockfile-resolved version". After T018, re-run this filter and assert each anchor's `waybill:version-absence` is a single scalar string — not an array, not a joined string. `pants-clojure-polyglot` carries four anchors and is where a dedup bug would multiply the value (data-model §1); T032's exact-equality acceptance would catch it only incidentally

**Checkpoint**: the blast radius is 7 components in 4 targets, measured rather than quoted.

---

## Phase 2: Foundational — the shared constants

**Purpose**: both readers must emit the same key and value. A literal in
each reader is two places to drift.

- [X] T004 Add `pub(crate) const VERSION_ABSENCE_KEY: &str = "waybill:version-absence";` and `pub(crate) const BY_CONSTRUCTION: &str = "by-construction";` to `waybill-cli/src/scan_fs/package_db/pants_resolve.rs`, beside the existing `ANNOTATION_KEY` (:36) and `NAMESPACE_KEY` (:310), with a doc comment stating the meaning and the C151 mutual exclusion

**Checkpoint**: both readers have one source for the pair.

---

## Phase 3: User Story 1 — a consumer can tell "no version exists" from "resolution failed" (Priority: P1)

**Goal**: the marker reaches all three emitted formats on every Pants
resolve anchor.

**Independent test**: scan a Pants repo with a Python and a JVM resolve;
each anchor carries the marker with an identical value in all three
documents, and no other component carries it.

**CHANGE ONE — must be green with all five I8 declarations intact.**

### Emission

- [X] T005 [US1] Insert `VERSION_ABSENCE_KEY` → `BY_CONSTRUCTION` into the `extra_annotations` map in `resolve_component_entry` at `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs:382`, beside the existing `waybill:component-kind` insert
- [X] T006 [US1] Same insert in `resolve_component_entry` at `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs:495`
- [X] T007 [P] [US1] Unit test in `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs` tests module: a Python resolve anchor entry carries the key with value `by-construction` and an empty `version`
- [X] T008 [P] [US1] Unit test in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs` tests module: the JVM anchor carries the identical key and value, proving namespace-independence (FR-002)
- [X] T009 [P] [US1] Unit test asserting a non-anchor `PackageDbEntry` from the same reader does NOT carry the key, so absence stays byte-identical to pre-feature output on packages
- [X] T009a [P] [US1] Unit test for the C151 mutual exclusion in `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs` tests module: a design-tier entry carrying `waybill:unresolved-reason` does NOT also carry `waybill:version-absence`. This is US1 acceptance scenario 3 and the data-model's first invariant — the distinction the whole feature exists to create. T009 is adjacent but weaker: it proves a *package* lacks the marker, not that a *failed resolution* lacks it

### Catalogue and parity

- [X] T010 [US1] Add row C197 to `docs/reference/sbom-format-mapping.md` after C196 (the verified high-water mark), copying the column shape of C160 and pasting the Principle V audit from `contracts/c197-version-absence.md`, including the "future values may extend the enum without new C-rows" clause (FR-003)
- [X] T011 [P] [US1] Add `cdx_anno!(c197_cdx, "waybill:version-absence", component);` to `waybill-cli/src/parity/extractors/cdx.rs`, matching `c160_cdx` at :969
- [X] T012 [P] [US1] Add `spdx23_anno!(c197_spdx23, "waybill:version-absence", component);` to `waybill-cli/src/parity/extractors/spdx2.rs`, matching `c160_spdx23` at :668
- [X] T013 [P] [US1] Add `spdx3_anno!(c197_spdx3, "waybill:version-absence", component);` to `waybill-cli/src/parity/extractors/spdx3.rs`, matching `c160_spdx3` at :752
- [X] T014 [US1] Add the `ParityExtractor { row_id: "C197", label: "waybill:version-absence", cdx: c197_cdx, spdx23: c197_spdx23, spdx3: c197_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false }` row to `EXTRACTORS` in `waybill-cli/src/parity/extractors/mod.rs`, and add `c197_cdx`, `c197_spdx23`, `c197_spdx3` to the three `use` lists at :63, :74 and :85
- [X] T015 [US1] Run `cargo test -p waybill parity::extractors` and confirm `every_catalog_row_has_an_extractor` and `holistic_parity` pass — both directions are gated, so a row without extractors fails here

### Goldens

- [ ] T016 [US1] Dispatch the regen: `gh workflow run "Public corpus regression" -f branch=1156-version-absence-marker -f regen_goldens=true`. Use `-f branch=`, NEVER `--ref` — `--ref` silently runs against main and reports success (#1223). Budget ~5 min warm (measured: last three runs 5m04s / 5m02s / 4m36s)
- [ ] T017 [US1] Download the artifact and review with `cargo run -p xtask -- corpus-diff` before committing. The review tool SORTS ARRAYS; the lane compares byte-for-byte after masking and does not tolerate reordering, so a clean review can still fail the lane — if it does, the fix belongs in `mask_nondeterministic`, not the review tool
- [ ] T018 [US1] Commit exactly 12 files under `waybill-cli/tests/fixtures/public_corpus/{pants-clojure-polyglot,pants-example-django,pants-example-jvm,pants-example-python}/{cdx,spdx-2.3,spdx-3}.json`, verifying the only change within them is the marker on the 7 anchors (FR-015, SC-006). Any other difference is a finding to explain, not to absorb
- [ ] T019 [US1] Dispatch the verify run with `-f regen_goldens=false`, then run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Confirm `invariant-declarations.toml` is untouched and all five I8 rows still report `AcceptedKnownFailure` — the cells have not moved, which is what makes change one shippable alone

**Checkpoint**: CHANGE ONE COMPLETE. The marker is on the wire, nothing reads it yet, the lane is green.

---

## Phase 4: User Story 2 — the explanation is proven real, not vacuously accepted (Priority: P2)

**Goal**: I8 accepts the marker, the four declarations are gone, and a
committed registry makes it impossible for those cells to go quiet.

**Independent test**: the four cells report `Pass`; forcing any one to
`NotApplicable` fails the lane and names it.

**Dependency, stated honestly**: this phase requires change one's goldens
to be on the default branch. US1 is independently shippable; US2 is not
independently shippable *before* US1, and the spec does not claim it is.

**CHANGE TWO.**

### Acceptance

- [ ] T020 [US2] Extend `is_explained` in `waybill-cli/tests/corpus_harness_195/invariants/checks/versionless_explained.rs:64` to accept `cdx_prop(component, "waybill:version-absence") == Some("by-construction")` — keyed to the pinned value, not the key's presence (FR-006)
- [ ] T021 [US2] Rewrite the doc comment above `is_explained` in the same file. It currently argues that a resolve anchor is NOT explained and that widening the set is the wrong move; that reasoning was correct before the marker existed and is wrong after it. State instead why `component-kind` remains rejected (FR-009) and why this marker is not it
- [ ] T022 [US2] Add the fifth accepted annotation to the loop in `i8_does_not_flag_explained_absences_or_sentinel_versions` in `waybill-cli/tests/corpus_harness_195/invariants/mutation_tests.rs:283`, which enumerates the accepted set as `("mutation-tier-file", …)` tuples
- [ ] T023 [US2] Rewrite the note at `docs/reference/sbom-invariants.md:181`. It says `component-kind` is not accepted and "the fix is a marker that states the anchor is versionless" — the fix now exists, so the note must name C197 and keep the `component-kind` rejection. Prose outside the fields `catalog.rs` parses, so untested; FR-008 requires it anyway because it pre-specifies the fix and is the first thing a reader checks
- [ ] T024 [US2] Delete the four I8 declarations for `pants-clojure-polyglot`, `pants-example-django`, `pants-example-jvm` and `pants-example-python` from `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`, keeping `image-postgres16` (FR-011). Update the I8 section's header comment, which currently describes two populations in the present tense
- [ ] T025 [US2] Run the matrix and confirm the four cells now report `Pass` and the lane is green with one I8 declaration remaining (SC-001, SC-002, SC-003)

### The must-pass registry — **DEFERRED to a follow-up spec**

**Decision, recorded because the fallback must not be silent.** The
registry is general machinery closing the per-cell vacuity hole for all
136 cells, seeded with four. It is larger than the acceptance change it
was bundled with, and the maintainer settled it on 2026-10-17: the
registry stays in scope for the project and moves to its own spec.

What that leaves: FR-012's literal requirement is satisfied by T032
alone — a positive control naming the four targets and asserting `Pass`
rather than merely not-fail. **SC-010 and SC-011 are deferred with it**,
and so is the only thing that would catch the same vacuity on any *other*
cell. The trap stays armed everywhere except these four, which is the cost
of the split and the reason the follow-up is not optional.

The tasks below are the follow-up's input, kept rather than deleted so the
decision has a record. A draft implementation exists and is not committed:
`must_pass.rs` with its six validation rules, the line-stability pair, the
`NotApplicable` mutation (SC-011), and the seeded `invariant-must-pass.toml`
whose four reasons were written against the measured corpus — including
`pants-example-django`, the only target carrying both the C151 and C197
populations.

- [ ] T026 **[DEFERRED — next spec]** [US2] Create `waybill-cli/tests/corpus_harness_195/invariants/must_pass.rs` with `MustPass { invariant, target, reason, issue }`, `MustPassFile { must_pass: Vec<MustPass> }`, `must_pass_path()` and `load()`, mirroring `declarations.rs` field for field so a reviewer reading one can read the other
- [ ] T027 **[DEFERRED — next spec]** [US2] Implement `validate(&MustPassFile, &DeclarationsFile) -> Vec<String>` in the same file with all six rules from `contracts/must-pass-registry.md`: live invariant, known target, non-empty reason, no duplicate `(invariant, target)`, sorted ascending, and NO OVERLAP with the declarations file. All six run before any comparison — a partly-honoured file is an invisible exemption
- [ ] T028 **[DEFERRED — next spec]** [US2] Implement the lane pass in the same file with the shape its precedent uses — `pub fn unsatisfied(cells: &[Cell], file: &MustPassFile) -> Vec<String>`, mirroring `unfalsifiable_invariants(cells: &[Cell]) -> Vec<&'static str>` in `mod.rs:319`. For each entry, resolve the cell's outcome and fail on anything but `Pass`. The message must name the invariant, the target, the observed outcome AND the entry's reason — a bare "must-pass cell failed" sends the reader back to the file to learn what they broke
- [ ] T029 **[DEFERRED — next spec]** [US2] Add `mod must_pass;` to `waybill-cli/tests/corpus_harness_195/invariants/mod.rs` beside the other module declarations (:24–:36) and invoke the pass where the lane aggregates failures, as a sibling of `unfalsifiable_invariants` — do NOT add a third argument to `verdict()`, which stays the pure two-input eight-row state machine its own tests pin (research R2)
- [ ] T030 **[DEFERRED — next spec]** [US2] Surface must-pass failures in the matrix report via `waybill-cli/tests/corpus_harness_195/invariants/report.rs` (`build_scoped` at :133 already takes the declarations file), so an operator reading the matrix sees the failure rather than only a test name
- [ ] T031 **[DEFERRED — next spec]** [US2] Create `waybill-cli/tests/corpus_harness_195/invariant-must-pass.toml` seeded with the four Pants I8 cells, sorted by `(invariant, target)`, each `reason` answering "what would a `NotApplicable` here mean?" — not "it passes today", which records no judgement and is rejected by rule 3. Include a header comment explaining the file's relationship to the declarations file and why it is not named `invariant-expectations.toml` (**feature 1155's** FR-020/FR-020a terminology gate reserves "expectation" for a target-specific check; this feature's own requirements end at FR-018)

### Proof the gates have teeth

- [ ] T032 [P] [US2] Positive control in `mutation_tests.rs`: for each of `pants-clojure-polyglot`, `pants-example-django`, `pants-example-jvm`, `pants-example-python` NAMED LITERALLY, assert I8 reports `CellOutcome::Pass` and specifically not `NotApplicable`. Do not use `first_passing("I8")` — that anchor floats, so if all seven anchors vanished it would silently pick another target and stay green (FR-012, SC-001)
- [ ] T033 [P] [US2] Mutation in `mutation_tests.rs`: remove the `waybill:version-absence` property from exactly one anchor on one Pants target; assert I8 on that target fails with `KIND_VERSIONLESS_AND_UNEXPLAINED` (FR-013, SC-004)
- [ ] T034 [P] [US2] Mutation in `mutation_tests.rs`: replace one anchor's marker value with an unrecognised string; assert I8 fails — proving acceptance is keyed to the pinned value, not the key's presence (US2 scenario 3)
- [ ] T035 **[DEFERRED — next spec]** [US2] Mutation for the registry itself in `must_pass.rs`'s tests: force a registered cell to report `NotApplicable` and assert the lane fails and names the cell. This is SC-011 and the ONLY evidence the registry does anything — a registry that cannot fail is the same vacuity one level up
- [ ] T036 **[DEFERRED — next spec]** [P] [US2] Validation unit tests in `must_pass.rs` covering each of the six rules independently, including the overlap rule with a fixture entry present in both files (FR-012b)
- [ ] T037 **[DEFERRED — next spec]** [P] [US2] Line-stability tests in `must_pass.rs` mirroring `line_stability.rs`: inserting an entry sorts into position without moving any other entry's rendered content, and removing it restores the rendering exactly. Note that `line_stability.rs` tests the parsed model through a `render()` helper rather than the file's bytes — mirror that approach (FR-012d)
- [ ] T038 **[DEFERRED — next spec]** [US2] Test that the committed `invariant-must-pass.toml` is valid, mirroring `the_committed_declarations_file_is_valid` in `declarations.rs`

**Checkpoint**: the four cells pass, and the lane fails if they stop meaning anything.

---

## Phase 5: User Story 3 — an anchor that loses its edges is still explained (Priority: P3)

**Goal**: I7 accepts the marker, so a future edge-wiring change cannot turn
these seven components into a second defect class.

**Independent test**: a versionless, edgeless component carrying the marker
is not reported by I7 — demonstrable in a unit test with no corpus target
exhibiting it.

**CHANGE TWO.** Genuinely independent of US2: nothing in it touches the
registry or I8.

- [ ] T039 [US3] Extend `is_explained` in `waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs` to accept `waybill:version-absence` = `by-construction`, and document in the doc comment that this is forward-looking — the anchors are reachable today, so no current cell moves (FR-007)
- [ ] T040 [P] [US3] Unit test in `waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs` tests module: a synthetic component with no edges, an empty version and the marker is accepted by I7 as explained
- [ ] T041 [P] [US3] Test that I7's outcome is unchanged on all 17 targets relative to before this feature, confirming the arm adds acceptance without moving a cell (SC-007). All five current I7 declarations belong to #1214's population and must still report `AcceptedKnownFailure`

**Checkpoint**: I7 insured, nothing observable changed.

---

## Phase 6: Polish & Cross-Cutting

- [ ] T042 **[DEFERRED — next spec, with the registry]** [P] Update the doc comment in `waybill-cli/tests/corpus_harness_195/invariants/no_new_io.rs:28` which enumerates the permitted repository-file reads ("the catalog, the declarations file, the ecosystems doc and the committed goldens") to name the must-pass file too. `std::fs::read` is already permitted so the gate passes either way; the comment is the thing that would be wrong. Deferred because without the registry there is no must-pass file to name — the comment is correct as it stands
- [ ] T043 [P] Add a CHANGELOG.md entry per change: the emission change under Added (a new wire annotation is consumer-visible), the acceptance change under Changed or Fixed
- [ ] T044 Final verification: run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` on each change in isolation and confirm both are green independently (FR-018, SC-012). Checking the gate's own exit status, not a pipeline's
- [ ] T045 Confirm every success criterion: SC-001 (4 targets Pass), SC-002 (5→1 failing I8 cells), SC-003 (1 declaration), SC-005 (21 component records, one value), SC-006 (12 files), SC-008 (the `no_new_io` gate still passes and the matrix needs no network — automatic under `cargo test`, so name it here or nobody checks it deliberately), SC-009 (catalogue gate both directions). **SC-010 and SC-011 are deferred with the registry** and must be reported as deferred, not as met

---

## Dependencies

```
Phase 1 (T001-T003) ─ baseline, blocks nothing but informs everything
        │
Phase 2 (T004) ─ shared constants
        │
        ├──> Phase 3 / US1 (T005-T019) ─── CHANGE ONE, shippable alone
        │            │
        │            │ (goldens must be on the default branch)
        │            ▼
        │    Phase 4 / US2 (T020-T038) ─┐
        │                               ├─ CHANGE TWO
        └──> Phase 5 / US3 (T039-T041) ─┘
                     │
             Phase 6 (T042-T045)
```

- **US1 → US2**: real and unavoidable. US2's positive control asserts the
  four cells pass, which needs goldens carrying the marker.
- **US3 is independent of US2**: it touches only `orphan_explained.rs` and
  can be written and tested at any point after T004.
- **T020–T024 are sequential**: all four edits land in the acceptance change
  together, because T024 alone (deleting declarations before acceptance
  widens) fails the lane, and T020 alone (widening before deletion) makes
  the declarations stale, which also fails the lane. Neither half is green
  on its own; the pair is.

## Parallel opportunities

| Phase | Parallel set | Why safe |
|---|---|---|
| 1 | T002, T003 | read-only, different sources |
| 3 | T007, T008, T009, T009a | four reader tests; T009a shares a file with T007, so serialize those two edits |
| 3 | T011, T012, T013 | one macro line in each of three files |
| 4 | T032, T033, T034 | independent tests, same file — serialize the edits, parallelize the authoring |
| 4 | T036, T037 | independent test modules in `must_pass.rs` |
| 5 | T040, T041 | independent tests |
| 6 | T042, T043 | unrelated files |

T011–T013 are the cleanest fan-out: identical one-line edits in three
files, with T014 as the join.

## Implementation strategy

**MVP is US1 alone.** It delivers the entire consumer-visible value — an
SBOM that distinguishes "no version exists" from "resolution failed" — and
ships green with the declarations intact. If the harness work stalls, the
wire is still more honest than it was.

**US2 is where the scope actually grew, and the fallback was taken.** The
must-pass registry is general machinery closing the vacuity hole for all 136
cells, seeded with four. This file anticipated that T026–T031 might prove
larger than they look and named the fallback: T032 alone, a named-target
control with no registry, which still satisfies FR-012's literal
requirement. **That fallback was taken deliberately on 2026-10-17, by the
maintainer, with the registry moved to its own spec** — see the deferral
note above the registry tasks for what stays armed as a result. The
condition this file set for taking it was that it be deliberate and
recorded; both hold.

**Do not start T016 until T005–T015 are green locally.** A golden refresh
dispatched against incomplete emission produces goldens you will have to
regenerate, at five minutes a round trip plus review.
