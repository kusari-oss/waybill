---
description: "Task list for 1279-closure-dep-field-coverage"
---

# Tasks: `unreferenced` must mean "nothing depends on this"

**Input**: Design documents in `/specs/1279-closure-dep-field-coverage/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`
**Issue**: #1346

**Tests are included.** SC-007 requires a mutation that proves the widened
field set is load-bearing, SC-009 requires a test that the consulted set and
the disclosure's complement agree, and the repository's gate is the acceptance
criterion for every change here.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable — different file, no dependency on an incomplete task
- **[Story]**: US1 … US4, on user-story tasks only

## Path Conventions

Single Rust workspace, three crates. Everything is in `waybill-cli`.

- Reader: `waybill-cli/src/scan_fs/package_db/nix/closure/`
- Parity: `waybill-cli/src/parity/extractors/`
- Catalog: `docs/reference/sbom-format-mapping.md`
- Tests: `waybill-cli/tests/`, corpus harness at `waybill-cli/tests/corpus_harness_195/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: fixtures covering every state a dependency attribute can be in.
`data-model.md` names four, not two, and the fourth is what this milestone
adds — a fixture set that only covers consulted-vs-unconsulted would leave
FR-005 and research R4 untested.

**⚠️ Do not let any fixture be evaluated by a real `nix`.** On 2026-11-04 that
rewrote a committed `flake.lock` and broke 11 tests in an unrelated file. These
fixtures are closure JSON read from disk; nothing here evaluates a flake.

- [ ] T001 [P] Add `waybill-cli/tests/fixtures/nix_closure_dep_fields/newly_consulted.json`: four members, each referenced **only** through one of `depsTargetTarget`, `depsTargetTargetPropagated`, `depsBuildTarget`, `propagatedNativeBuildInputs`, so each attribute's effect is separable
- [ ] T002 [P] Add `waybill-cli/tests/fixtures/nix_closure_dep_fields/stdenv_only.json`: one member referenced only through each of the **three** stdenv-injected attributes — `defaultNativeBuildInputs`, `defaultBuildInputs`, `extraNativeBuildInputs` — the FR-012 population, each of which must gain the marker and no role. The third was missed in the spec's first draft because it is unpopulated on darwin and populated on linux, so the fixture covers all three rather than whichever the census of the day showed
- [ ] T003 [P] Add `waybill-cli/tests/fixtures/nix_closure_dep_fields/attribute_states.json` covering the states `data-model.md` enumerates: an attribute declared-but-empty, one consulted-but-never-declared, one unconsulted-and-populated, and a member named by both an artifact-side and a tooling-side attribute (the union case)
- [ ] T004 [P] Add `waybill-cli/tests/fixtures/nix_closure_dep_fields/propagation_chain.json`: a member reachable only through two hops of `propagatedNativeBuildInputs`, for US2's transitive case

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the unclassified-reference index. Nothing observable changes in
this phase, which is what lets US1's census delta be measured on its own.

**⚠️ CRITICAL**: blocks US1 through US4.

- [ ] T005 Add the unclassified-reference index to `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs` per `data-model.md`: per member, the sorted set of dependency attributes that named it and that classification does not consult. Sorted so the emitted value is byte-stable; keyed per member, not per declaring derivation
- [ ] T006 Build the index from the same per-derivation walk `read_outcome` already performs in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`, so the two cannot disagree about which attributes are unconsulted
- [ ] T007 Thread the index out of `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs` alongside the existing roles and read outcome, with no emitter consuming it yet
- [ ] T008 [P] Unit-test the index against T002 and T003 in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`'s test module, with the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard the crate requires

---

## Phase 3: User Story 1 — `unreferenced` means one thing (Priority: P1) 🎯 MVP

**Goal**: a member nix references through any attribute stops reading as
though nothing depends on it — by gaining a role where the attribute is now
consulted, and by gaining a marker where it is deliberately not.

**Independent test**: for every `unreferenced` member of the corpus closure,
no derivation names it in any dependency attribute; `numactl` carries
`artifact-input` and no scope.

### Tests for User Story 1

- [ ] T009 [P] [US1] Test contract property P1 in `waybill-cli/tests/closure_dep_fields.rs`: a member referenced only through each of the four newly-consulted attributes carries the expected role — `artifact-input` for the two target triples, `build-tooling` for `depsBuildTarget`. Assert on the **role**, never on a parsed field: the store-path basename join is a second silent failure, and a field-level assertion stays green while every member classifies as unreferenced
- [ ] T010 [P] [US1] Test contract property P6 in `waybill-cli/tests/closure_dep_fields.rs`: a member referenced only through any of the three stdenv-injected attributes acquires **no role** and carries the marker naming that attribute
- [ ] T011 [P] [US1] Test contract property P3 in `waybill-cli/tests/closure_dep_fields.rs`: a declared-but-empty attribute credits nothing and is not reported as a gap
- [ ] T012 [P] [US1] Test contract property P4 in `waybill-cli/tests/closure_dep_fields.rs`: `depsBuildHost` and `nativeCheckInputs` are consulted but declared on no derivation, and must not surface as gaps — the third state research R4 found
- [ ] T013 [P] [US1] Test contract property P2 in `waybill-cli/tests/closure_dep_fields.rs`: a member named by both an artifact-side and a tooling-side attribute unions to `both`, unchanged by the widening
- [ ] T014 [P] [US1] Test FR-013 in `waybill-cli/tests/closure_dep_fields.rs`: the marker **names** the attributes, as a sorted array, and a member named by two carries both
- [ ] T044 [P] [US1] Test the gating row the others miss, in `waybill-cli/tests/closure_dep_fields.rs`: a member referenced through **both** a consulted attribute and an unclassified one carries its role **and** the marker. `contracts/unclassified-marker.md` says the marker is present whatever the role, and every other marker test uses a member whose only reference is unclassified — so that row is untested and a gating rule that only emitted for `unreferenced` members would pass
- [ ] T015 [US1] Add the control every classifier test needs, in `waybill-cli/tests/closure_dep_fields.rs`: assert the fixture holds the derivations it claims and that something is referenced — a parse producing nothing satisfies every "not wrongly classified" assertion vacuously

### Implementation for User Story 1

- [ ] T016 [US1] Widen `ARTIFACT_FIELDS` with `depsTargetTarget` and `depsTargetTargetPropagated`, and `TOOLING_FIELDS` with `depsBuildTarget` and `propagatedNativeBuildInputs`, in `waybill-cli/src/scan_fs/package_db/nix/closure/classify.rs`. Record in the module doc **why** the target triples are artifact-side: `build-tooling` would assert `scope: "excluded"`, and a cross-compiler ships its target libc, so that assertion can be false
- [ ] T017 [US1] Emit the marker per component in `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs`, gated per `contracts/unclassified-marker.md` — present whenever an unclassified attribute names the member, whatever its role; absent otherwise
- [ ] T018 [US1] Add the marker's catalog row to `docs/reference/sbom-format-mapping.md` with its Principle V audit, including the rejected `dependsOn` option and why
- [ ] T019 [US1] Add the marker's three extractors in `waybill-cli/src/parity/extractors/` **in the same change as T018** — a documented row with no extractor fails `every_catalog_row_has_an_extractor`
- [ ] T020 [US1] Verify `waybill-cli/src/resolve/closure_role_scope.rs` needs no change: `numactl` loses its `scope: "excluded"` through m1277's existing rule, which asserts nothing for `artifact-input`. If this task edits that file, the plan's confinement argument has broken and SC-008 needs re-checking against every target
- [ ] T021 [US1] Update the `nix-closure-moat` expectations in `waybill-cli/tests/corpus_harness_195/expectations.rs`: `numactl` carries `artifact-input` and **no** scope (SC-013); **all five** triple-reachable members — `numactl`, `gmp-with-cxx`, `libffi`, `elfutils`, `ncurses` — carry no absent-from-artifact marker (SC-010), not just the one that changes today, since the other four already carry no scope and would all gain one if the triples were later reclassified as tooling; and no component carrying `unreferenced` is named by any dependency attribute (SC-001)

**Checkpoint**: `unreferenced` means one thing. US1 delivers value with nothing else landed.

---

## Phase 4: User Story 2 — propagation (Priority: P2)

**Goal**: a member reachable only through `propagatedNativeBuildInputs`,
including transitively, is build tooling.

**Independent test**: the two-hop chain in T004 classifies as tooling.

- [ ] T022 [P] [US2] Test in `waybill-cli/tests/closure_dep_fields.rs`: a member named directly in `propagatedNativeBuildInputs` is `build-tooling`
- [ ] T023 [P] [US2] Test in `waybill-cli/tests/closure_dep_fields.rs`: the two-hop chain of T004's fixture classifies as `build-tooling` and not `unreferenced` — propagation is the one unread attribute whose effect was not local, so a direct-only test would miss the property that makes it different in kind
- [ ] T024 [US2] Confirm T016 alone satisfies both, and record in `waybill-cli/src/scan_fs/package_db/nix/closure/classify.rs`'s module doc that transitivity falls out of the existing reachability walk rather than needing its own pass

**Checkpoint**: propagation covered; US1 unaffected.

---

## Phase 5: User Story 3 — the disclosure narrows (Priority: P2)

**Goal**: C204's `fields-unconsulted` shrinks to exactly what remains
unclassified, so the field keeps meaning "declared and not consulted".

**Independent test**: none of the four newly-consulted attributes appears in
`fields-unconsulted`; both stdenv attributes still do.

- [ ] T025 [P] [US3] Test SC-009 in `waybill-cli/tests/closure_dep_fields.rs`: the consulted set and the disclosure's complement agree, asserted over a fixture declaring attributes on both sides of the boundary. This is the property a passing role test does **not** cover — the classifier could read an attribute the disclosure still reports as unconsulted, and a consumer would be told a gap exists where it does not
- [ ] T026 [P] [US3] Test in `waybill-cli/tests/closure_dep_fields.rs`: every stdenv-injected attribute that is **populated on the fixture** still appears in `fields-unconsulted`, so the remaining gap stays visible rather than being silently closed — and an unpopulated one does **not**, since an empty attribute is correctly not a gap (FR-005)
- [ ] T027 [P] [US3] Test in `waybill-cli/tests/closure_dep_fields.rs`: a closure with nothing left unconsulted emits no disclosure at all
- [ ] T028 [US3] Confirm no change is needed in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`'s `read_outcome`, because `consulted_fields()` derives from the two constants T016 widened — and record that in the function's doc as the reason the two cannot drift
- [ ] T029 [US3] Update the C204 expectations for `nix-closure-moat` in `waybill-cli/tests/corpus_harness_195/expectations.rs`

