---
description: "Task list for milestone 770 — Nightly SBOM Quality Regression Corpus"
---

# Tasks: Nightly SBOM Quality Regression Corpus

**Input**: Design documents from `/specs/770-sbom-quality-corpus/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

**Tests**: Test tasks ARE included. Not because TDD was requested, but because Constitution
Principle VII mandates unit coverage for parsing and comparison logic, and because
`CONTRIBUTING.md` makes `cargo +stable test --workspace` a merge gate. Tests are placed
alongside their implementation task rather than strictly before it.

**Organization**: Grouped by user story so each is independently implementable and testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on incomplete work)
- **[Story]**: US1 / US2 / US3, mapping to spec.md user stories
- Exact file paths are given in every task

## Path Conventions

All work lands in `xtask/` and `.github/workflows/`. No `waybill-cli/`, `waybill-common/`, or
`waybill-ebpf/` file is touched (plan.md Structure Decision).

---

## Phase 1: Setup

**Purpose**: Wire the new subcommand into the existing crate.

- [ ] T001 Add `toml = "0.8"` to `[dependencies]` in `xtask/Cargo.toml`. Verify with `cargo tree -p xtask | grep toml` that no new transitive crate enters the lockfile (plan.md Complexity Tracking claims zero — confirm it).
- [ ] T002 Create the module skeleton `xtask/src/quality/mod.rs` with a `QualityArgs` clap `Args`-derive struct carrying every flag from [contracts/xtask-quality-cli.md § C-1](./contracts/xtask-quality-cli.md), plus empty sibling modules `config.rs`, `fetch.rs`, `measure.rs`, `analyze.rs`, `score.rs`, `evaluate.rs`, `report.rs`.
- [ ] T003 Add the `Quality(quality::QualityArgs)` variant to the `Cli` enum in `xtask/src/main.rs` and dispatch it to `quality::run(args)`, matching the existing `Bench` arm.
- [ ] T004 [P] Add `target/quality/` to `.gitignore`.

**Checkpoint**: `cargo run -p xtask -- quality --help` prints the flag surface.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Types and config parsing every user story depends on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T005 Define newtypes in `xtask/src/quality/config.rs` per [data-model.md § 3](./data-model.md): `TargetName(String)`, `Range { min: u64, max: u64 }`, `RangeF { min: f64, max: f64 }`. Each MUST have a validating constructor rejecting `min > max`, so an inverted range is unrepresentable at runtime (Constitution IV).
- [ ] T006 Define the serde structs `CorpusConfig`, `Target`, `Pin`, `Expectations` in `xtask/src/quality/config.rs` per [data-model.md §§ 1–3](./data-model.md). `Pin` is an enum with `Sha` and `Ref` variants from the outset (FR-003), with `Ref` rejected at parse time as not-yet-supported.
- [ ] T007 Implement `CorpusConfig::load(path)` in `xtask/src/quality/config.rs` performing every validation in [contracts/corpus-config.md § C-4](./contracts/corpus-config.md). It MUST collect and return **all** configuration errors, not the first (FR-021).
- [ ] T008 [P] Unit tests in `xtask/src/quality/config.rs` for: duplicate names rejected; `min > max` rejected; non-hex `sha` rejected; `ref` key rejected with the not-supported message; empty `targets` rejected; `sbomqs` bound outside 0.0–10.0 rejected; multiple simultaneous errors all reported. Guard the module with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per house convention.
- [ ] T009 Define `TargetMeasurement`, `MeasurementStatus`, `UnmeasurableReason`, `Violation`, `MetricKind`, and `QualityReport` in `xtask/src/quality/report.rs` per [data-model.md §§ 4–5](./data-model.md), with serde attributes matching [contracts/quality-report.md](./contracts/quality-report.md). Measurement fields MUST be `Option` and MUST serialize as absent (not zero) when unmeasurable.
- [ ] T010 [P] Define the exit-code policy as an enum in `xtask/src/quality/mod.rs` — `Clean = 0`, `Violations = 1`, `ConfigError = 2` — per [contracts/quality-report.md § C-5](./contracts/quality-report.md).

**Checkpoint**: A malformed corpus file is rejected with all its errors and exit 2, before anything is fetched.

---

## Phase 3: User Story 1 — Measure and report (Priority: P1) 🎯 MVP

**Goal**: Produce a complete, accurate measurement record for every corpus target. No gating.

**Independent Test**: Run against the committed corpus with no `[targets.expect]` blocks
anywhere. Every reachable target yields wall time, sbomqs score, package/file counts, and the
flatness triple; the command exits 0.

- [ ] T011 [US1] Implement shallow fetch in `xtask/src/quality/fetch.rs` per [contracts/xtask-quality-cli.md § C-3](./contracts/xtask-quality-cli.md): `git init` → `remote add` → `fetch --depth 1 origin <sha>` → `checkout FETCH_HEAD`. **No `--recurse-submodules`** (research R6). Write a marker file on success; its presence is the cache-hit test. Honour `--cache-dir` and `--refresh`.
- [ ] T012 [US1] Implement the timed scan in `xtask/src/quality/measure.rs` per [contracts/xtask-quality-cli.md § C-4](./contracts/xtask-quality-cli.md). `--offline` is a **global** flag preceding `sbom scan`. Time only this subprocess (FR-009). Enforce the per-target timeout via spawn-and-kill, mapping expiry to `UnmeasurableReason::ScanTimedOut`. Set `$GOMODCACHE` to an empty per-run directory so Go edge counts do not drift with host cache state (research R2).
- [ ] T013 [US1] Implement document analysis in `xtask/src/quality/analyze.rs`: split components into package-tier (has `purl`) and file-tier (no `purl`); compute `edges`, `nodes_with_out_edges`, and `max_depth` by BFS from `metadata.component.bom-ref`; derive `flat = max_depth <= 1`; extract `waybill:graph-completeness` from `metadata.properties[]` into its own field. Per [data-model.md § 4](./data-model.md).
- [ ] T014 [P] [US1] Unit tests in `xtask/src/quality/analyze.rs` over hand-built CycloneDX values: a deep graph (depth > 1, not flat); a star graph (depth 1, flat); an empty `dependencies[]` (depth 0, flat); a document with no root `bom-ref`; a document mixing purl-bearing and purl-less components. Assert the graph-completeness field is captured but never influences `flat`.
- [ ] T015 [US1] Implement scoring in `xtask/src/quality/score.rs`: locate `sbomqs` via `WAYBILL_SBOMQS_BIN` then `$PATH` (mirroring `waybill-cli/tests/sbomqs_parity.rs:33`); run `sbomqs score --json <cdx>`; read `files[0].sbom_quality_score`. **Absence fails the run** (FR-016) — deliberately unlike `sbomqs_parity.rs`, which skips. Compare the reported version against the corpus `sbomqs_version` and record a mismatch as a warning that does not fail.
- [ ] T016 [US1] Store scores in a `HashMap<String, f64>` keyed by format name, populating only `"cyclonedx"`. This is the shape that makes adding SPDX additive (FR-030) — do not flatten it to a bare number.
- [ ] T017 [US1] Implement the JSON report in `xtask/src/quality/report.rs`: atomic write (temp file + rename) to `--output` or `target/quality/run-<sha12>.json`. Populate `waybill_sha`, `corpus_sha`, `sbomqs_version`, timestamps, and `runner` (FR-025). Sort `measurements` by name (FR-026).
- [ ] T018 [US1] Implement the human summary table in `xtask/src/quality/report.rs` per [contracts/quality-report.md § C-4](./contracts/quality-report.md), including the explicit "no violations" line when the run is clean (C-4.1).
- [ ] T019 [US1] Wire orchestration in `xtask/src/quality/mod.rs` per [contracts/xtask-quality-cli.md § C-2](./contracts/xtask-quality-cli.md): parse config → verify sbomqs → per-target fetch/scan/analyze/score → write report → print summary. **The report MUST be written before the exit decision** (C-2.1) so a failing run still leaves one behind.
- [ ] T020 [P] [US1] Implement `--filter` glob matching in `xtask/src/quality/mod.rs`, unioning repeated flags, `*` the only metacharacter. An empty match set reports "nothing selected" and exits 0, matching `xtask bench` semantics. Include unit tests.
- [ ] T021 [US1] Author `xtask/corpus/quality-corpus.toml` with all 18 targets and their resolved SHAs from [data-model.md § 6](./data-model.md). **Ship with no `[targets.expect]` blocks.** Every target carries an `# Observed 2026-09-03 (offline):` comment with its measured values from [research.md § R7](./research.md), plus the authoring notes for the special cases (ansible's inverted package/file ratio, pytorch's empty sub-repositories, the three permanently-flat targets).
- [ ] T022 [US1] Add an environment-gated end-to-end test at `xtask/tests/quality_smoke.rs` that runs one small target (`go-cobra`) and asserts a well-formed report. Gate behind `WAYBILL_QUALITY_E2E=1` so the default `cargo test --workspace` stays offline (Constitution VII).

