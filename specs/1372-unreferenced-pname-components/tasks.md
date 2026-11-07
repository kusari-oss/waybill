---
description: "Task list for milestone 1372 — the closure's build inputs that `unreferenced` hides"
---

# Tasks: the closure's build inputs that `unreferenced` hides

**Input**: Design documents from `/specs/1372-unreferenced-pname-components/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/annotations.md`, `quickstart.md`
**Issue**: #1366 (spec number ≠ issue number; issue #1372 is the unrelated postgres16 golden drift)

**Tests**: **Required.** SC-007 states *"Disabling the feature makes a test fail"*, and SC-002/SC-003/SC-004/SC-004a/SC-005 are each assertions about emitted documents.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: `[US1]` / `[US2]` / `[US3]` — user-story phases only

## Path conventions

Single Rust workspace. Production code under `waybill-cli/src/`, integration tests under `waybill-cli/tests/`, catalog in `docs/reference/sbom-format-mapping.md`.

---

## Phase 1: Setup — pin the counts before anything moves

**Purpose**: this feature's diff is 641 relabellings wide, and a wrong implementation produces a plausible one. The only defence is numbers measured **before** the change.

- [X] T001 Create `specs/1372-unreferenced-pname-components/measurements/README.md` recording the host (aarch64-darwin), the pinned nixpkgs revision (`a799d3e3`), the moat commit (`d069055`), and the warning that these figures are **not** comparable to the x86_64-linux corpus goldens
- [X] T002 Capture the pre-change document by scanning the pinned moat clone with `--nix-closure`, and record in `measurements/README.md` the role histogram, the component count and the member count
- [X] T003 [P] Record in `measurements/README.md` the four counts every later assertion is checked against: **742** `unreferenced`, **741** of them named by `inputs.drvs`, **641** already emitting, **100** newly emitting — each read from the capture rather than quoted from the spec
- [X] T004 [P] Record in `measurements/README.md` the pre-change **edge count** and **lifecycle-scope histogram**, so the two deltas research R5 and R7 predict are measurable rather than inferred
- [X] T005 [P] Record in `measurements/README.md` the pre-change residual sbomnix name gap by re-running `specs/1276-boot-library-scope/measurements/compare-sbomnix.sh`; it is 32 today

**Checkpoint**: every number the implementation will be judged against exists as a recorded observation.

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: the referrer set and its validation rules, unit-testable with no scan.

**⚠️ CRITICAL**: blocks US1, US2 and US3.

- [X] T006 Add a `fn graph_referrers(closure: &RawClosure) -> BTreeSet<&str>` in `waybill-cli/src/scan_fs/package_db/nix/closure/classify.rs`, built from each derivation's `inputs.drvs` (already parsed at `derivation.rs:58`) — the signal FR-001 requires
- [X] T007 Implement validation rule **V1** in `classify.rs`: a derivation naming itself in its own `inputs.drvs` does not count as referenced. Not observed on the pinned closure, so it needs a constructed test, not the corpus
- [X] T008 Implement **V2** in `classify.rs`: a referrer that is itself not emitted still counts. The alternative would make the signal depend on its own output
- [X] T009 Implement **V3** in `classify.rs`: an `inputs.drvs` key that resolves to no derivation is ignored rather than counted; milestone 1280 already has a dangling-key fixture
- [X] T010 [P] Unit-test `graph_referrers` in `classify.rs` for V1, V2 and V3 against constructed closures, each asserting the referrer set's membership directly rather than a downstream role

**Checkpoint**: the signal is computable and its rules hold, with no role changed yet.

---

## Phase 3: User Story 2 — `unreferenced` stops saying something untrue (Priority: P1)

**Goal**: the label becomes true. 741 components stop claiming nothing depends on them.

**Why this phase precedes US1 despite sharing its priority**: US1's new components arrive *because* the role changes — `role_alone_emits` is what admits them. Doing the correction first means there is never a state in which a component is emitted on a signal its role does not reflect.

**Independent Test**: scan the pinned closure and assert no component the build graph references carries `unreferenced`.

