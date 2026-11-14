# Tasks: `fragment` requires evidence of a live-bootstrap function

**Input**: Design documents from `specs/1503-fragment-positive-evidence/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/wire.md, quickstart.md

**Tests**: Requested. SC-005 requires a fixture control, and the project's mutation-test practice applies (spec 1492 precedent). Test tasks precede the code they cover.

**Organization**: By user story. US1 (kaem evidence plus the `package` default) can ship on its own. US2 adds the `scaffolding` rung. US3 is the five-closure measurement and the golden review.

## Standing rules for every task

- **Base.** `main` at ac2bd28. Code tasks edit `waybill-cli/src/scan_fs/package_db/nix/closure/{derivation,member}.rs` and `waybill-cli/src/cli/debug_cmd.rs` only. `graph.rs` and `emit.rs` are not edited (plan, R3).
- **Revert mutations with `cp`**, never `git checkout`.
- **Never pipe a check through `head`.** Count with `wc -l` or write to a file.
- **No closure is evaluated locally** (FR-006).
  - The five captures live outside the repo in `$CAP`, the scratchpad `captures/` directory.
  - moat: run 43535531153, sha256 `898fcdd3…44ab`.
  - The other four: run 43629708718, artifact `nix-closures-x86_64-linux`.
  - Capture files are not committed (≈10 MB each). Their run IDs and sha256s are committed.
- **Ask the binary.** Roles and rungs come from `waybill debug closure-dump`. `measurements/probe/attrs.py` only joins those rows to the `builder` and `stdenv` attributes, and runs as `python3 -I -B`.
- **The gate:** `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. No `+toolchain`, and never `cargo fmt`.
- **Commit trailer:** `Assisted-by: Claude:claude-opus-5-5`.

---

## Phase 1: Setup (captures, baseline, research R7)

**Purpose**: Settle R1 and R2 on all five closures and record the pre-change state, before any code changes.

- [X] T001 Download artifact `nix-closures-x86_64-linux` from public-corpus run 43629708718 into `$CAP` with `gh run download 43629708718 -n nix-closures-x86_64-linux -D "$CAP"`.
  - Verify with `(cd "$CAP" && sha256sum -c SHA256SUMS)`.
  - Assert that `moat-closure.json`'s sha256 equals `898fcdd38aa235840cc2a76b9ee4048ef826a853ce8abe19efdfb3076c8744ab`, the capture-method control.
  - If the moat sha differs, stop. The capture method changed, and no measurement taken from this run can be compared with run 43535531153.
  - Also read the run's "Capture nix-closure closures" step log and confirm every `cap` printed a derivation count, which means none failed silently.
- [X] T002 Build the base binary at ac2bd28 with `cargo build --release -p waybill`. Copy `target/release/waybill` to `$CAP/waybill-ac2bd28` so the before-measurements stay reproducible after code changes.
- [X] T003 [P] For each target in {moat, shellcheck, slack-web, openai, slacklinker}, run `$CAP/waybill-ac2bd28 debug closure-dump --closure-json "$CAP/<target>-closure.json"` and save the output to `specs/1503-fragment-positive-evidence/measurements/<target>/closure-dump-before.json`.
  - Record its `member_roles` object in `measurements/<target>/README.md`, along with the capture run ID and sha256.
- [X] T004 [P] At ac2bd28, for each target, run `WAYBILL_TEST_CLOSURE_JSON="$CAP/<target>-closure.json" cargo test --release --bin waybill s1367_edge_accounting -- --nocapture`. Save the accounting block to `measurements/<target>/edge-accounting-before.txt`.
  - Run moat 3 times and record whether the outputs are byte-identical.
