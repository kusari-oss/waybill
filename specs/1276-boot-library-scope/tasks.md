# Tasks: a package the compiler ships is never scoped as absent from the artifact

**Input**: Design documents from `/specs/1276-boot-library-scope/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/README.md](./contracts/README.md),
[quickstart.md](./quickstart.md)

**Tests**: REQUIRED. FR-011 and SC-007 demand a test per requirement that fails
when the behaviour is removed, mutation-tested. FR-003's control gets its own
test, because the plausible wrong implementation over-exempts — the same defect
this feature fixes, pointing the other way.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel — different files, no dependency on an incomplete task
- **[Story]**: `[US1]`, `[US2]`, `[US3]`. Setup, Foundational and Polish carry no label.

## Paths

Single Rust workspace. The fix is in
`waybill-cli/src/resolve/scope_propagation.rs`; the set is threaded from
`waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs` through
`waybill-cli/src/cli/scan_cmd.rs`; the invariants are in
`waybill-cli/tests/corpus_harness_195/invariants/`.

---

## Phase 1: Setup

**Purpose**: establish the baseline this feature is measured against, and the
mutation harness. No production code.

- [X] T001 Record the baseline in `specs/1276-boot-library-scope/measurements/baseline.md`: run `WAYBILL=./target/debug/waybill bash specs/1273-haskell-nixpkgs-truthfulness/measurements/sc003-public-library.sh` and `python3 -I specs/1276-boot-library-scope/measurements/scope-always-present.py .`, recording `pretty` as the 1 compiler-supplied component among the 11 reachable only through the marked pair, the 8 non-boot controls, and the corpus figures (78 boot libraries of which 10 mis-scoped; 142 OS-package and 175 file-tier components carrying no scope). Any figure that differs means the corpus or the revision moved and SC-001 through SC-004's populations must be restated before anything is built on them (SC-008, C15).
- [X] T002 [P] Copy milestone 1275's mutation harness to `specs/1276-boot-library-scope/measurements/mutate.py`, updating only its docstring. It is parameterised — label, file, find/replace text, suites are all arguments — so nothing in it names a milestone. A mutant that SURVIVES is a finding about the test, and milestone 1275 had three survive first, each exposing a real coverage gap.
- [X] T003 [P] Create `specs/1276-boot-library-scope/measurements/findings.md` with an empty numbered list. Every surprise, probe error and departure below is appended as it happens rather than reconstructed at the end.

**Checkpoint**: the figures the success criteria quote are observed on this branch.

---

## Phase 2: Foundational (blocking prerequisites)

**⚠️ CRITICAL**: T004 is a constitution requirement that must precede
implementation. T005–T007 are shared by more than one story.

- [X] T004 Confirm `specs/1276-boot-library-scope/spec.md`'s FR-007a citation matches research R4 before any emission code is written. **The amendment was applied during the `/speckit.analyze` remediation**, which also found the guard had been attached to the wrong requirement: FR-006a's conditional sat on FR-006, which research resolved needs no new field, while FR-007 — which does — had no guard at all. This task is now the check rather than the edit, and it stays a task because the constitution directs reviewers to reject a spec that introduces a `waybill:*` field without the citation. Verify the spec states: CycloneDX 1.6 has `compositions[].aggregate` for the completeness of the component list but no document-level field for a pass's exemption count; SPDX 2.3 has document-level `annotations[]` only; SPDX 3.0.1 has a document-subject `Annotation` only. Result: one new row, KEEP-NO-NATIVE, parity-bridge, same shape as C184 and C188. And that FR-006 needs **no** new field because C170 already carries the fact, which is how FR-006a's conditional is discharged. Milestone 1275 treated this as a formality and spent three commits correcting it.
- [X] T005 Add `boot_libraries: Option<std::collections::BTreeSet<String>>` to `EnrichmentSummary` in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs` and assign it from the `BootSet { boot, .. }` destructured at `mod.rs:941`, which currently uses `boot` and drops it. Then thread it from the summary into the `propagate_non_runtime_scope` call at `waybill-cli/src/cli/scan_cmd.rs:4851`. The defect occurs **without** `--nix-closure` — `slack-web` and `haskell-security-advisories` are both `ScanMode::Offline` — so milestone 1275's boot-set load inside the `--nix-closure` arm is unreachable here and must not be reused. Same shape as m1275's `package_set` field, for the same reason: the value is already in hand.
- [X] T006 Implement the exempt-class predicate in `waybill-cli/src/resolve/scope_propagation.rs` with unit tests in the same file written first: given the nulled set and a component, decide whether it is compiler-supplied. `#[cfg(test)]` guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the crate's deny. Note the target: `resolve` unit tests run under `cargo test -p waybill --bin waybill`, **not** `--lib`, which reports `0 passed; … filtered out` for a test that exists.
- [X] T007 Verify research R3's coincidence and record it in `findings.md`: every component in the exempt class carries C170 `waybill:haskell-version-unresolved-reason = compiler-supplied`. R3 argues they must, because both derive from the same nulled set through `classify()`. **It is a measurement, not a deduction** — a component in the boot set without C170 would be exempt without recording why, which is FR-006 unsatisfied, and US2's design would change. Measured so far: 10 of 10 on the mis-scoped population; this task extends it to all 78.