**Checkpoint**: `cargo run -p xtask --release -- quality` measures all 18 targets and reports. US1 is independently shippable here.

---

## Phase 4: User Story 2 — Gate on ranges (Priority: P2)

**Goal**: Convert measurements into a regression gate.

**Independent Test**: Author a range deliberately narrower than a known-good value; confirm the
run fails naming that target and metric. Widen it; confirm it passes.

- [ ] T023 [US2] Implement range evaluation in `xtask/src/quality/evaluate.rs`: for each measurement with an authored expectation, test inclusive containment (FR-017) and emit a `Violation` carrying target, metric, expected bound, and observed value. Measurements with no expectation are skipped, never failed (FR-020).
- [ ] T024 [US2] Implement flatness evaluation in `xtask/src/quality/evaluate.rs`: compare observed `flat` against the expected boolean (FR-022). `graph_completeness` MUST NOT be evaluated against anything (research R3) — add a comment saying so, because it is the obvious wrong turn for a future maintainer.
- [ ] T025 [P] [US2] Unit tests in `xtask/src/quality/evaluate.rs`: value at the low bound passes; at the high bound passes; one below fails; one above fails; absent expectation yields no violation; expected-flat vs observed-not-flat fails and vice versa; float comparison at bounds behaves.
- [ ] T026 [US2] Evaluate **every** measurement on **every** target before returning, accumulating all violations (FR-018). Add a test asserting that a corpus with violations on multiple targets reports all of them.
- [ ] T027 [US2] Render violations in both outputs: the `violations` array in the JSON report (sorted by `(target, metric)` per FR-026) and the `VIOLATIONS (n)` block in the human summary per [contracts/quality-report.md § C-3/C-4](./contracts/quality-report.md).
- [ ] T028 [US2] Wire the exit-code policy in `xtask/src/quality/mod.rs`: any violation **or any unmeasurable target** yields exit 1 (FR-019); configuration errors yield exit 2. Add tests covering each code.
- [ ] T029 [US2] Implement `--no-gate`: suppress only the exit code, still computing and printing violations (C-1.1). It MUST NOT suppress the missing-`sbomqs` failure (C-1.2) — add a test.