**Checkpoint**: the disclosure is honest about the narrowed gap.

---

## Phase 6: User Story 4 — an unknown attribute is loud (Priority: P3)

**Goal**: a dependency attribute nobody has seen is reported, not silently
treated as absent.

**Independent test**: removing a newly-consulted attribute from the constant
makes a test fail; adding an unknown attribute to a fixture makes it appear in
the disclosure.

- [ ] T030 [P] [US4] Test FR-007 in `waybill-cli/tests/closure_dep_fields.rs`: a fixture declaring a plausible future attribute (`depsHostTarget`, which nixpkgs does not currently emit) names it in the disclosure rather than ignoring it
- [ ] T031 [US4] Test SC-007 in `waybill-cli/tests/closure_dep_fields.rs` as a **mutation**: with `propagatedNativeBuildInputs` removed from `TOOLING_FIELDS`, the US2 tests go red. A widened field set no test is sensitive to is not evidence that the widening does anything
- [ ] T032 [US4] Test SC-011 in `waybill-cli/tests/closure_dep_fields.rs`: a member named only by a stdenv default has the **same role** with and without the marker emitted. This is what keeps the census attributable to the four consulted attributes alone

**Checkpoint**: the next attribute nixpkgs adds cannot hide.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T033 [P] Record the before/after census per attribute in `specs/1279-closure-dep-field-coverage/measurements/README.md` per FR-009, labelled by system — the linux and darwin figures differ five-fold on the FR-002 population and an unlabelled figure is the defect #1348's research shipped
- [ ] T034 Run the local gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with **no `+toolchain`** (#1189). Assert on `GATE_RESULT`, the expect line, and the per-target `test result: ok. N passed; 0 failed` lines
- [ ] T035 Commit the refresh prediction **before** dispatching it, in `specs/1279-closure-dep-field-coverage/measurements/refresh-prediction.md`: the census `270/118/42/1319 → 268/121/45/1315`, 7 derivations moving in the three measured directions, `numactl` losing its scope, 2 newly emitted components, 3 components gaining the marker, and which rows are least certain
- [ ] T036 Refresh the corpus goldens through the `public-corpus.yml` workflow dispatch. **Never** set `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` locally
- [ ] T037 Score the T035 prediction against the refresh diff in `specs/1279-closure-dep-field-coverage/measurements/refresh-prediction.md`, including any miss and what it corrected, and satisfy SC-004's accounting clause: **attribute every member whose role changed to the specific newly-consulted attribute that reaches it**. A member whose change is not traceable to one of the four is a defect in the measurement, not a finding about nix — recording a census without that attribution would leave SC-004 half-met
- [ ] T038 Confirm the regen run's own log reports a non-zero `corpus-regen-golden: … CHANGED` count for `nix-closure-moat` and zero for every other target, using the per-format verdicts added to `waybill-cli/tests/corpus_harness_195/layer2_golden.rs` by #1345 — a directory upload contributes bytes for targets that never wrote
- [ ] T039 Check every target under `waybill-cli/tests/fixtures/public_corpus/` for movement outside `nix-closure-moat` (SC-008) — a non-closure target that moved means the widening leaked past the two constants
- [ ] T040 Reconcile `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml` with the refreshed goldens: retire what the fix makes stale, declare any new offender with its reason and count
- [ ] T041 [P] Satisfy FR-008's second half: dispatch a compare-mode corpus run of `.github/workflows/public-corpus.yml` (`regen_goldens=false`) and confirm the regen count in its log is **0**, so the run is known to have compared rather than rewritten. This is the only execution of the expectations T021/T029 add: locally the target prints `skipping nix-closure-moat: WAYBILL_RUN_PUBLIC_CORPUS not set` and reports `ok` in 0.00s
- [ ] T042 [P] Record anything this milestone corrected about its own reasoning in `specs/1279-closure-dep-field-coverage/measurements/findings.md`
- [ ] T043 Run the `quickstart.md` validation end to end, including the control that the probe's `before` arm equals the golden's own C184 census exactly, and the six traps

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001–T004)**: no dependencies, all parallel
- **Foundational (T005–T008)**: T005 → T006 → T007 in order (same file, then threading); T008 needs Setup. **Blocks every user story**
- **US1 (T009–T021, plus T044)**: after Foundational. **T044 belongs to this phase despite its number** — it was added after the list was first written, and appending it was chosen over inserting it so no existing ID moved and no citation broke
- **US2 (T022–T024)**, **US3 (T025–T029)**: after Foundational; independent of each other
- **US4 (T030–T032)**: after US1 and US2 — T031 mutates what they assert
- **Polish (T033–T043)**: after every story being delivered

