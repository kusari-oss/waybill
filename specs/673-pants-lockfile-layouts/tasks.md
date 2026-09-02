---
description: "Task list for m673 Pants lockfile discovery layout extensions"
---

# Tasks: m673 Pants lockfile discovery layout extensions

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/673-pants-lockfile-layouts/`
**Prerequisites**: plan.md (loaded), spec.md (loaded, US1+US2+US3), research.md (R1–R7), data-model.md (2 enum extensions + 1 pure function), contracts/ (2 contracts), quickstart.md (8-step recipe).

**Tests**: Integration tests are IN SCOPE per spec.md's user-story acceptance scenarios. Unit tests inline in `pants/lockfile.rs` for the content-detection matrix.

**Organization**: 2-file source surface (`pants/{mod,lockfile}.rs`) + one new integration test file. Tasks grouped by user story per spec priority (US1 = P1, US2 = P1, US3 = P2). Zero new Cargo dependencies.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 per spec.md
- Every task lists an absolute file path

## Path Conventions

Single Rust workspace crate: `waybill-cli` at repo root. All source edits inside `/Users/mlieberman/Projects/mikebom/waybill-cli/`.

---

## Phase 1: Setup

No new crates, no `Cargo.toml` changes, no new workspace deps. Single setup task confirms the branch state before edits begin.

- [X] T001 Verify branch `673-pants-lockfile-layouts` is checked out and clean (no uncommitted m672 residue on main). Run `git status` and `git log --oneline -3` to confirm HEAD sits on the m673 branch created by `/speckit.specify` and main is at the m672 amended merge (`5ae2698 feat(m672): Pants pex-lockfile reader follow-up ...`).

---

## Phase 2: Foundational (blocks all user-story work)

- [X] T002 [P] Extend the `DiscoverySource` enum at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs` per `data-model.md` §"Enum 1". Add two new variants: `RepoRootGlob` (m673 FR-001) and `LockfilesGlob` (m673 FR-002). Update the m672 `dedup_by_canonical_path` winner-selection rule to include them as tied peers with `DefaultGlob` per `research.md` §R4 (precedence: `PythonResolvesMap` > `PythonLockfileSingular` > {`DefaultGlob`, `RepoRootGlob`, `LockfilesGlob`}). **No behavior change** at this task — the new variants aren't constructed until T004/T008. `cargo +stable check -p waybill` clean. Add `#[allow(dead_code)]` guards on the new variants if clippy complains — remove them at T004/T008 wire-in.
- [X] T003 [P] Add `is_pex_lockfile_content` pure function at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/lockfile.rs` per `contracts/content_detection.md` C1–C7 + `data-model.md` §"Function 1". Signature: `pub(crate) fn is_pex_lockfile_content(bytes: &[u8]) -> bool`. Implementation reuses m672 `strip_pants_frontmatter` then parses to `serde_json::Value` then checks `pex_version` string prefix per contract C1/C6. Add inline unit tests covering the 15-row test matrix from `contracts/content_detection.md` — one test per row (clean PEX accept, PEX with `//`-frontmatter, Pex 1.9 reject, Pex 2.0 pre-release accept, hypothetical Pex 3.x reject, Cargo TOML reject, Poetry TOML reject, bun JSONC reject, empty file reject, empty-object reject, integer `pex_version` reject, null `pex_version` reject, top-level-array reject, unterminated JSON reject, binary garbage reject).