**Checkpoint**: the predicate is tested in isolation, the set reaches the propagation, the spec carries its audit, and FR-006's carrier is confirmed rather than assumed.

---

## Phase 3: User Story 1 — what the compiler ships is not reported as absent (P1) 🎯 MVP

**Goal**: a package the pinned revision identifies as compiler-supplied never
carries `optional` or `excluded`.

**Independent Test**: scan `slack-web` offline against its pinned revision and
assert `pretty` carries no non-runtime scope, while the 8 non-boot components
below the same disabled stanza keep theirs.

**⚠️ T011 is one task on purpose.** Measured in research R1: all 10 affected
components become **undeclared** I3 violations the moment the marker is
removed, I3 has zero declarations today, and `Verdict::UndeclaredViolationKind`
makes an undeclared violation a hard lane failure. There is no ordering in
which the tree is green with half applied.

### Tests for User Story 1 (write first, watch fail)

- [X] T008 [P] [US1] Add the C2 test to `waybill-cli/src/resolve/scope_propagation.rs`'s test module (FR-001): a compiler-supplied component reachable only through non-runtime edges is **not** assigned a non-runtime scope, **and carries no scope at all**. Assert both halves. "No non-runtime scope" is also satisfied by an explicit `required`, so an implementation stamping `required` on the 10 would pass every other contract and SC-004's 569-survivor count while making them inconsistent with the 68 boot-library components that already carry none — the spec's Assumptions choose absence and this is what enforces it. Assert on the positive signal — that the component appears in the immunity count — never on the absence of an assignment, which a pass that never ran would also produce.
- [X] T009 [P] [US1] Add the C3 test (FR-002) to `waybill-cli/src/resolve/scope_propagation.rs`'s test module, asserting the exemption is **immunity, not repair**: the component appears in `PropagationSummary`'s immunity count and in **no** assignment count. A pass that assigned then removed the scope would produce the same document and a different summary, and the summary is what distinguishes them (data-model §3).
- [X] T010 [P] [US1] Add the C4 control test (FR-003) to `waybill-cli/src/resolve/scope_propagation.rs`'s test module: a component that is **not** compiler-supplied, reachable only through non-runtime edges, still receives the scope. This is the test for the mirror-image defect, and FR-011 singles it out: an over-exempting implementation makes every component below a disabled stanza look runtime, which is this feature's failure mode rather than a hypothetical one.
- [X] T010a [US1] Run T008–T010 and record in `findings.md` that each fails, and with what message. A test never watched to fail is not yet evidence.

### Implementation for User Story 1

- [X] T011 [US1] **One indivisible change** across `waybill-cli/src/resolve/scope_propagation.rs`, `waybill-cli/tests/corpus_harness_195/invariants/checks/scope_propagation.rs` and `waybill-cli/tests/nix_closure_hackage_identity.rs`, because the corpus lane is red between its parts (research R1):
  1. `waybill-cli/src/resolve/scope_propagation.rs` — add the exempt class to the immune set alongside `runtime_reachable`, so the propagation never reaches these components rather than reaching them and being corrected. Extend `PropagationSummary` with the exemption count.
  2. `waybill-cli/tests/corpus_harness_195/invariants/checks/scope_propagation.rs` — amend **I3** to exclude the exempt class from its population, and update its `statement` in `invariants/mod.rs` to say so. I3 currently demands the marker this feature removes; all 10 affected components would otherwise be undeclared violations. **Read I3's own module comment first**: it documents its boundary with I7 and records that a loose boundary once put two instances of one defect class under the wrong number.
  3. `waybill-cli/tests/nix_closure_hackage_identity.rs` — invert `t057`, milestone 1275's test asserting this defect. C12 and FR-010 forbid deleting it: the `optional_scope` fixture it drives is the only deterministic reproduction of a boot library inheriting a scope through closure-derived edges. **Keep its premise assertions** — that the fixture's member really is non-runtime and really carries the declared edge — or the inverted test passes vacuously.

  Both I3 and the propagation MUST read the exempt class from the same source. Two rows disagreeing about which components are exempt is worse than either being absent, because each is then locally satisfiable while the pair is not (data-model §4).
