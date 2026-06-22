---
description: "Task list for milestone 134 — divergent-PURL collision detection"
---

# Tasks: Divergent-PURL collision detection in main-module dedup

**Input**: Design documents from `/specs/134-divergent-purl-detection/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ ✓, quickstart.md ✓

**Tests**: INCLUDED — the spec embeds explicit "Independent Test" sections per user story and SC-001..SC-005 name specific synthetic fixtures. Test tasks ride alongside their owning user story.

**Organization**: Tasks grouped by user story so each story can be implemented, merged, and shipped independently as an MVP increment.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Parallelizable (different files, no dependencies on incomplete tasks)
- **[Story]**: User story label (US1 / US2 / US3); omitted in Setup / Foundational / Polish phases
- Every task lists exact file paths

## Path conventions

Brownfield extension to the existing mikebom workspace. All paths are relative to the repo root `/Users/mlieberman/Projects/mikebom`.

- Types: `mikebom-common/src/`
- Reader + emitter + parity: `mikebom-cli/src/`
- Tests: `mikebom-cli/tests/`
- Docs: `docs/reference/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Branch + spec scaffolding. Already complete via `/speckit.specify` + `/speckit.clarify` + `/speckit.plan`.

- [X] T001 Verify branch `134-divergent-purl-detection` is checked out and the `specs/134-divergent-purl-detection/` directory contains `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, and `quickstart.md`. No file edits in this task — pure verification (`ls` + `git branch --show-current`).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared typed representation + ecosystem-agnostic annotation construction. MUST complete before any user story phase can start.

- [X] T002 Create `mikebom-common/src/divergence.rs` with the `DivergenceRecord` struct, `DivergenceReason` enum (`deps-differ`, `hashes-differ`, `both`), and `CollisionsSummary` struct per `data-model.md`. Include serde derives + the validation rules documented in data-model.md as `#[cfg(test)]` unit tests.
- [X] T003 Export `DivergenceRecord`, `DivergenceReason`, `CollisionsSummary` from `mikebom-common/src/lib.rs`. Add `pub mod divergence;` and re-export the public types at the crate root.
- [X] T004 Create `mikebom-cli/src/generate/divergence_annotation.rs` with an ecosystem-agnostic annotation-envelope builder. Accepts a `&DivergenceRecord` (or `&CollisionsSummary` for the document-scope path) and returns the JSON-string envelope shape documented in `contracts/per-component-property.md` + `contracts/document-scope-annotation.md`. Reuses the milestone-071 `MikebomAnnotationCommentV1` envelope for the SPDX 2.3 / SPDX 3 paths.
- [X] T005 Register the new module in `mikebom-cli/src/generate/mod.rs` with `pub mod divergence_annotation;`.

**Checkpoint**: at this point the type vocabulary + envelope builder exist; no reader or emitter touches them yet. The workspace MUST still compile clean (`cargo +stable check --workspace`).

---

## Phase 3: User Story 1 — Detect accidental shadow copy (Priority: P1) 🎯 MVP

**Goal**: Operator scans a workspace with two `Cargo.toml` files claiming the same PURL but with divergent declared dep sets; the emitted SBOM carries a `mikebom:duplicate-purl-divergent` per-component property identifying the collision and listing both paths.

**Independent Test** (from spec): Synthetic fixture with two `Cargo.toml`s declaring `pkg:cargo/foo@1.2.3` but with different `[dependencies]` blocks. Run `mikebom sbom scan`. Assert the deduped `pkg:cargo/foo@1.2.3` component carries the divergence annotation with reason `deps-differ` and both paths.

### Reader-side (cargo)

