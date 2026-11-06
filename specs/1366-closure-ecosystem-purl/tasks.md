---
description: "Task list for milestone 1366 — an ecosystem identity for closure components"
---

# Tasks: an ecosystem identity for the closure components a feed could match

**Input**: Design documents from `/specs/1366-closure-ecosystem-purl/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/annotations.md`, `quickstart.md`
**Issue**: #1365 (spec number ≠ issue number)

**Tests**: **Required, not optional.** SC-007 states *"Disabling the feature makes a test fail"* and SC-002/SC-003/SC-004/SC-006 are each assertions about emitted documents. Test tasks below are part of the deliverable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: `[US1]` / `[US2]` / `[US3]` — user-story phases only

## Path conventions

Single Rust workspace. Production code under `waybill-cli/src/`, integration tests under `waybill-cli/tests/`, catalog in `docs/reference/sbom-format-mapping.md`.

---

## Phase 1: Setup — measure before building

**Purpose**: establish the baselines every later claim is quoted against, and settle the one open empirical question **before** any code depends on the answer.

> Per CLAUDE.md: *"any number in a spec, plan, contract or success criterion that describes external behaviour must be traceable to an observation."* Phase 1 produces those observations.

- [X] T001 Create `specs/1366-closure-ecosystem-purl/measurements/` and add a `README.md` recording the host (aarch64-darwin), the pinned nixpkgs revision (`a799d3e3`), the moat commit (`d069055`), and the explicit warning that these figures are **not** comparable to the x86_64-linux corpus goldens
- [X] T002 Capture the pinned closure once to the session scratchpad via `nix derivation show --recursive <moat>#default`, and record its derivation count in `measurements/README.md` (expected 1,275 — if it differs, stop and reconcile before continuing)
- [X] T003 [P] **Settle research R8 empirically.** Extend `specs/1276-boot-library-scope/measurements/pypi-identity-probe.py` to group candidate names by their PEP 503 normalised form and report any group of size > 1; run it against T002's closure and record the result in `measurements/README.md`. A non-empty result makes validation rule V5 load-bearing rather than defensive
- [X] T004 [P] Capture the pre-change baseline documents for the moat closure (all three formats) into the scratchpad, for the FR-005/FR-006 byte-identity diffs in T038–T039

**Checkpoint**: the candidate population, the collision answer, and the before-state are all recorded facts.

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: the candidate rule, the entity types, and the one new seam onto the enrichment path. No user story can begin until these exist.

**⚠️ CRITICAL**: blocks US1, US2 and US3.

