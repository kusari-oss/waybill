# Tasks: components for the closure members waybill does not name

**Feature**: `1282-closure-nameless-components` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Branch**: `1282-closure-nameless-components`

Test tasks are generated because the spec asks for them by name: every user
story carries an **Independent Test** clause, and SC-008 requires that
disabling the emission makes a test fail.

**Stack**: Rust stable pinned 1.98.1, zero new Cargo dependencies, no
subprocess, no network. Every input is a closure already parsed into memory.

---

## Phase 1: Setup

- [ ] T001 Create `waybill-cli/src/scan_fs/package_db/nix/closure/member.rs` with a module doc comment stating what distinguishes it from `classify.rs` — that one assigns the C182 role to **derivations** and covers all 1,749, this one assigns a role to the things that become **components** (research R4, the confusion that falsified a clarification)
- [ ] T002 [P] Register the module in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs`
- [ ] T003 [P] Add the fixture closure at `waybill-cli/tests/fixtures/nix_closure_members/closure.json` covering all four roles plus the two cases that drive identity: two members sharing a name with **different** content, and two sharing a name with **identical** content. Synthetic names only (`waybill-fixture-*`)
- [ ] T004 [P] Record the pre-change baselines in `specs/1282-closure-nameless-components/measurements/baseline.txt` by reading the committed golden — 381 components, 3,459 edges, census totalling 1,749. Read them, do not quote this document: figures carried from memory were wrong three times

**Checkpoint**: module compiles and is wired; fixtures exist.

---

## Phase 2: Foundational (blocking)

The classifier and the identity basis block every story.

- [ ] T005 Define `MemberRole` (Source / Package / Scaffolding / Fragment) with `wire()` and `is_edge_endpoint()` in `member.rs` per `contracts/member-classification.md`
- [ ] T006 Define `IdentityBasis` (Content / Recipe) in `member.rs`, as a two-variant enum rather than `Option<ContentHash>` so the merging behaviour is a property of the type (data-model.md)
- [ ] T007 Implement `classify(drv) -> MemberRole` in `member.rs` as a **total** function (FR-005) reading only the closure — no network, no second evaluation, no maintained name list (FR-006). Evidence order fixed: content hash, then `env.text`, then version-shaped name, then fragment
- [ ] T008 Implement the content-hash read in `member.rs` against **both** `outputs[<name>].hash` and `env.outputHash` — the structured form carries 549 and the older spelling 139; reading only the latter undercounts the population fourfold
- [ ] T009 Implement `identity(drv, drv_key) -> IdentityBasis` in `member.rs` (FR-003, FR-004), taking the recipe hash from the closure map key's leading hash so a member with no content hash is still uniquely identified
- [ ] T010 **Choose the hash-prefix length by measurement**, not convention: extend `measurements/classify-nameless.py` to report collisions at candidate prefix lengths, run it, and pick the shortest with zero collisions plus a margin. Record the chosen length and its evidence in `measurements/README.md`. The plan deliberately left this open because full-hash collision-freedom is a different claim from prefix collision-freedom
- [ ] T011 [P] Unit-test in `member.rs` that `classify` is total and reproduces the measured distribution on the fixture, asserting the **four counts** and not merely the total — a classifier returning one role for everything still sums correctly
- [ ] T012 [P] Unit-test in `member.rs` that a derivation carrying both a content hash and `env.text` classifies as `Source`, fixing the evidence order against regression
- [ ] T013 [P] Unit-test in `member.rs` that `identity` returns `Content` where a hash exists under either spelling, and `Recipe` otherwise

**Checkpoint**: members can be classified and identified. Nothing is emitted yet.

---

## Phase 3: User Story 1 — a consumer can see the sources a build consumed (P1) 🎯 MVP

**Goal**: the 549 fixed-output derivations become components carrying their content hash.

**Independent test**: scan a fixture closure containing fixed-output derivations and assert each becomes a component whose identity includes its content hash.

- [ ] T014 [US1] Replace the `pname()`-is-`None` skip in `emit::components` (`closure/emit.rs`) with the classifier, so every member is emitted (FR-001)
- [ ] T015 [US1] Build the identifier in `closure/emit.rs` as `pkg:generic/<name>-<hash-prefix>`, with the hash in the **name segment** — a qualifier does not satisfy FR-002 (contracts)
- [ ] T016 [US1] Merge members sharing a name and content hash into one component in `closure/emit.rs` (FR-003); keep members differing in content distinct
- [ ] T017 [US1] Keep the derivation's own `name` unmodified on the component's `name` field (FR-016)
- [ ] T018 [US1] Carry the source URL where the derivation records one (FR-008); measured 138 of 1,145
- [ ] T019 [P] [US1] Integration test in `waybill-cli/tests/closure_members.rs`: a fixed-output member emits a component whose identifier carries its content hash (US1 scenario 1)
- [ ] T020 [P] [US1] Integration test: two derivations fetching **identical** content emit **one** component (US1 scenario 2, FR-003)
- [ ] T021 [P] [US1] Integration test: two derivations sharing a `name` with **different** content emit **two** components whose identifiers differ **without reference to any qualifier** (US1 scenario 3, FR-002, SC-014)
- [ ] T022 [P] [US1] Integration test: the component's `name` field is the derivation's name, not the hashed identifier (FR-016)

- [ ] T048 [P] [US1] Integration test in `closure_members.rs`: the fixed-output population merges to the measured component count — 549 members yielding 502 components on the pinned proportions — and a member with no content hash is still uniquely identified (SC-010, FR-004)
- [ ] T049 [P] [US1] Integration test: two scans of one closure emit identical component sets, asserted on the sorted identifier list (SC-004)

**Checkpoint**: US1 is independently shippable — sources are visible.

---

## Phase 4: User Story 2 — a consumer can tell a source from a build script (P1)

**Goal**: every added component states which of four kinds it is, so the scope chosen in Clarifications is defensible.

**Independent test**: scan a fixture containing all four classes; assert every emitted component carries a role and that the roles partition the set.

- [ ] T023 [US2] Emit `waybill:closure-member-role` on every added component in `closure/emit.rs`, exactly one value per component (FR-005)
- [ ] T024 [US2] Add the catalog row for `waybill:closure-member-role` to `docs/reference/sbom-format-mapping.md` **and** its entry in `waybill-cli/src/parity/extractors/mod.rs` EXTRACTORS **in the same commit** — a doc row without an extractor fails `every_catalog_row_has_an_extractor` and `holistic_parity`
- [ ] T025 [US2] Write the parity extractor for the new row in `waybill-cli/src/parity/extractors/`, reading the emitted value rather than re-deriving it
- [ ] T026 [P] [US2] Integration test in `closure_members.rs`: a `writeText` member is marked scaffolding, not presented as a dependency (US2 scenario 1)
- [ ] T027 [P] [US2] Integration test: every added component carries exactly one role, and the four counts sum to the number added (US2 scenario 2, SC-002)
- [ ] T028 [P] [US2] Integration test: filtering to `source` and `package` yields 585 components on the fixture's proportions and leaves no scaffolding or fragments (US2 scenario 3, FR-007, SC-005)
- [ ] T029 [P] [US2] Integration test: the role is emitted natively where a format has a carrier and as an annotation otherwise, consistently across CycloneDX, SPDX 2.3 and SPDX 3

- [ ] T050 [P] [US2] Unit-test in `member.rs` that classification touches no network, no subprocess and no name list — assert it against a derivation record alone, with the test holding no fixture beyond that record (FR-006)

**Checkpoint**: the scope decision is now defensible — a consumer can filter.

---

## Phase 5: User Story 3 — the document stays usable at triple the component count (P2)

**Goal**: every document-scope count either excludes the new population or states that it includes it, and the reshaped graph is reported rather than discovered.

**Independent test**: scan a fixture before and after; assert each count is accounted for.

- [ ] T030 [US3] Widen `graph::stops` in `closure/graph.rs` to `pname`-bearing OR `MemberRole::is_edge_endpoint`, and pass the matching set as `project`'s `emitted` argument — derived from the one rule, never a second copy (research R1, R2)
- [ ] T031 [US3] Assert in `closure/graph.rs` that the edge-endpoint set is a **subset** of the emitted set, so the deliberate divergence introduced by this feature cannot silently become a disagreement
- [ ] T032 [US3] Emit the four-role count record at document scope in `closure/summary.rs` (FR-009), additive beside the milestone-1277 census
- [ ] T033 [US3] Carry the post-change edge figure in the document (FR-018), so a graph that changed size is distinguishable from one that broke
- [ ] T034 [US3] Add one orphan-reason variant in `waybill-cli/src/generate/orphan_reason.rs` for the FR-014 population, and explain it via the new role annotation rather than widening the `unreferenced` arm — widening it would re-create the defect milestone 1280 fixed (research R5)
- [ ] T035 [P] [US3] Integration test: the milestone-1277 census is numerically identical before and after on the same closure (SC-013, FR-012)
- [ ] T036 [P] [US3] Integration test: every added component unreachable by an edge carries a reason, and the count equals scaffolding + fragment (SC-012, FR-014)
- [ ] T037 [P] [US3] Integration test: no dependency edge names a component absent from the document (SC-009, SC-011, FR-013)
- [ ] T038 [P] [US3] Integration test: a scan without `--nix-closure` is byte-identical to today (SC-006, FR-010)
- [ ] T039 [US3] Re-measure the component-level edge count after T030 and record it in `measurements/README.md` beside the 3,459 baseline, with the direction of the change stated. The −35% figure in research R3 is a derivation-level proxy and MUST NOT be reported as the component-level result

- [ ] T051 [US3] Make graph-completeness and orphan accounting state which population they describe (FR-015), and emit a member whose name is absent or unusable rather than dropping it, identified by its derivation hash alone (FR-017). Measured: zero such derivations occur, so FR-017 is defensive — the test constructs one rather than waiting to meet it

**Checkpoint**: all three stories complete and independently testable.

---

## Phase 6: Polish & cross-cutting

- [ ] T040 **SC-008 mutation check**: disable the new emission, confirm tests fail, re-enable, and name the failing tests in the PR body. A population no test is sensitive to is not evidence that it is emitted
- [ ] T041 [P] Verify SC-001: the component count rises to approximately 1,479 on the measured closure, and SC-003: no two added components share an identity under a qualifier-ignoring comparison
- [ ] T042 [P] Verify SC-007: no component another reader emits is displaced or renamed (FR-011)
- [ ] T043 Check the invariant harness: 513 new orphans and ~1,098 new components will move `orphan_explained` and the graph-completeness checks. Update `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml` only where the new state is correct, and **never** touch `explained_pre_1156`, which is a frozen drift detector
- [ ] T044 Run the pre-PR gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189), and assert on the positive signal rather than the absence of failure
- [ ] T045 Refresh corpus goldens via `public-corpus.yml` workflow dispatch, never locally
- [ ] T046 Review the refreshed goldens with a normalised, sorted diff — mask content-addressed ids including `rel-`/`anno-` and `LC_ALL=C sort` before diffing, or SPDX-3 array reordering reads as a semantic hit
- [ ] T047 Update `docs/design-notes.md` with the four-role vocabulary and why it is distinct from the C182 derivation roles, since the two were confused once already and will be again

---

## Dependencies

```
Phase 1 (T001–T004)
      ↓
