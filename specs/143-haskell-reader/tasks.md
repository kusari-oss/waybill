---

description: "Task list for milestone 143 — Haskell ecosystem reader"
---

# Tasks: Haskell ecosystem reader

**Input**: Design documents from `/specs/143-haskell-reader/`
**Prerequisites**: plan.md ✓, spec.md ✓ (with Q1+Q2+Q3 clarifications), research.md ✓, data-model.md ✓, contracts/haskell-component-purl.md ✓, quickstart.md ✓

**Tests**: Integration tests included — established convention for milestones 064 / 066 / 068 / 069 / 070 / 122 / 135 / 136 / 137 / 138 / 139 / 140 / 141 / 142. Synthetic-fixture pattern via `tempfile::tempdir()`.

**Organization**: Tasks grouped by user story (US1 = P1 MVP `cabal.project.freeze` baseline; US2 = P2 Stack lockfile + snapshot placeholder + Q1 GHC-stdlib annotation; US3 = P3 design-tier fallback + Q2 multi-stanza union + multi-package + Q3 Hpack-detect). Setup + Foundational phases are blocking prerequisites for ALL user stories.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Maps task to user story phase (US1 / US2 / US3)
- Setup / Foundational / Polish phases: no story label

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Module skeleton + mod.rs declaration. No `read_all` integration yet — that lands in T015 once the parse pipeline is wired up.

- [X] T001 Create `mikebom-cli/src/scan_fs/package_db/haskell.rs` with module-level docstring (mirrors scala.rs preamble: milestone reference 143, FR list, PURL shape summary, Q1+Q2+Q3 clarifications recap, research §R1+R3+R4+R5 references), `use` block (`anyhow`, `serde::{Deserialize, Serialize}`, `serde_yaml`, `serde_json::{self, json, Value}`, `tracing::{warn, debug}`, `std::collections::{BTreeMap, HashSet, HashMap}`, `std::path::{Path, PathBuf}`, `std::sync::OnceLock`, `regex::Regex`, `mikebom_common::types::purl::Purl`, `mikebom_common::resolution::LifecycleScope`, the existing `PackageDbEntry` from `super`, `ExclusionSet` from `super::exclude_path`), and `pub fn read(rootfs: &Path, _include_dev: bool, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry>` stub returning `Vec::new()`.

- [X] T002 Add `pub mod haskell;` declaration to `mikebom-cli/src/scan_fs/package_db/mod.rs` (placed alphabetically — after `pub mod gradle;`/`pub mod golang;` and before `pub mod kotlin_dsl;`). No `read_all` integration yet — that lands in T015.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared helpers used by all user stories — types, GHC boot-library allowlist, regex compile-once helpers, Q2 merge_scope helper, Cabal-DSL stanza-block extractor, Q3 content-shape validator for `stack.yaml.lock`, `serde_yaml`-deserializable types for the Stack lockfile shape.

- [X] T003 Add private enums + structs in `mikebom-cli/src/scan_fs/package_db/haskell.rs` per data-model §2: `enum CabalFreezeEntry { ExactPin {...}, RangeConstraint {...} }` (with `#[derive(Debug, Clone)]`), `enum StackLockEntry { Hackage {...} }` (Git variant deferred to v1.1; warn-and-skip), `struct StackSnapshot { resolver, sha256: Option<String> }`, `struct CabalManifest { name: Option<String>, version: Option<String>, stanzas: Vec<CabalStanza>, hpack_generated: bool }`, `struct CabalStanza { kind: StanzaKind, label: Option<String>, build_depends: Vec<DeclaredDep>, build_tool_depends: Vec<DeclaredDep> }`, `enum StanzaKind { Library, Executable, TestSuite, Benchmark, ForeignLibrary }` (with `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`), `struct DeclaredDep { name, range: Option<String> }`.

- [X] T004 [P] Add `const GHC_STDLIB_ALLOWLIST: &[&str]` in `mikebom-cli/src/scan_fs/package_db/haskell.rs` per data-model §3 with the 22-name boot-library allowlist (`base`, `ghc-prim`, `template-haskell`, `integer-gmp`, `integer-simple`, `array`, `bytestring`, `containers`, `deepseq`, `directory`, `filepath`, `ghc`, `mtl`, `parsec`, `pretty`, `process`, `stm`, `text`, `time`, `transformers`, `unix`, `Win32`). Add module-level doc comment citing Q1 + FR-014 with the explicit "informational only, does NOT gate emission" wording.

