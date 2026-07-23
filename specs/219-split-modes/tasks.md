---

description: "Task list for milestone 219 — --split=<mode> extensibility with directory-grouping mode"
---

# Tasks: Split-mode grouping strategies

**Input**: Design documents from `/specs/219-split-modes/`
**Prerequisites**: spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md — ALL committed on branch `219-split-modes` (commit `a741fc4` and earlier).

**Tests**: Yes — TDD-style unit tests for `SplitMode::group_key` are called out in `contracts/grouping-strategy.md`; integration tests are gated by SC-001/SC-002/SC-003/SC-005/SC-006/SC-007/SC-009; SC-005 byte-identity is load-bearing.

**Organization**: Tasks grouped by user story. MVP is US1 alone (directory-mode grouping); US2 is the extensibility gate proven mechanically via a synthetic test.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2)
- File paths in descriptions are absolute repository-relative.

## Path Conventions

Single-crate (`waybill-cli`) touch per plan.md's Project Structure section.
- Production code: `waybill-cli/src/**`
- Tests: `waybill-cli/tests/**` (integration) + `#[cfg(test)] mod tests` (unit)
- Docs: `docs/reference/**`
- Spec artifacts: `specs/219-split-modes/**`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify branch state + establish pre-implementation baselines that the SC-005 byte-identity gate leans on.

