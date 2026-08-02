---
description: "Task list for feature 224 (Pants coursier JVM lockfile reader)"
---

# Tasks: Pants coursier JVM lockfile reader

**Input**: Design documents from `/specs/224-pants-coursier-jvm/`
**Prerequisites**: plan.md ✅, spec.md ✅ (3 user stories, 11 FRs, 6 SCs), research.md ✅ (5 items), data-model.md ✅ (5 Deserialize types + config + Coordinate), contracts/coursier-lockfile-schema.md ✅, quickstart.md ✅

**Tests**: Tests ARE included — every reader shipped since m002 has test coverage per Constitution Principle VII, and the coord-string parser + non-Pants discriminator introduce failure modes that only tests can audit.

**Organization**: Tasks grouped by user story. Follows m223's shape closely; several phases removed because m223's parity infrastructure is reused verbatim (see plan.md §"Zero new parity work").

## Format: `[TaskID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3
- File paths absolute or repo-relative from `/Users/mlieberman/Projects/mikebom`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare the module directory + promote the shared Maven-PURL helper.

- [X] T001 Create module directory `waybill-cli/src/scan_fs/package_db/pants_jvm/` with 5 empty stub files (`mod.rs`, `lockfile.rs`, `config.rs`, `coordinate.rs`, `resolve_classifier.rs`), each carrying only a `//! Milestone 224: <purpose>` doc-comment.
- [X] T002 Register the new module: add `pub mod pants_jvm;` to `waybill-cli/src/scan_fs/package_db/mod.rs` alphabetically (between `pants` and `pip`). Verify with `cargo +stable build -p waybill --bin waybill` — should compile clean (readers do nothing yet).

**Checkpoint**: Empty pants_jvm module registered. Compile clean.

**Note on removed T003** (per finding A1 from `/speckit-analyze`): a
draft of this spec included a T003 that promoted
`maven::build_maven_purl` at `waybill-cli/src/scan_fs/package_db/maven.rs:2365`
from `fn` to `pub(crate) fn`. Removed because pants_jvm builds its
own PURL string inline in T015/T016 (per research.md §R3 option B)
and no other in-flight consumer needs the promotion — YAGNI. Filed
as a follow-up in plan.md §"Follow-ups (out-of-scope for this branch)".

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define the coursier lockfile Deserialize types + `Coordinate` parser + `pants.toml` config parser + resolve classifier so all three user stories can consume them.

**⚠️ CRITICAL**: US1/US2/US3 all depend on these types.

- [X] T004 In `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`, add module-private Deserialize types `CoursierLockfile`, `Entry`, `EntryCoord`, `EntryFileDigest`, `PantsMetadata` per data-model.md §"New types". Use `#[derive(Debug, Deserialize)]` + `#[serde(default)]` on optional fields exactly as documented. Do NOT add parse logic yet — types only.
- [X] T005 In `waybill-cli/src/scan_fs/package_db/pants_jvm/coordinate.rs`, add module-private `Coordinate` struct + `pub(crate) fn parse_coord_string(s: &str) -> Option<Coordinate>` per research.md §R2. Add 10 unit tests covering the R2 edge-case table: `"g:a:v"`, `"g:a:v,url=X"`, `"g:a:v,url=X,jar=Y"`, `"g:a"` (missing version), `"g:a:v:extra"` (splitn-3 catches extras), `""`, `":a:v"`, `"g::v"`, `"g:a:"`, `"g:a:v,"` (trailing comma).
- [X] T006 In `waybill-cli/src/scan_fs/package_db/pants_jvm/config.rs`, add module-private Deserialize types `PantsConfig { jvm: JvmSection }` + `JvmSection { default_resolve: Option<String>, resolves: HashMap<String, String> }` per data-model.md §"PantsConfig + JvmSection". Add `pub(crate) fn parse(bytes: &[u8]) -> Option<PantsConfig>` returning `None` on any `toml::from_slice` error (per FR-004 fail-open). Add 4 unit tests: valid `[jvm].default_resolve` + `[jvm.resolves]` table, missing `[jvm]` section, `[jvm]` present but no `resolves`, malformed TOML.
- [X] T007 In `waybill-cli/src/scan_fs/package_db/pants_jvm/resolve_classifier.rs`, add `const DEV_RESOLVE_NAMES: &[&str] = &[...]` per FR-008 allowlist: `scalatest`, `junit`, `testng`, `mockito`, `assertj`, `hamcrest`, `scalafmt`, `scalastyle`, `scalafix`, `checkstyle`, `spotbugs`, `pmd`, `errorprone`, `jacoco`, `dokka`, `ktlint`, `detekt`, plus generics (`lint`, `test`, `dev`, `ci`, `check`, `tools`, `docs`). Add `pub(crate) fn classify_resolve(name: &str) -> LifecycleScope` returning `LifecycleScope::Development` for allowlisted names (case-insensitive), else `LifecycleScope::Runtime`. Add 5 unit tests: `"default"` → Runtime, `"junit"` → Development, `"KTLINT"` → Development (case-insensitive), `"scalatest"` → Development, `"my-service-runtime"` → Runtime.

