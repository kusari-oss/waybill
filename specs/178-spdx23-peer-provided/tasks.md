# Tasks: SPDX 2.3 PROVIDED_DEPENDENCY_OF for npm peer deps (m178)

**Input**: Design documents from `/specs/178-spdx23-peer-provided/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/, quickstart.md

**Tests**: Integration + unit tests included per spec SC-001…SC-008. The classifier is small (one match arm + one lookup-set pre-compute) so unit tests are minimal; integration tests carry most of the acceptance coverage per US.

**Organization**: Tasks are grouped by user story. This milestone has a substantial Foundational phase because the classifier extension is the shared prerequisite every US downstream tests — one enum variant + one pre-compute + one match arm serves all three USs uniformly; US1/US2/US3 differ only in test scenarios.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3
- Include exact file paths in descriptions

## Path Conventions

Code lives in `mikebom-cli/src/generate/spdx/relationships.rs` (single file for all Rust changes); tests under `mikebom-cli/tests/`; docs under `docs/reference/`. All paths absolute from repo root.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm branch state and zero-dep posture. m178 branch was created after m177 landed on main, so no rebase should be needed.

- [X] T001 Verify branch `178-spdx23-peer-provided` is checked out; `git log --oneline main..HEAD` returns empty (branch at main HEAD). Working tree state: modified `CLAUDE.md` (from `/speckit-plan` update-agent-context) + untracked `specs/178-spdx23-peer-provided/` + m176-era `image-baz.cdx.actual.json` scratch (from every prior PR's leftover — kept out of commit).
- [X] T002 Verify zero-dep claim via `cargo tree -p mikebom --depth 1` — existing `serde_json`, `std::collections::HashSet`, `mikebom_common::resolution::{RelationshipType, ResolvedComponent}` all in the tree. No new Cargo deps needed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend the SPDX 2.3 emitter with the new `ProvidedDependencyOf` enum variant + peer-edge lookup set + match arm. **BLOCKS all user stories** — every US integration test depends on the classifier firing correctly.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 In `mikebom-cli/src/generate/spdx/relationships.rs`, add the new `ProvidedDependencyOf` enum variant to `SpdxRelationshipType` per data-model.md §Entity 1. Insert adjacent to the existing typed dep-scope variants (`DevDependencyOf`/`BuildDependencyOf`/`TestDependencyOf`). Include the milestone-178 doc-comment explaining the reversed-direction convention (matches m228) and the "provided" semantic. The existing `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` attribute automatically serializes it as `PROVIDED_DEPENDENCY_OF`.
- [X] T004 In the same file's `build_relationships` function, insert the peer-edge lookup set pre-compute per data-model.md §Entity 2. Place it immediately AFTER the `purl_to_id` map construction (~line 152) and BEFORE the relationships-loop that begins around line 156. Iterate `artifacts.components`; for each component with `extra_annotations["mikebom:peer-edge-targets"]` populated as a JSON-array-in-string, parse and insert `(c.purl.as_str().to_string(), target_purl)` tuples into a `HashSet<(String, String)>`. Fail-open on missing annotation, non-string value, or malformed JSON per Constitution Principle III.
- [X] T005 In the same `build_relationships` function, extend the existing `match (compat, kind)` block (line ~186) with the new peer-edge arm per data-model.md §Entity 3. Insert BEFORE the generic `(_, RelationshipType::DependsOn)` arm at line 190. Guard clause: `(crate::generate::Spdx2RelationshipCompat::Full, RelationshipType::DependsOn) if peer_edges.contains(&(rel.from.clone(), rel.to.clone()))`. Body: `(to_id, from_id, SpdxRelationshipType::ProvidedDependencyOf)` — reversed direction per m228 convention. Basic mode requires NO changes (existing catch-all Basic arm collapses peer edges to `DependsOn` naturally per research §R3).
- [X] T006 Verify Phase 2 code compiles: `cargo +stable check -p mikebom 2>&1`. Also run `cargo +stable clippy -p mikebom --all-targets -- -D warnings 2>&1` to catch lint issues early (matches CI 1.97 gates).
- [X] T007 Verify existing SPDX 2.3 relationship-emission unit tests still pass: `cargo +stable test -p mikebom --bin mikebom generate::spdx::relationships`. The m228 tests at line 480+ exercise the full match block — they MUST continue to pass unchanged. If they fail, the m178 arm's guard clause is over-firing.

**Checkpoint**: `ProvidedDependencyOf` variant + peer-edge lookup set + match arm all landed and unit-tested. Every US phase can now begin. The classifier fires uniformly across US1/US2/US3 — the only difference between US phases is test scenarios.

---

## Phase 3: User Story 1 — Full-mode SPDX 2.3 emits `PROVIDED_DEPENDENCY_OF` (Priority: P1) 🎯 MVP-part-1

**Goal**: An SPDX 2.3 consumer sees peer edges as `PROVIDED_DEPENDENCY_OF` (reversed direction) under the default `--spdx2-relationship-compat=full`. Consumers distinguish install-driven peer edges from functional-dep edges via the native SPDX 2.3 relationship type — no need to inspect the `mikebom:peer-edge-targets` annotation.

**Independent Test**: emit an SPDX 2.3 SBOM from a synthesized npm-with-peer-deps fixture under default (full) mode. Assert the peer-driven relationship carries `relationshipType: "PROVIDED_DEPENDENCY_OF"`. Verified by T008.

### Implementation for User Story 1

- [X] T008 [US1] Create `mikebom-cli/tests/spdx23_peer_provided.rs` with shared `scan_spdx` helper (release-independent `Command::new(bin())` invocation with `apply_fake_home_env` — same shape as m175's `design_tier_advisory.rs` and m177's `reachability_signal.rs`). Include a fixture-writer helper `write_npm_peer_fixture` that synthesizes a minimal `package.json` + `package-lock.json` with one peer edge (consumer-pkg → provided-pkg). Add 2 US1 tests:
  - `t001_us1_full_mode_emits_provided_dependency_of` (SC-001): scan the fixture under `--offline` with the default compat mode (flag omitted); assert the emitted SPDX 2.3 SBOM's `relationships[]` contains ≥1 entry with `relationshipType == "PROVIDED_DEPENDENCY_OF"`.
  - `t002_us1_reversed_direction` (SC-001 detail): assert the `PROVIDED_DEPENDENCY_OF` edge's `spdxElementId` corresponds to the peer TARGET's Package (the provided one) and `relatedSpdxElement` corresponds to the SOURCE (the consumer) — verifies the m228 reversed-direction convention.
- [X] T009 [US1] Run `cargo +stable test -p mikebom --test spdx23_peer_provided` and confirm t001+t002 pass.

**Checkpoint**: US1 verified. Default-mode SPDX 2.3 emission upgrades peer edges to the native `PROVIDED_DEPENDENCY_OF` type with the correct m228-convention direction reversal.

---

## Phase 4: User Story 2 — Basic-compat mode preserves `DEPENDS_ON` (Priority: P1) 🎯 MVP-part-2

**Goal**: Under `--spdx2-relationship-compat=basic`, peer edges collapse to `DEPENDS_ON` (natural direction) — byte-identical to pre-178 behavior. Preserves the m228 escape hatch for downstream consumers with basic-vocabulary tooling.

**Independent Test**: emit the same fixture from Phase 3 under `--spdx2-relationship-compat=basic`. Assert ZERO `PROVIDED_DEPENDENCY_OF` entries; peer edge appears as natural-direction `DEPENDS_ON`. Verified by T010.

### Implementation for User Story 2

- [X] T010 [US2] Extend `mikebom-cli/tests/spdx23_peer_provided.rs` with 2 US2 tests:
  - `t003_us2_basic_mode_collapses_to_depends_on` (SC-002): scan the same npm-peer-fixture from Phase 3 with `--spdx2-relationship-compat basic`; assert the emitted SPDX 2.3 SBOM has ZERO `relationships[]` entries with `relationshipType == "PROVIDED_DEPENDENCY_OF"`; assert peer edges appear as `relationshipType == "DEPENDS_ON"` natural-direction (source→target order matches the internal `DependsOn` direction).
  - `t004_us2_basic_mode_direction_natural` (SC-003 partial): assert the `DEPENDS_ON` edge under basic mode has `spdxElementId` corresponding to the SOURCE (consumer) and `relatedSpdxElement` corresponding to the TARGET (provided) — verifies natural direction unchanged.
- [X] T011 [US2] Run the extended test file; confirm t003+t004 pass alongside t001+t002.

**Checkpoint**: US2 verified. Basic-mode fallback preserves pre-178 behavior byte-identically. m228 escape hatch respected.

---

## Phase 5: User Story 3 — Annotation retained in both modes (Priority: P2)

**Goal**: `mikebom:peer-edge-targets` annotation remains present with byte-identical value in both compat modes. Consumers walking BOTH the native relationship type AND the annotation cross-check successfully.

**Independent Test**: emit the fixture under both compat modes; assert the annotation exists on the source Package in both SBOMs with byte-identical `comment` value. Verified by T012.

### Implementation for User Story 3

- [X] T012 [US3] Extend `mikebom-cli/tests/spdx23_peer_provided.rs` with 2 US3 tests + 1 SC-005 invariant test:
  - `t005_us3_annotation_present_both_modes` (SC-004 core): scan the fixture twice (full + basic); assert the source Package's `annotations[]` contains a `MikebomAnnotationCommentV1` envelope with `field == "mikebom:peer-edge-targets"` in BOTH SBOMs.
  - `t006_us3_annotation_value_byte_identical_across_modes` (SC-004 detail): extract the `comment` field verbatim from BOTH SBOMs; assert byte-equal.
  - `t007_us3_fr007_bidirectional_invariant` (SC-005): under full mode, for the fixture, extract every `(source_purl, target_purl)` tuple from `mikebom:peer-edge-targets` annotations AND every `(source_purl, target_purl)` tuple derived from `PROVIDED_DEPENDENCY_OF` edges (accounting for direction reversal — SPDX `relatedSpdxElement` is the annotation-source; SPDX `spdxElementId` is the annotation-target). Assert the two sets are equal. Verifies FR-007 bidirectional invariant.
- [X] T013 [US3] Run the full test file; confirm all 7 tests pass.

**Checkpoint**: US3 verified. Annotation carries the fine-grained target list intact across compat modes; FR-007 bidirectional invariant holds.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Docs + golden regeneration + SC-006/SC-007/SC-008 gate verification + pre-PR + quickstart walk.

- [X] T014 [P] Extend `docs/reference/reading-a-mikebom-sbom.md`'s existing `mikebom:peer-edge-targets` subsection per data-model.md §Entity 4 shape: append a new sub-paragraph explaining the m178 SPDX 2.3 primary-signal behavior + compat-mode interaction + jq recipe. Cross-reference: m228's `--spdx2-relationship-compat` flag + m147's annotation origin + Principle V native-first as the direct motivation. ~40 lines.
- [X] T015 [P] Extend `docs/reference/sbom-format-mapping.md`'s C-row for `mikebom:peer-edge-targets` per data-model.md §Entity 5: update the SPDX 2.3 column to cite `PROVIDED_DEPENDENCY_OF` (full) / `DEPENDS_ON` (basic) as the primary native signal with the annotation as finer-grained supplement. First find the exact C-row via `grep -n peer-edge-targets docs/reference/sbom-format-mapping.md`. ~5 lines.
- [X] T016 Regenerate SPDX 2.3 goldens: `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test -p mikebom --test spdx_regression`. Then `git diff --stat mikebom-cli/tests/fixtures/golden/spdx-2.3/` and enumerate which fixtures flipped. **Expected per research R4**: `npm.spdx.json` flips; other SPDX 2.3 goldens byte-identical. Verify SC-006 gate — non-npm goldens show zero drift. Verify SC-007 gate — npm SPDX 2.3 golden shows ONLY peer-edge `relationshipType` flips (`DEPENDS_ON` → `PROVIDED_DEPENDENCY_OF`) + direction reversal on those edges; zero other bytes change. Non-peer edges in the same golden MUST stay `DEPENDS_ON` natural-direction.
- [X] T017 Regenerate CDX + SPDX 3 goldens IF touched (they shouldn't be per FR-004/FR-005): `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression && MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test spdx3_regression`. Then `git diff --stat mikebom-cli/tests/fixtures/golden/cyclonedx/ mikebom-cli/tests/fixtures/golden/spdx-3/`. **Expected**: zero diff (SC-008 gate). If any drift appears, m178 accidentally rippled to CDX or SPDX 3 — investigate before proceeding.
- [X] T018 Regenerate `pkg_alias_binding` golden IF touched: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test pkg_alias_binding_us1`. Then `git diff --stat mikebom-cli/tests/fixtures/pkg_alias_binding/`. Expected: zero diff (this fixture uses `pkg:generic/...` PURLs, no npm peer edges).
- [X] T019 Run `./scripts/pre-pr.sh` — verify both `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero errors/warnings — matches CI 1.97 lint gates) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) pass. Preserve the full per-target output when confirming green per feedback_prepr_gate_full_output.md.
- [X] T020 Walk quickstart.md Path A (US1 full-mode `PROVIDED_DEPENDENCY_OF`) + Path B (US2 basic-mode `DEPENDS_ON`) + Path C (US3 annotation retention across modes) + Path D (SC-006/SC-008 byte-identity gates) + Bonus Path (FR-007 bidirectional invariant) against the release build. Confirm each expected result per quickstart.md §Full success criteria table. Document any deviation in the PR body.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no blockers.
- **Foundational (Phase 2)**: depends on Setup. **BLOCKS ALL user stories** — the classifier is the substrate every US downstream tests.
- **User Story 1 (Phase 3)**: depends on Phase 2. Independent of US2/US3.
- **User Story 2 (Phase 4)**: depends on Phase 2. Extends US1's test file with basic-mode scenarios.
- **User Story 3 (Phase 5)**: depends on Phase 2. Extends US1+US2's test file with annotation-retention + FR-007 invariant.
- **Polish (Phase 6)**: T014 + T015 (docs) can run in parallel and don't block on user-story code. T016 + T017 + T018 (golden regen) depend on Phase 2 landing. T019 + T020 depend on ALL prior phases.

### User Story Dependencies

- **US1 (P1) MVP-part-1** — independent given Phase 2.
- **US2 (P1) MVP-part-2** — extends US1's test file with basic-mode scenarios; landing US1 first avoids merge conflicts.
- **US3 (P2)** — extends US1+US2's test file with annotation-retention and FR-007 invariant.

### Within Each User Story

- Tests are additive to `mikebom-cli/tests/spdx23_peer_provided.rs`; land them in test-number order (t001…t007) to avoid file conflicts within the same PR.

### Parallel Opportunities

- **Phase 6 docs (T014 + T015)** [P] — two different files.
- Everything else is sequential or intra-Phase-2 lockstep (T003 → T004 → T005 → T006 → T007 must land in order — all touch the same file).

---

## Parallel Example: Phase 6 docs

```bash
# Two independent doc edits can run in parallel:
Task: "Extend mikebom:peer-edge-targets subsection in docs/reference/reading-a-mikebom-sbom.md per data-model §Entity 4"
Task: "Extend C-row in docs/reference/sbom-format-mapping.md per data-model §Entity 5"
```

---

## Implementation Strategy

### MVP First (US1 + US2 landed together — P1 milestone shape)

1. Complete Phase 1: Setup (T001–T002).
2. Complete Phase 2: Foundational (T003–T007) — enum variant + lookup set + match arm + compile-check + existing-tests-pass check.
3. Complete Phase 3: US1 tests (T008–T009).
4. Complete Phase 4: US2 tests (T010–T011).
5. **STOP and VALIDATE**: quickstart.md Path A + Path B walks. Full-mode SPDX 2.3 upgrades peer edges to `PROVIDED_DEPENDENCY_OF`; basic-mode preserves pre-178 behavior. **Shippable as MVP.**
6. Optional: continue to US3 in the same PR (annotation-retention + FR-007 invariant).

### Incremental Delivery

1. Setup + Foundational → substrate ready.
2. Add US1 → default-mode Principle V native-first migration verified → shippable.
3. Add US2 in same PR → basic-mode escape hatch preserved → coordinated pair shippable.
4. Add US3 → annotation invariant + FR-007 verified → shippable.
5. Polish → docs + goldens + pre-PR.

### Solo Strategy (recommended for m178 given ~15 lines of Rust)

1. Sequential through Phase 1 → 2 → 3 → 4 → 5 → 6.
2. Bundle into a single PR.
3. Estimated wall-clock: ~1-2 hours for a solo dev — implementation is trivial; test scaffolding + golden regen review is the bulk.

---

## Notes

- [P] tasks = different files, no dependencies.
- Every FR from spec.md maps to at least one task; every SC has a verifying task in Phase 5 or 6.
- **Golden regeneration is expected and bounded** per plan.md §Complexity Tracking — the SC-007 gate at T016 codifies the "peer-edge relationshipType flip + direction reversal only" invariant.
- **First canonical Principle V migration**: m178 IS the reference implementation of Principle V for a scoped native construct that exists in ONE format but not others. Future contributors doing Principle V audits can cite m178's pattern (elevate native-first for the format that has the construct; keep annotation as parity-bridge for the others).
- Pre-PR gate (T019) is MANDATORY per project CLAUDE.md. Do not open PR without both clippy + tests clean.
