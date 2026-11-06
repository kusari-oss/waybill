---
description: "Task list for milestone 1367 — a CPE for the closure components a feed could match"
---

# Tasks: a CPE for the closure components a feed could match

**Input**: Design documents from `/specs/1367-closure-component-cpes/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/annotations.md`, `quickstart.md`
**Issue**: #1371 (spec number ≠ issue number; issue #1367 is the unrelated edge undershoot)

**Tests**: **Required.** SC-007 states *"Disabling the feature makes a test fail"*, and SC-002/SC-003/SC-004/SC-005 are each assertions about emitted documents. Test tasks are part of the deliverable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: `[US1]` / `[US2]` / `[US3]` — user-story phases only

## Path conventions

Single Rust workspace. Production code under `waybill-cli/src/`, integration tests under `waybill-cli/tests/`, catalog in `docs/reference/sbom-format-mapping.md`.

---

## Phase 1: Setup — capture the before-state

**Purpose**: the baselines every later claim is quoted against. The "before" here is unusually clean — zero CPEs — which is exactly why the defect was invisible.

- [ ] T001 Create `specs/1367-closure-component-cpes/measurements/README.md` recording the host (aarch64-darwin), the pinned nixpkgs revision (`a799d3e3`), the moat commit (`d069055`), and the warning that these figures are **not** comparable to the x86_64-linux corpus goldens
- [ ] T002 Capture the pre-change document by scanning the pinned moat clone with `--nix-closure`, and record in `measurements/README.md` that it carries **0** components with a `cpe` field — the control that makes "6 after" a change rather than a number
- [ ] T003 [P] Record in `measurements/README.md` the six target components and their versions (`openssl 3.6.2`, `curl 8.20.0`, `zlib 1.3.2`, `sqlite 3.51.2`, `pcre2 10.46`, `llvm 21.1.8`), read out of the pre-change document rather than from issue #1371
- [ ] T004 [P] Record in `measurements/README.md` how many components reach the pass with **empty** `cpes` and a release-shaped version, split by PURL ecosystem — the population research R7 predicts is larger than six

**Checkpoint**: the before-state is a recorded fact, including the prediction R7 makes about the delivered total.

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: the release-shape predicate, testable with no wiring and no scan.

**⚠️ CRITICAL**: blocks US1, US2 and US3.

- [ ] T005 Add a `fn is_release_shaped(version: &str) -> bool` helper in `waybill-cli/src/cli/scan_cmd.rs`, next to where the new pass will call it. **Not** in `waybill-cli/src/generate/cpe.rs`: changing the shared synthesizer would alter CPEs for every deb/apk/rpm component and every golden carrying one (research R5)
- [ ] T006 [P] Unit-test `is_release_shaped` in `waybill-cli/src/cli/scan_cmd.rs` against the shapes **measured on the real closure**, accepting `3.6.2`, `8.20.0`, `10.46`, `21.1.8`, `1.0.2k` and declining `21.1.6+apple-sdk-26.4`, `20251016-3.1`, `0-unstable-2018-06-17`, `26.05pre-git`, `boot`
- [ ] T007 Add a `fn fill_missing_cpes(components: &mut [ResolvedComponent]) -> CpeSynthesisCoverage` in `waybill-cli/src/cli/scan_cmd.rs` that, for each component, skips a non-empty `cpes`, skips a non-release version, otherwise assigns `generate::cpe::synthesize_cpes(c)`; and counts both outcomes per `data-model.md` §2
- [ ] T008 [P] Define `CpeSynthesisCoverage` in `waybill-cli/src/cli/scan_cmd.rs` with `with_candidates`, `eligible_unmapped` and a `wire()` producing the C209 JSON object per `contracts/annotations.md` §2

**Checkpoint**: the pass exists as a pure function over a slice and is unit-testable without a scan.

---

## Phase 3: User Story 2 — a component that is not software does not acquire a CPE (Priority: P1)

**Goal**: the guards hold before anything is emitted, so the pass is never, even briefly, in a state that puts CPEs on patch files.

**Why this phase precedes US1 despite sharing its priority**: US2 is satisfied by *declining*. Building the guards first means there is no window in which the pass fills unconditionally. The reverse order creates that window and relies on a later task to close it.

**Independent Test**: run `fill_missing_cpes` over a component set containing members and non-release versions and assert none receives candidates.

