# Tasks: transitive propagation of non-runtime lifecycle scope

**Input**: Design documents from `/specs/1269-transitive-dev-scope/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: INCLUDED and non-optional. The spec requires them by name —
SC-003 demands a mutation-demonstrated positive control, SC-002 the
corpus invariant lane, FR-009 the parity suite. A pass that
over-propagates satisfies every count-based check while demoting
components that ship, so the tests are the feature, not its trim.

**Organization**: by user story. Note the deliberate inversion — **US2
(P1) ships before US1 (P1)**. Both are P1; the immunity guard must
exist before propagation is switched on, because switching it on first
means shipping the dangerous direction untested.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable — different file, no dependency on an incomplete task
- **[Story]**: US1 (dev transitives marked), US2 (runtime never demoted), US3 (declared vs inferred on the wire), US4 (per-ecosystem record)

## Path Conventions

Single Rust workspace. Production code in `waybill-common/src/` and
`waybill-cli/src/`; tests live beside the code they test
(`#[cfg(test)] mod tests`) except golden/corpus suites under
`waybill-cli/tests/`.

**One finding that removes work before it is written**: milestone 023's
generic per-component annotation bag already surfaces every
`extra_annotations` entry in all three formats —
`cyclonedx/builder.rs:1805`, `spdx/document.rs`, and
`spdx/v3_annotations.rs:428`. Writing the derivation key into
`extra_annotations` is therefore the whole of emission. **No emitter
task exists below**, and if one seems necessary, check the internal-key
filters (`is_internal_emission_key`, `is_field_owned_annotation_key`)
before adding plumbing.

---

## Phase 1: Setup

**Purpose**: the module exists and compiles before anything fills it.

- [X] T001 Create `waybill-cli/src/resolve/scope_propagation.rs` with the module doc comment stating the two-phase contract, the lattice, and that it is strictly additive, citing `specs/1269-transitive-dev-scope/contracts/propagation-algorithm.md`
- [X] T002 Register the module in `waybill-cli/src/resolve/mod.rs` beside `reconciler`

**Checkpoint**: `cargo build -p waybill --all-targets` compiles with an empty module.

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ CRITICAL**: no user story work begins until this phase is complete — every story reads the lattice and the traversal edge set.

- [X] T003 Add `LifecycleScope::strength(&self) -> u8` to `waybill-common/src/resolution.rs` as an **exhaustive match** (FR-004) (`Runtime 4 > Optional 3 > Build 2 > Test 1 > Development 0`), with the doc comment recording why `Optional` outranks `Development` (optional may ship; development does not — the weaker claim about production is the safer one to publish) and why this is not `#[derive(Ord)]` (declaration order is invisible and silently wrong when a variant is inserted mid-list)
- [X] T004 [P] Add a unit test in `waybill-common/src/resolution.rs` asserting `strength()` is injective over all five variants (FR-004) (five distinct values) — a tie would make "strongest wins" ambiguous in a way the pass cannot detect
- [X] T005 Change `apply_lifecycle_scope_to_edges` in `waybill-cli/src/scan_fs/mod.rs:1904` from private to `pub(crate)`, leaving its body and its four tests at `:2663`–`:2721` untouched (research R4: it is idempotent, so it is re-run rather than moved)
- [X] T006 Implement the traversal edge-set predicate in `waybill-cli/src/resolve/scope_propagation.rs`: a dependency relation whose `provenance.data_type` is neither `cross-ecosystem-bridge` nor `cross-ecosystem-bridge-ambiguous` (FR-002b, research R3 — note `provenance.source` is NOT the discriminator, it holds the manifest path)
- [X] T007 Implement the P2 precondition assertion in `waybill-cli/src/resolve/scope_propagation.rs`: the input carries no relationship whose `provenance.source` is `milestone-868-resolve-ownership`, `milestone-860-retained-main-module`, or `milestone-158-workspace-peer-linkage`. FR-002a holds by placement today (every anchoring site is inside a format builder); this is what makes a future change that moves anchoring upstream fail loudly instead of silently immunizing population B
- [X] T008 [P] Unit tests in `waybill-cli/src/resolve/scope_propagation.rs` for T006 and T007: both bridge `data_type` values are excluded from the edge set, a manifest-path `source` is not, and a planted anchoring-provenance edge trips the assertion

**Checkpoint**: the lattice and the edge set are testable; no behaviour has changed in any emitted document.

---

## Phase 3: User Story 2 — a real runtime dependency is never demoted (Priority: P1) 🎯 ships first

**Goal**: establish absolute immunity for anything that is runtime-reachable, and prove the guard fires, before any propagation exists to be guarded.

**Independent Test**: the mixed-reachability fixture keeps `lifecycle_scope: None` on a component reachable both ways, and fails when the immunity check is removed.