- [X] T005 Fill research R7 in `specs/1503-fragment-positive-evidence/research.md` from T003 and the probe, run per target as `python3 -I -B specs/1503-fragment-positive-evidence/measurements/probe/attrs.py "$CAP/<target>-closure.json" measurements/<target>/closure-dump-before.json`. Record per target:
  - fragment count;
  - how many fragments have a `kaem` builder, and how many have a `stdenv` attribute;
  - how many members on the last rung have a stage0 builder that is neither `kaem` nor `bash` (R1);
  - how many fragment names end in `.sh` or `.bash`, and how many end in any other script-like extension (`.pl`, `.py`, `.rb`), with the names (R2);
  - how many fragment names end in `-hook`, with a sample of up to 10 names (R2).

  Write the per-target table into R7 and save the raw probe TSVs as `measurements/<target>/fragments-before.tsv`.
- [X] T006 Settle R1 and R2 against T005.
  - If R1 or R2 hold as written, mark them final in `research.md`.
  - If R2 needs amending (for example `.pl`, or the `-hook` population), update data-model rung 6, the R2 decision, and `contracts/wire.md` together.
  - **If the pname-less `-hook` members on openai and slack-web number more than a handful, stop and ask the user.** The question is whether FR-004's "hooks → scaffolding" governs them or cross-closure consistency with moat (`package`) does. Record the answer under the spec's Clarifications.
- [X] T007 [P] Fixture inventory, using the base binary.
  - Run `$CAP/waybill-ac2bd28 debug closure-dump --closure-json <file>` over every closure JSON under `waybill-cli/tests/fixtures/nix_closure_*/`, `waybill-cli/tests/fixtures/nix_config_closure/`, `waybill-cli/tests/fixtures/nix_ifd/` and `waybill-cli/tests/fixtures/nix_eval/`. Skip files the command rejects as not a closure, and list which those were.
  - Write every row with `member_role == "fragment"` as `file<TAB>drv<TAB>name` to `specs/1503-fragment-positive-evidence/measurements/fixture-fragments.tsv`, and print the count.
  - Then grep `waybill-cli/src/scan_fs/package_db/nix/closure/{member,graph,emit,summary}.rs`, `waybill-cli/src/cli/scan_cmd.rs` and `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs` for inline-JSON derivations used in tests that have no `pname` and no `builder`. Append them as `inline<TAB><file:line><TAB>name`.
  - Each listed member is either meant to be a live-bootstrap fragment (it gets a `kaem` builder in T014) or has no such intent (its expectation changes in T014). Note which, per row.

**Checkpoint**: R7 is filled, R1 and R2 are final, and the baselines and fixture inventory are committed. Commit: `docs(spec): 1503 research R7 from the CI captures, and baselines`.

---

## Phase 2: Foundational (parse `builder`, expose the rung; no behaviour change)

**Purpose**: The plumbing every story uses. After this phase, every role on every capture is unchanged.

- [X] T008 In `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`, add `#[serde(default)] pub(crate) builder: Option<String>` to `RawDerivation`, with a doc comment.
  - The comment cites research R5 and says `builder` is top-level in `nix derivation show`, unaffected by `__structuredAttrs`.
  - Add a unit test showing that a derivation with `"builder": "/nix/store/<32 chars>-kaem-1.9.1/bin/kaem"` parses to `Some(..)` and one without it parses to `None`.
- [X] T009 In `waybill-cli/src/scan_fs/package_db/nix/closure/member.rs`, add `pub(crate) enum MemberRung` with seven variants: `ContentHash`, `Patches`, `EnvText`, `VersionName`, `LiveBootstrapBuilder`, `ScriptName` and `Default`.
  - It has `wire()` returning the kebab-case strings in data-model.md, and `pub(crate) const ALL: [Self; 7]`.
  - Add `pub(crate) fn classify_with_rung(drv, patches) -> (MemberRole, MemberRung)` holding today's evidence order, with today's last rung returning `(Fragment, Default)`.
  - `classify` becomes `classify_with_rung(drv, patches).0`.
  - No role changes in this task.
- [X] T010 In `waybill-cli/src/cli/debug_cmd.rs`, use `member::classify_with_rung` and add `"member_role_rung"` to each member row next to `member_role` (`null` for non-members).
  - Extend `waybill-cli/tests/debug_closure_dump.rs` to assert that the field is present on every member row and that its values are drawn from `MemberRung::ALL`'s wire strings.
