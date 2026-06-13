# Tasks: Automatic binary-name binding via produces-binaries annotation

**Input**: Design documents from `/specs/116-produces-binaries/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/property.md ✓, contracts/binder.md ✓, quickstart.md ✓

**Tests**: Per spec Assumption "A small negative test … verifies the backwards-compatibility (SC-005) acceptance criterion. A full integration test for every ecosystem (US1 + US2 + US3) verifies the per-ecosystem extraction. The cross-tier binder gets its own focused test for the alias-source distinction (FR-003)." Integration-test tasks are explicit per phase.

**PR-split strategy**: Per plan.md § "PR-split strategy", this tasks list spans **three sequential PRs**:
- **PR-A** (T001–T024): Setup + Foundational + US1 (Cargo) + polish. MVP — after PR-A merges, the issue body's Rust workflow is closed end-to-end.
- **PR-B** (T025–T044): US2 (npm + pip + gem + maven) + polish.
- **PR-C** (T045–T053): US3 (Go) + polish.

Each PR is independently mergeable; the tasks within each PR are tracked together with `[X]` as they're completed.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

Single-project Rust workspace at repo root. Affected paths:
- `mikebom-cli/src/binding/{mod.rs,verify.rs}` (foundational binder extensions)
- `mikebom-cli/src/scan_fs/package_db/{cargo.rs,npm/walk.rs,pip/mod.rs,gem.rs,maven.rs,golang/legacy.rs}` (per-ecosystem extractors)
- `mikebom-cli/tests/produces_binaries_*.rs` (integration tests)
- `mikebom-cli/tests/fixtures/produces_binaries/{cargo,npm,pip,gem,maven,golang}/` (vendored test fixtures)
- `docs/reference/sbom-format-mapping.md` (Constitution Principle V audit citation)
- `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` (milestone-115 allow-list — touched only in PR-C if Go extractor adds a new walker function)

---

# PR-A: Foundation + US1 (Cargo)

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish helpers + docs row that every subsequent extractor depends on.

- [X] T001 Add the `normalize_produces_binaries()` shared helper at a new module `mikebom-cli/src/scan_fs/produces_binaries.rs`. Signature: `fn normalize_produces_binaries(names: impl IntoIterator<Item = String>) -> Vec<String>`. Behavior per contracts/property.md § "Value invariants": lowercase ASCII, strip trailing `.exe`/`.jar` (case-insensitive), dedupe, lex-sort. Returns empty `Vec` if all inputs strip to empty. Used by every per-ecosystem extractor in this feature so the normalization logic lives in one place. Include `#[cfg(test)]` unit tests covering: empty input → empty output; mixed case → lowercase; `.exe`/`.Exe`/`.jar` suffixes → stripped; duplicates → deduped; unsorted input → sorted; non-ASCII characters → stripped (per `^[a-z0-9][a-z0-9_-]*$` invariant).

- [X] T002 Add a `mikebom:produces-binaries` row to `docs/reference/sbom-format-mapping.md` per Constitution Principle V bullet 5 documentation requirement. Cite research.md § Decision 1's audit conclusion (no native CDX 1.6 or SPDX 2.3/3.x field carries "list of executable names this package produces"; closest CDX `externalReferences[type=executable]` is URL-shaped not name-shaped). Follow the existing row format used by C40 (`mikebom:component-role`) at the same file.

- [X] T003 Create the fixture-directory infrastructure at `mikebom-cli/tests/fixtures/produces_binaries/` with subdirs `cargo/`, `npm/`, `pip/`, `gem/`, `maven/`, `golang/`, each containing a `README.md` describing the fixture's intent. Cargo subdir is populated in T011; others stay placeholder until PR-B/PR-C.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend `SourceDocumentBinding`, `SourceSbomContext`, and `attach_bindings_to_components` so the cross-tier binder can consume produces-binaries declarations from ANY ecosystem. US1+US2+US3 all depend on this phase being complete.

**⚠️ CRITICAL**: No user-story extractor work begins until T004–T009 are complete.

- [X] T004 Add the `AliasSource` enum at `mikebom-cli/src/binding/mod.rs` per data-model.md § Entity 2: `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "kebab-case")] pub enum AliasSource { OperatorSupplied, AutomaticFromProducesBinaries }`. Place adjacent to the existing `BindingStrength` enum at lines 138-163 for visual grouping.