- [X] T011 [US2] Change the `(false, false)` arm in `classify.rs` to yield `BuildTooling` when `graph_referrers` names the key and `Unreferenced` otherwise. **Only that arm** — FR-003 requires the relabelling and FR-003a requires a declared attribute always win
- [X] T012 [US2] Unit-test in `classify.rs` that a derivation named by **both** a declared attribute and `inputs.drvs` keeps the declared role (SC-004a). The pinned closure cannot produce this case, so it must be constructed — and this is the mutation most likely to leave the corpus green
- [X] T013 [P] [US2] Unit-test in `classify.rs` that a derivation nothing names stays `Unreferenced` — the closure-root case, and the control that detects admission on no evidence (FR-004, SC-004)
- [X] T014 [P] [US2] Integration test in `waybill-cli/tests/closure_edges.rs` on the stub-`nix` pattern from `tests/closure_component_cpes.rs`: after a scan, exactly one component carries `unreferenced` and it is the closure root (SC-003, SC-004)

**Checkpoint**: the label is true. Components may now begin to appear — which is US1.

---

## Phase 4: User Story 1 — a consumer sees the tools the build actually used (Priority: P1) 🎯 MVP

**Goal**: `gzip`, `gawk`, `gnused`, `less`, `patch` and `coreutils` exist as components.

**Independent Test**: scan the pinned closure and find all six with the versions the closure records.

> No production change is expected in this phase. The 100 are admitted by
> `role_alone_emits` as a consequence of T011, and `emits_as_component` is
> **not** edited. If a change is needed here, the Phase 3 arm is wrong.

- [X] T015 [US1] **Write the control first** in `waybill-cli/tests/closure_edges.rs`: assert the fixture closure yields components at all before asserting any named one exists
- [X] T016 [P] [US1] Test in `waybill-cli/tests/closure_edges.rs` that a derivation named only by another's `inputs.drvs` is emitted as a component (FR-002, US1 scenario 1)
- [X] T017 [US1] Verify against the real closure that all six named tools now appear, and record their versions in `specs/1372-unreferenced-pname-components/measurements/README.md` (SC-002)
- [X] T018 [US1] Record the measured newly-emitted count in `measurements/README.md` and compare it to T003's **100**; a different number is a finding, not a pass (SC-001)

**Checkpoint**: US1 and US2 hold. This is the MVP — six CVE-bearing tools that no consumer could see now exist.

---

## Phase 5: User Story 3 — a reader can tell how a component was reached (Priority: P2)

**Goal**: a graph-admitted component says so; a declared one is untouched.

- [X] T019 [US3] Write the C211 value into `component.extra_annotations` from the closure emitter in `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs`, only for components the graph admitted. **This alone gives all three formats** — the bag is carried generically by `generate/cyclonedx/builder.rs`, `generate/spdx/annotations.rs` and `generate/spdx/v3_annotations.rs`
- [X] T020 [P] [US3] Test in `waybill-cli/tests/closure_edges.rs` that a graph-admitted component carries C211 and a declared-attribute component carries none — presence is the signal (FR-005)
- [X] T021 [P] [US3] Test in `waybill-cli/tests/closure_edges.rs` that C211's value is identical in all three formats

**Checkpoint**: all three stories hold independently.

---

## Phase 6: FR-008 — the document-scope record

- [X] T022 Define `GraphAdmissionRecord` with `admitted` and `relabelled` per `data-model.md` §4, in `waybill-cli/src/scan_fs/package_db/nix/closure/classify.rs`, asserting `admitted + relabelled` equals the number of roles changed
- [X] T023 Thread the record from `waybill-cli/src/cli/scan_cmd.rs` to the emitters, mirroring how `nix_closure_degraded` (C190) and `closure_ecosystem_identity` (C208) are threaded. **Gate it on a closure having been recorded**, like C204/C208 and unlike C209 — this is what keeps every local golden still
- [X] T024 [P] Emit C210 as a document property in `waybill-cli/src/generate/cyclonedx/metadata.rs`
- [X] T025 [P] Emit C210 as a document-scope annotation in `waybill-cli/src/generate/spdx/annotations.rs`
- [X] T026 [P] Emit C210 as a document-scope annotation in `waybill-cli/src/generate/spdx/v3_annotations.rs`
- [X] T027 Add the C210 and C211 rows to `docs/reference/sbom-format-mapping.md` with the Principle V native-field audit from `contracts/annotations.md`
- [X] T028 Add the six extractors and the two `EXTRACTORS` rows in `waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs`. **Same change as T027** — a catalog row without an extractor fails `every_catalog_row_has_an_extractor`, and that gate lives in the **lib** target
- [X] T029 [P] Test in `waybill-cli/tests/closure_edges.rs` that C210 distinguishes a closure with nothing to do (`admitted: 0, relabelled: 0`) from a scan with no closure at all (C210 absent) — FR-008
- [X] T030 [P] Test in `waybill-cli/tests/closure_edges.rs` that C210's payload is identical in all three formats

