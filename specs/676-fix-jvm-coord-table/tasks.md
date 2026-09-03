---

description: "Task list for feature 676: Fix m224 Pants coursier-JVM reader — accept coord-table directDependencies (issue #756)"
---

# Tasks: Fix m224 Pants coursier-JVM reader — accept coord-table `directDependencies`

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/676-fix-jvm-coord-table/`
**Prerequisites**: `plan.md` (required), `spec.md` (required), `research.md`, `data-model.md`, `contracts/reader-behavior.md`, `quickstart.md`

**Tests**: 5 new unit tests are called out explicitly in the spec (FR-007) and delivered under US2. These lock in the shape-tolerance contract; the fix itself is trivially small (~5-10 lines).

**Organization**: Tasks are grouped by user story. Phase 2 (Foundational) contains the production code fix — all three user stories depend on it. US1 verifies via empirical smoke-scan (0 new code). US2 adds the shape-tolerance test suite. US3 restores the corpus regression gate.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Every task includes exact file paths from the repo root at `/Users/mlieberman/Projects/mikebom/`.

## Path Conventions

Single-project layout — all production changes under `waybill-cli/src/scan_fs/package_db/pants_jvm/`. Test-infra changes under `waybill-cli/tests/`. External artifact: existing fork at `github.com/kusari-sandbox/example-jvm` (created in PR #757, unchanged).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify the fork the corpus target references is still reachable at the pinned SHA. No new external actions.

- [X] T001 Verify `kusari-sandbox/example-jvm` still reachable at pinned SHA `675ee75d36f2c1b096b0def51efcfffd02bd1251` via `git ls-remote --heads https://github.com/kusari-sandbox/example-jvm main`. Expected output line begins with `675ee75d36f2c1b096b0def51efcfffd02bd1251`. If the SHA has drifted (upstream force-push, unlikely for a stable pantsbuild example), pause and investigate before proceeding.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Apply the production code fix. This is the smallest possible change that closes issue #756 and unblocks all three user stories.

**⚠️ CRITICAL**: US1, US2, US3 all depend on this phase completing.

