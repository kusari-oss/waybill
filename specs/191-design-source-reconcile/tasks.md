---
description: "Task list for m191 — Design-Tier / Source-Tier Reconciliation"
---

# Tasks: Design-Tier / Source-Tier Reconciliation

**Input**: Design documents from `/specs/191-design-source-reconcile/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/emission-shape.md, quickstart.md

**Tests**: Included — mikebom's standard TDD-flavored integration-test-plus-unit-test pattern (matches m190 shipped by the previous milestone).

**Organization**: Tasks grouped by user story. US2 (versionless PURL) is Phase 3 despite spec-labeling as "US2" — it's the smaller, more localized fix and unblocks US1's "standalone design-tier" branch. Both stories are P1 in the spec; the swap is implementation-order only.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1 = reconciliation, US2 = versionless PURL)
- File paths absolute where non-obvious; workspace root is `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

- **mikebom-cli crate**: `mikebom-cli/src/…`, `mikebom-cli/tests/…`
- **Feature spec dir**: `specs/191-design-source-reconcile/…`

---

## Phase 1: Setup

**Purpose**: Verify baseline is clean so any regression signal in later phases is unambiguous.

- [X] T001 Confirm `191-design-source-reconcile` branch is checked out and clean (`git status` shows only the specs/ directory as untracked).
- [X] T002 [P] Run baseline `./scripts/pre-pr.sh` to capture the pre-m191 test count. Deferred to T046 — no Rust changes yet so pre-m191 baseline equals m190's post-merge state (241 test suites passing).

**Checkpoint**: Baseline recorded; workspace clean.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Research-derived audits + data-model verifications that MUST land before any US work so drift-set + emitter-shape assumptions are locked in.

⚠️ **CRITICAL**: No US work can begin until this phase is complete.

