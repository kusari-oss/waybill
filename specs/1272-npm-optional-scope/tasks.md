---
description: "Task list for milestone 1272 — npm-ecosystem optional-axis classification"
---

# Tasks: npm-ecosystem optional-axis classification

**Input**: Design documents from `/specs/1272-npm-optional-scope/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/reader-optional-axis.md](./contracts/reader-optional-axis.md)

**Tests**: Test tasks are included because the spec requires them as
deliverables, not as a style choice — FR-013 (manager-produced
fixtures), SC-005 (every arm proven by one), SC-006 (deleting any arm's
logic fails a test). Tests are written before the change they cover.

**Organization**: One phase per user story, in spec priority order. Each
arm is independently implementable, independently testable and
independently revertible.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel — different file, no dependency on an incomplete task
- **[Story]**: US1 (pnpm v9) · US2 (bun) · US3 (yarn Berry) · US4 (yarn v1)

## Path Conventions

Single Rust workspace. Readers at
`waybill-cli/src/scan_fs/package_db/npm/`, integration tests at
`waybill-cli/tests/`, probes at
`specs/1272-npm-optional-scope/measurements/`.

## Two facts that shrink this list

**No new annotation value, no new parity extractor.** All four readers
already emit `waybill:optional-derivation` with
`npm-optional-dependencies` (`pnpm_lock.rs:475`, `yarn_lock.rs:521`) or
`bun-optional-dependencies` / `bun-optional-peers`
(`bun_lock.rs:856`), and C122's extractor already exists as
`SymmetricEqual` (`parity/extractors/mod.rs:550`). FR-008 is satisfied
by the code already in place, so no task adds a catalog row and the
`every_catalog_row_has_an_extractor` gate is not at risk.

**pnpm's classifier is already correct.** `pnpm_lock.rs:464-490` has
the complete three-way classifier with the Optional branch and the peer
guard. US1 changes where one boolean comes from; it does not write a
classifier.

---

## Phase 1: Setup

**Purpose**: make FR-010 / SC-004 ("the development axis is
byte-identical") checkable rather than assertable. Capture the baseline
*before* touching a reader, because once an arm lands there is nothing
left to compare against.

- [X] T001 Record the pre-change baseline: run `cargo test --workspace --no-fail-fast 2>&1 | tee` into the session scratchpad and save the per-target `test result: ok. N passed; 0 failed` lines, so a later regression is attributable to a specific arm rather than to "something in npm"
- [X] T002 [P] Capture the current development-axis and optional-axis output for every existing npm-ecosystem fixture by running the five dialect test binaries (`optional_dep_classification`, `optional_dep_pnpm_e2e`, `optional_dep_yarn_berry_e2e`, `optional_dep_yarn_v1_e2e`, `scan_bun_lock`) and storing their output in the scratchpad as the SC-004 comparison basis
- [X] T003 [P] Re-run both probes (`specs/1272-npm-optional-scope/measurements/optional-axis-probe.sh` and `cancellation-probe.sh`) and confirm every row of `measurements/findings.md` still reproduces, including that the cancellation probe's C1 and C2 cases return *different* answers — this is SC-007's first execution and the guard against designing against a manager that has since moved

**Checkpoint**: the baseline exists on disk. No reader has changed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: FR-011a / SC-008 — all four arms must warn on an
unrecognized lockfile format version. pnpm already parses its marker
(`pnpm_lock.rs:305`), so this is three readings plus four diagnostics.
Foundational because every arm's "I read what I know and said so" is
the honesty guarantee the whole feature exists to restore, and because
an arm shipped without it reintroduces the silent-absence defect for
the next format move.

Known-version table is research R6: pnpm `9.0`, bun `2`, Berry
`__metadata.version: 8`, yarn v1 header comment.

- [X] T004 Add the known-format-version constants and a shared `warn_unrecognized_lockfile_version(dialect, found, source_path)` helper emitting a `tracing::warn!` that names the dialect, the found version and that the optional axis may be incomplete, in `waybill-cli/src/scan_fs/package_db/npm/mod.rs`
- [X] T005 [P] Call the FR-011a helper from the pnpm reader using the already-parsed `lock_version` when its major exceeds the known maximum, in `waybill-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`
- [X] T006 [P] Read bun's `lockfileVersion` from the lockfile root and call the FR-011a helper when it exceeds the known value, in `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs`
- [X] T007 Read Berry's `__metadata.version` and the yarn v1 `# yarn lockfile v1` header marker, and call the FR-011a helper from each path when unrecognized, in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`
- [X] T008 Assert for each of the four dialects that a synthetic lockfile bearing an implausibly high format version still parses, still reads the locations the reader knows, emits the diagnostic naming that version, and does NOT fail the scan (SC-008, contract C7) in `waybill-cli/tests/optional_dep_classification.rs`

