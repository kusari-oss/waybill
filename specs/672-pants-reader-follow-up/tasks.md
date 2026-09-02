---
description: "Task list for m672 Pants pex-lockfile reader follow-up"
---

# Tasks: m672 Pants pex-lockfile reader follow-up

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/672-pants-reader-follow-up/`
**Prerequisites**: plan.md (loaded), spec.md (loaded, US1+US2+US3), research.md (R1–R7), data-model.md (4 structs), contracts/ (2 contracts), quickstart.md (7-step recipe).

**Tests**: Integration tests are IN SCOPE per spec.md's user-story acceptance scenarios. Unit tests inline in `pants/lockfile.rs` + `pants/config.rs` where called out.

**Organization**: 4-file source surface (`pants/{mod,config,lockfile}.rs` + one new test file). Tasks grouped by user story per spec priority (US1 = P1, US2 = P1, US3 = P2). Zero new Cargo dependencies.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 per spec.md
- Every task lists an absolute file path

## Path Conventions

Single Rust workspace crate: `waybill-cli` at repo root. All source edits inside `/Users/mlieberman/Projects/mikebom/waybill-cli/`.

---

## Phase 1: Setup

No new crates, no `Cargo.toml` changes, no new workspace deps. The single setup task confirms the branch state before edits begin.

- [X] T001 Verify branch `672-pants-reader-follow-up` is checked out and clean (no uncommitted m671 residue). Run `git status` and `git log --oneline -3` to confirm HEAD sits on the m672 branch created by `/speckit.specify` and main is at the m671 merge (`0c881f7 feat(m671): opt-in --file-inventory=source-tree ...`). **Confirmed**: branch `672-pants-reader-follow-up` checked out; HEAD at `0c881f7 feat(m671) ...` (m671 merge). CLAUDE.md carries the m672 spec docs (untracked spec dir + the CLAUDE.md m672 stub added by `update-agent-context.sh` during `/speckit.plan`); no code residue.

---

## Phase 2: Foundational (blocks all user-story work)

- [X] T002 [P] Add the `DiscoverySource` enum + extend `DiscoveredLockfile` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs` per `data-model.md` §"Struct 2". Add `#[derive(Debug, Clone, Copy, PartialEq, Eq)] enum DiscoverySource { DefaultGlob, PythonLockfileSingular, PythonResolvesMap }` and add `origin: DiscoverySource` to `DiscoveredLockfile`. Update every existing construction site inside `discover_lockfiles` to name the origin — the default-glob loop tags entries `DefaultGlob`, the legacy-`lockfile` singular branch tags `PythonLockfileSingular`. **No behavior change** at this task; all existing tests must still pass. `cargo +stable check -p waybill` clean.
- [X] T003 [P] Extend `PythonSection` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/config.rs` per `data-model.md` §"Struct 1". Add `resolves: BTreeMap<String, toml::Value>` with `#[serde(default)]`. Add a unit test `parse_python_resolves_map_bare_strings_only` covering: (a) empty map → empty `resolves`, (b) all-bare-string map → correct key/value population, (c) mixed bare-string + table entries → both deserialize successfully (walking + WARN happens later; here just prove `toml::from_str` succeeds). No `pants.toml`-caller changes yet.
- [X] T004 [P] Add `strip_pants_frontmatter` pure function at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/lockfile.rs` per `contracts/front_matter_stripper.md` C1–C7 + `research.md` R2. Signature: `fn strip_pants_frontmatter(bytes: &[u8]) -> &[u8]`. Add 9 inline unit tests exactly matching the contract's test matrix — one test per row. Do NOT wire it into `parse()` yet; foundational-only. **Byte-identity requirement**: on clean-JSON input (`bytes[0] == b'{'`), the function MUST return a slice pointing at the same start byte as `bytes` (contract C4).
- [X] T005 Add `LegacyShapeCounter` struct at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs` per `data-model.md` §"Struct 3". `#[derive(Debug, Default)] struct LegacyShapeCounter { count: usize }` with `record_stripped(&mut self, stripped_bytes: usize)` and `as_log_value(&self) -> usize`. Add 3 inline unit tests: (a) fresh counter reports 0, (b) `record_stripped(0)` leaves counter at 0, (c) `record_stripped(N > 0)` increments by 1 (not by N).

**Checkpoint (Phase 2)**: `cargo +stable clippy -p waybill --tests` clean. `cargo +stable test -p waybill --bin waybill scan_fs::package_db::pants::` passes with 12+ new unit tests. Existing m223 integration tests (`scan_pants_pex`) also pass unchanged — the stripper isn't wired yet.