- [X] T005 Extend `SourceDocumentBinding` at `mikebom-cli/src/binding/mod.rs:187-217` with one new optional field: `#[serde(default, skip_serializing_if = "Option::is_none")] pub alias_source: Option<AliasSource>`. Mirror milestone-111's `alias_from`/`alias_to` serde pattern exactly so pre-feature SBOMs deserialize cleanly. Add a doc-comment explaining the paired-presence invariant (`alias_source` is `Some` iff `alias_from` is `Some`).

- [X] T006 Extend `SourceSbomContext` at `mikebom-cli/src/binding/verify.rs:460-474` with a new field `binary_name_to_purl: HashMap<String, Vec<Purl>>` per data-model.md § Entity 3. Vec-valued (not single `Purl`) because FR-013's name-collision case needs all candidates retained for the audit trail.

- [X] T007 Extend `SourceSbomContext::load()` at `mikebom-cli/src/binding/verify.rs:478-507` to populate `binary_name_to_purl` while parsing the source SBOM. For each component, look for a `mikebom:produces-binaries` property (in CDX `properties[]`; SPDX path uses the existing `MikebomAnnotationCommentV1` envelope wrapper). Parse the value as a JSON array of strings. For each entry, push the component's PURL into `binary_name_to_purl[entry]`. Handle malformed values defensively: log a `tracing::warn!` and skip the entry (do not panic, do not propagate the error — backwards compat with old/non-mikebom SBOMs requires graceful degradation).

- [X] T008 Extend `SourceSbomContext::binding_for_purl()` at `mikebom-cli/src/binding/verify.rs:520-544` with the auto-alias fallback branch per contracts/binder.md § "Pipeline overview". When the existing exact-PURL match returns `Unknown { source-not-found-in-bind-target }` (line 524) AND the incoming `purl` is shaped `pkg:generic/<name>`: normalize `<name>` via `to_lowercase()` + strip trailing `.exe`/`.jar` (case-insensitive); look up in `binary_name_to_purl`; return per the collision policy in contracts/binder.md § "Collision policy". On single-candidate match: produce a binding result with `alias_from = original purl`, `alias_to = matched candidate`, `alias_source = AutomaticFromProducesBinaries`. On multi-candidate match: produce `Weak` strength with `reason = "multiple-source-candidates-for-binary-name"`, `alias_to = first candidate`, `alias_source = AutomaticFromProducesBinaries`. On no match: return the original `Unknown` unchanged.

- [X] T009 Update `attach_bindings_to_components()` at `mikebom-cli/src/cli/scan_cmd.rs:2317-2389` to stamp the `alias_source` field consistently. When the operator-supplied `--pkg-alias` path runs (lines 2337-2343 + 2359-2374 — milestone-111 logic), set `alias_source = OperatorSupplied`. When the auto-alias path runs (returned from `binding_for_purl()` per T008), the `alias_source` is already `AutomaticFromProducesBinaries` from T008 — no extra work here. This task verifies the operator-precedence rule (FR-004): when both an operator alias AND an automatic alias would produce the same source-side PURL match, the operator path runs FIRST (existing milestone-111 sequencing at line 2337-2343) and the auto-alias path is never consulted for that component.

---

## Phase 3: User Story 1 — Rust operator binds without per-image alias flags (Priority: P1) 🎯 MVP

**Goal**: Cargo per-ecosystem extractor reads `[[bin]]` table + `src/main.rs` default + `src/bin/*.rs` implicit binaries; stamps the canonical `mikebom:produces-binaries` declaration on the main-module component. The foundational machinery from Phase 2 then enables automatic cross-tier binding for the Rust workflow described in issue #225.

**Independent Test**: A Rust fixture project produces a source-tier SBOM whose main-module component carries `mikebom:produces-binaries: ["baz"]`. A synthetic image SBOM containing a `pkg:generic/baz` component binds against the source SBOM with strength `weak` (no hash evidence in fixture) AND `alias_source = automatic-from-produces-binaries`. Backwards-compat test confirms pre-feature SBOMs (no property) bind identically to milestone-072 baseline.

### Implementation for User Story 1

