# Tasks: the nix-closure tier says why it degraded

**Input**: Design documents from `/specs/1274-nix-degradation-reason/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/README.md, quickstart.md

**Tests**: REQUIRED. FR-015 demands every requirement be covered by a test
that fails when the behaviour is removed, and names a negative test for the
classifier specifically — an over-matching classifier converts one
uninformative answer into one *misleading* answer, which FR-005 calls worse
than the status quo. Mutation steps are per requirement.

**Organization**: by user story, with one deliberate departure recorded below.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no dependency on incomplete work)
- **[Story]**: US1 / US2 from spec.md
- Exact file paths in every description

## Path Conventions

Single Rust workspace. Sources under `waybill-cli/src/`, integration tests
under `waybill-cli/tests/`, probes under this feature's `measurements/`.

## Why two things sit in Foundational rather than in a story

Both are forced by the dependency graph, not by preference:

1. **The salient-line rule (`rposition` + relocation).** R1 measured that
   with `position()` the classifier is handed a bare `error:` opener and can
   never match the clause however the anchor is written. So US1's classifier
   is **untestable** until this lands, even though the rule is nominally
   US2's concern.
2. **C190's `{reason, detail}` shape.** US1 needs `reason` in the document
   and US2 needs `detail` there. Landing the shape once, with both keys,
   avoids changing a published wire format twice and avoids rewriting the
   same six assertions twice.

---

## Phase 1: Setup

- [ ] T001 Extend `specs/1274-nix-degradation-reason/measurements/reproduce-1283.sh` with a third fixture, `nixconfig_unrelated/`: a flake whose `nixConfig.allow-import-from-derivation = true` and whose default package is a nonexistent attribute. R2 measured that this emits `warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.` followed by `error: attribute '…' missing`, which is the false positive FR-005a guards against — the probe must carry it so the guard stays re-verifiable
- [ ] T002 [P] Add a `--raw-stderr` mode to the same probe that prints the line index, the error-predicate matches and the byte offset of the disabled-clause for each fixture, so R1's "the cause is at byte 1320 of 1382, 1,155 past the window" is reproducible rather than quoted

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ Blocks both stories.** T003 is the FR-016 baseline; T004–T006 are the
rule, which US1's classifier cannot be tested without; T007–T009 are the wire
shape, which both stories write into.

- [ ] T003 Capture the pre-change baseline: run the probe and save its output to `specs/1274-nix-degradation-reason/measurements/baseline-0.14.0.md`, and record that `grep -rl "nix-closure-degraded" waybill-cli/tests/fixtures/public_corpus/` returns nothing. FR-016 requires each correction be justified against a measurement, and a baseline taken afterwards has already lost the comparison
- [ ] T004 Move `salient_stderr` out of `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs` into `waybill-cli/src/scan_fs/package_db/nix/eval/mod.rs` (or a sibling both paths can reach), `pub(crate)` within the nix module tree. Do NOT copy it — FR-008, and copying is the defect this milestone fixes. `guard()` two definitions below the drift already carries the comment *"two copies would drift, and the drift would be exactly the thing the guard prevents"*
- [ ] T005 Change the selection from `position` to `rposition` in the relocated `salient_stderr`, with a comment recording R1's measurement: nix prints the stack trace first and the cause last, so on the measured refusal the first match is a bare `error:` opener at line 1 of 26 while the clause is on line 25, byte 1320 of 1382 — 1,155 characters past the 300-character window. Note that `rposition` is strictly better rather than a trade: identical where one line matches, picks the cause where a trace precedes it
- [ ] T006 Add unit tests for the relocated rule beside it, over inline `&str` literals (no `nix` needed): the measured 26-line refusal stderr selects the clause; a single-error stderr selects the same line either way; a stderr with no identifiable error line still returns something (FR-009, the retained `unwrap_or(0)`); and the R2 false-positive stderr selects the attribute error rather than the warning
- [ ] T007 Change C190's emitted value from a bare string to `{"reason": …}` in `waybill-cli/src/generate/cyclonedx/metadata.rs`, `waybill-cli/src/generate/spdx/annotations.rs` and `waybill-cli/src/generate/spdx/v3_annotations.rs`, rendered through `serde_json::Map` so keys sort and a golden can pin the string. Omit `detail` entirely when there is none — never `"detail": null` (data-model §2)
- [ ] T008 Correct the six C190 assertions in `waybill-cli/tests/nix_config_closure.rs` to read the `reason` field rather than the bare string: four assert specific reasons (`several-system-configurations`, `no-evaluable-attribute`, `offline-requested`, `ifd-refusal-unverified`) and two assert `None`. **Correct, do not pin** (FR-016). This suite is where FR-013 is enforced, so these assertions are what prove the existing wire strings survived the shape change
- [ ] T009 Update C190's row in `docs/reference/sbom-format-mapping.md`: it currently describes a "document-scope string" and the value is now an object. `mapping_doc_bidirectional` enforces this, so it is a task rather than a hazard. Add nothing to the catalog — FR-002b, C11: extending a row that already has three extractors is not adding one

**Checkpoint**: the rule picks the cause, the carrier can hold a reason, and
nothing has changed about which flakes degrade.

---

## Phase 3: User Story 1 — a refusal says it was a refusal (P1)

**Goal**: an operator reading the document can tell a deliberate policy
refusal from a broken flake.

**Independent test**: scan a `callCabal2nix` flake and assert the document
names import-from-derivation as the cause, without reading the log.

**Why P1**: it is the whole feature. Everything else is a consequence of
getting this one signal right.

### Tests first

- [ ] T010 [P] [US1] Write `waybill-cli/tests/nix_ifd_reason.rs` with a failing `m1274_us1_refusal_is_named_in_the_document` (FR-001, FR-002, C2, SC-001): scanning the `callCabal2nix` fixture records `reason = "import-from-derivation-required"` in the emitted document. Measured today: `evaluation-failed`
- [ ] T011 [P] [US1] Add failing `m1274_us1_an_unrelated_failure_keeps_the_generic_reason` (FR-005, FR-005a, C6, SC-004) to `waybill-cli/tests/nix_ifd_reason.rs`, using T001's `nixconfig_unrelated` fixture. **This is the negative test FR-015 names.** The setting's name is in stderr and the failure is a missing attribute; a classifier keyed on the bare name reports it as a policy refusal
- [ ] T012 [P] [US1] Add failing `m1274_us1_a_refusal_alongside_another_error_is_still_a_refusal` (FR-005c, C7) to `waybill-cli/tests/nix_ifd_reason.rs`: construct a stderr carrying both and assert the refusal wins **and** the detail retains the other error's line. Assert at the rule/classifier level over a literal rather than by building a flake that fails two ways, which is not reliably constructible
- [ ] T013 [P] [US1] Add failing `m1274_us1_a_reworded_clause_degrades_to_generic` (FR-005b, FR-006, C8) to `waybill-cli/tests/nix_ifd_reason.rs`: a stderr whose clause is altered classifies generic, with no panic and no failed scan
- [ ] T014 [P] [US1] Add failing `m1274_us1_other_reasons_keep_their_wire_strings` (FR-013, C10) to `waybill-cli/tests/nix_ifd_reason.rs`: assert the nine existing wire strings are unchanged, and assert `ifd-refusal-unverified` specifically — it means the refusal could not be *confirmed in effect*, close to the opposite of the new reason, and conflating them would tell an operator their nix is too old when their flake needs a grant
- [ ] T015 [P] [US1] Add failing `m1274_us1_the_reason_is_logged_too` (FR-003, C13) to `waybill-cli/tests/nix_ifd_reason.rs`, asserting on stderr. Separate from the document assertions, which read the emitted JSON with the log discarded — milestone 1273 found that one assertion covering both halves lets either rot
- [ ] T016 [US1] Create the fixtures T010–T015 need. The `callCabal2nix` and handwritten flakes already exist in the probe; commit them under `waybill-cli/tests/fixtures/nix_ifd/` so the test suite does not depend on the probe's `mktemp` dirs, and record in a README that these need a real `nix` and are therefore `#[ignore]`-free but gated on `nix` being present the way `nix_config_closure.rs` already gates