**Checkpoint**: an unrecognized future format version is visible in
every dialect. No classification behaviour has changed yet.

---

## Phase 3: User Story 1 — pnpm v9 (Priority: P1) 🎯 MVP

**Goal**: a pnpm v9 project's optional dependency is classified
optional, with the transitive case included for free.

**Independent Test**: scan a pnpm v9 fixture whose manifest declares
one dependency under `optionalDependencies` and one under
`dependencies` — the optional one reports the optional scope, the
runtime one does not, and exactly one component is optional. Delivers
the axis for one of four dialects with no change to any other reader.

**Why this is the MVP**: one source expression. The classifier, the
annotation and the peer guard all already exist; `is_optional` at
`pnpm_lock.rs:384` reads the `packages:` entry and measurement shows v9
moved the flag to `snapshots:`. Shippable alone, and it is the only arm
that needs no closure.

### Tests for US1 (write first — they must fail for the right reason)

- [X] T009 [US1] Generate a manager-produced pnpm v9 fixture — one dependency under `optionalDependencies`, one under `dependencies`, synthetic names per FR-015 — and commit it under `waybill-cli/tests/fixtures/` with a header recording the pnpm version that produced it
- [X] T010 [US1] Add a failing test asserting the v9 fixture's optional dependency carries `LifecycleScope::Optional` and `waybill:optional-derivation`, and the runtime one carries neither (contract C1) in `waybill-cli/tests/optional_dep_pnpm_e2e.rs`
- [X] T011 [P] [US1] Add a failing test for the transitive case — a runtime parent declaring an optional child classifies the child optional and the parent not (contract C2, acceptance scenario 3) in `waybill-cli/tests/optional_dep_pnpm_e2e.rs`
- [X] T012 [P] [US1] Add a failing test for the contested case measured in Probe C — a package reachable by both an optional and a non-optional path is NOT optional even when the root declared it optional (contract C3, FR-006) in `waybill-cli/tests/optional_dep_pnpm_e2e.rs`
- [X] T013 [P] [US1] Confirm the three new tests fail because the flag is read from the wrong section, not because the fixture is malformed — print the fixture's `snapshots:` entry and verify it carries `optional: true` where the `packages:` entry does not

### Implementation for US1

- [X] T014 [US1] Extend `build_snapshots_lookup` to return per-snapshot-identity `optional` and `peer` booleans alongside the existing dependency names, reading them from the snapshot entry mapping, in `waybill-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`
- [X] T015 [US1] Source `is_optional` and `is_peer` from the snapshot map for v9-or-later lockfiles while leaving the v6/v7 path reading `tbl` unchanged, gated on the existing `is_v9_or_later`, in `waybill-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`
- [X] T016 [US1] Replace the stale comment at `pnpm_lock.rs:380-386` that describes `is_optional` as "currently unused as a filter" with what the field now does and which section it comes from per dialect, in `waybill-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`

### Regression and evidence for US1

- [X] T017 [US1] Assert a pnpm v6 and a v7 lockfile classify exactly as before, since the flag genuinely lives in `packages:` for those dialects (FR-002, contract C6) in `waybill-cli/tests/optional_dep_pnpm_e2e.rs`
- [X] T018 [US1] Assert the existing peer-optional precedence still wins for pnpm — the existing `pnpm_peer_optional_stays_peer_not_optional` test must still pass unchanged (FR-009, contract C4)
- [X] T019 [US1] Verify the development axis is byte-identical against T002's baseline for every pnpm fixture (FR-010, SC-004, contract C5) — **reader-level evidence recorded**: 37 dev-axis tests pass, among them `pnpm_dev_true_wins_over_optional`, `pnpm_v9_importer_generated_fixture_seeds_direct_dev_only` and `pnpm_v9_optional_workspace_closure_cannot_be_demoted_to_development`; target count 350 and failures 0 both unchanged, and all +12 tests are ones this milestone added. Corpus-golden byte-identity is NOT covered locally (all `#[ignore]`d, PR lane skips them) and remains the nightly's gate — see `feedback_green_local_gate_excludes_corpus_goldens`
- [X] T020 [US1] SC-006 load-bearing check: delete the T015 source change, confirm a named US1 test fails, restore it, and record which test in the task notes — a test that passes without the code it covers is not evidence

