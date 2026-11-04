---
description: "Task list for 1278-closure-structured-attrs"
---

# Tasks: a closure member's attributes are read wherever nix put them

**Input**: Design documents in `/specs/1278-closure-structured-attrs/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`
**Issue**: #1348

**Tests are included.** Not by default — because SC-010 requires a mutation
that proves the guard can fail, and because the repository's gate is the
acceptance criterion for every change here.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel — different file, no dependency on an incomplete task
- **[Story]**: US1 … US4, on user-story tasks only

## Path Conventions

Single Rust workspace, three crates. Everything here is in `waybill-cli`.

- Reader: `waybill-cli/src/scan_fs/package_db/nix/closure/`
- Emission: `waybill-cli/src/generate/`
- Parity: `waybill-cli/src/parity/extractors/`
- Catalog: `docs/reference/sbom-format-mapping.md`
- Tests: `waybill-cli/tests/`, corpus harness at `waybill-cli/tests/corpus_harness_195/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: the fixtures every later phase asserts against. Built from the
shapes measured in the real closure, not invented — a hand-guessed typed form
would test the reader against a shape nix does not emit.

- [X] T001 [P] Add `waybill-cli/tests/fixtures/nix_closure_structured_attrs/matched_pair.json`: two derivations declaring the same dependency, one through flat `env`, one through `structuredAttrs`, each with a distinct referenced member, modelled on the real `source` derivation shape recorded in `specs/1278-closure-structured-attrs/measurements/README.md`
- [X] T002 [P] Add `waybill-cli/tests/fixtures/nix_closure_structured_attrs/awkward_shapes.json` covering every case contract `attribute-resolution.md` P3/P4 names: an empty list, a boolean-valued field (`doCheck`), a nested object (`env`), a map (`outputChecks`), and a dependency field holding a single string rather than a list
- [X] T003 [P] Add `waybill-cli/tests/fixtures/nix_closure_structured_attrs/version_unknown.json`: a closure whose top-level `version` is a value the reader has never seen, with attributes in both shapes, for FR-013
- [X] T004 [P] Add `waybill-cli/tests/fixtures/nix_closure_structured_attrs/both_shapes_disagree.json`: one derivation carrying the same dependency field in both carriers with different values, for FR-006 — a case measured as absent from both real closures, so it only exists if engineered

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the seam. Nothing observable changes in this phase — the widened
reads exist and the read outcome is assembled, but no emitter consumes it yet.
Keeping it unobservable is what lets US1 be measured on its own.

**⚠️ CRITICAL**: blocks US1 through US4.

- [X] T005 Add the optional `structured_attrs` field to `RawDerivation` in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`, typed as an optional `serde_json::Value` because its members are heterogeneous (strings, lists, booleans, nested objects)
- [X] T006 Implement the resolution rule of `contracts/attribute-resolution.md` in `RawDerivation::paths_in` in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`: flat first, then typed array (skipping non-string members individually, never fatally), then typed string split on whitespace, then nothing — total over `Value`, no `.unwrap()` (Principle IV)
- [X] T007 Extend `RawDerivation::pname`, `declared_pname` and `version` in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs` with the same precedence, taking string values only and preserving the existing declared-name / own-name distinction
- [X] T008 Capture the closure's `version` field on `RawClosure` in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs` — parsed and dropped today, which is how the v3→v4 move went unnoticed (research R5)
- [X] T009 Add the `ReadOutcome` record per `data-model.md` in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`: format version, sorted set of unconsulted field names, declaration count, affected-derivation count — sorted and integer-valued so the emitted value is byte-stable
- [X] T010 Assemble `ReadOutcome` while reading the closure in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs` and thread it to the emission path alongside the existing closure result, without any emitter reading it yet
- [X] T011 [P] Unit-test the resolution rule against T001–T004 in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`'s test module, including the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard the crate requires

---

## Phase 3: User Story 1 — the role reflects every way nix references a member (Priority: P1) 🎯 MVP

**Goal**: a member referenced only through the typed form is classified as
referenced, and stops carrying a native marker claiming it is absent from the
artifact.

**Independent test**: the nine named components in SC-003 carry the corrected
role and no absent-from-artifact marker; the derivation census matches SC-004.

### Tests for User Story 1

- [X] T012 [P] [US1] Create `waybill-cli/tests/closure_structured_attrs.rs` with the crate-level attributes its sibling `waybill-cli/tests/closure_role_scope.rs` carries — `#![allow(clippy::unwrap_used)]`, and `#![cfg(unix)]` only if the tests genuinely need it, gating the whole file rather than shimming a non-unix early return — then test contract property P2 in it: the typed-form derivation of T001's matched pair resolves to the **same role** as its flat twin. Assert on the role, never on the parsed field — the store-path basename join is a second, silent failure, and a field-level assertion passes while the feature is broken
- [X] T013 [P] [US1] Test contract property P1 in `waybill-cli/tests/closure_structured_attrs.rs`: every existing flat-form fixture classifies byte-identically to `main`, pinned as a regression
- [X] T014 [P] [US1] Test contract property P3 in `waybill-cli/tests/closure_structured_attrs.rs`: an empty typed list credits no member. Load-bearing — `buildInputs: []` is on all 56 `source` derivations in the pinned closure, and crediting it would manufacture edges that read as an improvement
- [X] T015 [P] [US1] Test contract property P4 in `waybill-cli/tests/closure_structured_attrs.rs`: each awkward shape in T002 yields no paths and does not panic
- [X] T016 [P] [US1] Test contract property P6 in `waybill-cli/tests/closure_structured_attrs.rs`: T004's disagreement resolves from the flat map and is reported
- [X] T017 [P] [US1] Test FR-013 in `waybill-cli/tests/closure_structured_attrs.rs`: T003's unknown-version closure classifies exactly as a known-version one, and does not degrade the tier
- [X] T018 [US1] Add the CONTROL each classifier test needs, in `waybill-cli/tests/closure_structured_attrs.rs`: assert the fixture holds the derivations it claims and that something is referenced — a parse that produced nothing would satisfy every "not wrongly classified" assertion vacuously