### Tests for User Story 2

- [X] T009 [US2] Add the mixed-reachability positive control to `waybill-cli/src/resolve/scope_propagation.rs` tests: `root → shared`, `root → devtool` (declared `Development`), `devtool → shared`; assert `shared` keeps `lifecycle_scope: None` and carries no C62 derivation marker (FR-002, SC-003)
- [X] T010 [P] [US2] Add a test asserting a reader-assigned scope is never changed — a declared `Optional` component reachable from a `Development` seed stays `Optional` (FR-003)
- [X] T011 [P] [US2] Add a test asserting no assignment happens in an ecosystem with no root seed (FR-012) — immunity cannot be established there, and the failure direction is demotion

### Implementation for User Story 2

- [X] T012 [US2] Implement Phase A in `waybill-cli/src/resolve/scope_propagation.rs`: BFS from the root seed set over T006's edge set, never traversing *into* a component whose `lifecycle_scope` is `Some(s)` where `s != Runtime`; returns the immunity set
- [X] T013 [US2] Thread the per-ecosystem root seed set into the pass from the same source the emitters' completeness reporting uses (P3 in the algorithm contract), so immunity is computed from the root the document declares

### Mutation evidence for User Story 2

- [X] T014 [US2] Delete the immunity check from the pass, re-run T009, and record the result in `specs/1269-transitive-dev-scope/measurements/mutation-evidence.md` — including the restored-green run. If T009 passes with the check deleted it is not exercising the guard and SC-003 is unmet; the test is the deliverable, not the assertion count

**Checkpoint**: immunity works and is proven to be load-bearing. Nothing is marked yet, so every document is still byte-identical.

---

## Phase 4: User Story 1 — dev transitives stop reading as runtime (Priority: P1)

**Goal**: a component reachable only through non-runtime components carries a non-runtime scope, in all three formats.

**Independent Test**: quickstart check 1 reads `0` for `python-flask` where it read `51`, with `npm-express` unchanged at `0`.

### Tests for User Story 1

- [X] T015 [P] [US1] Add a cycle test to `waybill-cli/src/resolve/scope_propagation.rs`: a dependency cycle among non-runtime components terminates and every member is assigned (FR-011)
- [X] T016 [P] [US1] Add a population-B test: a component unreachable from the root but reachable from a declared non-runtime component IS marked (FR-005) — the case `pydantic`'s 103 live in, which root-seeded reachability alone would miss
- [X] T017 [P] [US1] Add a strongest-wins test: a component reached from both an `Optional` and a `Development` seed is assigned `Optional`, in both seed orders, proving the result is order-independent (FR-004, Q6). Assert the assigned value is an existing `LifecycleScope` variant equal to the seed's, not a new "transitively optional" one (FR-006) — the variant is what every consumer-side filter and every format's native scope mapping already keys on
- [X] T018 [P] [US1] Add an unscoped-transparency test: a component fed by one declared dev member and one unscoped orphan IS marked (FR-001 as clarified — only positive runtime reachability blocks marking)

### Implementation for User Story 1

- [X] T019 [US1] Implement Phase B in `waybill-cli/src/resolve/scope_propagation.rs`: for each non-runtime scope strongest-first (`Optional, Build, Test, Development`), BFS from that scope's declared members over T006's edge set, recording an assignment for each reached component that is not immune and carries no scope. The assignment is the seed's own variant — no new variant is introduced (FR-006)
- [X] T020 [US1] Apply the assignments: set `lifecycle_scope` and write `waybill:lifecycle-scope-derivation = "transitive"` into `extra_annotations`, after all BFS passes complete so no component is read mid-mutation (FR-007, data-model "Apply")
- [X] T021 [US1] Return the `PropagationSummary` (`immune`, `assigned_by_scope`, `seeded_from`) and emit one INFO line when `assigned_by_scope` is non-empty, silent otherwise (research R9)
- [X] T022 [US1] Call the pass from `waybill-cli/src/cli/scan_cmd.rs` immediately after the second-pass m191 reconciler (`:4022`) and **before** the `--exclude-scope` filter (`:4034`), with a comment recording all three placement constraints from research R1
- [X] T023 [US1] Re-run `apply_lifecycle_scope_to_edges` at the same site, immediately after the pass, so edges into newly-marked components carry the typed relationship in every format (FR-010)

### Verification for User Story 1

- [X] T024 [US1] Run quickstart check 1 against freshly emitted documents for `python-flask`, `rust-ripgrep`, `haskell-language-server` and `npm-express`; expect `0 / 0 / 0 / 0` where the committed goldens read `51 / 2 / 26 / 0`, and record the before/after in `specs/1269-transitive-dev-scope/measurements/`

