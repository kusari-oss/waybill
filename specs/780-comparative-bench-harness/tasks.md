---
description: "Task list for 780-comparative-bench-harness"
---

# Tasks: Private comparative benchmark harness

**Input**: Design documents from `/specs/780-comparative-bench-harness/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/xtask-compare-cli.md, quickstart.md

**Tests**: INCLUDED, and not optional here. This feature is a measuring
instrument built because the previous ad-hoc measurements were wrong. Its
own correctness is the deliverable, so the tests that demonstrate it are
part of the feature rather than an add-on.

**Organization**: Grouped by user story. Phase order follows plan.md's
delivery order, which puts US2 (package identity) ahead of the other P1
stories because US3 and US4 both consume it and it is the source of the
worst historical error.

**Privacy note on ordering**: US5 is P1 but appears late. Its *structural*
parts — gitignore entries, config location, placeholder-only example — land
in Setup, where they must, because the alternative is writing code that
leaks and fixing it after. Phase 8 is where those properties get *asserted*,
not where they get built.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no incomplete dependencies)
- **[Story]**: US1–US6
- Exact file paths included

---

## Phase 1: Setup

- [X] T001 Create the module skeleton at `xtask/src/compare/` with `mod.rs`, `config.rs`, `identity.rs`, `truth.rs`, `score.rs`, `selfcheck.rs`, `report.rs`, each containing only a header comment citing `specs/780-comparative-bench-harness/plan.md`. Register `pub mod compare;` in `xtask/src/lib.rs`.
- [X] T002 Add `Compare(compare::CompareArgs)` to the `Cli` enum in `xtask/src/main.rs` and route it to `compare::run(args)`. Args per `contracts/xtask-compare-cli.md`: `--config` (default `xtask/compare/tools.local.toml`), repeatable `--target`, `--repeats` (default 5, minimum 3). The body may return `unimplemented!()` at this stage; the point is that the surface exists before anything fills it.
- [X] T003 [P] Add two `.gitignore` entries: `xtask/compare/*.local.toml` and `target/compare/`. **Do this before any code can write output** — a results file committed once is public forever, and the repository is public.
- [X] T004 [P] Write `xtask/compare/tools.example.toml` using placeholder ids only (`tool-a`, `tool-b`) plus a `waybill` entry, documenting every `ToolSpec` field from data-model.md. This is the committed file that must never name a competing tool (FR-015).
- [X] T005 [P] Write `xtask/compare/targets.toml` with the pinned corpus: at minimum one Go target at an exact SHA, declaring its `truth.method`. Reuse the shape of `xtask/corpus/quality-corpus.toml`.

---

## Phase 2: Foundational (BLOCKING)

**Purpose**: config types and the reuse wiring every story needs.

- [X] T006 Implement `ToolSpec`, `NetworkMode`, `Target`, `TruthSpec`, `TruthMethod` in `xtask/src/compare/config.rs` per data-model.md, with serde derives and TOML loading. `TruthMethod::is_superset()` must be implemented here — it is consumed by scoring and by reporting.
- [X] T007 [P] Unit-test config loading in `xtask/src/compare/config.rs`: a valid file round-trips; an absent config produces an error naming `tools.example.toml`; an unknown `network` value is rejected rather than defaulted, because silently defaulting an enriched tool to offline would mislabel its timings as authoritative.
- [X] T008 Wire target fetching to m770's existing pinned-SHA fetcher (`xtask/src/quality/fetch.rs`) rather than reimplementing it. If its interface is private, widen it to `pub(crate)` in the smallest possible edit and note why in a comment.
- [X] T009 Wire host classification to m669's existing `classify_noise` (`xtask/src/bench/run.rs`) and the `NoiseClass` type from `xtask/src/bench/schema.rs`. Do not introduce a second notion of host class.

**Checkpoint**: config parses, targets fetch, host class is known.

---

## Phase 3: User Story 2 — Counting the same things (P1) 🎯 MVP

**Goal**: every tool's output reduces to comparable package identities by one
shared rule.

**Independent Test**: feed the reducer output containing the same package
repeated once per manifest and confirm the distinct count is the package
count, with the raw count reported beside it.

**Why first**: US3 and US4 both consume it, and it caused the five-fold
misstatement that motivated the feature.

- [X] T010 [US2] Implement `PackageIdentity` and PURL normalisation in `xtask/src/compare/identity.rs` per data-model.md: lowercase type, normalise namespace, **retain version**, drop qualifiers and subpath. Hand-rolled, not via `waybill-common` — research.md R4: an instrument sharing code with its subject would apply the same bug to every tool *and* to its own self-check.
- [X] T011 [P] [US2] Unit tests in `xtask/src/compare/identity.rs` for normalisation: case differences in type collapse; qualifiers and subpath are dropped; a malformed purl is rejected rather than silently producing a partial identity.
- [X] T012 [US2] **The regression test for the historical error.** In `xtask/src/compare/identity.rs`, assert that the same `pkg:golang/foo@v1.0` repeated five times reduces to one identity, AND that `pkg:golang/foo@v1.0` plus `pkg:golang/foo@v2.0` reduces to **two**. The second assertion is what makes a silent regression to version-stripping fail visibly — the rule chosen in clarification differs from the one used in the ad-hoc comparison precisely here.
- [X] T013 [US2] Implement CycloneDX component reduction in `xtask/src/compare/identity.rs`: parse `components[].purl`, count purl-less components separately into `identityless_components` (FR-007), and return `distinct_packages` alongside the `raw_components` it reduced from (FR-006a).
- [X] T014 [P] [US2] Unit test the reduction in `xtask/src/compare/identity.rs` against a small hand-authored CycloneDX document containing duplicates, purl-less entries, and two versions of one package, asserting all three counts independently.

**Checkpoint**: counts are comparable across tools and the reduction is proven.

---

## Phase 4: User Story 4 — The harness proves itself first (P2)

**Goal**: the harness recovers a known answer before it scores anyone.

**Independent Test**: deliberately break a scoring rule; confirm the run
aborts before any tool is measured.

- [X] T015 [US4] Build the known-answer fixture at `xtask/compare/fixtures/known-answer-go/`: a synthetic Go module tree whose exact module set is fixed by construction, **including two versions of one package** so a version-stripping regression fails here too. Use `waybill-fixture-*` naming per project convention; no real coordinates.
- [X] T016 [US4] Write the expected set as a plain committed list at `xtask/compare/fixtures/known-answer-go/EXPECTED.txt`, one identity per line, hand-authored rather than generated — a generated expectation would move with the bug it is meant to catch.
- [X] T017 [US4] Implement the self-check in `xtask/src/compare/selfcheck.rs`: derive identities from the fixture, compare against `EXPECTED.txt`, and on mismatch return the symmetric difference.
- [X] T018 [US4] Gate the run on it in `xtask/src/compare/mod.rs`: the self-check runs before any tool is invoked against any target, and a failure exits 1 having measured nothing (contract C-1).
- [X] T019 [US4] **Prove the gate has teeth.** Add a test in `xtask/src/compare/selfcheck.rs` that injects a deliberately wrong reduction rule and asserts the self-check fails with the discrepancy named. A gate never observed failing is not known to be a gate — five keyless tests in this project spent a year `#[ignore]`d for want of exactly this check.

**Checkpoint**: the instrument has standing to measure.

---

## Phase 5: User Story 1 — A measurement that survives repetition (P1)

**Goal**: two runs agree, or the harness says why not.

**Independent Test**: run twice on an unchanged tree; verdicts and figures
match within declared tolerance.

- [X] T020 [US1] Implement serialized, **interleaved** execution in `xtask/src/compare/mod.rs` (contract C-2): round-robin across tools per repeat, never per-tool blocks, never concurrent. Block execution is how a 2.7× contention error entered the motivating measurements; interleaving cancels first-order drift for free.
- [X] T021 [US1] Implement per-invocation measurement in `xtask/src/compare/mod.rs`: wall clock and peak RSS. Reuse m669's approach in `xtask/src/bench/measure.rs`; note that `/usr/bin/time -l` output is easily swallowed when the child's stderr is redirected, so parse it from a dedicated stream.
- [X] T021a [US1] Capture provenance in `xtask/src/compare/mod.rs` (FR-005): run each tool's `version_argv` once per session and record the result in `ComparisonRun.tool_versions`, alongside each target's pinned revision. A figure without the version that produced it cannot be compared against a later one, which is the whole reason this is a requirement rather than a nicety.
- [X] T022 [US1] Implement `Outcome` classification in `xtask/src/compare/mod.rs`: `Ok`, `Failed{code}`, `TimedOut`, `Unparseable`, `ToolAbsent`. A failed tool must never be scored as having found nothing (FR-011) — that reading turns a crash into a coverage result.
- [X] T023 [US1] Implement the timing spread gate in `xtask/src/compare/mod.rs`: compute max/min per tool per target; exceed the configured tolerance and add `WithheldReason::TimingSpreadExceeded` naming tool and observed ratio.
- [X] T024 [US1] Implement the coverage-reproducibility check in `xtask/src/compare/mod.rs`: coverage figures must be **identical** across repeats (FR-001c). A difference is a defect, not noise — add `WithheldReason::CoverageNotReproducible` rather than averaging. Research R2 establishes these metrics are exactly reproducible when inputs are pinned.
- [X] T025 [US1] Implement `Verdict` assembly in `xtask/src/compare/mod.rs` accumulating **all** applicable `WithheldReason`s rather than short-circuiting on the first, so one run tells the operator everything to fix.
- [X] T026 [US1] Add the host-class reason in `xtask/src/compare/mod.rs`: a non-reference-class host yields `WithheldReason::HostNotReferenceClass` while still recording figures (FR-004).
- [X] T026a [US1] Add `WithheldReason::ToolVersionMismatch` in `xtask/src/compare/mod.rs` and refuse to compare a run against a prior one whose recorded tool versions or target revisions differ (FR-005). Mirrors the guard m818 added to `xtask bench` for host class — same failure shape, same remedy: the metadata was already being recorded and simply never checked.
- [X] T027 [P] [US1] Unit-test verdict assembly in `xtask/src/compare/mod.rs`: multiple simultaneous reasons all surface; a clean run yields `Comparable`; a withheld verdict is not an error exit (contract: exit 0, verdict is a result).

**Checkpoint**: repeated runs agree, or explain themselves.

---

## Phase 6: User Story 3 — Scoring against truth (P2)

**Goal**: "found more" becomes "was more nearly right", with the truth's
limitations stated.

**Independent Test**: on a fixture with a known set, scores match a
hand-computed one.

- [X] T028 [P] [US3] Implement `TruthMethod::GoSumUnion` in `xtask/src/compare/truth.rs`: union module paths across every `go.sum` in the tree. Measured 479 on kubernetes `b1856e29` (research R5).
- [X] T029 [P] [US3] Implement `TruthMethod::GoModRequires` in `xtask/src/compare/truth.rs`: union of require directives across every `go.mod`. Measured 423 on the same tree.
- [X] T030 [P] [US3] Implement `TruthMethod::DeclaredExact` in `xtask/src/compare/truth.rs`: read a committed expected list. This is the method the self-check fixture uses.
- [X] T031 [US3] Implement scoring in `xtask/src/compare/score.rs`: `found`, `missed`, `extra` against the truth set, carrying `method` and `truth_is_superset` into the `Accuracy` record.
- [X] T031a [US3] Exclude non-`Ok` measurements from accuracy scoring in `xtask/src/compare/score.rs` (SC-008). A tool that crashed found nothing *because it crashed*; scoring it as having found nothing turns a failure into a coverage result and would make a broken tool look merely thorough-less. Test the exclusion.
- [X] T031b [US3] Handle the no-truth branch in `xtask/src/compare/score.rs` (FR-009): when a target declares no truth method, report distinct and raw counts, omit accuracy entirely, and **state that accuracy was not scored** rather than showing zeros or blanks. A blank accuracy column is indistinguishable from a tool scoring nothing, which is the same fail-open reading FR-011 forbids elsewhere. The `ripgrep` target in `xtask/compare/targets.toml` already exercises this — cargo truth derivation is not implemented.
- [X] T032 [US3] Enforce the method guard in `xtask/src/compare/score.rs`: refuse to compare scores derived by different methods, yielding `WithheldReason::TruthMethodMismatch` (FR-008a).
- [X] T033 [US3] Implement superset labelling in `xtask/src/compare/score.rs` (FR-008b). **This exists because of a specific error**: waybill scored 468 against the go.sum union of 479 and was called "closest to ground truth", but go.sum holds hashes for modules merely considered during resolution. Scoring against a superset rewards over-reporting and penalises correct omission.
- [X] T034 [P] [US3] Unit-test scoring in `xtask/src/compare/score.rs` against hand-computed expectations, including one case where a tool correctly omits a module present in a superset truth and is therefore scored *worse* — the behaviour the label warns about.

**Checkpoint**: accuracy is scored and its limits are visible.

---

## Phase 7: User Story 6 — Comparing like with like (P3)

**Goal**: a tool's cheapest mode is never set against another's richest.

**Independent Test**: request a mismatched comparison; confirm it is refused
or prominently labelled.

- [X] T035 [US6] Implement mode matching in `xtask/src/compare/mod.rs`: comparing tools whose `NetworkMode` differs yields `WithheldReason::ModeMismatch`. The motivating comparison ran waybill `--offline` against another tool's default and drew a conclusion from it.
- [X] T036 [US6] Implement the enriched-mode split in `xtask/src/compare/report.rs` (FR-002a/FR-002b, contract C-8): enriched timings labelled indicative with a wider tolerance and excluded from speed statements; coverage from the same run reported as authoritative without caveat.
- [X] T037 [P] [US6] Unit-test in `xtask/src/compare/report.rs` that one tool run in two modes produces two rows, never merged.

---

## Phase 8: User Story 5 — Private by construction (P1)

**Goal**: running the harness cannot publish anything.

**Independent Test**: run it, then confirm the working tree is clean of
publishable files.

- [X] T038 [US5] Implement output writing in `xtask/src/compare/report.rs` to `target/compare/run-<timestamp>.json` only, plus a stdout summary. No path outside `target/` is written (FR-014).
- [X] T039 [US5] Implement the report renderer in `xtask/src/compare/report.rs` per contract C-3/C-4/C-5/C-7: distinct beside raw, identityless separate, truth method and superset flag beside every score, timing as within-session ratio with absolutes labelled context.
- [X] T040 [US5] Enforce the no-comparative-language rule in `xtask/src/compare/report.rs` (FR-017): the renderer emits quantities and conditions, never "faster", "better", "more accurate". Add a test asserting the rendered output contains none of those tokens — the instrument should be structurally incapable of producing the sentence we do not want published.
- [X] T041 [P] [US5] Add a test in `xtask/src/compare/mod.rs` asserting no file under `.github/workflows/` references the compare subcommand (FR-016). Artefacts on a public repository are world-readable, so a CI lane would defeat the purpose.
- [X] T042 [P] [US5] Add a test asserting `xtask/compare/tools.example.toml` contains no tool identifier other than `waybill` and the documented placeholders (FR-015, SC-006). This is the check that keeps the committed source tool-agnostic as people edit it later.

**Checkpoint**: privacy holds structurally, not by convention.

---

## Phase 9: Polish

- [X] T042a [US5] Verify SC-005 rather than assume it: run the harness end to end, then assert `git status --porcelain` reports no new tracked or untracked publishable file, and that nothing was written outside `target/`. The gitignore entries and the write path are both already in place — this is the task that checks they actually hold, because a privacy property that is built and never tested is the same shape as the notification path m779 built and never fired.
- [~] T043 **DEFERRED — no reference-class host available; decision taken 2026-09-09 to proceed on the provisional 1.25 and not quote timing externally. Recorded in first-run.md.** Calibrate the thresholds and record the measurements in `specs/780-comparative-bench-harness/calibration.md`: repeat count, offline tolerance, enriched tolerance, timeout. These were deliberately left unset in the spec because they need measuring rather than guessing. Note the host class of the calibration run — m770 observed a 2.6× wall-time spread on identical hardware, so a tolerance chosen on a laptop will not transfer.
- [X] T044 [P] Run the harness end to end against the pinned corpus with a real tool set and record the output in `target/compare/first-run.md` — **gitignored, NOT under `specs/`**, because the output names the tools compared against and `specs/` is published (FR-014) — including the verdict. If the verdict is withheld, that is a legitimate result to record, not a failure to hide.
- [X] T044a **Verify the feature's central claim (SC-001, SC-001a).** Run the full harness twice on an unchanged tree, diff the two result files, and record both plus the diff in `target/compare/first-run.md` (**gitignored, NOT under `specs/`** — see T044). Offline coverage and accuracy figures MUST be byte-identical; timings MUST agree within the calibrated tolerance; verdicts MUST match. For an enriched mode, coverage MUST still be identical while timings may differ (SC-001a). **Nothing else in this feature demonstrates run-to-run reproducibility** — T024 checks repeats inside one run, which is a different claim. A harness for reproducible measurement that was never run twice would ship with its purpose untested.
- [X] T045 Walk `specs/780-comparative-bench-harness/quickstart.md` end to end and correct anything that does not behave as written.
- [X] T046 Run `./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace` must pass; enumerate every `^---- .+ stdout ----` line before claiming green.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: none. T003 (gitignore) must land before any code can write output.
- **Phase 2 (Foundational)**: needs Phase 1. Blocks every story.
- **Phase 3 (US2)**: needs Phase 2. **Blocks US3 and US4** — both consume package identity.
- **Phase 4 (US4)**: needs US2.
- **Phase 5 (US1)**: needs Phase 2; independent of US2/US3/US4 for its own logic.
- **Phase 6 (US3)**: needs US2 for identities and US4's fixture for its exact-method test.
- **Phase 7 (US6)**: needs US1's verdict machinery.
- **Phase 8 (US5)**: needs the report renderer, so effectively last among the stories.
- **Phase 9**: needs everything.

### Story dependency graph

```text
Setup → Foundational
            ├─→ US2 (identity) ──┬─→ US4 (self-check)
            │                    └─→ US3 (truth + scoring)
            └─→ US1 (determinism) ─→ US6 (mode matching)
                                        └─→ US5 (privacy + report)
```

### Parallel opportunities

- T003, T004, T005 in parallel.
- T011, T014 alongside their implementation tasks (separate concerns in one file — coordinate).
- T028, T029, T030 in parallel (three independent derivation methods).
- T041, T042 in parallel.

### Within each story

- Tests alongside or before the code they cover; T012 and T019 specifically must be *seen failing* before the code that satisfies them exists.
- Types before consumers: `PackageIdentity` before reduction, `TruthMethod` before scoring.

---

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (US2).** At that point the single most
damaging error — comparing raw counts across tools that count differently —
is structurally impossible. That alone would have prevented the worst
misstatement in the episode that motivated this work.

**Second increment = Phase 4 (US4).** The self-check is what lets the harness
claim standing. Without it we have a faster way to be confidently wrong.

**Phases 1–4 and 6 need no external tool installed and no network.** Most of
the feature is pure logic over JSON, which is why the delivery order front-
loads it.

**Two habits this feature exists to encode:**

- A measurement without its conditions is not a measurement. Every figure
  the harness emits carries host class, tool version, target revision, mode
  and spread, because every wrong conclusion in the motivating episode came
  from a number that had been separated from its context.
- Declining to answer is a valid output. The verdict machinery exists so the
  harness can say "not comparable" instead of producing a number someone
  will quote.