---

## Phase 3: User Story 1 — Legacy `//`-comment lockfiles round-trip (Priority: P1)

**Goal**: The reader tolerates the pre-Pants-2.30 `//`-comment lockfile shape. Legacy files that today WARN + skip now parse successfully and emit their components.

**Independent Test**: Craft a synthetic fixture with (a) one `//`-comment-prefixed lockfile carrying real `locked_resolves` and (b) one clean 2.31+ lockfile. Assert both files emit components with correctly-tagged resolve names. Assert a `//`-prefixed file with MALFORMED body warns + skips + does not abort the scan.

### Implementation for US1

- [X] T006 [US1] Route `parse()` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/lockfile.rs` through `strip_pants_frontmatter` (contract C5 — uniform invocation). Change the signature from `pub(crate) fn parse(bytes: &[u8]) -> Option<PexLockfile>` to `pub(crate) fn parse(bytes: &[u8]) -> Option<(PexLockfile, bool /* was_legacy_shape */)>`. Compute `body = strip_pants_frontmatter(bytes)`, then `was_legacy_shape = body.len() < bytes.len()`. Pass `body` to `serde_json::from_slice` verbatim. Preserve every existing WARN message + skip path (contract inherits m223 fail-open).
- [X] T007 [US1] Thread the new `was_legacy_shape` bool through `mod.rs::read` into the `LegacyShapeCounter` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs`. On every successful `parse()` return, call `legacy_counter.record_stripped(bytes.len() - body_len)` (approximated by the bool — pass `1` if `was_legacy_shape`, else `0`). Wire the counter's `as_log_value()` into a NEW `legacy_shape_lockfiles` field in the existing `pants-pex reader complete` INFO log. **Byte-identity guard**: this task must not change the emitted CDX/SPDX bytes for any pre-m672 test — verify by running `cargo +stable test -p waybill --test scan_pants_pex` before AND after this task; the two outputs must be identical (SC-003 gate).
- [X] T008 [US1] Create the m672 integration test file at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_pants_m672.rs` with (a) file-header doc comment naming spec.md + user-story coverage, (b) `write_pants_repo` shared fixture helper per `quickstart.md` Step 5, (c) `binary_path()` helper via `env!("CARGO_BIN_EXE_waybill")`, (d) `run_scan(root, extra_args)` helper returning parsed `serde_json::Value`, (e) `#![cfg(test)]` + `#![allow(clippy::unwrap_used)]` module attributes. No tests yet — scaffolding only. Compiles clean via `cargo +stable test --no-run -p waybill --test scan_pants_m672`.
- [X] T009 [US1] Add US1 integration test `legacy_shape_lockfile_round_trips_through_stripper` to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_pants_m672.rs`. Fixture: `3rdparty/python/legacy.lock` containing the R1-observed `//`-comment header (`// This lockfile was autogenerated by Pants.\n//\n// --- BEGIN PANTS LOCKFILE METADATA ---\n// { ... }\n// --- END PANTS LOCKFILE METADATA ---\n`) followed by a valid `{"pex_version":"2.10.0","locked_resolves":[{"locked_requirements":[{"project_name":"waybill-fixture-legacy-alpha","version":"1.0.0"}, {"project_name":"waybill-fixture-legacy-beta","version":"2.0.0"}]}]}`. Assert (a) exactly 2 components emitted with those PURLs, (b) both tagged with a resolve-name annotation naming `legacy`, (c) scan-exit 0.
- [X] T010 [US1] Add US1 fail-open test `legacy_shape_malformed_body_fails_open` to the same file. Fixture: `3rdparty/python/legacy_malformed.lock` — same `//`-header block, then `{"pex_version":"2.10.0","locked_resolves":[{invalid_json` (missing quotes + close brace). Assert (a) scan-exit 0 (fail-open — m223 contract inherited), (b) 0 components emitted, (c) stderr contains a WARN naming the lockfile path + a JSON-parse error message.
- [X] T011 [US1] Add US1 clean-JSON byte-identity assertion `clean_json_lockfile_is_stripper_no_op` to the same file. Fixture: one clean-JSON lockfile at `3rdparty/python/clean.lock` containing `{"pex_version":"2.10.0","locked_resolves":[{"locked_requirements":[{"project_name":"waybill-fixture-clean-only","version":"3.0.0"}]}]}`. Assert (a) exactly 1 component emitted with the expected PURL, (b) `legacy_shape_lockfiles=0` on the reader-complete log (need to capture stderr via `Command::stderr(Stdio::piped())`; grep for the field name substring), (c) resolve name is `clean` (file-stem derivation still works for glob-discovered files).