- [X] T001 Verify branch `219-split-modes` is checked out and up-to-date with `main` post-alpha.67 release. Confirm HEAD is the plan-phase commit via `git log -1 --oneline`. Expected: `a741fc4 plan(219): split-modes — plan + research + data-model + contracts + quickstart`.
- [X] T002 Capture the alpha.67 baseline for the SC-005 byte-identity gate: build the release binary at HEAD (`cargo +stable build -p waybill --release`), then run `./target/release/waybill sbom scan --path <m215-split-fixture> --split --output-dir /tmp/m219_baseline_workspace/` against a representative m215 split fixture (locate via `find waybill-cli/tests/fixtures -name '*split*' -o -path '*split*'`). Snapshot the output-dir contents (filenames + file sizes) to `/tmp/m219_baseline_manifest.txt` for later diff verification. This is the reference set that bare `--split` and `--split=workspace` post-m219 MUST reproduce byte-identically.
- [X] T003 [P] Read `waybill-cli/src/generate/split.rs` fully (~720 lines). Note the file's structure: `SubprojectRoot` at :42, `SplitProjection` at :68, `enumerate_workspace_roots` at :96, `project_for_root` at :220, `compute_shared_deps` at :329, `filename_for` at :439, `build_collision_map` at :491, `emit_split` at :564. These are the m215 surfaces m219 extends.
- [X] T004 [P] Read `waybill-cli/src/generate/split_manifest.rs` fully (~160 lines). Note `SplitManifest` at :19, `SplitEntry` at :32, `SPLIT_MANIFEST_SCHEMA_V1` at :14. This is the additive-optional surface m219 extends.
- [X] T005 [P] Read `waybill-cli/src/cli/scan_cmd.rs:436-455` (the m215 `--split` + `--output-dir` clap arg definitions) + `scan_cmd.rs:3505-3545` (the m215 `emit_split` invocation site). These are the CLI + orchestration surfaces m219 rewrites.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Type surface + CLI flag rewrite. Nothing user-facing yet — this is the substrate US1 + US2 both depend on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T006 In `waybill-cli/src/generate/split.rs`, add the `SplitMode` enum per data-model E1/E2/E3 + contracts/grouping-strategy.md. Derives: `Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum`. `#[value(rename_all = "lowercase")]`. Variants: `Workspace` (default; m215 semantics), `Directory` (m219; canonicalized source_dir grouping). Impl `group_key(&self, root: &SubprojectRoot) -> String` with match arm per variant (`Workspace` → `root.subproject_id()`; `Directory` → `root.source_dir.to_string_lossy().to_string()` with `""` → `"root"` sentinel per R4). Impl `Default` returning `SplitMode::Workspace`. **Also impl `std::fmt::Display for SplitMode`** returning the lowercase `ValueEnum` wire form via `self.to_possible_value().expect("SplitMode variants all have possible values").get_name()`. This Display impl is load-bearing for T016 (see analyze-phase B1 remediation): the FR-010 INFO log renders via `%mode` (Display), NOT `?mode` (Debug), so the operator-visible substring is `mode=directory` (lowercase, matching CLI wire form), NOT `mode=Directory` (Rust Debug of the variant name).
- [X] T007 [P] In `waybill-cli/src/generate/split_manifest.rs`, add the `SplitMember` struct + extend `SplitEntry` with the additive-optional `members: Option<Vec<SplitMember>>` field per data-model E4/E5 + contracts/manifest-additive-members.md. Use `#[serde(skip_serializing_if = "Option::is_none", default)]` on the `members` field. `SplitMember` fields: `purl: String`, `source_dir: String`. Derive `Debug, Clone, Serialize, Deserialize, PartialEq, Eq` on both.
- [X] T008 [P] In `waybill-cli/src/generate/split.rs`, add the `GroupedProjection` type per data-model E6. Fields: `group_key: String`, `members: Vec<SubprojectRoot>`, `components: Vec<ResolvedComponent>`, `relationships: Vec<Relationship>`, `shared_deps_count: usize`. `pub(crate)` visibility (parallels `SplitProjection`).
- [X] T009 In `waybill-cli/src/generate/split.rs`, add the `dir_slug(source_dir: &str) -> String` helper per contracts/multi-member-filename.md (path-separator → `-`, leading-`-` strip, m215's char-safety pass, truncate 100, lowercase, empty → `"root"` sentinel). Add 3 unit tests: (1) `services/api` → `services-api`; (2) `` (empty) → `root`; (3) uppercase + non-ASCII → sanitized.
- [X] T010 In `waybill-cli/src/cli/scan_cmd.rs`, rewrite the `--split` clap arg per data-model E8 + contracts/split-mode-flag.md. Change type from `pub split: bool` to `pub split: Option<crate::generate::split::SplitMode>`. Clap attribute stack: `#[arg(long, value_enum, num_args = 0..=1, default_missing_value = "workspace", require_equals = true, conflicts_with = "output")]`. Update `Default for ScanArgs` impl to set `split: None`. Update the `--output-dir` requirement check at `scan_cmd.rs:2368` from `if args.split && ...` to `if args.split.is_some() && ...`.

**Checkpoint**: Foundation ready. `SplitMode` + `SplitMember` + `SplitEntry.members` + `GroupedProjection` + `dir_slug` + CLI flag all exist. The CLI parses `--split=directory` without error but doesn't route it anywhere yet.

---

## Phase 3: User Story 1 - Directory-grouped split for polyglot repos (Priority: P1) 🎯 MVP

**Goal**: When `--split=directory` is passed, group all main-modules whose canonicalized source dirs match into ONE sub-SBOM per dir. Deliver SC-001 (2 files on the two_dir_polyglot fixture), SC-003 (multi-member manifest entry with `members[]` length 2), and SC-004 (no component overlap between neighbor groups).

**Independent Test**: Scan the T017 two_dir_polyglot fixture WITH `--split=directory`. `ls out/*.cdx.json | wc -l == 2` (one per dir). `jq '.entries[] | select(.source_dir | endswith("services/api")) | .members | length' == 2` on the manifest.

### Implementation for User Story 1

- [X] T011 [US1] In `waybill-cli/src/generate/split.rs`, add the `group_roots(roots: &[SubprojectRoot], mode: SplitMode) -> Vec<GroupedProjection>` function per R7 + R8. Groups by `mode.group_key(root)`, sorts members within each group lex by `purl_string`, sorts groups by `group_key`. Returns `Vec<GroupedProjection>` with `components`/`relationships` left empty (populated by T012). Add 3 unit tests: (1) workspace mode → 1 group per root (single-member each); (2) directory mode + two roots same dir → 1 group with 2 members; (3) directory mode + two dirs → 2 groups.
- [X] T012 [US1] In `waybill-cli/src/generate/split.rs`, extend `emit_split` per R8: for each `GroupedProjection`, call `project_for_root` per member, then merge the per-member projections into the group's aggregate `components` + `relationships`. Dedup rules per R10: component dedup by `purl.as_str()` (last-write-wins); relationship dedup by `(from, to, kind)` tuple via `BTreeSet`. Preserve main-module role on all members (no cross-member demotion — the existing per-BFS-projection demotion in `project_for_root:287-289` still runs per member; the merge unions post-demotion sets).
- [X] T013 [US1] Refactor `emit_split` signature per R8 to accept `mode: SplitMode` as a NEW final parameter. Update the caller in `waybill-cli/src/cli/scan_cmd.rs:3515` from `if args.split { emit_split(...) }` to `if let Some(mode) = args.split { emit_split(..., mode) }`. In the body of `emit_split`, replace the current `let mut projections: Vec<SplitProjection> = roots.iter().map(project_for_root).collect();` with `let mut groups: Vec<GroupedProjection> = group_roots(&roots, mode);` followed by the per-member BFS + merge loop from T012.
- [X] T014 [US1] In `waybill-cli/src/generate/split.rs`, extend `filename_for` (or add a parallel `filename_for_group(group: &GroupedProjection, format_id: &str) -> String` helper) per contracts/multi-member-filename.md. When `group.members.len() == 1`: reuse the existing m215 `filename_for` verbatim (SC-005 byte-identity gate). When `group.members.len() >= 2`: emit `<dir-slug>.multi.<format-ext>` where `<dir-slug>` comes from `dir_slug(&group.group_key)`. Reserved-Windows-basename guard applies to both branches.
- [X] T015 [US1] In `emit_split`'s manifest-population loop, populate `SplitEntry.members` per contracts/manifest-additive-members.md. When `group.members.len() == 1`: `members = None` (byte-identity). When `group.members.len() >= 2`: build `Some(sorted-lex-by-purl Vec<SplitMember>)`. Populate `subproject_id = <dir-slug>.multi` and `root_purl = pkg:generic/<dir-slug>@0.0.0-unknown` per R6 + E7 for multi-member groups; keep m215's single-member derivation verbatim.
- [X] T016 [US1] Emit the FR-010 INFO log at `emit_split` exit: `tracing::info!(mode = %mode, groups = groups.len(), total_main_modules = roots.len(), "split emission complete");`. **CRITICAL — use `%mode` (Display) NOT `?mode` (Debug)** — per analyze-phase B1 remediation, the T006 `Display for SplitMode` impl renders lowercase (`workspace`/`directory`), matching the CLI wire form. Debug-form (`?mode`) would render `Workspace`/`Directory` and break the SC-007 substring assertion in T023 (which asserts `mode=directory` lowercase). The log line's operator-visible substring MUST be `mode=directory` (lowercase) when `--split=directory` is passed.

### Tests for User Story 1

- [X] T017 [US1] Create the two-dir polyglot fixture at `waybill-cli/tests/fixtures/split_modes/two_dir_polyglot/`. Structure: (a) `services/api/Cargo.toml` declaring a minimal cargo package with 1-2 deps; (b) `services/api/package.json` declaring a minimal npm package with 1-2 deps; (c) `services/api/Cargo.lock` + `services/api/package-lock.json` for lockfile completeness; (d) `services/worker/go.mod` declaring a minimal Go module with 1-2 deps; (e) `services/worker/go.sum` for completeness. **Precedent for lockfile-authoring shape**: `waybill-cli/tests/fixtures/split_heterogeneous/{backend,frontend,ruby-svc}/` (m215) — copy the minimal-lockfile pattern for each ecosystem. **⚠️ FIXTURE-STATE VERIFICATION (per analyze-phase C1 remediation)**: BEFORE authoring the fixture-consumer tests (T018-T020), verify the fixture actually produces the intended state via manual smoke test: `cargo +stable build -p waybill --release && ./target/release/waybill sbom scan --path waybill-cli/tests/fixtures/split_modes/two_dir_polyglot --format cyclonedx-json | jq '[.components[] | select(.properties[]?.name == "waybill:component-role" and .properties[]?.value == "main-module") | {purl, source_dir: (.properties[]? | select(.name == "waybill:source-files") | .value)}]'`. Expected: 3 main-modules — one `pkg:cargo/api@*` + one `pkg:npm/api@*` (both with `source_dir` ending `services/api`) + one `pkg:golang/*` (with `source_dir` ending `services/worker`). If the cargo reader doesn't emit a main-module for the nested Cargo.toml, or the npm reader doesn't emit for the nested package.json, the fixture needs a different shape (candidates: use m216-style Gemfile-only main-module at scan root + package.json at scan root; use a top-level `[workspace]` Cargo.toml declaring `services/api` as a member alongside the npm package.json).
- [X] T018 [US1] Create `waybill-cli/tests/split_modes.rs` with a `run_scan` helper following the m217 `goroot_skip.rs` isolated-HOME env pattern. Then add `#[test] fn us1_split_directory_emits_two_sbom_per_dir_fixture()` — invoke `--split=directory` on the T017 fixture; assert `ls out/*.cdx.json | wc -l == 2`; assert the manifest's `entries[]` has 2 elements. Delivers SC-001.
- [X] T019 [US1] Add `#[test] fn us1_multi_member_group_entry_carries_members_field()` — same fixture, `--split=directory`; parse `split-manifest.json`; find the entry whose `source_dir` ends with `services/api`; assert `members` array length == 2; assert both members' PURLs appear (one `pkg:cargo/...`, one `pkg:npm/...`); assert members sorted lex by `purl`. Delivers SC-003.
- [X] T020 [US1] Add `#[test] fn us1_directory_mode_no_component_overlap_between_groups()` — same fixture, `--split=directory`; parse both emitted CDX files; assert the two `components[]` arrays share NO PURL. Delivers SC-004.

**Checkpoint**: US1 complete. `--split=directory` groups by canonicalized source-dir; multi-member sub-SBOMs land at `<dir-slug>.multi.<format-ext>`; manifest carries sorted `members[]` for multi-member entries.

---

## Phase 4: User Story 2 - Extensibility gate (Priority: P2)

**Goal**: Prove the enum-with-method extension contract holds mechanically. Adding a future variant (Ecosystem, Owner, Custom) requires touching only 4 surfaces (enum, group_key match arm, docs, test). Delivers SC-009.

**Independent Test**: Hand-add a `#[cfg(test)] TestOnlyEcosystem` variant + match arm + test scenario. Build succeeds. Scenario passes. If any of the "zero changes" surfaces from contracts/grouping-strategy.md needed edits, the extensibility contract is broken and the test fails at compile.

### Implementation for User Story 2

- [X] T021 [US2] In `waybill-cli/src/generate/split.rs::tests` (the existing unit-test module), add the SC-009 extensibility mechanical test. Author it with an inline `mod extensibility_gate { ... }` that: (a) defines a NEW `TestOnlySplitMode` enum with 3 variants (`Workspace`, `Directory`, `TestOnlyEcosystem`) — copy-shape of `SplitMode`; (b) implements a matching `group_key` method with a `TestOnlyEcosystem => root.ecosystem.clone()` arm; (c) invokes it against a synthetic `Vec<SubprojectRoot>` with 2 roots of different ecosystems and asserts they land in different groups. The test being in the SAME file as `SplitMode` is intentional — it proves the extension pattern doesn't require touching any file outside `split.rs`.
- [X] T022 [US2] Add `#[test] fn us2_invalid_mode_value_fails_cli_parse()` to `waybill-cli/tests/split_modes.rs`. Invoke waybill with `--split=nonexistent-mode --output-dir /tmp/should-not-exist`. Assert exit status non-zero; assert stderr contains the string `nonexistent-mode` AND both `workspace` AND `directory` (clap auto-generates a "possible values" listing). Assert `/tmp/should-not-exist` was NOT created. Delivers SC-006.
- [X] T023 [US2] Add `#[test] fn us2_info_log_carries_mode_field()` to `waybill-cli/tests/split_modes.rs`. Invoke waybill with `RUST_LOG=info` + `--split=directory` against the T017 fixture; capture stderr; assert the captured output contains the substring `mode=directory` — lowercase, matching the T006 Display impl (per analyze-phase B1 remediation; do NOT match `mode=Directory` Debug form). Delivers SC-007.

- [X] T023b [US1] Add `#[test] fn us1_directory_mode_zero_boundaries_falls_back()` to `waybill-cli/tests/split_modes.rs` (per analyze-phase E1 remediation — closes the FR-009 coverage gap for the mode-value path). Scan a fixture with NO main-modules (candidates: empty dir; dir containing only a `README.md`; dir containing only `.git/`). Invoke `--split=directory --output-dir out/`. Assert: (a) exit status success; (b) `out/split-manifest.json` is NOT created (fallback path skips the manifest); (c) exactly ONE SBOM file emitted at `out/` (the single-SBOM fallback); (d) stderr contains the m215 WARN log substring `no workspace boundaries detected` (per `split.rs:581-586`). Proves the FR-009 fallback contract extends unchanged to the new `--split=directory` mode.

**Checkpoint**: US2 complete. Extensibility mechanically proven; invalid modes reject cleanly; INFO log carries the mode field.

---

## Phase 5: SC-005 byte-identity + additional US1 coverage

**Purpose**: Lock in the load-bearing SC-005 backward-compat invariant + polish edge-case coverage. This phase is technically part of US1 (backward-compat is a P1 correctness gate) but grouped separately for coverage-atomicity.

- [X] T024 [US1] Add `#[test] fn sc002_split_workspace_emits_one_sbom_per_main_module()` to `waybill-cli/tests/split_modes.rs`. Invoke `--split=workspace` on the T017 fixture; assert `ls out/*.cdx.json | wc -l == 3`. Compare against T018's `--split=directory` count of 2 to prove the two modes produce different structures for the same input. Delivers SC-002.
- [X] T025 [US1] Add `#[test] fn sc005_bare_split_byte_identical_to_workspace()` to `waybill-cli/tests/split_modes.rs`. Invoke `--split` (bare) on the T017 fixture into `out_bare/`; invoke `--split=workspace` on the same fixture into `out_explicit/`. Assert the two output dirs are byte-identical (compare every file's SHA-256; assert the file lists match). Delivers SC-005 for the workspace-mode path.
- [ ] T026 [US1] Add `#[test] fn us1_empty_source_dir_uses_root_sentinel()` to `waybill-cli/tests/split_modes.rs`. Create a synthetic scan setup where two main-modules have empty `source_dir` (i.e., both live at scan root — e.g., a fixture with a top-level `Gemfile` + top-level `package.json`, both m216-alike main-modules). Invoke `--split=directory`; assert exactly one sub-SBOM emitted with filename `root.multi.cdx.json` (proves the empty→`"root"` sentinel from FR-005 + R4). Fixture may be hand-constructed at `waybill-cli/tests/fixtures/split_modes/single_dir_polyglot/` if a Gemfile+package.json combo works; if not, the test uses hand-constructed `PackageDbEntry` records at the resolver level.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Consumer docs (FR-011 / SC-008), SC-005 verification against every existing m215 test, pre-PR gate, PR.

- [X] T027 [P] Author `docs/reference/split-modes.md` per FR-011 + research R3 (six sections, ~150-200 lines): what the modes mean; when to choose which (decision table); worked example per mode; `split-manifest.json` schema evolution (`members[]` additive-optional); filename convention for multi-member groups (`<dir-slug>.multi.<format-ext>`); extensibility contract for contributors (the 4-file touch list from contracts/grouping-strategy.md). Cross-link from `docs/user-guide/cli-reference.md#split` if that page exists; if not, note the gap in the PR body.
- [X] T028 [P] Update `README.md`'s "SBOM interpretation" section (added for m218) to link `docs/reference/split-modes.md`. Delivers SC-008 (docs page exists + linked). Verify via `grep 'split-modes' README.md` returns a hit.
- [X] T029 SC-005 verification against the full m215 test suite: run `cargo +stable test -p waybill --test <every-existing-split-test>`. Every m215 split test MUST pass unchanged. Zero test-file edits allowed as part of this milestone (except the NEW `split_modes.rs` file added by T018). If any m215 test fails, investigate — it's a real SC-005 violation.
- [ ] T030 Pre-PR gate per Constitution: `./scripts/pre-pr.sh` — clippy `-D warnings` + `cargo test --workspace` (every suite `ok. N passed; 0 failed`). Watch for the pre-existing podman env-var race per `reference_podman_test_flake.md` memory. Read `feedback_prepr_gate_bails_on_first_failure.md` before treating any failure as a flake — use `--no-fail-fast` + enumerate every `^---- .+ stdout ----` line before claiming green.
- [ ] T031 m214 grep gate: `BADHITS=$(grep -rE '\bmikebom\b' waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml Dockerfile.ebpf-test scripts 2>/dev/null | grep -v '^Binary file' | grep -vE 'mikebom-test-fixtures' || true)`; expects zero output.
- [ ] T032 Push branch: `git push origin 219-split-modes`.
- [ ] T033 Open PR against `main` titled `impl(219): --split=<mode> extensibility with directory grouping`. PR body includes: (a) summary + link to spec/plan + why extensibility matters; (b) Test Plan enumerating every US1/US2 integration test + all unit tests + pre-PR gate + SC-005 verification against every m215 test + m214 grep gate; (c) Migration/backward-compat note (bare `--split` and `--split=workspace` are byte-identical to alpha.67; existing consumers see no change); (d) Docs link to `docs/reference/split-modes.md`; (e) Locked clarify decisions summary (additive-optional `members[]` per Q1; `<dir-slug>.multi.<format-ext>` per Q2).
- [ ] T034 CI-side verification: all 20 CI checks (linux-x86_64 default + ebpf-tracing, macOS, Windows, Kusari Inspector, 15 rootfs/language scanners) MUST pass. Merge blocked until all green. Watch for the pre-existing podman env-var race per `reference_podman_test_flake.md`; rerun failed CI job once before treating as a real regression.

---

## Dependency Graph

- **Phase 1** (T001-T005) — T001-T002 sequential; T003-T005 parallel (all read-only).
- **Phase 2** (T006-T010) — blocks all subsequent phases. T006 → T009 (dir_slug references SplitMode); T007 || T008 (different files); T010 depends on T006 (references SplitMode).
- **Phase 3 US1** (T011-T020) — depends on Phase 2 complete. T011 → T012 → T013 (chain within emit_split refactor); T014 depends on T009 + T012 (needs dir_slug + GroupedProjection); T015 depends on T014 (part of the same emit_split rewrite); T016 depends on T013 (INFO log at emit_split exit). T017 (fixture author) can proceed in parallel with T011-T016. T018-T020 test tasks depend on T017 fixture + T013-T016 emission code.
- **Phase 4 US2** (T021-T023 + T023b) — T021 depends on T006 only (independent of Phase 3). T022 + T023 depend on T010 (CLI flag routing + emit_split completion). T023b (per analyze-phase E1 remediation) depends on T013 (emit_split routing accepts mode) + T016 (INFO log emits) — it's a US1 test tagged in the US2 section for coverage-atomicity. All 4 tasks can run in parallel with Phase 5 tasks.
- **Phase 5 additional coverage** (T024-T026) — depend on Phase 3 US1 complete. All 3 tests parallel-safe (different `#[test] fn`s in same file).
- **Phase 6 Polish** (T027-T034) — T027 || T028 (different files); T029 depends on Phase 3 US1 complete (SC-005 verifies workspace-mode still produces m215 output); T030 depends on everything else; T031-T034 sequential.

## Parallel Execution Examples

- **After T002**: T003 || T004 || T005 (three read-only file surveys).
- **After T006**: T007 || T008 (different files).
- **After T017 fixture**: T018 || T019 || T020 (three tests in same file; sequential commits but concurrent authoring).
- **T027 || T028**: docs page authoring parallel with README link addition.
- **US1 vs US2 phases**: after Phase 2 completes, US1 (T011-T020) and US2 (T021) can proceed in parallel — different code surfaces.

## Implementation Strategy

**MVP scope (US1 only)**: Ship Phases 1+2+3+5 (T001-T020, T024-T026). Delivers the directory-mode grouping + SC-005 byte-identity + SC-001/SC-002/SC-003/SC-004/SC-005 gates. Extensibility (US2) is proven mechanically via T021 but the invalid-mode + INFO-log tests (T022/T023) are stretch coverage — the CLI-flag definition and INFO log emit exist regardless. **28 tasks.**

**Recommended scope (US1 + US2)**: Ship all 34 tasks. Adds SC-006 (invalid mode error) + SC-007 (INFO log) + SC-009 (extensibility mechanical test) coverage. This is the natural PR scope. **34 tasks total.**

## Format Validation

All 34 tasks follow the checklist format (`- [ ] TID [P?] [Story?] Description with file path`). Story labels present on all Phase 3-5 tasks (US1/US2); absent on Phase 1/2/6 tasks per convention. File paths absolute-repository-relative throughout. Parallel markers `[P]` applied where independence is genuine.
