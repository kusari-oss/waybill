# Tasks: Opt-in `nix eval` resolution tier

**Input**: Design documents from `/specs/1034-nix-eval-tier/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/
**Issue**: **#971 part A** (the directory number is a milestone number, not an issue reference)

**Tests**: **Included.** Not optional here — spec SC-001…SC-009 are test-shaped,
`./scripts/pre-pr.sh` is a mandatory gate, and the parity-extractor gate fails
the build if a catalogue row lands without its extractor.

**Organization**: Grouped by user story. Note the ordering exception under
Phase 3.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on incomplete work)
- **[Story]**: US1–US5 per spec.md

## Path Conventions

Per plan.md's Structure Decision: the tier is `waybill-cli/src/scan_fs/package_db/nix/eval/`,
a new submodule under the existing nix reader, reusing `lockfile.rs` (revision
discovery) and `haskell_packages/cache.rs` unchanged.

---

## Phase 1: Setup

**Purpose**: Scaffolding and the flag surface, with nothing yet wired to `nix`.

- [X] T001 Create the module skeleton `waybill-cli/src/scan_fs/package_db/nix/eval/{mod,preflight,invoke,result,reason}.rs` with `pub(crate)` stubs and register `mod eval;` in `waybill-cli/src/scan_fs/package_db/nix/mod.rs`
- [X] T002 Add `--nix-eval`, `--nix-eval-system`, `--nix-eval-timeout-secs` to `waybill-cli/src/cli/scan_cmd.rs` (`ScanArgs`, the `sbom scan` struct — **not** `cli/scan.rs`, which is `trace`) per contracts/cli-flags.md, with `requires` on the two companions
- [X] T003 Write the `--nix-eval` help text stating what enabling it evaluates — nixpkgs at the pinned revision, not the project's own flake — and that the revision is the one repository-controlled value involved (spec FR-003) in `waybill-cli/src/cli/scan_cmd.rs`
- [X] T004 [P] Build the whole workspace with `cargo build --all-targets` to confirm the new clap fields compile in test helpers too, not only the bin (memory: `feedback_build_check_all_targets`)

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ CRITICAL**: No user story work begins until this phase completes.

### Measurements that gate two defaults

- [ ] T005 [P] Implement research task **T-R6**: measure evaluation cost and attribute-path behaviour for a *project's own* flake on both corpus Haskell targets; commit the probe to `specs/1034-nix-eval-tier/measurements/` and record findings in `research.md` §R8
- [ ] T006 [P] Implement research task **T-R5**: on a clean CI runner, measure wall-clock and byte cost of acquiring a pinned nixpkgs revision into a store that has never held it; commit probe + findings as above
- [ ] T007 Set the `--nix-eval-timeout-secs` default from T005, and SC-007's ratio from T005+T006, in `specs/1034-nix-eval-tier/contracts/cli-flags.md` and `spec.md`. **Do not choose a number before T005 and T006 have run** — no figure describing external behaviour ships without a measurement behind it

### Types and shared machinery

- [X] T008 [P] Implement the `NixSystem` newtype with `<arch>-<os>` validation and `FromStr` in `waybill-cli/src/scan_fs/package_db/nix/eval/result.rs` (Principle IV)
- [X] T009 [P] Implement the `DegradationReason` `thiserror` enum with all seven variants and their wire forms in `waybill-cli/src/scan_fs/package_db/nix/eval/reason.rs`
- [X] T010 [P] Implement the `EvaluationOutcome` struct including the `ifd_refused_verified` field in `waybill-cli/src/scan_fs/package_db/nix/eval/result.rs`
- [X] T011 Implement the budgeted-subprocess helper in `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs`, following the `Command` + `thread` + `mpsc::recv_timeout` pattern at `waybill-cli/src/scan_fs/package_db/golang/go_mod_graph.rs:81`
- [X] T012 [P] Unit-test `NixSystem` parsing and `DegradationReason` wire forms in the same files under `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]`

**Checkpoint**: types exist, nothing invokes `nix` yet.

---

## Phase 3: User Story 3 — Scanning never runs the repository's code (Priority: P1) 🎯 MVP gate

**Ordering exception**: US1, US2 and US3 are all P1, and within P1 this one goes
**first**. Its pre-flight is what makes evaluating permissible at all — building
US1 first would mean a working code path that runs repository-controlled
expressions with no verified guard. The measured reason is research R3: passing
`--option allow-import-from-derivation false` to a `nix` that does not support it
is a **silent no-op with exit code 0**.

**Goal**: The tier refuses to evaluate unless it has confirmed the
import-from-derivation refusal is in effect.

**Independent Test**: Scan the IFD fixture; assert no derivation is built **and**
that the tier reported a specific outcome — an unrun tier also builds nothing.

### Tests for User Story 3

- [X] T013 [P] [US3] Promote the research probe's IFD flake to a test fixture at `waybill-cli/tests/fixtures/nix_eval/ifd/flake.nix`, parameterised by host system
- [X] T014 [P] [US3] Write the failing test: scanning the IFD fixture with `--nix-eval` builds no `ifd-marker` store path, in `waybill-cli/tests/nix_eval_tier.rs`
- [X] T015 [US3] Add the **control assertion** to T014 — assert the tier actually ran and reported an outcome, so the test cannot pass by never reaching evaluation (memory: `feedback_a_passing_test_may_exercise_nothing`)
- [X] T016 [P] [US3] Write the failing test: a stub `nix` on `PATH` whose `config show` omits the setting yields `IfdRefusalUnverified` and degrades, in `waybill-cli/tests/nix_eval_tier.rs`, routing the `PATH` mutation through `crate::testing::EnvGuard::acquire()`

### Implementation for User Story 3

- [X] T017 [US3] Implement the pre-flight in `waybill-cli/src/scan_fs/package_db/nix/eval/preflight.rs`: run `nix config show --option allow-import-from-derivation false` and accept only on the exact line `allow-import-from-derivation = false`
- [X] T018 [US3] Make consuming `EvaluationOutcome.versions` impossible unless `ifd_refused_verified` is true, in `waybill-cli/src/scan_fs/package_db/nix/eval/mod.rs` — prefer a type-level guard over a runtime `if`
- [X] T019 [US3] Wire `--option allow-import-from-derivation false` into every evaluating invocation in `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs`
- [X] T020 [US3] Verify the guard has teeth: temporarily remove the pre-flight, confirm T014 fails, restore it. A safety gate that has never been observed to fail is not known to work (memory: `feedback_schema_gate_ref_stubs`)

**Checkpoint**: evaluation is gated. SC-004 satisfiable.

---

## Phase 4: User Story 1 — Correct versions for a Nix-built project (Priority: P1)

**Goal**: Evaluated versions supersede file-parsed ones.

**Independent Test**: Scan the corpus target pinned to `cbb5cf35…` and compare
every resolved version against `nix eval` via the existing oracle.

### Tests for User Story 1

- [ ] T021 [P] [US1] Write the failing test: the #1033 override components carry the evaluated version, in `waybill-cli/tests/nix_eval_tier.rs`
- [ ] T022 [P] [US1] Write the failing test: `cargo run -p xtask -- nix-oracle` reports zero `Disagree` verdicts for a `--nix-eval` scan of the corpus Haskell target (SC-001)
- [ ] T023 [P] [US1] Write the failing test for **SC-009**: assert one evaluation pass returned both the derivation closure and per-component `meta`, reading what evaluation returned rather than any emitted output (FR-018), in `waybill-cli/tests/nix_eval_tier.rs`

### Implementation for User Story 1

- [X] T024 [US1] Implement host-system detection as a **separate** impure call (`nix eval --impure --raw --expr 'builtins.currentSystem'`) in `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs` per contracts/nix-invocation.md §2
- [ ] T025 [US1] Implement the flake attribute-path strategy that research task **T-R6** selects (research.md §R8) in `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs`; a flake exposing no evaluable output degrades with `NoEvaluableAttribute` rather than erroring (FR-007). **Blocked on T005** — do not pick a strategy before the measurement
- [X] T026 [US1] Build the resolving expression in `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs`: pinned 40-char revision, **system as a literal**, per-attribute `tryEval … or null`, and **no `--impure`** (research R2)
- [X] T027 [US1] Escape component names when interpolating into the Nix expression — names come from parsed manifests and are untrusted input to a language evaluator (contracts/nix-invocation.md §3)
- [ ] T028 [US1] Request the derivation closure and per-component `meta` in the **same** evaluation as the versions, so #1034 and #1040 need no second pass (FR-018, FR-019), in `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs`
- [X] T029 [US1] Parse `nix eval --json` into `BTreeMap<String, Option<String>>` in `waybill-cli/src/scan_fs/package_db/nix/eval/result.rs`, treating `null` as absent rather than as an error or a version
- [X] T030 [US1] Implement reconciliation in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`: evaluated wins, file-parsed retained (FR-004, FR-005)
- [X] T031 [US1] Reuse `waybill-cli/src/scan_fs/package_db/nix/lockfile.rs` for revision discovery — do not add a second revision reader
- [X] T032 [US1] Confirm the resolving call never inherits the oracle's `--impure`; add a test asserting the constructed argv contains no `--impure` (research R2, contracts §6)