- [X] T012 [US1] Add FR-002a's new invariant in `waybill-cli/tests/corpus_harness_195/invariants/checks/` with its registry entry in `invariants/mod.rs`: no compiler-supplied component carries a non-runtime scope, from any origin. It is I3's complement, and with the amended I3 the two partition the class — every component satisfies exactly one precondition (C8, C9). Allocate the next free invariant id; I1–I8 are taken.
- [X] T013 [US1] Verify T008–T010 pass and `cargo test -p waybill --test public_corpus` is green with **no new rows in `invariant-declarations.toml`**. Adding the 10 there is forbidden: that file records defects awaiting a fix, and these are components this feature deliberately corrected (C8).
- [X] T014 [US1] Run mutations M1 (drop the exempt class from the immune set), M2 (exempt every component, not only compiler-supplied ones — the over-exemption mutant) and M3 (amend I3 but not the propagation, so the two disagree) through `measurements/mutate.py`, and confirm each is CAUGHT. Record in `findings.md`. M3 is the one most likely to survive, because the document it produces is correct while the invariant pair is incoherent.

**Checkpoint**: US1 is independently demonstrable and the corpus lane is green. **This is the shippable increment** — US2 and US3 add transparency, not correctness.

---

## Phase 4: User Story 2 — the exemption is auditable and derived (P2)

**Goal**: each exempted component records that the compiler supplies it, and
the exempt set keeps coming from the pinned revision's own configuration.

**Independent Test**: assert every exempted component carries C170, and that
the exempt set is empty when the compiler configuration is unavailable.

### Tests for User Story 2 (write first, watch fail)

- [X] T015 [P] [US2] Add the C5 test to `waybill-cli/tests/corpus_harness_195/invariants/` (or `waybill-cli/src/resolve/scope_propagation.rs`): every component the rule exempts carries C170 `waybill:haskell-version-unresolved-reason = compiler-supplied`. T007 measured the coincidence; this asserts it, because "both derive from the same set" is an argument about code that can change.
- [X] T016 [P] [US2] Add the C7 degradation test — and assert it against `haskell-aeson`'s real shape, per findings F2: that target carries **zero** C170 components because the pass never ran for it, while 11 of its components share names with a nulled set and one of them (`directory`) is non-runtime. A scan where the pass did not run must exempt nothing **even where names collide**. Place it in `waybill-cli/tests/corpus_harness_195/invariants/` or `waybill-cli/src/resolve/scope_propagation.rs`: with the compiler configuration unreadable — point `WAYBILL_NIXPKGS_CACHE` at an empty `tempfile::tempdir()` — the exempt set is empty, **no** component is exempted, and the scan still succeeds. Assert on the recorded exemption count being zero, not on the absence of exemptions, which a crashed pass would also produce. This is the fail-closed direction: unearned immunity is the dangerous default (FR-005).
- [X] T017 [P] [US2] Add the C6 test for FR-004 and FR-004a to `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`'s test module (where `candidate_series` lives): when the flake names a compiler series, the exempt set is that series' nulled names alone. **Already true by construction** — `candidate_series` scans `flake.nix` for `ghc<NN>` and returns only what it finds (research R2) — so this asserts rather than builds. The figures make it checkable: 41 names for `9.10.x` at revision `a799d3e3` against 50 for the union, so a correct implementation and an over-exempting one differ by count alone (SC-006a).
- [X] T018 [US2] Run T015–T017 and record that each fails, and how, in `findings.md`. T017 may pass on arrival, since FR-004a is satisfied by construction; if so record that as a departure rather than manufacturing a red run, and rely on T019's mutant for its teeth.