- [X] T011 Check that Phase 2 changes no role. On moat, run the new binary's `debug closure-dump` and compare it with `measurements/moat/closure-dump-before.json` after dropping `member_role_rung`. They must be identical. Then run the gate.
  - *Done 2026-11-14:* the moat dump is identical after dropping `member_role_rung`. Its rungs split as 549 content-hash, 5 patches, 267 env-text, 73 version-name and 240 default. **The gate moved to T016**, because `builder` and the two new rung variants are unused until US1. Clippy reports them as dead code, so a Phase 2 gate would fail for a reason US1 removes.

**Checkpoint**: Moat's roles are identical. Commit: `refactor(nix-closure): parse builder; closure-dump names the deciding rung (#1511)`.

---

## Phase 3: User Story 1 — `fragment` means live-bootstrap (Priority: P1) 🎯 MVP

**Goal**: The last rung gives `fragment` only to a `kaem`-built member. Every other member there gets `package` (rungs 5 and 7; rung 6 comes in US2).

**Independent Test**: With no other evidence, a `kaem`-built pname-less member is `fragment` and a bash-built one is `package`. On moat, `fragment` appears only with rung `live-bootstrap-builder`.

### Tests for User Story 1 (write first; they fail before T013)

- [X] T012 [US1] Add `s1503_*` tests to the `#[cfg(test)]` module of `waybill-cli/src/scan_fs/package_db/nix/closure/member.rs`:
  - `s1503_a_kaem_built_bare_member_is_a_fragment`: `{"name":"strncpy","builder":"/nix/store/<hash>-kaem-1.9.1/bin/kaem","env":{}}` gives `(Fragment, LiveBootstrapBuilder)`.
  - `s1503_a_bash_built_bare_member_is_a_package`: `nuke-refs` with a bash builder and `env.stdenv` gives `(Package, Default)` and is an edge endpoint.
  - `s1503_no_builder_is_not_evidence`: the same member without `builder` gives `Package`.
  - `s1503_the_builder_test_reads_the_file_name`: a builder path whose *store name* contains `kaem` but whose file name is `bash` (`/nix/store/<hash>-kaem-wrapper/bin/bash`) is not `Fragment`.
  - `s1503_earlier_rungs_outrank_the_builder` (FR-002): a `kaem`-built member with a content hash is `Source`, one with `env.text` is `Scaffolding`, one with a version-shaped name is `Package` via `VersionName`, and one listed in `patches` is `Source`.

  Confirm the first four fail on the Phase 2 code for the right reason (`Fragment` returned), and save the failing output to `measurements/us1-red.txt`.

### Implementation for User Story 1

- [X] T013 [US1] In `classify_with_rung` in `waybill-cli/src/scan_fs/package_db/nix/closure/member.rs`, replace the last rung:
  - if `builder` is present and its final `/`-separated component is exactly `"kaem"`, return `(Fragment, LiveBootstrapBuilder)`;
  - otherwise return `(Package, Default)`.

  Update the `classify` doc comment's evidence-order paragraph and the `MemberRole::Package` and `MemberRole::Fragment` variant docs to data-model.md's definitions.
