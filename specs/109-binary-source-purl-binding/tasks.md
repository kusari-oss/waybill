---
description: "Tasks for milestone 109 — binary-source PURL binding via cmake build-directory observation"
---

# Tasks: Bind binary-tier C/C++ components to source-tier PURLs via cmake build-directory observation

**Input**: Design documents from `/specs/109-binary-source-purl-binding/`
**Prerequisites**: plan.md (required), spec.md, research.md, data-model.md, contracts/{walker-protocol,attribution-rules,annotation-emission}.md, quickstart.md.

**Tests**: integration tests required — the milestone's correctness rests on multi-component scan outputs that unit tests can't cover. Each user-story phase includes the integration test(s) that prove the story's contract.

**Organization**: Tasks grouped by user story to enable independent implementation + testing increments. The MVP is Phase 3 (US1) — Phases 4-7 each add an independently-shippable property on top of the MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: `[US1]`/`[US2]`/`[US3]`/`[US4]`/`[US5]` for user-story-phase tasks; omit for Setup / Foundational / Polish.

## Path Conventions

This is a Rust workspace CLI; all source paths are under `mikebom-cli/src/` and tests under `mikebom-cli/tests/`. Plan's "Project Structure" section is authoritative.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: project-level wiring + clean working tree.

- [X] T001 Sync `main` (`git checkout main && git pull --ff-only`), drop any stale `109-*` branches, create the working branch (`git checkout -b 109-binary-source-purl-binding main`).
- [X] T002 Confirm pre-PR baseline is clean: run `./scripts/pre-pr.sh` once on a fresh `main` checkout BEFORE adding any code. This is the SC-003 baseline (zero regressions from milestone 109).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the data shapes + module skeleton + matcher signature change. All user-story phases depend on Phase 2 being merged.

- [X] T003 Create `mikebom-cli/src/scan_fs/binary/source_binding/mod.rs`. Declare sub-modules `cmake_observer` and `registry`. Re-export the public types `CmakeBuildDirObservation`, `BuildAttributionRegistry`, `BuildDirObserver` trait (per research.md §R5). Add `#[allow(dead_code)]` on every item until Phase 3 wires them.
- [X] T004 [P] Define `CmakeBuildDirObservation` struct in `mikebom-cli/src/scan_fs/binary/source_binding/mod.rs` per `data-model.md`'s schema (library_name, source_tier_purl, source_mechanism, build_artifact_dir, cmake_project_build_root). Derive Debug + Clone. Document validation rules from data-model.md in doc comments.
- [X] T005 [P] Define `BuildAttributionRegistry` struct + `lookup(library: &str, binary_path: &Path) -> Option<&CmakeBuildDirObservation>` method in `mikebom-cli/src/scan_fs/binary/source_binding/registry.rs`. Implement the path-ancestor closest-match tie-break per research.md §R4. Add unit tests for: empty-registry-returns-none, single-observation-hit, multi-observation-pick-closest-ancestor, library-name-case-insensitive.
- [X] T006 [P] Define the `BuildDirObserver` trait in `mikebom-cli/src/scan_fs/binary/source_binding/mod.rs` per research.md §R5: `fn observe(&self, scan_root: &Path, source_declarations: &[PackageDbEntry]) -> Vec<CmakeBuildDirObservation>`. Trait is `pub(crate)` (internal to mikebom-cli for now; future Bazel/Meson observers land in this module).
- [X] T007 Modify `mikebom-cli/src/scan_fs/binary/binary/mod.rs` to declare `pub(crate) mod source_binding;` between `pub(crate) mod fingerprints;` and `pub mod jdk_collapse;`. Verify cargo +stable build clean (the new module is dead-code-allowed; build succeeds).
- [X] T008 Modify `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs::scan_with_corpus` signature to accept two new optional parameters: `attribution_registry: Option<&BuildAttributionRegistry>` and `binary_path: Option<&Path>`. Both `None` preserves milestone-108 behavior exactly. Update the existing `scan(...)` wrapper to pass `None, None` (preserves byte-identity for all existing callers). Update all in-tree callers' signatures.
- [X] T009 Implement the in-place rewrite logic inside `symbol_fingerprint.rs::scan_with_corpus`: after the matcher loop produces `Vec<SymbolFingerprintMatch>` and BEFORE returning, iterate matches and rewrite `match.target_purl` from `pkg:generic/<library>` to `observation.source_tier_purl` when `registry.lookup(match.library, binary_path)` returns `Some(_)`. The rewrite happens only when BOTH `attribution_registry` AND `binary_path` are `Some(_)`. Per `contracts/attribution-rules.md`'s join-key contract.