**Checkpoint (US1)**: US1 tests pass (3 tests). m223 existing integration test suite unchanged. `cargo +stable clippy -p waybill --tests` clean.

---

## Phase 4: User Story 2 — `[python.resolves]` map override (Priority: P1)

**Goal**: `pants.toml` `[python.resolves]` map entries with bare-string values extend the discovery set beyond the default `3rdparty/python/*.lock` glob. Table-shape values WARN and skip. Map key wins over file-stem derivation on dedup.

**Independent Test**: Craft a fixture with `[python.resolves]` naming a lockfile in `build-support/py/` (outside the default glob) AND a duplicate resolve at the default glob path. Assert both are discovered but parsed once, and the map key wins.

### Implementation for US2

- [X] T012 [US2] Extend `discover_lockfiles` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs` to walk `cfg.python.resolves` after the existing legacy-singular branch. For each `(key, value)` in the map: (a) if `value.as_str()` succeeds → attempt to canonicalize `scan_root.join(value_str)` via `std::fs::canonicalize`. If canonicalize succeeds, construct a `DiscoveredLockfile { path: canonical, resolve_name: key.clone(), origin: DiscoverySource::PythonResolvesMap }` and append. If canonicalize fails (path missing / not readable), emit contract-C2 WARN naming the resolve name + the requested path + the OS error. (b) if `value` is not a bare string → emit contract-C3 WARN naming the resolve name + `value.type_str()` + the migration hint; do not append. Preserve every existing WARN in the file.
- [X] T013 [US2] Add per-scan dedup pass in `discover_lockfiles` per FR-009 + `data-model.md` §"Dedup relation". After the union of all three discovery sources (default glob + legacy singular + resolves map), canonicalize every candidate's `path` (via `std::fs::canonicalize`) and group by the canonical form. For each collision group: if any entry has `origin == PythonResolvesMap`, keep that one and drop the rest. Else, keep the lexically-first `resolve_name` (deterministic tie-breaker). **Also canonicalize the default-glob and legacy-singular paths** upstream so all three sources feed the dedup pass with the same address space.
- [X] T014 [US2] Add US2 integration test `python_resolves_map_extends_discovery_set` to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_pants_m672.rs`. Fixture: (a) `pants.toml` with `[python.resolves]\nmypy = "build-support/py/mypy.lock"\nuser_reqs = "3rdparty/python/user_reqs.lock"`, (b) both lockfiles present with valid clean-JSON bodies naming 1 synthetic package each (different `project_name`s), (c) NO default-glob candidate outside those two. Assert (a) 2 components emitted, (b) one tagged `resolve_name=mypy` (the `build-support/py/` file), (c) one tagged `resolve_name=user_reqs`.
- [X] T015 [US2] Add US2 dedup test `python_resolves_map_wins_over_default_glob_on_collision` to the same file. Fixture: `pants.toml` with `[python.resolves]\nmy-resolve = "3rdparty/python/my-resolve.lock"` AND the SAME file at that path (also matched by the default glob). Assert (a) exactly 1 component emitted (single-parse; the reader-complete log shows `lockfiles_parsed_ok=1`), (b) the emitted resolve name annotation is `my-resolve` (map wins), NOT the file-stem-derived name (which would also be `my-resolve` — deliberately choose a resolve name that differs from the file stem to make the test meaningful; use `pants.toml` naming `custom-name = "3rdparty/python/generic-file.lock"` and assert `resolve_name=custom-name`).
- [X] T016 [US2] Add US2 non-string-value test `python_resolves_table_shape_warns_and_skips` to the same file. Fixture: `pants.toml` with `[python.resolves]\nvalid-resolve = "3rdparty/python/valid.lock"\n[python.resolves.table-resolve]\npath = "3rdparty/python/table.lock"`. Assert (a) exactly 1 component emitted (only `valid-resolve`), (b) stderr contains a WARN mentioning `table-resolve` AND the observed TOML type (`table`), (c) the WARN includes the "migrate to bare-string OR file a v2 follow-up issue" hint string.
- [X] T017 [US2] Add US2 missing-path test `python_resolves_map_missing_path_warns_and_skips` to the same file. Fixture: `pants.toml` with `[python.resolves]\nexists = "3rdparty/python/exists.lock"\nghost = "3rdparty/python/ghost.lock"` — but only `exists.lock` is written to disk. Assert (a) exactly 1 component from `exists.lock`, (b) stderr contains a WARN naming both `ghost` and the missing path.
- [X] T018 [US2] Add US2 legacy-singular-union test `python_lockfile_singular_and_resolves_map_both_honored` to the same file. Fixture: `pants.toml` with `[python]\nlockfile = "build-support/legacy.lock"\n[python.resolves]\nmodern = "build-support/modern.lock"`, both files present. Assert (a) 2 components emitted, (b) one tagged `resolve_name=legacy` (file-stem derivation from the singular field per FR-006), (c) one tagged `resolve_name=modern` (map key).