---

## Phase 7: Verification, measurement and goldens

### The controls a 641-row diff would otherwise hide

- [X] T031 Assert in `waybill-cli/tests/closure_edges.rs` that the **member count is identical** before and after — the 603 milestone-1282 members emit via `declared_pname().is_none()`, and emission must not become role-dependent (research R4, quickstart Control 3)
- [X] T032 [P] Assert against the real closure that exactly **one** *derivation* stays `unreferenced` and it is the closure root, recording the result in `specs/1372-unreferenced-pname-components/measurements/README.md` (SC-003, SC-004). **Corrected during implementation**: this task said *component*, and as written it is false — the root becomes the document's `metadata.component`, which carries no `waybill:closure-role` at all, so both hosts emit **zero** `unreferenced` components. The control is a derivation-level fact and is asserted where it lives, in `classify` and in `m1372_the_root_stays_unreferenced`
- [X] T033 [P] Assert against the real closure that **zero** derivations named by a declared dependency attribute had their role changed, recording it in `measurements/README.md` (SC-004a)
- [X] T034 [P] Assert in `waybill-cli/tests/closure_edges.rs` that a name appearing at two versions among the newly admitted — `cctools-binutils-darwin` at `1010.6` and `boot`, `macOS-SDK` at `14.4` and `26.4` — survives as **two** components. The merge key is `(name, version)`, so a regression here collapses distinct artifacts into one (spec Edge Cases)
- [X] T035 Assert that every component emitted before the change is identical after it **except for its C182 role**, diffing against T002's capture and recording the count in `measurements/README.md`; expected **641** (FR-006, SC-005)

### The three numbers later work will be diagnosed against

- [X] T036 Record the measured **lifecycle-scope delta** in `measurements/README.md` as a number distinct from the 741 role changes — scope has its own exemptions (m1276), so the two will differ and a reader assuming they match will misread the diff (research R5)
- [X] T037 Record the measured **edge delta** in `measurements/README.md`, before and after, **explicitly so that issue #1367's edge undershoot is not later diagnosed against a figure this feature moved** (research R7). Also assert FR-007: every edge no longer emitted has a newly admitted component on its former path
- [X] T038 Re-measure the residual sbomnix name gap with `specs/1276-boot-library-scope/measurements/compare-sbomnix.sh` and record it in `measurements/README.md`; all 23 named gap names are in this population, so near-zero is the expectation and substantially above zero is a finding (SC-008)

### SC-007 — the mutation check

- [X] T039 **Mutation-check the suite**, recording which tests went red in `measurements/README.md` (SC-007). Make the graph signal a no-op → the US1 tests must fail. Apply it to all four arms rather than `(false,false)` → T012 must fail. Drop the V1 self-reference guard → T010 must fail. If the second mutation leaves everything green, SC-004a is not covered

### Non-regression and goldens