**Checkpoint**: cargo +stable build clean. cargo +stable test --workspace passes (existing matcher tests continue to work because `scan(...)` wrapper passes `None`). Foundational layer is dormant — no behavioral change in production until Phase 3 wires the registry construction.

---

## Phase 3: User Story 1 — Cross-tier PURL equality after a project-root scan (Priority: P1) 🎯 MVP

**Goal**: scanning the cmake-demo project root with `--fingerprints-corpus` emits one zlib component with PURL `pkg:github/madler/zlib@v1.3.1` instead of separate source-tier + binary-tier components.

**Independent Test**: SC-001 — `mikebom sbom scan --path . --fingerprints-corpus --output sbom.cdx.json` against the cmake-demo project root; assert `jq '.components | map(select(.name == "zlib")) | length'` returns `1` and the surviving component's PURL is `pkg:github/madler/zlib@v1.3.1`. Without this phase, the same scan returns `2`.

### Walker implementation

- [X] T010 [US1] Implement the cmake-build-dir discovery algorithm in `mikebom-cli/src/scan_fs/binary/source_binding/cmake_observer.rs` per `contracts/walker-protocol.md` §"Discovery algorithm": walk scan_root with bounded recursion (depth 6 per research.md §R1); at each directory check `<dir>/CMakeCache.txt` + `<dir>/_deps/` co-presence; record each match as a cmake-project build root; stop descending into matched build dirs. Lexical sort of results for determinism per `contracts/walker-protocol.md` §"Discovery determinism".
- [X] T011 [US1] Implement the per-declaration observation phase in the same file per `contracts/walker-protocol.md` §"Per-declaration observation": for each (cmake declaration, build root) pair, check `<build_root>/_deps/<name>-build/` is a directory; if yes, emit a `CmakeBuildDirObservation`; if no, emit nothing (declared-but-unbuilt falls through to milestone-108 generic-PURL path naturally). Use the `BuildDirObserver` trait from T006 as the implementer of this logic.
- [X] T012 [US1] Add unit tests in `cmake_observer.rs::tests` against synthetic `tempfile::TempDir` fixtures (no real cmake needed per research.md §R6): empty-scan-root-no-observations, cmake-with-fetchcontent-builds-emits-observation, cmake-without-build-dir-emits-nothing, depth-limit-respected, symlink-cycle-doesn't-loop, permission-denied-warns-and-skips.
- [X] T013 [US1] Implement `source_binding::build_attribution_registry(scan_root, cmake_declarations) -> BuildAttributionRegistry` in `mikebom-cli/src/scan_fs/binary/source_binding/mod.rs`. Constructs the registry by running the cmake observer + indexing observations by lowercased library_name into the registry's `BTreeMap`. Returns an empty registry when no cmake projects are found in the scan root (the common no-cmake-project case).

### Caller wiring

- [X] T014 [US1] Modify `mikebom-cli/src/scan_fs/binary/binary/mod.rs::read()` to construct the `BuildAttributionRegistry` once per scan, BEFORE the per-binary loop. Hoist the construction next to the existing `fingerprints::load_corpus(...)` call (~line 132). When `external_corpus.is_some()`, also build the attribution registry from the cmake reader's parsed declarations (visible via the same scan-time package-db machinery the matcher already consumes). When the external corpus is off, skip registry construction.
- [X] T015 [US1] Modify the per-binary loop in `binary/mod.rs::read()` to pass `Some(&attribution_registry)` and `Some(&binary_path)` into `symbol_fingerprint::scan_with_corpus(...)` when the registry was constructed. When the registry is `None`, pass `None, None` (preserves milestone-108 behavior).
- [X] T016 [US1] Verify `entry::symbol_match_to_entry` reads `match.target_purl` verbatim (it should already; the rewrite happens inside the matcher). If `target_purl` is empty when attribution doesn't fire (defensive), `symbol_match_to_entry` falls back to constructing the `pkg:generic/<library>` PURL from `match.library`. Confirm no regression from T009's rewrite path.