### Story Dependencies

US1 carries the whole classification change (T016) and the marker. US2 and US3
are then **tests plus doc corrections** — T024 and T028 exist to confirm that
T016 alone satisfies them, which is the design working rather than a gap.
Stated plainly instead of claimed as independence. US4 is the only story that
depends on another, because a mutation test needs something to mutate.

### Within Each Story

Tests before implementation. T031 must be able to fail: run it against the
pre-T016 constants and confirm red before relying on it.

### Parallel Opportunities

- T001–T004 together
- T009–T014 and T044 together (one file, independent cases)
- T022/T023, T025–T027, T030 pairwise
- T033, T041, T042 together

---

## Parallel Example: User Story 1

```bash
# The contract properties, independent cases in one file:
Task: "T009 each newly-consulted attribute credits the expected ROLE"
Task: "T010 a stdenv-only member gets the marker and no role"
Task: "T011 a declared-but-empty attribute credits nothing"
Task: "T012 consulted-but-never-declared is not a gap"
Task: "T013 the union rule is unchanged"
Task: "T014 the marker names its attributes, sorted"
```

---

## Implementation Strategy

### MVP — US1 only

Setup → Foundational → US1 → stop and validate. That alone makes
`unreferenced` mean one thing: four attributes consulted, the rest disclosed,
and `numactl` no longer claiming to be absent from an artifact nix references
it into.

### Incremental delivery

US1 (MVP) → US2 → US3 → US4 → Polish. Only Polish's golden refresh is shared,
which is why T036 comes after every output-changing task — the refresh is the
expensive step, and a story left half-landed costs a second one.

### Three sequencing constraints worth repeating

- **T018 and T019 land together.** A documented catalog row that nothing emits
  fails the catalog's own completeness test.
- **T035 before T036.** The prediction is committed before the dispatch, or it
  is a description.
- **T031 must be red before T016 is trusted.** A mutation test written after
  the change it is meant to detect proves nothing.