**Checkpoint**: the defect is fixed in-memory and measurable; goldens have not yet moved.

---

## Phase 5: User Story 3 — declared vs inferred is on the wire (Priority: P2)

**Goal**: a consumer can tell a scope waybill inferred from one a reader read.

**Independent Test**: a document with both populations shows the marker on exactly the propagated components, identically in all three formats.

- [X] T025 [US3] **Amended at implementation time**: the key `waybill:lifecycle-scope-derivation` is already catalog row **C62**, so there is no new row. Extend C62's open enum in `docs/reference/sbom-format-mapping.md` with `transitive`, widen its prose from the Go test-scope discriminator to the derivation discriminator for any non-runtime scope in any ecosystem, and record the re-run KEEP-NO-NATIVE verdict from `contracts/scope-derivation-annotation.md` (FR-009, FR-017)
- [X] T026 [US3] ~~Add `c199_cdx`~~ **void** — `c62_cdx` already extracts this key
- [X] T027 [P] [US3] ~~Add `c199_spdx23`~~ **void** — `c62_spdx23` already extracts this key
- [X] T028 [P] [US3] ~~Add `c199_spdx3`~~ **void** — `c62_spdx3` already extracts this key
- [X] T029 [US3] **Void — nothing to register.** C62 is already registered `SymmetricEqual, order_sensitive: false` with `c62_cdx`/`c62_spdx23`/`c62_spdx3`. T026–T028's extractors were written, then reverted when `extractors_have_unique_labels` caught the duplicate label. Replaced by T029a
- [X] T029a [US3] Assert C62's one-value-per-component contract, which the shared enum makes necessary: `a_component_with_an_existing_derivation_is_not_touched` in `waybill-cli/src/resolve/scope_propagation.rs`, plus a `debug_assert!` in the apply loop. Both Go writers set the scope and the derivation together and this pass assigns only where the scope is `None`, so a collision is impossible — but that argument lives in another module and would not travel with a change to it
- [X] T030 [US3] Add a test asserting the marker is absent on a declared non-runtime component and absent on a runtime component, so "absent means declared" is enforced rather than assumed (FR-008, SC-006)

**Checkpoint**: `cargo test -p waybill --bin waybill parity::` and `holistic_parity` green; the distinction is readable in all three formats.

---

## Phase 6: User Story 4 — the reach of the fix is recorded per ecosystem (Priority: P2)

**Goal**: "does this work for gem?" is answered by a record, not an inference from one target.

**Independent Test**: every ecosystem row cites a corpus target or a named fixture; no row reads "assumed".

- [X] T031 [US4] Add a gem fixture test (no corpus target exists — the 17 are listed in spec US4 scenario 2) covering `Gemfile` group members and their transitives, and state in the test's doc comment that gem evidence is synthetic
- [X] T032 [P] [US4] Add equivalent fixture coverage for any ecosystem in FR-015's list with no corpus target and no existing scope test: composer `require-dev`, mix, rebar — landed with T031 in one file, `waybill-cli/tests/scope_propagation_ecosystems.rs`, plus a **uv** fixture that was not in the task list and is the only one where the C62 derivation marker is present: it reproduces #1209's `pydantic` shape small enough to assert on, which the real pin cannot do because it is not a corpus target
- [X] T033 [US4] Write `specs/1269-transitive-dev-scope/measurements/per-ecosystem-record.md`: one row per ecosystem with outcome *fixed* / *already correct* / *not reached*, each citing a corpus target or a named fixture, and a stated reason for every *not reached* (FR-015, SC-005)

**Checkpoint**: the record exists and is citable; US4 is satisfiable independently of whether phase 7 has run.

---

## Phase 7: Golden refresh, declarations, and the gate

**Purpose**: the acceptance evidence the corpus lane owns. Nothing here runs on a local machine except the first task.