### Implementation for User Story 1

- [X] T019 [US1] Confirm `classify` in `waybill-cli/src/scan_fs/package_db/nix/closure/classify.rs` needs no change because it reads through the accessors, and record that in its module doc — the seam is the design (FR-007), not an accident
- [X] T020 [US1] Verify that `assign_role_scope` in `waybill-cli/src/resolve/closure_role_scope.rs` asserts no scope for a corrected `artifact-input` or `both` role, and add the assertion if it rests on an unexercised path — FR-008 follows from m1277's existing rule and must not become a new one
- [X] T021 [US1] Update the `nix-closure-moat` expectations in `waybill-cli/tests/corpus_harness_195/expectations.rs` for the corrected role census, the nine components of SC-003, and SC-005's transition table **per direction** (5 `build-tooling`→`both`, 4 `unreferenced`→`artifact-input`, 3 `unreferenced`→`build-tooling`, 2 `artifact-input`→`both`), plus the assertion that nothing moves toward `unreferenced` — a total that matches by the wrong transitions must fail

**Checkpoint**: the nine false exclusions are retracted. US1 delivers value with nothing else landed.

---

## Phase 4: User Story 2 — patch evidence is complete (Priority: P2)

**Goal**: a derivation listing its patches in the typed form contributes them.

**Independent test**: the 28 patch entries that 7 derivations declare in the typed form are present, and the distinct-CVE count does not fall.

### Tests for User Story 2

- [X] T022 [P] [US2] Test in `waybill-cli/tests/closure_structured_attrs.rs`: a typed-form `patches` list produces one patch record per listed patch against the right component
- [X] T023 [P] [US2] Test in `waybill-cli/tests/closure_structured_attrs.rs`: a CVE-naming filename in a typed-form patch list reaches the distinct-CVE record

### Implementation for User Story 2

- [X] T024 [US2] Confirm `patches.rs` in `waybill-cli/src/scan_fs/package_db/nix/closure/` is corrected by T006 alone, and correct its module doc, which currently says it "reads `env.patches`"
- [X] T025 [US2] Update the patch and CVE expectations for `nix-closure-moat` in `waybill-cli/tests/corpus_harness_195/expectations.rs`

**Checkpoint**: patch evidence complete; US1 unaffected.

---

## Phase 5: User Story 3 — a member that declares its identity typed still becomes a component (Priority: P2)

**Goal**: an identity declared only in the typed form yields a component.

**Independent test**: the 11 derivations whose name and version are typed-only are emitted (SC-008).

### Tests for User Story 3

- [X] T026 [P] [US3] Test in `waybill-cli/tests/closure_structured_attrs.rs`: a derivation declaring `pname` and `version` only in the typed form is emitted under that name and version
- [X] T027 [P] [US3] Test in `waybill-cli/tests/closure_structured_attrs.rs`: a derivation declaring a name in neither shape is still skipped — this story adds a source for the name, it does not invent one

