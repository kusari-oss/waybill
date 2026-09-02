---
description: "Task list for m674 uv.lock reader"
---

# Tasks: m674 uv.lock reader for the UV Python package manager

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/674-uv-lock-reader/`
**Prerequisites**: plan.md (loaded), spec.md (loaded, US1+US2+US3), research.md (R1–R8), data-model.md (1 enum + 2 structs + 1 shared type + C157 row), contracts/ (3 contracts), quickstart.md (11-step recipe).

**Tests**: Integration tests are IN SCOPE per spec.md's user-story acceptance scenarios. Unit tests inline in `uv/source_variant.rs` + `uv/lockfile.rs` for per-variant + schema-parse coverage.

**Organization**: 3-file new source module (`uv/{mod,lockfile,source_variant}.rs`) + 4 plumbing edits + 3 new committed fixtures + 1 new integration test file. Zero new Cargo dependencies.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 per spec.md
- Every task lists an absolute file path

## Path Conventions

Single Rust workspace crate: `waybill-cli` at repo root. All source edits inside `/Users/mlieberman/Projects/mikebom/waybill-cli/`.

---

## Phase 1: Setup

No new crates, no `Cargo.toml` changes. Single setup task confirms branch state.

- [X] T001 Verify branch `674-uv-lock-reader` is checked out and clean (no uncommitted m673 residue on main). Run `git status` and `git log --oneline -3` to confirm HEAD sits on the m674 branch created by `/speckit.specify` and main is at the m673 merge (`b0d7cdd feat(m673) ...`).

---

## Phase 2: Foundational (blocks all user-story work)

- [X] T002 [P] Create the `uv/` module scaffold at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/uv/mod.rs`. Empty `pub(crate) mod lockfile; pub(crate) mod source_variant;` declarations + a stub `pub fn read(scan_root: &std::path::Path) -> Vec<super::PackageDbEntry> { Vec::new() }` (returns empty until wired in T007). Add `pub(crate) mod uv;` to `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/mod.rs`. `cargo +stable check -p waybill` clean.
- [X] T003 [P] Create `UvSource` enum + per-variant helpers at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/uv/source_variant.rs` per `data-model.md` §"Enum 1" + `contracts/source_variants.md` C1–C6. Declare the 6-variant enum with `#[serde(untagged)]`. Add helpers `build_purl(&self, name, version) -> Option<Purl>` (returns None for Editable/Virtual per FR-006) and `build_source_annotations(&self) -> Vec<(String, String)>` returning `(annotation_name, annotation_value)` pairs per contract §C1–C4. Add inline unit tests covering the 8-row test matrix from `contracts/source_variants.md` — one test per variant × sub-variant (registry-default, registry-custom, git, path-abs, path-rel, url, editable-skip, virtual-skip). Reuse `pip::normalize_pypi_name_for_purl` at `waybill-cli/src/scan_fs/package_db/pip/mod.rs:99` for FR-015 identity consistency.
- [X] T004 [P] Create schema deserialization types at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/uv/lockfile.rs` per `data-model.md` §"Struct 2/3/4". Declare `UvLockfile`, `UvPackage`, `UvDependency`, `UvHashArtifact` with `#[derive(Debug, serde::Deserialize)]`. Follow the field shapes verbatim from data-model.md. Add `#[serde(default)]` on every Optional field and `#[serde(rename = "package")]` on `UvLockfile.packages`. Do NOT set `#[serde(deny_unknown_fields)]` (per contract C3). No parse function yet — just the type declarations. `cargo +stable check -p waybill` clean.