- [X] T034 Regenerate the **local** (non-corpus) goldens whose content embeds component scope, one env var per format — `WAYBILL_UPDATE_CDX_GOLDENS`, `WAYBILL_UPDATE_SPDX_GOLDENS`, `WAYBILL_UPDATE_SPDX3_GOLDENS` — then diff with content-addressed IDs masked and `LC_ALL=C sort` applied, and confirm every moved line is a scope change or a C62 derivation marker. Check `optional_dep_classification.rs` first: it asserts scope classification directly and is the most likely to carry a real regression rather than churn
- [X] T035 In `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`, delete the **four** `kind = "unmarked-non-runtime-reachable"` entries this feature fixes (`python-flask`, `haskell-language-server`, `haskell-security-advisories`, `nix-closure-moat`) and **retain** `rust-ripgrep`'s with its `reason` rewritten to the measured cause (the cargo reader assigns `Runtime` to transitives of a non-activated optional dependency, so FR-003 forbids this pass from marking them) and `issue = 1272`. Same change as the fix — phases 3-6's code, the deletions and the re-point ship in one PR, never as a follow-up, because the lane fails on a stale declaration by design and a declaration naming the wrong defect is what that rule exists to prevent (FR-014)
- [ ] T036 Run the full pre-PR gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with **no `+toolchain` argument** (#1189), and report `GATE_EXIT`, the per-target `test result: ok. N passed; 0 failed` tally, and the positive-signal line
- [ ] T037 Dispatch the corpus lane on the branch: `gh workflow run public-corpus.yml -f branch=1269-transitive-dev-scope` (`-f branch=`, never `--ref`, per #1223). **Never** set `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` locally
- [ ] T038 From the lane's output, confirm: the four deleted I3 cells pass with no declaration re-added and `rust-ripgrep`'s retained cell still reports `unmarked-non-runtime-reachable` against its declaration (SC-002), `npm-express`'s three documents are byte-identical (SC-004, FR-013), and every other changed corpus golden is explained by a propagated scope or a C62 derivation marker (SC-007). I3 reads CycloneDX only; SC-001's three-format clause rests on C42 `waybill:lifecycle-scope` being a `SymmetricEqual` parity row, so record that `holistic_parity` passed alongside the lane rather than treating the CDX result as covering all three (SC-001)
- [ ] T039 Re-measure the issue's own target on the current build and record it: `pydantic`'s unscoped-transitive count must reach 0 (SC-008). The 103 in #1209 was observed with waybill 0.10.1 against a pin that is not a corpus target, so it is provenance, not a target
- [X] T040 Add the `## [Unreleased]` CHANGELOG entry, leading with the consumer-visible change and naming the **second** one explicitly: `--exclude-scope development` now drops the transitive closure it already promised to drop, so an operator using that flag gets strictly less output than before
- [X] T041 Confirm FR-018 is honoured: nothing in the PR, changelog or measurements claims a reduction in `orphaned-components-detected`. Scope marking does not change reachability, and that count is derived from reachability alone
- [X] T042 Verify FR-016 by set-diffing `Cargo.lock`'s package name/version pairs against `origin/main` and asserting "packages added: none; packages removed: none". The pre-PR gate does not inspect the lockfile, so a dependency could land green; this is the check used for the `strum` dev-dependency in #1270. Runs any time after phase 2 — placed last only so no task ID renumbers (FR-016)

---

## Dependencies & Story Completion Order

```
Phase 1 (setup)
  └─▶ Phase 2 (lattice + edge set + anchoring assertion)   ← blocks everything
        └─▶ Phase 3 / US2 (immunity + mutation evidence)    ← MUST precede US1
              └─▶ Phase 4 / US1 (propagation + call site)
                    ├─▶ Phase 5 / US3 (C62 enum entry)
                    ├─▶ Phase 6 / US4 (per-ecosystem record)
                    └─▶ Phase 7 (goldens, declarations, gate)
```

- **US2 before US1** is not a preference. Switching on propagation without the immunity guard ships the under-reporting failure, which passes every count-based check US1 is measured by.
- **US3 and US4 are independent of each other** and can proceed in parallel once US1 lands.
- **T035 (declaration deletion) must be in the same commit as the fix**, not a follow-up.

## Parallel Execution Opportunities

- Phase 2: T004 (`waybill-common`) ∥ T008 (`scope_propagation.rs`) — different files
- Phase 3: T010 ∥ T011 — both additive tests, no shared state
- Phase 4: T015 ∥ T016 ∥ T017 ∥ T018 — four independent test cases, all added before T019–T023 make them pass
- Phase 5: T027 ∥ T028 — different extractor files; T026 and T029 are not parallel with them (T029 depends on all three)
- Phase 6: T031 ∥ T032 — different fixtures

## Implementation Strategy

**MVP is Phases 1–4** — US2 and US1 together. Not US1 alone: a
propagation pass without its immunity guard is net-harmful, so the
smallest shippable increment is both P1 stories.

**Incremental delivery**: Phases 1–4 fix the defect and are
independently verifiable in-memory with no golden movement at all.
Phase 5 adds the wire-level transparency. Phase 6 makes the reach
auditable. Phase 7 is where goldens move and where the acceptance
evidence is produced — and it is the only phase that needs CI.

**Where this is most likely to go wrong**: T034's golden diff. "Churn
to regenerate" is how a regression gets committed, which is why SC-007
requires every moved line to be explained and why
`optional_dep_classification.rs` is called out by name — it asserts
scope classification directly, so a change there is a finding until
proven otherwise.