- [X] T010 [US1] Extend `build_cargo_main_module_entry()` at `mikebom-cli/src/scan_fs/package_db/cargo.rs:352` to parse `Cargo.toml`'s `[[bin]]` table entries (per FR-005 source (a) — explicit `name = "..."` table-entry names). The existing `toml::from_str()` at line 356 already deserializes the manifest; extend the deserialization struct to include `bin: Option<Vec<CargoBinEntry>>` where `CargoBinEntry { name: Option<String>, ... }`. Collect each `bin.name` into a `Vec<String>` of explicit binary names.

- [X] T011 [US1] Extend the same `build_cargo_main_module_entry()` to handle the default-binary inference rule (FR-005 source (b)): when `src/main.rs` exists relative to the manifest dir, the package's `name` field is one of the produced binaries. Add the package name to the binary-name list iff `src/main.rs` is a regular file. The relative-path check uses the manifest's parent dir; no recursion.

- [X] T012 [US1] Extend the same `build_cargo_main_module_entry()` to handle the implicit-`src/bin/*.rs` rule (FR-005 source (c)): walk `src/bin/` at depth-1 only via `scan_fs::walk::safe_walk` per research.md § Decision 5. `WalkConfig { max_depth: 1, should_skip: &|p, _| { /* reject anything that's not a regular file with .rs extension */ }, exclude_set }`. For each matched file, add `file_stem()` as a binary name. Per Cargo docs, only depth-1 `*.rs` files are implicit binaries; subdirectories of `src/bin/` are NOT.

- [X] T013 [US1] In the same `build_cargo_main_module_entry()`, after T010+T011+T012 have collected all candidate names: call `normalize_produces_binaries()` from T001 to lowercase + strip suffixes + sort + dedupe. If the result is non-empty, union-merge with any pre-existing `mikebom:produces-binaries` value in `extra_annotations` per FR-012 (read the existing JSON array, merge into a `HashSet`, re-normalize), then stamp the value as `serde_json::Value::Array(...)` under key `mikebom:produces-binaries`. If empty (library-only crate), DO NOT stamp the property — absence per FR-001 / FR-005 is correct.

- [X] T014 [US1] Create the Cargo fixture at `mikebom-cli/tests/fixtures/produces_binaries/cargo/`. Contents: a minimal Rust project with `Cargo.toml` (`name = "fixture-baz"`, `version = "1.0.0"`, no `[lib]`, one `[[bin]] name = "fixture-baz-alt"`), `src/main.rs` (default-binary trigger), `src/bin/fixture-baz-helper.rs` (implicit-binary trigger). Expected source-tier emission: `["fixture-baz", "fixture-baz-alt", "fixture-baz-helper"]`. Add a `README.md` documenting the fixture's intent and expected output.

- [X] T015 [US1] Create integration test `mikebom-cli/tests/produces_binaries_cargo.rs` covering: (a) source-tier scan of the T014 fixture emits the expected `mikebom:produces-binaries` value; (b) library-only Cargo fixture (a second sub-fixture with `[lib]` and no `[[bin]]`/`src/main.rs`/`src/bin/`) emits NO property; (c) a Cargo workspace with two member crates each declaring `[[bin]]` emits the property on each member's main-module component (verifies the spec clarification Q1 "per-member, not consolidated-onto-workspace-root" decision); (d) the union-merge case (a pre-existing `mikebom:produces-binaries` value in the input gets merged with mikebom's discoveries). Use `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md test-module convention.

- [X] T016 [US1] Add a cross-tier integration test at `mikebom-cli/tests/produces_binaries_cargo.rs` (extend T015's file): construct a synthetic source-tier SBOM with one main-module component declaring `mikebom:produces-binaries: ["fixture-baz"]` AND a synthetic image-tier SBOM with one `pkg:generic/fixture-baz` component. Run `--bind-to-source`. Assert: binding strength is `weak` (no hash evidence in synthetic fixtures); `alias_from = "pkg:generic/fixture-baz"`; `alias_to = "pkg:cargo/fixture-baz@1.0.0"`; `alias_source = "automatic-from-produces-binaries"`. Also test the FR-013 collision case: two source-tier components both declaring `"fixture-baz"`; assert the binding result is `weak` with `reason = "multiple-source-candidates-for-binary-name"`.

- [X] T017 [US1] Add an operator-precedence test at `mikebom-cli/tests/produces_binaries_cargo.rs` (extend the same file): same setup as T016 but ALSO pass `--pkg-alias "pkg:generic/fixture-baz=pkg:cargo/other-baz@2.0.0"` where `other-baz@2.0.0` is a different source-tier component. Assert: binding `alias_to = "pkg:cargo/other-baz@2.0.0"` (operator wins); `alias_source = "operator-supplied"` (NOT automatic). Verifies FR-004 + spec clarification Q3's precedence rule.

- [X] T018 [US1] Create the backwards-compatibility integration test at `mikebom-cli/tests/produces_binaries_backcompat.rs`. Two test functions: (a) a pre-feature source-tier SBOM (no `mikebom:produces-binaries` properties anywhere) + an image scan with `--bind-to-source` — assert bindings are byte-identical to milestone-072 baseline (exact-PURL match only, no auto-alias path engages); (b) a milestone-111-era SBOM with operator `--pkg-alias` already applied + no `alias_source` field — assert it deserializes cleanly via `#[serde(default)]` and presents `alias_source = None` to consumers. This test is the principal SC-005 verification.