### Implementation

- [ ] T017 [US1] Add `ImportFromDerivationRequired(String)` to `DegradationReason` in `waybill-cli/src/scan_fs/package_db/nix/eval/reason.rs` with wire string `import-from-derivation-required`, and a doc comment distinguishing it from `IfdRefusalUnverified` — the refusal taking effect versus the refusal not being confirmable
- [ ] T018 [US1] Implement the classifier beside the relocated rule: a `const IFD_REFUSED_CLAUSE: &str = "because the option 'allow-import-from-derivation' is disabled"` matched against the **selected error line**, satisfying both of FR-005a's conditions — the clause appears only in the refusal, and the selection already excludes warnings. Record R2's measured false positive in the comment
- [ ] T019 [US1] Implement FR-005c's precedence as an explicit search for the clause across error lines, not as a by-product of the selection: with `rposition` the last error line may be the *other* error, so the refusal has to be looked for. Keep the other error's line in the detail (C7)
- [ ] T020 [US1] Wire the classifier into both failure paths — `nix/closure/mod.rs:310` and `nix/eval/invoke.rs:457` — so a refusal is classified wherever it surfaces, and the generic reason remains the fallback (FR-006)
- [ ] T021 [US1] Populate C190's `reason` from the new variant through the existing `DegradationReason::wire()`, so no emitter learns about this variant specifically (FR-004: one carrier, no parallel channel)