**Checkpoint**: Gating works end to end. Ranges can now be authored incrementally, target by target.

---

## Phase 5: User Story 3 — Scheduled and on-demand execution (Priority: P3)

**Goal**: Run overnight without being asked; retain the report.

**Independent Test**: Trigger manually against a branch; confirm the full corpus runs, the
report is downloadable, and pass/fail shows in the job outcome.

- [ ] T030 [US3] Create `.github/workflows/quality-corpus.yml` with `schedule` (cron) and `workflow_dispatch` triggers, the latter accepting a `branch` input. Model it on `.github/workflows/bench.yml`: `permissions: contents: read`, `persist-credentials: false` on checkout, SHA-pinned actions, and a `concurrency` group.
- [ ] T031 [US3] Choose a nightly cron slot that does **not** collide with the existing `public-corpus.yml` run at `17 6 * * *` — both build waybill and clone repositories. Record the chosen slot and the reasoning in a workflow comment.
- [ ] T032 [US3] Add build steps: `dtolnay/rust-toolchain@<pinned>`, `Swatinem/rust-cache@<pinned>` with its own `shared-key`, then `cargo build --release -p waybill --bin waybill`. Note in a comment that the corpus needs the **waybill** binary specifically, not `xtask`, so the build step cannot be trimmed.
- [ ] T033 [US3] Add an `sbomqs` install step pinned to the same version `.github/workflows/ci.yml:315` installs, redirecting `GOMODCACHE` to a throwaway path exactly as that workflow does, then prepend `$HOME/go/bin` to `PATH`.
- [ ] T034 [US3] Invoke `cargo run -p xtask --release -- quality`. Pass any `workflow_dispatch` filter input through a per-step **env var**, never by direct `${{ }}` interpolation into `run:` — the shell-injection guard `bench.yml` documents.
- [ ] T035 [US3] Upload `target/quality/run-*.json` via `actions/upload-artifact@<pinned>` with `if: always()`, so a failing run still retains its report (FR-029). Set a retention period and `if-no-files-found: warn`.
- [ ] T036 [US3] Set an explicit `timeout-minutes` sized from the measured ~4-minute corpus plus build time, with headroom. Do **not** add `continue-on-error` — the gate must be able to fail the job.
- [ ] T037 [US3] Do **not** add an `actions/cache` step for the repository cache. Add a comment recording why (research R6: the 10 GB repo-wide cache ceiling would evict the Rust build caches other workflows depend on, and a ~95 s cold fetch is cheaper than that coordination).