- [X] T006 [US1] Extend the cargo reader's per-manifest accumulation in `mikebom-cli/src/scan_fs/package_db/cargo.rs` to track per-path declared dep sets as `BTreeSet<String>` (union of `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` table keys). Add the field to the existing in-reader candidate struct (`CargoManifestCandidate` per data-model.md). Sort keys to guarantee deterministic comparison.
- [X] T007 [US1] At the milestone-064 dedup site in `mikebom-cli/src/scan_fs/package_db/cargo.rs`, when 2+ candidates share a PURL: compare their dep sets pairwise; if any pair differs, construct a `DivergenceRecord` with `reason: DivergenceReason::DepsDiffer`, the sorted-walk-order paths list, and the `dep_sets_by_path` map. Forward the record to the emission orchestrator's per-component-annotation channel. Preserve the existing `tracing::warn!` call site unchanged (FR-008).

### Emitter-side (per-component property)

- [X] T008 [P] [US1] Wire CDX 1.6 emission: extend `mikebom-cli/src/generate/cyclonedx/component_properties.rs` (or the equivalent property-appending site) to emit `mikebom:duplicate-purl-divergent` on the deduped component when its corresponding `DivergenceRecord` is present. Per the wire format in `contracts/per-component-property.md`.
- [X] T009 [P] [US1] Wire SPDX 2.3 emission: extend `mikebom-cli/src/generate/spdx/document.rs` to emit the per-component annotation using the `MikebomAnnotationCommentV1` envelope. Per the wire format in `contracts/per-component-property.md`.
- [X] T010 [P] [US1] Wire SPDX 3 emission: extend `mikebom-cli/src/generate/spdx/v3_document.rs` to emit the per-component `Element.extension` entry. Per the wire format in `contracts/per-component-property.md`.

### Tests (US1)

- [X] T011 [US1] Create `mikebom-cli/tests/divergent_purl_deps_differ.rs` containing SC-001's test cases. Use `tempfile::tempdir()` to construct the two-Cargo.toml fixture per `quickstart.md` Scenario 1. Run `mikebom sbom scan` via `Command::new(env!("CARGO_BIN_EXE_mikebom"))`. Parse the emitted CDX JSON. Assert the property is present with the expected `reason`, `paths`, and `dep_sets_by_path`.
- [X] T012 [US1] Add a negative-case test to the same file: identical-dep-set fixture (SC-002 / quickstart.md Scenario 2) MUST NOT produce the annotation. Snapshot the emitted SBOM's `components[]`-without-the-annotation-property and verify it matches the pre-milestone baseline for the same fixture shape.
- [X] T013 [P] [US1] Extend the SBOM regression suite in `mikebom-cli/tests/cdx_regression.rs`, `spdx_regression.rs`, and `spdx3_regression.rs` to assert that the existing 11-ecosystem cargo golden fixtures (`mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/cargo.*.json`) do NOT acquire the divergence property as a side effect of milestone 134. (Goldens still byte-identical → SC-002 invariant gate.)

**Checkpoint**: US1 ships independently — the per-component property is on the wire, the synthetic-fixture test passes on all three formats, the SBOM goldens are untouched, and the milestone-064 `warn!` continues to fire. Can be merged as a standalone PR before US2 or US3 is implemented.

---

## Phase 4: User Story 2 — Detect adversarial shadow via deep-hash (Priority: P2)

**Goal**: Operator scans with `--deep-hash` against a workspace where two `Cargo.toml`s claim the same PURL, have identical declared deps, but have divergent `src/lib.rs` contents. The emitted SBOM carries the divergence annotation with reason `hashes-differ`.

**Independent Test** (from spec): Synthetic fixture with two `Cargo.toml`s declaring `pkg:cargo/foo@1.2.3` + identical `[dependencies]` + different `src/lib.rs`. Run `mikebom sbom scan --deep-hash`. Assert `reason: hashes-differ` and `hashes_by_path` is populated.