**Checkpoint**: US1 is independently shippable — the document distinguishes a
refusal from a failure, even with `detail` still absent.

---

## Phase 4: User Story 2 — the detail names the line that explains the failure (P2)

**Goal**: the recorded detail is the cause, not whichever line nix printed
first, on every path.

**Independent test**: induce an evaluation failure whose explanatory line is
not the first line of stderr, and assert the recorded detail contains the
explanatory line.

**Why P2**: narrower than US1 and partly subsumed by it — once the reason is
specific, the detail matters less for *this* cause. It matters for every
other cause, which is why it is not dropped: the same blind prefix
mis-reports a syntax error or a nixpkgs type error just as badly.

### Tests first

- [ ] T022 [P] [US2] Write `waybill-cli/tests/nix_degradation_detail.rs` with a failing `m1274_us2_the_document_carries_the_explanatory_line` (FR-002a, FR-007, C3, SC-002): the `callCabal2nix` scan's C190 value carries a `detail` containing nix's disabled-clause, read from the emitted JSON with the log discarded
- [ ] T023 [P] [US2] Add failing `m1274_us2_the_detail_is_the_cause_not_the_stack_trace` (FR-007, C4) to `waybill-cli/tests/nix_degradation_detail.rs`, asserting the detail contains **the clause** — not merely that it differs from the git warning. C4's whole point: a weaker assertion passes on 1,155 characters of stack trace
- [ ] T024 [P] [US2] Add failing `m1274_us2_both_paths_produce_the_same_detail` (FR-008, C5, SC-005) to `waybill-cli/tests/nix_degradation_detail.rs`: feed one stderr through the closure path's and the resolution path's detail selection and assert equality. Assert at the function level, since inducing the identical stderr from both paths through a flake is not reliably constructible
- [ ] T025 [P] [US2] Add failing `m1274_us2_a_flake_that_evaluates_is_untouched` (FR-010, C9, SC-003) to `waybill-cli/tests/nix_degradation_detail.rs`, using the handwritten control flake: no degradation recorded, and the component count unchanged. The control is load-bearing — a `callCabal2nix` flake that degrades proves nothing alone, because a broken fixture degrades too
- [ ] T026 [P] [US2] Add failing `m1274_us2_an_unidentifiable_failure_still_records_something` (FR-009) to `waybill-cli/tests/nix_degradation_detail.rs`: a stderr with no line matching the error predicate records the opening text rather than nothing. An unparseable failure is not an excuse for silence