**Checkpoint**: pnpm v9 answers correctly, root and transitive. Two of
five dialects now agree. Independently shippable.

---

## Phase 4: User Story 2 — bun (Priority: P2)

**Goal**: a bun project's optional dependency is classified optional.

**Independent Test**: scan a bun fixture whose root manifest declares
one dependency optional — that component reports the optional scope and
the runtime sibling does not.

**Two defects, not one** (research R4): the root declaration lives in
`workspaces.""`, and the m667 tracker keys through
`parent_key_to_out_idx`, which by construction excludes workspace
shapes — so the declaration is structurally invisible. Separately, the
finalize pass's `state.any_hard` cancels at depth 1, which also
suppresses a package whose only parent is optional and depends on it
normally — exactly Probe C's C2 case, which both reference managers
mark optional.

### Tests for US2 (write first)

- [X] T021 [P] [US2] Generate a manager-produced bun fixture with a root `optionalDependencies` declaration and a runtime sibling, synthetic names, and commit it under `waybill-cli/tests/fixtures/` with the bun version in its header
- [X] T022 [US2] Create `waybill-cli/tests/optional_dep_bun_e2e.rs` with a failing test asserting the root optional declaration carries `LifecycleScope::Optional` **and** `waybill:optional-derivation = "bun-optional-dependencies"`, and the runtime sibling carries neither (contract C1, FR-008, acceptance scenario 1)
- [X] T023 [P] [US2] Add a failing test for the transitive case — a package reachable only through an optional edge is optional although nothing declares it so (contract C2, FR-007a) in `waybill-cli/tests/optional_dep_bun_e2e.rs`
- [X] T024 [P] [US2] Add a failing test for the cancellation case — a package declared optional by one workspace and runtime by another is not optional (contract C3, acceptance scenario 2) in `waybill-cli/tests/optional_dep_bun_e2e.rs`

### Implementation for US2

- [X] T025 [US2] Collect the optional seed from every `workspaces.<path>.optionalDependencies` mapping, independently of `parent_key_to_out_idx` so workspace shapes are no longer excluded, in `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs`
- [X] T026 [US2] Collect the runtime seed from `workspaces.<path>.dependencies` **split from** the optional seed — see the research R7 trap: merging them as the development-axis helper does would make FR-006 subtract a set from itself — in `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs`
- [X] T027 [US2] Close the runtime seed transitively over the parent-metadata edge map to build `RuntimeClosure`, in `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs`
- [X] T028 [US2] Close the optional seed transitively, subtract `RuntimeClosure`, and use the result in place of the depth-1 `state.any_hard` guard in the finalize pass at `bun_lock.rs:808-860`, preserving the existing `-dependencies`-beats-`-peers` derivation-string precedence, in `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs`
- [X] T029 [US2] Replace the finalize pass's comments describing `any_hard` as the cancellation rule with the transitive form and a pointer to Probe C, in `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs`

### Regression and evidence for US2

- [X] T030 [US2] Assert the m667 optional-peers behaviour and its `bun-optional-peers` derivation value are unchanged (FR-009, contract C4) — **the premise was wrong**: measurement (Probe E2) shows real bun emits `optionalPeers` as a JSON *array* while m667's Pass 2 reads every section with `as_object()`, so `seen_optional_peers` never fired and `bun-optional-peers` was an unreachable value. There was no behaviour to preserve. US2 makes it reachable by sourcing the derivation from its own array-aware closure; the separate `depends`-edge gap is left unfixed as a graph change out of scope
- [X] T031 [US2] Verify bun's development-axis output is byte-identical against T002's baseline (FR-010, SC-004, contract C5) — the m1271 dev-axis helper `bun_workspace_dev_scopes` is untouched by this arm (verified by diff), and all 10 `m1271_us1_bun_*` tests pass, including `..._optional_is_never_demoted_to_development` and `..._dev_and_protected_sets_are_disjoint`
- [X] T032 [US2] SC-006 load-bearing check: delete T025's workspace seed, confirm a named US2 test fails, restore, and record which test

**Checkpoint**: three of five dialects agree. Shippable without US3 or US4.