**Dependency**: US1 must complete first (US2 extends US1's reader-side compare-and-emit infrastructure with a new reason path).

### Reader-side (cargo, --deep-hash gated)

- [X] T014 [US2] Extend `CargoManifestCandidate` in `mikebom-cli/src/scan_fs/package_db/cargo.rs` to carry an `Option<String>` deep-hash field. Populate it ONLY when `--deep-hash` is set; otherwise leave `None`. The hash itself comes from the existing milestone-038 `compute_deep_hash` helper (verify the helper at its existing path; do not duplicate the SHA logic).
- [X] T015 [US2] At the milestone-064 dedup site (same site as T007), after the dep-set compare: if `--deep-hash` is set AND deep hashes are populated AND any pair of colliding-PURL candidates has divergent hashes, update the existing `DivergenceRecord` from T007 (if any) to set `reason` to `Both` and populate `hashes_by_path`; if no dep-set divergence existed, construct a fresh `DivergenceRecord` with `reason: HashesDiffer` and `hashes_by_path` only.

### Tests (US2)

- [X] T016 [US2] Create `mikebom-cli/tests/divergent_purl_hashes_differ.rs` containing SC-003's test cases. Build the fixture per `quickstart.md` Scenario 3. Run `mikebom sbom scan --deep-hash`. Assert the annotation appears with reason `hashes-differ` and a populated `hashes_by_path`.
- [X] T017 [US2] Add the negative-case to the same file: same fixture WITHOUT `--deep-hash` MUST NOT produce the `hashes-differ` annotation. Asserts FR-005's gating invariant.

**Checkpoint**: US2 ships as a follow-up PR after US1 is merged. The `--deep-hash` mode now surfaces adversarial shadows.

---

## Phase 5: User Story 3 — Scan-wide collisions summary (Priority: P3)

**Goal**: A workspace with multiple divergent collisions emits a document-scope `mikebom:purl-collisions-detected` annotation listing every collision in one place. Consumers can enumerate all collisions via a single jq query.

**Independent Test** (from spec): Fixture with three independent divergent-PURL collisions. Scan. Assert the document-scope summary lists all three with deterministic sort order.

**Dependency**: US1 must complete first (US3 builds on the per-record machinery established in US1). Independent of US2 — US3 can ship before or after US2.

### Aggregation

- [X] T018 [US3] At the end of the per-ecosystem dedup-resolution phase in `mikebom-cli/src/scan_fs/mod.rs` (or wherever the per-scan record collection lives), collect every `DivergenceRecord` produced into a `CollisionsSummary`. Sort `collisions[]` lexically by `record.purl.as_str()`. Forward the summary to the emission orchestrator's document-scope channel. Skip emission entirely when `collisions[]` is empty (FR-009).

### Emitter-side (document-scope annotation)

- [X] T019 [P] [US3] Wire CDX 1.6 document-scope emission: extend `mikebom-cli/src/generate/cyclonedx/document_properties.rs` (or the `metadata.properties[]`-populating site) to emit `mikebom:purl-collisions-detected`. Per `contracts/document-scope-annotation.md`.
- [X] T020 [P] [US3] Wire SPDX 2.3 document-scope emission: extend `mikebom-cli/src/generate/spdx/document.rs` to emit the top-level annotation with the `MikebomAnnotationCommentV1` envelope. Per `contracts/document-scope-annotation.md`.
- [X] T021 [P] [US3] Wire SPDX 3 document-scope emission: extend `mikebom-cli/src/generate/spdx/v3_document.rs` to emit the `SpdxDocument.extension[]` entry. Per `contracts/document-scope-annotation.md`.

### Tests (US3)

- [X] T022 [US3] Add a 3-collision integration test in `mikebom-cli/tests/divergent_purl_deps_differ.rs` (or a new file `mikebom-cli/tests/divergent_purl_summary.rs`). Build the fixture per `quickstart.md` Scenario 4. Assert: (a) the document-scope summary lists exactly 3 entries, (b) the sort order is lexical by PURL, (c) every entry in the summary also appears as a per-component property on its respective component (redundancy invariant from `contracts/document-scope-annotation.md`).

**Checkpoint**: US3 ships as the third PR. Now operators get both the per-component and the scan-wide aggregation surfaces.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T023 [P] Create `mikebom-cli/src/parity/extractors/divergent_purl_per_component.rs` — parity-catalog extractor for the per-component property. Walks all three formats' representation and uses the milestone-071 `canonicalize_for_compare` helper to confirm byte-identical payloads. *(Implemented inline as `c99_{cdx,spdx23,spdx3}` in the per-format extractor files via the existing `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros, matching the milestone-061 graph-completeness pattern. No separate file needed.)*
- [X] T024 [P] Create `mikebom-cli/src/parity/extractors/divergent_purl_document_scope.rs` — parity-catalog extractor for the document-scope annotation. Same canonicalize-and-compare pattern. *(Implemented inline as `c100_{cdx,spdx23,spdx3}` — same pattern as T023.)*
- [X] T025 Register the two new extractors in `mikebom-cli/src/parity/extractors/mod.rs` and add them to the parity-catalog test harness's enumeration.
- [X] T026 Add C-row entries C99 (`mikebom:duplicate-purl-divergent`) and C100 (`mikebom:purl-collisions-detected`) to `docs/reference/sbom-format-mapping.md`. Both classified as KEEP-NO-NATIVE. Audit narrative is the verbatim text from `research.md` R1. Cross-link to the per-format wire-format documents under `specs/134-divergent-purl-detection/contracts/`.
- [X] T027 Update `CHANGELOG.md` with a milestone-134 entry. Cite #125 as the closing issue.
- [X] T028 Run the mandatory pre-PR gate per `CLAUDE.md`: `./scripts/pre-pr.sh` (which runs `cargo +stable clippy --workspace --all-targets -- -D warnings` followed by `cargo +stable test --workspace`). Both MUST report zero errors / `0 failed`. If clippy flags any async / iterator-style lints, fix them locally before pushing — `feedback-clippy-before-async-patterns` memory note applies.
- [ ] T029 Open the PRs in order: one for US1 (Phase 3 alone is the MVP slice), one for US2 (Phase 4), one for US3 (Phase 5 + Phase 6). Each PR closes #125 partially; the third one (US3 + Polish) closes it.

---

## Dependencies

```text
Phase 1 (Setup)                  → Phase 2
Phase 2 (Foundational)           → Phase 3 (US1)  [REQUIRED — types must exist before reader/emitter wiring]
Phase 3 (US1, P1)                → Phase 4 (US2, P2)   [US2 extends US1's reason path]
Phase 3 (US1, P1)                → Phase 5 (US3, P3)   [US3 builds on per-record machinery]
Phase 4 (US2)                    ⫪ Phase 5 (US3)        [US2 and US3 are independent — either order]
Phase 5 (US3)                    → Phase 6 (Polish)
```

## Parallel-execution opportunities

Within Phase 3 (US1), once T007 (reader-side compare-and-emit) is done:

```text
T008 (CDX wiring)   |
T009 (SPDX 2.3 wiring)  | — all three can run concurrently; different files
T010 (SPDX 3 wiring)    |
```

Within Phase 5 (US3), once T018 (aggregation) is done:

```text
T019 (CDX document-scope)   |
T020 (SPDX 2.3 doc-scope)   | — all three can run concurrently
T021 (SPDX 3 doc-scope)     |
```

Within Phase 6, after types and emitters exist:

```text
T023 (per-component extractor)   |
T024 (document-scope extractor)  | — two extractors are independent files
```

## MVP scope

**Phase 3 (US1, P1) alone is the shipping MVP slice.** It delivers the headline detection use case (accidental shadow copy via declared-dep divergence) with the per-component property surface. Operators get the signal even without `--deep-hash`; consumers get a structured property on every divergent root component.

US2 and US3 are strictly additive — neither is required for US1 to ship a useful capability, and either can land in a follow-up PR.

## Format validation

All 29 tasks follow the strict checklist format: `- [ ] T<NNN> [P?] [Story?] Description with file path`. Setup (T001), Foundational (T002–T005), and Polish (T023–T029) tasks omit the story label per spec. User-story phases (T006–T022) carry the [US1] / [US2] / [US3] labels. Parallelizable tasks across independent files are marked [P].
