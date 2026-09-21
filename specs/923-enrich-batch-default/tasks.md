# Tasks: Enrichment is fast by default

**Input**: Design documents from `/specs/923-enrich-batch-default/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/enrichment-default.md, quickstart.md
**Issue**: [#927](https://github.com/kusari-oss/waybill/issues/927)

**Tests**: included. Two of the requirements here are invisible in output —
a circuit breaker that never trips and an opt-out that silently does nothing
both produce *correct documents*. Only a test that counts attempts or asserts
path selection can tell.

**Organization**: by user story. US1 and US2 are coupled by construction (you
cannot flip a default without providing the opt-out); US3 is independent but
is a **shipping** prerequisite — see Dependencies.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an incomplete task.
- Most of this feature lives in two files, so genuine parallelism is limited
  and the marker is used sparingly rather than decoratively.

## Path Conventions

Repository root. `waybill-cli/src/cli/scan_cmd.rs` (flag surface),
`waybill-cli/src/enrich/depsdev_source.rs` (path selection, circuit breaker,
tests), `waybill-cli/src/enrich/deps_dev_batch.rs` (endpoint constant).

---

## Phase 1: Setup

- [X] T001 Capture the pre-change baseline into `specs/923-enrich-batch-default/measurements/baseline.md`: wall clock and emitted document for all four flag combinations (`--offline`; online with deps.dev disabled; online default; online `--enrich-batch --enrich-no-cache`) against a repository of ~2,000 packages. **Record the deps.dev-disabled run explicitly** — it is what makes the attribution valid, and without it any later claim about enrichment cost is a flag-toggle inference rather than a measurement.
- [X] T002 Preserve the pre-change release binary for the byte-identity check in T009 and the teeth-check in T022, and note in `specs/923-enrich-batch-default/measurements/baseline.md` that a default scan currently uses the per-component path.

**Checkpoint**: the cost is on record, and so is the evidence that it is deps.dev's.

---

## Phase 2: Foundational — the equivalence guard

**Purpose**: FR-002 is what licenses the entire feature. If the two paths ever produce different documents, the faster one is not a valid default regardless of its speed. This lands **before** the default moves.

- [X] T003 Extend the `MockServer`-based tests in `waybill-cli/src/enrich/depsdev_source.rs` to assert **path equivalence** (FR-002 / C-2): the same input enriched via both paths yields identical package identities, identical licence values and identical edges. The helper `src(server, batch)` is already parameterised on the path, so this extends existing coverage rather than building a second harness.
- [X] T004 Confirm `batch_failure_falls_back_and_content_is_unchanged` (`depsdev_source.rs:1168`) still covers C-5, and note in the test what it protects: the fallback is the whole safety argument for the default, so this test failing means the default is no longer defensible — not merely that a test broke.

**Checkpoint**: the property that justifies the change is asserted, before the change.

---

## Phase 3: User Story 1 — a scan enriches at batch speed without being asked (P1)

**Goal**: the default path is the fast one.

**Independent test**: scan a ~2,000-package repository with no enrichment flags; enrichment completes in seconds with unchanged licence coverage.

- [X] T005 [US1] Invert the flag in `waybill-cli/src/cli/scan_cmd.rs`: batched becomes the default and a new opt-out selects the per-component path. Note that `#[arg(long)] pub enrich_batch: bool` is clap's *flag* form — absent means `false` — so this cannot be done by adding a `default_value`; the surface has to change shape (research R1).
- [X] T006 [US1] Keep the existing `--enrich-batch` accepted as a no-op in `waybill-cli/src/cli/scan_cmd.rs` (FR-004 / C-4). Scripts passing it today must keep working; erroring on an unknown argument would break them louder than ignoring it.
- [X] T007 [US1] Update the test-helper default at `waybill-cli/src/cli/scan_cmd.rs:6057` so helper-constructed args match the new production default. A helper still defaulting to the old path would make every test exercise the path users no longer get.
- [X] T008 [US1] Test in `waybill-cli/src/enrich/depsdev_source.rs` that a scan with no enrichment flags selects the batched path — **asserted as path selection, not as output**, since both paths produce the same document and an output assertion would pass whether or not the flip worked.
- [X] T009 [US1] Test in `waybill-cli/tests/enrich_default.rs` that an enrichment-disabled scan (`--offline`) is **byte-identical** to the T001 baseline (FR-008 / FR-009 / SC-004 / C-8). The default flip must not reach a scan that makes no network calls.

