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

- [ ] T001 Verify `1277-closure-role-scope` is checked out and current with `main` (`5a0fa58` or later), and that `git log --oneline main..HEAD` shows only this feature's spec/plan commits, in the repo root.
- [ ] T002 Re-run the committed baseline harness `python3 -I specs/1277-closure-role-scope/measurements/roles-and-scopes.py` and confirm it still reports 231/34/80/17, ten already-scoped components, and 1 target with roles / 16 without. A figure that has moved means `main` changed under the spec and every success criterion needs re-deriving before any code is written.
- [ ] T003 [P] Record the pre-feature `scope` census per corpus target into `specs/1277-closure-role-scope/measurements/baseline.md` as a committed table — target, components, components with a `scope`, and the value — so US1's byte-identity control (FR-008) has a stated before rather than an implied one.

---

## Phase 2: Foundational (blocking prerequisites)

- [ ] T004 Add the two-state signal to `waybill_common::resolution::ResolvedComponent` in `waybill-common/src/resolution.rs` per `data-model.md`: a closed enum with one meaningful variant (`NotInArtifact`) plus `Option` absence, `serde`-derived, documented with why `lifecycle_scope` and `build_inclusion` were both rejected (research R1, R2).
- [ ] T005 Update every `ResolvedComponent` literal in the tree to initialise the new field, by building with `cargo build --all-targets` — not `cargo build -p waybill`, which compiles only the bin and silently skips literal test helpers (`feedback_build_check_all_targets`).
- [ ] T006 Create `waybill-cli/src/resolve/closure_role_scope.rs` with the pass signature taking `&mut [ResolvedComponent]` and `&Option<BTreeSet<String>>` (the m1276 boot set), returning a summary struct carrying the per-role assigned counts and the exempted count, and register the module in `waybill-cli/src/resolve/mod.rs`.

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

- [ ] T007 [P] [US1] Write the role→scope mapping tests in `waybill-cli/src/resolve/closure_role_scope.rs`'s test module, one per row of `contracts/role-scope.md`: `build-tooling` → set, `unreferenced` → set, `artifact-input` → unset, `both` → unset, absent role → unset. Tests precede the implementation and MUST fail first.
- [ ] T008 [P] [US1] Write the unioned-role test asserting the scope derives from the union and never from a single variant (FR-003), using a component whose variants disagree the way `flex 2.6.4` does in a real closure (`Both` as one variant, `Unreferenced` as another).
- [ ] T009 [P] [US1] Write the fail-closed tests (FR-007): a degraded closure tier and an absent role each assign nothing, and an absent role is never treated as `build-tooling`.
- [ ] T010 [US1] Write `waybill-cli/tests/closure_role_scope.rs` as an end-to-end test driving the closure tier through the stub `nix` that `nix_config_closure.rs` established, with a seeded package-set cache. **Point `WAYBILL_NIXPKGS_CACHE` at a *seeded* tempdir, not an empty one** — an empty directory does not make the test offline, it makes `enrich` fetch 16 MB (m1275 F8, m1276 F8; the same error twice).

### Implementation for User Story 1

- [ ] T011 [US1] Implement the pass body in `waybill-cli/src/resolve/closure_role_scope.rs`: read `extra_annotations["waybill:closure-role"]` per component, match the role exhaustively, set the signal for `build-tooling` and `unreferenced`, and leave `artifact-input`, `both` and absent untouched.
- [ ] T012 [US1] Wire the single call site in `waybill-cli/src/cli/scan_cmd.rs`, immediately after `propagate_non_runtime_scope`, passing the `boot_libraries` m1276 already threads there. Emit one INFO line naming the per-role counts and the exempted count.
- [ ] T013 [US1] Emit the native CDX `scope: "excluded"` in `waybill-cli/src/generate/cyclonedx/builder.rs` from the new signal, **outside the `include_dev` gate**, placed beside the m112 `BuildInclusion::NotNeeded` block it mirrors, with a comment stating the rationale: nix's dependency fields are a build-graph fact, not a lifecycle-scope preference the operator can toggle (research R2).
- [ ] T014 [US1] Verify FR-002 holds on real data: the ten components that already carry a scope — nine `artifact-input` and one `both`, all test frameworks — keep it byte-identical. Assert in `waybill-cli/tests/closure_role_scope.rs` against a fixture that reproduces that shape, not only against the corpus.
- [ ] T015 [US1] Verify FR-008 by running the three regression suites that read non-Nix goldens (`cdx_regression`, `spdx_regression`, `spdx3_regression`) and confirming `test result: ok` with zero golden drift.

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