### Implementation

- [ ] T027 [US2] Replace `out.stderr.trim().chars().take(200).collect()` at `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs:310` with a call to the relocated rule. This is the drift R1 found: the fix existed 100 lines away in `eval/invoke.rs`, with a doc comment quoting the cabal2nix message as its motivating case, and was never applied here
- [ ] T028 [US2] Populate C190's `detail` from the selected line in all three emitters, omitting the key when the reason carries none — `OfflineRequested`, `ToolAbsent` and `NoEvaluableAttribute` have nothing to add (data-model §2)
- [ ] T029 [US2] Confirm no second detail-selection rule exists anywhere under `waybill-cli/src/scan_fs/package_db/nix/` — grep for `take(200)`, `take(300)` and `chars().take(` and show the result, because FR-008's failure mode is a copy nobody noticed

**Checkpoint**: both paths select the same line, and it is the cause.

---

## Phase 5: Polish & Cross-Cutting

### Mutation testing — per requirement (FR-015)

`specs/1273-haskell-nixpkgs-truthfulness/measurements/mutate.py` is available
and refuses to report a verdict for a mutation it could not apply: a
replacement that does not change the file is an **ERROR**, not a SURVIVED.
Three mutations in milestone 1273 reported SURVIVED without ever being
applied, and a mutant that survives unapplied looks exactly like a missing
test.

- [ ] T030 (SC-006) Mutate T005 back to `position` and confirm the US1 classifier tests AND the US2 detail tests fail. This is the mutation that matters most: it is the real pre-change state, and R1 showed the classifier cannot match the clause under it — so a suite that stays green here is testing nothing about either story
- [ ] T031 [P] Mutate the classifier to match the bare setting name anywhere in stderr and confirm `m1274_us1_an_unrelated_failure_keeps_the_generic_reason` fails. The plausible wrong implementation, and the one R2 was measured to rule out
- [ ] T032 [P] Mutate FR-005c's precedence to take whichever error line comes first and confirm T012 fails
- [ ] T033 [P] Mutate FR-013 by changing one existing wire string and confirm T014 and `nix_config_closure.rs` both fail
- [ ] T034 [P] Mutate FR-002a by emitting the reason without the detail and confirm T022/T023 fail; separately mutate FR-003 by demoting the log and confirm T015 fails. FR-018-style document/log split: 1273 found that a single assertion over both halves lets either rot
- [ ] T035 [P] Mutate FR-010 by having the classifier change whether a flake evaluates and confirm T025 fails
- [ ] T036 (SC-006) Record every mutation in `specs/1274-nix-degradation-reason/measurements/mutations.md`, including any SURVIVED with its explanation — equivalent mutant, missing test, or harness error — and name the suite each verdict is relative to. A verdict scoped to one suite can report a survivor the wider suite catches

### Non-regression

- [ ] T037 Run `cargo test -p waybill --test nix_config_closure` and confirm 14 of 14 pass with the corrected assertions (FR-013, C10, C12)
- [ ] T038 Verify SC-007: every other nix tier and every other ecosystem is byte-identical. Run the adjacent nix and Haskell suites — `nix_flake_lock_reader`, `nix_flake_lock_graph`, `nix_flake_lock_failure_modes`, `nix_haskell_resolution_m926`, `nixpkgs_declarations`, `haskell_hpack_us1`, `haskell_optional_closure_us2` — and record each result. The degradation record is the only thing this milestone may change; a change anywhere else is blast radius, not progress
- [ ] T039 [P] Confirm zero corpus goldens changed: `git diff --name-only waybill-cli/tests/fixtures/public_corpus/` is empty. A golden that *does* change means something reached further than intended (R4, FR-016)
- [ ] T040 [P] Run the parity suites — `cargo test -p waybill --test holistic_parity`, `--test mapping_doc_bidirectional`, `--lib parity` — and confirm C190 still has three extractors and no row was added (FR-002b, FR-014, C11)
- [ ] T041 [P] Verify the CLI surface is unchanged (FR-011, FR-012, C1): diff `waybill sbom scan --help` against `main`, and confirm the branch's diff adds no `process::Command`, no `reqwest`, no `PATH` read, and leaves `Cargo.toml`/`Cargo.lock` untouched