**Checkpoint**: the feature's user-visible promise is met.

---

## Phase 4: User Story 2 — the slow path stays reachable and exercised (P2)

**Goal**: the fallback the default depends on can be selected and does not rot.

**Independent test**: select the per-component path explicitly; content matches the batched path.

- [X] T010 [US2] Test in `waybill-cli/src/enrich/depsdev_source.rs` that the opt-out selects the per-component path, again asserted as selection rather than output.
- [X] T011 [US2] Test in `waybill-cli/tests/enrich_default.rs` that a scan passing the legacy `--enrich-batch` flag succeeds rather than erroring (SC-007).
- [X] T012 [US2] Make the both-paths coverage explicit in `waybill-cli/src/enrich/depsdev_source.rs` — every behavioural enrichment test runs against both selections, so neither can break silently. **The per-component path is the fallback the default's safety argument rests on**; if only the fast path is tested, that argument is untested.

**Checkpoint**: the escape hatch works and is guarded.

---

## Phase 5: User Story 3 — an operator can tell when the fast path stopped working (P3)

**Goal**: a persistent upstream failure costs one wasted attempt, and says so twice.

**Independent test**: with the batch endpoint failing, a scan completes with full content, attempts batch exactly once, logs it, and records it.

- [X] T013 [US3] Write the failing test in `waybill-cli/src/enrich/depsdev_source.rs`: with the endpoint failing, the scan makes **exactly one** batch attempt (SC-005a / C-6). **Count attempts, not output** — content is unchanged either way, so an output-based test would pass with no breaker wired in at all. Today this fails with ~one attempt per chunk.
- [X] T014 [US3] Implement the circuit breaker in `waybill-cli/src/enrich/depsdev_source.rs`: after the first batch failure, stop attempting the batched path for the remainder of the scan. Exactness is available because requests are issued sequentially (research R2) — the failure is observed before the next request goes out.
- [X] T015 [US3] Emit a log line on trip in `waybill-cli/src/enrich/depsdev_source.rs`, naming the failure (FR-007b / SC-005b). This is for the operator watching a scan get slow; the document record is for whoever reads the document later. Different audiences, both needed.
- [X] T016 [P] [US3] Test in `waybill-cli/src/enrich/depsdev_source.rs` that the log line is emitted when the circuit trips.
- [X] T017 [P] [US3] Test in `waybill-cli/src/enrich/depsdev_source.rs` that the existing document-scope degradation record is still emitted after a trip (FR-007 / C-7). It already works (research R4); this pins that the breaker did not bypass it.
- [X] T018 [P] [US3] Test in `waybill-cli/src/enrich/depsdev_source.rs` that enrichment content after a trip matches a successful run (C-5) — the breaker must not turn a speed degradation into a coverage one.

**Checkpoint**: the failure path is bounded, visible, and lossless.

---

## Phase 6: Polish

- [X] T019 Add the standing check for the batch endpoint leaving `v3alpha` (FR-007c / C-9), as a scheduled workflow in `.github/workflows/` following the existing canary pattern (bpf-linker, public-corpus): probe for a non-alpha batch endpoint and open a deduped issue when one appears. **This feature accepts a risk whose entire justification is that the upstream surface is unstable**; when that stops being true the trade deserves re-examination, and nobody will look unless something says so.
- [X] T020 [P] Add a test pinning the batch endpoint to `v3alpha` in `waybill-cli/src/enrich/deps_dev_batch.rs`. This is the cheap floor and is **not** a substitute for T019: it catches *our* change to the URL, not upstream's graduation. Note that `v3alpha` also appears at `hash_resolver.rs:65` for a different endpoint, so the check should say which surface moved.
- [X] T021 **Measure the after, not just the before (SC-001 / SC-005c / SC-008).** Re-run the T001 flag matrix against the post-change binary and record the deltas in `specs/923-enrich-batch-default/measurements/after.md`, for **two** repositories: the ~2,000-package one from T001 and a small one (tens of packages).
  - **SC-001** — a default scan's enrichment completes in seconds, not minutes. This is the feature's headline claim and the task list previously measured only the baseline, so the claim could have shipped unverified.
  - **SC-008** — the small repository is no slower than before. One batched request for a handful of components should not lose to a handful of individual ones, but that was an Assumption, and an assumption is what a measurement replaces.
  - **SC-005c** — with the endpoint failing, batch attempts stay at one on **both** sizes. T013's single-input test would pass even if the breaker tripped per chunk-group; running two sizes is what distinguishes a real bound from a coincidence.