- [ ] T016 [US2] Build the fixture under `waybill-cli/tests/fixtures/closure_role_scope/exempt/` so the exemption is the **only** thing preventing the scope. This is the task most likely to be done wrong: m1275's F7 found that the test its spec called most important could not fail, because two independent rules protected the same case. Before writing the assertion, **delete the exemption locally and confirm the test fails**; if it still passes, the fixture is not exercising FR-005 and must be rebuilt.
- [ ] T017 [US2] Implement the exemption in `closure_role_scope.rs`: skip any component that is Haskell and whose name is in the boot set, reading the **boot set** rather than the C170 annotation (research R3 — I9 reads C170 off the emitted document, so two readers of one fact catch each other's disagreement rather than agreeing by construction).
- [ ] T018 [US2] Assert `None` boot set exempts nothing (FR-005, m1276's posture): unearned exclusion is as dangerous as unearned immunity. Include a negative control — a tooling-role member that is *not* compiler-supplied still carries `excluded`, so the exemption is narrow rather than blanket.
- [ ] T019 [US2] Assert FR-005a: an exempt member keeps its C182 `waybill:closure-role` annotation reading `build-tooling`. The exemption withholds the scope, not the classification.
- [ ] T020 [US2] Run the m1276 suite `cargo test -p waybill --test scope_exemption_record` and the m1275 suite `--test nix_closure_hackage_identity` and confirm both stay green; neither may be edited to accommodate this feature.

**Checkpoint**: the overlap is handled, demonstrated on a fixture that can
fail. I9 is untouched.

---

## Phase 5: User Story 3 — the operator can tell why a scope was set (Priority: P3)

**Goal**: each scoped component names the mechanism that classified it;
a component with no scope carries no such record.

- [ ] T021 [US3] Add catalog row **C203 `waybill:scope-classification-source`** to `docs/reference/sbom-format-mapping.md` with the full Principle V audit performed against the vendored schemas at `waybill-cli/tests/fixtures/schemas/` — **not from memory** — recording that CDX 1.6 `component.scope`, SPDX 2.3 `relationshipType` and SPDX 3 `LifecycleScopeType` each carry the *classification* and none has a slot for its provenance (research R5). State the value set: `closure-role`, `lifecycle-propagation`, `both`, and absence meaning "nothing set a scope".
- [ ] T022 [US3] Add the C203 extractors to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` and the `ParityExtractor` row to `mod.rs`. **`EXTRACTORS` must stay sorted by row id** (`extractors_table_is_sorted_by_row_id`), and a doc row without a matching extractor fails `every_catalog_row_has_an_extractor` + `holistic_parity` — so T021 and T022 land together, never separately.
- [ ] T023 [US3] Emit C203 per scoped component from `closure_role_scope.rs`'s summary plus the existing `lifecycle_scope`, in CDX (`cyclonedx/builder.rs`), SPDX 2.3 (`spdx/annotations.rs`) and SPDX 3 (`spdx/v3_annotations.rs`). Word the value as the provenance of the **classification**, not of the CycloneDX field — two of the three formats have no such field (research R5).
- [ ] T024 [US3] Assert the three-state coverage in `waybill-cli/tests/closure_role_scope.rs`: role-set, propagation-set, and **absent** — the third is the one that silently passes if the annotation is emitted unconditionally. m1276's US3 shipped exactly that defect and the gate caught it as 35 byte-identity failures across 11 ecosystems.
- [ ] T025 [US3] Confirm `both` is currently the empty set and say so in the test's comment with the measurement behind it: the ten already-scoped components are all in roles US1 skips, so no component is in both populations today. A value that can never occur must be reachable by construction or it is dead code asserted to be correct.

---

## Phase 6: Polish & cross-cutting concerns

### The invariant

- [ ] T026 Add invariant **I10** — no member whose closure role is `build-tooling` or `unreferenced` lacks a non-runtime scope — as `waybill-cli/tests/corpus_harness_195/invariants/checks/closure_role_scope.rs`, registered in `checks/mod.rs` and in `invariants/mod.rs` beside I9's entry, which takes `id`, `statement`, `precondition`, `kinds` and `checker`. The **precondition matters**: it must be "at least one component carries a closure role", so the row reports `-` (n/a) on the 16 targets that emit none rather than passing vacuously on them — the shape I9 uses for its own zero-population case.
- [ ] T027 Document I10 in `docs/reference/sbom-invariants.md` — its row and its `### I10` diagnostic section, both of which `catalog_and_registry_agree` gates.
- [ ] T028 State I10's boundary with I9 and I3 explicitly in the module header, the way I3 and I9 document theirs: I10 asserts the scope is **present** for a tooling role, I9 asserts it is **absent** for a compiler-supplied component, and the exemption is where they meet. I3's own header records getting a boundary wrong once, putting two instances of one defect class under the wrong number.
- [ ] T029 Prove I10 has teeth with a mutation: remove a `build-tooling` member's scope from a synthetic document and confirm I10 fails (SC-006). An invariant that never fails is indistinguishable from one that never runs.

### FR-004a — the probe

- [ ] T030 Write the `#[ignore]`d, env-gated probe `m1277_field_coverage` in `waybill-cli/src/scan_fs/package_db/nix/closure/classify.rs`'s test module: read a closure JSON from `WAYBILL_T_CLOSURE_FIELDS`, enumerate the distinct dependency-shaped env keys present, and report every key that neither `ARTIFACT_FIELDS` nor `TOOLING_FIELDS` names.
- [ ] T031 Run T030 against a real closure and record the result in `specs/1277-closure-role-scope/measurements/findings.md`. **Do not substitute a grep over the in-tree fixtures** — they carry only the seven keys the code already knows, because they were written from those lists, so the answer they give is the answer they were built from (research R7).
- [ ] T032 Satisfy SC-009 by recording the count as *known*, not as zero. A non-zero answer is a finding to scope — it would mean `unreferenced` conflates "nothing references it" with "we did not read the reference", making FR-004's assignment unsound for that member — and it gets its own issue rather than being absorbed into this milestone.

### Mutation testing

- [ ] T033 [P] Plant and catch a mutant per rule: (M1) `build-tooling` assigns nothing; (M2) `artifact-input` assigns `excluded`; (M3) the exemption is removed; (M4) the role is read from one variant rather than the union; (M5) the CDX emission moves back inside the `include_dev` gate; (M6) C203 is emitted unconditionally. Record each in `measurements/mutations.md` with which test caught it.
- [ ] T034 For any mutant that **survives**, decide whether it is an invalid mutation or a real gap, and say which in writing. m1276 discarded two candidates as invalid rather than counting them — one mutated an optimisation, one could not be caught while the committed goldens still carried the markers the fix removed — and replaced them rather than claiming six of six.

### Goldens

- [ ] T035 Write the refresh prediction to `specs/1277-closure-role-scope/measurements/refresh-prediction.md` **before dispatching the run**, split into what rests on a measurement and what on an inference. Expected: `nix-closure-moat` gains 97 `scope: excluded` and 97 C203 values; the other 16 targets byte-identical; the 10 already-scoped components unchanged.
- [ ] T036 Dispatch `gh workflow run "Public corpus regression" -f branch=1277-closure-role-scope -f regen_goldens=true`, then audit the artifact by **enumerating the files per target** (#1286 — a directory upload contributes committed bytes for a target that never wrote) and by the files written rather than any SHA the run reports (#1287). Note that in regen mode the summary's "Drifted" heading lists failed tests, not changed goldens (#1345), so it is not the audit.
- [ ] T037 Check the diff against T035's prediction and treat any mismatch as a finding to run down, recording the scorecard in `measurements/findings.md` including the misses.
- [ ] T038 Install the regenerated goldens and re-run the gate. The goldens change the tree, so the gate must run again after installing them — not before.

### Docs and close-out

- [ ] T039 [P] Update `CLAUDE.md`'s milestone entry if `/speckit.plan`'s agent-context script has not already covered it, and confirm no stray `*.actual.json` is staged (the ignore rule now covers `waybill-cli/tests/**`, fixed on this branch's parent).
- [ ] T040 File the Principle V follow-up issue for research **R6**: SPDX 2.3 `BUILD_TOOL_OF` and SPDX 3 `LifecycleScopedRelationship` + `LifecycleScopeType/build` are native carriers this milestone does not emit, and the constitution names this family as its canonical case. Cite that m112 — the model this feature follows — is itself CDX-native plus an SPDX annotation, and that widening is a spec change because new relationships for 97 components change the relationship graph that feeds graph completeness, orphan reasons and the I-series. End with the required disclosure line.
- [ ] T041 Run the full gate `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` and report the positive signal count plus the per-target `test result: ok. N passed; 0 failed` lines. Assert on the positive signal, never on the absence of the word FAILED — a build that never compiled also contains no failures.
- [ ] T042 Open the PR against `main` with the measured before/after, the mutation scorecard, the prediction scorecard, and R6 named as a known gap with its follow-up issue number.

---

## Dependencies

```text
Phase 1 (T001-T003)  →  Phase 2 (T004-T006)  →  ┬→ Phase 3 US1 (T007-T015)  🎯 MVP
                                                 ├→ Phase 4 US2 (T016-T020)   depends on US1's pass existing
                                                 └→ Phase 5 US3 (T021-T025)   depends on US1's summary
                                                        ↓
                                                 Phase 6 (T026-T042)
```

- **US1 is the MVP** and is independently shippable: it closes the defect for 97 of 97 affected components.
- **US2 needs US1's pass to exist** (it adds a skip to it) but no US1 *test*; it can start as soon as T011 lands.
- **US3 needs US1's summary struct** (T006/T012) and is otherwise independent of US2.
- **T021 and T022 must land in the same commit** — a catalog row without its extractor fails two tests.
- **T038 must follow T036/T037**, never precede them.

## Parallel opportunities

- T007, T008, T009 are three test files' worth of work in one module — write them together, they share no assertion.
- T033's six mutants are independent of each other once US1–US3 are in.
- T039 and T040 are independent of everything in Phase 6.

## Implementation strategy

Ship **US1 alone** first if the milestone must be cut short: it is the whole
of #1342's reported defect, its population is non-empty on real data, and it
needs no new fixture. US2 is correctness insurance for a population measured
at zero — necessary, but undemonstrable without the fixture T016 builds, so
it cannot be the MVP. US3 is transparency and earns its place only because
the ten already-scoped components make the two mechanisms genuinely
confusable.