- [X] T005 [P] Add private helper `fn merge_scope(existing: LifecycleScope, new: LifecycleScope) -> LifecycleScope` in `mikebom-cli/src/scan_fs/package_db/haskell.rs` per data-model §2.4. Runtime-wins-over-Development per Q2 most-binding precedence. Also add `fn stanza_lifecycle_scope(kind: StanzaKind) -> LifecycleScope` returning Runtime for Library/Executable/ForeignLibrary and Development for TestSuite/Benchmark. Include 3 unit tests covering: (a) Runtime + Development → Runtime, (b) Development + Development → Development, (c) stanza_lifecycle_scope returns correct mappings for all 5 StanzaKind variants.

- [X] T006 [P] Add private regex `OnceLock` helpers in `mikebom-cli/src/scan_fs/package_db/haskell.rs` for the parse patterns per research §R2 + §R4: `CONSTRAINTS_KEYWORD_RE` ((?ms)^constraints:\s*(.+?)(?:^\w|\z)`), `EXACT_PIN_RE`, `FLAG_TOGGLE_RE`, `RANGE_CONSTRAINT_RE`, `CABAL_NAME_RE` (`(?m)^name:\s*(\S+)`), `CABAL_VERSION_RE` (`(?m)^version:\s*(\S+)`), `CABAL_STANZA_RE` (`(?m)^(library|executable|test-suite|benchmark|foreign-library)(?:\s+(\S+))?\s*$`), `CABAL_BUILD_DEPENDS_RE`, `CABAL_BUILD_TOOL_DEPENDS_RE`, `HPACK_HEADER_RE` (`(?m)^-- This file has been generated from package\.yaml by hpack version`). Each regex stored in a private `static REGEX_NAME: OnceLock<Regex> = OnceLock::new();` slot at MODULE scope (NOT inside loops — hoist per research §R9 + milestone-141 R7 + milestone-142 R8 lesson). Expose via `fn constraints_keyword_re() -> &'static Regex` accessor functions per the established cross-milestone pattern.

- [X] T007 [P] Add `serde_yaml`-deserializable types in `mikebom-cli/src/scan_fs/package_db/haskell.rs` for the Stack lockfile schema per data-model §1.2 + §2.2 + research §R3. Types: `#[derive(Deserialize)] struct StackYamlLock { snapshots: Vec<StackSnapshotYaml>, packages: Vec<StackPackageYaml> }`, `struct StackSnapshotYaml { completed: Option<StackSnapshotCompleted>, original: Option<StackSnapshotOriginal> }`, `struct StackSnapshotCompleted { sha256: Option<String>, size: Option<u64>, url: Option<String> }`, `struct StackSnapshotOriginal { resolver: Option<String> }`, `struct StackPackageYaml { completed: Option<StackPackageCompleted>, original: Option<StackPackageOriginal> }`, `struct StackPackageOriginal { hackage: Option<String> }`. All non-required fields wrapped in `Option<>`; use `#[serde(default)]` to tolerate schema evolution.

- [X] T008 [P] Add private helper `fn validate_stack_lock_shape(json: &serde_yaml::Value) -> bool` in `mikebom-cli/src/scan_fs/package_db/haskell.rs` per Q3-style content-shape gate from research §R3. Returns `true` only when the YAML contains a top-level `snapshots:` key as an array. Used by `parse_stack_lock` to skip non-Stack-plugin files that match the `stack.yaml.lock` filename. Cite the Q3-equivalent gate inline + reference milestone-142 Q3 precedent.

- [X] T009 [P] Add private Cabal-DSL stanza-block extractor `fn extract_stanzas(cabal_text: &str) -> Vec<CabalStanza>` in `mikebom-cli/src/scan_fs/package_db/haskell.rs` per data-model §2.3 + research §R4. For each `CABAL_STANZA_RE` match, capture the indented block (from the line after the stanza opener until the next stanza opener or EOF) and extract its `build-depends:` + `build-tool-depends:` blocks via the respective regexes. Each captured dep-block string is comma-split (handling multi-line continuations via whitespace flattening), then per-entry parsed into `DeclaredDep { name, range }` via splitting on the first whitespace (everything before the whitespace is the name, lowercased; everything after is the range string preserved verbatim).

---

## Phase 3: User Story 1 — Operator scans a cabal-managed Haskell project with `cabal.project.freeze` (P1) 🎯 MVP

**Goal** (SC-001 + Q1 + FR-014): A scan of a synthetic Haskell project (3 direct deps + 5 transitives = 8 freeze entries) produces a CDX SBOM with 8 `pkg:hackage/*` components + 1 main-module; boot libraries in the lockfile carry `mikebom:ghc-stdlib = "true"`.

**Independent Test**: `cargo test -p mikebom --test haskell_cabal_baseline` passes.

- [X] T010 [US1] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn discover_cabal_files(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf>` + `fn discover_cabal_freezes(...)` + `fn discover_cabal_projects(...)` using `crate::scan_fs::walk::safe_walk` (per research §R11). Filter by file name: `discover_cabal_files` matches `*.cabal` (extension match); `discover_cabal_freezes` matches `cabal.project.freeze` (literal match); `discover_cabal_projects` matches `cabal.project` (literal match — for FR-001 detection signal only). Standard excludes per research §R11: `.git/`, `dist-newstyle/`, `dist/`, `.stack-work/`, `node_modules/`. Each helper returns sorted PathBuf vec for deterministic emission.

- [X] T011 [US1] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn parse_cabal_freeze(path: &Path) -> anyhow::Result<Vec<CabalFreezeEntry>>` per data-model §2.1 + research §R2. Read the file, locate the `constraints:` keyword via `CONSTRAINTS_KEYWORD_RE` from T006, concatenate continuation lines into one logical line, comma-split, then per-entry regex-dispatch via `EXACT_PIN_RE` (emit `CabalFreezeEntry::ExactPin`), `FLAG_TOGGLE_RE` (SKIP — flag toggles are not deps per Edge Case), or `RANGE_CONSTRAINT_RE` (emit `CabalFreezeEntry::RangeConstraint` with the raw range string preserved). Lowercase the captured name per FR-004 + research §R1. Non-matching entries warn-and-skip with `tracing::warn!` carrying the file path.

- [X] T012 [US1] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn build_freeze_component(entry: &CabalFreezeEntry) -> anyhow::Result<PackageDbEntry>` per data-model §4.1 + the boot-library annotation logic. For `ExactPin`: construct PURL `pkg:hackage/<name>@<version>`; set `mikebom:source-type = "hackage-freeze"`, `mikebom:evidence-kind = "cabal-freeze"`. For `RangeConstraint`: construct PURL with `sanitize_purl_version(range)` for the version slot; add `mikebom:sbom-tier = "design"`, `mikebom:requirement-range = <raw range>`. **Per Q1 + FR-014**: when `GHC_STDLIB_ALLOWLIST.iter().any(|s| s.eq_ignore_ascii_case(&entry.name))`, additionally insert `mikebom:ghc-stdlib = "true"` into `extra_annotations`. Lifecycle scope: Runtime (freeze entries are runtime unless context-tagged later).

- [X] T013 [US1] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement basic `fn parse_cabal_manifest(path: &Path) -> anyhow::Result<CabalManifest>` per data-model §2.3 — for US1 scope only `name` + `version` extraction via `CABAL_NAME_RE` + `CABAL_VERSION_RE` (stanza extraction lands in T024). Also detect Hpack-generated source via `HPACK_HEADER_RE` and populate `manifest.hpack_generated = true` when matched (the Q3 warn emission happens in T026; just detect here). Apply FR-013 fallbacks: when `name:` keyword missing → parent-directory basename; when `version:` missing → `"0.0.0-unknown"`.

- [X] T014 [US1] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn build_main_module(manifest: &CabalManifest, cabal_path: &Path, has_lockfile: bool) -> Option<PackageDbEntry>` per data-model §4.5. Construct PURL `pkg:hackage/<name>@<version>` using the (possibly fallback-derived) name + version from `manifest`. Populate `extra_annotations`: `mikebom:component-role = "main-module"`, `mikebom:source-type = "hackage-main-module"`. `sbom_tier`: `"source"` when `has_lockfile == true` else `"design"`. **Per data-model §4.5**: main-modules do NOT carry `mikebom:ghc-stdlib` or `mikebom:stackage-resolver` (they're never boot libraries; the snapshot is a sibling component).

- [X] T015 [US1] [P] Create `mikebom-cli/tests/haskell_cabal_baseline.rs` with synthetic-fixture tests covering SC-001 + Q1 + FR-014: `sc001_baseline_eight_freeze_components` (fixture with `my-app.cabal` declaring 3 direct deps + `cabal.project.freeze` pinning 8 exact-pin entries; assert exactly 8 `pkg:hackage/*` components emit), `sc001_main_module_emission` (assert `pkg:hackage/my-app@1.2.3` main-module with `mikebom:component-role = "main-module"`), `q1_ghc_stdlib_annotation_emitted` (assert boot-library entries — `base`, `text`, `containers` — carry `mikebom:ghc-stdlib = "true"`; non-boot entries like `aeson`, `lens` do NOT). Test module uses `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

  Also wire up `read_all` integration: in `mikebom-cli/src/scan_fs/package_db/mod.rs`, integrate `haskell::read(...)` into the `read_all` dispatcher (place alphabetically after the gradle/golang family and before kotlin_dsl). Pass the same `(rootfs, include_dev, exclude_set)` triple. **Checkpoint**: After T015, `cargo test -p mikebom --test haskell_cabal_baseline` MUST pass (US1 independently complete).

---

## Phase 4: User Story 2 — Operator scans a Stack-managed Haskell project (P2)

**Goal** (SC-002 + SC-010): A scan of a Stack project produces 2 `pkg:hackage/` extra-deps + 1 Stackage snapshot placeholder + 1 main-module = 4 total Haskell-derived components.

**Independent Test**: `cargo test -p mikebom --test haskell_stack_discrimination` passes.

- [X] T016 [US2] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn discover_stack_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf>` + `fn discover_stack_yamls(...)` + `fn discover_package_yamls(...)` using `safe_walk` (per research §R11). Filter by file name: `discover_stack_locks` matches `stack.yaml.lock` (literal); `discover_stack_yamls` matches `stack.yaml` (literal); `discover_package_yamls` matches `package.yaml` (literal — for Q3 Hpack-detect; reader does NOT parse this file). Standard excludes per research §R11.

- [X] T017 [US2] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn parse_stack_lock(path: &Path, stack_yaml_paths: &[PathBuf]) -> anyhow::Result<(Vec<StackLockEntry>, Vec<StackSnapshot>)>` per data-model §1.2 + §2.2 + research §R3. Read the file → parse via `serde_yaml::from_str::<StackYamlLock>` → validate via `validate_stack_lock_shape` from T008 (warn-and-skip if false). For each `snapshots[]` entry: extract `original.resolver` + `completed.sha256` → emit `StackSnapshot`. For each `packages[]` entry: read `original.hackage` (e.g., `"aeson-2.2.0.0"`) and split on the LAST dash to recover `name` + `version` (lowercase name per FR-004); emit `StackLockEntry::Hackage`. Git-source `extra-deps` (`original: {git: ..., commit: ...}`) warn-and-skip per the deferral noted in research §R3. **When `stack.yaml.lock` is absent but `stack.yaml` is present** in the same directory, the function also accepts a path to `stack.yaml` only and extracts the `resolver:` field via YAML, emitting one `StackSnapshot { resolver, sha256: None }` (drives `pkg:generic/<resolver>@unspecified`).

- [X] T018 [US2] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn build_stack_lock_component(entry: &StackLockEntry) -> anyhow::Result<PackageDbEntry>` per data-model §4.2. Identical shape to `build_freeze_component` from T012 except `mikebom:source-type = "hackage-stack-lock"` + `mikebom:evidence-kind = "stack-yaml-lock"`. Boot-library Q1 annotation applies identically — call the same GHC_STDLIB_ALLOWLIST check.

- [X] T019 [US2] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn build_snapshot_placeholder(snapshot: &StackSnapshot) -> anyhow::Result<PackageDbEntry>` per data-model §4.3 + research §R5. PURL shape dispatch: `lts-*` / `nightly-*` resolvers → `pkg:generic/stackage-<resolver>@<sha-or-unspecified>`; `ghc-*` resolvers → `pkg:generic/<resolver>@<sha-or-unspecified>` (NOT `stackage-` prefixed); defensive fallback for other resolver shapes → `pkg:generic/<resolver>@<sha-or-unspecified>`. Populate `extra_annotations`: `mikebom:source-type = "hackage-snapshot"`, `mikebom:evidence-kind = "stack-yaml-lock"`, `mikebom:stackage-resolver = <snapshot.resolver>`. `sbom_tier`: `"source"` when `snapshot.sha256.is_some()` else `"design"`.

- [X] T020 [US2] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, extend `read()` for US2: call `discover_stack_locks` + `discover_stack_yamls` + `discover_package_yamls`. For each `stack.yaml.lock` (or `stack.yaml`-only when no lockfile), invoke `parse_stack_lock` from T017 → emit Stack extra-deps via `build_stack_lock_component` from T018 + emit one snapshot placeholder per `StackSnapshot` via `build_snapshot_placeholder` from T019. Dedup via `seen_purls: HashSet<String>` (Stack `aeson` collides naturally with cabal-freeze `aeson` if both ecosystems are present in the same project — accept the first PURL hit).

- [X] T021 [US2] [P] Create `mikebom-cli/tests/haskell_stack_discrimination.rs` with synthetic-fixture tests covering SC-002 + SC-010: `sc002_stack_baseline_four_components` (fixture with `stack.yaml` resolver `lts-22.0` + `stack.yaml.lock` with snapshot SHA + 2 extra-deps + `my-app.cabal`; assert 4 Haskell-derived components — 2 hackage-stack-lock + 1 hackage-snapshot placeholder + 1 main-module), `sc010_snapshot_placeholder_purl_and_annotations` (assert snapshot placeholder PURL `pkg:generic/stackage-lts-22.0@<sha>` + both `mikebom:source-type = "hackage-snapshot"` AND `mikebom:stackage-resolver = "lts-22.0"`), `stack_nightly_resolver_purl` (fixture with `resolver: nightly-2024-01-15`; assert PURL prefix `pkg:generic/stackage-nightly-2024-01-15@`), `stack_ghc_only_resolver_no_prefix` (fixture with `resolver: ghc-9.6.4`; assert PURL `pkg:generic/ghc-9.6.4@unspecified` — NO `stackage-` prefix), `stack_lock_q1_ghc_stdlib_annotation` (extra-dep `base-4.18.0.0` in `stack.yaml.lock`; assert emitted component carries `mikebom:ghc-stdlib = "true"`). **Checkpoint**: After T021, US2 independently complete.

---

## Phase 5: User Story 3 — Operator scans without lockfile + Q2 multi-stanza + multi-package + Q3 Hpack-detect (P3)

**Goal** (SC-003 + SC-007 + SC-009 + SC-012 + FR-015): `*.cabal`-only projects emit design-tier components with Q2 multi-stanza union + per-stanza lifecycle-scope; multi-package projects emit one main-module per discovered `*.cabal`; test/benchmark deps tag as dev-scope; Hpack-generated `*.cabal` alongside `package.yaml` emits the FR-015 diagnostic.

**Independent Test**: `cargo test -p mikebom --test haskell_tier_fallbacks` passes.

- [X] T022 [US3] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, extend `parse_cabal_manifest` from T013 to populate the `stanzas` field via `extract_stanzas` from T009 per data-model §2.3 + research §R4. The output `CabalManifest.stanzas` carries one `CabalStanza` per `library`/`executable`/`test-suite`/`benchmark`/`foreign-library` block discovered, each with its `build_depends` + `build_tool_depends` extracted.

- [X] T023 [US3] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn collect_design_tier_deps(manifest: &CabalManifest) -> Vec<(DeclaredDep, LifecycleScope)>` per Q2 union + data-model §2.4. For each stanza in `manifest.stanzas`: derive the base scope via `stanza_lifecycle_scope(stanza.kind)`. For each dep in `stanza.build_depends`: add `(dep, base_scope)` to a `HashMap<String, (DeclaredDep, LifecycleScope)>` keyed by `dep.name`; if the name already exists, update the scope via `merge_scope(existing, new)` per most-binding-wins rule. For each dep in `stanza.build_tool_depends`: add `(dep, LifecycleScope::Development)` (build-tool-depends ALWAYS Development per FR-010, regardless of containing stanza). Return the HashMap's values as a Vec.

- [X] T024 [US3] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn build_design_tier_components(manifest: &CabalManifest, cabal_path: &Path) -> Vec<PackageDbEntry>` per data-model §4.4. Call `collect_design_tier_deps` from T023; for each `(dep, scope)`: build PURL `pkg:hackage/<name>@<sanitized-range>`; populate `extra_annotations` with `mikebom:source-type = "hackage-cabal-design"`, `mikebom:evidence-kind = "cabal-pkg-descriptor"`, `mikebom:sbom-tier = "design"`, `mikebom:requirement-range = <raw range>` (or `""` when absent); apply Q1 boot-library annotation if name matches GHC_STDLIB_ALLOWLIST; set `lifecycle_scope = Some(scope)`. The Q2 main-module-`depends` set (per FR-006) is the union of all names in the returned Vec — wire this through `build_main_module` from T014 by extending its signature to accept the union name list and populate `PackageDbEntry.depends`.

- [X] T025 [US3] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, extend `read()` for US3 multi-package + design-tier emission: discover all `*.cabal` files via `discover_cabal_files` from T010 (already covers multi-package per research §R7 filesystem walk approach); for each discovered `*.cabal`, parse via `parse_cabal_manifest`, detect if a sibling `cabal.project.freeze` OR `stack.yaml.lock` exists in any ancestor directory (sets `has_lockfile`), emit the main-module via `build_main_module`, and when `has_lockfile == false` also emit the design-tier components via `build_design_tier_components`. Per the spec Edge Case "Multiple `*.cabal` files in one directory": when multiple `*.cabal` exist in the same dir, pick the alphabetically-first as the main-module source and warn-and-continue for the others.

- [X] T026 [US3] In `mikebom-cli/src/scan_fs/package_db/haskell.rs`, implement `fn emit_hpack_warnings(cabal_manifests: &[(PathBuf, CabalManifest)], package_yaml_paths: &[PathBuf])` per Q3 + FR-015 + research §R4. For each `(cabal_path, manifest)` pair where `manifest.hpack_generated == true`: check if a `package.yaml` exists in the SAME directory as the `cabal_path`. When both conditions match, emit `tracing::warn!` naming both paths with the recommended-action message: `"haskell: Hpack-generated *.cabal detected alongside package.yaml — run 'hpack' to regenerate before scanning if package.yaml has been edited. cabal_path=<...> package_yaml=<...>"`. Call this from `read()` after parsing all manifests (Phase C in data-model §5). The warn is one-shot per matching pair (do NOT re-trigger across multiple call sites in the same scan).

- [X] T027 [US3] [P] Create `mikebom-cli/tests/haskell_tier_fallbacks.rs` with synthetic-fixture tests covering SC-003 + SC-007 + SC-009 + SC-012 + Q3: `sc003_design_tier_from_cabal_only` (fixture with `my-lib.cabal` declaring `build-depends: base, text` + NO lockfile; assert 2 components emit with `mikebom:sbom-tier = "design"` + `mikebom:requirement-range`), `sc007_test_stanza_dev_scope` (fixture with `test-suite spec\n  build-depends: hspec >= 2.10`; assert `hspec` component has CDX `scope == "excluded"` per the milestone-052 dev-scope bridge; assert `--exclude-scope dev` top-level flag suppresses it), `sc009_multi_package_three_subpackages` (fixture with `cabal.project` declaring 3 sub-package paths + each has its own `<subdir>/*.cabal`; assert 3 main-module components emit), `sc012_q2_multi_stanza_union_with_scope_merging` (fixture with multi-stanza `*.cabal` per spec SC-012; assert 6 distinct components — `base`, `text`, `optparse-applicative`, `hspec`, `criterion`, `my-app` self-ref; assert `hspec` + `criterion` have CDX `scope == "excluded"`; assert `base` has CDX `scope == "required"` despite appearing in both library AND test stanzas per most-binding-wins rule), `q3_hpack_detect_emits_warn` (fixture with `package.yaml` + Hpack-generated `*.cabal`; assert scan exits 0 AND stderr contains `haskell: Hpack-generated`). **Checkpoint**: After T027, US3 independently complete.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Edge-case coverage, CDX builder extension, docs, pre-PR gate.

- [X] T028 [P] Create `mikebom-cli/tests/haskell_edge_cases.rs` with: `sc004_no_op_on_non_haskell_tree` (covers FR-008 + SC-004 — fixture has no `*.cabal` / `cabal.project*` / `stack.yaml*`; assert ZERO components with `mikebom:source-type` starting with `"hackage-"` AND ZERO `haskell:` warnings in stderr), `sc005_malformed_freeze_falls_back_to_design_tier` (covers FR-009 + SC-005 — corrupt `cabal.project.freeze` + valid sibling `*.cabal`; assert scan exits 0, warns about parse failure, AND emits design-tier components from the `*.cabal`), `q3_content_shape_skips_non_stack_files` (fixture with a file literally named `stack.yaml.lock` containing valid YAML but no top-level `snapshots:` array; assert ZERO hackage-snapshot components emit AND warn-and-skip diagnostic appears), `flag_only_constraints_skipped` (fixture with `cabal.project.freeze` containing `constraints: foo +bar, baz ==1.0`; assert only `baz` component emits — flag toggle ignored), `range_constraint_in_freeze_emits_design_tier` (fixture with `constraints: text >=2.0 && <2.1`; assert component emits with `mikebom:sbom-tier = "design"` + range preserved), `main_module_version_fallback` (covers FR-013 fallback — `*.cabal` without `version:`; assert main-module PURL contains `@0.0.0-unknown`), `main_module_name_fallback_to_dir_basename` (covers FR-013 fallback — `*.cabal` without `name:`; assert main-module PURL contains the parent-dir basename), `multiple_cabal_files_in_one_dir_alphabetical_wins` (covers Edge Case — two `*.cabal` in same dir; assert alphabetically-first becomes the main-module + the other warns), `boot_library_allowlist_case_insensitive_match` (covers FR-014 — `Win32` allowlist entry; assert that a freeze entry pinning `win32` lowercase matches the allowlist + carries `mikebom:ghc-stdlib = "true"`). Test module uses `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

- [X] T029 In `mikebom-cli/src/generate/cyclonedx/builder.rs`, extend the `mikebom:evidence-kind` allowlist enum (per the milestone-141 + 142 precedent — search for the `"rebar-lock"` / `"sbt-lock"` entries) to include `"cabal-freeze"`, `"stack-yaml-lock"`, and `"cabal-pkg-descriptor"`. Per the F4 empirical lesson from milestone 141: builder's allowlist hard-rejects unknown values via `debug_assert!` — without this extension, the US1 baseline test will panic at scan time. The `mikebom:source-type` value-set is NOT hardcoded in the builder (verified during milestone 141/142); no extension needed there. **Per the deferred decision in plan.md "Source Code Structure" + research §R6**: ALSO verify during T021's US2 test run whether the metadata.rs curated allowlist needs to propagate `mikebom:stackage-resolver` (only relevant if a snapshot placeholder gets promoted to `metadata.component`, which is rare — usually a regular `*.cabal` main-module wins). If empirical observation shows promotion, add the propagation per the milestone-142 F6 pattern.

- [X] T030 Update `docs/reference/sbom-format-mapping.md` per Constitution Principle V + research §R6 — add a new row "Milestone 143 (Haskell)" in Section I documenting the parity-bridge annotations introduced by this milestone: `mikebom:ghc-stdlib` (with justification clause from research §R6 audit: "no native CDX `scope` / SPDX 2.3 `primaryPackagePurpose` / SPDX 3 `software_softwarePurpose` field carries 'is this an ecosystem stdlib member'; the OPERATING-SYSTEM primary-purpose enum is OS-distro scoped and misusing it for language stdlibs would conflict with spec intent"), `mikebom:stackage-resolver` (with justification clause: "no native field carries the curated-bundle identifier this component participated in; `component.group` is vendor-grouping not curation, `Package.sourceInfo` would lose machine-readability"). Cross-reference the milestone-141 `mikebom:erlang-app-dep-kind` + milestone-142 `mikebom:scala-version-source` precedents for the doc shape.

- [X] T031 Run `./scripts/pre-pr.sh` and confirm both `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace` pass clean (zero warnings + every suite `ok. N passed; 0 failed`). Per Constitution mandatory pre-PR gate. Capture the full output (not greppped) per memory `feedback_prepr_gate_full_output.md`. If clippy flags `unwrap_used` inside any new test module, guard with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per project convention. If clippy flags `regex_creation_in_loops`, hoist the affected `OnceLock<Regex>` to function/module scope per the milestone-141 R7 + milestone-142 R8 lesson + research §R9. **Per F2 milestone-142 remediation pattern**: as a final FR-012 enforcement step, run `grep -E 'reqwest|tokio::net|hyper::Client|ureq|isahc' mikebom-cli/src/scan_fs/package_db/haskell.rs` and confirm zero matches — closes the "no network calls during scan" verification gap.

---

## Dependencies

```text
Phase 1 (Setup: T001 → T002)
    ↓
Phase 2 (Foundational: T003 → T004 ‖ T005 ‖ T006 ‖ T007 ‖ T008 ‖ T009 — types first, then parallel helpers)
    ↓
Phase 3 (US1 P1 MVP: T010 → T011 → T012 → T013 → T014 → T015 [with read_all hookup])
    ↓
Phase 4 (US2 P2: T016 → T017 → T018 → T019 → T020 → T021)
    ↓
Phase 5 (US3 P3: T022 → T023 → T024 → T025 → T026 → T027)
    ↓
Phase 6 (Polish: T028 ‖ T029 ‖ T030 → T031)
```

**Notes**:

- US2 depends on US1 because T020 extends the `read()` flow established in T015.
- US3 depends on US2 because T025 extends T020's per-subproject loop with the multi-package + design-tier flow.
- T015 bundles the `read_all` hookup as the final step (mirrors milestone-141 T014 + milestone-142 T014 pattern). After T015, US1 is independently testable.
- T029 (builder.rs allowlist extension) is critical-path for T015 — without it the US1 test panics at scan time per the F4 empirical lesson from milestones 141+142. The tasks.md order keeps it in Polish phase because both can land in the same PR and the test order during CI is alphabetical, but if running tests interactively hoist T029 to run BEFORE T015.

## Parallel Execution Examples

**Phase 2 (Foundational)**: T003 must complete first (types feed every other helper). T004–T009 (6 tasks) touch the same `haskell.rs` file but add INDEPENDENT helpers — drafting-parallelizable, commit sequential due to same-file edits.

**Phase 6 (Polish)**: T028 (new test file) ‖ T029 (builder.rs) ‖ T030 (docs) — different files entirely. T031 (pre-PR gate) MUST run last after every code change.

## Implementation Strategy

**MVP scope**: Phases 1-3 (T001-T015, 15 tasks) — closes the headline "cabal-managed Haskell with `cabal.project.freeze`" case + Q1 GHC-stdlib annotation. ~15 tasks, ~700 LOC including the test fixture.

**Incremental delivery** (after MVP merge):

- Phase 4 (T016-T021) adds Stack lockfile + snapshot placeholder + Q3-style content-shape gate.
- Phase 5 (T022-T027) adds Q2 multi-stanza union + multi-package discovery + Q3 Hpack-detect.
- Phase 6 (T028-T031) tightens edges + docs + pre-PR.

**Single-PR delivery** (recommended, matches milestones 137-142 convention): Ship Phases 1-6 in one PR. Branch is already `143-haskell-reader`; one PR per milestone keeps the changelog clean.

## Format Validation

All 31 tasks above follow the required format: `- [ ] T<NNN> [P?] [Story?] <description with file path>`. Checkbox + ID + optional `[P]` marker + optional `[US1]`/`[US2]`/`[US3]` story label (story label REQUIRED for Phase 3-5 tasks, ABSENT from Phase 1-2 + Phase 6 tasks) + clear file path in every description. Verified.