- [ ] T022 Teeth-check T008, T009, T010, T013 against the preserved pre-change binary and record in `specs/923-enrich-batch-default/measurements/verification.md` which fail and which are guards. A test passing on both sides is a guard — label it rather than counting it.
- [ ] T023 Run the mandatory pre-PR gate: `./scripts/pre-pr.sh`. Enumerate the per-target results rather than citing the exit code. Use `-j 2 --test-threads=2` if the full run exhausts memory.
- [ ] T024 [P] Document the rationale (FR-010) in `docs/user-guide/configuration.md`, where the enrichment flags are already described: why batched is the default, that it runs against a surface its publisher documents as liable to change incompatibly, and that the fallback is what makes that acceptable. **The rationale has to survive the decision**, or a future reader mistakes the default for an oversight and reverts it.
- [ ] T025 [P] Write the CHANGELOG entry. Lead with what an operator gets — enrichment in seconds rather than minutes — and state plainly that the legacy flag still works and that enrichment-disabled scans are unchanged.
- [ ] T026 Confirm corpus goldens are untouched. Expect **no movement**: the harness hard-codes `--offline` (`corpus_harness_195/harness.rs:184`), so enrichment never runs there. Movement would mean the default flip reached a path that makes no network calls, which FR-008 forbids.
- [ ] T027 Comment on #927 with what landed and the measured numbers, and on #929 noting that this feature's one-attempt guarantee depends on the sequential execution it describes — so adding concurrency must revisit FR-007a rather than silently weakening it.

---

## Dependencies

```
Phase 1 (T001-T002)   baseline + the deps.dev-disabled control
        │             T001 before any edit — byte-identity is unprovable after
Phase 2 (T003-T004)   the equivalence guard          ◀── BLOCKING
        │             FR-002 licenses the whole feature; it lands first
Phase 3 US1 (T005-T009)   the default flip
        │
Phase 4 US2 (T010-T012)   opt-out reachable + both paths tested
        │
Phase 5 US3 (T013-T018)   circuit breaker, log, degradation
        │
Phase 6 (T019-T027)
```

**Story independence, honestly**: US1 and US2 are **not** independent — you
cannot flip a default without providing the opt-out, so T005 and T010 are two
halves of one change. US3 *is* technically independent of the flip, and is
listed last by priority, but it is a **shipping prerequisite**: before the
flip only opt-in users met a batch failure; after it, every scan can. Priority
reflects user-facing value; shipping order reflects risk. **Do not ship
Phase 3 without Phase 5.**

## Parallel opportunities

- **T016, T017, T018** — three tests over one behaviour, separable.
- **T020, T024, T025** — endpoint pin, docs, CHANGELOG.

Most of this feature lives in `scan_cmd.rs` and `depsdev_source.rs`, so
parallelism is genuinely limited. Marking more tasks `[P]` would be decoration.

## Implementation strategy

**MVP = Phases 1–3 plus Phase 5.** The default flip is the feature; the
circuit breaker is what makes it safe to ship. US2's tests can follow, but its
opt-out flag cannot — it lands with T005.

**T003 before T005.** The equivalence property is what makes "faster" an
acceptable reason to change a default. Asserting it after the flip would be
asserting it about a decision already made.

**T021 exists because the first draft measured only the baseline.** The task
list covered every correctness requirement thoroughly and left the feature's
headline speed claim with no verification at all — a bias worth naming, since
it would have shipped a performance feature whose performance nobody checked.

**The two tests that catch a plausible non-implementation:**

- **T013** counts *attempts*, because a breaker that was never wired in
  produces identical documents. An output-based test here passes trivially.
- **T008 / T010** assert *path selection*, for the same reason — both paths
  emit the same thing, so output cannot tell you which one ran.

## Verification standard

1. **A test that passes against the pre-change binary is a guard, not a proof.** T022 requires labelling which is which.
2. **Where output cannot distinguish two behaviours, assert the behaviour**, not the output — attempts, selection, log lines.
3. **Byte-identity is measured against a captured baseline**, never asserted from memory.