**Checkpoint (Phase 2)**: `cargo +stable clippy -p waybill --tests` clean. `cargo +stable test -p waybill --bin waybill scan_fs::package_db::uv::source_variant::tests` passes with 8 new unit tests. Existing m223 + m672 + m673 tests still pass unchanged (the module isn't wired into `read_all` yet).

---

## Phase 3: User Story 1 — Standalone uv-managed Python project (Priority: P1)

**Goal**: `<scan_root>/uv.lock` at repo root discovers + parses + emits pypi components with SHA-256 hashes.

**Independent Test**: Synthetic fixture with `<repo-root>/pyproject.toml` + `<repo-root>/uv.lock` naming 3 registry-sourced packages + 1 transitive. Assert 4 pypi components emit with correct PURLs + hashes.

### Implementation for US1

- [X] T005 [US1] Implement `uv::lockfile::parse` at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/uv/lockfile.rs` per `contracts/uv_lockfile_schema.md` C1–C6 + `data-model.md` §"Struct 2" version-gate. Signature: `pub(crate) fn parse(bytes: &[u8]) -> Option<UvLockfile>`. Steps: (a) UTF-8 check + `toml::from_str::<UvLockfile>`, (b) version-gate — return None + WARN iff `version != 1` per contract C1, (c) return `Some(lockfile)` on success. Add inline unit tests covering the 7-row schema test matrix from `contracts/uv_lockfile_schema.md` — one test per row (minimal-accept, multi-source-accept, version-drift-reject, missing-field-reject, unknown-variant-reject, unknown-top-level-field-ignore, malformed-TOML-reject).
- [X] T006 [US1] Implement `uv::lockfile::to_entry` in the same file per `contracts/source_variants.md` C7 + `data-model.md` §"Struct 3" emission rules. Signature: `pub(crate) fn to_entry(package: &UvPackage, source_file: &std::path::Path, pants_resolve_name: Option<&str>) -> Option<PackageDbEntry>`. Steps: (a) call `package.source.build_purl(name, version)` — early-return `None` if the source is Editable/Virtual per FR-006, (b) extract SHA-256 hashes from `package.sdist` (Option) + `package.wheels[]`, dedup by hex-value per FR-008, (c) build annotations per contract §C7 (`waybill:python-lockfile-format=uv` + `waybill:source-files=<path>` + optional `waybill:pants-resolve=<name>` iff Some + variant-specific `source-type`/`source-url`), (d) construct `PackageDbEntry` with `sbom_tier = Some("lockfile")`. Add inline unit tests for at least 4 shapes: registry-emit-pypi, git-emit-generic, editable-return-None, wheel-hash-dedup.
- [X] T007 [US1] Implement `uv::mod::read` orchestrator at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/uv/mod.rs` per FR-001 + FR-012 + FR-013. Replace the T002 stub with real logic: (a) check `<scan_root>/uv.lock` existence — if missing, return `Vec::new()` silently per FR-013, (b) `std::fs::read` the file, (c) call `lockfile::parse` — on `None`, WARN + return empty, on `Some`, iterate `packages` and call `to_entry(package, &uv_lock_path, None)` (standalone context, no Pants resolve name), (d) emit `pants-pex reader complete`-style INFO log with `lockfiles_discovered / parsed_ok / components_emitted / skipped_corrupt` per FR-012. Log name: `uv reader complete`. Preserves m673 shape.
- [X] T008 [US1] Register the uv reader in the `package_db::read_all` dispatcher at `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/mod.rs`. Search for how `pants::read` is invoked (grep for `pants::read`) and mirror the pattern: add a call to `uv::read(scan_root)` at the same layer + concatenate results into the returned Vec. This is the ONE plumbing edit that makes the reader visible to `sbom scan` output.
- [X] T009 [US1] Create the m674 integration test scaffold at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_uv_lock_m674.rs`. Reuse the m673 harness pattern: `#![cfg(test)]` + `#![allow(clippy::unwrap_used)]` module attributes, `binary_path()` via `env!("CARGO_BIN_EXE_waybill")`, `strip_ansi()` for tracing log parsing, `run_scan(root, extra_args)` returning `(Value, String)`, `component_purls(doc)` for sorted-lex assertions, `pypi_components_only(purls)` filter helper. Include a `fixture_root(name: &str) -> PathBuf` helper resolving to `waybill-cli/tests/fixtures/uv_lock/<name>/`. No tests yet — scaffolding only. Compiles clean via `cargo +stable test --no-run -p waybill --test scan_uv_lock_m674`.
- [X] T010 [US1] Create the `minimal_uv` committed fixture at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/uv_lock/minimal_uv/`. Files: `pyproject.toml` (trivial `[project]` with 3 deps: `waybill-fixture-alpha`, `waybill-fixture-beta`, `waybill-fixture-gamma`); `uv.lock` (version = 1 + 4 `[[package]]` entries — 3 top-level + 1 transitive `waybill-fixture-alpha-dep`, all registry-sourced from `https://pypi.org/simple`, each with sdist + 1 wheel URL each carrying a distinct SHA-256 hash from the `aaaa...`/`bbbb...` synthetic-hash pool). Total file size < 3 KB. Names use `waybill-fixture-*` prefix.
- [ ] T011 [US1] Add US1 happy-path integration test `standalone_uv_project_emits_pypi_components` to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_uv_lock_m674.rs`. Fixture: `minimal_uv/`. Assert (a) exactly 4 `pkg:pypi/*` components emit, (b) each carries a SHA-256 hash from the fixture's synthetic pool, (c) each carries `waybill:python-lockfile-format = "uv"` property, (d) NONE carry `waybill:pants-resolve` (standalone context), (e) reader-complete INFO log shows `lockfiles_discovered=1 lockfiles_parsed_ok=1 components_emitted=4`, (f) scan-exit 0. This is SC-001's regression guard.
- [ ] T012 [US1] Create the `multi_source` committed fixture at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/uv_lock/multi_source/`. Files: `uv.lock` (version = 1 + 6 `[[package]]` entries — one per `UvSource` variant: `waybill-fixture-reg` (Registry, `https://internal.pypi.example/simple` — custom registry to exercise `waybill:pypi-source-url`), `waybill-fixture-git` (Git, `https://github.com/kusari-sandbox/waybill-fixture-git.git`, `rev=abc123def456`), `waybill-fixture-path` (Path, `../local-package`), `waybill-fixture-url` (Url, `https://example.test/wheel.whl`), `waybill-fixture-editable` (Editable, `.`), `waybill-fixture-virtual` (Virtual, `workspace-root`)). Every entry has version + minimal artifacts. Total file size < 4 KB.
- [ ] T013 [US1] Add US1 multi-variant integration test `multi_source_variants_emit_correctly` to the same test file. Fixture: `multi_source/`. Assert (a) exactly 4 components emit — Registry + Git + Path + Url (Editable + Virtual SKIP per FR-006), (b) Registry → `pkg:pypi/*` with `waybill:pypi-source-url` annotation naming the custom registry URL, (c) Git/Path/Url → `pkg:generic/*` each with correct `waybill:source-type` + `waybill:source-url` annotations per `contracts/source_variants.md` §C2–C4, (d) reader-complete log shows `components_emitted=4` (proves Editable + Virtual didn't sneak through).
- [ ] T014 [US1] Add US1 skip-verification integration test `editable_and_virtual_are_skipped` to the same test file. Same `multi_source/` fixture. Assert (a) NO component carries `name=waybill-fixture-editable` OR `name=waybill-fixture-virtual` (verify by iterating `.components[].name` — neither literal string appears), (b) no WARN log lines mention `editable` or `virtual` skip (FR-006 is silent-skip, not WARN-skip — those are semantically correct behavior, not corruption).

**Checkpoint (US1)**: US1 tests pass (4 tests: T011, T013, T014, T009 scaffold). m223 + m672 + m673 existing integration tests unchanged. `cargo +stable clippy -p waybill --tests` clean.

---

## Phase 4: User Story 2 — Pants monorepo using uv as resolver backend (Priority: P1)

**Goal**: The m673 Pants pipeline dispatches `uv::lockfile::parse` on files that fail `pants::lockfile::parse`, emitting components with Pants context annotations preserved.

**Independent Test**: Fixture with `pants.toml` `[python.resolves]` naming 2 uv-shape lockfiles at `3rdparty/python/*.lock`. Assert both files' components emit with `waybill:pants-resolve` matching the pants.toml map keys.

### Implementation for US2

- [ ] T015 [US2] Add the FR-002 fallback in `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/pants/mod.rs::read` per `contracts/pants_integration.md` C1–C6 + `quickstart.md` Step 5. In the existing per-candidate parse loop: on `pants::lockfile::parse(&bytes) → None` (parse failed), invoke `crate::scan_fs::package_db::uv::lockfile::parse(&bytes)`. If that succeeds: emit an INFO log line naming the file + package count ("uv-lock reader: recognized `<path>` as uv.lock format after Pex parse rejection; parsed <N> packages"), iterate the returned `UvLockfile.packages`, and for each `package` call `uv::lockfile::to_entry(package, &candidate.path, Some(&candidate.resolve_name))` — passing the Pants resolve name as the third argument so emitted components carry `waybill:pants-resolve` per FR-002 + C4. Extend all counters correctly: `lockfiles_parsed_ok += 1` should fire when the uv fallback succeeds (from the Pants reader's perspective, the file did parse — just via a different parser). Do NOT double-count `legacy_shape_lockfiles`.
- [ ] T016 [US2] Create the `pants_uv_backend` committed fixture at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/uv_lock/pants_uv_backend/`. Files: `pants.toml` (with `[GLOBAL]` + `[python.resolves]` naming `python-default = "3rdparty/python/python-default.lock"` + `tools = "3rdparty/python/tools.lock"`); `3rdparty/python/python-default.lock` (uv-shape, version = 1, ~5 registry packages named `waybill-fixture-pd-*`); `3rdparty/python/tools.lock` (uv-shape, version = 1, ~3 registry packages named `waybill-fixture-tools-*`). Total fixture < 10 KB.
- [ ] T017 [US2] Add US2 recovery integration test `pants_uv_backend_recovers_components` to the same test file. Fixture: `pants_uv_backend/`. Assert (a) exactly 8 `pkg:pypi/*` components emit (5 + 3), (b) reader-complete INFO log shows `lockfiles_discovered=2 lockfiles_parsed_ok=2`, (c) the `pants-pex reader` WARN log lines (from the failed PEX parse) ARE present in stderr — verify the WARN messages by grepping "failed to parse Pex lockfile as JSON" — that's the artifact of the fallback flow per C3 (retain PEX WARN + emit uv INFO), (d) the uv-fallback INFO log line "recognized `<path>` as uv.lock format after Pex parse rejection" appears twice (once per file). This is SC-002's regression guard.
- [ ] T018 [US2] Add US2 annotation-propagation integration test `pants_resolve_annotation_preserved_via_fr002_fallback` to the same test file. Fixture: `pants_uv_backend/`. Assert (a) every emitted pypi component carries a `waybill:pants-resolve` property, (b) 5 components have `value="python-default"` (from `python-default.lock`) and 3 have `value="tools"` (from `tools.lock`) — matches the `pants.toml` map keys, (c) every component ALSO carries `waybill:python-lockfile-format = "uv"` (both annotations coexist).
- [ ] T019 [US2] Add US2 mixed-format integration test `mixed_pex_and_uv_lockfiles_both_emit` to the same test file. Create a new committed fixture `mixed_pex_and_uv/` at `waybill-cli/tests/fixtures/uv_lock/mixed_pex_and_uv/`: `pants.toml` naming both a PEX-shape lockfile (`pex-resolve = "3rdparty/python/pex.lock"`) AND a uv-shape lockfile (`uv-resolve = "3rdparty/python/uv.lock"`); `3rdparty/python/pex.lock` (PEX-shape JSON with `//`-frontmatter, 2 packages); `3rdparty/python/uv.lock` (uv-shape TOML, 3 packages). Assert (a) 5 total pypi components emit, (b) 2 tagged `waybill:pants-resolve=pex-resolve` + 3 tagged `waybill:pants-resolve=uv-resolve`, (c) 2 tagged `waybill:python-lockfile-format` = "pex" IF m674 back-attributes pex OR ABSENT if it doesn't (per FR-011 v1 scope, only uv emits the annotation; pex-sourced components do NOT get it in v1 — the annotation is emitted by the uv reader only). ← **The assertion should reflect v1 scope**: pex-sourced components have NO `waybill:python-lockfile-format`; uv-sourced components have `waybill:python-lockfile-format = "uv"`.

**Checkpoint (US2)**: US2 tests pass (3 new tests: T017, T018, T019). Cumulative m674 test count: 7. m223 + m672 + m673 tests still pass unchanged.

---

## Phase 5: User Story 3 — Interaction with m670 pyproject.toml + m191 reconciler (Priority: P2)

**Goal**: Verify the m191 reconciler correctly dedups m670-declared-deps against m674 uv.lock-resolved entries. `version=null` unresolved components from m670 MUST be suppressed when uv.lock is present with resolved versions.

**Independent Test**: Fixture with pyproject.toml (3 declared deps) + uv.lock (3 matching packages + 2 transitives). Assert 5 total pypi components emit with NO `version=null` entries.

### Implementation for US3

- [ ] T020 [US3] Add US3 reconciler integration test `pyproject_declared_deps_deduped_against_uv_lock` to the same test file. Extend the existing `minimal_uv/` fixture — its `pyproject.toml` already declares 3 deps matching the uv.lock top-level packages. Assert (a) exactly 4 pypi components emit (3 top-level + 1 transitive from uv.lock), (b) NONE have `version = null` OR `version = "unresolved"` — verify by iterating `.components[].version` and asserting every entry is a non-empty semver-ish string, (c) each top-level component carries a `waybill:python-lockfile-format = "uv"` property (proves the uv reader won the m191 reconciler contest, not m670's declared-deps fallback which would omit this annotation). This is SC-005's regression guard.
- [ ] T021 [US3] Add US3 byte-identity guard `pre_m674_byte_identity_on_non_uv_repos` to the same test file. Use a synthetic `tempfile::tempdir()` fixture with ONLY a `pyproject.toml` (declaring 2 deps) — no uv.lock, no requirements.txt, no pants.toml, no 3rdparty/python. Assert (a) exactly 2 pypi components emit — via the m670 pyproject-declared-deps fallback — with `version = "unresolved"` (m670 shape preserved), (b) NO `waybill:python-lockfile-format` property on either component (uv reader stayed silent), (c) reader-complete log line for the uv reader is ABSENT from stderr (byte-identity — no uv reader activity). This is SC-004's regression guard.

**Checkpoint (US3)**: US3 tests pass (2 new tests). Cumulative m674 test count: 9. `cargo +stable clippy -p waybill --tests` clean.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T022 [P] Register C157 in the parity catalog per `data-model.md` §"New parity catalog row" + m670 C154 / m671 C156 precedent. (a) Add C157 row to `/Users/mlieberman/Projects/mikebom/docs/reference/sbom-format-mapping.md` after C156 with the value shape (closed-enum string, v1 value = "uv"), `SymmetricEqual` directionality, and Principle-V bullet-5 native-alternative audit citing C155 + C124 as sibling patterns. (b) Add extractor macros `c157_cdx` / `c157_spdx23` / `c157_spdx3` to `waybill-cli/src/parity/extractors/{cdx.rs,spdx2.rs,spdx3.rs}` (component-scope). (c) Register the `ParityExtractor { row_id: "C157", label: "waybill:python-lockfile-format", cdx: c157_cdx, spdx23: c157_spdx23, spdx3: c157_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false }` entry in `parity/extractors/mod.rs::EXTRACTORS` + add the 6 name imports across the mass-import lines.
- [ ] T023 [P] Add US1 version-gate integration test `version_2_uv_lock_rejected_with_warn` to `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/scan_uv_lock_m674.rs`. Create a new fixture `version_2_uv/` with a `uv.lock` starting with `version = 2\n` + 1 valid-looking `[[package]]`. Assert (a) 0 pypi components emit from that fixture, (b) stderr contains a WARN naming the unsupported version — grep for `unsupported uv.lock schema version` or similar per contract C1. This is the version-gate regression guard.
- [ ] T024 [P] Update `/Users/mlieberman/Projects/mikebom/CLAUDE.md` "Recent Changes" section with an m674 entry describing: (a) new `uv/` reader module recovering both standalone uv-managed projects + Pants-with-uv-backend monorepos, (b) 6-variant `UvSource` enum with per-variant PURL rules, (c) `is_pex_lockfile_content` style content-detect via version-gate (v1 accepts version=1 only), (d) FR-002 Pants FR-002 integration via `pants/mod.rs::read` hook, (e) new parity C-row C157 `waybill:python-lockfile-format=uv`, (f) empirical grounding — recovers ≥400 components on backend.ai's 9 uv-shape lockfiles that today emit 0 from the Pants reader, (g) zero new Cargo deps. Match the m673 entry style.
- [ ] T025 [P] Create a new memory note at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_uv_lock_reader.md` documenting: (a) uv.lock format (version=1 schema, TOML shape, 6-variant source), (b) `is_pex_lockfile_content`-style version-gate rejection for v2+, (c) FR-002 Pants integration via `pants/mod.rs::read` dispatch, (d) C157 annotation shape, (e) v1/v2 boundary (marker-filtering, recursive discovery, wheel-per-platform all deferred). Register in MEMORY.md as `[m674 uv.lock reader (C157)](reference_uv_lock_reader.md) — brief tag line`.
- [ ] T026 Byte-identity guard: run BOTH `cargo +stable test -p waybill --test pants_pex_reader` (m223 goldens — must show 10 passed) AND `cargo +stable test -p waybill --test scan_pants_m672` (m672 — must show 10 passed) AND `cargo +stable test -p waybill --test scan_pants_m673` (m673 — must show 6 passed). All three MUST pass without regeneration. If any fails, m674 changes leaked into pre-m674 behavior — investigate + fix — do NOT regenerate goldens.
- [ ] T027 Real-world smoke test per `quickstart.md` Step 10. Clone `pantsbuild/example-python` + `pantsbuild/example-django` + `meilisearch/meilisearch-python` + `lablup/backend.ai`, scan each with `--offline` + `--no-deep-hash` + `RUST_LOG=info`, and assert: (a) `example-python` + `example-django` unchanged from post-m673 baselines (10 + 45 pypi respectively — SC-004 byte-identity), (b) `meilisearch-python` emits ≥ 50 pypi from uv.lock (SC-003), (c) `backend.ai` emits ≥ 400 pypi from the 9 uv-shape lockfiles (SC-002, was 133 pre-m674). Save the smoke-test outputs (ANSI-stripped log lines only — NOT full stderr) to `specs/674-uv-lock-reader/artifacts/smoke-<repo>-<date>.log`.
- [ ] T028 **Mandatory pre-commit customer/competitor grep** per updated memory `feedback_no_customer_names_in_code_or_docs` tier-based policy. Run: `grep -rEi '<blocklist-pattern>' $(git diff --cached --name-only) 2>/dev/null` — MUST return zero hits. Also review pantsbuild/meilisearch/lablup/backend.ai references — those are Tier 1 (neutrally-governed OSS or open-source projects, not competitors) so they're OK. This task is the belt-and-suspenders enforcement of the tier-based policy; must complete before T029 pre-PR.
- [ ] T029 Run the mandatory pre-PR gate: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 PATH="/Users/mlieberman/Projects/mikebom/.venv/spdx3-validate/bin:$PATH" ./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace` MUST pass green. Per Constitution v2.1.0 §Development Workflow.
- [ ] T030 Walker-audit allowlist sanity check per memory `feedback_walker_audit_local_check`. Reproduce the CI logic locally (use `command grep` + `/usr/bin/sed`). Expected: byte-for-byte match with pre-m674 allowlist (12 entries) — m674 does NOT add any new `fn walk[_(]` functions (the uv reader uses `std::fs::read` on a single known path, not directory walking).

**Checkpoint (Phase 6)**: Parity + docs + memory + byte-identity + real-world smoke + customer-grep + pre-PR + walker-audit all green. Ready to open PR against main.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 — one-shot branch verification. No dependencies.
- **Foundational (Phase 2)**: T002 (scaffold) is a strict prerequisite for T003 + T004 (they add types inside the scaffold). T003 + T004 are parallelizable (different files). After all three: uv module exists but does nothing.
- **US1 (Phase 3)**: T005 → T006 sequential (T006 depends on T005's `UvLockfile` type being consumable). T007 depends on T005 + T006 (orchestrator needs parse + to_entry). T008 depends on T007 (dispatcher needs `uv::read`). T009 (test scaffold) parallel to T005–T008 on a separate file. T010 (fixture) parallel to T009. T011 depends on T007 + T008 + T009 + T010. T012 + T013 + T014 depend on T011.
- **US2 (Phase 4)**: T015 depends on T005 + T006 (uv parse/to_entry must exist for Pants dispatch). T016 + T017 + T018 + T019 sequential in the same test file after T015.
- **US3 (Phase 5)**: T020 + T021 sequential in the same test file. T020 depends on T007 + T010 (uv reader working + minimal_uv fixture). T021 is byte-identity — depends on nothing except the release binary.
- **Polish (Phase 6)**: T022 (parity) parallel to T023 (version-gate test — different files) parallel to T024/T025 (docs). T026 depends on all US1/US2/US3 tasks landing. T027 depends on T026 (byte-identity confirmed first). T028 mandatory before T029. T029 last. T030 alongside T029.

### Recommended Execution Order (single contributor)

1. **T001** — verify branch (~2 min)
2. **T002 → T003 + T004 (parallel on different files)** foundational (~1.5 hours — T003 has 8 unit tests, T004 is schema type declarations)
3. **T005 → T006 → T007 → T008** uv reader core (~2 hours)
4. **T009 → T010** test scaffold + minimal fixture (~30 min)
5. **T011** US1 happy-path test (~15 min)
6. **T012 → T013 → T014** US1 remaining tests (~1 hour)
7. **T015 → T016 → T017 → T018 → T019** US2 (~1.5 hours)
8. **T020 → T021** US3 (~30 min)
9. **T022 + T023 + T024 + T025 (parallel)** polish (~1 hour)
10. **T026** byte-identity gate (~5 min)
11. **T027** real-world smoke (~15 min)
12. **T028** customer/competitor grep (~2 min)
13. **T029** pre-PR gate (~10–15 min)
14. **T030** walker-audit (~2 min)

Total: ~8 hours of focused work.

### Parallel Opportunities

- **Phase 2**: {T003 source_variant.rs, T004 lockfile.rs} parallel — different files.
- **Phase 6**: {T022 parity, T023 version-gate test, T024 CLAUDE.md, T025 memory} all parallel.

---

## Implementation Strategy

### MVP-First path (US1 only)

Shipping just US1 (T001–T014) as a shippable minimum recovers the standalone uv-managed-project ecosystem (`meilisearch/meilisearch-python` shape) — every uv-only project that today emits 0 or misidentifies via pyproject.toml would work post-US1. US2 + US3 are additional value but not strictly required for the US1 hello-world case.

**Recommended**: ship US1 + US2 + US3 in one PR (matches m671/m672/m673 shape). Splitting would create fixture-helper duplication + delay the backend.ai-shape recovery.

### Incremental delivery

- After Phase 3 (US1): every standalone uv-managed Python project (no Pants) has proper lockfile-tier detection.
- After Phase 4 (US2): plus, Pants monorepos using uv as resolver backend work (backend.ai case recovers).
- After Phase 5 (US3): plus, m191 reconciler dedup verified — no duplicate `version=null` entries from m670 fallback + resolved uv.lock entries.

### Byte-identity gate placement

T026 runs the m223 + m672 + m673 test suites unchanged as the final regression guard BEFORE the real-world smoke (T027) and pre-PR (T029). If T026 fails, one of the m674 edits (most likely T015 Pants FR-002 dispatch) broke pre-m674 behavior — do NOT regenerate goldens; fix the code first.

### Pre-PR customer-grep gate (T028)

This is the memory-mandated enforcement of the tier-based external-name policy (memory `feedback_no_customer_names_in_code_or_docs`). Runs the grep for the blocklist pattern (customer + competitor names per memory policy). Non-zero exit halts the pre-PR. This task exists specifically because m672 + m673 both slipped despite the memory rule; T028 makes the check mechanical rather than trust-based.