- [X] T002 Reproduce the bug on `main` first (per `quickstart.md` "Reproduce the bug (baseline)"): clone `pantsbuild/example-jvm` at pinned SHA to `/tmp/repro-676`, run `waybill --offline sbom scan --path .`, confirm the `WARN pants-coursier-jvm reader: coursier TOML body parse error` log line fires and `components_emitted=0`. This is the "before" snapshot for the fix's empirical proof.
- [X] T003 Remove the `direct_dependencies` field from `struct Entry` in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs` (delete lines 58-60: the `#[serde(default, rename = "directDependencies")] pub(crate) direct_dependencies: Vec<String>,` declaration and its doc-comment lines). Serde's default field-tolerance handles the shape variability — the field is invisible to the deserializer, so any TOML shape at that key parses without error.
- [X] T004 Remove the dead-code sink line at `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs:358` (`let _ = &entry.direct_dependencies;`). Preserve the surrounding dead-code-sink block for `file_name` and `serialized_bytes_length` — only remove the specific `direct_dependencies` reference. Also update the block's comment if it still lists `direct_dependencies`.
- [X] T005 Update the in-file test constructor in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs::tests::parse_valid_pants_coursier_lockfile` (around line 497): remove the `direct_dependencies: Vec::new(),` line from the `Entry { ... }` initializer. All other fields unchanged.
- [X] T006 Compile-check the crate: `cargo build -p waybill 2>&1 | tail -5`. Must complete without errors. Fix any compilation errors surfaced by the field removal (unlikely — the research proved zero downstream uses).
- [X] T007 Run existing unit + integration tests: `cargo test -p waybill --lib pants_jvm && cargo test --test pants_coursier_jvm_reader`. Both suites must complete `ok. N passed; 0 failed` — proves FR-006 (byte-identical output for pre-fix passing fixtures).

**Checkpoint**: Foundational fix landed. Empirical verification (US1) + new unit tests (US2) + corpus target (US3) can now proceed.

---

## Phase 3: User Story 1 — SBOM operator scanning real Pants JVM monorepo (Priority: P1) 🎯 MVP

**Goal**: Prove the bug is fixed by empirical smoke-scan of `pantsbuild/example-jvm` at the pinned SHA. Zero new code — verifies the Foundational phase's deliverable.

**Independent Test**: Scan the fixture; verify emitted SBOM contains `pkg:maven/*` components matching the resolve's declared coordinates; verify no `coursier TOML body parse error` WARN fires.

### Implementation for User Story 1

- [X] T008 [US1] Repeat the reproduction step from T002 on the fixed branch: `cargo build -p waybill --release` then scan `/tmp/repro-676` with `RUST_LOG=info /path/to/waybill --offline sbom scan --path . --format cyclonedx-json --output /tmp/jvm-post-fix.cdx.json`. Verify: (a) no `WARN pants-coursier-jvm reader: coursier TOML body parse error` line, (b) `INFO pants-coursier-jvm reader complete` line reports `lockfiles_parsed_ok=1, lockfiles_skipped_corrupt=0, components_emitted>=20`, (c) `jq '[.components[].purl | select(startswith("pkg:maven/"))] | length' /tmp/jvm-post-fix.cdx.json` returns ≥ 20, (d) `jq '.components[].purl' /tmp/jvm-post-fix.cdx.json | grep -c 'pkg:maven/com.google.guava/guava@'` returns ≥ 1, (e) `jq '.components[].purl' /tmp/jvm-post-fix.cdx.json | grep -c 'pkg:maven/org.scala-lang/scala-library@'` returns ≥ 1.

**Checkpoint**: US1 delivered. The MVP is shippable at this point — the P1 bug is closed and empirically verified.

---

## Phase 4: User Story 2 — Reader robust across both empty-array and coord-table shapes (Priority: P2)

**Goal**: Lock in the shape-tolerance contract via 5 new unit tests. Ensures future edits to `Entry` cannot silently re-introduce shape-sensitivity.

**Independent Test**: All 5 new unit tests pass. Together they cover every legal `directDependencies` shape a coursier lockfile can emit.

### Implementation for User Story 2

- [X] T009 [US2] Add `parse_coord_table_single_dep` test to the `#[cfg(test)] mod tests` block in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`. TOML fixture per `data-model.md` §Entity 4 T-A: one `[[entries]]` block with a single `[[entries.directDependencies]]` coord-table entry (fields: group, artifact, version). Call `parse(fixture_bytes)`; assert `Ok(lock)` and `lock.entries.len() == 1`.
- [X] T010 [US2] Add `parse_coord_table_multi_dep` test in the same file. TOML fixture per §Entity 4 T-B: one `[[entries]]` block with three `[[entries.directDependencies]]` coord-table entries. Assert `Ok(lock)` and `lock.entries.len() == 1`.
- [X] T011 [US2] Add `parse_mixed_empty_and_coord_table` test in the same file. TOML fixture per §Entity 4 T-C: two `[[entries]]` blocks — first with `directDependencies = []`, second with `[[entries.directDependencies]]` coord-table. Assert `Ok(lock)` and `lock.entries.len() == 2`.
- [X] T012 [US2] Add `parse_legacy_string_form_deps` test in the same file. TOML fixture per §Entity 4 T-D: one `[[entries]]` block with `directDependencies = ["com.google.guava:guava:31.0.1-jre"]` (legacy string-array form). Assert `Ok(lock)` and `lock.entries.len() == 1`.
- [X] T013 [US2] Add `malformed_coord_entry_skipped_at_emission` test in the same file per §Entity 4 T-E: parse a lockfile with one entry whose `coord.group = ""`, then call `entry_to_package_db_entry(&entry, &fake_path, "test-resolve")`. Assert the return is `None` (per existing FR-004 fail-open behavior; test locks in current-code contract). Zero production code change needed — this test verifies existing behavior.
- [X] T014 [US2] Run the new tests: `cargo test -p waybill --lib pants_jvm::lockfile::tests`. All 5 new tests plus all pre-existing tests must pass.

**Checkpoint**: US2 delivered. Shape-tolerance contract locked in.

---

## Phase 5: User Story 3 — Re-enable pants-example-jvm corpus regression gate (Priority: P3)

**Goal**: Restore the `pants-example-jvm` corpus target from PR #757's reservation. Nightly CI thereafter catches any future regression against a real-world upstream lockfile.

**Independent Test**: `WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus corpus_pants_example_jvm` passes.

### Implementation for User Story 3

- [X] T015 [P] [US3] Restore the `pants-example-jvm` `CorpusTarget` entry in `waybill-cli/tests/corpus_harness_195/manifest.rs`. Replace the multi-line `NOTE: pants-example-jvm intentionally omitted for now — the m224 reader rejects the coord-table form...` comment block with the entry from `data-model.md` §Entity 5: `name: "pants-example-jvm"`, `source: SourceKind::Git { clone_url: "https://github.com/kusari-sandbox/example-jvm" }`, `pinned: PinnedRef::Sha { hex: "675ee75d36f2c1b096b0def51efcfffd02bd1251" }`, `ecosystem: Ecosystem::JavaMaven`, `exercises: "..."`, `layer1: super::layer1_assertions::pants_example_jvm_layer1`.
- [X] T016 [P] [US3] Add `pants_example_jvm_layer1` function to `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` per `data-model.md` §Entity 6. Four invariants in order: (1) `maven-transitives-present-at-scale` — count `pkg:maven/*` components ≥ 20; (2) `top-level-guava-present` — any `pkg:maven/com.google.guava/guava@*`; (3) `top-level-scala-library-present` — any `pkg:maven/org.scala-lang/scala-library@*`; (4) `pants-resolve-annotation-present` — at least one `pkg:maven/*` component carries `waybill:pants-resolve` in `.properties[]`. Reuse existing helpers `cdx_has_component_purl` and `cdx_has_component_property` from the same file. Each failure's `suggested_action` names the m224 reader as the suspected regression site.
- [X] T017 [P] [US3] Add `#[test] fn corpus_pants_example_jvm()` to `waybill-cli/tests/public_corpus.rs`, immediately after the existing `corpus_pants_example_django` test (matches manifest ordering). Body: single line `run_target("pants-example-jvm")`.
- [X] T018 [US3] Compile-check the test binary: `cargo test --test public_corpus --no-run`. Must complete without unused-import warnings.
- [X] T019 [US3] Run non-network manifest audits: `cargo test --test public_corpus`. All existing audit tests + all skip-tests (including the new `corpus_pants_example_jvm`) must pass.
- [X] T020 [US3] Generate goldens: `WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1 WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus corpus_pants_example_jvm`. Verify `waybill-cli/tests/fixtures/public_corpus/pants-example-jvm/{cdx.json,spdx-2.3.json,spdx-3.json}` now exist.
- [X] T021 [US3] Verify byte-identity across two runs: `WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus corpus_pants_example_jvm`. Test passes (compares emitted output against just-written goldens).
- [X] T022 [US3] Sanity check golden size + content: `du -sk waybill-cli/tests/fixtures/public_corpus/pants-example-jvm/` (informational — no hard limit; document the observed size in the PR body). `jq '[.components[].purl | select(startswith("pkg:maven/"))] | length' waybill-cli/tests/fixtures/public_corpus/pants-example-jvm/cdx.json` returns ≥ 20 (SC-001 spec-level check baked into the golden).

**Checkpoint**: US3 delivered. Corpus regression gate is live; future JVM-reader regressions fail nightly CI.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final verification + PR mechanics.

- [ ] T023 Run the full pre-PR gate: `./scripts/pre-pr.sh`. Confirms workspace clippy + all workspace tests pass. Per memory `feedback_prepr_gate_full_output`, verify the final "all pre-PR checks passed" line — do not grep-and-declare-victory on partial output.
- [ ] T024 Commit changes with a message summarizing the fix (delete-field approach per research R1 Option B). Reference issue #756 explicitly. Include the `Co-Authored-By: Claude Opus 4.7 (1M context)` line.
- [ ] T025 Push the branch and open a PR against `main` on `kusari-oss/waybill`. PR body includes: Summary (bug + fix approach), Before/After empirical numbers from T002/T008, Research findings (`direct_dependencies` was dead code), Test plan checklist including the new 5 unit tests + corpus target. Reference `Closes #756`.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no dependencies. Read-only network verification.
- **Phase 2 (Foundational)**: T002-T007. T002 is the baseline reproduction (before the fix); T003-T005 apply the fix in dependency order (all touch the same file, sequential); T006 compile-checks; T007 runs existing tests. All later phases depend on Phase 2 completing.
- **Phase 3 (US1)**: T008 depends on Phase 2 complete. Empirical verification only.
- **Phase 4 (US2)**: T009-T014 depend on Phase 2 complete. T009-T013 all edit the same file (`lockfile.rs` `mod tests` block) — sequential. T014 depends on T009-T013.
- **Phase 5 (US3)**: T015, T016, T017 modify different files → parallel opportunity. T018 depends on T015-T017 all done. T019 depends on T018. T020 depends on T019. T021 depends on T020. T022 depends on T020.
- **Phase 6 (Polish)**: T023 depends on all preceding. T024 depends on T023. T025 depends on T024.

### User Story Dependencies

- **US1 (P1)**: Delivered by Phase 2 (the production fix). US1's independent test (T008) can execute the moment Phase 2 completes.
- **US2 (P2)**: Independent from US1 in terms of code paths, but both depend on Phase 2. US2's tests exercise the post-fix `Entry` struct.
- **US3 (P3)**: Depends on Phase 2 (fix must land before corpus target can pass). Independent from US1 and US2 in terms of code paths.

### Parallel Opportunities

- **Phase 5**: T015, T016, T017 can run in parallel (different files: `manifest.rs`, `layer1_assertions.rs`, `public_corpus.rs`).
- **Phases 3, 4, 5** can theoretically run in parallel with each other once Phase 2 is done — but a single agent typically batches them sequentially because Phase 3 (US1) is a fast empirical smoke, Phase 4 (US2) is a compact test suite, and Phase 5 (US3) has the longest task chain.

---

## Parallel Example: Phase 5 (US3)

```bash
# Parallel — three different files
Task: "Restore pants-example-jvm CorpusTarget in waybill-cli/tests/corpus_harness_195/manifest.rs"
Task: "Add pants_example_jvm_layer1 fn in waybill-cli/tests/corpus_harness_195/layer1_assertions.rs"
Task: "Add corpus_pants_example_jvm #[test] in waybill-cli/tests/public_corpus.rs"

# Sequential — verification chain
cargo test --test public_corpus --no-run                                          # T018
cargo test --test public_corpus                                                   # T019
WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1 \
  WAYBILL_CORPUS_SKIP_OCI=1 cargo test --test public_corpus corpus_pants_example_jvm  # T020
WAYBILL_RUN_PUBLIC_CORPUS=1 WAYBILL_CORPUS_SKIP_OCI=1 \
  cargo test --test public_corpus corpus_pants_example_jvm                        # T021
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 (Setup) — 1-task network verification.
2. Complete Phase 2 (Foundational) — apply the production fix.
3. Complete Phase 3 (US1) — smoke-scan proves the bug is closed.
4. **STOP and VALIDATE**: T008's empirical checks satisfy US1's Independent Test.
5. Ship-ready at this checkpoint. US2 and US3 add regression protection but the P1 bug is closed.

### Incremental Delivery

The whole feature is scope-tight enough to land in one PR. Splitting into two PRs (production fix + tests, then corpus target) would add review overhead without shipping-velocity gain. Recommended: one PR covering all six phases.

### Parallel Team Strategy

With one contributor (expected staffing model), the task ordering above is the natural flow. `[P]` markers in Phase 5 benefit less from multi-agent parallelism than from single-agent batching.

---

## Notes

- The production fix is ~5-10 lines of Rust deletion in a single file. This is a **scope-tight bug fix**, not a feature. SC-007 (≤ 100 line production diff) is comfortably satisfied.
- T005 updates a test-side constructor — it is production code in the "same-file-as-non-test-code" sense, but it lives inside `#[cfg(test)] mod tests`. Grouped with T003/T004 for coordination even though it's technically test code.
- Do NOT add `#[serde(deny_unknown_fields)]` to `Entry`. The fix relies on serde's default field-tolerance to silently ignore `directDependencies` at whatever shape upstream chooses. Adding the strict attribute would revert the mechanism.
- Do NOT touch `dependencies` (the sibling field carrying the transitive dep graph). Only `direct_dependencies` is unused per research R1.
- Do NOT delete the whole dead-code sink block at lines 355-362. It also silences warnings for `file_name` and `serialized_bytes_length`. Only remove the specific `let _ = &entry.direct_dependencies;` line.
- The 5 pre-existing corpus goldens in issue #763 will still show drift when run under `WAYBILL_RUN_PUBLIC_CORPUS=1` in T019 — that's independent of this PR and documented as such.