**Checkpoint (US2)**: US2 tests pass (5 tests). Cumulative m672 test count: 8. `cargo +stable clippy -p waybill --tests` clean. m223 existing suite still passes unchanged.

---

## Phase 5: User Story 3 — Diagnostic log on zero-discovered path (Priority: P2)

**Goal**: When the reader is invoked on a directory that has at least one Pants signal (`3rdparty/python/` OR `pants.toml`) but discovers zero lockfiles, emit a single-line INFO diagnostic naming the outcome + the two supported override keys. When the directory has NO Pants signal, stay silent (preserves m223 SC-003).

**Independent Test**: Two scans on synthetic directories — one with a Pants signal but no lockfiles, one with no Pants signal at all. Assert the first emits a hint-containing INFO log; the second emits zero pants-pex log lines.

### Implementation for US3

- [X] T019 [US3] Refactor `mod.rs::read` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs` to replace the early-return at line 111 (`if candidates.is_empty() { return Vec::new(); }`) with a Pants-signal-gated path per FR-010/FR-011/FR-012 + `quickstart.md` Step 4. Compute `pants_signal_present = scan_root.join("3rdparty/python").exists() || scan_root.join("pants.toml").exists()` BEFORE `discover_lockfiles`. On empty candidates: if `pants_signal_present`, emit a single-line INFO `pants-pex reader complete` with `lockfiles_discovered=0` + a `hint` field naming the two supported keys (`[python].lockfile` and `[python.resolves]`); else return silently (preserves non-Pants-repo byte-identity).
- [X] T020 [US3] Add US3 integration test `zero_discovered_with_pants_signal_logs_hint` to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_pants_m672.rs`. Fixture: `pants.toml` with valid TOML but NO `[python]` section (so no override) AND NO `3rdparty/python/` directory. Assert (a) 0 components emitted, (b) scan-exit 0, (c) stderr contains the string `pants-pex reader complete` AND `lockfiles_discovered=0` AND both hint-key names (`[python].lockfile` and `[python.resolves]`).
- [X] T021 [US3] Add US3 silence test `zero_discovered_no_pants_signal_stays_silent` to the same file. Fixture: empty tempdir (or a single unrelated file like `README.md`) — no `pants.toml`, no `3rdparty/`. Assert (a) 0 components emitted, (b) scan-exit 0, (c) stderr contains NO occurrence of `pants-pex reader` (byte-identity for non-Pants repos per m223 SC-003).