### Implementation for User Story 3

- [X] T028 [US3] Confirm `emit.rs` in `waybill-cli/src/scan_fs/package_db/nix/closure/` is corrected by T007 alone, and that the component count change is confined to the identities of SC-006 and SC-008
- [X] T029 [US3] Update the component-count expectations for `nix-closure-moat` in `waybill-cli/tests/corpus_harness_195/expectations.rs`

**Checkpoint**: all three correctness stories independently functional.

---

## Phase 6: User Story 4 — the next shape change is loud, not silent (Priority: P3)

**Goal**: a partially-read closure says so — natively where the formats have a
word for it, and by annotation for the detail they do not.

**Independent test**: the disclosure is absent from a fully-read closure and
present with equal content in all three formats otherwise; removing the typed
reads makes the guard fail.

### Tests for User Story 4

- [X] T030 [P] [US4] Test SC-015 in `waybill-cli/tests/closure_structured_attrs.rs`: a document whose closure had an unconsulted declaration carries no native claim that its dependency relationships are complete. **This is the test for the defect research R1 found in the committed golden**, so it must fail before T032 lands
- [X] T031 [P] [US4] Test the emission-gating matrix of `contracts/read-outcome.md` in `waybill-cli/tests/closure_structured_attrs.rs`: absent without `--nix-closure`, absent when no closure was recorded (C190's case — a zero-valued disclosure would claim a closure was read where none was), absent when fully read, present when partially read

### Implementation for User Story 4

- [X] T032 [US4] Add the partial read as a demotion input to `dependency_claims` in `waybill-cli/src/generate/cyclonedx/compositions.rs`, affecting **both** claims it produces: the ecosystem claim over nix-derived components and the root dependency claim that `target_aggregate` derives from trace integrity — the latter being the record measured as false today (research R2)
- [X] T033 [US4] Confirm SPDX 3's `Relationship.completeness` follows from the same helper in `waybill-cli/src/generate/spdx/`, adding no second demotion path — a parallel path would reintroduce the cross-format drift m1069 removed
- [X] T034 [US4] Emit the disclosure annotation in all three formats from `waybill-cli/src/generate/`, carrying the fact as well in SPDX 2.3, whose schema has no completeness field at all
- [X] T035 [US4] Add the catalog row to `docs/reference/sbom-format-mapping.md` (C204, confirming it is still the next free number) with the JSON-object shape of `contracts/read-outcome.md` and the per-row Principle V audit
- [X] T036 [US4] Add the three parity extractors in `waybill-cli/src/parity/extractors/` **in the same change as T035** — a documented row with no extractor fails `every_catalog_row_has_an_extractor`, so the two are not separable
- [X] T037 [US4] Implement the unconsulted-field detector feeding `ReadOutcome`, reporting every store-path-holding field classification does not consult, not only the seven it does (FR-014), in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs`
- [X] T038 [US4] Test SC-014 in `waybill-cli/tests/closure_structured_attrs.rs`: every member's role is identical with and without the T037 diagnostic. This is what keeps SC-004's census attributable to the shape fix alone
- [X] T039 [US4] Test SC-010 in `waybill-cli/tests/closure_structured_attrs.rs` as a **mutation**: with the typed-form reads removed, the guard goes red. A guard that only passes after the fix is no evidence it can fail
- [X] T051 [P] [US4] Test SC-012 and FR-012 in `waybill-cli/tests/closure_structured_attrs.rs`: the disclosure records the format version the closure was read from, asserted on the emitted value and not inferred, covering US4's third acceptance scenario. Runs with T031; numbered last so no existing task ID moved

**Checkpoint**: the disclosure is live and proven able to fail.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T040 [P] Record the corrected census against the committed one per FR-011 in `specs/1278-closure-structured-attrs/measurements/README.md`, so the figures in the earlier closure milestones' research can be read as what they were measured through
- [X] T041 [P] Per FR-011, give the census its system where it lacks one: `specs/1035-nix-closure-sbom/research.md` R3 states its figures without naming a system, now recorded as aarch64-darwin. **`waybill-cli/tests/corpus_harness_195/manifest.rs` needed no change** — it already states its system and warns the counts differ from the CI closure; the claim that it was mislabelled was wrong and is retracted in `measurements/findings.md` F0
- [X] T042 Run the local gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with **no `+toolchain` argument** (#1189). Assert on `GATE_RESULT`, the expect line, and the per-target `test result: ok. N passed; 0 failed` lines — the absence of FAILED is not evidence
- [X] T043 Satisfy FR-016's first half: commit the refresh prediction **before** dispatching it, in `specs/1278-closure-structured-attrs/measurements/refresh-prediction.md` — which targets move, which do not, and the expected role, component, patch and disclosure deltas
- [X] T044 Refresh the corpus goldens through the `public-corpus.yml` workflow dispatch. **Never** set `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` locally
- [X] T045 Satisfy FR-016's second half: score the T043 prediction against the refresh diff in `specs/1278-closure-structured-attrs/measurements/refresh-prediction.md`, including any miss and what it corrected
- [X] T046 Check every target under `waybill-cli/tests/fixtures/public_corpus/` for movement outside `nix-closure-moat` beyond the disclosure appearing on closure-scanning targets — a non-closure target that moved means the widening leaked past the type it is confined to (research R6)
- [X] T047 Satisfy FR-017: reconciled `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml` against the refreshed goldens — **no change required**. Verified positively, not by absence: the compare run's `a_stale_declaration_fails_the_lane` and `an_undeclared_regression_fails_the_lane_and_names_the_cell` gates both ran and passed, and no `nix-closure-moat` row exists among the 11 declarations. 23 new components arrived and none became an I7 or I8 offender
- [X] T048 [P] Record the measured `fields-unconsulted` population as the first measurement of #1346's coverage gap in `specs/1278-closure-structured-attrs/measurements/README.md` — the output SC-013 deliberately did not predict
- [X] T049 Run the `quickstart.md` validation end to end, including the control that the env-only arm still reproduces the goldens and the five traps
- [X] T050 [P] Record anything the milestone corrected about its own research in `specs/1278-closure-structured-attrs/measurements/findings.md`, including the 16→9 false-exclusion count fixed during spec writing

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001–T004)**: no dependencies, all parallel
- **Foundational (T005–T011)**: needs Setup for T011 only; T005→T006/T007/T008 in order (same file), T009 after T008, T010 after T009. **Blocks every user story**
- **US1 (T012–T021)**: after Foundational
- **US2 (T022–T025)**, **US3 (T026–T029)**: after Foundational; independent of US1 and of each other
- **US4 (T030–T039, plus T051)**: after Foundational. T030 must fail before T032. **T051 belongs to this phase despite its number** — it was added after the list was first written, and appending it was chosen over inserting it so that no existing ID moved and no citation broke
- **Polish (T040–T050)**: after every story that is being delivered

### Story Dependencies

US1, US2 and US3 are independent **in their observables** and share one seam
in their implementation — T006 corrects US1 and US2 together, T007 corrects
US3. Stated plainly rather than claimed as independence: once Foundational
lands, US2 and US3 are each a test plus a doc correction. US4 is the only
story with its own implementation surface.

### Within Each Story

Tests before implementation. T030 in particular must be red before T032, since
it is the test for a defect that exists in the committed golden today.

### Parallel Opportunities

- T001–T004 together
- T012–T017 together (one file, independent cases — serialize if the harness contends)
- T022/T023, T026/T027, T030/T031/T051 pairwise
- T040, T041, T048, T050 together

---

## Parallel Example: User Story 1

```bash
# The contract properties, all independent cases:
Task: "T012 typed form resolves to the same ROLE as its flat twin"
Task: "T013 flat-form fixtures classify byte-identically to main"
Task: "T014 an empty typed list credits no member"
Task: "T015 awkward shapes yield no paths and do not panic"
Task: "T016 a both-shapes disagreement resolves from the flat map"
Task: "T017 an unknown format version classifies as a known one"
```

---

## Implementation Strategy

### MVP — US1 only

Setup → Foundational → US1 → stop and validate. That alone retracts nine
native claims that a member is absent from the artifact while nix references
it into one, which is the defect m1277 shipped to prevent.

### Incremental delivery

US1 (MVP) → US2 → US3 → US4 → Polish. Each is independently verifiable; only
Polish's golden refresh is shared, which is why T044 comes after every
output-changing task. The refresh is the expensive step — one dispatch, read
in full — so a story left half-landed costs a second one.

### Two sequencing constraints worth repeating

- **T035 and T036 land together.** A documented catalog row that nothing emits
  fails the catalog's own completeness test.
- **T043 before T044.** The prediction is committed before the dispatch, or it
  is not a prediction.
