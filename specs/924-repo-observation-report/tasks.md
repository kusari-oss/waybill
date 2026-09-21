# Tasks: Repo Observation Report

**Input**: Design documents from `/specs/924-repo-observation-report/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/report-schema.md
**Issue**: #932 · **Branch**: `924-repo-observation-report`

**Tests**: Included. The spec's success criteria are verification-shaped
(SC-003 reconciliation, SC-005 determinism, SC-006 schema conformance,
SC-009 SBOM byte-identity), and Constitution Principle VIII plus the mandatory
pre-PR gate make them non-optional here.

**Organization**: Grouped by user story. US1 alone is a shippable MVP; each
later story enriches a report that already exists rather than completing a
partial one.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelisable — different files, no dependency on incomplete work
- **[Story]**: US1–US4, on user-story phases only

---

## Phase 1: Setup

**Purpose**: Establish the baseline that FR-024 / SC-009 are measured against, before any code changes.

- [ ] T001 Capture the pre-change SBOM baseline: run `waybill sbom scan` against this repository and the polyglot reference repo in all three formats, storing outputs under `specs/924-repo-observation-report/measurements/baseline/`. SC-009 asserts byte-identity against these, and they cannot be reconstructed after the walker is touched.
- [ ] T002 Create the module skeleton at `waybill-cli/src/report/mod.rs` with `census`, `significance`, `content_kind`, `ecosystems`, `schema` submodules declared and empty, wired into `waybill-cli/src/lib.rs` (or `main.rs` module tree) so later tasks compile in isolation.
- [ ] T003 [P] Record in `specs/924-repo-observation-report/measurements/baseline/README.md` exactly which binary and commit produced the T001 baselines, and the cache state of every enrichment source at capture time. A baseline whose conditions are unstated is not a baseline.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Retain the signal the walker already computes. Everything else depends on this and nothing else does.

⚠️ **No user story can start until T004–T007 are complete.**

- [ ] T004 In `waybill-cli/src/scan_fs/walk_registry/walker.rs`, retain the existing `dispatched_to` value (already bound at line ~264) and route it to a census sink alongside the existing `self.metrics.tick_file(&dispatched_to)` call. **Add no traversal and change no dispatch behaviour** — this file is on the hot path of every scan and carries byte-identity guarantees for 21 migrated readers.
- [ ] T005 Add the census accumulator in `waybill-cli/src/report/census.rs`: per-directory claimed/unclaimed/skipped tallies keyed by canonical path, plus per-reader `files_matched`. Accumulation only — no classification, no policy.
- [ ] T006 [P] Define the report root types in `waybill-cli/src/report/schema.rs` per `data-model.md`: `ObservationReport`, `RepositoryTotals`, `ReaderCoverage`, with serde derives and the two-part `schema_version`.
- [ ] T007 Prove T004 changed nothing observable: re-run the T001 scans and assert byte-identity against the stored baselines (SC-009). **Run this before building anything on top** — a regression introduced here would otherwise be discovered several stories later, with the cause buried.

---

## Phase 3: User Story 1 — The census (Priority: P1) 🎯 MVP

**Goal**: Report which readers claimed what, per directory, with totals that reconcile exactly.

**Independent test**: Run against a repository mixing supported and unsupported project types; the report names claimed directories with their readers, lists unclaimed ones, and its totals reconcile.

- [ ] T008 [P] [US1] Write the reconciliation test in `waybill-cli/tests/repo_report_census.rs` asserting FR-003 / SC-003: `files_walked == files_claimed + files_unclaimed + Σ files_skipped`, on a fixture with all three categories present. **This test must fail before T010 exists.**
- [ ] T009 [P] [US1] Write the FR-004 test in `waybill-cli/tests/repo_report_census.rs` asserting that a reader which matched files but emitted zero components is reported distinctly from one that never matched. Build a fixture containing a deliberately malformed manifest of a supported ecosystem — matched-but-yielded-nothing is unobservable without one.
- [ ] T010 [US1] Implement the significance decision in `waybill-cli/src/report/significance.rs` per FR-021a: record if marker-bearing, claimed, a scan/exclusion boundary, or unclaimed and over the threshold; otherwise aggregate into the nearest recorded ancestor. Default threshold **25** (research R3).
- [ ] T011 [US1] Implement aggregation in `waybill-cli/src/report/significance.rs` per FR-021b so an aggregated directory's counts roll into its nearest recorded ancestor, keeping `files_direct` and `files_aggregated` distinct per `data-model.md`.
- [ ] T012 [US1] Assemble `DirectoryObservation` records and `RepositoryTotals` in `waybill-cli/src/report/mod.rs`, emitting `claim_status` as the exclusive enum of FR-012a and `claimed_by` non-empty exactly when claimed.
- [ ] T013 [US1] Add the `waybill repo report` subcommand in `waybill-cli/src/cli/report_cmd.rs`: `--path`, `--output`. Per FR-022a it runs walk + readers + resolution and **never** invokes enrichment or emission; per R5 resolution is constructed offline with no operator-facing switch.
- [ ] T014 [US1] Emit `significance_threshold` into the report (FR-021c). Two reports produced under different thresholds are not comparable, and a consumer must never have to guess which applied.
- [ ] T015 [P] [US1] Write the aggregation test in `waybill-cli/tests/repo_report_census.rs` for SC-013: a deep tree of unmarked, unclaimed, undersized directories produces no records of its own, yet totals still reconcile — aggregation loses records without losing counts.
- [ ] T016 [US1] Verify SC-007 structurally: run the subcommand with network access denied and **no** offline flag passed, and assert success. This guards a hard-coded value, not an absence — see the FR-022b qualification in `plan.md`.

**Checkpoint**: US1 is independently shippable. The census answers the feature's primary goal without US2–US4.

---

## Phase 4: User Story 2 — Naming the unknown (Priority: P2)

**Goal**: Convert "unknown directory" into "this is Deno, and waybill has no reader for it".

**Independent test**: Place markers for several unsupported ecosystems in separate directories; each is named with a no-reader status, and a directory with no recognised marker is reported unrecognised rather than mislabelled.

- [ ] T017 [P] [US2] Create the marker table at `waybill-cli/src/report/ecosystems.data` seeded from research R6 — `deno.json`, `Pipfile`, `pixi.toml`, `build.zig`, `shard.yml`, `Project.toml`, `nimble.toml`, `dune-project` — as data, not code (FR-009).
- [ ] T018 [US2] **Write the anti-staleness test** in `waybill-cli/tests/repo_report_ecosystems.rs`: assert no entry in `ecosystems.data` matches any pattern in the live reader registry. This table is exactly the shape that rots — a reader lands and the table keeps announcing the ecosystem unsupported — and this project has already shipped that bug class undetected for roughly two years. The test must fail if an entry is added for an ecosystem that already has a reader.
- [ ] T019 [US2] Implement marker lookup in `waybill-cli/src/report/ecosystems.rs` producing `EcosystemAttribution` records with `evidence_marker` and `support` per `data-model.md`.
- [ ] T020 [P] [US2] Write the FR-008 negative test in `waybill-cli/tests/repo_report_ecosystems.rs`: a directory containing only source files of an unsupported language, with **no** marker, is reported unrecognised and is **not** assigned an ecosystem. Extensions must not produce attribution.
- [ ] T021 [US2] Distinguish supported-but-yielded-nothing from unsupported in the emitted record (spec US2 scenario 2) — the reader exists and produced nothing is a different problem from no reader existing, and the report must not merge them.

**Checkpoint**: unclaimed directories are now actionable where a marker exists.

---

## Phase 5: User Story 3 — Typed uncertainty (Priority: P3)

**Goal**: Record ambiguity with its evidence instead of guessing.

**Independent test**: Run against this repository; `waybill-cli/tests/` is reported claimed **and** ambiguous, naming the competing interpretations.

- [ ] T022 [P] [US3] Implement content-kind sampling in `waybill-cli/src/report/content_kind.rs` per research R4: NUL byte in the first 8 KiB ⇒ binary, else valid UTF-8 ⇒ text, else binary. Emit `content_sample_bytes` so a reader knows the verdict came from a sample.
- [ ] T023 [US3] Implement `DirectoryObservationDetail` assembly in `waybill-cli/src/report/mod.rs` per FR-011: file count, max depth, extension histogram, content kind — for every directory not confidently classified.
- [ ] T024 [US3] Implement `AmbiguityRecord` in `waybill-cli/src/report/mod.rs` per FR-012b, independent of `claim_status`, with `interpretations` (≥2, never ranked) and `evidence` (FR-015).
- [ ] T025 [US3] Detect the multi-ecosystem-lockfile case (FR-013) and emit an ambiguity record listing every ecosystem observed, selecting none as authoritative.
- [ ] T026 [US3] **Write the SC-001 self-test** in `waybill-cli/tests/repo_report_ambiguity.rs` against this repository: `waybill-cli/tests/` must carry a claim status reflecting that readers matched its lockfiles **and** an ambiguity record. Assert both. A report showing `claimed` with no ambiguity means the two-field model has collapsed back into a single verdict — the exact defect the clarification session removed.
- [ ] T027 [P] [US3] Write the content-kind discrimination test in `waybill-cli/tests/repo_report_ambiguity.rs`: two fixture directories of equal file count, one all binary and one all text, produce different `content_kind` values. This is the field that makes an unclassified directory actionable.
- [ ] T028 [US3] Assert FR-014 in `waybill-cli/tests/repo_report_ambiguity.rs`: no directory carrying an ambiguity record also carries a single authoritative ecosystem attribution — the report never resolves ambiguity by preference.

**Checkpoint**: ambiguity is now first-class and self-tested against a real instance.

---

## Phase 6: User Story 4 — Schema, redaction, determinism (Priority: P4)

**Goal**: Make the report safe to share and stable enough to diff across submissions.

**Independent test**: A report for a repository with sensitive-looking names contains no absolute paths and no file contents, and validates against its published schema.

- [ ] T029 [P] [US4] Publish the JSON Schema at `waybill-cli/src/report/schema/observation-report.schema.json` covering every field in `data-model.md`, with `schema_stability: alpha` and the two-part version (FR-016 / FR-017a).
- [ ] T030 [US4] Write the schema-conformance test in `waybill-cli/tests/repo_report_schema.rs` validating emitted reports with the existing `jsonschema` dev-dep (SC-006).
- [ ] T031 [US4] **Prove T030 has teeth**: temporarily emit a field absent from the schema, confirm the test fails, restore, confirm it passes. Record both outcomes in `specs/924-repo-observation-report/measurements/verification.md`. A schema gate that stubs `$ref` resolution validates nothing while appearing green — this project has hit that exact failure before, so a passing test is not evidence until the failing direction is observed.
- [ ] T032 [US4] Implement the `--redact` mode in `waybill-cli/src/report/mod.rs` per FR-019b: each path segment replaced by a stable identifier, identical segments mapping identically within a report so nesting and repetition survive.
- [ ] T033 [US4] Emit `redaction_mode` unconditionally (FR-019c) and surface the stricter mode in the command's own output (FR-019d) — an operator must learn it exists at the moment they are about to share, not from documentation they read afterwards.
- [ ] T034 [P] [US4] Write the redaction test in `waybill-cli/tests/repo_report_schema.rs` for SC-011: assert mechanically that no original directory name from the fixture appears anywhere in a redacted report, and that two directories sharing a segment still share an identifier.
- [ ] T035 [P] [US4] Write the FR-019 unconditional test in `waybill-cli/tests/repo_report_schema.rs` for SC-004: **in both modes**, zero absolute paths and zero bytes excerpted from any scanned file.
- [ ] T036 [US4] Emit `volatile_fields` as a self-describing list (FR-020 / C-5) so a differ needs no out-of-band knowledge and stays correct as the schema grows.
- [ ] T037 [US4] Write the determinism test in `waybill-cli/tests/repo_report_schema.rs` for SC-005: two consecutive runs are byte-identical after masking exactly the fields `volatile_fields` names — read from the document, not hard-coded in the test.
- [ ] T038 [P] [US4] Write the unknown-enum-member test in `waybill-cli/tests/repo_report_schema.rs` for SC-015 / FR-017c: a consumer meeting an unknown enumeration member preserves and surfaces it rather than coercing or dropping it; a consumer meeting an unrecognised major refuses the report.

**Checkpoint**: the report is shareable, versioned, and diffable.

---

## Phase 7: Polish & Cross-Cutting

- [ ] T039 Measure the report against real repositories and record in `specs/924-repo-observation-report/measurements/after.md`: `directories_recorded` versus `directories_walked` for this repository, the polyglot reference repo, and at least three corpus targets. **Validate the threshold-25 choice from research R3 against the shipped implementation** — R3 measured a proxy, not the real significance rule, and the two can disagree.
- [ ] T040 Confirm SC-008 empirically from T039's numbers: a repository an order of magnitude larger does not produce an order of magnitude more records. If it does, the threshold or the significance rule is wrong — say so rather than adjusting the criterion.
- [ ] T041 [P] Document the subcommand in `docs/user-guide/`, covering what the report answers, the redaction trade, and that `significance_threshold` must match before two reports are compared.
- [ ] T042 [P] Write the CHANGELOG entry. Lead with what an operator gets — a readable account of what waybill did and did not understand — and state plainly that emitted SBOM content is unchanged.
- [ ] T043 Re-run T007's byte-identity check at final state (SC-009): emitted SBOM content byte-identical to the T001 baselines across all three formats.
- [ ] T044 Confirm corpus goldens are untouched. Expect **no movement** — this feature emits no SBOM content. Movement means FR-024 has been violated somewhere.
- [ ] T045 Run the mandatory pre-PR gate: `./scripts/pre-pr.sh`. Enumerate the per-target results rather than citing the exit code; use `-j 2 --test-threads=2` if the full run exhausts memory.
- [ ] T046 Resolve the FR-022b qualification raised in `plan.md`: either amend the spec's "structural rather than flag-dependent" wording to match what R5 found, or record why the stronger wording stands. **Do not leave the spec claiming a guarantee the implementation does not provide.**
- [ ] T047 Comment on #932 with what landed, the T039 measurements, and the schema version shipped.

---

## Dependencies & Execution Order

```
Phase 1 (Setup)          T001 → T002, T003
                              ↓