### Closeout

- [ ] T042 [P] Update `docs/ecosystems.md`'s nix section to record that a refused import-from-derivation is now named as such, and that the degradation annotation carries a reason and a detail
- [ ] T043 [P] Record the Phase 0 findings that outlived planning in `specs/1274-nix-degradation-reason/measurements/findings.md`: R1's byte offsets, R2's false positive, R4's measured blast radius, and R5's single-version limit
- [ ] T044 Re-run the probe and confirm it reproduces its recorded findings (SC-008), updating each expectation to say which side of the fix it describes
- [ ] T045 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. No `+toolchain` (#1189). When checking clippy separately use `cargo clippy --workspace --all-targets -- -D warnings` — **with** the flag, which is what the gate runs; without it, dead-code and argument-count lints are warnings you will not see, which cost two gate cycles in milestone 1273

---

## Dependencies

```
Phase 1 (T001–T002)      probe fixtures, incl. R2's false positive
   ↓
Phase 2 (T003–T009)      ← BLOCKS BOTH STORIES
   T003  baseline (FR-016)
   T004–T006  the rule: relocate, rposition, unit tests
   T007–T009  the carrier: object shape, 6 assertions, catalog prose
   ↓
   ├──────────────────────────┐
   ↓                          ↓
US1 (T010–T021)  ──────→  US2 (T022–T029)
   the reason                 the detail
   ↓                          ↓
   └──────────────────────────┘
                ↓
      Phase 5 (T030–T045)
```

**Why US1 → US2 is an arrow and not a parallel pair.** They touch the same
emitters (T021 and T028 both write C190's value) and the same classifier
module. US2's T027 also assumes the relocated rule exists, which Phase 2
provides — but T024's "both paths agree" cannot be asserted until T020 has
wired the classifier into both. Running them concurrently means a merge in
three files.

**Why the rule is in Phase 2 and not in US2.** R1: with `position()` the
classifier receives a bare `error:` opener and can never match the clause.
US1's tests would fail for the wrong reason, and a passing classifier test
after T005 would not distinguish "the classifier works" from "the selection
finally reaches the clause".

## Parallel Execution

**Phase 1**: T002 is independent of T001.

**Phase 2**: T003 is independent of everything. T004–T006 are sequential
(relocate → change → test). T007–T009 are sequential (shape → assertions →
prose). The two groups are independent of each other, so a second pair of
hands can take one each.

**Within US1**: T010–T015 are all `[P]` — one new test file, separate
functions. T016's fixtures must land first or each test fails on a missing
path rather than on its assertion, which is the vacuous-failure trap
milestone 1273 hit.

**Within US2**: T022–T026 are all `[P]`.

**Phase 5**: T031–T035 are `[P]` (independent mutants). T038–T043 are `[P]`.
T030, T036, T037, T044 and T045 are sequential — each depends on the state
the previous leaves.

## Implementation Strategy

**MVP = Phase 2 + US1.** An operator who can tell a policy refusal from a
broken flake has the thing #1283 asked for. The detail is an improvement on
top, and US1's reason already carries the actionable half: *what you would
have to accept.*

**Suggested delivery: one PR.** Unlike milestone 1273 this is small — roughly
one enum variant, one word in a selection rule, one classifier, and a value
shape. US1 and US2 touch the same three emitters and the same module, so
splitting them means a stacked pair with a rebase between, and `swansong`'s
`currency: tree` costs a land cycle per PR. If it grows beyond that, US1 is
the natural cut.

**Two things to carry into execution:**

1. **Observe each test fail for the right reason.** A test that fails because
   a fixture path is missing is indistinguishable from one that fails because
   the behaviour is absent, and the second is the only one that counts.
2. **T030 is the gate on the whole exercise.** Reverting `rposition` must
   break both stories' suites. If it does not, the suites are asserting
   something other than what R1 measured.