- [X] T014 [US1] Fix every test the change breaks, guided by `measurements/fixture-fragments.tsv` (T007).
  - Run `cargo test --workspace --no-fail-fast`. Write the list of failing tests (`^---- .+ stdout ----` lines) to `measurements/us1-broken-tests.txt` and count it.
  - For each failure:
    - **The member stands for a live-bootstrap fragment** (`strncpy`, `memcpy`, `crt-builder`-style names, or `m1282_t011`'s two fragments): add a `kaem` `builder` to the fixture JSON or inline derivation.
    - **The test asserts walk-through of an unevidenced member**, for example the control in `graph.rs`'s `s1492_the_walk_stops_at_a_listed_patch`, which walks through an *unlisted* patch: give that member a `kaem` builder, so the control still exercises walk-through, and say so in a comment citing #1511.
    - **The assertion encodes the old default** (`m1282_t012_a_bare_name_is_a_fragment`, line ~415; `m1282_t050`, ~453; `m1282_t051`, ~505; `s1492_an_unlisted_twin_stays_fragment`, ~716): split it into a `kaem`-built twin that stays `Fragment` and a builder-less twin that is `Package`. Rename tests whose name now lies.
    - **`classify_before_1380`, and the `s1380_only_package_members_move_and_only_between_endpoints` test that compares against it:** give the historical classifier the same rungs 5 and 7. That test asserts the version rung's split, not the last rung, so the comparison must not count this feature's moves. State that in the doc comment.
  - Never weaken an assertion to make it pass.
  - Record each fix as one line in `measurements/us1-broken-tests.txt`: test, cause, fix.
- [X] T015 [US1] SC-005 control.
  - Copy `member.rs` to `$CAP/member.rs.1503`.
  - Mutate the builder test to always return true, which restores the old fall-through. Run `cargo test --bin waybill s1503`, and confirm `s1503_a_bash_built_bare_member_is_a_package` and `s1503_no_builder_is_not_evidence` fail.
  - Mutate the test to always return false. Confirm `s1503_a_kaem_built_bare_member_is_a_fragment` fails.
  - Restore with `cp "$CAP/member.rs.1503" waybill-cli/src/scan_fs/package_db/nix/closure/member.rs` after each mutation, and confirm `git diff --stat` matches the pre-mutation state.
  - Write both mutations, the failing test names and the restore check to `specs/1503-fragment-positive-evidence/measurements/mutations.md`.
- [X] T016 [US1] Run the gate. *2026-11-14: the gate fails at clippy with `variant ScriptName is never constructed` (dead code is denied). That variant is US2's rung 6, so the gate moved to T020 and US1 was committed locally without it, unpushed. The workspace suite was green on the T014 fixes for the closure targets.* Commit: `fix(nix-closure): fragment requires a kaem builder; unevidenced members are package (#1511)`.

**Checkpoint**: US1 is complete and testable on its own. Every `fragment` is `kaem`-built, and the graph stops at former fragments, including the scripts, which US2 moves.

---

## Phase 4: User Story 2 — the leavers get an evidenced role (Priority: P1)

**Goal**: Hook and helper scripts become `scaffolding` (rung 6, as R2 was settled in T006). `stdenv-linux` and the stages are `package`, and the walk stops there.

**Independent Test**: `setup-hook.sh` built by bash is `(Scaffolding, ScriptName)`. `stdenv-linux` built by bash is `(Package, Default)`. A projection over a consumer → `stdenv-linux` → `gcc` closure emits consumer → `stdenv-linux` and not consumer → `gcc`.

### Tests for User Story 2 (write first)

- [X] T017 [P] [US2] Add tests in the `#[cfg(test)]` module of `waybill-cli/src/scan_fs/package_db/nix/closure/member.rs`:
  - `s1503_a_hook_script_is_scaffolding`: `setup-hook.sh`, `utils.bash` and `python-setup-hook.sh`, each bash-built, give `(Scaffolding, ScriptName)`.
  - `s1503_programs_data_and_stages_are_package`: `nuke-refs`, `expand-response-params`, `mirrors-list`, `bootstrap-stage0-glibc-iconv-minimal-bootstrap` and `stdenv-linux` give `(Package, Default)`.
  - `s1503_the_builder_outranks_the_script_name`: a `kaem`-built `foo.sh` is `Fragment`, and the order is stated in a comment.
  - `s1503_a_suffix_is_not_a_substring`: a member named `sh-utils` or `bash` is `Package`.

  Add one case for each extension R2 settled on in T006.
- [X] T018 [P] [US2] Add `s1503_the_walk_stops_at_stdenv_linux` to the test module of `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs`. It uses a constructed closure: `app` (pname) → `stdenv-linux` (no pname, bash builder) → `gcc` (pname).
  - Assert that the projection contains `app → stdenv-linux` and not `app → gcc`.
  - **CONTROL:** the same closure with `stdenv-linux` given a `kaem` builder walks through it and yields `app → gcc`. This proves the test can see the difference.

### Implementation for User Story 2

- [X] T019 [US2] In `classify_with_rung` in `waybill-cli/src/scan_fs/package_db/nix/closure/member.rs`, insert rung 6 between the builder test and the default: if the name ends with one of R2's extensions, return `(Scaffolding, ScriptName)`.
  - Keep the extension list a `const` slice with a doc comment citing R2's measured table.
  - Update the `MemberRole::Scaffolding` variant doc to data-model.md's definition.
- [X] T020 [US2] Re-run `cargo test --workspace --no-fail-fast`. Any newly failing test is handled under T014's rules, and the outcome is appended to `measurements/us1-broken-tests.txt` under a `US2` heading. Then run the gate. Commit: `fix(nix-closure): hook and helper scripts are scaffolding (#1511)`.

**Checkpoint**: Rungs 5–7 are complete, and the T018 control proves the stop at `stdenv-linux`.

---

## Phase 5: User Story 3 — the edge change, measured and reviewed (Priority: P1)

**Goal**: For each of the five closures, every role change and every added or removed edge is listed and attributed (FR-007, SC-001 to SC-004). The goldens are regenerated and reviewed (FR-010, SC-006).

**Independent Test**: For each target, `delta.md` attributes all differing edges to reclassified members, with 0 unexplained.

- [X] T021 [P] [US3] With the post-US2 binary, run `debug closure-dump` on each capture and save it as `measurements/<target>/closure-dump-after.json`.
  - From it, write `measurements/<target>/default-members.tsv`: every row with `member_role_rung == "default"`, as drv, name and the probe's builder column.
  - Run moat's dump 3 times and require byte-identical output.
- [X] T022 [P] [US3] With the post-US2 code, run the `s1367` accounting (as T004) on each capture and save it as `measurements/<target>/edge-accounting-after.txt`. Run moat 3 times.
- [ ] T023 [US3] For each target, write `measurements/<target>/delta.md`.
  - **Role transitions**, by joining before and after dump rows on `drv` and counting each transition (for example `fragment → package`).
  - **Assert:** every changed row was `fragment` before. Any other transition is a defect, and the task stops.
  - **The bucket deltas** from the accounting before and after, attributed as in `specs/1492-patch-member-source/measurements/edge-delta.md`.
  - **SC-001 (moat):** exactly 25 leave `fragment`, 10 to `scaffolding` and 15 to `package`, with the names matching spec Background. 215 stay.
  - **SC-002:** on every target, every `fragment` row has rung `live-bootstrap-builder`.
  - **SC-004 (openai, slack-web):** the edges each spec-1492 patch contributes before and after, and whether the `stdenv-linux` fan-out remains.
  - Write the five-target summary table to `measurements/edge-delta.md`.
- [ ] T024 [US3] Push the branch. Dispatch `gh workflow run public-corpus.yml --ref main -f branch=1503-fragment-positive-evidence -f regen_goldens=true`.
  - Watch the run with `gh run watch <id>` in the background. When it finishes, enumerate every job's conclusion.
  - Download the `corpus-goldens-regen` artifact. Confirm it holds 67 files, the same set as `waybill-cli/tests/fixtures/public_corpus/`, missing nothing in either direction.
- [ ] T025 [US3] Review the regenerated goldens and write `specs/1503-fragment-positive-evidence/measurements/golden-review.md` in the shape of `specs/1492-patch-member-source/measurements/golden-review.md`.
  - **SC-006:** exactly the 15 closure SBOM files differ. The 49 other files and the three `recall-truth.json` are byte-identical.
  - **Per closure target**, compare CDX components (recursively, nested included), `dependencies[].dependsOn` edges and properties, using a normalized `LC_ALL=C`-sorted diff.
  - Every changed component must be a member in that target's `delta.md`, and its only changed properties must be `waybill:closure-member-role`, `waybill:orphan-reason` and the explained C184 and graph-completeness values.
  - Every changed edge must touch a reclassified member (SC-003).
  - The C184 `edges` value must equal the `edge-accounting-after.txt` "projected, identity level" figure.
  - No `bom-ref` or PURL may change (R4).
  - Run `waybill sbom parity-check` on the old and new triples, and confirm the parity result is unchanged (FR-008).
  - List every explained difference beyond FR-007's list, as 1492's review did.
- [ ] T026 [US3] Copy the regenerated closure goldens into `waybill-cli/tests/fixtures/public_corpus/nix-closure-*/`, the 15 files only.
  - Commit with the measurements: `test(corpus): regenerate the nix-closure goldens for #1511, reviewed edge by edge`.

**Checkpoint**: SC-001 to SC-004 and SC-006 are each stated with their measured value in `measurements/`.

---

## Phase 6: Polish & cross-cutting

- [ ] T027 [P] Edit the C206 row in `docs/reference/sbom-format-mapping.md` per `contracts/wire.md`.
  - Change the evidence-order sentence to end with the three rungs.
  - Use data-model.md's definitions for `package`, `scaffolding` and `fragment`.
  - Replace "`fragment` is now said only of live-bootstrap functions" with a sentence true of all five regenerated goldens. Check it against T023's SC-002 result.
  - Make sure the parity extractor for C206 still matches. Read it before editing the row.
- [ ] T028 [P] In `docs/reference/nix-evaluation.md`, update the role table (definitions, and the moat counts from T021), the evidence-order paragraph, and any sentence that says `fragment` is the fall-through or cites #1502/#1511 as open.
- [ ] T029 Add a `Changed` entry under Unreleased in `CHANGELOG.md`, placed so it is not adjacent to any other unreleased entry added by an open PR. Cite #1511 and the measured moat transitions.
- [ ] T030 Run the gate one final time. Confirm the log's last lines read `GATE_EXIT=0` and `>>> all pre-PR checks passed.`, and keep the per-target `test result: ok. N passed; 0 failed` lines for the PR body.
- [ ] T031 Verify every task's claim, not just its tick.
  - Re-read each `[X]` task and check that its artifact exists and says what the task requires.
  - Compare the `measurements/` file list with the task list.
- [ ] T032 Open the PR against `main`, titled `fix(nix-closure): fragment requires live-bootstrap evidence (#1511)`. The body must:
  - contain `Fixes #1511`;
  - give the five-target table from `measurements/edge-delta.md`;
  - quote the gate's lines;
  - end with the `---` attribution line.

  The PR must contain no session URLs and no tool comparisons.
- [ ] T033 Delete the branch-only capture branch with `git push origin --delete 1503-closure-capture` and `git worktree remove` for its worktree. This runs after T001's artifacts are saved and T024's run has passed.

---

## Dependencies & Execution Order

- **Phase 1** blocks everything. T001 needs run 43629708718 to have finished. T005 and T006 settle R2, which T017 and T019 depend on.
- **Phase 2** (T008 → T009 → T010 → T011) blocks US1 and US2.
- **US1** (T012 → T013 → T014 → T015 → T016) comes before **US2** (T017/T018 → T019 → T020). Both edit `member.rs`, so they run in sequence, not in parallel.
- **US3** needs US1 and US2. T021 and T022 can run in parallel, and T023 needs both. T024 → T025 → T026.
- **Polish:** T027 and T028 can run in parallel after T023, since they need measured counts. T029, then T030 → T031 → T032. T033 comes last.

### Parallel opportunities

- T003, T004 and T007 use different outputs and the base binary.
- T017 (member.rs) and T018 (graph.rs) touch different files.
- T021 and T022 are separate measurements.
- T027 and T028 are separate docs.

## Implementation Strategy

1. **Phase 1 first, in full.** The rule for rung 6 is not final until the captures are read (T006), and T006 may need a user decision.
2. **MVP = US1.** It alone makes the C206 claim true. Scripts would sit at `package`, the conservative choice, until US2.
3. **US2, then US3's measurement and golden regeneration in one CI dispatch.** Regenerating goldens after US1 alone would only be thrown away.
4. **One PR, with the stack considered.** If another PR is queued ahead, stack by merging its exact head, base `main`, with CHANGELOG entries not adjacent.