- [ ] T009 [US2] Unit-test in `waybill-cli/src/cli/scan_cmd.rs` that a component with a non-release version receives no candidates even when its name is a curated mapping row (FR-003)
- [ ] T010 [P] [US2] Unit-test in `waybill-cli/src/cli/scan_cmd.rs` that a component carrying `waybill:closure-member-role` receives no candidates (FR-002). Note the belt-and-braces relationship the data model records: only 1 of 539 members carries a version at all, so this guard and FR-003's overlap on the measured population and the test must construct a member that *does* carry a release version
- [ ] T011 [P] [US2] Unit-test in `waybill-cli/src/cli/scan_cmd.rs` that a component whose `(ecosystem, name)` has no mapping row receives no candidates and produces no diagnostic (FR-007), and is counted as `eligible_unmapped` rather than ignored

**Checkpoint**: the pass declines everything it should. Mergeable on its own as a no-op.

---

## Phase 4: User Story 1 — a consumer matches a closure's C libraries against NVD (Priority: P1) 🎯 MVP

**Goal**: the six measured components carry CPE candidates an NVD-backed scanner can match.

**Independent Test**: scan the pinned closure and confirm the six carry CPEs whose vendor, product and version correspond to what the closure records.

### Tests for User Story 1 (write first; they must fail before T015)

- [ ] T012 [US1] Create `waybill-cli/tests/closure_component_cpes.rs` on the milestone-1366 stub-`nix` pattern (`waybill-cli/tests/closure_ecosystem_identity.rs`), with a fixture closure containing synthetic `waybill-fixture-*` software whose names are mapped and whose versions are release-shaped
- [ ] T013 [US1] **Write the control first** in `waybill-cli/tests/closure_component_cpes.rs`: assert the scanned document contains the mappable components at all. A scan whose closure tier degraded emits nothing and would satisfy every "has a CPE" assertion vacuously — the exact shape that let this defect live
- [ ] T014 [P] [US1] Test in `waybill-cli/tests/closure_component_cpes.rs` that a mapped, release-versioned closure component carries a `cpe` field in CycloneDX, a `cpe23Type` external ref in SPDX 2.3 and an `externalIdentifier` in SPDX 3 (FR-006, US1 scenarios 1 and 2)

### Implementation for User Story 1

- [ ] T015 [US1] Call `fill_missing_cpes` from `waybill-cli/src/cli/scan_cmd.rs` **after** the milestone-1366 identity pass and the closure merge. Placement matters for a reason research R2 records: `synthesize_cpes` switches on `purl.ecosystem()`, so a component re-identified from `pkg:generic` to `pkg:pypi` takes a different arm depending on which side of that pass this runs
- [ ] T016 [US1] Thread the returned `CpeSynthesisCoverage` out of the match block to the emitters, mirroring how `nix_closure_degraded` (C190) and `closure_ecosystem_identity` (C208) are threaded in `waybill-cli/src/cli/scan_cmd.rs`
- [ ] T017 [P] [US1] Test in `waybill-cli/tests/closure_component_cpes.rs` that the component's PURL is **unchanged** — a CPE is emitted alongside `pkg:generic`, never instead of it, so a consumer matching on the PURL sees no difference (`contracts/annotations.md` §1)
- [ ] T018 [US1] Verify against the real closure that the six named in T003 now carry CPEs, and record the per-component values in `measurements/README.md`

**Checkpoint**: US1 and US2 both hold. The MVP — six components that an NVD-backed scanner could not see are now visible to it.

---

## Phase 5: User Story 3 — a component is not given two sets of CPEs (Priority: P2)

**Goal**: an earlier pass's CPEs, and an operator's, both survive.

**Independent Test**: supply a CPE via `--supplement-cdx` for a component the mapping table would also match, and confirm the operator's value is what the document carries.

> **This is the mutation most likely to pass unnoticed.** `supplement::merge`
> runs at `scan_cmd.rs:4334`, before any post-closure pass, so an
> unconditional fill would overwrite operator data — and **nothing on the
> pinned closure would reveal it**, because no component there both receives a
> supplement and matches the table. The fixture below is the only thing that
> can catch it.