- [X] T003 Audit existing goldens for m191 drift set per research §R8: `grep -rEn '"pkg:[^"]*@"' mikebom-cli/tests/fixtures/` (trailing-`@` PURLs — US2 signal) AND `grep -rEn '"name":\s*"([^"]+)"[^{]*"version":\s*""' mikebom-cli/tests/fixtures/` (empty-version fields). Record identified filenames in `specs/191-design-source-reconcile/scratch/drift-set.txt`.
- [X] T004 [P] Audit existing goldens for design/source PAIRS (US1 signal): for each ecosystem in the golden set, grep `"name":\s*"<name>"` and identify names appearing in 2+ components with distinct versions where one has `mikebom:sbom-tier: design` and the other has `source`. Record findings in same scratch file.
- [X] T005 [P] Verify current `ResolvedComponent.extra_annotations` handles `serde_json::Value::Array` at the emission layer: grep the CDX / SPDX 2.3 / SPDX 3 annotation emitters for how they iterate `extra_annotations`. If any emitter type-narrows the value with `.as_str()` and drops arrays, add a Phase 6 fixup task for the affected emitter.
- [X] T006 [P] Verify current parity extractor C20 (`mikebom:requirement-range`) shape: read `mikebom-cli/src/parity/extractors/{cdx.rs,spdx2.rs,spdx3.rs}` C20 macro / helper. Confirm it returns ALL matching properties (as a `Vec<Value>`), not just the first. Record findings in scratch; if it returns only the first, add Phase 5 T033-companion task.
- [X] T007 [P] Verify how the pre-emission model carries dependency edges (per research §R7): check if `Vec<Relationship>` sibling exists on the pipeline output, OR whether relationships are constructed inside each format emitter. Findings determine WHERE the FR-005 rewrite lands (single central rewrite vs 3× per-format rewrites). **MUST record in `specs/191-design-source-reconcile/scratch/edge-carrier.txt` two facts explicitly**: (a) the canonical field name + type carrying dep edges (e.g., `ResolvedComponent.dependencies: Vec<Purl>` OR "no central carrier — per-format"), (b) the resulting decision: `central-rewrite` (T033 walks the pipeline model once) OR `per-format-rewrite` (T033 walks each emitter's relationship-build step). T033 consults this file before implementation — no branching-at-code-time.

**Checkpoint**: Drift set captured; parity + annotation + edge plumbing verified.

---

## Phase 3: User Story 2 — Versionless PURL (Priority: P1)

**Goal**: Every per-ecosystem `build_*_purl` helper emits a purl-spec-canonical `pkg:<type>/<name>` string (no trailing `@`) when `version` is empty. Every format emitter omits the version field format-idiomatically (CDX omit / SPDX 2.3 `NOASSERTION` / SPDX 3 omit). Closes issue #558.

**Independent Test**: Scan a synthetic npm project declaring `optional-dep: "^1.0.0"` in `package.json` but with NO corresponding entry in `package-lock.json`. Assert `.components[?(@.name=='optional-dep')].purl == "pkg:npm/optional-dep"` (no `@`), `.version` field absent from JSON, `.bom-ref == "pkg:npm/optional-dep"`.

### Tests for User Story 2

> **Write tests FIRST; ensure they FAIL against the pre-m191 tree before implementation.**

- [X] T008 [P] [US2] Add unit tests for `build_npm_purl` empty-version branch in `mikebom-cli/src/scan_fs/package_db/npm/mod.rs::tests`. Cover: unscoped versionless (`build_npm_purl("lodash", "")` → `"pkg:npm/lodash"`), scoped versionless (`build_npm_purl("@angular/core", "")` → `"pkg:npm/%40angular/core"`), non-empty version unchanged (byte-identity for `build_npm_purl("lodash", "4.17.21")` → `"pkg:npm/lodash@4.17.21"`).
- [X] T009 [P] [US2] Add unit tests for `build_cargo_purl` empty-version branch in `mikebom-cli/src/scan_fs/package_db/cargo.rs::tests`. Same shape as T008.
- [X] T010 [P] [US2] Add unit tests for `build_pypi_purl` empty-version branch in `mikebom-cli/src/scan_fs/package_db/pip/mod.rs::tests` (or the module where the PURL builder lives — grep `mikebom-cli/src/scan_fs/package_db/pip/*.rs` for `fn build_pypi_purl` / `format!("pkg:pypi/`).
- [X] T011 [P] [US2] Add unit tests for `build_maven_purl` empty-version branch in `mikebom-cli/src/scan_fs/package_db/maven.rs::tests`.
- [X] T012 [P] [US2] Add unit tests for `build_gem_purl` empty-version branch in `mikebom-cli/src/scan_fs/package_db/gem.rs::tests`.
- [X] T013 [P] [US2] Add unit tests for `build_composer_purl`, `build_dart_purl`, `build_cocoapods_purl`, `build_scala_purl`, `build_haskell_purl`, `build_erlang_purl` empty-version branches — one unit test per ecosystem in the respective reader's `tests` module (7 more, all co-located with their target files).
- [X] T014 [P] [US2] Add integration test file `mikebom-cli/tests/design_tier_versionless_purl.rs` with US2 assertions per quickstart.md Reproducer 2 (Assertions 5-9): scan synthetic npm project with unresolved `optionalDependencies` entry, assert versionless PURL / omitted `.version` / `NOASSERTION` SPDX 2.3 / omitted SPDX 3 property / spdx3-validate conformance.
- [ ] T014a [P] [US2] Add PURL round-trip fuzz test in `mikebom-cli/src/scan_fs/package_db/npm/mod.rs::tests` (or a co-located `purl_roundtrip` unit test module) satisfying SC-004 / FR-014. Generate at least 100 synthetic versionless PURLs across all 11 ecosystems (`pkg:npm/foo`, `pkg:cargo/foo`, `pkg:pypi/foo`, `pkg:maven/foo/bar`, `pkg:gem/foo`, `pkg:composer/foo/bar`, `pkg:pub/foo`, `pkg:cocoapods/foo`, `pkg:generic/foo`, `pkg:hackage/foo`, `pkg:hex/foo` etc.) with names covering: single-token, scoped (`%40scope/name`), namespaced (`ns/name`), URL-encoded segments (`foo%20bar`), max-length (~255 chars). For each: parse via `Purl::new`, serialize back, parse again, assert equality across BOTH parse-then-serialize steps. Fixture generator + assertion in one test.

### Implementation for User Story 2

- [X] T015 [US2] Implement empty-version branch in `build_npm_purl` at `mikebom-cli/src/scan_fs/package_db/npm/mod.rs:676-693`. Follow the pattern in data-model.md — `if let Some(rest) = name.strip_prefix('@')` scoped branch + else branch, EACH with an inner `if version.is_empty()` guard producing the versionless shape. Preserve byte-identity for non-empty inputs.
- [X] T016 [US2] Implement empty-version branch in `build_cargo_purl`.
- [X] T017 [US2] Implement empty-version branch in `build_pypi_purl` (grep for the actual function name + location).
- [X] T018 [US2] Implement empty-version branch in `build_maven_purl`.
- [X] T019 [US2] Implement empty-version branch in `build_gem_purl`.
- [X] T020 [US2] Implement empty-version branch in the remaining 6 ecosystem PURL builders (composer, dart, cocoapods, scala, haskell, erlang). Batch across the 6 files.
- [X] T021 [US2] Update CDX emitter at `mikebom-cli/src/generate/cyclonedx/builder.rs` to omit `.version` field when `component.version.is_empty()`. Wrap the existing `entry["version"] = json!(component.version);` in `if !component.version.is_empty() { … }`. Preserve byte-identity for non-empty version paths.
- [X] T022 [US2] Update SPDX 2.3 emitter at `mikebom-cli/src/generate/spdx/packages.rs` to emit `versionInfo: "NOASSERTION"` when `component.version.is_empty()`. Locate the `version_info` field initializer and add the empty-check.
- [X] T023 [US2] Update SPDX 3 emitter at `mikebom-cli/src/generate/spdx/v3_document.rs` (or wherever `software_Package` graph elements are built) to omit `software_packageVersion` when `component.version.is_empty()`.
- [X] T024 [US2] Update bom-ref generation for versionless design-tier components: verify (`mikebom-cli/src/generate/cyclonedx/builder.rs` bom-ref assignment) that when the PURL is versionless, the bom-ref equals the versionless PURL (per Q3 answer A / FR-013). Most likely already works if bom-ref is derived from PURL directly; add a unit assertion.
- [X] T025 [US2] Run T008-T013 unit tests + T014 integration test locally: `cargo test -p mikebom --bin mikebom` for units + `cargo test -p mikebom --test design_tier_versionless_purl` for integration. MUST all pass.

**Checkpoint**: Standalone design-tier components emit spec-clean versionless PURLs. All US2 acceptance scenarios (5) pass. Byte-identity preserved for non-empty-version paths.

---

## Phase 4: User Story 1 — Design-Tier / Source-Tier Reconciliation (Priority: P1)

**Goal**: When a design-tier component has a matching source-tier resolution in the same workspace scope, collapse them into ONE source-tier component with design-tier metadata attached as multiple property entries. Rewrite incoming dep-graph edges. INFO-level summary + DEBUG-level per-component logging. Closes issue #560.

**Independent Test**: Scan a synthetic npm workspace where the root manifest declares `commander: "^11.1.0"` + root lockfile resolves to `11.1.0`. Assert exactly ONE `commander` component in CDX output with `.version == "11.1.0"`, `.purl == "pkg:npm/commander@11.1.0"`, and `.properties` includes both `mikebom:requirement-range` + `mikebom:source-manifest` transferred from the removed design-tier component.

### Tests for User Story 1

> **Write tests FIRST; ensure they FAIL against the pre-m191 tree before implementation.**

- [X] T026 [P] [US1] Add unit tests for `reconcile_design_source_tiers` in `mikebom-cli/src/resolve/reconciler.rs::tests` (module to be created in T031). Cover:
  - No design-tier → identity (empty design list → input unchanged).
  - Design-tier with matching source-tier → merged; design removed; source annotations updated.
  - Design-tier with NO source-tier match → design preserved verbatim (standalone case).
  - Multiple design-tier entries reconcile to same source-tier → all ranges preserved as `Value::Array` on `extra_annotations` per R4.
  - Design-tier matched to multiple source-tier entries (workspace peer-dep hoisting) → annotations attached to every match.
  - Non-npm ecosystems (cargo, pip, maven, gem) — reconciliation match works across ecosystems.
- [X] T027 [P] [US1] Add unit tests for `workspace_scope_for` helper in the same test module. Cover: standalone project (no workspace parent) → returns own manifest-parent; npm workspace member → returns workspace root; Cargo workspace member → returns workspace root; missing `mikebom:source-manifest` annotation → returns scan root.
- [X] T028 [P] [US1] Add integration test file `mikebom-cli/tests/design_source_reconcile.rs` with US1 assertions per quickstart.md Reproducers 1 + 3 (Assertions 1-4 + 10). Cases:
  - Fixture A: simple npm project → 1 `commander` component with transferred annotations.
  - Fixture B: workspace with two child manifests → 1 `commander` component with 2 `mikebom:requirement-range` entries + 2 paired `mikebom:source-manifest` entries.
  - **Fixture D (1:many per FR-003 / C2 remediation)**: workspace with `packages/root/package.json` declaring `foo: "^1.0"` (single design-tier declaration) but the root lockfile resolving `foo` to BOTH `foo@1.0.5` (top-level `node_modules/foo`) AND `foo@1.2.0` (nested `node_modules/bar/node_modules/foo` — peer-dep hoisting shape). Assert BOTH source-tier `foo` components carry the same `mikebom:requirement-range: ^1.0` annotation transferred from the single design-tier component. Zero design-tier `foo` remaining after reconciliation.
  - Cross-format PURL parity assertion (CDX == SPDX 2.3 == SPDX 3).
- [X] T029 [P] [US1] Add integration test for FR-005 graph-edge rewriting in `mikebom-cli/tests/design_source_reconcile.rs::edges_rewritten_after_reconciliation`. Fixture where a parent component's `dependencies[].dependsOn` list references BOTH the pre-m191 design-tier bom-ref AND the source-tier bom-ref; assert post-m191 the list contains ONLY the source-tier bom-ref (design entry rewritten OR deduped).
- [X] T030 [P] [US1] Add spdx3-validate conformance assertion in `mikebom-cli/tests/design_source_reconcile.rs::spdx3_validate_accepts_reconciled_output`. Same pattern as m190's `us2_spdx3_validate_accepts_compound_license_ipk` — scan Fixture A + validate the SPDX 3 output.

### Implementation for User Story 1

- [X] T031 [US1] Create new file `mikebom-cli/src/resolve/reconciler.rs` with `pub fn reconcile_design_source_tiers` signature per data-model.md. Implement Pass A (source-tier index) + Pass B (design-tier reconciliation walk) + Pass D (removal). Defer Pass C (graph-edge rewriting) to T033. Add the standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard on the co-located `mod tests` block per Constitution Principle IV / codebase convention.
- [ ] T031a [US1] Alias-handling per spec Edge Cases (C3 remediation): implement alias detection in `reconcile_design_source_tiers`. When a design-tier component's name differs from its matching source-tier component's name (npm alias pattern: `"my-alias": "npm:actual-pkg@1.0.0"` where the design-tier component was keyed on `my-alias` but the source-tier is `actual-pkg`), reconciliation MUST still match on the resolved PURL name (source-tier side) AND transfer the alias name onto the survivor as a `mikebom:declared-as` annotation (reusing the m111 pkg-alias-binding annotation channel — verify m111 uses that exact key; if a different key, follow m111's convention). Add integration test `mikebom-cli/tests/design_source_reconcile.rs::alias_declaration_carries_declared_as_annotation` — Fixture E: npm project with `"my-alias": "npm:commander@11.1.0"` in `package.json` + resolved `commander@11.1.0` in lockfile. Assert exactly ONE `commander` component + `.properties` includes `mikebom:declared-as: my-alias`.
- [X] T032 [US1] Add `pub mod reconciler;` to `mikebom-cli/src/resolve/mod.rs`.
- [X] T033 [US1] Implement Pass C (graph-edge rewriting) in `reconcile_design_source_tiers`. Consult research §R7 findings from T007: if edges live on `ResolvedComponent.dependencies` field, walk components' dep lists and rewrite matching bom-refs. If edges live in per-format construction, add a rewrite pass at each emitter's relationship-build step (fallback strategy).
- [X] T034 [US1] Implement `workspace_scope_for` helper + `WorkspaceIndex` cache in `reconciler.rs` per data-model.md. Handle npm / pnpm / yarn / Cargo / pyproject / composer workspace-parent claim checks. Cache misses on first lookup per workspace root; hits for peer members.
- [X] T035 [US1] Implement FR-020 observability: INFO log summary at pass exit; DEBUG log per reconciled pair. Use `tracing::info!` / `tracing::debug!` macros consistent with m173 / m158 patterns.
- [ ] T036 [US1] Handle multi-declaration case (FR-004 / Q1 answer B): when transferring annotations, if the source-tier already has a `mikebom:requirement-range` — whether from a **prior reconciliation** OR from a **reader-populated value** (e.g., dart/cocoapods/composer/scala/haskell/pip/npm readers already set `requirement_range: Some(...)` on some source-tier entries per grep) — promote the value to `serde_json::Value::Array` and append the new range. Same for `mikebom:source-manifest`. Dedup rule: if the incoming range/manifest tuple is byte-identical to an existing entry on the source-tier (reader-populated or previously reconciled), SKIP the append (no double-entry). Preserve insertion order for pairing. Add unit test cases in T026's suite covering: (a) reader-populated scalar + one reconciled → array of 2, (b) reader-populated scalar + reconciled duplicate → scalar unchanged (dedup fired), (c) empty source-tier + two reconciled → array of 2.
- [X] T037 [US1] Wire `reconcile_design_source_tiers` call into `mikebom-cli/src/scan_fs/mod.rs:807` (immediately after the first `deduplicate`) AND `mikebom-cli/src/cli/scan_cmd.rs:2742` (immediately after the second `deduplicate`). Use `let components = crate::resolve::reconciler::reconcile_design_source_tiers(components);` at both sites.
- [ ] T038 [US1] Update format emitters to handle `serde_json::Value::Array` in `extra_annotations` values (per R4 emitter translation). Verify T005's audit findings; if any emitter type-narrows with `.as_str()` and drops arrays, refactor to iterate `Value::Array` → emit one property/annotation/graph element per array entry. Files potentially affected: `mikebom-cli/src/generate/cyclonedx/builder.rs`, `mikebom-cli/src/generate/spdx/annotations.rs`, `mikebom-cli/src/generate/spdx/v3_annotations.rs`.
- [X] T039 [US1] Run T026-T030 tests locally. MUST all pass. If graph-edge rewriting test fails, drill into T007/T033's findings to locate the actual edge-carrier structure.

**Checkpoint**: Reconciliation collapses design/source pairs into one source-tier component per pair. Multi-declaration ranges preserved as multiple property entries. Dep-graph edges rewritten. INFO summary logs count. All US1 acceptance scenarios (5) pass.

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Cross-milestone verification, parity extractor update, golden regen, real-world validation, docs.

- [ ] T040 [P] Update parity extractor C20 (`mikebom:requirement-range`) at `mikebom-cli/src/parity/extractors/{cdx.rs,spdx2.rs,spdx3.rs}` if T006's audit found single-value extraction. Change to return `Vec<Value>` of all matching properties. Extend row registration at `mikebom-cli/src/parity/extractors/mod.rs:164` if the ParityExtractor signature requires a shape change. Verify parity tests still pass.
- [ ] T041 [P] Same for parity row covering `mikebom:source-manifest` (C-row grep for existing row ID). If not currently tracked, add a new row with `Directionality::SymmetricEqual`.
- [X] T042 Regenerate drift-set goldens per T003/T004. Use the "targeted regen" approach per memory `feedback_release_bump_regen_all_golden_tests`:
  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 \
    cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test pkg_alias_binding_us1 --test oci_pull_backward_compat --test optional_dep_classification
  ```
  Diff-review the resulting changes: every diff MUST be either (a) a design/source pair collapsing to one component, (b) a `pkg:*/*@` becoming `pkg:*/*`, or (c) a `"version": ""` becoming an omitted field / `NOASSERTION`. Reject any other class of diff.
- [X] T043 Non-drift byte-identity gate — run the full test suite; every golden test NOT in the drift set MUST pass byte-identically. This is SC-006's enforcement. If any non-drift golden fails, investigate before merge.
- [X] T044 [P] Real-world validation per quickstart.md real-world section — if a large monorepo scan target is available locally (React Native, big Rust workspace, etc.), diff component counts pre/post m191. Expected: ≥5% reduction per SC-005. Record findings in `specs/191-design-source-reconcile/scratch/real-world-validation.txt`.
- [X] T045 [P] Update CLAUDE.md agent-context "Active Technologies" and "Recent Changes" if the auto-updater at `.specify/scripts/bash/update-agent-context.sh` wasn't already invoked during `/speckit-plan`. Verify current CLAUDE.md lists 191-design-source-reconcile.
- [X] T046 Pre-PR gate — run `./scripts/pre-pr.sh` and confirm BOTH commands pass clean per memory `feedback_prepr_gate_full_output`. Every test suite MUST report `ok. N passed; 0 failed`; clippy MUST report zero errors AND zero warnings.

**Checkpoint**: Full workspace clippy clean, full workspace test suite green, drift-set goldens regenerated with explainable diffs, non-drift goldens byte-identical, parity extractors handle multi-property case.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001, T002. No dependencies.
- **Foundational (Phase 2)**: T003-T007. Depends on Setup. BLOCKS all US phases.
- **US2 (Phase 3)**: Depends on Foundational. Independent of US1. Ships first for atomic-commit granularity.
- **US1 (Phase 4)**: Depends on Foundational. Independent of US2 EXCEPT that US1's "standalone design-tier" branch benefits from US2's versionless PURL shape being in place. Practical ordering: US2 → US1.
- **Polish (Phase 5)**: Depends on both US phases complete.

### User Story Dependencies

- **US2 → US1**: US1's "no source-tier match" path emits standalone design-tier components with versionless PURLs — the shape US2 delivers. Land US2 first so US1's standalone branch tests pass immediately.
- **US1 ⊥ US2 otherwise**: Different code paths (new `reconciler.rs` vs edits to per-ecosystem PURL builders). Different fixtures. No hard dependency beyond the shape coupling above.

### Within Each User Story

- Tests BEFORE implementation (matches mikebom's standard TDD approach; matches m190 sequencing).
- Unit tests before integration tests.
- Per-ecosystem fixes (T015-T020) can proceed in parallel — each edits a different file.
- Reconciler + workspace_scope + observability + multi-decl + wiring (T031-T038) mostly sequential — all edit `reconciler.rs` or its call sites.

### Parallel Opportunities

- Phase 2: T003-T007 (T003 must complete first as it feeds T004's fixture identification; T005/T006/T007 can then run in parallel).
- Phase 3: T008-T014 all `[P]` — parallel test authoring across ecosystems + integration.
- Phase 3: T015-T020 all `[P]` — parallel implementation across ecosystem PURL builders.
- Phase 4: T026-T030 all `[P]` — parallel test authoring.
- Phase 4: T031-T038 mostly sequential (same file); T034 (workspace_scope) can parallelize with T033 (graph-edge).
- Phase 5: T040, T041, T044, T045 all `[P]`.

Different developers CAN split US1 and US2 across two branches after Phase 2 lands. Recommended: single-PR shape (commit granularity per US) matching m190 delivery pattern.

---

## Parallel Example: User Story 2

```bash
# All test-authoring tasks in parallel:
Task: "T008 Add unit tests for build_npm_purl empty-version branch"
Task: "T009 Add unit tests for build_cargo_purl"
Task: "T010 Add unit tests for build_pypi_purl"
Task: "T011 Add unit tests for build_maven_purl"
Task: "T012 Add unit tests for build_gem_purl"
Task: "T013 Add unit tests for 6 remaining ecosystem PURL builders"
Task: "T014 Add integration test design_tier_versionless_purl.rs"

# After Foundational lands, all implementation tasks in parallel:
Task: "T015 Implement empty-version in build_npm_purl"
Task: "T016 Implement empty-version in build_cargo_purl"
Task: "T017 Implement empty-version in build_pypi_purl"
Task: "T018 Implement empty-version in build_maven_purl"
Task: "T019 Implement empty-version in build_gem_purl"
Task: "T020 Implement empty-version in remaining 6 helpers"
```

---

## Implementation Strategy

### MVP (US2 Only)

1. Complete Phase 1 + Phase 2.
2. Complete Phase 3 (US2).
3. **STOP and VALIDATE**: `./scripts/pre-pr.sh` clean; scan Fixture C per quickstart Reproducer 2 → confirm versionless PURL emission.
4. Ship as #558-only if #560 needs more time.

### Incremental Delivery

1. Phase 1 + 2 → foundation ready.
2. Phase 3 (US2) → validate → ship #558 fix (partial milestone) OR continue.
3. Phase 4 (US1) → validate → ship #558 + #560 fixes together as m191 alpha.61.
4. Phase 5 → docs + golden regen + real-world verification.

### Single-PR Delivery (matches m190 pattern)

Land Phases 1–5 in a single PR titled "impl(191): design-tier / source-tier reconciliation — versionless PURL + collapse-when-resolved (#558, #560)". Commit granularity per phase, reviewer digestibility per US.

---

## Notes

- Total tasks: 46 across 5 phases.
- US2: 18 tasks (T008–T025). US1: 14 tasks (T026–T039). Setup/Foundational/Polish: 14 tasks.
- Every `[P]` task edits a distinct file; no file-collision hazards among parallel tasks.
- Zero new Cargo dependencies (spec FR: research §R9 audit satisfied); zero new `mikebom:*` annotations (FR-018).
- Byte-identity gate (SC-006) is enforced at T043 as a HARD blocker for merge.
- Real-world smoke (T044) is `[P]` because it's validation-only; failure does NOT block merge but MUST be surfaced in the PR body.
- Cross-format parity gate (FR-015) enforced via T028's cross-format assertion in `design_source_reconcile.rs`.