- [X] T005 Create `waybill-cli/src/scan_fs/package_db/nix/closure/ecosystem_identity.rs` and register it in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs`
- [X] T006 Define `EcosystemCandidate` and `CandidateEvidence` (single variant `VersionedInterpreterPrefix`) in `ecosystem_identity.rs` per `data-model.md` §1
- [X] T007 Define `ConfirmationOutcome` and `UncheckedReason` in `ecosystem_identity.rs` per `data-model.md` §2, as a **total** mapping from `LookupResult` with no default arm — adding a `LookupResult` variant must break the build
- [X] T008 Define `ConfirmationRecord` in `ecosystem_identity.rs` per `data-model.md` §3, with the `confirmed + declined + unchecked == candidates` invariant asserted in code
- [X] T009 Implement `fn candidates(...)` in `ecosystem_identity.rs`: derivation `name` matches `^python3\.\d+-` **and** `pname` does not itself begin `python3`. Read `pname`/`version` through the reader's existing `attr_str`, which consults **both** the flat `env` map and top-level `structuredAttrs` (research R3 — 66 of 1,275 moat derivations use only the latter)
- [X] T010 Implement validation rules V1–V5 in `ecosystem_identity.rs`: V1 non-empty `pname`; V2 keyable version (else `Unchecked{Unkeyable}`, never a silent drop); V3 the component carries **no** `waybill:closure-member-role` (C206); V4 the component was **appended**, not merged; V5 normalised-coordinate uniqueness, declining **every** member of a colliding group
- [X] T011 [P] Reuse `pip::normalize_pypi_name_for_purl` (`waybill-cli/src/scan_fs/package_db/pip/mod.rs:99`) for PEP 503 normalisation in `ecosystem_identity.rs` — do **not** write a third implementation of this rule
- [X] T012 Add a `pub(crate)` confirmation entry point to `waybill-cli/src/enrich/depsdev_source.rs` that takes `&[EnrichmentKey]` and returns the per-key `LookupResult`s by routing through the existing private `fetch_results`, so the in-memory cache, the disk cache and the batch path are all inherited (FR-011)
- [X] T013 [P] Unit-test the candidate rule in `ecosystem_identity.rs` against a synthetic closure covering: a versioned-prefix package, a `python-*` setup hook (must be excluded), a `python3.13-bootstrap-*` variant (must be excluded), a `structuredAttrs`-only `pname`, and a member carrying a C206 role (must be excluded)
- [X] T014 Create the synthetic fixture `waybill-cli/tests/fixtures/nix_closure_ecosystem/closure.json` using `waybill-fixture-*` names only — never a real coordinate (`feedback_fixture_synthetic_package_names`; real names tripped the advisory scan in PR #285 and #640)

**Checkpoint**: candidates can be derived and validated offline, with no network and no identifier change yet.

---

## Phase 3: User Story 2 — an unconfirmed coordinate is never invented (Priority: P1)

**Goal**: the feature starts in its safe state. Every candidate keeps today's identifier until something confirms it.

**Why this phase comes first despite US1 sharing its priority**: US2 is satisfied by the *absence* of a change. Implementing it first means there is never a window in which candidates are rewritten unconditionally. The reverse order creates that window and relies on a later task to close it.

**Independent Test**: present the pass with candidates that cannot be confirmed and assert the emitted document is byte-identical to today's.

- [X] T015 [US2] Implement `fn apply(...)` in `ecosystem_identity.rs` so that it derives candidates, assigns **every** outcome as `Unchecked`, mutates nothing, and returns a populated `ConfirmationRecord` — the complete US2 behaviour, with confirmation not yet wired
- [X] T016 [US2] Invoke `apply` from `waybill-cli/src/cli/scan_cmd.rs` in the research-R11 window: after the milestone-1275 Hackage identity pass (~line 4760) and **strictly before** `closure::graph::relationships` (~line 4833)
- [X] T017 [US2] Gate the pass on the deps.dev flags in `scan_cmd.rs`: `--no-deps-dev` / `--no-deps-dev-license` yield `Unchecked{Disabled}` for every candidate (research R7 — this is the corpus path, and it must be the *documented* path, not an accident)
- [X] T018 [P] [US2] Test in `waybill-cli/tests/closure_ecosystem_identity.rs`: a closure of only unconfirmable candidates emits a document byte-identical to the pre-change baseline except for the C208 record (spec US2 acceptance scenario 2)
- [X] T019 [P] [US2] Test in `waybill-cli/tests/closure_ecosystem_identity.rs`: a component the pass declined carries exactly its prior identifier and **no** C207 (spec US2 acceptance scenario 1)

**Checkpoint**: the pass runs end to end, changes nothing, and records that it changed nothing. Safe to merge on its own.

---

## Phase 4: User Story 1 — a consumer matches the closure's Python packages against a feed (Priority: P1) 🎯 MVP

**Goal**: a confirmed candidate carries an ecosystem-qualified identifier a vulnerability feed recognises.

**Independent Test**: stub deps.dev, scan a synthetic closure, assert the confirmed component's identifier and that the declined one is untouched.

> **The corpus cannot witness this story** (research R7). The one corpus target that runs a closure passes `--no-deps-dev`. T020–T022 are the *only* place confirmation actually fires; a green corpus lane is not evidence this story works.

### Tests for User Story 1 (write first; they must fail before T023)

- [X] T020 [US1] Stand up a wiremock deps.dev stub in `waybill-cli/tests/closure_ecosystem_identity.rs` using the existing `wiremock = "0.6"` dev-dependency and `DepsDevClient::with_base_url` (`deps_dev_client.rs:137`): one arm answers 200 with a `VersionInfo`, one answers **404**
- [X] T021 [P] [US1] Test in `waybill-cli/tests/closure_ecosystem_identity.rs`: the 200-arm candidate's identifier becomes `pkg:pypi/<pep503-name>@<version>` and the component's `name` and `version` fields are unchanged (spec US1 acceptance scenarios 1 and 2)
- [X] T022 [P] [US1] Test in `waybill-cli/tests/closure_ecosystem_identity.rs`: the **404**-arm candidate is `Declined` and keeps today's identifier — the only place FR-003's "a 404 is a decline, not a failure" distinction is exercised

### Implementation for User Story 1

- [X] T023 [US1] Wire confirmation into `apply` in `ecosystem_identity.rs`: build `EnrichmentKey`s, call T012's entry point, map `LookupResult` → `ConfirmationOutcome` per `data-model.md` §2
- [X] T024 [US1] Rewrite the identifier on `Confirmed` only in `waybill-cli/src/scan_fs/package_db/nix/closure/ecosystem_identity.rs`, constructing it through `waybill_common::types::purl::Purl::new` so an unparseable coordinate declines the candidate rather than reaching output (Constitution Principle V)
- [X] T025 [US1] Map `Failed` → `Unchecked{Unreachable}` and `deps_dev_client::Throttled` → `Unchecked{Throttled}` in `ecosystem_identity.rs`, and confirm by inspection that `apply` returns a record rather than a `Result` the caller can `?` on (FR-012: confirmation cannot fail a scan)
- [X] T026 [P] [US1] Test in `waybill-cli/tests/closure_ecosystem_identity.rs`: PEP 503 normalisation — a candidate whose `pname` differs from its published name by case or separator is identified under the normalised form (spec US1 acceptance scenario 3)
- [X] T027 [P] [US1] Test in `waybill-cli/tests/closure_ecosystem_identity.rs`: a transport failure and a throttle each leave every identifier unchanged and the scan exits successfully (FR-009, FR-012)
- [X] T028 [US1] **Mutation check for SC-007.** Make confirmation unconditionally decline, run the suite, and record in `measurements/README.md` which tests went red and how many. A test that cannot fail is not evidence (`feedback_a_passing_test_may_exercise_nothing`)

**Checkpoint**: US1 and US2 both hold. This is the MVP — the document now answers a vulnerability query for confirmed Python packages.

---

## Phase 5: User Story 3 — a reader can tell which identities were upgraded (Priority: P2)

**Goal**: the derived identity and its basis are visible in all three formats, and the document-scope record distinguishes "no candidates" from "confirmation did nothing".

**Independent Test**: scan the pinned closure and confirm every re-identified component carries the disclosure and components left generic do not.

- [X] T029 [US3] Write the C207 value into `component.extra_annotations` from the pass in `waybill-cli/src/scan_fs/package_db/nix/closure/ecosystem_identity.rs`. **This alone gives both SPDX formats**: the bag is carried generically by `waybill-cli/src/generate/spdx/annotations.rs:414` and `waybill-cli/src/generate/spdx/v3_annotations.rs:437`, so neither SPDX emitter needs new code
- [X] T030 [US3] Suppress the generic property emission for the C207 key on the CDX path in `waybill-cli/src/generate/cyclonedx/builder.rs:1877`, so the signal has exactly one home per format. Emitting both the property and the native field would break SymmetricEqual byte-identity — the hazard `evidence.rs:57` already records for C56: *"Callers MUST NOT also emit a `waybill:also-detected-via` component property"*
- [X] T031 [US3] Emit C207 natively on the CDX path in `waybill-cli/src/generate/cyclonedx/evidence.rs`: an `evidence.identity[]` entry with `field: "purl"`, `confidence: 0.8`, `concludedValue`, and a `methods[]` entry carrying `technique: "manifest-analysis"` plus the extension key `waybill-closure-ecosystem-evidence`, following the milestone-105 `waybill-source-mechanism` precedent. Lands with T030 or parity breaks
- [X] T032 [US3] Emit C208 at document scope in all three formats — CDX `metadata.properties[]`, SPDX 2.3 and SPDX 3 document-scope annotation — per `contracts/annotations.md` §3, threading the value from `scan_cmd.rs` through the emitter arguments the way `nix_closure_degraded` (C190) already is. Present wherever C204 is present; absent otherwise
- [X] T033 [US3] Add the C207 and C208 rows to `docs/reference/sbom-format-mapping.md`, each carrying the Principle V justification clause from `research.md` R5 — including why this audit adopts `evidence.identity[]` where C201's audit rejected it
- [X] T034 [US3] Add the six extractor functions and the two `EXTRACTORS` rows (`Directionality::SymmetricEqual`, `order_sensitive: false`) in `waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs`. **Same change as T033** — a catalog row without an extractor fails `every_catalog_row_has_an_extractor` and `holistic_parity`
- [X] T035 [P] [US3] Test in `waybill-cli/tests/closure_ecosystem_identity.rs`: a confirmed component carries C207 in all three formats and a declined one carries none (spec US3 acceptance scenarios 1 and 2)
- [X] T036 [P] [US3] Test in `waybill-cli/tests/closure_ecosystem_identity.rs`: C208 distinguishes three documents — zero candidates (no `reason`), candidates all declined, candidates all unchecked with `reason: "disabled"` (FR-008; `feedback_zero_results_can_mean_broken`)

**Checkpoint**: all three stories independently functional and visible in every format.

---

## Phase 6: Verification, measurement and goldens

**Purpose**: the claims the spec makes about the real closure, established by running them rather than by reasoning about them.

### Ordering and graph safety

- [X] T037 Test in `waybill-cli/tests/closure_ecosystem_identity.rs` that every `dependsOn` endpoint resolves to an emitted component after confirmation, and that the orphan count does not rise — the regression research R11 identifies, where a mis-ordered pass orphans every component the feature fixes

### Non-regression (FR-005, FR-006)

- [X] T038 [P] Assert the 539 closure members are byte-identical before and after, by diffing the C206-carrying components' `{name, version, purl}` against T004's baseline; record the diff in `specs/1366-closure-ecosystem-purl/measurements/README.md` (SC-003)
- [X] T039 [P] Assert components contributed by other readers are byte-identical before and after, against T004's baseline; record the diff in `specs/1366-closure-ecosystem-purl/measurements/README.md` (SC-004)

### The measured numbers

- [X] T040 **Produce SC-001's number.** Run the real scan from `quickstart.md` Control 2 with deps.dev enabled, on the pinned closure, and record candidates / confirmed / declined with the exact command in `measurements/README.md`. Quote this figure and no other; the spec forbids predicting it
- [X] T041 Demonstrate SC-005 end to end on the five names with known vulnerability histories — `urllib3`, `requests`, `lxml`, `jinja2`, `certifi` — by querying the emitted document by ecosystem-qualified identifier and recording the result in `specs/1366-closure-ecosystem-purl/measurements/README.md`
- [X] T042 Verify SC-006 by scanning with confirmation unavailable, confirming the document is byte-identical to today's apart from C208 — which states that confirmation did not run — and recording the comparison in `specs/1366-closure-ecosystem-purl/measurements/README.md`

### Goldens and the gate

- [X] T043 Refresh the `nix-closure-moat` goldens through the `public-corpus.yml` workflow dispatch — **never** locally; `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is never set on this machine. Expect exactly one change: the added C208 record saying confirmation did not run
- [X] T044 Confirm SC-008 by enumerating the other 16 corpus targets' goldens under `waybill-cli/tests/fixtures/public_corpus/` as unchanged, reading each target's own result rather than the run's summary field
- [X] T045 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` argument (#1189). Assert on the positive signal and `GATE_RESULT=pass` in the log, never on a pipeline's exit status. Watch the bin/lib split: `scan_fs` is in the **bin** target, `parity` in the **lib**, and `cargo test --lib <filter>` over a bin test reports `0 tests; N filtered out`, which looks exactly like a pass
- [X] T046 [P] Update `docs/ecosystems.md` and the nix closure section of the user-facing docs to state what a `pkg:pypi` identifier on a closure component means — existence confirmed, derivation identity not proven (research R9)

---

## Dependencies & execution order

### Phase dependencies

- **Phase 1 (Setup)**: no dependencies. T003's answer determines whether V5 is load-bearing or defensive — but V5 is implemented either way.
- **Phase 2 (Foundational)**: depends on Phase 1. **Blocks every user story.**
- **Phase 3 (US2)**: depends on Phase 2. Independently mergeable.
- **Phase 4 (US1)**: depends on Phase 2 and on T015–T017 from Phase 3 (US1 opens up the pass that US2 establishes in its closed state).
- **Phase 5 (US3)**: depends on Phase 2; the C207 tests need US1's confirmation to produce a confirmed component, so T035 depends on T024.
- **Phase 6**: depends on all three stories.

### Story dependencies

- **US2** → none beyond Foundational. The one story that is complete on its own.
- **US1** → US2's `apply` skeleton and call site (T015–T017).
- **US3** → US1's confirmation for its positive cases; its negative cases (T036) need only US2.

### Within each story

- Tests before implementation in Phase 4 (T020–T022 before T023).
- Entities before the pass; the pass before its call site; the call site before the emitters.
- T033 and T034 land together or CI rejects both.

### Parallel opportunities

- T003 and T004 (Phase 1)
- T011, T013, T014 (Phase 2, after T005–T010)
- T018, T019 (Phase 3)
- T021, T022, then T026, T027 (Phase 4)
- T035, T036 (Phase 5). T030 and T031 are one logical change in two files — land them together
- T038, T039 and T046 (Phase 6)

---

## Parallel example: Phase 5

```bash
# T030 and T031 are one CDX change in two files and land together.
# The two disclosure tests are the parallel pair:
Task: "T035 confirmed carries C207 in all three formats, declined carries none"
Task: "T036 C208 distinguishes zero-candidate, all-declined and all-unchecked"
```

---

## Implementation strategy

### MVP (US2 + US1)

1. Phase 1 → the baselines and the collision answer exist
2. Phase 2 → candidates derivable offline
3. Phase 3 (US2) → the pass runs and changes nothing. **Mergeable alone**
4. Phase 4 (US1) → confirmation opens it up; T028's mutation check proves the tests have teeth
5. **STOP and VALIDATE**: T040's measured number is the first honest statement of what the feature achieved

### Incremental delivery

Each phase boundary is a mergeable state. Phase 3 alone is a no-op that records what it would have done — which is a genuinely useful artifact, because it makes the candidate population visible in the document before anything acts on it.

---

## Notes

- **The corpus is blind to this feature.** Say so in the PR body rather than letting a green lane imply coverage.
- **Figures are per-closure and per-system.** Phase 1 and Phase 6 measurements are aarch64-darwin; the corpus goldens pin x86_64-linux, a *different closure*. Quoting one against the other is wrong in membership.
- **Never evaluate a fixture flake in place** — it rewrote a committed `flake.lock` and broke 11 tests on 2026-11-04. Copy the clone and verify with `md5 -q`.
- Test modules using `.unwrap()` need `#[cfg_attr(test, allow(clippy::unwrap_used))]` on the `mod tests` item — clippy's `--all-targets` enforces the crate-root deny inside `#[cfg(test)]` too.
- Commit after each task or logical group. `Assisted-by:` trailer, never `Co-Authored-By:`. No session URLs in any public artifact.