**Checkpoint**: SC-001 satisfiable. US3 + US1 together are the minimum shippable pair.

---

## Phase 5: User Story 2 — The tier never breaks a scan (Priority: P1)

**Goal**: Every failure degrades; the scan always exits 0.

**Independent Test**: Run with `nix` absent from `PATH`; assert success and that
output differs from the flag-off output only by degradation metadata.

### Tests for User Story 2

- [X] T033 [P] [US2] Write the degradation matrix test covering all seven `DegradationReason` variants (SC-006) in `waybill-cli/tests/nix_eval_tier.rs`, per the quickstart.md provocation table, with every environment mutation behind `EnvGuard::acquire()`
- [X] T034 [P] [US2] Write the failing test: flag on with `nix` absent produces output equal to flag-off output apart from the degradation annotation (SC-003)
- [X] T035 [P] [US2] Write the failing test: `--nix-eval-system` / `--nix-eval-timeout-secs` without `--nix-eval` are **argument errors**, not degradations
- [X] T036 [P] [US2] Write the failing test for **FR-010**: a `--nix-eval` scan writes nothing outside `/nix/store` and waybill's own cache — snapshot the writable tree before and after and diff, in `waybill-cli/tests/nix_eval_tier.rs`

### Implementation for User Story 2

- [X] T037 [US2] Implement the degradation decision tree from data-model.md in `waybill-cli/src/scan_fs/package_db/nix/eval/mod.rs`, with every edge landing on unchanged file-parsing output
- [X] T038 [US2] Distinguish `ToolAbsent` from `ToolUnusable` from `IfdRefusalUnverified` — three different operator remedies
- [X] T039 [US2] Budget revision acquisition **separately** from evaluation, mapping to `RevisionUnfetchable` vs `BudgetExceeded` (research R5) in `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs`
- [X] T040 [US2] Kill the child process on budget expiry rather than only abandoning the channel — `nix` imposes no time bound of its own (research R4)
- [X] T041 [US2] Ensure the flag-off path starts **no** `nix` process at all (FR-002); add a test asserting no subprocess is spawned
- [X] T042 [US2] Emit C180 `waybill:nix-eval-degraded` at document scope — **US2's own acceptance depends on it**, so it cannot wait for the annotation phase
- [X] T043 [US2] Emit C178 `waybill:nix-eval-tier` at document scope, including `revision` and the degradation reason (FR-016)

