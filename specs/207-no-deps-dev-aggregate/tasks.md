---
description: "Task list for m207 — fix --no-deps-dev flag UX (aggregate disable, closes #596)"
---

# Tasks: Fix `--no-deps-dev` Flag UX — Aggregate Disable

**Input**: Design documents from `/specs/207-no-deps-dev-aggregate/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, quickstart.md ✓

**Tests**: Tests-included. Unit tests for the `resolve_enrich_sources` truth table + integration regression test pinning SC-001 (reporter's exact invocation).

**Organization**: 4 phases — setup (recon), foundational (new flag + semantic change + tests), US1/US2 acceptance-test phases (both satisfied by the same code change, so each phase adds a story-specific test), polish. All work fits in one source file plus one new integration test file.

## Format: `[ID] [P?] [Story] Description with file path`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1, US2 mapping to spec.md user stories
- **File paths**: absolute or repo-relative — every task cites exact target

## Phase 1: Setup (Recon)

**Purpose**: Verify plan.md / data-model.md line-numbers still match the current tree + establish baseline for SC-006 pre-PR delta.

- [X] T001 Verify pre-m207 baseline pre-PR is green: run `./scripts/pre-pr.sh` on branch `207-no-deps-dev-aggregate` HEAD (post-checkout, pre-implementation) and capture wall-clock time to `/tmp/m207-prepr-baseline.txt` for SC-006 delta measurement.
- [X] T002 [P] Recon: run quickstart.md `Empirical re-verification at implement time` block. Concretely:
  - `grep -n "pub no_deps_dev:\|pub no_deps_dev_graph:\|fn resolve_enrich_sources\|deps_dev_graph: !args.no_deps_dev_graph" mikebom-cli/src/cli/scan_cmd.rs | head` — expect `pub no_deps_dev:` at line 599, `pub no_deps_dev_graph:` at 636, `fn resolve_enrich_sources` at 1631, `deps_dev_graph: !args.no_deps_dev_graph,` at 1642.
  - Record output to `/tmp/m207-recon.txt`.

## Phase 2: Foundational — flag + semantic change + INFO log

**Purpose**: Land the entire behavioral change. Both US1 (aggregate) and US2 (fine-grained) are satisfied by the same three edits: (a) new flag, (b) semantic change in `resolve_enrich_sources`, (c) doc-comment updates + migration INFO log.

- [X] T003 Add `pub no_deps_dev_license: bool` field to `ScanArgs` in `mikebom-cli/src/cli/scan_cmd.rs` adjacent to the existing `pub no_deps_dev: bool` (line 599) per data-model E1. Include the full doc-comment from data-model E1 verbatim (mentions m207 (#596), migration path from pre-m207 `--no-deps-dev`, composition with `--offline` and `--enrich-sources`).
- [X] T004 Modify `resolve_enrich_sources` in `mikebom-cli/src/cli/scan_cmd.rs:1631-1645` per data-model E2. In the default-mode branch (lines 1638-1644), change:
  ```rust
  deps_dev: !args.no_deps_dev,
  clearly_defined: !args.no_clearly_defined,
  deps_dev_graph: !args.no_deps_dev_graph,
  ```
  to:
  ```rust
  deps_dev: !args.no_deps_dev && !args.no_deps_dev_license,
  clearly_defined: !args.no_clearly_defined,
  deps_dev_graph: !args.no_deps_dev && !args.no_deps_dev_graph,
  ```
  Add doc-comment explaining the m207 aggregate semantic. Allowlist-mode branch (lines 1632-1637) UNCHANGED per FR-004.
- [X] T005 [P] Update `--no-deps-dev` flag doc-comment at `mikebom-cli/src/cli/scan_cmd.rs:587-593` per data-model E4. Post-m207 text explains aggregate semantic + migration path (`--no-deps-dev-license` for pre-m207 behavior) + composition with `--offline` and `--enrich-sources`. Also update `--no-deps-dev-graph` doc-comment at line 625-630 to add the companion note about `--no-deps-dev-license`.
- [X] T006 [P] Add FR-006 migration INFO log per data-model E3. Insert immediately after `let enrich_cfg = resolve_enrich_sources(&args);` (around scan_cmd.rs:2714):
  ```rust
  if args.no_deps_dev && !args.no_deps_dev_license && !args.no_deps_dev_graph {
      tracing::info!(
          "--no-deps-dev now disables ALL deps.dev enrichment paths \
           (m207 aggregate semantic per #596). For the pre-m207 \"license \
           only\" behavior, use --no-deps-dev-license instead."
      );
  }
  ```
  Fires ONCE per scan.
- [X] T007 Post-T003/T004/T005/T006 sanity: run `CARGO_TARGET_DIR=/tmp/m207-c cargo +stable check --workspace --tests 2>&1 | tail -20`. Expected clean compile.
- [X] T008 Add unit tests to `mikebom-cli/src/cli/scan_cmd.rs::tests` covering the `resolve_enrich_sources` truth table (data-model E2). All are pure-function tests constructing synthetic `ScanArgs` structs:
  - `resolve_enrich_no_flags_default_all_on_m207` — no flags → `EnrichConfig { deps_dev: true, clearly_defined: true, deps_dev_graph: true }`.
  - `resolve_enrich_no_deps_dev_disables_both_paths_m207` — `no_deps_dev = true` → `EnrichConfig { deps_dev: false, clearly_defined: true, deps_dev_graph: false }` (**US1 acceptance**).
  - `resolve_enrich_no_deps_dev_license_disables_license_only_m207` — `no_deps_dev_license = true` → `EnrichConfig { deps_dev: false, clearly_defined: true, deps_dev_graph: true }` (**US2 acceptance**).
  - `resolve_enrich_no_deps_dev_graph_disables_graph_only_m207` — `no_deps_dev_graph = true` → `EnrichConfig { deps_dev: true, clearly_defined: true, deps_dev_graph: false }` (US2 companion).
  - `resolve_enrich_no_deps_dev_wins_over_no_deps_dev_graph_m207` — both set → same as `--no-deps-dev` alone (composition sanity).
  - `resolve_enrich_no_deps_dev_license_and_graph_equals_aggregate_m207` — `no_deps_dev_license = true` + `no_deps_dev_graph = true` → same as `--no-deps-dev` alone (composition sanity).
  - `resolve_enrich_sources_allowlist_overrides_no_deps_dev_m207` — `enrich_sources = [DepsDev]` AND `no_deps_dev = true` → `EnrichConfig { deps_dev: true, ... }` (allowlist wins per FR-004).
  - `resolve_enrich_no_clearly_defined_unaffected_by_no_deps_dev_m207` — `no_deps_dev = true` alone leaves `clearly_defined: true` (regression guard).
  - **F4 remediation** `no_deps_dev_help_mentions_enrich_sources_m207` — invoke `<ScanArgsForTest as clap::CommandFactory>::command().debug_assert()` (or fetch the `--no-deps-dev` arg's `long_help`/`help` via clap's introspection API) and assert the help text contains the substring `"enrich-sources"`. Pins FR-008 — operators reading `mikebom sbom scan --help` see the composition hint next to the flag they're setting.
- [X] T009 **F6 remediation** — locate the existing default-flags-off test via `grep -n "!parsed.inner.no_deps_dev\b" mikebom-cli/src/cli/scan_cmd.rs` (avoids brittle hard-coded line numbers per m199-m206 lesson). Extend the found assertion to ALSO assert `!parsed.inner.no_deps_dev_license` (new default: OFF).

## Phase 3: User Story 1 — Aggregate disable "just works" (Priority: P1)

**Story Goal**: Reporter's exact invocation produces zero deps.dev-sourced components in the emitted SBOM. `--no-deps-dev` alone suffices.

**Independent Test Criterion**: SC-001. Scan any project with `--no-deps-dev`; assert `grep deps.dev` on the emitted SBOM returns zero component-provenance hits.

- [X] T010 [US1] **F1+F2 remediation** — network-content assertion under `--offline` is untestable (both pre-m207 and post-m207 produce identical output because `--offline` short-circuits every deps.dev enrichment path regardless of `--no-deps-dev`). Reduce T010 to a scan-succeeds smoke test in a NEW file `mikebom-cli/tests/scan_no_deps_dev.rs`. SC-001 content verification (reporter's exact invocation → zero deps.dev-provenance components) moves to the PR-body manual reproducer per quickstart.md Reproducer 1 (network-required). Task body:
  - Add test `us1_no_deps_dev_scan_succeeds_and_fires_migration_log_m207` to `mikebom-cli/tests/scan_no_deps_dev.rs`.
  - Scan any pre-existing non-image fixture (`mikebom-cli/tests/fixtures/public_corpus/npm-express`) with `--offline --no-deps-dev`.
  - Assertions:
    - (a) Scan exits 0 (FR-007 no new failure modes).
    - (b) stderr contains the substring `"m207 aggregate semantic"` — FR-006 migration signal fires. (T011 pins the same assertion in a dedicated stderr-only test; T010 asserts it alongside the scan-succeeds guarantee to ensure the log is emitted from a real end-to-end scan, not just a synthesized invocation.)
  - Behavioral proof of the semantic change lives in T008's truth table (unit tests) + the manual quickstart Reproducer 1 verification captured in T016's PR body checklist.
- [X] T011 [P] [US1] Add stderr-assertion helper test `fr006_migration_info_log_fires_when_aggregate_flag_used_alone_m207` in `mikebom-cli/tests/scan_no_deps_dev.rs`:
  - Scan a non-image `--path` fixture (e.g., npm-express from public_corpus) with `--offline --no-deps-dev`.
  - Assert stderr contains `"m207 aggregate semantic"` exactly once.
- [X] T012 [P] [US1] Add negative test `fr006_migration_info_log_suppressed_when_fine_grained_flag_also_set_m207` in `mikebom-cli/tests/scan_no_deps_dev.rs`:
  - Scan with `--offline --no-deps-dev --no-deps-dev-license` (aggregate PLUS fine-grained escape hatch).
  - Assert stderr does NOT contain `"m207 aggregate semantic"` (fine-grained-aware operators aren't spammed with the migration signal per data-model E3 rationale).

## Phase 4: User Story 2 — Fine-grained sub-flags still work (Priority: P2)

**Story Goal**: `--no-deps-dev-license` disables only the license path; `--no-deps-dev-graph` continues to disable only the graph path. Both operate as documented via CLI-help post-fix.

**Independent Test Criterion**: Passing `--no-deps-dev-license` alone leaves the dep-graph enrichment active; passing `--no-deps-dev-graph` alone leaves the license enrichment active. Verified via T008's truth-table unit tests (`_disables_license_only` and `_disables_graph_only`).

- [X] T013 [US2] **F3 remediation** — network-free stderr-only assertion (same pattern as T011/T012). Add test `us2_no_deps_dev_license_alone_does_not_fire_aggregate_migration_log_m207` to `mikebom-cli/tests/scan_no_deps_dev.rs`:
  - Scan any pre-existing non-image fixture (`mikebom-cli/tests/fixtures/public_corpus/npm-express`) with `--offline --no-deps-dev-license` (aggregate flag NOT set; only fine-grained license flag).
  - Assertions:
    - (a) Scan exits 0.
    - (b) stderr does NOT contain the substring `"m207 aggregate semantic"` — the migration log fires ONLY for `--no-deps-dev` alone per data-model E3 rationale (fine-grained-aware operators aren't spammed).
  - Behavioral proof that `--no-deps-dev-license` correctly disables the license path but leaves the graph path active lives in T008's truth-table unit test `resolve_enrich_no_deps_dev_license_disables_license_only_m207`.

## Phase 5: Polish & Delivery

**Purpose**: Verification, PR body.

- [X] T014 [P] Run every existing enrichment-related test to confirm zero regression: `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --bin mikebom -- cli::scan_cmd::tests --no-fail-fast 2>&1 | tail -5` (expected `ok. N passed; 0 failed`). Verify T008's 8 new tests are included in the count.
- [ ] T015 Run `./scripts/pre-pr.sh` post-implementation. Capture wall-clock time; compute delta vs T001 baseline; MUST be ≤ 5s per SC-006. On failure, enumerate every `^---- .+ stdout ----` line per `feedback_prepr_gate_bails_on_first_failure` memory.
- [ ] T016 Draft PR body with `Closes #596` per SC-007. Include:
  - (a) 1-paragraph summary: root cause (name-vs-semantic mismatch), fix (1-line semantic change at scan_cmd.rs:1642 + new `--no-deps-dev-license` fine-grained flag + FR-006 migration INFO log).
  - (b) Reporter attribution (external gist / issue #596 opened during m206 session).
  - (c) Migration guidance: operators relying on the pre-m207 `--no-deps-dev` semantic can migrate by renaming to `--no-deps-dev-license`. The INFO log fires the first time an operator uses the aggregate flag without fine-grained escape hatches so they see it in their scan logs.
  - (d) Test coverage: 9 unit tests covering the truth table (including F4 help-text assertion) + 3 integration tests (T010 scan-succeeds + FR-006 log-fires; T011 log-fires-alone; T012 log-suppressed-with-fine-grained; T013 US2 fine-grained-flag stderr assertion).
  - (e) **Manual pre-merge SC-001 reproducer checklist** (per F1+F2 remediation — network-required content assertion is out of scope for automated CI). Reviewer performs quickstart.md Reproducer 1 verbatim against any real project + verifies (i) `jq '[.components[]? | .properties[]? | select(.name == "mikebom:source-files") | .value | select(. == "[\"deps.dev\"]")] | length'` returns `0` post-m207, and (ii) stderr contains the m207 migration INFO log line. Include the commands verbatim in the PR body so the reviewer just copy-pastes.
  - (f) Zero wire-format change; zero emitter touched; zero new Cargo deps.

---

## Dependencies

Sequential within phases; phases mostly sequential across the milestone:

```
Phase 1 (Setup) ── T001, T002 in parallel
     ↓
Phase 2 (Foundational) ── T003 → T004 → T005, T006 in parallel → T007 (sanity) → T008 (unit tests) → T009 (default-off test extension)
     ↓
Phase 3 (US1) ── T010 → T011, T012 in parallel
     ↓
Phase 4 (US2) ── T013 (independent of US1)
     ↓
Phase 5 (Polish) ── T014, T015 → T016
```

**MVP** = Phase 1 + Phase 2. Delivers: `--no-deps-dev` now works as its name suggests. US1 + US2 acceptance tests (T010-T013) add regression coverage but the code change is fully live after T004.

## Parallel opportunities

- **Setup**: T002 read-only.
- **Foundational**: T005 (doc-comment) + T006 (migration INFO log) — different sections of the same file, but different logical concerns; safe to write in parallel.
- **US1**: T011 + T012 — different `#[test]` functions in the same file.
- **Polish**: T014 read-only.

## Implementation strategy

Ship as a single PR. The behavioral change is 1 line + 1 new flag + 1 INFO log line; the coherent semantic + tests all belong together. Zero risk of partial-implementation issues.

**Total task count**: 16 tasks.
**By story**: US1 = 3 tasks (T010-T012), US2 = 1 task (T013). Phase 1 = 2, Phase 2 = 7, Phase 5 = 3. Total 16.