- [ ] T019 [US3] Create `waybill-cli/tests/fixtures/cpe_supplement/` with a supplement file supplying a CPE for a component whose name is also a curated mapping row
- [ ] T020 [US3] Test in `waybill-cli/tests/closure_component_cpes.rs` that a scan with `--supplement-cdx` emits the operator's CPE and not a synthesized one (FR-004)
- [ ] T021 [P] [US3] Test in `waybill-cli/tests/closure_component_cpes.rs` that no component carries the same CPE value twice, over the whole emitted document (FR-005)
- [ ] T022 [P] [US3] Test in `waybill-cli/tests/closure_component_cpes.rs` that a component whose CPEs an earlier reader already supplied is unchanged after the pass (FR-004, US3 scenario 1)

**Checkpoint**: all three stories hold independently.

---

## Phase 6: FR-008 — the document-scope record

**Purpose**: make "no mapped software" and "the pass did not run" different documents. Without this, the next regression of exactly this kind is as invisible as this one was.

- [ ] T023 Add `cpe_coverage: Option<&'a str>` to `ScanArtifacts` in `waybill-cli/src/generate/mod.rs`, and render the C209 value in `waybill-cli/src/cli/scan_cmd.rs` beside the existing document-scope records
- [ ] T024 [P] Emit C209 as a document property in `waybill-cli/src/generate/cyclonedx/metadata.rs`
- [ ] T025 [P] Emit C209 as a document-scope annotation in `waybill-cli/src/generate/spdx/annotations.rs`
- [ ] T026 [P] Emit C209 as a document-scope annotation in `waybill-cli/src/generate/spdx/v3_annotations.rs`
- [ ] T027 Add the C209 row to `docs/reference/sbom-format-mapping.md` with the Principle V native-field audit from `contracts/annotations.md` §2, recording why no format carries a synthesis-coverage count natively
- [ ] T028 Add the three C209 extractors and the `EXTRACTORS` row in `waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs`. **Same change as T027** — a catalog row without an extractor fails `every_catalog_row_has_an_extractor` and `holistic_parity`, and that gate lives in the **lib** target
- [ ] T029 [P] Test in `waybill-cli/tests/closure_component_cpes.rs` that C209 distinguishes three documents: nothing eligible, eligible-and-mapped, and eligible-but-unmapped (FR-008)
- [ ] T030 [P] Test in `waybill-cli/tests/closure_component_cpes.rs` that the C209 payload is identical in all three formats

---

## Phase 7: Verification, measurement and goldens

### Non-regression

- [ ] T031 [P] Assert the 539 closure members are byte-identical before and after, against T002's baseline, and record the diff in `specs/1367-closure-component-cpes/measurements/README.md` (SC-002)
- [ ] T032 [P] Assert that a scan of a project with **no** nix closure produces a byte-identical document apart from C209, against a baseline from the same pre-change binary, and record it in `specs/1367-closure-component-cpes/measurements/README.md` (SC-005)
- [ ] T033 Assert every emitted CPE's version field is release-shaped in `waybill-cli/tests/closure_component_cpes.rs`, read out of the emitted document rather than from the guard's unit test (SC-003)

### The measured numbers

- [ ] T034 **Produce SC-001's number.** Record in `measurements/README.md` the measured count of components carrying a CPE after the change, with the command that produced it. Research R7 predicts it exceeds six because the 74 `pkg:pypi` components take the pypi arm; **that prediction is not data** and the recorded figure must come from the run
- [ ] T035 Record in `measurements/README.md` the breakdown of the delivered total by PURL ecosystem, so the six from the curated table are distinguishable from whatever else the pass reached
- [ ] T036 Demonstrate SC-006 by confirming `openssl`, `curl`, `zlib` and `sqlite` each carry at least one CPE candidate, and record the values in `specs/1367-closure-component-cpes/measurements/README.md`

### SC-007 — the mutation check

- [ ] T037 **Mutation-check the suite.** Make the pass a no-op → the US1 tests must go red. Make it fill unconditionally instead of only-if-empty → the T020 supplement test must go red. Remove the release-shape guard → the T009 test must go red. Record which tests failed and how many in `measurements/README.md`; if the second mutation leaves everything green, FR-004 is not covered

### Goldens and the gate