**Checkpoint**: SC-003 and SC-006 satisfiable.

---

## Phase 6: User Story 4 — The result says which machine it describes (Priority: P2)

**Goal**: The platform is an explicit parameter and is recorded.

**Independent Test**: Evaluate one project for two platforms; assert the
documents differ and each names its platform.

- [X] T044 [P] [US4] Write the failing test using the measured `hinotify` case — 0.4.2 on `x86_64-linux`, 0.1.8 on `aarch64-darwin` at one revision (SC-005) — in `waybill-cli/tests/nix_eval_tier.rs`
- [X] T045 [US4] Thread `NixSystem` from the flag through to expression construction in `waybill-cli/src/scan_fs/package_db/nix/eval/{mod,invoke}.rs`
- [X] T046 [US4] Degrade with a reason code when the named platform is unsupported by the flake, rather than failing
- [X] T047 [US4] Emit C179 `waybill:nix-eval-system` at document scope

**Checkpoint**: SC-005 satisfiable.

---

## Phase 7: User Story 5 — A disagreement stays visible (Priority: P2)

**Goal**: The superseded file-parsed value survives in the document.

**Independent Test**: Scan a project with a known override; assert the component
carries the evaluated version and metadata naming the superseded one.

- [X] T048 [P] [US5] Write the failing test for SC-008: evaluated version in `version`, file-parsed in C177, document divergence count matching
- [X] T049 [US5] Implement the `ResolutionDivergence` record in `waybill-cli/src/scan_fs/package_db/nix/eval/result.rs`
- [X] T050 [US5] Emit C177 `waybill:nix-eval-superseded-version` per component
- [X] T051 [US5] Emit C181 `waybill:nix-eval-origin` on **every** component the tier touched, agreements included — marking only divergences makes absence ambiguous (the trap C175 documents)
- [X] T052 [US5] Verify C177 is not conflated with the existing C172 `waybill:nixpkgs-version-disagreement` — opposite precedence, different source pair; add a test asserting both can coexist in one document

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T053 Add catalogue rows C177–C181 to `docs/reference/sbom-format-mapping.md` **in the same change as** their extractors in `waybill-cli/src/parity/extractors/mod.rs` — never doc-first, or `every_catalog_row_has_an_extractor` fails (memory: `feedback_sbom_format_mapping_extractor_gate`)
- [X] T054 Edit `docs/reference/sbom-format-mapping.md` by **exact string match**, not anchored regex, and diff the whole field afterwards (memory: `feedback_anchored_regex_edits_drift`)
- [X] T055 [P] Write extractors for C177–C181 covering all three formats in `waybill-cli/src/parity/extractors/`
- [ ] T056 [P] Implement research task **T-R7**: verify the R3 pre-flight behaves identically on the CI runner's `nix` build, not only Determinate 3.20.0; record in `research.md`
- [X] T057 Verify SC-002: with the flag off, every committed corpus golden is unchanged and no `nix` process is spawned
- [ ] T058 Regenerate corpus goldens **in CI, not locally**, if and only if T057 shows legitimate churn; regenerate all six golden-writing test files, not only the three `*_regression` ones (memory: `feedback_release_bump_regen_all_golden_tests`)
- [ ] T059 Verify golden churn with a normalized, sorted diff masking content-addressed IDs including `rel-` and `anno-` prefixes (memory: `feedback_verify_golden_churn_normalized`)
- [X] T060 [P] Document the tier in `docs/reference/nix-evaluation.md`, linked from `docs/index.md`: leads with the execution warning and the sandbox-or-trusted-flake guidance (FR-003a), states precisely what is evaluated, and gives the accuracy argument alongside the cost and the coverage limits
- [X] T061 Run the walker-audit grep locally if any new `fn walk`-shaped function appeared — it is not in `pre-pr.sh` (memory: `feedback_walker_audit_local_check`)
- [X] T062 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo "EXIT=$?"` — read the **script's** status, never a pipeline's — then assert the positive signals: `>>> all pre-PR checks passed.` and the per-target `test result: ok. N passed; 0 failed` lines

---

## Dependencies & Execution Order

```
Phase 1 (Setup)
   │