**Checkpoint (US3)**: US3 tests pass (2 tests). Cumulative m672 test count: 10. `cargo +stable clippy -p waybill --tests` clean.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T022 [P] Update `/Users/mlieberman/Projects/mikebom/CLAUDE.md` "Recent Changes" section with an m672 entry describing: (a) the two additive capabilities (front-matter tolerance + `[python.resolves]` map), (b) log-only FR-013 field (no annotation, no parity work), (c) zero new Cargo deps, (d) SC-003 byte-identity gate, (e) v2 extension points (table-shape parsing, per-file DEBUG log, machine-actionable annotation). Match the m223 + m671 entry style.
- [X] T023 [P] Extend the existing m223 memory note at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_pants_pex_reader.md` (per memory `reference_pants_pex_reader`) with an m672 section documenting: (a) `//`-front-matter tolerance now built-in (2029+ default), (b) `[python.resolves]` bare-string map support (table shape deferred), (c) map key wins over file-stem on dedup, (d) `legacy_shape_lockfiles` counter in the reader-complete log for operator nudge-to-regenerate signal. Update the description line at the top-of-file to note the m672 extensions. Update the pointer entry in `MEMORY.md` if the description line changed.
- [X] T024 [P] Byte-identity guard: run the existing m223 integration test suite unchanged — `cargo +stable test -p waybill --test scan_pants_pex 2>&1 | tail`. Assert `test result: ok. N passed; 0 failed` matches the pre-m672 baseline (SC-003). If any m223 test fails, the always-strip pass introduced a regression on clean-JSON files; investigate before proceeding.
- [X] T025 Run the mandatory pre-PR gate: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 PATH="/Users/mlieberman/Projects/mikebom/.venv/spdx3-validate/bin:$PATH" ./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace` MUST pass green. Per Constitution v2.1.0 §Development Workflow.
- [X] T026 Walker-audit allowlist sanity check per memory `feedback_walker_audit_local_check`. Reproduce the CI logic locally (use `command grep` + `/usr/bin/sed` — the claude-code plugin wraps `grep`). Expected: byte-for-byte match with the pre-m672 allowlist entries — m672 does NOT add any new `fn walk[_(]` functions (the extension threads through the existing `pants/mod.rs::discover_lockfiles`, which is NOT a walker).

**Checkpoint (Phase 6)**: Docs + memory + byte-identity gate + pre-PR + walker-audit all green. Ready to open PR against main.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 — one-shot branch verification. No dependencies.
- **Foundational (Phase 2)**: T002 || T003 || T004 || T005 parallelizable across the 3 pants files. T005 (mod.rs) reads the enum from T002 (also mod.rs) — coordinate to avoid write-conflict; safer to do T002 → T005 sequentially on `mod.rs` and let T003 (config.rs) + T004 (lockfile.rs) run alongside.
- **US1 (Phase 3)**: T006 → T007 sequential (T007 depends on T006's new `parse()` return shape). T008 → T009/T010/T011 sequential (test-scaffold-then-populate).
- **US2 (Phase 4)**: T012 → T013 sequential (T013's dedup consumes T012's `PythonResolvesMap` origin). T014–T018 sequential (all in the same test file; write-conflict avoidance).
- **US3 (Phase 5)**: T019 sequential prerequisite. T020 + T021 both in the same test file — sequential.
- **Polish (Phase 6)**: T022 || T023 || T024 parallelizable (independent files). T025 last (pre-PR gate). T026 alongside T025.

### Recommended Execution Order (single contributor)

1. **T001** — verify branch (~2 min)
2. **T002 → T005** foundational (~1 hour; T002+T005 sequential on mod.rs, T003+T004 parallel on their own files)
3. **T006 → T007** wire the stripper into the parse pipeline (~30 min)
4. **T008 → T011** US1 tests (~1 hour)
5. **T012 → T013** discover_lockfiles map union + dedup (~1 hour)
6. **T014 → T018** US2 tests (~1.5 hours)
7. **T019** US3 refactor (~30 min)
8. **T020 → T021** US3 tests (~30 min)
9. **T022 → T024** parallel polish (~30 min)
10. **T025** pre-PR gate (~5–15 min depending on cache warmth)
11. **T026** walker-audit local check (~2 min)

Total: ~7 hours of focused work.

### Parallel Opportunities

- **Phase 2**: {T003, T004} run in parallel; T002 + T005 chained on `mod.rs`.
- **Phase 6**: {T022, T023, T024} parallelizable.

---

## Implementation Strategy

### MVP-First path (US1 only, if we needed to stop after Phase 3)

Deliver just US1 (front-matter tolerance) as a shippable minimum. Value: recovers the early adopter's `python-default.pants.lock` legacy file + generic pre-2.30 Pants users. The `[python.resolves]` map support in US2 is a distinct high-value addition — bundling both in one PR is cheaper than two shippable-slice PRs because they share the fixture helpers (T008) and the mod.rs discovery loop shape (T012–T013).

**Recommended**: ship US1 + US2 + US3 in one PR (as m671 did with its three user stories). All three land in a single reader — the atomic-unit is the pants reader, not the user story.

### Incremental delivery

Each phase is testable in isolation:

- After Phase 3 (US1): the reader recovers legacy-shape lockfiles even without any config change on the operator's side.
- After Phase 4 (US2): plus, operators with non-default resolve locations get their lockfiles surfaced.
- After Phase 5 (US3): plus, "why did nothing emit" support cost drops via the diagnostic log.

### Byte-identity gate placement

T024 runs the m223 test suite unchanged as the final regression guard. This gate MUST land before T025 (pre-PR). If T024 fails, one of the reader edits (most likely T006 or T007's `parse()` signature change, or T013's canonicalization pass) broke the pre-m672 output shape — do NOT regenerate goldens; fix the code first.