**Checkpoint**: Deserialize types + coord parser + config parser + resolve classifier all compile with green unit tests. Ready for the orchestrator wiring.

---

## Phase 3: User Story 1 — Scan a Pants JVM repo and emit Maven components (Priority: P1) 🎯 MVP

**Goal**: `waybill sbom scan` against a Pants JVM repo emits one `pkg:maven/*` component per locked distribution, with correct sha256 hashes, dependency edges, `waybill:pants-resolve` annotation, and lifecycle-scope tagging via the JVM dev-tool allowlist.

**Independent Test**: Run the scan against `waybill-cli/tests/fixtures/pants_coursier_jvm/minimal_jvm/`. Assert the emitted CDX has 3 components with `pkg:maven/dev.waybill.fixture/{core,util,api}@1.0.0` PURLs, sha256 hashes matching the fixture lockfile, and `waybill:pants-resolve=default` on each.

### Tests for User Story 1

- [X] T008 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/minimal_jvm/3rdparty/jvm/default.lock` per research.md §R5 fixture 1: valid Pants coursier lockfile with 3 synthetic entries (`dev.waybill.fixture:core:1.0.0`, `dev.waybill.fixture:util:1.0.0`, `dev.waybill.fixture:api:1.0.0`), each with 1 sha256 fingerprint. Include ONE `dependencies[]` edge (`api` depends on `core`) for FR-003 coverage. Include the `# --- BEGIN PANTS LOCKFILE METADATA` header block with `"version": 1`.
- [X] T009 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/multi_resolve/3rdparty/jvm/{default,junit,scalatest}.lock` per research.md §R5 fixture 2: 3 lockfiles, 2 entries each. Different fixture package names per resolve (`dev.waybill.fixture:runtime-{a,b}:1.0.0` in default, `dev.waybill.fixture:testing-junit-{a,b}:1.0.0` in junit, `dev.waybill.fixture:testing-scala-{a,b}:1.0.0` in scalatest). All 3 lockfiles carry the Pants metadata header. Exercises FR-008 lifecycle-scope tagging.
- [X] T010 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/with_classifier/3rdparty/jvm/default.lock` per research.md §R5 fixture 5: 4 entries covering FR-002 PURL qualifier edge cases + FR-009 source-url coverage — one plain (packaging=jar, no classifier, no url), one with `packaging = "war"` (non-default), one with `classifier = "linux-x86_64"` + `packaging = "so"`, and one with `[entries.coord].url = "https://internal-mirror.example.test/dev/waybill/fixture/internal-source/1.0.0/internal-source-1.0.0.jar"` set (packaging=jar, no classifier). Exercises the `?classifier=<c>&type=<packaging>` PURL emission AND the `waybill:source-url` annotation emission per FR-009 (per finding C1 from `/speckit-analyze`).
- [X] T011 [P] [US1] Create integration test file `waybill-cli/tests/pants_coursier_jvm_reader.rs` with 4 initial `#[test]` functions (bodies filled in T012–T014a):
  - `us1_minimal_jvm_lockfile_emits_3_maven_components`
  - `us1_multi_resolve_tags_scope_per_allowlist`
  - `us1_classifier_and_packaging_qualifiers_emit_correctly`
  - `us1_fr010_info_log_emits_all_five_structured_fields`
  Import helpers `bin()`, `run_scan()`, `read_cdx()`, `get_property()` mirroring `waybill-cli/tests/pants_pex_reader.rs` verbatim.
