# Tasks: Transitive runtime closure for Nix-built Haskell projects

**Feature**: milestone 985 | **Issue**: #962 | **Branch**: `985-nix-haskell-runtime-closure`
**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/closure-contract.md](./contracts/closure-contract.md)

## Format: `[ID] [P?] [Story] Description`

- **[P]** — parallelizable: different file, no dependency on an incomplete task
- **[US1]–[US4]** — the user story the task serves; absent on Setup/Foundational/Polish

## Path Conventions

Single Rust workspace. All production changes land in `waybill-cli/`; shared
types already live in `waybill-common/`. Paths below are repository-relative.

## Why tests are included

The contract carries four verification obligations, and every success criterion
is only checkable through a test. Two rules apply throughout and are not
restated per task:

1. **Every new assertion must be mutation-tested** — shown to fail when the
   behaviour it guards is reverted. A test that has never failed has not been
   shown to test anything.
2. **Corpus goldens are generated through CI, never locally** (rule zero).

---

## Phase 1: Setup (Shared Infrastructure)

- [ ] T001 Capture the pre-feature baseline before any code changes: scan the corpus Haskell target and save all three formats to `/tmp/m985-baseline/`, so contract C-7 / SC-009 can be checked later. Record the command and the commit SHA in the PR description. The committed goldens at `waybill-cli/tests/fixtures/public_corpus/haskell-language-server/` are the other copy of this baseline and MUST NOT be regenerated until T046.
- [ ] T002 Create the closure module skeleton at `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure.rs` with module docs stating the walk's purpose, its termination rule, and a pointer to research R4/R7; register it in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`.
- [ ] T003 [P] Extend the synthetic package set in `waybill-cli/tests/nix_haskell_resolution_m926.rs` (`PACKAGES`) with a transitive chain — a declared package whose `libraryHaskellDepends` names a second package, which names a third — plus one cycle and one name absent from the set. The fixture must contain the shapes the walk has to survive, not only the shape it expects.
- [ ] T004 [P] Extend `waybill-cli/tests/fixtures/nix_haskell/resolvable/waybill-fixture-app.cabal` with a comment block naming each fixture dependency's intended closure role, mirroring the existing per-dependency comments.

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ Blocks every user story — the walk cannot run without relation data.**