**Checkpoint for PR-A**: After T010–T018, the US1 contract is met end-to-end. The Cargo extractor produces the declaration; the binder consumes it; backwards-compat is preserved. PR-A is mergeable.

---

## Phase 4: PR-A Polish

- [X] T019 Update `CLAUDE.md` (project instructions) "Recent Changes" section by re-running `.specify/scripts/bash/update-agent-context.sh claude`. Verify the milestone-116 entry is present.

- [ ] T020 Run `MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh` from the repo root. Verify clippy `--workspace --all-targets -D warnings` passes clean AND `cargo +stable test --workspace` passes clean (all suites `ok. N passed; 0 failed`). Per CLAUDE.md this is MANDATORY before opening any PR.

- [X] T021 Verify the spec quality checklist at `specs/116-produces-binaries/checklists/requirements.md` is still 16/16 PASS after the implementation matched the spec (no spec drift).

- [X] T022 Update `tasks.md` (this file) marking T001–T018 as `[X]` completed.

- [ ] T023 Commit PR-A's changes per CLAUDE.md commit protocol. Commit title: `feat(binding): produces-binaries auto-alias foundation + Cargo extractor (milestone 116 PR-A, US1 of #225)`. Include in the commit body: a summary of the foundational binder change (envelope extension + index + auto-alias resolution), the Cargo extractor's three sources (`[[bin]]` + `src/main.rs` + `src/bin/*.rs`), the backwards-compat verification (T018), and the Principle V audit citation (T002). NO `--no-verify` flag.

- [ ] T024 Open PR-A. Title: `feat(binding): produces-binaries auto-alias foundation + Cargo extractor (milestone 116 PR-A, US1 of #225)`. Body includes: (1) spec link; (2) a `## Summary` listing the binder extension + Cargo extractor + backwards-compat test; (3) a `## Test plan` listing the four US1 acceptance scenarios + the FR-014 backwards-compat check + the FR-004 operator-precedence check + the FR-013 collision check as manually-verified-on-this-PR checklist items; (4) reference to issue #225 (this PR closes part of it; full closure waits for PR-C).

---

# PR-B: US2 (npm + pip + gem + maven)

**Prerequisites**: PR-A merged. The foundational binder machinery + the shared `normalize_produces_binaries()` helper + the docs row are all on main.

## Phase 5: User Story 2 — Polyglot operator binds across npm/pip/gem/maven (Priority: P2)

**Goal**: Four per-ecosystem extractors layer onto the PR-A foundation. Each is independently mergeable (different file in the workspace) but bundled into PR-B for review efficiency.

**Independent Test**: For each ecosystem (npm/pip/gem/maven), a per-ecosystem fixture project's source-tier SBOM declares its produced binary names AND a containerized variant's `--bind-to-source` scan produces a non-Unknown binding for the flagship component without `--pkg-alias` flags.

### npm slice

- [X] T025 [P] [US2] Extend `build_npm_main_module_entry()` at `mikebom-cli/src/scan_fs/package_db/npm/walk.rs:307` to parse `package.json`'s `bin` field per FR-006. Handle both shape forms: (a) string form `"bin": "./bin/baz.js"` → binary name = the package's `name` field (per npm convention; the file path is the implementation, not the name); (b) object form `"bin": {"baz": "./cli.js", "baz-init": "./init.js"}` → each key is one binary name. Use `serde_json::Value`-based handling at the existing `serde_json::from_str()` site at line 311 to support both shapes without adding a custom deserializer. Pass collected names through `normalize_produces_binaries()` (T001) and stamp via the same `extra_annotations` channel pattern from T013.