Phase 2 (T005–T013)   ← blocks everything; T010 blocks T015
      ↓
Phase 3 US1 (T014–T022, T048–T049)   ← MVP
      ↓
Phase 4 US2 (T023–T029, T050)   depends on US1 emitting
      ↓
Phase 5 US3 (T030–T039, T051)   depends on roles existing (T023)
      ↓
Phase 6 (T040–T047)
```

T024 and T025 must land in one commit. T045 precedes T046. T039 depends on
T030 having landed. T048–T051 carry trailing IDs rather than being inserted:
renumbering would silently break the range references above.

## Parallel opportunities

- **Phase 1**: T002, T003, T004 together.
- **Phase 2**: T011, T012, T013 together once T005–T009 land.
- **Phase 3**: T019–T022, T048 and T049 together once T014–T018 land.
- **Phase 4**: T026–T029 and T050 together once T023–T025 land.
- **Phase 5**: T035–T038 together once T030–T034 land.
- **Phase 6**: T041, T042 together.

## Implementation strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (US1)** — the 549 sources become visible
with verifiable identity. That alone answers "what source did this come from",
which is the clearest loss today.

US2 is also P1 and should not be deferred far: it is the condition on which the
scope was accepted. Shipping US1 without it produces a document where
`strncpy` and a tarball are indistinguishable — and US1 alone does not emit
`strncpy`, so the risk only materialises when US2's population arrives.

Three cautions carried from the plan:

1. **The edge count will change, and may fall.** Stopping at sources replaces
   transitive fan-outs with direct edges. A fall is expected; a *rise* is the
   surprise worth investigating.
2. **513 orphans are by construction**, not a defect. An orphan without a
   reason is the defect.
3. **Do not quote figures from memory.** Three baselines carried through
   earlier sessions were wrong, and the golden is the source of truth.