- [X] T040 [P] Assert that a scan of a project with **no** nix closure produces a byte-identical document, against a baseline from the pre-change binary, recording it in `specs/1372-unreferenced-pname-components/measurements/README.md` (SC-006)
- [X] T041 Confirm **no local golden changes** under `waybill-cli/tests/fixtures/golden/` by running the gate before any regeneration. A local golden moving means C210 or C211 is emitted unconditionally — a bug, not a refresh (research R9, quickstart)
- [X] T042 Refresh the corpus goldens through the `public-corpus.yml` workflow dispatch — never locally. Expect `nix-closure-moat` to change substantially and the other 16 targets not at all
- [X] T043 Account for **every** differing golden file under `waybill-cli/tests/fixtures/public_corpus/` from the artifact rather than the run's summary, and keep `image-postgres16` out of this feature's diff (#1372)
- [X] T044 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189). Assert on the positive signal and `GATE_RESULT=pass` in `target/gate/prepr.log`. Never pipe a check through `head`
- [X] T045 [P] Update `docs/ecosystems.md` to state what `unreferenced` now means — that it is measured against the build graph nix records and not only against declared attributes — and what a `build-graph` admission says and does not say

---

## Dependencies & execution order

### Phase dependencies

- **Phase 1 (Setup)**: no dependencies. T003's four counts are what T018, T032 and T034 are checked against.
- **Phase 2 (Foundational)**: depends on Phase 1. **Blocks every user story.**
- **Phase 3 (US2)**: depends on Phase 2. The correction; mergeable on its own.
- **Phase 4 (US1)**: depends on T011 — the components arrive *because* the role changed, with no further production edit.
- **Phase 5 (US3)**: depends on T011 to know which components were graph-admitted.
- **Phase 6 (FR-008)**: depends on T022's counts, which depend on T011.
- **Phase 7**: depends on all of the above.

### Story dependencies

- **US2** → none beyond Foundational. The only story complete on its own.
- **US1** → US2, and *only* US2: no production code of its own.
- **US3** → US2, for the admission signal it discloses.

### Within each story

- Controls before assertions (T015 before T016).
- T027 and T028 land together or CI rejects both.
- T041 runs **before** any golden regeneration, not after.

### Parallel opportunities

- T003, T004, T005 (Phase 1)
- T010 (Phase 2, after T006–T009)
- T013, T014 (Phase 3)
- T016 (Phase 4)
- T020, T021 (Phase 5)
- T024, T025, T026, then T029, T030 (Phase 6)
- T032, T033 and T040, T045 (Phase 7)

---

## Parallel example: Phase 6

```bash
# The three C210 emitters touch different files:
Task: "T024 C210 document property in generate/cyclonedx/metadata.rs"
Task: "T025 C210 document annotation in generate/spdx/annotations.rs"
Task: "T026 C210 document annotation in generate/spdx/v3_annotations.rs"

# Then the two record tests:
Task: "T029 C210 distinguishes nothing-to-do from no-closure"
Task: "T030 C210 is identical in all three formats"
```

---

## Implementation strategy

### MVP (US2 + US1)

1. Phase 1 → the four counts exist as observations
2. Phase 2 → the signal computes, no role changed
3. Phase 3 (US2) → `unreferenced` becomes true. **Mergeable alone**, and the larger half of the change
4. Phase 4 (US1) → the six tools appear, with no further production edit
5. **STOP and VALIDATE**: T017 and T018 against the real closure

### Incremental delivery

Phase 3 is the unusual one: it is mergeable by itself and it is where 641 of
the 741 changes happen. Phase 4 adds no production code at all — if it needs
any, the Phase 3 arm is wrong and that is worth knowing before Phase 5.

---

## Notes

- **A 641-row diff hides errors.** Over-admission, an FR-003a violation and a
  lost closure root all look like more of the same correction. T003's counts
  and T032's root check are the defence.
- **The corpus cannot exercise FR-003a.** Every derivation a declared
  attribute names already has a non-`unreferenced` role, so T012's constructed
  fixture is the only thing that can catch the mutation.
- **Figures are per-closure and per-system.** Measurements are
  aarch64-darwin; the corpus goldens pin x86_64-linux, a different closure.
- **Never evaluate a fixture flake in place** — it rewrote a committed
  `flake.lock` and broke 11 tests on 2026-11-04. Copy the clone, verify with
  `md5 -q`.
- Test modules using `.unwrap()` need `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- Commit after each task or logical group. `Assisted-by:` trailer, never
  `Co-Authored-By:`. No session URLs in any public artifact.