### Implementation for User Story 2

- [X] T019 [US2] Run, via `specs/1276-boot-library-scope/measurements/mutate.py`, mutations M4 (make the exempt set the union unconditionally, ignoring the flake's named series) and M5 (treat an unreadable configuration as exempting everything rather than nothing) and confirm both are CAUGHT. M4 is FR-004a's teeth and should be caught by T017's count assertion; M5 is FR-005's.

**Checkpoint**: the exemption explains itself and degrades safely.

---

## Phase 5: User Story 3 — the correction is visible rather than silent (P3)

**Goal**: the document records how many components the rule exempted, including
zero.

**Independent Test**: assert the count appears at document scope in all three
formats, and that a scan exempting nothing records zero rather than omitting
the record.

### Tests for User Story 3 (write first, watch fail)

- [ ] T020 [P] [US3] Create `waybill-cli/tests/scope_exemption_record.rs` with the C10 test: the emitted document carries the exemption count at document scope, in CycloneDX, SPDX 2.3 and SPDX 3. `PropagationSummary` already holds the figure but is **logged and never emitted**, and no document-scope row exists for the pass — only per-component C42 and C62. A log line cannot serve an operator comparing two documents, which is what C174 exists to fix for the neighbouring pass.
- [ ] T021 [P] [US3] Add the US3 scenario-2 test to `waybill-cli/tests/scope_exemption_record.rs`: a scan that exempts nothing records **zero** rather than omitting the record. "The rule exempted nothing" and "the rule did not run" are different facts, and the `skipped_unrooted` counter in `PropagationSummary` exists because they once looked identical from outside.
- [ ] T022 [US3] Run T020–T021 and record that each fails, and how, in `findings.md`.

### Implementation for User Story 3

- [ ] T023 [US3] Add the document-scope row to `docs/reference/sbom-format-mapping.md` with the Principle V audit clause from T004, and its three extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` plus the registry entry in `extractors/mod.rs`. A catalogue row without an extractor fails `every_catalog_row_has_an_extractor`, so they land in one change. **Insert the registry entry in row-id order**: `extractors_table_is_sorted_by_row_id` rejects a row grouped next to its topical neighbours, which is how milestone 1275 first placed C201.
- [ ] T024 [US3] Emit the count from `PropagationSummary` through the three builders under `waybill-cli/src/generate/{cyclonedx,spdx}/`, in all three formats with the same value (C11). Encode it as a string in every format, not an object in SPDX and a string in CDX: a CDX property value must be a string, and a row differing in shape between formats forces its extractor to compensate.
- [ ] T025 [US3] Verify T020–T021 pass, then run mutation M6 (emit the count only when nonzero) and confirm it is CAUGHT by T021. Record in `findings.md`.

**Checkpoint**: all three stories are independently functional.

---

## Phase 6: Polish & cross-cutting

- [ ] T026 [P] Assert C1 — `waybill sbom scan --help` is byte-identical to the parent commit. No new flag, no changed default (FR-013).
- [ ] T027 [P] Assert C4/SC-004 at corpus scale: **569 of the 579** non-runtime scopes across the corpus survive, and only the 10 move. This is the headline control for the over-exemption failure mode; run `measurements/scope-always-present.py` and diff against T001's baseline.
- [ ] T028 [P] Assert C16/FR-008 (and C13/FR-012) against `waybill-cli/tests/fixtures/cargo` and `waybill-cli/tests/fixtures/pip` — no OS-package, file-tier, npm, Go, cargo, Maven or Python component changes scope. Diff a normalized, sorted emission on a non-Haskell fixture against the parent commit, masking content-addressed IDs and timestamps.
- [ ] T029 [P] Assert C11/FR-009/SC-006 via `waybill-cli/src/parity/extractors/` — all three formats report the same scope for every component, via the parity extractors and a cross-format comparison on the affected targets.
- [ ] T030 Run SC-007a over `waybill-cli/tests/corpus_harness_195/invariants/`: demonstrate FR-002a's new invariant **fails** when a compiler-supplied component is given a non-runtime scope by any means. An invariant never seen to fail is not yet evidence of anything, and this one is expected green from the moment the fix lands because the direct-mark population is measured at **0**.
- [ ] T031 Run the full mutation set M1–M6 in one pass through `measurements/mutate.py` and record every verdict in `specs/1276-boot-library-scope/measurements/mutations.md`: which mutant, which test caught it, and **every survivor with its explanation**. Keep the harness's per-test attribution — milestone 1275's three survivors were all found because the verdict named *which* test caught each mutant (SC-007, C14).
- [ ] T032 Re-run every committed probe — `scope-always-present.py` and the m1273 `sc003-public-library.sh` — and confirm each reproduces its recorded finding (SC-008, C15).
- [ ] T033 Measure the **post-change** state and record it in `specs/1276-boot-library-scope/measurements/after-1276.md` beside T001's baseline: `pretty` unscoped on `slack-web` (SC-001), 0 of 78 boot libraries non-runtime corpus-wide (SC-002), the 8 controls still non-runtime (SC-003), 569 surviving (SC-004), the exemption count present in all three formats (SC-005). **Four success criteria are before/after ratios and the "after" is only real when measured** — milestone 1275 shipped with that gap until an analyze pass caught it, and its first attempt at the comparison used an invalid baseline because `git stash` reverts to HEAD. Build the baseline from a detached worktree at the parent commit and prove it lacks the feature by a positive signal in its own output.
- [ ] T034 [P] Update `docs/ecosystems.md`'s nix/Haskell section with the rule, and check that no other doc asserts boot libraries can carry a non-runtime scope.
- [ ] T035 Note in `findings.md` that this feature changes the goldens of `haskell-security-advisories` and `nix-closure-moat` (scope values on 10 components), so a `public-corpus.yml` dispatch is required before the nightly is green — and that milestone 1275, on which this branch is stacked, needs its own refresh first. The expected diff here is **2 targets, scope only, 10 components**; anything else is a finding.
- [ ] T036 Run the pre-PR gate with **no** `+toolchain` argument (#1189): `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Enumerate the per-target `test result: ok. N passed; 0 failed` lines and the `>>> all pre-PR checks passed.` line. Assert on that positive signal — the absence of the word FAILED is not evidence, since a build that never compiled also contains no failures. Never run `cargo fmt`.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: depends on Phase 1. **Blocks every story.** T004 is a constitution requirement that must precede implementation; T007 can change US2's design.
- **Phase 3 (US1)**: depends on Phase 2. **T011 is indivisible** — see its note.
- **Phase 4 (US2)**: depends on Phase 2 for the predicate; independent of US1's invariant work.
- **Phase 5 (US3)**: depends on Phase 2 (the count comes from the summary T005 feeds); independent of US1 and US2.
- **Phase 6 (Polish)**: depends on every shipped story.

### Within Phase 2

T004 is independent of the code tasks. T005 precedes T006's integration but not its unit tests. T007 is a measurement and can run alongside.

### Parallel opportunities

- T002, T003 together; T004 alongside T005–T007.
- T008–T010 together; T015–T017 together; T020–T021 together.
- T026–T029 and T034 together.

---

## Implementation Strategy

### MVP

Phase 1 → Phase 2 → **Phase 3**. US1 is the conformance fix and is shippable
alone: US2 and US3 add auditability and visibility, not correctness. Unlike
milestone 1275, where US1 without US2 was actively dangerous, here the
increment is safe on its own — the risk inside US1 is over-exemption, and
T010's control plus T014's M2 mutant are in that phase.

### Incremental delivery after the MVP

1. Phase 4 (US2) — the exemption explains itself and degrades safely.
2. Phase 5 (US3) — the document records the count.

---

## Notes

- `[P]` means different files and no dependency on an incomplete task.
- Commit after each phase. `Refs #1340`, never `Closes` — and no qualifier like "part 1" beside an issue number, which is the signal the keyword is wrong rather than a way to make it right.
- Commits end with `Assisted-by: AGENT_NAME:MODEL_VERSION`. No `Co-Authored-By:`, no session URL in any public artifact.
- Never generate corpus goldens locally; the refresh goes through `public-corpus.yml` workflow dispatch. The 109 corpus tests are `#[ignore]`d locally, so a green local gate says nothing about them — and this feature changes two of those targets (T035).
- `resolve` and `scan_fs` unit tests live in the **bin** target; `--lib` silently filters them out.
- Every number in this file traces to `research.md` or a measurement in `spec.md`. The two that do not yet — T007's extension to all 78 components, and T033's post-change figures — are tasks precisely because they are unmeasured.