- [ ] T038 Refresh the corpus goldens through the `public-corpus.yml` workflow dispatch — never locally. **This pass runs on every scan, not only closure scans**, so the expectation is not "only the closure target changes"; which targets change is a measurement
- [ ] T039 Account for **every** differing golden file under `waybill-cli/tests/fixtures/public_corpus/` before committing any, enumerating them from the artifact rather than from the run's summary. `image-postgres16` drifts for an unrelated reason (issue #1372) and its regenerated goldens MUST NOT be committed in this feature
- [ ] T040 Confirm SC-008 by enumerating the corpus targets under `waybill-cli/tests/fixtures/public_corpus/` that do not scan a closure and whose goldens are unchanged, reading each target's own result
- [ ] T041 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189). Assert on the positive signal and `GATE_RESULT=pass` in `target/gate/prepr.log`. Never pipe cargo through `head`: it closes the pipe mid-run and wedges the process at the filter's count
- [ ] T042 [P] Update `docs/ecosystems.md` to state what a CPE on a closure component means — a candidate for a downstream matcher, derived from a name match in a curated table, not a proof that the closure's `llvm 21.1.8` is the product NVD calls `llvm:llvm`

---

## Dependencies & execution order

### Phase dependencies

- **Phase 1 (Setup)**: no dependencies. T004's number is what T034 is compared against.
- **Phase 2 (Foundational)**: depends on Phase 1. **Blocks every user story.**
- **Phase 3 (US2)**: depends on Phase 2. Independently mergeable as a no-op.
- **Phase 4 (US1)**: depends on Phase 2 and on US2's guards (T009–T011).
- **Phase 5 (US3)**: depends on T015's call site existing.
- **Phase 6 (FR-008)**: depends on T007 returning the coverage struct.
- **Phase 7**: depends on all of the above.

### Story dependencies

- **US2** → none beyond Foundational. The one story complete on its own.
- **US1** → US2's guards; otherwise there is a window where the pass fills unconditionally.
- **US3** → US1's call site, because the precedence it tests is a property of where the pass runs.

### Within each story

- Controls before assertions (T013 before T014).
- Tests before implementation in Phase 4 (T012–T014 before T015).
- T027 and T028 land together or CI rejects both.

### Parallel opportunities

- T003, T004 (Phase 1)
- T006, T008 (Phase 2, after T005/T007)
- T010, T011 (Phase 3)
- T014, then T017 (Phase 4)
- T021, T022 (Phase 5)
- T024, T025, T026, then T029, T030 (Phase 6)
- T031, T032 and T042 (Phase 7)

---

## Parallel example: Phase 6

```bash
# The three C209 emitters touch different files:
Task: "T024 C209 document property in generate/cyclonedx/metadata.rs"
Task: "T025 C209 document annotation in generate/spdx/annotations.rs"
Task: "T026 C209 document annotation in generate/spdx/v3_annotations.rs"

# Then the two record tests:
Task: "T029 C209 distinguishes the three states"
Task: "T030 C209 is identical in all three formats"
```

---

## Implementation strategy

### MVP (US2 + US1)

1. Phase 1 → the before-state is recorded, including the 0-CPE control
2. Phase 2 → the predicate and the pass exist as pure functions
3. Phase 3 (US2) → the pass declines everything it should. **Mergeable alone**
4. Phase 4 (US1) → the six become visible to an NVD-backed scanner
5. **STOP and VALIDATE**: T018 against the real closure

### Incremental delivery

Each phase boundary is a mergeable state. Phase 3 alone is a no-op that counts
what it would have done — useful on its own, because it makes the population
visible in the document before anything acts on it.

---

## Notes

- **Zero CPEs is what this defect looked like for a whole milestone.** Every
  test that asserts a CPE exists needs a control proving the population is
  non-empty first.
- **The supplement case cannot be caught by the corpus.** No component there
  both receives a supplement and matches the table; T019's fixture is the only
  thing standing between an unconditional fill and overwritten operator data.
- **Figures are per-closure and per-system.** Measurements are aarch64-darwin;
  the corpus goldens pin x86_64-linux, a different closure.
- **Never evaluate a fixture flake in place** — it rewrote a committed
  `flake.lock` and broke 11 tests on 2026-11-04. Copy the clone, verify with
  `md5 -q`.
- Test modules using `.unwrap()` need `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- Commit after each task or logical group. `Assisted-by:` trailer, never
  `Co-Authored-By:`. No session URLs in any public artifact.