- [X] T026 [P] [US2] Create the npm fixture at `mikebom-cli/tests/fixtures/produces_binaries/npm/`. Two sub-fixtures: (a) `string-form/` with `package.json` declaring `{"name": "fixture-baz", "version": "1.0.0", "bin": "./bin/cli.js"}` — expected `["fixture-baz"]`; (b) `object-form/` with `{"name": "fixture-baz", "version": "1.0.0", "bin": {"baz": "./cli.js", "baz-init": "./init.js"}}` — expected `["baz", "baz-init"]`. README documents both.

- [X] T027 [P] [US2] Create integration test `mikebom-cli/tests/produces_binaries_npm.rs` covering: (a) string-form fixture emits `["fixture-baz"]`; (b) object-form fixture emits `["baz", "baz-init"]`; (c) library-only npm package (no `bin` field) emits NO property; (d) cross-tier auto-alias test mirroring T016 but for npm. Use the spec's US2 AS1 + AS2 as the acceptance script.

### pip slice

- [X] T028 [P] [US2] Extend `build_pip_main_module_entry()` at `mikebom-cli/src/scan_fs/package_db/pip/mod.rs:399` to parse `pyproject.toml`'s `[project.scripts]` AND `[project.gui-scripts]` keys per FR-007. The existing `toml::from_str()` at line 402 already deserializes the manifest; extend the struct to include `project: Option<PyProjectProject>` with `scripts: Option<BTreeMap<String, String>>` and `gui_scripts: Option<BTreeMap<String, String>>`. Each key is one binary name. Pass through `normalize_produces_binaries()` and stamp.

- [X] T029 [P] [US2] Add a `setup.cfg` fallback: when `pyproject.toml` doesn't declare scripts (or doesn't exist), read `setup.cfg`'s `[options.entry_points]` for `console_scripts` AND `gui_scripts` keys per FR-007's fallback rule. The setup.cfg format is INI-shaped; reuse any existing parser if one exists in the pip reader, otherwise use `std::fs::read_to_string` + a simple line-scanner for `name = module:func`-shaped entries.

- [X] T030 [P] [US2] Create the pip fixture at `mikebom-cli/tests/fixtures/produces_binaries/pip/`. Two sub-fixtures: (a) `pyproject/` with `pyproject.toml` declaring `[project.scripts] baz = "baz.cli:main"` + `[project.gui-scripts] baz-gui = "baz.gui:main"`; (b) `setupcfg-fallback/` with `setup.cfg` declaring `[options.entry_points] console_scripts = baz = baz.cli:main`. Expected outputs documented in README.

- [X] T031 [P] [US2] Create integration test `mikebom-cli/tests/produces_binaries_pip.rs` covering: (a) pyproject fixture emits `["baz", "baz-gui"]`; (b) setup.cfg fallback fixture emits `["baz"]`; (c) library-only pip package emits NO property; (d) cross-tier auto-alias test mirroring T016 but for pip.

### gem slice

- [X] T032 [P] [US2] Extend the gem main-module extractor at `mikebom-cli/src/scan_fs/package_db/gem.rs` (the function that builds the main-module entry per milestone 069 Phase A; locate the function near the existing `parse_gemspec_full` at line 947 and the milestone-069 phase-A emission around line 720+) to parse the gemspec's `executables` array per FR-008. The gemspec is parsed via the existing regex-based pure-Rust parser (`parse_gemspec_full`); add an `executables: Vec<String>` field to the parsed struct and extract via a regex matching `s.executables = ["a", "b", ...]` or `s.executables = %w[a b]` (gemspec syntax). Pass through `normalize_produces_binaries()` and stamp.

- [X] T033 [P] [US2] Create the gem fixture at `mikebom-cli/tests/fixtures/produces_binaries/gem/`. Two sub-fixtures: (a) `with-executables/` with `fixture-baz.gemspec` declaring `s.executables = ["baz", "baz-server"]`; (b) `library-only/` with no executables array. Expected outputs documented in README.