Phase 2 (Foundational) ── T005/T006 measurements gate T007's defaults
   │
Phase 3 (US3 safety) ◄── MUST precede US1: the gate makes evaluating permissible
   │
Phase 4 (US1 resolution) ── the MVP completes here, with US3
   │
   ├── Phase 5 (US2 degradation)  ─┐
   ├── Phase 6 (US4 platform)      ├── independent of each other
   └── Phase 7 (US5 divergence)   ─┘
   │
Phase 8 (Polish)
```

**Hard dependencies**

- T007 depends on T005 **and** T006 — the timeout default and SC-007's ratio are
  measurements, not choices.
- Phase 4 depends on Phase 3. This is the one place the spec's equal P1
  priorities do not imply free ordering.
- T025 (attribute-path strategy) depends on T005 — the strategy is chosen by
  that measurement, not in advance.
- T042/T043 emit C180/C178 inside **Phase 5**, not the annotation phase,
  because US2's own acceptance test reads the degradation reason out of the
  document. Phasing them later would leave a P1 story untestable until P2
  work landed.
- T053 and T055 must land together; either alone fails the parity gate.
- T058 depends on T057 producing evidence of *legitimate* churn. Unexplained
  churn is a defect to investigate, not goldens to refresh.

**Parallel opportunities**

- Phase 2: T005, T006, T008, T009, T010, T012 — different files, no shared state.
- Phase 3: T013, T014, T016 (T015 amends T014, so it follows).
- Phases 5, 6 and 7 can proceed concurrently once Phase 4 lands.
- Phase 8: T055, T056, T060.

---

## Implementation Strategy

**MVP = Phase 1 + 2 + 3 + 4.** That is US3 and US1: the tier resolves correct
versions *and* cannot run repository code. US1 alone is not a shippable
increment, which is why the phases are ordered against the spec's priority
labels here.

**Second increment**: Phase 5 (US2). Until the degradation matrix is complete the
flag is not safe to set unconditionally in CI, which is the main way it will be
used.

**Third**: Phases 6 and 7, in either order.

**Do not start Phase 8's golden regeneration** until Phase 5 is done — every
degradation path that still fails a scan would otherwise churn goldens for a
reason that is about to change.

---

## Notes

- `xtask/src/nix_oracle/mod.rs` is **used** (T022) but **not copied** (T032). It
  passes `--impure` and does not disable IFD; that is acceptable for a
  developer-run review tool and unacceptable for a scan path.
- Test modules using `.unwrap()` need
  `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]` — clippy's
  `--all-targets` enforces the crate-root deny inside tests too.
- Any test that mutates `PATH`, `NIX_REMOTE` or any other environment variable
  routes through `crate::testing::EnvGuard::acquire()`, or it will race with the
  podman and cargo-metadata suites that already do.