- [ ] T005 Add `DependencyRelations` (data-model E1) to `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/package_set.rs`, holding `library` and `executable` name lists.
- [ ] T006 Extend the attribute-keyed parser in `package_set.rs` to extract `libraryHaskellDepends` and `executableHaskellDepends` from each attribute body, in the same pass that extracts `version` and `sha256`. Key on the ATTRIBUTE name, never `pname` (E1.1, research R2, #970).
- [ ] T007 Do NOT extract `testHaskellDepends` or `benchmarkHaskellDepends` (E1.2, FR-002). Add a comment naming issue #985 so the omission reads as a decision rather than an oversight.
- [ ] T008 [P] Unit-test relation extraction in `package_set.rs`: a multi-line dependency list, an attribute with no relations, an attribute with only executable relations, and case-sensitive names (`Diff` ≠ `diff`, E1.4).
- [ ] T009 [P] Unit-test that `testHaskellDepends` present in the input is NOT extracted, so the scope boundary is enforced by a test rather than by memory.
- [ ] T010 Add the `ComponentOrigin` enum (E3) to `closure.rs` as a typed enum with `Declared` and `Transitive` variants — not a bare string (Principle IV).

**Checkpoint**: relation data is available and scoped; no behaviour has changed yet.

---

## Phase 3: User Story 1 — A consumer sees what the project actually depends on (P1) 🎯 MVP

**Goal**: the closure resolves and its members are emitted with the same version, hash and reason treatment declared dependencies receive.

**Independent test**: scan a Nix-built Haskell project; the document contains packages the project never declares, each with a version and a native SHA-256.

### Tests for User Story 1

- [ ] T011 [P] [US1] Add `m985_a_transitive_dependency_is_emitted_with_a_version` to `waybill-cli/tests/nix_haskell_resolution_m926.rs` — asserts a package reachable only through another is present and versioned.
- [ ] T012 [P] [US1] Add `m985_a_transitive_dependency_carries_a_native_source_hash` — same treatment as a declared dependency (FR-004).
- [ ] T013 [P] [US1] Add `m985_the_walk_terminates_on_a_cycle` using the cyclic fixture from T003 (FR-010, R4).
- [ ] T014 [P] [US1] Add `m985_an_unresolvable_transitive_name_is_emitted_versionless_with_a_reason` (FR-005a, C-5).
- [ ] T015 [P] [US1] Add `m985_a_boot_library_in_the_closure_is_not_traversed` — its relations must not appear (FR-011, R5).
- [ ] T016 [P] [US1] Add `m985_each_package_appears_once` for a package reachable by two parents (FR-012, E2.2).

### Implementation for User Story 1

- [ ] T017 [US1] Implement `ClosureMember` (E2) in `closure.rs` with `name`, `origin`, and a `BTreeSet` `reached_from` — `BTree*` throughout, because output must be byte-identical across runs (E2.4, FR-013).
- [ ] T018 [US1] Implement the breadth-first walk in `closure.rs`: seed from the declared set, check `seen` BEFORE enqueue so cycles terminate (FR-010), and do not traverse into boot libraries (FR-011).
- [ ] T019 [US1] Resolve each reached name through the existing `classify` in `mod.rs`, so closure members get identical version, hash and alias handling to declared ones — including the compiler-configuration alias fallback (#984, research R6).
- [ ] T020 [US1] Emit a versionless component with the existing reason vocabulary for any name that does not resolve; introduce NO new reason values (C-5, FR-005a).
- [ ] T021 [US1] Wire the walk into `enrich()` in `mod.rs`, after declared-dependency classification so the declared set is known first.

**Checkpoint**: the closure resolves and emits. Components are present but not yet marked or connected.

---

## Phase 4: User Story 2 — A consumer can tell declared from transitive (P1)

**Goal**: every Haskell component the resolver touched carries an explicit origin.

**Independent test**: every such component has an origin value readable without traversing the graph.

### Tests for User Story 2

- [ ] T022 [P] [US2] Add `m985_every_haskell_component_carries_an_origin` — declared ones included (FR-006a). Absence on any is a failure.
- [ ] T023 [P] [US2] Add `m985_a_component_reachable_both_ways_is_declared` (FR-007, E3.2).
- [ ] T024 [P] [US2] Add `m985_origin_reaches_every_format` — CycloneDX, SPDX 2.3 and SPDX 3 (C-2, `SymmetricEqual`).

### Implementation for User Story 2

- [ ] T025 [US2] Set `ComponentOrigin::Declared` for names in the declared set and `Transitive` for the rest, with declared winning on collision (E3.2, FR-007).
- [ ] T026 [US2] Emit `waybill:nixpkgs-component-origin` per component in CycloneDX via `waybill-cli/src/generate/cyclonedx/` (C-2).
- [ ] T027 [P] [US2] Emit the same in SPDX 2.3 via `waybill-cli/src/generate/spdx/annotations.rs` (C-2).
- [ ] T028 [P] [US2] Emit the same in SPDX 3 via `waybill-cli/src/generate/spdx/v3_annotations.rs` (C-2).
- [ ] T029 [US2] Add the catalog row for `waybill:nixpkgs-component-origin` to `docs/reference/sbom-format-mapping.md`, recording the Principle V audit from research R3 — no format has a native carrier — as the justification.
- [ ] T030 [US2] Add three parity extractors for that row in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` and register it in `mod.rs` with `Directionality::SymmetricEqual`. The row and the extractors land together, or `every_catalog_row_has_an_extractor` fails.

**Checkpoint**: origin is explicit and format-symmetric.

---

## Phase 5: User Story 3 — The dependency graph stays connected (P1)

**Goal**: every closure member is reachable from the root by real edges, and no endpoint dangles.

**Independent test**: no edge endpoint names an absent component, and every transitively-added component has an inbound edge.

**⚠️ Highest-risk phase.** Milestone 980 was exactly this failure at smaller scale — identities rewritten without rewriting endpoints, disconnecting every resolved component. See research R7 and contract C-4.3.

### Tests for User Story 3

- [ ] T031 [P] [US3] Add `m985_no_closure_edge_dangles` — no `dependsOn` target absent from the component set (FR-008, C-4.1).
- [ ] T032 [P] [US3] Add `m985_an_edge_comes_from_the_actual_parent_not_the_root` (FR-009, C-4.2).
- [ ] T033 [P] [US3] Add `m985_every_transitive_component_has_an_inbound_edge`.
- [ ] T034 [P] [US3] Add `m985_closure_identities_rewritten_after_edges_are_still_connected` — the #980 regression, at closure scale.

### Implementation for User Story 3

- [ ] T035 [US3] Implement `ClosureEdge` (E5) in `closure.rs`, one per resolved relation, `from` the actual parent (E5.2).
- [ ] T036 [US3] Convert closure edges into `Relationship` values in `waybill-cli/src/cli/scan_cmd.rs`, appending to the existing relationship set rather than replacing it.
- [ ] T037 [US3] Apply `apply_renames` (added in #981) to closure edges as well as declared ones, so a version assignment that rewrites a PURL rewrites the endpoints in the same step (C-4.3, E5.3).
- [ ] T038 [US3] Verify against the per-PR integrity suite: `cargo +stable test -p waybill --test document_integrity` must stay green with the closure enabled.

**Checkpoint**: the graph is complete and connected at closure scale.

---

## Phase 6: User Story 4 — An operator can see what the closure did (P2)

**Goal**: the pass records its own outcome in the document.

**Independent test**: read declared / transitive / unresolved counts at document scope without inspecting components.

- [ ] T039 [P] [US4] Add `m985_a_closure_records_its_counts_at_document_scope` (FR-014).
- [ ] T040 [P] [US4] Add `m985_a_scan_without_the_closure_records_nothing` (FR-015, C-6).
- [ ] T041 [US4] Implement `ClosureSummary` (E4) in `closure.rs` with `BTreeMap` ordering (E4.4), and return it from the walk.
- [ ] T042 [US4] Thread the summary into `ScanArtifacts` and emit `waybill:nixpkgs-haskell-closure` at document scope in all three formats. **It must reach the document, not a `tracing::info!`** — milestone 973 exists because milestone 926 computed exactly this record and dropped it.
- [ ] T043 [US4] Add the catalog row and three parity extractors for `waybill:nixpkgs-haskell-closure`, same discipline as T029/T030.

**Checkpoint**: all four stories complete.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T044 Add the `--no-nixpkgs-haskell-closure` flag (C-1) in `waybill-cli/src/cli/scan_cmd.rs`, defaulting to absent so the closure runs (FR-016). It MUST NOT disable milestone 926's declared-dependency resolution.
- [ ] T045 Add `m985_the_opt_out_returns_pre_feature_output` — with the flag set, output is byte-identical to the T001 baseline (FR-017, C-7, SC-009). **This task MUST complete before T046**; once the goldens are regenerated the comparison no longer exists.
- [ ] T046 Regenerate the public-corpus goldens through CI per `docs/development/refreshing-corpus-goldens.md`. Read every diff and attribute each category before accepting. Expect the Haskell target to change substantially and every other target not to change at all — a non-Haskell target moving is a finding, not noise.
- [ ] T047 [P] Verify SC-002 against the oracle, not against waybill's own parse: run `measurements/nix_closure_oracle.sh` on a project with a populated cache and compare totals. Any disagreement must be explained — an unexplained off-by-one is how a wrong number enters a spec.
- [ ] T048 [P] Verify SC-008: time the same scan with and without the closure, on the same machine with the package set local; the ratio must be ≤ 1.5×. Record both numbers in the PR.
- [ ] T049 [P] Mutation-test every assertion added in T011–T016, T022–T024, T031–T034, T039–T040 and T045: revert the behaviour each guards and confirm the test fails. Record which mutation was used for each in the PR.
- [ ] T050 [P] Update `docs/reference/reading-a-waybill-sbom.md` with a section on reading the closure — what `waybill:nixpkgs-component-origin` means, how to filter to declared-only, and that document size grows 1.5–3.8×.
- [ ] T051 [P] Add a CHANGELOG entry recording the measured multipliers, the oracle agreement, and the opt-out flag.
- [ ] T052 Run the full gate: `./scripts/pre-pr.sh`. Both commands, enumerated per-target output, never an exit code alone.
- [ ] T053 Re-run the corpus target with `WAYBILL_RUN_PUBLIC_CORPUS=1` and confirm layer 0 (invariant I2), layer 1 tripwires and layer 2 goldens all pass at closure scale.

---

## Dependencies & Execution Order

### Phase Dependencies

```
Setup (T001–T004)
   └─► Foundational (T005–T010)   ← blocks everything
          ├─► US1 (T011–T021)     ← MVP
          │      ├─► US2 (T022–T030)   needs members to mark
          │      └─► US3 (T031–T038)   needs members to connect
          │             └─► US4 (T039–T043)  needs the walk's counts
          └─► Polish (T044–T053)
```

### User Story Dependencies

- **US1** depends only on Foundational. It is the MVP: a closure that resolves and emits, even unmarked and unconnected, already closes the false-negative gap Principle VIII objects to.
- **US2** and **US3** both depend on US1 and are independent of each other — one marks components, the other connects them. They can proceed in parallel.
- **US4** depends on US1 for its counts; it can start once the walk returns a result, in parallel with US2/US3's emission work.

### Critical ordering constraints

1. **T001 before everything** — the pre-feature baseline cannot be captured after the code changes.
2. **T045 before T046** — SC-009 compares against goldens that T046 replaces.
3. **T029/T030 and T043 are atomic pairs** — a catalog row without its extractors reddens `every_catalog_row_has_an_extractor`.
4. **T037 with T035/T036** — edges built without the rename application is the #980 defect.

### Parallel Opportunities

- Setup: T003, T004
- Foundational: T008, T009
- US1 tests: T011–T016 all together
- US2: T027, T028 after T026; tests T022–T024 together
- US3: tests T031–T034 together
- Polish: T047, T048, T049, T050, T051

### Parallel Example: User Story 1

```
T011 ┐
T012 ├─ all six tests written together, all failing
T013 ├─ (no implementation exists yet)
T014 │
T015 │
T016 ┘
  └─► T017 → T018 → T019 → T020 → T021  (sequential; same file, shared state)
```

---

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + US1.** A closure that resolves and emits members
with versions and hashes already delivers the feature's core value, even before
those members are marked or connected. It is demonstrable against a real project
and measurable against the oracle.

**Increment 2 = US2 + US3 in parallel.** Marking and connecting are independent
and together make the document trustworthy rather than merely fuller. US3 is the
riskier of the two; do not defer it, because an unconnected closure is worse than
no closure — it adds components a consumer cannot reason about.

**Increment 3 = US4 + Polish.** The summary and the opt-out. T045 must land
before T046 regardless of how the rest is sequenced.

**Total: 53 tasks** — 4 setup, 6 foundational, 11 (US1), 9 (US2), 8 (US3),
5 (US4), 10 polish.