**Checkpoint (Phase 2)**: `cargo +stable clippy -p waybill --tests` clean. `cargo +stable test -p waybill --bin waybill scan_fs::package_db::pants::lockfile::tests::is_pex` passes with 15 new unit tests. Existing m223 + m672 tests still pass unchanged (the new variants aren't constructed yet).

---

## Phase 3: User Story 1 — Repo-root `<resolve>.lock` discovery (Priority: P1)

**Goal**: `<scan_root>/*.lock` files that content-detect as PEX lockfiles get discovered + emit their components with `resolve_name` derived from the file stem.

**Independent Test**: Craft a synthetic fixture with `<repo-root>/pants.toml` (no `[python.resolves]` map) AND `<repo-root>/python-default.lock` (PEX shape with `//`-frontmatter + N synthetic packages). Assert the scan emits N components with `pkg:pypi/*` PURLs + `waybill:pants-resolve=python-default` annotation, without needing any `pants.toml` override.

### Implementation for US1

- [X] T004 [US1] Add the repo-root discovery loop in `discover_lockfiles` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs` per `contracts/discovery_paths.md` path 4 + `quickstart.md` Step 3. Enumerate `<scan_root>/*.lock` (non-recursive), read each file's bytes, gate via `lockfile::is_pex_lockfile_content` — files that FAIL the gate SILENT-skip (no WARN, no counter increment). Files that PASS append to the candidate list with `origin: DiscoverySource::RepoRootGlob` and `resolve_name` derived from `path.file_stem()`. Place this loop AFTER the existing `[python.resolves]` map walk but BEFORE the final `dedup_by_canonical_path(out)` call. Remove any `#[allow(dead_code)]` guard on `RepoRootGlob` from T002.
- [X] T005 [US1] Create the m673 integration test scaffold at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_pants_m673.rs` per `quickstart.md` Step 5. Reuse the m672 helper shape: `#![cfg(test)]` + `#![allow(clippy::unwrap_used)]` module attributes, `binary_path()` via `env!("CARGO_BIN_EXE_waybill")`, `strip_ansi()` for tracing log parsing, `run_scan(root, extra_args)` → `(Value, String)` for parsed CDX + stderr, `write_pants_repo(root, layout)` fixture helper, `component_purls(doc)` for sorted-lex purl-list assertions, `synth_clean_lockfile(packages)` for building minimal PEX bodies with PyPI artifact URLs, `synth_legacy_lockfile(packages)` for the `//`-frontmatter variant. No tests yet — scaffolding only. Compiles clean via `cargo +stable test --no-run -p waybill --test scan_pants_m673`.
- [X] T006 [US1] Add US1 integration test `repo_root_lockfile_discovered` to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_pants_m673.rs`. Fixture: `<root>/pants.toml` (valid TOML, NO `[python.resolves]` map) + `<root>/python-default.lock` (built via `synth_legacy_lockfile` naming 3 synthetic packages). Assert (a) exactly 3 pypi components emitted with `pkg:pypi/*` PURLs matching the 3 synthetic packages, (b) each tagged with `waybill:pants-resolve=python-default` via the m223 annotation channel, (c) reader-complete INFO log shows `lockfiles_discovered=1 lockfiles_parsed_ok=1 legacy_shape_lockfiles=1` (the m672 counter fires because the fixture uses `//`-frontmatter), (d) scan-exit 0.
- [X] T007 [US1] Add US1 multi-lockfile integration test `multiple_repo_root_lockfiles_discovered_with_stem_names` to the same file. Fixture: `<root>/python-default.lock` (1 synthetic package) + `<root>/mypy.lock` (1 different synthetic package) + `<root>/pytest.lock` (1 more) — all built via `synth_clean_lockfile` (no `//`-frontmatter this time, to prove that shape is also accepted). Assert (a) exactly 3 pypi components, one per lockfile, (b) each tagged with the correct `waybill:pants-resolve` value derived from the filename stem (`python-default`, `mypy`, `pytest`), (c) reader-complete log shows `lockfiles_discovered=3 lockfiles_parsed_ok=3 legacy_shape_lockfiles=0`.

**Checkpoint (US1)**: US1 tests pass (2 tests). m223 + m672 existing integration tests unchanged. `cargo +stable clippy -p waybill --tests` clean.

---

## Phase 4: User Story 2 — `lockfiles/` directory discovery (Priority: P1)

**Goal**: `<scan_root>/lockfiles/*.lock` files that content-detect as PEX lockfiles get discovered + emit their components. FR-006 signal detection extends to include `<scan_root>/lockfiles/` directory existence.

**Independent Test**: Craft a fixture with `<repo-root>/lockfiles/python-default.lock` + `<repo-root>/lockfiles/mypy.lock`. Scan; assert both files emit their components with correctly-tagged resolve names.

### Implementation for US2

- [X] T008 [US2] Add the `lockfiles/` discovery loop in `discover_lockfiles` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs` per `contracts/discovery_paths.md` path 5 + `quickstart.md` Step 3. Enumerate `<scan_root>/lockfiles/*.lock` (immediate children of the `lockfiles/` directory only — non-recursive per FR-009), read + gate + append candidates identically to T004 but with `origin: DiscoverySource::LockfilesGlob`. Place this loop immediately after T004's repo-root loop. Remove the `#[allow(dead_code)]` guard on `LockfilesGlob` from T002.
- [X] T009 [US2] Extend FR-006 signal detection at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs::read` per `contracts/discovery_paths.md` C7 + `quickstart.md` Step 4. Add a new `lockfiles_dir_exists = scan_root.join("lockfiles").exists()` check and include it in the m672 `pants_signal_present` disjunction (`default_dir_exists || pants_toml_exists || lockfiles_dir_exists`). This ensures a repo that has ONLY a `lockfiles/` directory (no `pants.toml`, no `3rdparty/python/`) still fires the m672 US3 zero-discovered diagnostic INFO log when discovery finds nothing usable.
- [X] T010 [US2] Add US2 integration test `lockfiles_directory_layout_discovered` to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_pants_m673.rs`. Fixture: `<root>/pants.toml` (empty `[GLOBAL]` section, no `[python]`) + `<root>/lockfiles/python-default.lock` + `<root>/lockfiles/mypy.lock` (both built via `synth_clean_lockfile` naming distinct synthetic packages). Assert (a) exactly 2 pypi components, (b) resolve names match filename stems (`python-default`, `mypy`), (c) reader-complete log shows `lockfiles_discovered=2 lockfiles_parsed_ok=2`.
- [X] T011 [US2] Add US2 mixed-content test `lockfiles_dir_ignores_non_lock_files` to the same file. Fixture: `<root>/lockfiles/README.md` (arbitrary markdown content) + `<root>/lockfiles/python-default.lock` (valid PEX via `synth_clean_lockfile`). Assert (a) exactly 1 pypi component (from the PEX lockfile only), (b) NO WARN in stderr about `README.md`, (c) reader-complete log shows `lockfiles_discovered=1 lockfiles_parsed_ok=1` (README.md's non-`.lock` extension excludes it from discovery entirely).

**Checkpoint (US2)**: US2 tests pass (2 tests). Cumulative m673 test count: 4. m223 + m672 existing integration tests still pass unchanged.

---

## Phase 5: User Story 3 — Content-detection defensive guard (Priority: P2)

**Goal**: Non-PEX `.lock` files (Cargo, Poetry, bun) at repo-root OR under `lockfiles/` MUST silent-skip through the Pants reader — NO WARN, NO false-positive component, NO counter increment. Downstream readers (cargo, pip-poetry) handle those files normally.

**Independent Test**: Fixture with `<repo-root>/Cargo.lock` (real cargo shape) + `<repo-root>/lockfiles/poetry.lock` (real poetry shape) — both `.lock` files but neither is a PEX lockfile. Scan; assert (a) the Pants reader emits NO components from those files, (b) the Pants reader emits NO WARN about them, (c) the Pants reader-complete log does NOT count those files in `lockfiles_discovered`.

### Implementation for US3

- [X] T012 [US3] Add US3 defensive test `content_detection_silent_skips_cargo_and_poetry` to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_pants_m673.rs`. Fixture: `<root>/Cargo.lock` (real cargo shape: `version = 3\n[[package]]\nname = "waybill-fixture-c1"\nversion = "1.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n[[package]]\nname = "waybill-fixture-c2"\nversion = "2.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n`) + `<root>/lockfiles/poetry.lock` (real poetry shape: `[metadata]\nlock-version = "2.0"\npython-versions = "^3.10"\n[[package]]\nname = "waybill-fixture-p1"\nversion = "1.0.0"\n`) + `<root>/pyproject.toml` (minimal, to activate the poetry-shape recognition in the pip reader). Assert (a) the Pants reader emits ZERO components from those files (verify by checking that no emitted component has `waybill:pants-resolve` annotation), (b) stderr contains NO `pants-pex reader: failed to parse` WARN — grep for the substring "failed to parse Pex lockfile as JSON" and assert zero matches, (c) reader-complete log shows `lockfiles_discovered=0` OR the log is absent entirely (both are acceptable — the fixture may or may not trip signal detection depending on whether `<root>/lockfiles/` counts even when non-PEX-only).
- [X] T013 [US3] Add US3 discovery-count assertion `repo_root_non_pex_lockfile_silent_skipped` to the same file. Fixture: `<root>/Cargo.lock` (real cargo shape as in T012) + `<root>/python-default.lock` (valid PEX via `synth_clean_lockfile` naming 1 synthetic package). Assert (a) exactly 1 pypi component (from the PEX lockfile), (b) `lockfiles_discovered=1` in the reader-complete log (the Cargo.lock DID NOT contribute), (c) stderr contains NO WARN about Cargo.lock from the Pants reader, (d) the cargo reader handled Cargo.lock — verify via a cargo-emitted component being present (or, at minimum, no cargo-reader errors in stderr). This test is the anti-regression guard for FR-004 silent-skip semantics.

**Checkpoint (US3)**: US3 tests pass (2 tests). Cumulative m673 test count: 6. `cargo +stable clippy -p waybill --tests` clean.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T014 [P] Update `/Users/mlieberman/Projects/mikebom/CLAUDE.md` "Recent Changes" section with an m673 entry describing: (a) two new discovery paths (repo-root + `lockfiles/`), (b) FR-003 `pex_version` content-detection gate for the wide-scope paths, (c) FR-004 silent-skip policy vs m223's WARN-and-skip policy (narrow-scope), (d) FR-006 signal-detection extension (`lockfiles/` directory existence), (e) empirical grounding (`pantsbuild/example-python` was 0 components pre-m673; now ≥ 8), (f) zero new Cargo deps, (g) SC-005 byte-identity gate. Match the m223 + m672 entry style.
- [X] T015 [P] Extend the existing m223 + m672 memory note at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_pants_pex_reader.md` with an m673 section documenting: (a) the three canonical Python-lockfile layouts + why they exist (research.md §R1 empirical validation), (b) `is_pex_lockfile_content` pure-function gate + `pex_version` prefix-match discriminator, (c) FR-004 silent-skip vs m223 WARN-and-skip (per the 2026-09-02 clarify Q1), (d) FR-006 signal-detection extension. Update the top-of-file description line to name m673. Update the MEMORY.md pointer entry if the description line changed.
- [X] T016 [P] Byte-identity guard: run BOTH `cargo +stable test -p waybill --test pants_pex_reader` (m223 goldens — must show `test result: ok. 10 passed; 0 failed`) AND `cargo +stable test -p waybill --test scan_pants_m672` (m672 tests — must show `test result: ok. 10 passed; 0 failed`). Both must pass without any regeneration. If either fails, m673's discovery loop drift changed pre-m672 behavior; investigate + fix — do NOT regenerate goldens.
- [X] T017 Real-world smoke test per `quickstart.md` Step 7. Clone `pantsbuild/example-python` + `pantsbuild/example-django`, scan each with `--offline` + `--no-deep-hash` + `RUST_LOG=info`, and assert: (a) `example-python` emits ≥ 8 pypi components from the actual lockfile (was 0 pre-m673 from Pants reader), (b) `example-django` emits > 20 pypi components (Django's transitive closure), (c) both scans exit 0, (d) both scans emit a `pants-pex reader complete` INFO log. Save the smoke-test outputs to `specs/673-pants-lockfile-layouts/artifacts/smoke-<repo>-<date>.log` for the PR body.
- [X] T018 Run the mandatory pre-PR gate: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 PATH="/Users/mlieberman/Projects/mikebom/.venv/spdx3-validate/bin:$PATH" ./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace` MUST pass green. Per Constitution v2.1.0 §Development Workflow.
- [X] T019 Walker-audit allowlist sanity check per memory `feedback_walker_audit_local_check`. Reproduce the CI logic locally (use `command grep` + `/usr/bin/sed` — the claude-code plugin wraps `grep`). Expected: byte-for-byte match with the pre-m673 allowlist entries (12 entries) — m673 does NOT add any new `fn walk[_(]` functions (the extension threads through `pants/mod.rs::discover_lockfiles` which is NOT a walker; content-detect is a pure function).

**Checkpoint (Phase 6)**: Docs + memory + byte-identity gate + real-world smoke + pre-PR + walker-audit all green. Ready to open PR against main.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 — one-shot branch verification. No dependencies.
- **Foundational (Phase 2)**: T002 || T003 parallelizable across the 2 pants files (mod.rs + lockfile.rs). Both must land before any US work touches `discover_lockfiles`.
- **US1 (Phase 3)**: T004 sequential prerequisite (extends `discover_lockfiles`). T005 → T006/T007 sequential (test-scaffold-then-populate).
- **US2 (Phase 4)**: T008 sequential prerequisite (extends `discover_lockfiles` — depends on T004's placement). T009 sequential (edits `mod.rs::read`). T010 + T011 in the same test file — sequential.
- **US3 (Phase 5)**: T012 + T013 in the same test file — sequential. Both depend on T004 + T008 having landed (they test the silent-skip behavior of the T004/T008 loops).
- **Polish (Phase 6)**: T014 || T015 || T016 parallelizable (independent files). T017 depends on the release binary being current — run after T016 confirms tests pass. T018 last (pre-PR gate). T019 alongside T018.

### Recommended Execution Order (single contributor)

1. **T001** — verify branch (~2 min)
2. **T002 + T003** foundational (~1 hour; T003 has 15 unit tests — bulk of the time)
3. **T004** wire repo-root discovery loop (~15 min)
4. **T005 → T007** US1 tests (~1 hour)
5. **T008 + T009** wire `lockfiles/` discovery + FR-006 signal extension (~30 min)
6. **T010 + T011** US2 tests (~30 min)
7. **T012 + T013** US3 defensive tests (~30 min)
8. **T014 → T016** parallel polish (~30 min)
9. **T017** real-world smoke test (~10 min including clone time)
10. **T018** pre-PR gate (~5–15 min depending on cache warmth)
11. **T019** walker-audit local check (~2 min)

Total: ~5 hours of focused work.

### Parallel Opportunities

- **Phase 2**: {T002 mod.rs, T003 lockfile.rs} run in parallel — different files.
- **Phase 6**: {T014 CLAUDE.md, T015 memory-note, T016 byte-identity-gate} parallelizable.

---

## Implementation Strategy

### MVP-First path (US1 only, if we needed to stop after Phase 3)

Deliver just US1 (repo-root discovery + content-detection gate) as a shippable minimum. Value: fixes `pantsbuild/example-python` (Pants's own #1 tutorial repo) and every Pants 2.31+ default-layout monorepo without `[python.resolves]`. US2 is a distinct high-value addition (fixes `example-django` + multi-resolve setups with dedicated `lockfiles/` dir). US3 is a defensive guard — CAN'T ship US1 without it (because US1's wide-scope enumeration would false-positive-WARN on every Rust repo's `Cargo.lock` without content-detection).

**Recommended**: ship US1 + US2 + US3 in one PR (matches m671 + m672 shape). All three land in a single reader; splitting would create fixture-helper duplication for no operational benefit.

### Incremental delivery

Each phase is testable in isolation:

- After Phase 3 (US1): the reader picks up Pants 2.31+ default-layout lockfiles at the repo root.
- After Phase 4 (US2): plus, the `lockfiles/` directory convention (`example-django` shape) works.
- After Phase 5 (US3): plus, the reader is defensively-safe against non-PEX `.lock` files in the wide-scope discovery paths.

### Byte-identity gate placement

T016 runs the m223 + m672 test suites unchanged as the final regression guard. This gate MUST land before T018 (pre-PR). If T016 fails, one of the reader edits (most likely T004 / T008 / T009) broke the pre-m673 output shape — do NOT regenerate goldens; fix the code first.