### Integration test (US1 MVP-shippable proof)

- [X] T017 [US1] Add `mikebom-cli/tests/binary_source_binding_cmake.rs`. Test `attribution_fires_when_cmake_decl_and_build_dir_present`: construct a synthetic source tree with `CMakeLists.txt` containing `FetchContent_Declare(zlib GIT_REPOSITORY https://github.com/madler/zlib.git GIT_TAG v1.3.1)`; construct a synthetic build dir with `CMakeCache.txt` + `_deps/zlib-src/` + `_deps/zlib-build/` (placeholder files); copy a real macOS/Linux binary that exports zlib symbols (reuse cmake-demo's `crc-demo`) into a per-test path that lives UNDER the synthetic build dir; run `mikebom sbom scan --path <synthetic-project-root> --fingerprints-corpus --no-deep-hash`; assert the emitted SBOM contains EXACTLY ONE zlib component with PURL `pkg:github/madler/zlib@v1.3.1` and ZERO components with PURL `pkg:generic/zlib`.
- [X] T018 [US1] Add `attribution_falls_back_when_build_dir_absent` in the same file: synthetic source tree with `FetchContent_Declare(zlib ...)` BUT no `build/` dir; assert the SBOM contains the source-tier `pkg:github/madler/zlib@v1.3.1` component AND ZERO binary-tier `pkg:generic/zlib` component (declared-but-not-built; no fingerprint match because no binary was scanned).

**Checkpoint**: US1 shippable. The MVP is verifiable end-to-end. PR title (proposed): `feat(source-binding): cmake _deps/ observer + matcher PURL rewrite (milestone 109 US1)`.

---

## Phase 4: User Story 2 — Consumer joins source + binary SBOMs by PURL equality (Priority: P1)

**Goal**: an external consumer running `jq` set-difference between a source-tier and a binary-tier SBOM gets only genuine signals (no phantom mismatches from PURL form drift).

**Independent Test**: SC-002 — produce two SBOMs from the cmake-demo project (one source-only, one project-root with binary scan); assert their PURL sets equality-join (computed via shell + `jq`) on every FetchContent-declared library with 100% recall.

- [X] T019 [US2] Add `consumer_equality_join_recovers_zero_phantom_mismatches` in `mikebom-cli/tests/binary_source_binding_cmake.rs`: emit two SBOMs against the same synthetic fixture from T017 — (a) `mikebom sbom scan --path src/ --no-deep-hash` (source-only — temporarily moves the synthetic build dir out of the scan), (b) `mikebom sbom scan --path . --fingerprints-corpus --no-deep-hash` (project root). Compute `jq -r '.components[].purl' source.cdx.json | sort` for each. Assert the set difference of the source-tier PURLs ⊆ binary-tier PURLs is empty (every source-tier PURL appears in the binary-tier SBOM).
- [X] T020 [US2] Add `consumer_detects_declared_but_not_linked_dep` in the same file: synthetic fixture with TWO FetchContent declarations (zlib + libcurl) but the binary only links zlib (libcurl's `_deps/libcurl-build/` doesn't exist OR the binary has no libcurl symbols). Run the same two-SBOM scan; assert libcurl appears in source-tier-not-in-binary-tier set-difference (legitimate "declared but not linked" signal) AND zlib does NOT appear in either side's difference (correctly joined).

**Checkpoint**: US2 shippable. Consumer-facing outcome verified end-to-end. Could fold into US1's PR if branching gets noisy.

---

## Phase 5: User Story 3 — Binary-only scans preserve pre-109 behavior (Priority: P2)

**Goal**: SC-003 byte-identity for non-opt-in scans + SC-004 byte-identity for single-binary scans. The milestone-108 contract MUST hold unchanged.

**Independent Test**: existing 33 byte-identity goldens pass byte-identically AFTER milestone 109 lands. Plus a new single-binary scan with `--fingerprints-corpus` produces output byte-identical to alpha.44.

- [X] T021 [US3] Add `mikebom-cli/tests/binary_source_binding_regression.rs`. Test `no_opt_in_scan_byte_identical_to_alpha_44`: capture the alpha.44 output of `mikebom sbom scan --path mikebom-cmake-demo/ --no-deep-hash` (no `--fingerprints-corpus`) as a golden; assert that the same scan post-milestone-109 produces byte-identical output (modulo MIKEBOM_FIXED_TIMESTAMP-managed timestamps + the always-random `serialNumber`, stripped before compare per the milestone-107 byte-identity convention).
- [X] T022 [US3] Add `single_binary_scan_with_corpus_byte_identical_to_alpha_44`: copy a single binary (no source tree, no cmake build dir) to a tempdir; run `mikebom sbom scan --path <tempdir> --fingerprints-corpus --no-deep-hash`; assert the output matches what alpha.44 produced for the same input (the registry is empty → fallback path → milestone-108 behavior).
- [X] T023 [US3] Verify the existing 33 byte-identity goldens (11 CDX + 11 SPDX 2.3 + 11 SPDX 3) pass byte-identically — they should because none of the existing golden fixtures include a cmake-build-dir scenario. Re-running `cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression` post-Phase-2 (without `MIKEBOM_UPDATE_*_GOLDENS=1`) MUST pass.

**Checkpoint**: US3 shippable. SC-003 + SC-004 regression guarantees enforced.

---

## Phase 6: User Story 4 — Attribution is transparent and auditable (Priority: P2)

**Goal**: each cross-attributed component carries both the source-tier `mikebom:source-mechanism` AND the binary-tier `mikebom:fingerprint-corpus-sha` annotations per FR-004 + `contracts/annotation-emission.md`.

**Independent Test**: inspect the cmake-demo's US1 output SBOM; assert the merged zlib component carries `mikebom:source-mechanism = "cmake-fetchcontent-git"` AND `mikebom:fingerprint-corpus-sha = "<12-hex>"` AND both `mikebom:evidence-kind` rows (cmake-fetchcontent-git + symbol-fingerprint).

- [X] T024 [US4] Add `attributed_component_carries_both_source_and_binary_evidence` in `mikebom-cli/tests/binary_source_binding_cmake.rs`: run the synthetic-fixture scan from T017; jq-extract the zlib component's properties; assert all 5 expected annotations are present per `contracts/annotation-emission.md`'s "Per-format emission" table (mikebom:source-mechanism, both mikebom:evidence-kind rows, mikebom:fingerprint-symbols-matched, mikebom:fingerprint-corpus-sha).
- [X] T025 [US4] Verify the milestone-105 dedup pipeline merges the source-tier + binary-tier components correctly when both share the same PURL. If the existing pipeline does NOT exhibit this merge behavior, add a minimal targeted fix to the dedup-pipeline merge logic (research.md §R3 noted this as TBD — verify empirically; the expected behavior aligns with the milestone-105 spec). Document the verification (or the fix) in the PR description.
- [X] T026 [US4] Verify cross-format symmetry by running `mikebom sbom scan` with `--format cyclonedx-json,spdx-2.3-json,spdx-3-json` against the synthetic fixture; jq/grep the zlib component's annotations in each output; assert FR-005 cross-format parity (the same `mikebom:source-mechanism` value appears as a CDX property, a SPDX 2.3 annotation, and a SPDX 3 graph-element Annotation respectively). Reuses the existing parity-extractors infrastructure — no new C-row needed.

**Checkpoint**: US4 shippable. Transparency contract verified across all three formats.

---

## Phase 7: User Story 5 — Forward-compat for non-cmake build systems (Priority: P3)

**Goal**: the architectural trait surface (`BuildDirObserver` from T006) is observer-agnostic; a future Bazel/Meson observer lands without rework.

**Independent Test**: code-review check — the `cmake_observer.rs` file contains zero references to anything Bazel/Meson-specific in the shared `registry.rs` or `mod.rs`. The trait can be implemented by a hypothetical `bazel_observer.rs` without modifying existing files.

- [X] T027 [US5] Add `bazel_observer_could_implement_trait_without_modifying_registry` integration-level smoke test in `mikebom-cli/src/scan_fs/binary/source_binding/mod.rs::tests`: define a stub `struct StubBazelObserver;` that implements `BuildDirObserver` returning a hardcoded `Vec<CmakeBuildDirObservation>` (the type name is generic enough to reuse). Pass it to `build_attribution_registry` via the trait. Assert the registry indexes the stub observations correctly. This proves the architectural contract without committing to a real Bazel reader.
- [X] T028 [US5] Add an architectural comment at the top of `source_binding/mod.rs` explicitly documenting the forward-compat extension path (Bazel/Meson lands as a sibling observer file; the trait + registry stay observer-agnostic). One paragraph; not boilerplate.

**Checkpoint**: US5 shippable. FR-012 forward-compat constraint architecturally satisfied.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: documentation cross-links + FR-014 offline-mode audit extension + release coordination.

- [X] T029 Update `docs/ecosystems.md` — extend the existing milestone-099/108 "Binary analysis — symbol-fingerprint corpus" subsection with a one-paragraph addition explaining the milestone-109 cmake-source-tier attribution and linking to `specs/109-binary-source-purl-binding/quickstart.md`. Same shape as the milestone-108 cross-link.
- [X] T030 [P] Update `docs/reference/identifiers.md` §11 — add a small subsection (§11.7?) explaining the milestone-109 attribution behavior: same `mikebom:fingerprint-corpus-sha` annotation as milestone 108, but components covered by cmake source-tier declarations now carry the source-tier PURL with both source-mechanism + fingerprint-corpus-sha annotations. Consumers don't need new decode logic.
- [X] T031 [P] Extend `mikebom-cli/tests/offline_mode_audit_ecosystem_108.rs`'s `ALL_FINGERPRINTS_FILES` list (or create a parallel `offline_mode_audit_ecosystem_109.rs`) covering the new `source_binding/` sub-module files. Per `contracts/walker-protocol.md`'s "Bounds + budgets": this milestone makes ZERO new network calls; all four new files (`source_binding/mod.rs`, `cmake_observer.rs`, `registry.rs`, plus the integration tests) MUST be free of `reqwest::` / `tokio::net::` / `Command::new` etc. Tripwire test asserts this. Same shape as milestone 108's audit.
- [ ] T032 [P] Update `mikebom-cmake-demo`'s README in the sibling repo (NOT in this branch — separate sibling-repo PR): the cmake-demo's "What this demo does NOT cover" section currently calls out the cross-tier binding gap; remove that bullet (milestone 109 closes it). Add a new "Scenario 5" demonstrating the equality-join recipe from quickstart.md §"Scenario 2".
- [X] T033 Verify the const-growth-guard from milestone 108 still passes (it should — milestone 109 doesn't touch the bundled `FINGERPRINTS` const).
- [X] T034 Mark all tasks T003-T031 with `[X]` in `specs/109-binary-source-purl-binding/tasks.md` with one-line completion notes for any deviations from the plan.
- [X] T035 Run `./scripts/pre-pr.sh` clean. Open polish PR titled `docs+test: milestone 109 polish — docs cross-link + FR-014 audit extension`.

**Checkpoint**: all polish in place. Ready for release-cut decision (alpha.45 or bundle with subsequent work).

---

## Phase 9: Release (optional — only if cutting alpha.45 immediately)

Mirrors the milestone-108 Phase 9 release-cut pattern. Skip if bundling with subsequent milestones.

- [ ] T036 Create release branch `release/0.1.0-alpha.45` off main.
- [ ] T037 Bump `Cargo.toml` workspace version from `0.1.0-alpha.44` to `0.1.0-alpha.45`. Run `cargo +stable build` to update `Cargo.lock`.
- [ ] T038 Regenerate the 33 byte-identity goldens via `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression`. Verify deltas are version-bump-only.
- [ ] T039 Update `CHANGELOG.md` with the `[0.1.0-alpha.45]` entry covering milestone-109 PRs in order (foundational + US1 + US2 + US3 + US4 + US5 + polish, however many ship as separate PRs). Mirrors the milestone-108 alpha.44 CHANGELOG shape.
- [ ] T040 Run `./scripts/pre-pr.sh` clean. Open release PR titled `release: bump workspace to v0.1.0-alpha.45 + regen 33 byte-identity goldens`. After merge: tag `v0.1.0-alpha.45`, push, verify the four release artifacts (workflow + GitHub Release + GHCR image + cosign sig) per the alpha.44 verification pattern.

**Checkpoint**: milestone 109 fully delivered.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)** — no external blockers; assumes alpha.44 is merged to main.
- **Phase 2 (Foundational)** — blocks every user story. Sub-tasks T004 + T005 + T006 can land in parallel within the same PR (`[P]` markers).
- **Phase 3 (US1, MVP)** — depends on Phase 2 merged. Within Phase 3, T010 + T011 sequential (same file); T012 (tests) parallel with T013 (registry constructor); T014 + T015 sequential (same file at `binary/mod.rs`); T016 verification at the end.
- **Phase 4 (US2)** — depends on Phase 3 merged. Pure-test phase; could fold into the US1 PR if branching is noisy.
- **Phase 5 (US3)** — depends on Phase 2 merged (regression tests can land before US1 to enforce SC-003 throughout the development of US1, but they actually exercise the runtime registry-construction path so depend on T013).
- **Phase 6 (US4)** — depends on Phase 3 merged + T025's dedup-pipeline verification (the trickiest task in the milestone; may or may not require a pipeline fix).
- **Phase 7 (US5)** — depends on Phase 2's `BuildDirObserver` trait merged. Pure architectural verification; could land any time after Phase 2.
- **Phase 8 (Polish)** — depends on US1 + US3 + US4 merged. Multiple parallel-eligible sub-tasks.
- **Phase 9 (Release)** — depends on Phase 8 merged. Optional (could bundle with subsequent milestones).

### Parallelizable batches

- Phase 2 in one PR: T004 + T005 + T006 parallel (different files); T007 + T008 + T009 sequential (same files).
- Phase 3 US1 implementation: T010 + T011 sequential; T012 + T013 parallel; T014 + T015 sequential; T017 + T018 parallel (same file but independent tests).
- Phase 4 US2: T019 + T020 parallel.
- Phase 5 US3: T021 + T022 parallel; T023 sequential after T021/T022.
- Phase 6 US4: T024 + T026 parallel; T025 sequential (may require pipeline change).
- Phase 8 Polish: T029 + T030 + T031 + T032 parallel (different files / repos); T033-T035 sequential.

### MVP suggestion

**Phase 1 + Phase 2 + Phase 3 = MVP-shippable**. Foundational+US1 = one PR (~10 days of focused work given milestone-108's velocity baseline). US2/US3/US4/US5 each ship as sequential follow-on PRs (~half-day each on top of the MVP plumbing).

### Implementation strategy

- **Incremental delivery**: ship Phase 2 + Phase 3 as the foundational PR (MVP — milestone-108 has byte-identical fallback when registry is empty, so this is safe to merge as a no-op for non-cmake-project scans).
- **Each subsequent PR adds one verifiable property** (US2 = consumer recipe, US3 = regression guards, US4 = transparency, US5 = forward-compat). Each is independently testable + reviewable.
- **Polish PR**: docs + FR-014 audit at the end, mirrors milestone-108 polish PR shape.
- **Release**: alpha.45 cut after polish merges, OR bundle with subsequent milestones.