- [X] T034 [P] [US2] Create integration test `mikebom-cli/tests/produces_binaries_gem.rs` covering: (a) with-executables fixture emits `["baz", "baz-server"]`; (b) library-only fixture emits NO property; (c) cross-tier auto-alias test mirroring T016 but for gem.

### maven slice

- [X] T035 [P] [US2] Extend `build_maven_main_module_entry()` at `mikebom-cli/src/scan_fs/package_db/maven.rs` (per milestone 070 Phase A; locate near line 3405+) to parse POM XML for shade-plugin AND jar-plugin `<finalName>` per FR-009. Use the existing `quick-xml` parser at the POM read site. Walk the parsed POM tree for `/project/build/plugins/plugin[artifactId="maven-shade-plugin"]/configuration/finalName` AND `/project/build/plugins/plugin[artifactId="maven-jar-plugin"]/configuration/finalName`. Strip trailing `.jar` from the extracted value (per spec clarification Q2 — extractor emits extensionless canonical names; binder owns suffix translation). Pass through `normalize_produces_binaries()` and stamp.

- [X] T036 [P] [US2] Create the maven fixture at `mikebom-cli/tests/fixtures/produces_binaries/maven/`. Two sub-fixtures: (a) `shade-plugin/` with `pom.xml` configuring `maven-shade-plugin` `<finalName>baz</finalName>`; (b) `jar-plugin/` with `pom.xml` configuring `maven-jar-plugin` `<finalName>baz</finalName>`. Both expected to emit `["baz"]`. README documents both.

- [X] T037 [P] [US2] Create integration test `mikebom-cli/tests/produces_binaries_maven.rs` covering: (a) shade-plugin fixture emits `["baz"]`; (b) jar-plugin fixture emits `["baz"]`; (c) library-jar pom (no `<finalName>` configured) emits NO property; (d) cross-tier auto-alias test mirroring T016, additionally verifying the FR-002 `.jar`-suffix tolerance: image-tier component `pkg:generic/baz.jar` binds to source-tier `pkg:maven/com.acme/baz@1.0.0` declaring `["baz"]` via the binder's suffix-strip in T008.

---

## Phase 6: PR-B Polish

- [X] T038 Run `MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh` from the repo root. Verify clippy + tests pass clean.

- [X] T039 Update `tasks.md` (this file) marking T025–T037 as `[X]` completed.

- [X] T040 Commit PR-B's changes per CLAUDE.md commit protocol. Commit title: `feat(scan_fs): produces-binaries extractors for npm + pip + gem + maven (milestone 116 PR-B, US2 of #225)`. Commit body summarizes the four per-ecosystem extractors + the four fixture sets + the four integration tests.

- [X] T041 Open PR-B. Title matches the commit. Body includes: (1) spec link; (2) `## Summary` listing the four new extractors and their per-ecosystem extraction sources; (3) `## Test plan` listing the five US2 acceptance scenarios as manually-verified checklist items; (4) reference to issue #225 (this PR is one of two remaining).

- [X] T042 Verify post-PR-B that auto-alias works end-to-end on a real multi-ecosystem fixture by running the quickstart.md walkthrough against the four new fixtures. Update quickstart.md if any drift is found.