**Checkpoint**: Feature complete. All three stories delivered.

---

## Phase 6: Polish & Cross-Cutting

- [ ] T038 [P] Walk `quickstart.md` end to end on a clean machine and correct any drift between it and the shipped flag surface.
- [ ] T039 [P] Add a short section to `CONTRIBUTING.md` (or `docs/`) pointing at the corpus and describing how to add a repository and author its ranges.
- [ ] T040 [P] Add a `CHANGELOG.md` entry under the unreleased heading, matching the house format used by recent milestones.
- [ ] T041 Confirm the report is deterministic: run twice against an unchanged tree and diff the two JSON files. Only genuinely varying measurements (wall time) may differ (FR-026).
- [ ] T042 **Pre-PR gate (MANDATORY)** — run `./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets` (zero errors) and `cargo +stable test --workspace` (every suite `0 failed`) MUST pass. Per `CLAUDE.md`, a passing per-crate `cargo test -p xtask` is **not** sufficient evidence.

---

## Dependencies

```text
Phase 1 (Setup)
      │
Phase 2 (Foundational) ── BLOCKS everything below
      │
      ├─► Phase 3 (US1) ── MVP; independently shippable
      │         │
      │         └─► Phase 4 (US2) ── needs US1's measurements to compare
      │                   │
      │                   └─► Phase 5 (US3) ── wraps US1+US2 in automation
      │
      └─► Phase 6 (Polish) ── after the stories it documents
```

**Story-level**: US1 stands alone. US2 requires US1 (nothing to gate otherwise). US3 requires
US1 and is far more useful with US2, since a nightly job that cannot fail is only a report.

## Parallel Opportunities

- **Phase 2**: T008 (config tests) ∥ T010 (exit-code enum) — different concerns, different files.
- **Phase 3**: T014 (analyze tests) ∥ T020 (filter) once their implementations land. T011, T013, T015 touch three separate files and can be developed concurrently once Phase 2 types exist; only T019 must wait for all three.
- **Phase 4**: T025 (evaluate tests) ∥ T027 (rendering).
- **Phase 6**: T038, T039, T040 are fully independent.

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + Phase 3.** That delivers a working corpus measurement command
reporting all four metric families across 18 real repositories, exiting 0. It is genuinely
useful on its own: it is the first characterisation of waybill's real-world output in one
place, and it is the data a maintainer needs before any range can be written.

**Then Phase 4**, which is small — range comparison over measurements that already exist —
and converts the report into a gate.

**Then Phase 5**, the automation. Deliberately last: a maintainer can run the corpus by hand
and get the full benefit, so scheduling is the least urgent slice.

**Ranges are authored between Phase 4 and Phase 5**, not as a coding task. FR-020 makes
unranged measurements observe-only, so the corpus ships green and bounds are added
target-by-target as a maintainer reviews each one. Landing with zero ranges is a valid,
intentional end state for this milestone.