---

## Phase 5: User Story 3 — yarn Berry (Priority: P3)

**Goal**: an ordinary Berry project — one that never hand-wrote
`dependenciesMeta` — reports its optional dependencies.

**Independent Test**: scan a Berry fixture whose manifest contains **no**
`dependenciesMeta` key, the realistic shape. The component reports the
optional scope.

**This arm is misleading about its own coverage, not merely
incomplete**: it has passing tests for a manifest shape real Berry
projects do not produce, and a comment plus a fixture header that both
assert the lockfile does not carry the flag. Measurement says it does.

### Tests for US3 (write first)

- [ ] T033 [P] [US3] Generate a manager-produced Berry fixture whose manifest declares a dependency optional and whose manifest has **no** `dependenciesMeta` key after install, confirming by inspection that the lockfile's root workspace entry carries `dependenciesMeta.<n>.optional: true`, and commit it under `waybill-cli/tests/fixtures/` with the yarn version in its header
- [ ] T034 [US3] Add a failing test asserting the no-`dependenciesMeta`-manifest fixture classifies its optional dependency optional **and** carries `waybill:optional-derivation` (contract C1, FR-008, acceptance scenario 1, SC-005's Berry-specific clause) in `waybill-cli/tests/optional_dep_yarn_berry_e2e.rs`
- [ ] T035 [P] [US3] Add a failing test for a non-root lockfile entry carrying `dependenciesMeta` — the named dependency is optional (acceptance scenario 2) in `waybill-cli/tests/optional_dep_yarn_berry_e2e.rs`
- [ ] T036 [P] [US3] Add a failing test for the FR-004a stale-lockfile case — a manifest marking a dependency optional and a lockfile predating that edit still classifies it optional (acceptance scenario 4) in `waybill-cli/tests/optional_dep_yarn_berry_e2e.rs`

### Implementation for US3

- [ ] T037 [US3] Add a `berry_optional_names_from_lockfile` reader extracting `dependenciesMeta.<name>.optional == true` from every lockfile entry, root workspace and non-root alike, in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`
- [ ] T038 [US3] Union the lockfile source with the existing `berry_optional_names_from_pkg_json` result rather than replacing it, keeping the manifest path intact per FR-004a, at the `optional_names` construction at `yarn_lock.rs:133`, in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`
- [ ] T039 [US3] Build the Berry runtime and optional closures over the lockfile's entry edges and subtract, so a transitively optional package is classified and a runtime-reachable one is not (FR-006, FR-007a), in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`
- [ ] T040 [US3] Apply the existing `is_peer_optional` guard to the unioned set, preserving the current KEEP-BOTH polarity (FR-009), in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`

### FR-014 — the two texts that document the defect as a fact

- [ ] T041 [US3] Correct the comment at `yarn_lock.rs:127-129` which asserts "Berry's lockfile does NOT carry the flag (it lives in package.json only)" to state both sources and cite `measurements/findings.md`, in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`
- [ ] T042 [US3] Correct the `dev_scope_optional_meta_berry` test's header, which claims the flag lives in package.json "and nowhere else" while the test's own body writes `dependenciesMeta` into the lockfile, in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`

### Regression and evidence for US3

- [ ] T043 [US3] Assert a project that DOES hand-write `dependenciesMeta` classifies unchanged, proving the manifest source was retained and not replaced (acceptance scenario 3, FR-004a) in `waybill-cli/tests/optional_dep_yarn_berry_e2e.rs`
- [ ] T044 [US3] Assert the existing yarn peer-precedence suite passes unchanged (contract C4) in `waybill-cli/tests/optional_dep_yarn_peer_precedence.rs`
- [ ] T045 [US3] Verify Berry's development-axis output is byte-identical against T002's baseline (FR-010, SC-004, contract C5)
- [ ] T046 [US3] SC-006 load-bearing check: delete T037's lockfile reader, confirm the T034 no-`dependenciesMeta` test fails, restore, and record it — this is the check that would have caught the original defect

**Checkpoint**: four of five dialects agree, and the Berry arm's tests
now assert a shape real projects produce.

---

## Phase 6: User Story 4 — yarn v1 (Priority: P4)

**Goal**: a yarn v1 project's optional dependency is classified
optional.

**Independent Test**: scan a yarn v1 fixture whose manifest declares one
dependency optional — that component reports the optional scope.

**The only arm whose root answer cannot come from the lockfile.**
Measured: a v1 lockfile has no root or workspace entry, so a root
declaration has nowhere to go. It does record transitive optional edges
as a per-entry sub-block, and the reader already collects them at
`yarn_lock.rs:241` and already subtracts `regular_children_seen` at
`yarn_lock.rs:372` — one level deep.

### Tests for US4 (write first)

- [ ] T047 [P] [US4] Generate a manager-produced yarn v1 fixture with a root `optionalDependencies` declaration, synthetic names, and commit it under `waybill-cli/tests/fixtures/` with the yarn version in its header
- [ ] T048 [US4] Add a failing test asserting the root declaration is classified optional **and** carries `waybill:optional-derivation` (contract C1, FR-008, acceptance scenario 1) in `waybill-cli/tests/optional_dep_yarn_v1_e2e.rs`
- [ ] T049 [P] [US4] Add a failing test for the transitive case asserting a package reachable only through optional sub-block edges is optional, and one reachable by a runtime path is not (contracts C2 and C3, acceptance scenario 2) in `waybill-cli/tests/optional_dep_yarn_v1_e2e.rs`

### Implementation for US4

- [ ] T050 [US4] Seed the v1 optional set from the root `package.json > optionalDependencies` keys, which measurement shows is its only possible source (FR-005), in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`
- [ ] T051 [US4] Replace the one-level `optional_children_seen.difference(&regular_children_seen)` at `yarn_lock.rs:372` with the transitive form — close the runtime set over the entry edges, close the optional set, subtract — preserving the existing diamond-shape outcome (FR-006, FR-007a), in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`
- [ ] T052 [US4] Update the comment at `yarn_lock.rs:365-371` describing the subtraction as the FR-007 diamond rule to describe the transitive closure and cite Probe C, in `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`

### Regression and evidence for US4

- [ ] T053 [US4] Assert a yarn v1 project with no optional declarations produces byte-identical output to before the feature (acceptance scenario 3) in `waybill-cli/tests/optional_dep_yarn_v1_e2e.rs`
- [ ] T054 [US4] Verify yarn v1's development-axis output is byte-identical against T002's baseline (FR-010, SC-004, contract C5)
- [ ] T055 [US4] SC-006 load-bearing check: delete T050's manifest seed, confirm the T048 test fails, restore, and record it

**Checkpoint**: all five dialects agree.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: the contracts no single-arm phase can assert, the error
paths, and the gates.

### The cross-dialect contract (the reason the feature exists)

- [ ] T056 Build the same small dependency graph in all five dialects — one optional declaration, one runtime declaration, one transitively-optional package, one contested package — and assert all five produce identical optional classification (SC-002, SC-003, contract C10) in a new `waybill-cli/tests/optional_dep_cross_dialect.rs`
- [ ] T057 Assert against the T001 baseline that SC-001 holds per dialect: exactly one optional component where the pre-feature output had zero

### Error paths (FR-011, FR-012, contracts C8 and C9)

- [ ] T058 [P] Assert an optional declaration naming a package with no lockfile entry is dropped with a diagnostic and fabricates no component, in each of the four dialects (FR-012, contract C8) in `waybill-cli/tests/optional_dep_classification.rs`
- [ ] T059 [P] Assert a lockfile malformed in the axis-bearing region leaves the axis unset, does not abort the parse, and does not guess, in each of the four dialects (FR-011, contract C9) in `waybill-cli/tests/optional_dep_classification.rs`
- [ ] T060 [P] Assert an optional declaration naming a workspace member or a `link:`/`file:`/`workspace:` target invents no registry identity, matching the development axis (spec Edge Cases) in `waybill-cli/tests/optional_dep_classification.rs`

### Evidence and documentation

- [ ] T061 [P] Re-run both probes and confirm every row of `measurements/findings.md` reproduces and that the cancellation probe's two cases still disagree (SC-007) — a run in which they agree indicates a broken harness, not a changed manager
- [X] T062 [P] Confirm no new `waybill:optional-derivation` value was introduced — all four arms reuse `npm-optional-dependencies` or `bun-optional-dependencies`/`bun-optional-peers`, and C122's extractor already exists at `waybill-cli/src/parity/extractors/mod.rs:550` — and assert `parity::extractors::tests::every_catalog_row_has_an_extractor` and `holistic_parity` both pass. Add a row to `docs/reference/sbom-format-mapping.md` ONLY if that confirmation fails, since a row without its extractor breaks that gate (FR-008)
- [X] T063 [P] Record in `measurements/findings.md` any place where implementation found a manager behaviour the probes did not capture, so the next format move starts from a complete record — **done: Probe D**, three findings (no `peer:` key exists in v9; the flag must AND-fold across peer contexts; pnpm already cancels for peers), plus the committed `peer-context-probe.sh`. Two more fixtures asserting shapes pnpm cannot emit were found and paired with measured counterparts

### Gates

- [X] T064 Run the mandatory gate on default features with no `+toolchain` argument and assert on the positive signal: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`
- [ ] T065 Enumerate every job's conclusion on the PR rather than reading one summary field, and note that the corpus goldens are `#[ignore]`d locally and skipped by the PR lane — a reader change of this shape surfaces as a red nightly, and goldens are never generated locally
- [X] T066 Assert FR-016 deterministically: `git diff --stat main -- Cargo.lock '*/Cargo.toml'` shows no dependency-line change. `pre-pr.sh` runs clippy and tests and never inspects the manifests, so a new dependency would otherwise ship green (FR-016, SC-003's zero-new-deps posture)

---

## Dependencies & Execution Order

### Phase order

```
Phase 1 Setup  ──►  Phase 2 Foundational  ──►  Phase 3 US1 (P1)  ──►  Phase 7 Polish
                                           ├─►  Phase 4 US2 (P2)
                                           ├─►  Phase 5 US3 (P3)
                                           └─►  Phase 6 US4 (P4)
```

Phase 1 is genuinely blocking: T002's baseline cannot be captured after
an arm lands, and SC-004 is unverifiable without it.

Phase 2 is blocking by decision rather than by compilation — an arm
could ship without its version diagnostic, but shipping the
classification without the honesty guarantee reintroduces the defect
class for the next format move.

### Story independence

US1, US2, US3 and US4 are independent of each other and may be done in
any order or in parallel by different people. Priority order is value
order, not dependency order.

One file-level constraint: **US3 and US4 both edit
`yarn_lock.rs`**. They touch different functions, but they are not `[P]`
with respect to each other — sequence them or expect a conflict.

### Parallel opportunities

| Phase | Parallel tasks | Why safe |
|---|---|---|
| 1 | T002, T003 | different outputs, no shared file |
| 2 | T005, T006 | `pnpm_lock.rs` vs `bun_lock.rs` |
| 3 | T011, T012, T013 | separate test fns, after T010 establishes the fixture |
| 4 | T021, T023, T024 | fixture generation vs separate test fns |
| 5 | T033, T035, T036 | fixture generation vs separate test fns |
| 6 | T047, T049 | fixture generation vs a separate test fn |
| 7 | T058, T059, T060 and T061, T062, T063 | separate test fns; independent documents |

The four arms themselves are the largest parallel opportunity — three
developers could take US2, US3 and US4 concurrently once Phase 2 lands,
subject to the `yarn_lock.rs` constraint above.

---

## Implementation Strategy

### MVP

**Phase 1 + Phase 2 + Phase 3 (US1)** — 20 tasks. Ships the pnpm v9 arm,
which is one source expression and delivers both the root and the
transitive answer because pnpm publishes the closure already computed.
Takes the ecosystem from one of five dialects correct to two of five,
and establishes the FR-011a honesty guarantee for all four.

### Incremental delivery

Each arm is a separate PR. In priority order: US1 (one field), US2
(first real closure), US3 (source union plus the FR-014 corrections),
US4 (manifest root plus closure). Phase 7's cross-dialect test
(T056) only becomes meaningful once all four have landed, so it belongs
with US4's PR or a PR of its own.

### The one place this can silently break working code

`v9_importer_scopes` merges `dependencies` and `optionalDependencies`
into a single `protected` set. Correct for the development axis — both
are "not dev". Fatal for the optional axis: the cancelling set would
contain the packages being classified, and FR-006 would subtract a set
from itself.

T026 is where that trap is avoided explicitly, and T019 / T031 / T045 /
T054 are the per-arm assertions that catch it if the split is wrong.
Contract C5 is the statement being defended.

### On test evidence

Four tasks (T020, T032, T046, T055) delete the arm's own logic and
confirm a named test fails. They are not ceremony: this feature exists
because an arm had passing optional-classification tests while being
unable to classify any real project's optional dependency. A test that
passes without the code it covers is not evidence.
