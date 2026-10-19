# Tasks: Every unreachable component says why it has no edge

**Feature**: `1218-explain-every-orphan` | **Date**: 2026-10-19
**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Issue**: #1214. Defers #1234, #1235.

## Format: `[ID] [P?] [Story] Description`

`[P]` = parallelizable (different file, no dependency on an incomplete
task). Story labels appear only in user-story phases.

## Path Conventions

All paths are repository-relative from `/Users/mlieberman/Projects/mikebom`.
The feature's whole emission change is in
`waybill-cli/src/generate/orphan_reason.rs`; three supporting files are
`waybill-cli/src/cli/scan_cmd.rs`,
`waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs` and
`waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`.

## Two rules that govern the whole feature

**Never generate corpus goldens locally.** `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1`
is the flag never to set on a development machine — `docs/development/refreshing-corpus-goldens.md`
"Rule zero". Goldens are refreshed by dispatching the corpus lane
(T034), and the dispatch takes **`-f branch=<branch>`, never `--ref`**:
`--ref` silently runs against `main` and reports success (#1223).

**A better SC-001 number can mean a worse product.** Sixteen components
must still fail I7 when this is done — thirteen for #1234, three for
#1235. T020 and T021 are the assertions that catch an arm reaching them.
If the probe reports `total unexplained: 0`, the feature is broken.

---

## Phase 1: Setup (Shared Infrastructure)

- [X] T001 Review and lock the three wire strings in `specs/1218-explain-every-orphan/contracts/orphan-reason-vocabulary.md` — `build-toolchain`, `foreign-build-system`, `unreferenced-lock-node`. SBOM consumers key on these, so any change must happen here and now; after T005 they are frozen. Also confirm the contract's negative table (what each value must NOT be emitted on) still names the right counterexamples.
- [X] T002 Capture the pre-change baseline by running `python3 -I specs/1218-explain-every-orphan/measurements/i7-populations.py .` and pasting the output into the pull request draft. Expect `total unexplained: 34` and `Fail` on five targets. If it is not 34, stop: the goldens have moved since 2026-10-19 and every count in spec.md, plan.md and data-model.md needs re-deriving first (FR-019).
- [X] T003 [P] Record the `is_explained` arm-count baseline: `awk '/^fn is_explained/,/^}/' waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs | grep -c 'cdx_prop(component'` must be `5`. The scoping matters — a file-wide grep for the same pattern returns **9**, because the test module calls `cdx_prop` too, so an unscoped check measures the wrong thing and reports a wrong number. This is SC-014's tripwire — a sixth arm at the end means a new annotation was introduced and FR-001 was missed.
- [X] T004 [P] Record that `waybill-cli/src/parity/extractors/` is the diff that must stay empty (SC-007), and that `docs/reference/sbom-format-mapping.md` must show a row-text change only, never a new row.

**Checkpoint**: the vocabulary is frozen and the three numbers this feature is measured against (34, 5, 0) are written down.

---

## Phase 2: Foundational (Blocking Prerequisites)

No predicate is wired in this phase. The type surface exists first so the
compiler enumerates the remaining work (Constitution Principle IV).

- [X] T005 Add three variants to `OrphanReasonCode` in `waybill-cli/src/generate/orphan_reason.rs:71-91` — `BuildToolchain`, `ForeignBuildSystem`, `UnreferencedLockNode` — with their `as_str()` arms at `:82-90` returning exactly the T001 strings. Extend the enum's doc comment to carry the full eight-value priority order from the contract, replacing the five-value list wholesale so there is one order in one place.
- [X] T006 Add the three matching `usize` fields to `OrphanReasonCounts` at `waybill-cli/src/generate/orphan_reason.rs:96-121` and the three `tally()` arms at `:107-120`.
- [X] T007 Add a private helper to `waybill-cli/src/generate/orphan_reason.rs` that answers "does this component's source come from a file named X" by reading `c.evidence.source_file_paths` and comparing `std::path::Path::file_name`, NOT by substring-matching the path. Raw paths may be absolute or `path+file://<abs>`-shaped while every path in this feature's docs is the emit-time relativized spelling (research R2); a literal match finds nothing.
- [X] T008 Add a unit test in `orphan_reason.rs`'s existing `#[cfg(test)]` module asserting T007's helper returns the same answer for an absolute path, a scan-root-relative path and a `path+file://` path naming the same file. This is the test that would have caught the m226 Pants Go path-normalization defect.
- [X] T009 Add a private helper to `waybill-cli/src/generate/orphan_reason.rs` that reads `waybill:workspace-member` from `extra_annotations` and returns the member paths. The stored value is a JSON-array-**encoded string** (`"[\"examples/celery\"]"`), written at `waybill-cli/src/scan_fs/mod.rs:1663` — not a JSON array — so it must be parsed, not iterated. Return empty on absent or unparseable rather than failing the scan.
- [X] T010 Run `cargo build --workspace --all-targets` and confirm it compiles with the new variants unused. `--all-targets` is required: a plain build skips test helpers and would hide a missing match arm there.

**Checkpoint**: the vocabulary compiles, nothing is emitted yet, and the path trap has a regression test before any predicate depends on it.

---

## Phase 3: User Story 1 - The unused half of a duplicate pair says which it is (Priority: P1) 🎯 MVP

**Goal**: mechanism B's ten `uv.lock` resolution forks carry
`dead-lockfile-entry`, so a consumer reading a document with nine
duplicated names can tell which copy the resolution uses.

**Independent test**: `python-flask`'s offender count falls 23 → 13 and
every remaining offender is an `examples/celery/requirements.txt`
component. No cell flips to `Pass` on this story alone — #1234's thirteen
share the document — so the measurable outcome is the residual count and
its composition.

⚠️ **T011 and T012 must land in the same commit.** The window between
deleting the ecosystem guard and installing the ordering rule is exactly
when `maven-guice`'s three get classified `dead-lockfile-entry` and
#1235's cell closes silently.

### Implementation for User Story 1

- [X] T011 [US1] Delete both ecosystem guards in `waybill-cli/src/generate/orphan_reason.rs` — the sibling-index build at `:161` and the classification loop at `:176`, both `if ecosystem != "npm" && ecosystem != "golang" { continue; }`. The sibling index now spans every ecosystem, which mechanism B needs and which is also what makes T012 mandatory. Leave `versionless_identity` at `:49-55` untouched; its m866 doc comment explains why the key is the versionless PURL rather than `Purl::name()`, and re-breaking that mislabels every Go `/vN` module.
- [X] T012 [US1] Install the eight-arm total order from `contracts/orphan-reason-vocabulary.md` in the classification loop of `waybill-cli/src/generate/orphan_reason.rs`, with the three new arms strictly **below** all five m167 arms. Placing them below is what makes FR-004 hold by position rather than by case analysis. Keep the existing step-2a reachable skip and step-2b `flat-attached-fallback` preserve-guard ahead of everything.
- [X] T013 [US1] In `waybill-cli/src/generate/orphan_reason.rs`, narrow the `dead-lockfile-entry` arm so "has a reachable same-name sibling" is necessary but **not sufficient**: the component must also come from a lockfile resolution rather than a manifest-declared coordinate. Measured distinguishing inputs — `python-flask`'s ten carry `uv.lock` and no `waybill:source-type`; `maven-guice`'s three carry a `pom.xml` and `waybill:source-type = workspace`. Use T007's helper for the filename test.
- [X] T014 [US1] Add a unit test in `waybill-cli/src/generate/orphan_reason.rs`'s `#[cfg(test)]` module asserting a pypi component sourced from a lockfile, unreachable, with a reachable same-name sibling, classifies `dead-lockfile-entry`.
- [X] T015 [US1] Add a unit test in `waybill-cli/src/generate/orphan_reason.rs`'s `#[cfg(test)]` module asserting an npm component still classifies `dead-lockfile-entry` and a Go component still classifies `stale-go-sum-entry` under identical conditions — FR-004 at the unit level, before any golden exists to check it against.
- [X] T016 [US1] Add a unit test in `waybill-cli/src/generate/orphan_reason.rs`'s `#[cfg(test)]` module asserting a component sourced from a manifest (`pom.xml`, `source-type = workspace`), unreachable, **with** a reachable same-name sibling, classifies **nothing**. This is #1235's three in miniature and the single most important test in the feature.
- [X] T017 [US1] Run `cargo test -p waybill orphan_reason` and confirm every new unit test passes and no existing `orphan_reason` test regressed.

**Checkpoint**: mechanism B classifies, the m167 codes are unmoved, and the arm-4 trap has a unit-level guard. Golden-level confirmation waits for T034.

---

## Phase 4: User Story 2 - Foreign manifests and dangling lockfile nodes say so (Priority: P2)

**Goal**: mechanism C's three and mechanism D's three carry distinct
markers, and all three C components identify their build system
machine-readably.

**Independent test**: `go-opentelemetry` and `haskell-security-advisories`
report `Pass`; `maven-guice` falls 5 → 3; `python-flask` is unchanged by
this story.

### Implementation for User Story 2

- [X] T018 [US2] Implement the `foreign-build-system` arm in `waybill-cli/src/generate/orphan_reason.rs`: the component's declaring build system differs from the subject's. Measured population — two `pkg:bazel/*` from `WORKSPACE` in a maven subject, one `pkg:pypi/codespell` from a root `requirements.txt` in a Go subject; both carry `waybill:workspace-member = ["."]`, so this is a root-manifest test, not a sibling-member test. The arm must NOT fire on a component from the subject's own build system in a sibling module — that is #1235's three, whose build system is maven, as is the subject's.
- [X] T019 [US2] Implement the `unreferenced-lock-node` arm in `waybill-cli/src/generate/orphan_reason.rs`: a lockfile node that no other node in that lockfile references. Measured population — three `pkg:github/*` flake inputs from `flake.lock`, which the m925 reader emits as components while wiring edges only between lockfile nodes. The marker states the lockfile-internal fact; it must not claim the component is unused by the artifact, which a `flake.lock` alone cannot establish.
- [X] T020 [US2] Add a unit test in `waybill-cli/src/generate/orphan_reason.rs`'s `#[cfg(test)]` module asserting the `foreign-build-system` and `unreferenced-lock-node` arms produce **different** values on components that differ only in provenance. "A build system you do not use" and "a lockfile entry nothing references" call for different consumer actions, so one code for both would fail FR-001.
- [X] T021 [US2] Add a unit test in `waybill-cli/src/generate/orphan_reason.rs`'s `#[cfg(test)]` module asserting a maven component in a sibling module of a maven subject classifies **nothing** under the `foreign-build-system` arm. Together with T016 this closes both directions of the #1235 trap: that population satisfies "has a reachable sibling" *and* "is co-located with another build system".
- [X] T022 [US2] Emit `waybill:source-mechanism` (C55) on components the pip reader reads from a `requirements.txt`, in `waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs` beside the existing `extra_annotations` inserts at `:307`/`:318`/`:338`, before the map moves into the entry at `:385`. Model the three-line shape on `waybill-cli/src/scan_fs/package_db/bazel.rs:322-325`. Choose the value against C55's existing cross-reader vocabulary rather than inventing a spelling (research R7).
- [X] T023 [US2] Add a unit test in `requirements_txt.rs`'s test module asserting every entry it returns carries `waybill:source-mechanism`, mirroring the existing assertion at `waybill-cli/src/scan_fs/package_db/bazel.rs:526-533`.
- [ ] T024 [US2] Run `cargo test -p waybill orphan_reason requirements_txt` and confirm all new tests pass. Then re-verify T018 specifically: T022 widened `waybill:source-mechanism` from 2 components to 43, *after* T018's arm was written and tested, so add an assertion that a pip component carrying the new C55 value in a **pypi** subject does not classify `foreign-build-system`. The 41 recipients are all handled today by the step-2a reachable skip or by FR-008, but nothing asserts it and T022 is what could change that.

**Checkpoint**: two cells are ready to flip and SC-017 (C55 on all three mechanism-C components) is satisfiable — it reads 2 of 3 before this story.

---

## Phase 5: User Story 3 - A toolchain pin says it is build tooling (Priority: P3)

**Goal**: the two toolchain pins carry `build-toolchain`, so a consumer can
tell build tooling from a dependency.

**Independent test**: `haskell-language-server` reports `Pass`.

### Implementation for User Story 3

- [X] T025 [US3] Implement the `build-toolchain` arm in `waybill-cli/src/generate/orphan_reason.rs`, testing **all three** of `c.sbom_tier == design`, an empty `c.evidence.source_file_paths`, and an absent `waybill:workspace-member` (FR-012). `sbom_tier` is a **struct field** at `waybill-common/src/resolution.rs:103`, not an annotation. The tier alone is not sufficient — see T027a. Measured population — `pkg:generic/ghc-9.6.5@unspecified` and `pkg:generic/stackage-lts-23.28@unspecified`, the only two `design`-tier offenders in the corpus. Place the arm above `foreign-build-system` and `unreferenced-lock-node` per the contract's order: the tier test is the most specific predicate available.
- [X] T026 [US3] Add a unit test in `waybill-cli/src/generate/orphan_reason.rs`'s `#[cfg(test)]` module asserting a `design`-tier component with an empty `source_file_paths` and no `waybill:workspace-member` classifies `build-toolchain`. The two real components carry `waybill:source-files = [""]` and no member, so absence of those fields must be its own case and must not be treated as `"."`.
- [X] T027 [US3] Add a unit test in `waybill-cli/src/generate/orphan_reason.rs`'s `#[cfg(test)]` module asserting the arm does **not** fire on a `source`-tier component, and does not fire on a component already carrying `waybill:unresolved-reason` (FR-008). C151 is `design`-tier-only, so the two markers can co-occur on exactly this population and must not.
- [X] T027a [US3] Add a unit test in `waybill-cli/src/generate/orphan_reason.rs`'s `#[cfg(test)]` module asserting a `design`-tier component that **has** source files and a `waybill:workspace-member` classifies **nothing**. The live instance is `haskell-language-server`'s four `docs/requirements.txt` components — `docutils`, `myst-parser`, `sphinx`, `sphinx-rtd-theme`, all `design`-tier, all unreachable, all already carrying C151. They are documentation dependencies of a sibling member, not toolchain pins, and because they are already explained they are **not** I7 offenders: if the arm fires on them the probe's count does not move and SC-001 through SC-004 all still pass. This test and SC-018 are the only things that would catch it.
- [X] T028 [US3] Run `cargo test -p waybill orphan_reason` and confirm all three stories' unit tests pass together.

**Checkpoint**: all four mechanisms classify. Every assertion so far is unit-level; the corpus confirmation is Phase 7.

---

## Phase 6: The record

Documentation and committed data. These may run in parallel with each other
but all depend on Phase 5, and T031 depends on the golden refresh in T034 —
the declarations cannot be set to their residual counts until the refreshed
goldens show what the residual is.

- [X] T029 [P] Update the C45 row at `docs/reference/sbom-format-mapping.md:97` three ways: delete the sentence *"Emission scope is restricted to `pkg:golang/*` and `pkg:npm/*` ecosystems (FR-001); other ecosystems continue to carry no orphan-reason."* (FR-001b — this feature falsifies it); add the three new values to the open-enum list; replace the stated m167 priority order with the full eight-value order from the contract. Add no new row (SC-007).
- [X] T030 [P] Update the I7 prose at `docs/reference/sbom-invariants.md:148-162`, whose "When it fires" paragraph says *"Milestone 167 defined the reason vocabulary and milestone 194 anchored several populations; a new unexplained population is usually a reader that was added after those passes."* Record that the vocabulary now spans every ecosystem, and that the remaining two populations are reader defects tracked as #1234 and #1235 rather than vocabulary gaps.
- [ ] T031 Rewrite the five `I7` entries in `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml:218-278`: delete `go-opentelemetry`, `haskell-language-server` and `haskell-security-advisories` entirely; set `python-flask`'s `count_at_declaration = 13` with a reason citing **#1234** (the thirteen missing `requirements.txt` edges) and `maven-guice`'s to `3` citing **#1235** (the three duplicate maven coordinates). Keep `kind = "unreachable-and-unexplained"` on both survivors — the kind is unchanged, only the count and the attribution. Follow the wording precedent of commit `a3691e56`, which re-pointed the I8 declaration at the defect it actually had.
- [X] T032 [P] Add `bazel` and `nix/lockfile` to milestone 236's reader inventory at `specs/236-unresolved-reason/spec.md:187-194`, which lists 17 covered reader files and omits them, and extend `waybill:unresolved-reason` emission to `waybill-cli/src/scan_fs/package_db/bazel.rs` and `waybill-cli/src/scan_fs/package_db/nix/lockfile.rs` (FR-015). Record in the same edit that this closes **no** I7 cell: C151 attaches only to `design`-tier components and every I7 offender from those readers is `source`-tier, so it must not be counted toward SC-001.
- [X] T033 Record in `specs/236-unresolved-reason/spec.md` beside T032's edit that `cmake.rs` is the third reader m236 never enumerated and is deliberately unchanged here, with the measured reason (FR-016): the seventeen goldens contain exactly two components carrying a `waybill:source-mechanism`, both `bazel-http-archive`, and nothing sourced from cmake — so the change would be unverifiable by any lane. A corpus target that builds with cmake is the cheaper prerequisite.

**Checkpoint**: every document that tells a reader what the vocabulary covers has been corrected, and the two surviving declarations point at the defects they actually describe.

---

## Phase 7: Falsifiability, goldens and close-out

- [X] T034 Refresh the corpus goldens by dispatching the lane in **regen mode**:

```sh
gh workflow run public-corpus.yml \
  -f branch=1218-explain-every-orphan \
  -f regen_goldens=true
```

  Both inputs are required and the second is easy to miss — without
  `regen_goldens=true` the lane runs in *verify* mode, compares against the
  stale goldens and fails. `public-corpus.yml:84` maps that input to
  `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS`, which is the sanctioned place for
  it: "Rule zero" in `docs/development/refreshing-corpus-goldens.md` forbids
  setting it on a development machine, not in CI.

  Use **`-f branch=`, never `--ref`** (#1223). `public-corpus.yml:46` reads
  `ref: ${{ github.event.inputs.branch || github.ref }}`, so the input is
  what drives the checkout; `--ref` would change which workflow *file* runs
  while leaving the input at its `main` default. Note that
  `gh run view --json headBranch` reports **`main`** for a correct dispatch,
  because the workflow definition comes from the default branch — so
  `headBranch` is not the thing to check. Verify by reading the run's
  checkout step instead.

  The regenerated goldens arrive as an uploaded **artifact**
  (`public-corpus.yml:246-249`), not as a commit. Download it, then commit
  the refreshed goldens for every affected target and all three formats.
- [ ] T035 Verify the refreshed goldens with a normalized, sorted diff — mask content-addressed IDs including `rel-` and `anno-` prefixes, and `LC_ALL=C sort` before diffing — so SPDX 3 array reordering does not read as a semantic change. Expect **two** kinds of added property, not one (FR-002b): `waybill:orphan-reason` on 18 components in 5 targets, and `waybill:source-mechanism` on **41** components in 5 targets — `python-flask` 21, `pants-example-django` 13, `haskell-language-server` 4, `pants-example-python` 2, `go-opentelemetry` 1. Three of those targets are outside this feature's scope table and two report I7 `Pass`; their goldens change anyway, because FR-002a is a reader-level change and a reader that annotates only some of its output is worse than one that annotates none. Any *third* kind of delta is the finding.
- [ ] T036 Confirm SC-009 against the refreshed goldens: component counts, edge counts and the reachable set are identical to T002's baseline for every target and every format. This feature changes annotations only; any component or edge delta means an arm did something FR-009 forbids.
- [ ] T037 Add the positive control required by FR-017 to `waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs`'s test module, beside the existing `versionless_edgeless` fixture helper: a document whose only unexplained orphans are in a non-Go/npm ecosystem, asserted distinguishable from a fixture with none. A test that only checks for the absence of offenders proves nothing, because a document with no components also has none.
- [ ] T038 Add one mutation test per new value in `waybill-cli/tests/corpus_harness_195/invariants/mutation_tests.rs`, each showing the I7 cell or residual count it affects moves when that value is withheld (FR-018). A value that cannot be shown to fail is the vacuity this feature exists to remove, one level up.
- [X] T039 Add the three new counters to the **existing** `tracing::info!` at `waybill-cli/src/generate/orphan_reason.rs:320-327`, and correct the module header at `:18` which says the log "fires from the CALL SITE (not this module)" — it fires from the callee, at `:320`. (FR-012a, corrected at implement time: the log already exists and already fires unconditionally, so zero-matched is already distinguishable from not-run; the discarded `let _m167_orphan_reason_counts` binding at `scan_cmd.rs:4939` is harmless and stays. Without this task the new codes fire with no operator-visible counter.)
- [X] T040 Confirm SC-014: `awk '/^fn is_explained/,/^}/' waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs | grep -c 'cdx_prop(component'` still returns `5` (scoped to the function — the file-wide grep returns 9 and would hide the signal). A sixth arm means a new annotation was introduced and FR-001 was missed.
- [X] T041 Confirm SC-007: `git diff --stat waybill-cli/src/parity/extractors/` is empty and `docs/reference/sbom-format-mapping.md` shows a row-text change with no new row.
- [ ] T042 Confirm SC-016: every component carrying a milestone-167 code before the change carries the same code after it, by diffing the `waybill:orphan-reason` value per component ref across all seventeen goldens against T002's baseline. Only additions are permitted — never a changed or removed value.
- [ ] T043 Re-run `python3 -I specs/1218-explain-every-orphan/measurements/i7-populations.py .` and confirm `total unexplained: 16`, I7 `Pass` on 15 of 17, with `python-flask` at 13 and `maven-guice` at 3 (FR-019, SC-001 through SC-003, SC-015). Paste the output into the pull request. **A result of 0 is a failure**: it means an arm reached #1234's or #1235's population.
- [ ] T044 Confirm SC-004 by name: list the thirteen `examples/celery/requirements.txt` components and the three `pom.xml` components from T043's output and check each is still reported. This is the check that distinguishes a correct feature from one with a better headline number.
- [X] T045 [P] Add a CHANGELOG.md entry under the test-infrastructure or SBOM-emission heading: the three new C45 values, `dead-lockfile-entry`'s widening to pypi, the guard removal, and the two populations deliberately left reported with their issue numbers.
- [ ] T046 Run the pre-PR gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. No `+toolchain` argument — the pin in `rust-toolchain.toml` is what makes the gate match CI (#1189), and the constitution's own table has said so since v4.0.1 (#1232). Confirm `GATE_RESULT=pass` plus the per-target `test result: ok. N passed; 0 failed` lines, and enumerate every `^---- .* stdout ----` line before claiming green.
- [ ] T047 Confirm every success criterion by name, SC-001 through SC-017. SC-010 (no new I/O) and SC-012 (hermetic lane) are automatic under the gate, so name them here or nobody checks them deliberately; SC-005 is the nine-duplicate-name check in `python-flask`; SC-011 is the `unresolved-reason` grep in `bazel.rs` and `nix/lockfile.rs`; SC-017 is C55 on all three mechanism-C components.

---

## Dependencies & Execution Order

### Phase Dependencies

```
Phase 1 (setup)         → no dependencies
Phase 2 (foundational)  → Phase 1 (T001 freezes the strings T005 writes)
Phase 3 (US1, P1)       → Phase 2
Phase 4 (US2, P2)       → Phase 2  (independent of Phase 3)
Phase 5 (US3, P3)       → Phase 2  (independent of Phases 3 and 4)
Phase 6 (record)        → Phases 3-5, except T031 → T034
Phase 7 (close-out)     → everything; T031 sits between T034 and T043
```

### User Story Dependencies

US1, US2 and US3 are independent after Phase 2. Each adds its own arms to
one ordered list, and the order is installed once in T012 — so whichever
story lands first carries T011 and T012 with it, and the others slot their
arms into the list that already exists.

If US2 or US3 is taken first, T011 and T012 move into that phase. They are
written into US1 because US1 is the MVP, not because the other stories can
skip them.

### Within Each User Story

Predicate → unit tests → negative tests → run. The negative tests (T016,
T021, T027) are not optional polish: they are the only thing standing
between this feature and silently closing #1234's and #1235's cells.

### Parallel Opportunities

- T003 and T004 (baselines, different files)
- T029, T030, T045 (three different documents). **T032 and T033 are not parallel** — both edit `specs/236-unresolved-reason/spec.md`, and T033 goes beside T032's edit. **T018 and T019 are not parallel** either: both add an arm to the same `match` in `orphan_reason.rs`.
- US2 and US3 are fully parallel after Phase 2 if two people are available

### What cannot be parallelized

- **T011 and T012 must be one commit.** Between them, `maven-guice`'s three satisfy the widened sibling test and classify `dead-lockfile-entry`.
- **T031 after T034.** The declarations record residual counts; the residual is not observable until the refreshed goldens exist.
- **T034 is externally blocking.** The lane is a dispatch and a wait, not a local command.

## Implementation Strategy

**MVP = Phases 1-3** (T001-T017): mechanism B, the largest of the four, and
the one where the ambiguity is actively dangerous — a vulnerability
decision taken against the wrong copy of a duplicated name is wrong in a
way the document invited. It flips no cell on its own, so ship it with
T043's residual count as the evidence, not a verdict.

**Then Phase 4** (T018-T024): flips two cells and is the only phase that
touches a reader.

**Then Phase 5** (T025-T028): flips the third.

**Phases 6-7 are not optional.** T031 is load-bearing — the lane fails on a
stale declaration, so an un-updated survivor breaks the build, which is the
mechanism that makes the record self-enforcing. T037, T038 and T044 are
what make the feature falsifiable rather than merely green.

## Task Count

48 tasks: 4 setup, 6 foundational, 7 US1, 7 US2, 5 US3, 5 record, 14
close-out.

## Traceability

Every requirement and criterion maps to at least one task. Where a task
covers a requirement without citing its ID inline, the mapping is here.

| Requirement | Tasks |
|---|---|
| FR-001 C45 values, not a new annotation | T005, T040 |
| FR-001a mechanism B reuses `dead-lockfile-entry` | T013, T014 |
| FR-001b delete C45's Go/npm restriction sentence | T029 |
| FR-002 one code for C; no build system in the value | T018 |
| FR-002a pip emits C55 | T022, T023 |
| FR-002b C55's radius is 41 components, not 3 | T035 |
| FR-003 the ecosystem guard goes | T011 |
| FR-004 m167 assignments unchanged | T012, T015, T042 |
| FR-005 C45's existing row and extractor only | T029, T041 |
| FR-006 no new I/O on the emission path | T007, T047 |
| FR-007 a stated total order | T012, T029 |
| FR-008 never a second explaining marker | T027 |
| FR-009 no edge or reachability change | T036 |
| FR-010 I7's two `NotApplicable` verdicts unchanged | T043, T047 |
| FR-011 no catch-all; deferred populations still fail | T012, T044 |
| FR-012 E is a C45 value, not C151, not a tier; predicate is three-part | T025, T027, T027a |
| FR-012a surface the discarded counts | T039 |
| FR-013 delete the three fixed declarations | T031 |
| FR-014 rewrite the two survivors | T031 |
| FR-015 C151 to bazel and nix/lockfile; closes no cell | T032 |
| FR-016 record why `cmake.rs` is untouched | T033 |
| FR-017 positive control | T037 |
| FR-018 mutation test per value | T038 |
| FR-019 re-run the probe at close | T002, T043 |

| Criterion | Tasks |
|---|---|
| SC-001 34 → 16 | T043 |
| SC-002 I7 `Pass` on 15 of 17 | T043 |
| SC-003 the three cells flip, two fall | T043 |
| SC-004 the deferred sixteen still fail | T016, T021, T044 |
| SC-005 nine duplicate names resolved | T047 |
| SC-006 exactly two `I7` entries, 13 and 3 | T031, T047 |
| SC-007 zero new rows, zero new extractors | T041 |
| SC-008 the control fails when reverted | T037 |
| SC-009 counts and reachability byte-identical | T036 |
| SC-010 no new I/O | T047 |
| SC-011 `unresolved-reason` in both readers | T047 |
| SC-012 hermetic lane green | T047 |
| SC-013 pre-PR gate green | T046 |
| SC-014 `is_explained` still five arms | T003, T040 |
| SC-015 probe reproduces the closing numbers | T043 |
| SC-016 no m167 assignment changed | T042 |
| SC-017 C55 on all three mechanism-C components | T047 |
| SC-018 the four `docs/requirements.txt` components keep C151 and gain nothing | T027a, T047 |