- [X] T012 [US1] Implement `us1_minimal_jvm_lockfile_emits_3_maven_components` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses fixture from T008. **Emits BOTH formats in one scan invocation** (`--format cyclonedx-json --format spdx-2.3-json --output <fmt>=<path>`) per SC-001. Assert: (a) exit 0; (b) CDX contains exactly 3 components with `pkg:maven/dev.waybill.fixture/{core,util,api}@1.0.0` PURLs, each with 1 sha256 hash + `waybill:pants-resolve=default` in properties[]; (c) SPDX 2.3 output contains 3 `packages[]` with matching `externalRefs[]` + `checksums[]`; (d) the CDX `dependencies[]` graph has an edge from `api` → `core` per the fixture's `dependencies[]` array.
- [X] T013 [US1] Implement `us1_multi_resolve_tags_scope_per_allowlist` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses fixture from T009. Assert: 6 total components; `default.lock` components have `waybill:lifecycle-scope=runtime` (or absent — matches Runtime default); `junit.lock` + `scalatest.lock` components have `waybill:lifecycle-scope=development`; every component has its correct `waybill:pants-resolve=<name>` annotation.
- [X] T014 [US1] Implement `us1_classifier_and_packaging_qualifiers_emit_correctly` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses fixture from T010. Assert: 4 components; the plain one has PURL `pkg:maven/dev.waybill.fixture/plain@1.0.0` (no qualifiers, no `waybill:source-url` annotation); the war one has PURL containing `?type=war` (no `type=jar` anywhere); the classifier+so one has PURL containing both `?classifier=linux-x86_64&type=so` (or the query-param separator equivalent per purl-spec — `?` before first, `&` between); the fourth (internal-source) has PURL `pkg:maven/dev.waybill.fixture/internal-source@1.0.0` (no qualifiers — `url` doesn't affect PURL shape) AND a `waybill:source-url` property with the exact URL from the fixture (per FR-009 coverage — remediation of finding C1).
- [X] T014a [US1] Implement `us1_fr010_info_log_emits_all_five_structured_fields` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses fixture from T008. Subprocess with `RUST_LOG=info`; strip ANSI codes; assert stderr contains ALL FIVE structured field names (`lockfiles_discovered=`, `lockfiles_parsed_ok=`, `lockfiles_skipped_corrupt=`, `lockfiles_skipped_non_pants=`, `components_emitted=`). Note the new `lockfiles_skipped_non_pants` field vs m223.

### Implementation for User Story 1

- [X] T015 [US1] In `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`, add `pub(crate) fn parse(bytes: &[u8]) -> Option<CoursierLockfile>` implementing the 3-step parse per contracts/coursier-lockfile-schema.md §"Fail-open behavior boundaries":
  1. Scan bytes for `# --- BEGIN PANTS LOCKFILE METADATA`; if absent → return None + INFO log (FR-011 discriminator). Caller distinguishes this INFO from WARN cases via a separate return type OR a companion outcome enum — spec says INFO for non-Pants, WARN for corrupt; simplest: return `Option<(CoursierLockfile, bool /*is_pants*/)>` where the caller counts non-Pants separately in the FR-010 log. Alternative: `Result<CoursierLockfile, SkipReason>` where `SkipReason { NotPants, MetadataInvalid, TomlParseError }` classifies for the caller's log tally. **Chosen shape**: `Result<CoursierLockfile, SkipReason>` per Principle IV — typed skip reasons are more auditable than bool-in-Option.
  2. Strip-and-concat lines between `BEGIN`/`END` markers; parse metadata JSON; verify `version == 1`; on failure → return `Err(SkipReason::MetadataInvalid)` + WARN.
  3. Strip the entire header comment block (all leading `# ` lines up through the `END` marker); parse remainder as TOML via `toml::from_str::<CoursierLockfile>()`; on failure → return `Err(SkipReason::TomlParseError)` + WARN.
  Add 4 unit tests: valid Pants coursier → Ok; missing header → Err(NotPants) + INFO (not WARN — verify log level); bad metadata version → Err(MetadataInvalid); malformed TOML body after valid header → Err(TomlParseError).
- [X] T016 [US1] In `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`, add module-private helper `pub(crate) fn entry_to_package_db_entry(entry: &Entry, lockfile_path: &Path, resolve_name: &str) -> Option<PackageDbEntry>` per data-model.md field-mapping table. Returns None + WARN on: empty group/artifact/version, PURL construction failure. Consumes: coordinate-parse helper from T005, `classify_resolve` from T007. PURL construction is inline per research.md §R3 option B (matches maven.rs:1787-1796 pattern, appends `?classifier=<c>&type=<packaging>` qualifiers when non-default). Extracts coord-triple strings from `entry.dependencies[]` for the `depends` field (via `Coordinate` parse from T005; drops empty-segment errors with WARN). Emits `waybill:pants-resolve` annotation always; emits `waybill:source-url` iff `entry.coord.url` is non-null non-empty. Add 4 unit tests: happy-path plain entry, entry with classifier+war packaging, entry with `url` set (assert source-url annotation), entry with empty version (assert None + WARN).
- [X] T017 [US1] In `waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs`, implement `pub fn read(scan_root: &Path) -> Vec<PackageDbEntry>` per contracts §"Discovery + orchestration data flow":
  1. If `scan_root/pants.toml` exists: parse via `config::parse()`.
  2. Enumerate candidate lockfile paths: default glob `3rdparty/jvm/*.lock` (via `std::fs::read_dir` — same pattern m223 uses) + optional `[jvm.resolves]` entries from PantsConfig (resolved against scan_root).
  3. For each candidate: read bytes, call `lockfile::parse()`. Match on `Result<CoursierLockfile, SkipReason>` — increment the appropriate counter for the FR-010 log.
  4. For each `Ok(CoursierLockfile)`: iterate `entries[]`; for each Entry call `entry_to_package_db_entry(entry, path, resolve_name)`. Resolve name derived from filename stem OR `[jvm.resolves]` config-declared name (config wins if the path matches).
  5. Emit FR-010 INFO log with 5 structured fields (adds `lockfiles_skipped_non_pants` vs m223's 4). Log module path: `waybill::scan_fs::package_db::pants_jvm`.
  6. Return accumulated `Vec<PackageDbEntry>`. If zero lockfiles discovered → return `Vec::new()` early WITHOUT emitting the log (byte-identity guarantee per FR-007 / SC-003).
- [X] T018 [US1] Wire the new reader into `waybill-cli/src/scan_fs/package_db/mod.rs::read_all()`. Add `pants_jvm::read(rootfs)` call after the existing `pants::read(rootfs)` (m223) call. Follows the same "extend the aggregated result vector" pattern.

**Checkpoint**: Run `cargo +stable test -p waybill --test pants_coursier_jvm_reader us1_`. Expect T012 + T013 + T014 + T014a all green. `waybill sbom scan --path waybill-cli/tests/fixtures/pants_coursier_jvm/minimal_jvm/ --format cyclonedx-json --output /tmp/us1.cdx.json` produces the expected 3 components. **MVP shippable at this point.**

---

## Phase 4: User Story 2 — Dedup against `pom.xml` (Priority: P2)

**Goal**: When both a coursier lockfile and a `pom.xml` declare the same Maven coordinates, the SBOM contains exactly one component sourced from the (authoritative) lockfile.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_coursier_jvm/with_pom_xml/`. Assert the CDX contains exactly ONE `pkg:maven/dev.waybill.fixture/shared@1.0.0`, sourced from the lockfile.

### Tests for User Story 2

- [X] T019 [P] [US2] Create synthetic fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/with_pom_xml/` per research.md §R5 fixture 4: `3rdparty/jvm/default.lock` with `dev.waybill.fixture:shared:1.0.0` (Pants-header-carrying, sha256 present) + minimal `pom.xml` at root declaring `<groupId>dev.waybill.fixture</groupId><artifactId>shared</artifactId><version>1.0.0</version>`.
- [X] T020 [P] [US2] Add integration test `us2_lockfile_dedups_against_pom_xml` to `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses fixture from T019. Assert: exit 0; CDX contains exactly 1 component with PURL `pkg:maven/dev.waybill.fixture/shared@1.0.0`; that component has 1 sha256 hash (came from lockfile — pom-tier entries carry none); `waybill:source-files` annotation contains BOTH the lockfile path AND `pom.xml`.

### Implementation for User Story 2

- [X] T021 [US2] No new production code — dedup is entirely handled by the existing m191 reconciler (validated in m223 US2). Verify: run T020 first as a regression check. If it passes without any reconciler change, done. If it fails: root-cause via test failure output — the pants_jvm entry lacks `hashes.len() > 0` OR `sbom_tier="source"` OR the PURL doesn't match the Maven reader's normalization; fix in `entry_to_package_db_entry` (T016) NOT the reconciler.

**Checkpoint**: T020 green. US2 done with zero production LOC beyond US1's existing implementation.

---

## Phase 5: User Story 3 — `pants.toml` `[jvm.resolves]` table discovery (Priority: P3)

**Goal**: When `pants.toml` declares `[jvm.resolves] <name> = "<path>"`, waybill discovers lockfiles at those paths + names them accordingly.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_coursier_jvm/pants_toml_custom_path/`. Assert waybill discovers `build-support/jvm/prod.lock` (NOT `3rdparty/jvm/*.lock`, which doesn't exist) AND tags the component with `waybill:pants-resolve=prod` (config-declared name wins).

### Tests for User Story 3

- [X] T022 [P] [US3] Create synthetic fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/pants_toml_custom_path/` per research.md §R5 fixture 3: `pants.toml` declaring `[jvm.resolves] prod = "build-support/jvm/prod.lock"` + `build-support/jvm/prod.lock` file (Pants-header-carrying) with 2 synthetic entries. NO file at `3rdparty/jvm/`.
- [X] T023 [P] [US3] Add integration test `us3_pants_toml_custom_path_discovery` to `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses fixture from T022. Assert: exit 0; CDX contains 2 JVM components; each carries `waybill:pants-resolve=prod` (config-declared name wins); INFO log shows `lockfiles_discovered=1` at the `build-support/jvm/prod.lock` path.
- [X] T024 [P] [US3] Add integration test `us3_missing_pants_toml_falls_back_to_default_glob` to `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses the US1 fixture (`minimal_jvm/`) which has no `pants.toml`. Assert: exit 0; 3 components still discovered from default glob. Regression guard for FR-004's fallback contract.
- [X] T025 [P] [US3] Add integration test `us3_malformed_pants_toml_falls_back_gracefully` to `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses a new fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/malformed_pants_toml/`: `pants.toml` containing `not = valid = toml =` (garbage) + `3rdparty/jvm/default.lock` (Pants-header-carrying) with 1 entry. Assert: exit 0 (no scan-abort); 1 component discovered from default glob; WARN in log naming `pants.toml`.

### Implementation for User Story 3

- [X] T026 [US3] T017 already implements the `pants.toml` discovery per contracts §"Discovery + orchestration data flow" step 1 + 2. Verify via T023/T024/T025 passing. If T023 fails: config path resolution wrong — check that the `[jvm.resolves]`-declared path is resolved relative to `scan_root` (not CWD). If T025 fails: `config::parse()` isn't swallowing errors correctly — revisit T006 return-type.

**Checkpoint**: T023 + T024 + T025 green.

---

## Phase 6: FR-011 discriminator + edge cases (Cross-cutting)

**Purpose**: Cover the FR-011 non-Pants-coursier discriminator + SC-005 corruption behavior + FR-007 zero-cost guarantee.

- [X] T027 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/non_pants_coursier/3rdparty/jvm/default.lock` per research.md §R5 fixture 6: valid TOML matching the coursier `[[entries]]` shape, but WITHOUT the `# --- BEGIN PANTS LOCKFILE METADATA` header. Simulates a standalone coursier-CLI lockfile.
- [X] T028 [P] Add integration test `fr011_non_pants_coursier_lockfile_skipped_with_info` to `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses T027 fixture. Assert: exit 0; zero components emitted from this reader (the pkg:maven/... coords in the fixture must NOT appear in the SBOM output as pants_jvm-sourced); stderr contains INFO log naming the file + reason ("not a Pants-generated coursier lockfile; skipping"); `lockfiles_skipped_non_pants=1` in FR-010 log.
- [X] T029 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/corrupt_lockfile/3rdparty/jvm/default.lock` per research.md §R5 fixture 7: valid Pants header block followed by intentionally broken TOML body (e.g., `[[entries]]` line but no following fields, or `[entries.coord] group = ` with no value).
- [X] T030 [P] Add integration test `corrupt_lockfile_produces_warn_and_continues` to `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses T029 fixture. Assert: exit 0 (fail-open per SC-005); zero pants_jvm components emitted; subprocess stderr contains WARN with the corrupt file's path; `lockfiles_skipped_corrupt=1` in FR-010 log.
- [X] T031 [P] Add integration test `no_pants_jvm_no_lockfiles_produces_no_reader_activity` to `waybill-cli/tests/pants_coursier_jvm_reader.rs`. Uses ANY existing non-JVM fixture (reuse from another test that doesn't have `3rdparty/jvm/`). Assert: exit 0; zero JVM components from pants_jvm; INFO log line `pants-coursier-jvm reader complete` MUST NOT appear (reader must return early without logging when zero lockfiles discovered per FR-007). Regression guard for SC-003 byte-identity.

---

## Phase 7: Docs + memory

- [X] T032 [P] Update `docs/ecosystems.md` (append after the `## pants (Python)` section shipped in m223) — add `## pants (JVM)` section covering: default glob, `pants.toml` `[jvm.resolves]` discovery, Pants-header discriminator (FR-011), PURL construction (Maven + classifier/packaging qualifiers), multi-resolve + dev-tool allowlist, coexistence with existing Maven reader, FR-010 log shape, follow-ups. Cross-link to `specs/224-pants-coursier-jvm/quickstart.md`. Also add a row to the coverage-matrix table at the top of the file: `[pants (JVM)](#pants-jvm)` between the `[pants (Python)]` and `[kotlin]` rows.
- [X] T033 [P] Update `README.md` supported-ecosystems table (add a row `**pants (JVM)** *(224)*` between the `**pants (Python)**` and `**vcpkg**` rows). Bump the "Twelve production ecosystem readers" count to "Thirteen".
- [X] T034 [P] Add memory entry `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_pants_coursier_jvm_reader.md` documenting: module location (`scan_fs/package_db/pants_jvm/`), coursier TOML schema quirks (metadata header, coord-string shape), the JVM dev-tool allowlist, reuse of m223 C143/C144 catalog rows (zero new parity work), FR-011 discriminator rationale, follow-up milestones (standalone coursier, BUILD-file walker, eBPF build-subprocess trace). Add corresponding line to `MEMORY.md` index.

---

## Phase 8: Pre-PR gate

- [X] T035 Run `./scripts/pre-pr.sh`. Confirm: (a) `cargo +stable clippy --workspace --all-targets -- -D warnings` exit 0, zero warnings; (b) `cargo +stable test --workspace --no-fail-fast` — every suite reports `ok. N passed; 0 failed`. Report per-target counts per memory `feedback_prepr_gate_full_output`.
- [X] T036 Verify no unintended goldens changed: `git status waybill-cli/tests/fixtures/` MUST show only additions under `pants_coursier_jvm/` — no modifications to any existing golden. Any modification indicates a leaked side-effect on other readers.
- [X] T037 Run `cargo tree -p waybill --target x86_64-unknown-linux-gnu -e normal | grep -Ei 'openssl-sys|libz-sys|aws-lc-rs|aws-lc-sys|native-tls|mbedtls-sys|tough'` — expect zero output (Constitution Principle I regression guard).
- [X] T038 Locally walk `specs/224-pants-coursier-jvm/quickstart.md` §1 and §2 end-to-end against the largest fixture from Phase 3 (multi_resolve — 3 resolves, 6 components). Confirm the FR-010 INFO log line appears with the expected 5 structured fields including `lockfiles_skipped_non_pants=0`.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: T001 → T002. Two sequential tasks; no parallelism possible (T002 depends on T001 creating the module dir). T003 removed per `/speckit-analyze` finding A1 (dead-code-at-merge — reader builds PURL inline).
- **Foundational (Phase 2)**: T004 + T005 + T006 + T007 all `[P]` w.r.t. each other (4 different files). All 4 must complete before US1 starts.
- **US1 (Phase 3)**: T008 + T009 + T010 + T011 `[P]` (fixture creation + test scaffolding — different files). T015 → T016 (T016 consumes T015's SkipReason + T005's coord parser + T007's classifier). T017 depends on T015 + T016 + T006. T018 depends on T017. Tests T012/T013/T014/T014a depend on T018 (need the reader wired) + their respective fixtures.
- **US2 (Phase 4)**: T019 + T020 `[P]`. T021 is verification only.
- **US3 (Phase 5)**: T022 + T023 + T024 + T025 all `[P]` (different fixtures + different tests). T026 verification only.
- **Phase 6**: T027 + T028 + T029 + T030 + T031 all `[P]`.
- **Phase 7**: T032 + T033 + T034 all `[P]`.
- **Phase 8**: T035 → T036 → T037 → T038 sequential.

### Story dependencies (visualized)

```text
Phase 1 (Setup) ──> Phase 2 (Foundational) ──> Phase 3 (US1 MVP) ──> Phase 6 (edge cases)
                                                    │                     │
                                                    ├──> Phase 4 (US2)    │
                                                    └──> Phase 5 (US3)    │
                                                                          │
                                                     Phase 7 (docs) <─────┤
                                                                          │
                                                     Phase 8 (pre-PR) <───┘
```

### Parallel opportunities

- **Phase 2**: T004 + T005 + T006 + T007 in parallel (4 different files).
- **Phase 3 setup half**: T008 + T009 + T010 + T011 in parallel (fixtures + skeleton).
- **Phase 6**: all 5 tasks in parallel.
- **Phase 7**: all 3 tasks in parallel.

---

## Implementation Strategy

### MVP first (US1 only)

1. Complete Phase 1 (Setup — module skeleton + shared helper promotion).
2. Complete Phase 2 (types + parsers + classifier).
3. Complete Phase 3 (US1 — reader + orchestrator + 4 integration tests).
4. **STOP + VALIDATE**: `cargo +stable test -p waybill --test pants_coursier_jvm_reader us1_` should be all-green.
5. Deploy / demo MVP: `waybill sbom scan` now covers Pants JVM repos with default lockfile layout.

### Incremental delivery after MVP

- Phase 4 (US2 dedup) — 1 fixture + 1 test + verification.
- Phase 5 (US3 pants.toml discovery) — 1 fixture + 3 tests + verification.
- Phase 6 (edge-case coverage) — hardens SC-005 fail-open + FR-011 discriminator + FR-007 zero-cost guarantee.
- Phase 7 (docs polish).
- Phase 8 (pre-PR gate).

Estimated total effort: **~1.5 focused work-days** (30% smaller than m223 due to parity-work reuse + smaller LOC surface).

---

## Notes

- **Zero new parity-catalog rows or extractors.** m223's C143 (`waybill:pants-resolve`) + C144 (`waybill:source-url`) are reused verbatim. This is the single biggest simplification vs m223.
- All fixtures use synthetic Maven coordinates (`dev.waybill.fixture:*`) per memory `feedback_fixture_synthetic_package_names`. Never real Maven Central coordinates.
- `[P]` tasks touch different files with no ordering dependency.
- Every US phase is independently shippable.
- The `lockfiles_skipped_non_pants` FR-010 field is NEW vs m223 (FR-011 discriminator introduces a new skip class). Update `docs/ecosystems.md` FR-010 example to reflect this.
- Estimated production LOC: ~400 total (T004+T005+T006+T007 ≈ 200 LOC; T015+T016 ≈ 150 LOC; T017+T018 ≈ 50 LOC). Test LOC: ~250. Fixture LOC (TOML + XML): ~250. **~30% smaller than m223's ~500 + ~320 + ~200.**