- [ ] T043 (Optional, in PR-B's diff or a follow-up): bulk-update existing maven-related transitive-parity tests if any expected SBOM output now carries the new property on main-module components. Verify byte-identity goldens with `MIKEBOM_UPDATE_*_GOLDENS` only if the regen is genuinely necessary (the property is opt-in per FR-001; non-main-module-aware tests should be unaffected).

- [ ] T044 (Polish): if PR-B's review pushes back on the diff size, split into four byte-identity-preserving sub-PRs (npm → pip → gem → maven) per plan.md's "Internally sequenceable" allowance. Each sub-PR is one ecosystem's slice + tests.

---

# PR-C: US3 (Go)

**Prerequisites**: PR-A merged. PR-B may or may not be merged — PR-C is independent of PR-B (it touches a different ecosystem extractor).

## Phase 7: User Story 3 — Go operator binds with no flag burden (Priority: P3)

**Goal**: Extend the Go main-module extractor to walk for `package main` declarations and emit the declaration on the main-module component.

**Independent Test**: A Go fixture project with `cmd/baz/main.go` containing `package main` produces a source-tier SBOM declaring `["baz"]`; cross-tier auto-alias binding succeeds without `--pkg-alias`.

### Implementation for User Story 3

- [ ] T045 [US3] Extend `build_main_module_entry()` at `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:953` to walk the project root for `package main` directories per FR-010. Use `scan_fs::walk::safe_walk` (milestone 114) with `max_depth: 4` — covers `cmd/<name>/main.go` (depth 2), `cmd/<name>/<subcmd>/main.go` (depth 3 — deliberately broader than FR-010's narrow scope; over-coverage is harmless), and root-of-repo `main.go`. The `should_skip` closure rejects `vendor/`, `testdata/`, `_`-prefix dirs (matching milestone-091 Go-testdata-skip behavior), and standard build-output dirs. For each `*.go` file matched: read the file's text (capped at ~4 KB — enough for header + build tags + package clause; never the whole file); use multi-line regex `(?m)^package\s+main\b` or scan-lines-until-first-non-blank-non-comment-non-build-tag-line to find the package declaration. Real Go files commonly carry `// +build linux` (legacy) or `//go:build linux` (Go 1.17+) directives + comment blocks BEFORE the `package` line; the scanner MUST skip these. If `package main` is found, the directory's basename is the binary name. Root-of-repo special case (FR-010 acceptance scenario 3): if the root dir itself contains a `*.go` with `package main`, use the directory's basename (the project's local dir name) as the binary name — typically the same as the repo name.

- [ ] T046 [US3] In the same extractor, pass collected directory-basenames through `normalize_produces_binaries()` (T001) and stamp via the same `extra_annotations` pattern from T013. If empty (no `package main` found — module is library-only or all-cmd-less), DO NOT stamp the property.

- [ ] T047 [US3] Audit whether T045's safe_walk usage adds a new walker function visible to the milestone-115 walker-audit grep. If T045 uses `safe_walk` directly without introducing a new `fn walk*` function, no allow-list update is needed. If for some reason a new helper function is introduced (e.g., `fn walk_for_package_main_dirs`), update `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` per the milestone-115 workflow (regenerate via `grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | LC_ALL=C sort -u`). Prefer the no-new-function approach.

- [ ] T048 [US3] Create the Go fixture at `mikebom-cli/tests/fixtures/produces_binaries/golang/`. Three sub-fixtures: (a) `cmd-layout/` with `go.mod` (`module github.com/foo/fixture-baz`) + `cmd/baz/main.go` (`package main`) + `cmd/baz-helper/main.go` (`package main`) — expected `["baz", "baz-helper"]`; (b) `root-main/` with `go.mod` + a top-level `main.go` (`package main`) — expected `[<basename-of-fixture-dir>]`; (c) `library-only/` with `go.mod` + only `package foo` files (no `package main`) — expected NO property.

- [ ] T049 [US3] Create integration test `mikebom-cli/tests/produces_binaries_golang.rs` covering: (a) cmd-layout fixture emits `["baz", "baz-helper"]`; (b) root-main fixture emits the correct basename; (c) library-only fixture emits NO property; (d) cross-tier auto-alias test mirroring T016 for Go.

---

## Phase 8: PR-C Polish

- [ ] T050 Run `MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh`. Verify clippy + tests pass clean.

- [ ] T051 Update `tasks.md` (this file) marking T045–T049 as `[X]` completed.

- [ ] T052 Commit PR-C's changes per CLAUDE.md commit protocol. Commit title: `feat(scan_fs): produces-binaries extractor for Go package-main directories (milestone 116 PR-C, US3 of #225)`.

- [ ] T053 Open PR-C. Title matches the commit. Body includes: (1) spec link; (2) `## Summary` describing the package-main directory walk; (3) `## Test plan` listing the three US3 acceptance scenarios; (4) reference to issue #225 (this PR closes it — all three options of the issue body are now addressed: A via milestone 111, B via 116/PR-A+B+C, C explicitly off-the-table per the issue's recommendation). Suggest closing issue #225 once PR-C merges.

---

## Dependencies & Execution Order

```text
PR-A (T001 - T024)
  Phase 1 Setup:       T001 → T002 → T003     (sequential)
  Phase 2 Foundational: T004 → T005 → T006 → T007 → T008 → T009  (sequential — same files, build-on)
  Phase 3 US1:         T010 → T011 → T012 → T013 (sequential — same Cargo extractor)
                        ↓
                       T014 (fixture, independent)
                        ↓
                       T015 → T016 → T017 → T018 (sequential — same test files)
  Phase 4 Polish:      T019 → T020 → T021 → T022 → T023 → T024 (sequential)

PR-B (T025 - T044) — depends on PR-A merged
  Phase 5 US2 [P]:     T025 [P] → T026 [P] → T027 [P]   (npm slice, sequential within slice)
                        ‖
                       T028 [P] → T029 [P] → T030 [P] → T031 [P]  (pip slice, sequential within slice)
                        ‖
                       T032 [P] → T033 [P] → T034 [P]   (gem slice, sequential within slice)
                        ‖
                       T035 [P] → T036 [P] → T037 [P]   (maven slice, sequential within slice)
  Phase 6 Polish:      T038 → T039 → T040 → T041 → T042 → T043 → T044 (sequential)

PR-C (T045 - T053) — depends on PR-A merged (PR-B optional)
  Phase 7 US3:         T045 → T046 → T047 → T048 → T049 (sequential — same Go extractor + test file)
  Phase 8 Polish:      T050 → T051 → T052 → T053 (sequential)
```

**Sequential chains within PR-A**:
- T001 → T002 → T003 (Setup phase — different files but Setup-phase semantics)
- T004 → T005 → T006 → T007 → T008 → T009 (Foundational — same file `mod.rs` / `verify.rs` / `scan_cmd.rs`)
- T010 → T011 → T012 → T013 (US1 implementation — all extend `build_cargo_main_module_entry()`)
- T014 (fixture, independent of code tasks)
- T015 → T016 → T017 (integration tests, extending the same test file)
- T018 (backwards-compat test, independent file from T015–T017)

**Parallel branches within PR-B**: the four ecosystem slices (T025–T027, T028–T031, T032–T034, T035–T037) operate on different files and have no shared state. All four can land concurrently if multiple contributors are working in parallel.

**Polish phases**: T019 → T020 (PR-A pre-PR), T038 (PR-B pre-PR), T050 (PR-C pre-PR) follow CLAUDE.md mandatory gates.

## Parallel Opportunities

The four US2 ecosystem slices in PR-B are independently parallelizable since each touches a different ecosystem's per-package_db module:

```text
# Inside PR-B, after PR-A merge, all four can land concurrently:
T025/T026/T027 [US2] — npm slice
T028/T029/T030/T031 [US2] — pip slice
T032/T033/T034 [US2] — gem slice
T035/T036/T037 [US2] — maven slice
```

Per-slice tasks within each are sequential (extractor → fixture → test) because they build on each other's artifacts.

No other parallel opportunities exist — the foundational PR-A phase tasks build on each other; the polish phases are linear ratchets.

## Independent Test Criteria

Per the spec's three user stories:

- **US1 (P1) — MVP**: Confirmed by T015 (per-ecosystem emission shape) + T016 (cross-tier auto-alias) + T017 (operator-precedence) + T018 (backwards-compat) all passing.
- **US2 (P2)**: Confirmed by T027 + T031 + T034 + T037 all passing — each ecosystem's mini-acceptance script covers its US2 AS-N.
- **US3 (P3)**: Confirmed by T049 covering the three US3 acceptance scenarios.

## Implementation Strategy

**MVP scope (PR-A)**: T001 → T024. The foundational binder + Cargo extractor + backwards-compat verification + the PR mechanics. After PR-A merges, the issue body's textbook Rust workflow is closed end-to-end and SC-001 holds for Rust.

**PR-B (US2)**: Adds four ecosystems with no binder change. May be sub-split into four byte-identity-preserving sub-PRs if reviewer feedback pushes back on the diff size (T044 explicit allowance). SC-002 holds for the polyglot operator after PR-B merges.

**PR-C (US3)**: Adds Go. SC-002 fully holds. Issue #225 can close after PR-C merges.

**Format validation**: All 53 tasks above use the required checklist format — `- [ ]` checkbox + sequential ID (T001…T053) + optional [P] marker + [US1]/[US2]/[US3] label for user-story tasks (Setup + Foundational + Polish tasks have no story label) + description with exact file path(s).
