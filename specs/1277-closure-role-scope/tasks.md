# Tasks: a closure member that only builds the artifact must not read as part of it

**Feature**: 1277-closure-role-scope · **Issue**: #1342
**Spec**: [spec.md](spec.md) · **Plan**: [plan.md](plan.md) · **Research**: [research.md](research.md)

## Format: `[ID] [P?] [Story] Description`

`[P]` = parallelisable (different files, no dependency on an incomplete
task). `[US1]`/`[US2]`/`[US3]` label user-story phases only.

## Path Conventions

Single Rust workspace. `waybill-common` for the type, `waybill-cli` for the
pass, the emitters and the tests. `waybill-ebpf` untouched.

## Standing rules for every task here

- **The gate is `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain` argument** (#1189). A passing per-crate `cargo test` is not evidence.
- **`resolve` and `scan_fs` unit tests live in the BIN target.** `--lib` reports `0 passed; N filtered out` for a test that exists.
- **Test modules that use `.unwrap()`** need `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))] mod tests {`.
- **Match enums exhaustively, never with `matches!(x, A|B|C)`** — #892 survived about two years behind such an allowlist.
- **Fixtures use `waybill-fixture-*` names only** (#285, #640).
- **Goldens are refreshed through `public-corpus.yml` only, never locally** (rule zero).

---

## Phase 1: Setup

- [X] T001 Verify `1277-closure-role-scope` is checked out and current with `main` (`5a0fa58` or later), and that `git log --oneline main..HEAD` shows only this feature's spec/plan commits, in the repo root.
- [X] T002 Re-run the committed baseline harness `python3 -I specs/1277-closure-role-scope/measurements/roles-and-scopes.py` and confirm it still reports 231/34/80/17, ten already-scoped components, and 1 target with roles / 16 without. A figure that has moved means `main` changed under the spec and every success criterion needs re-deriving before any code is written.
- [X] T003 [P] Record the pre-feature `scope` census per corpus target into `specs/1277-closure-role-scope/measurements/baseline.md` as a committed table — target, components, components with a `scope`, and the value — so US1's byte-identity control (FR-008) has a stated before rather than an implied one.

---

## Phase 2: Foundational (blocking prerequisites)

- [X] T004 Add the two-state signal to `waybill_common::resolution::ResolvedComponent` in `waybill-common/src/resolution.rs` per `data-model.md`: a closed enum with one meaningful variant (`NotInArtifact`) plus `Option` absence, `serde`-derived, documented with why `lifecycle_scope` and `build_inclusion` were both rejected (research R1, R2).
- [X] T005 Update every `ResolvedComponent` literal in the tree to initialise the new field, by building with `cargo build --all-targets` — not `cargo build -p waybill`, which compiles only the bin and silently skips literal test helpers (`feedback_build_check_all_targets`).
- [X] T006 Create `waybill-cli/src/resolve/closure_role_scope.rs` with the pass signature taking `&mut [ResolvedComponent]` and `&Option<BTreeSet<String>>` (the m1276 boot set), returning a summary struct carrying the per-role assigned counts and the exempted count, and register the module in `waybill-cli/src/resolve/mod.rs`.

**Checkpoint**: the type exists and the workspace builds `--all-targets`. No behaviour has changed yet.

---

## Phase 3: User Story 1 — a build tool stops reading as shipped (Priority: P1) 🎯 MVP

**Goal**: a member nix referenced only through its tooling fields, or
through neither field set, carries `scope: excluded`.

**Independent test**: scan a Nix-built project with `--nix-closure`; every
`build-tooling` and `unreferenced` member carries `excluded`, every
`artifact-input` and `both` member's scope is unchanged, and every
non-`--nix-closure` document is byte-identical.

### Tests for User Story 1

- [X] T007 [P] [US1] Write the role→scope mapping tests in `waybill-cli/src/resolve/closure_role_scope.rs`'s test module, one per row of `contracts/role-scope.md`: `build-tooling` → set, `unreferenced` → set, `artifact-input` → unset, `both` → unset, absent role → unset. Tests precede the implementation and MUST fail first.
- [X] T008 [P] [US1] **Resolved upstream, not here — see the test's doc comment.** Write the unioned-role test asserting the scope derives from the union and never from a single variant (FR-003), using a component whose variants disagree the way `flex 2.6.4` does in a real closure (`Both` as one variant, `Unreferenced` as another).
- [X] T009 [P] [US1] Write the fail-closed tests (FR-007): a degraded closure tier and an absent role each assign nothing, and an absent role is never treated as `build-tooling`.
- [X] T010 [US1] Write `waybill-cli/tests/closure_role_scope.rs` as an end-to-end test driving the closure tier through the stub `nix` that `nix_config_closure.rs` established, with a seeded package-set cache. **Point `WAYBILL_NIXPKGS_CACHE` at a *seeded* tempdir, not an empty one** — an empty directory does not make the test offline, it makes `enrich` fetch 16 MB (m1275 F8, m1276 F8; the same error twice).

### Implementation for User Story 1

- [X] T011 [US1] (FR-001, FR-003, FR-004) Implement the pass body in `waybill-cli/src/resolve/closure_role_scope.rs`: read `extra_annotations["waybill:closure-role"]` per component, match the role exhaustively, set the signal for `build-tooling` and `unreferenced`, and leave `artifact-input`, `both` and absent untouched.
- [X] T012 [US1] Wire the single call site in `waybill-cli/src/cli/scan_cmd.rs`, immediately after `propagate_non_runtime_scope`, passing the `boot_libraries` m1276 already threads there. Emit one INFO line naming the per-role counts and the exempted count.
- [X] T013 [US1] (FR-001, FR-004, SC-001, SC-001a) Emit the native CDX `scope: "excluded"` in `waybill-cli/src/generate/cyclonedx/builder.rs` from the new signal, **outside the `include_dev` gate**, placed beside the m112 `BuildInclusion::NotNeeded` block it mirrors, with a comment stating the rationale: nix's dependency fields are a build-graph fact, not a lifecycle-scope preference the operator can toggle (research R2).
- [X] T014 [US1] (FR-002, SC-002) Verify FR-002 holds on real data: the ten components that already carry a scope — nine `artifact-input` and one `both`, all test frameworks — keep it byte-identical. Assert in `waybill-cli/tests/closure_role_scope.rs` against a fixture that reproduces that shape, not only against the corpus.
- [X] T015 [US1] (FR-008, SC-004) Verify FR-008 by running the three regression suites that read non-Nix goldens (`cdx_regression`, `spdx_regression`, `spdx3_regression`) and confirming `test result: ok` with zero golden drift.

**Checkpoint**: US1 is independently shippable. 97 of 362 members on
`nix-closure-moat` now carry `excluded`; nothing outside `--nix-closure`
moves.

---

## Phase 4: User Story 2 — the m1276 exemption is not re-broken (Priority: P2)

**Goal**: a compiler-supplied member acquires no role-derived scope, and
invariant I9 stays unamended.

**Independent test**: a synthetic fixture whose closure places a nulled
package in `nativeBuildInputs`. **Without** the rule the component acquires
`excluded` and I9 fails; **with** it the component carries its
`build-tooling` role and no scope.

- [X] T016 [US2] Build the fixture under `waybill-cli/tests/fixtures/closure_role_scope/exempt/` so the exemption is the **only** thing preventing the scope. This is the task most likely to be done wrong: m1275's F7 found that the test its spec called most important could not fail, because two independent rules protected the same case. Before writing the assertion, **delete the exemption locally and confirm the test fails**; if it still passes, the fixture is not exercising FR-005 and must be rebuilt.
- [X] T017 [US2] (FR-005) Implement the exemption in `closure_role_scope.rs`: skip any component that is Haskell and whose name is in the boot set, reading the **boot set** rather than the C170 annotation (research R3 — I9 reads C170 off the emitted document, so two readers of one fact catch each other's disagreement rather than agreeing by construction).
- [X] T018 [US2] Assert `None` boot set exempts nothing (FR-005, m1276's posture): unearned exclusion is as dangerous as unearned immunity. Include a negative control — a tooling-role member that is *not* compiler-supplied still carries `excluded`, so the exemption is narrow rather than blanket.
- [X] T019 [US2] *(FR-005a done at unit level; FR-006's all-four-roles assertion needs T010's end-to-end test)* Assert FR-005a **and FR-006**: an exempt member keeps its C182 `waybill:closure-role` annotation reading `build-tooling` — the exemption withholds the classification's native expression, not the role — and every C182 value is preserved unchanged across **all four roles**, not only the exempt one. FR-006 says "every"; asserting it for one role would leave three untested.
- [X] T020 [US2] Run the m1276 suite `cargo test -p waybill --test scope_exemption_record` and the m1275 suite `--test nix_closure_hackage_identity` and confirm both stay green; neither may be edited to accommodate this feature.

**Checkpoint**: the overlap is handled, demonstrated on a fixture that can
fail. I9 is untouched.

---

## Phase 5: User Story 3 — the operator can tell why a scope was set (Priority: P3)

**Goal**: each scoped component names the mechanism that classified it;
a component with no scope carries no such record.

- [X] T021 [US3] (FR-009, FR-010) Add catalog row **C203 `waybill:scope-classification-source`** to `docs/reference/sbom-format-mapping.md` with the full Principle V audit performed against the vendored schemas at `waybill-cli/tests/fixtures/schemas/` — **not from memory** — recording that CDX 1.6 `component.scope`, SPDX 2.3 `relationshipType` and SPDX 3 `LifecycleScopeType` each carry the *classification* and none has a slot for its provenance (research R5). State the value set: `closure-role`, `lifecycle-propagation`, `both`, and absence meaning "nothing set a scope".
- [X] T022 [US3] Add the C203 extractors to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` and the `ParityExtractor` row to `mod.rs`. **`EXTRACTORS` must stay sorted by row id** (`extractors_table_is_sorted_by_row_id`), and a doc row without a matching extractor fails `every_catalog_row_has_an_extractor` + `holistic_parity` — so T021 and T022 land together, never separately.
- [X] T023 [US3] (FR-009) Emit C203 per scoped component from `closure_role_scope.rs`'s summary plus the existing `lifecycle_scope`, in CDX (`cyclonedx/builder.rs`), SPDX 2.3 (`spdx/annotations.rs`) and SPDX 3 (`spdx/v3_annotations.rs`). Word the value as the provenance of the **classification**, not of the CycloneDX field — two of the three formats have no such field (research R5).
- [X] T024 [US3] (FR-009) Assert the three-state coverage in `waybill-cli/tests/closure_role_scope.rs`: role-set, propagation-set, and **absent** — the third is the one that silently passes if the annotation is emitted unconditionally. m1276's US3 shipped exactly that defect and the gate caught it as 35 byte-identity failures across 11 ecosystems.
- [X] T025 [US3] Confirm `both` is currently the empty set and say so in the test's comment with the measurement behind it: the ten already-scoped components are all in roles US1 skips, so no component is in both populations today. A value that can never occur must be reachable by construction or it is dead code asserted to be correct.

---

## Phase 6: User Story 4 — the same question answered in SPDX (Priority: P2)

**Goal**: a member in a tooling role carries each SPDX format's own native
construct for "builds the artifact and is not in it", so neither SPDX
consumer is left with the reading CycloneDX now forbids.

**Independent test**: emit SPDX 2.3 and SPDX 3 with `--nix-closure`; every
tooling-role member carries the native build-tool relation, `artifact-input`
and `both` members' relations are unchanged, and every I-series row passes.

- [X] T043 [P] [US4] (FR-012) Write the SPDX 2.3 tests in `waybill-cli/tests/closure_role_scope.rs`: a `build-tooling` member and an `unreferenced` member each carry `BUILD_TOOL_OF` to the artifact; an `artifact-input` member's relations are unchanged. Tests precede the implementation and MUST fail first.
- [X] T044 [P] [US4] (FR-013) Write the SPDX 3 tests: the same two members each carry a `LifecycleScopedRelationship` whose `scope` is `LifecycleScopeType/build`. Read the enum's members from the vendored `waybill-cli/tests/fixtures/schemas/spdx-3.0.1.json`, not from memory.
- [X] T045 [US4] (FR-012) Emit `BUILD_TOOL_OF` from the new signal in `waybill-cli/src/generate/spdx/relationships.rs`, relating the member to the artifact. Reuse the existing relationship-construction path rather than appending raw; m052 already emits `BUILD_DEPENDENCY_OF` from `lifecycle_scope` there, and the two must not double up on one component.
- [X] T046 [US4] (FR-013) Emit the `LifecycleScopedRelationship` in the SPDX 3 path, and validate the result with the pinned `spdx3-validate==0.0.5` at `.venv/spdx3-validate/bin/spdx3-validate` (milestone 078's gate) — a new relationship class is exactly the change that passes a round-trip and fails conformance.
- [X] T047 [US4] (FR-014) Assert the FR-005 exemption governs all three formats: a compiler-supplied member in a tooling role gains no CycloneDX scope, no `BUILD_TOOL_OF` and no build-scoped relationship. Extend T016's fixture rather than building a second one, and re-run T016's own control — delete the exemption and confirm all three assertions fail.
- [X] T048 [US4] (SC-011) *(local half done — I4's rule transcribed and asserted on the fixture; the corpus half is T035/T037)* Predict, then verify, how the added relations move graph completeness, the orphan-reason rows and each I-series row. 97 new relations change the graph those checks read, so state the expected movement in `measurements/refresh-prediction.md` **before** the refresh and treat an unpredicted change as a finding. An orphan that stops being an orphan because it gained a relation is the likeliest shape.

**Checkpoint**: all three formats answer the question natively. Principle V
is satisfied without a `waybill:*` substitute anywhere in the mapping.

---

## Phase 7: Polish & cross-cutting concerns

### The invariant

- [X] T026 Add invariant **I10** (FR-011, SC-006) — no member whose closure role is `build-tooling` or `unreferenced` lacks a non-runtime marker, covering **both** FR-001 and FR-004; an invariant naming only `build-tooling` would leave the 17 `unreferenced` members unguarded — as `waybill-cli/tests/corpus_harness_195/invariants/checks/closure_role_scope.rs`, registered in `checks/mod.rs` and in `invariants/mod.rs` beside I9's entry, which takes `id`, `statement`, `precondition`, `kinds` and `checker`. The **precondition matters**: it must be "at least one component carries a closure role", so the row reports `-` (n/a) on the 16 targets that emit none rather than passing vacuously on them — the shape I9 uses for its own zero-population case.
- [X] T027 Document I10 in `docs/reference/sbom-invariants.md` — its row and its `### I10` diagnostic section, both of which `catalog_and_registry_agree` gates.
- [X] T028 State I10's boundary with I9 and I3 explicitly in the module header, the way I3 and I9 document theirs: I10 asserts the scope is **present** for a tooling role, I9 asserts it is **absent** for a compiler-supplied component, and the exemption is where they meet. I3's own header records getting a boundary wrong once, putting two instances of one defect class under the wrong number.
- [X] T029 (SC-006) Prove I10 has teeth with a mutation: remove a `build-tooling` member's scope from a synthetic document and confirm I10 fails (SC-006). An invariant that never fails is indistinguishable from one that never runs.

### FR-004a — the probe

- [X] T030 Write the `#[ignore]`d, env-gated probe `m1277_field_coverage` in `waybill-cli/src/scan_fs/package_db/nix/closure/classify.rs`'s test module: read a closure JSON from `WAYBILL_T_CLOSURE_FIELDS`, enumerate the distinct dependency-shaped env keys present, and report every key that neither `ARTIFACT_FIELDS` nor `TOOLING_FIELDS` names.
- [X] T031 Run T030 against a real closure and record the result in `specs/1277-closure-role-scope/measurements/findings.md`. **Do not substitute a grep over the in-tree fixtures** — they carry only the seven keys the code already knows, because they were written from those lists, so the answer they give is the answer they were built from (research R7).
- [X] T032 Satisfy SC-009 by recording the count as *known*, not as zero. A non-zero answer is a finding to scope — it would mean `unreferenced` conflates "nothing references it" with "we did not read the reference", making FR-004's assignment unsound for that member — and it gets its own issue rather than being absorbed into this milestone.

### Mutation testing

- [X] T033 [P] Plant and catch a mutant per rule: (M1) `build-tooling` assigns nothing; (M2) `artifact-input` assigns `excluded`; (M3) the exemption is removed; (M4) the role is read from one variant rather than the union; (M5) the CDX emission moves back inside the `include_dev` gate; (M6) C203 is emitted unconditionally. Record each in `measurements/mutations.md` with which test caught it.
- [X] T034 For any mutant that **survives**, decide whether it is an invalid mutation or a real gap, and say which in writing. m1276 discarded two candidates as invalid rather than counting them — one mutated an optimisation, one could not be caught while the committed goldens still carried the markers the fix removed — and replaced them rather than claiming six of six.

### Goldens

- [ ] T035 Write the refresh prediction to `specs/1277-closure-role-scope/measurements/refresh-prediction.md` **before dispatching the run**, split into what rests on a measurement and what on an inference. Expected: `nix-closure-moat` gains 97 `scope: excluded` and 97 C203 values; the other 16 targets byte-identical; the 10 already-scoped components unchanged.
- [ ] T036 Dispatch `gh workflow run "Public corpus regression" -f branch=1277-closure-role-scope -f regen_goldens=true`, then audit the artifact by **enumerating the files per target** (#1286 — a directory upload contributes committed bytes for a target that never wrote) and by the files written rather than any SHA the run reports (#1287). Note that in regen mode the summary's "Drifted" heading lists failed tests, not changed goldens (#1345), so it is not the audit.
- [ ] T037 Check the diff against T035's prediction and treat any mismatch as a finding to run down, recording the scorecard in `measurements/findings.md` including the misses.
- [ ] T038 Install the regenerated goldens and re-run the gate. The goldens change the tree, so the gate must run again after installing them — not before.

### Docs and close-out

- [ ] T039 [P] Update `CLAUDE.md`'s milestone entry if `/speckit.plan`'s agent-context script has not already covered it, and confirm no stray `*.actual.json` is staged (the ignore rule now covers `waybill-cli/tests/**`, fixed on this branch's parent).
- [ ] T040 Self-audit the Principle V mapping before opening the PR: walk every row of `contracts/role-scope.md` and confirm each classification is carried by a **native** construct in all three formats, and that the only `waybill:*` row this feature adds (C203) carries provenance, which no format expresses. This replaces the follow-up issue an earlier draft of the plan would have filed — `/speckit.analyze` C1 brought that gap into scope as US4 instead, because Principle V's parity-bridge carve-out applies only where a format lacks the native field, and SPDX does not lack it.
- [ ] T049 (SC-003, SC-008, SC-012) Produce the post-feature census with the **same committed harness** as the baseline — `python3 -I specs/1277-closure-role-scope/measurements/roles-and-scopes.py`, extended to report the scope and the C203 value per role — and diff it against T003's committed before-census. Assert all five counts in one place: 80 and 17 newly scoped, 265 untouched, the 10 pre-existing unchanged, 16 targets byte-identical. A count verified only through a golden diff is verified against a prediction; this verifies it against a measurement.
- [ ] T050 (SC-005) Assert `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml` gains **no I9 row and no I10 row**. SC-005 requires I9 to pass *with no declaration*, and absence is the criterion — so grep for it rather than inferring it from a green lane. Milestone 1276 needed exactly such declarations temporarily and only its refresh removed them.
- [ ] T041 Run the full gate `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` and report the positive signal count plus the per-target `test result: ok. N passed; 0 failed` lines. Assert on the positive signal, never on the absence of the word FAILED — a build that never compiled also contains no failures.
- [ ] T042 Open the PR against `main` with the measured before/after, the mutation scorecard, the prediction scorecard, and R6 named as a known gap with its follow-up issue number.

---

## Dependencies

```text
Phase 1 (T001-T003)  →  Phase 2 (T004-T006)  →  ┬→ Phase 3 US1 (T007-T015)  🎯 MVP
                                                 ├→ Phase 4 US2 (T016-T020)
                                                 ├→ Phase 5 US3 (T021-T025)
                                                 └→ Phase 6 US4 (T043-T048)
                                                        ↓
                                                 Phase 7 (T026-T042, T049-T050)
```

Task IDs are stable across this revision and therefore not monotonic by
phase: US4's tasks are T043-T048 and the two new polish tasks are
T049-T050, appended rather than inserted. Renumbering would have moved every
reference in this file and in the commit trail, which is the hazard
`feedback_renumber_by_anchor_not_arithmetic` records — three distinct ways an
ID reference breaks on insertion.

- **US1 is the MVP** and is independently shippable: it closes the defect for 97 of 97 affected components.
- **US2 needs US1's pass to exist** (it adds a skip to it) but no US1 *test*; it can start as soon as T011 lands.
- **US3 needs US1's summary struct** (T006/T012) and is otherwise independent of US2.
- **US4 needs US1's signal** (T004) but not US1's CycloneDX emission; T043-T046 can run alongside T013.
- **T047 extends T016's fixture**, so US4's exemption task follows US2's.
- **T021 and T022 must land in the same commit** — a catalog row without its extractor fails two tests.
- **T038 must follow T036/T037**, never precede them.

## Parallel opportunities

- T007, T008, T009 are three test files' worth of work in one module — write them together, they share no assertion.
- T033's six mutants are independent of each other once US1–US4 are in. Add a seventh and an eighth for US4: the SPDX 2.3 relation omitted, and the exemption honoured in CycloneDX but not in SPDX — the second is the shape FR-014 exists to forbid.
- T039 and T040 are independent of everything in Phase 6.

## Implementation strategy

Ship **US1 alone** first if the milestone must be cut short: it is the whole
of #1342's reported defect, its population is non-empty on real data, and it
needs no new fixture. US2 is correctness insurance for a population measured
at zero — necessary, but undemonstrable without the fixture T016 builds, so
it cannot be the MVP. US3 is transparency and earns its place only because
the ten already-scoped components make the two mechanisms genuinely
confusable.