Phase 2 (Foundational)   T004 → T005 → T006 → T007     ⚠️ BLOCKS ALL STORIES
                              ↓
Phase 3 (US1, P1)        T008,T009 [P] → T010 → T011 → T012 → T013 → T014 → T015,T016
                              ↓                                    ← MVP ships here
Phase 4 (US2, P2)        T017 [P] → T018 → T019 → T020,T021
                              ↓
Phase 5 (US3, P3)        T022 [P] → T023 → T024 → T025 → T026 → T027,T028
                              ↓
Phase 6 (US4, P4)        T029 [P] → T030 → T031 → T032 → T033 → T034..T038
                              ↓
Phase 7 (Polish)         T039 → T040 → T041..T047
```

**Story independence**: US2, US3 and US4 each depend only on US1's census existing. They do not depend on each other and could be reordered or dropped without invalidating what shipped before them.

### Parallel opportunities

- **Phase 1**: T003 alongside T002.
- **Phase 2**: T006 alongside T005 — different files.
- **US1**: T008 and T009 together (both tests, same file but independent fns — coordinate or split); T015 and T016 together.
- **US2**: T017 is standalone data; T020 parallel with T021.
- **US3**: T022 is a leaf module, parallel with everything until T023; T027 parallel with T028.
- **US4**: T029 standalone; T034, T035 and T038 all parallel once T032/T033 land.
- **Phase 7**: T041 and T042 together.

---

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (T001–T016).** That delivers a census
that reconciles, which answers the feature's primary goal. Stop there and the
feature is useful; every later phase enriches a working report rather than
completing a broken one.

**Highest-risk tasks, front-loaded deliberately**:

- **T004** touches the hot path of every scan. T007 exists solely to catch a
  regression there before anything is built on top of it.
- **T018** and **T031** are guards against two failure modes this project has
  already experienced — a data table that silently rots, and a schema gate
  that validates nothing while appearing green. Both are written to fail
  first; a passing gate is not evidence until the failing direction has been
  observed.
- **T039** re-validates the threshold against the shipped implementation
  rather than trusting research R3, which measured a proxy for the
  significance rule rather than the rule itself.

**T046 is not optional bookkeeping.** The plan found that FR-022b claims a
stronger guarantee than the implementation provides. Shipping with that
unresolved leaves a spec asserting something untrue, which is the failure this
milestone's predecessor spent considerable effort correcting.
