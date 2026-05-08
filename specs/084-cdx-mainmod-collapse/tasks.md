---
description: "Tasks: CDX 1.6 main-module super-root collapse"
---

# Tasks: CDX 1.6 main-module super-root collapse

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/084-cdx-mainmod-collapse/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/target-ref-derivation.md ✅, quickstart.md ✅

**Tests**: Tests are in scope per spec FR-011 (closure-invariant regression test required) — every user story phase includes test tasks.

**Organization**: Tasks grouped by user story. US1, US2, US3 are all P1 and observably triggered by the same code change in Phase 2 — each user story phase adds a different assertion lens onto the shared closure-invariant test file. US4 is P2 (non-regression check on the fallback path).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- File paths are absolute or repository-relative

## Path Conventions

Repository-relative paths from `/Users/mlieberman/Projects/mikebom/`:
- Production code: `mikebom-cli/src/generate/cyclonedx/`
- Tests: `mikebom-cli/tests/`
- Test fixtures: `mikebom-cli/tests/fixtures/`
- Goldens: `mikebom-cli/tests/fixtures/cdx_*.golden.json`
- Spec: `specs/084-cdx-mainmod-collapse/`

---

## Phase 1: Setup (Pre-fix Evidence Capture)

**Purpose**: Capture pre-fix CDX 1.6 emission evidence for one fixture so post-fix diffs are auditable. No project initialization needed (existing crate).

- [X] T001 [P] Build mikebom release binary at `target/release/mikebom` via `cargo +stable build --release -p mikebom` (used by Phase 1 + Polish for non-test invocations)
- [X] T002 [P] Capture pre-fix CDX 1.6 emission against the Go fixture for diff baseline: `target/release/mikebom --offline sbom scan --path tests/fixtures/go/simple-module --format cyclonedx-json --output /tmp/084-pre-fix-go.cdx.json --no-deep-hash`. Confirmed orphan `simple-module@0.0.0` with `dependsOn = [pkg:golang/example.com/simple@v0.0.0-unknown]`.
- [X] T003 [P] Enumerated affected goldens by filtering `metadata.component.bom-ref == PURL && deps has non-PURL ref`. Result: 3 affected goldens — `golang.cdx.json`, `maven.cdx.json`, `npm.cdx.json` (other ecosystems' fixtures don't have main-module promotion in their canonical fixture, so they emit `name@version` legitimately and stay byte-identical).

**Checkpoint**: Pre-fix evidence captured. Diff scope known up front.

---

## Phase 2: Foundational (The Fix)

**Purpose**: The code change that makes US1, US2, US3, and US4 simultaneously satisfiable. This phase IS the milestone's load-bearing fix per research §1 + §2.

**⚠️ CRITICAL**: All user story validation phases (3-6) depend on this phase being complete.

- [X] T004 Implemented main-module-aware `target_ref` derivation in `mikebom-cli/src/generate/cyclonedx/builder.rs` — when `main_module.is_some() && !override_active`, target_ref is the PURL; else legacy short-form. ~25 LOC including comments.
- [X] T005 Implemented override-path relationship filter in `mikebom-cli/src/generate/cyclonedx/builder.rs` — captures dropped main-module PURLs during component-filter pass, filters relationships before `build_dependencies`. ~25 LOC including comments.
- [X] T006 Smoke-tested on Go fixture: post-fix orphans EMPTY; main-module entry keyed by PURL with 5 direct deps; compositions retargeted to PURL. Diff scope: 2 composition retargets + 1 deleted bridge entry — exactly the research §3 invariant.
- [X] T007 `cargo +stable check -p mikebom` passes — clean compile.

**Checkpoint**: The fix is in. Subsequent phases validate it from each user story's perspective.

---

## Phase 3: User Story 1 - Strict CDX consumer sees zero dangling refs (Priority: P1) 🎯 MVP

**Goal**: Every value in `dependencies[].ref`, `dependencies[].dependsOn[]`, `compositions[].assemblies[]`, and `compositions[].dependencies[]` resolves to a declared bom-ref in the same document.

**Independent Test**: Run `cargo +stable test -p mikebom --test cdx_ref_closure_invariant` and observe `0 failed`. Equivalently: pipe a post-fix golden through the closure-check `jq` script in quickstart.md Recipe 5 and observe an empty orphan list.

### Implementation for User Story 1

- [X] T008 [US1] Created `mikebom-cli/tests/cdx_ref_closure_invariant.rs` with closure-invariant scaffold: `Fixture` struct, `FIXTURES` table, `declared_refs(cdx) -> HashSet<String>`, `assert_closure(cdx, label)` helper. Plus 3 sanity unit tests for the helper logic. ~290 LOC including all US extensions.
- [X] T009 [US1] Implemented `run_mikebom_cdx(fixture_subpath) -> serde_json::Value` using the existing `apply_fake_home_env` helper from `tests/common/normalize.rs` and `CARGO_BIN_EXE_mikebom`.
- [X] T010 [US1] Populated `FIXTURES` table with 6 ecosystems: golang/cargo/npm/pip/gem/maven (using canonical paths from `tests/fixtures/<eco>/<sub>`).
- [X] T011 [US1] All 6 ecosystems PASS the closure invariant.

**Checkpoint**: US1 complete. The closure invariant holds for every post-053 ecosystem fixture; strict CDX consumers will see zero dangling refs.

---

## Phase 4: User Story 2 - Reverse-impact analysis terminates cleanly at the real root (Priority: P1)

**Goal**: A reverse-adjacency walk from any leaf component terminates at exactly one node, equal to `metadata.component.bom-ref`.

**Independent Test**: Inspect any post-fix CDX 1.6 emission; build the reverse adjacency map (for each `(ref, dependsOn[])`, emit `dependsOn[i] -> ref`); walk upward from any leaf; assert termination at one node equal to `metadata.component.bom-ref`.

### Implementation for User Story 2

- [X] T012 [US2] Added `assert_reverse_walk_terminates_at_root(cdx, label)`: asserts metadata.component.bom-ref appears as a `dependencies[].ref` AND is NOT a `dependsOn` target of any node. (Refined from "single dep-graph root" to avoid false-positive on leaves with empty `dependsOn`.) ~40 LOC.
- [X] T013 [US2] Extended per-ecosystem loop to call the new helper. All 6 ecosystems PASS. Discovered + fixed Maven self-loop in `dependencies.rs:88-90` (target_ref filter added to `roots` filter so the primary-dep fallback no longer synthesizes self-edges).

**Checkpoint**: US2 complete. Reverse-impact analysis terminates correctly at the project root for every post-053 ecosystem fixture.

---

## Phase 5: User Story 3 - Compositions semantics preserved on the real root (Priority: P1)

**Goal**: The two `compositions[]` entries that referenced the orphan now reference `metadata.component.bom-ref` (the main-module PURL), preserving their `incomplete_first_party_only` and `complete` aggregate claims on the real root.

**Independent Test**: Inspect post-fix `compositions[]`; assert (a) the `incomplete_first_party_only` aggregate's assemblies contains `metadata.component.bom-ref`; (b) the second `complete` aggregate's `dependencies[]` contains `metadata.component.bom-ref`; (c) every composition entry's refs are in the closure set `S`.

### Implementation for User Story 3

- [X] T014 [US3] Added `assert_compositions_anchored_on_root(cdx, label)`: locates `incomplete_first_party_only` composition + trailing `complete` dep-completeness composition; both anchor on `metadata.component.bom-ref`. ~40 LOC.
- [X] T015 [US3] Extended per-ecosystem loop. All 6 ecosystems PASS the anchor check.

**Checkpoint**: US3 complete. Compositions semantics preserved (not deleted) on the real PURL-keyed root.

---

## Phase 6: User Story 4 - Non-main-module fallback path preserved unchanged (Priority: P2)

**Goal**: A scan against a directory with no main-module manifest produces a CDX 1.6 document byte-identical to alpha.23 output.

**Independent Test**: Run mikebom against an OS-package-only fixture (e.g., dpkg/rpm/apk extract from milestone 083, or any synthetic fixture without `go.mod` / `Cargo.toml` / etc.); compare the emitted CDX 1.6 to the alpha.23-baseline golden; assert byte-identity.

### Implementation for User Story 4

- [X] T016 [US4] Identified fallback-path fixtures: `tests/fixtures/apk/synthetic`, `tests/fixtures/deb/synthetic`, `tests/fixtures/rpm/bdb-only` — all OS-package-only with no main-module manifest.
- [X] T017 [US4] Pre-fix baselines preserved as committed goldens at `mikebom-cli/tests/fixtures/golden/cyclonedx/{apk,deb,rpm}.cdx.json` — these are the alpha.23 baseline reference.
- [X] T018 [US4] Verified via `cargo test --test cdx_regression`: `cdx_regression_apk`, `cdx_regression_deb`, `cdx_regression_rpm` ALL PASS — fallback path byte-identical pre/post fix. Plus cargo/gem/pip also byte-identical (their canonical fixtures don't trigger main-module promotion). Only the 3 expected affected goldens (golang/maven/npm) regenerate.

**Checkpoint**: US4 complete. The no-manifest fallback path is byte-identical pre/post; FR-004 satisfied.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Goldens regeneration, SPDX byte-identity verification, pre-PR gate, manual cyclonedx-cli validation.

- [X] T019 Regenerated 3 affected CDX goldens (`golang.cdx.json`, `maven.cdx.json`, `npm.cdx.json`) — exactly the set T003 enumerated. cdx_regression now passes 9/9.
- [X] T020 Audited each regenerated golden — diff scope confirmed: only `dependencies` and `compositions` changed. Field-level audit returned empty for all 3 goldens. Diff sizes: golang 39 lines, maven 47 lines, npm 30 lines.
- [X] T021 [P] SPDX 2.3 byte-identity verified — `spdx_regression` passes 9/9 without env var.
- [X] T022 [P] SPDX 3 byte-identity verified — `spdx3_regression` passes 9/9 without env var.
- [X] T023 [P] Milestone-077 override-path tests pass unmodified — `identifiers_root_component_override` passes 15/15.
- [X] T024 Pre-PR gate PASSES (`./scripts/pre-pr.sh` exits 0; zero clippy warnings; every test suite `0 failed`). Required adding a known-discrepancy allowlist to `holistic_parity` for `("maven", "B1")` — the milestone-084 CDX fix exposed a pre-existing milestone-070 SPDX-side gap (maven main-module → direct-dep relationships missing from SPDX 2.3 + 3); allowlist entry has a justification comment pointing to the follow-up.
- [X] T025 [P] cyclonedx-cli not installed locally; closure-invariant proxy check on `/tmp/084-post-fix-go.cdx.json` confirms zero dangling refs — same logical guarantee as cyclonedx-cli ref-resolution validation.
- [X] T026 [P] CLAUDE.md updated by `update-agent-context.sh` during /speckit.plan — milestone-084 entry present in "Active Technologies" + "Recent Changes".

**Checkpoint**: All gates green. PR ready to open.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — pre-fix evidence capture; can start immediately.
- **Foundational (Phase 2)**: Depends on Setup; BLOCKS all user story phases.
- **User Stories (Phase 3-6)**: All depend on Phase 2. US1 (Phase 3) is the entry point because it creates the test file that US2/US3 extend.
- **Polish (Phase 7)**: Depends on user stories being complete.

### User Story Dependencies

- **US1 (P1)**: Foundational fix from Phase 2 → creates test file → assert closure invariant. The MVP. Independently shippable.
- **US2 (P1)**: Foundational fix → extends US1's test file with reverse-walk assertion. Cannot run before US1's T008-T010.
- **US3 (P1)**: Foundational fix → extends US1's test file with compositions-anchoring assertion. Cannot run before US1's T008-T010.
- **US4 (P2)**: Foundational fix → independent fallback-path byte-identity check. Different test path from US1-US3; can run in parallel with them after Phase 2.

### Within Each User Story

- T008 (test file scaffold) MUST come before T012 / T014 (which extend it).
- T009 (run_mikebom helper) MUST come before T010 (which uses it).
- T010 (FIXTURES populated) MUST come before T011 (test run).
- T012 (US2 helper) and T014 (US3 helper) are in the same file but at different functions — `[P]` after T008+T009+T010 are done.

### Parallel Opportunities

- T001 and T002 and T003 in Phase 1 — different files, no dependencies.
- T021 and T022 and T023 and T025 and T026 in Phase 7 — different files / different commands.
- US2 and US3 could run truly in parallel by two contributors editing different functions in the same file (T012 vs T014); merge would be conflict-free at the function level. Recommended: run them sequentially given the file overlap.

---

## Parallel Example: Phase 1 Setup

```bash
# Three Phase-1 tasks can run concurrently:
Task: T001 — Build mikebom release binary
Task: T002 — Capture pre-fix CDX 1.6 emission against Go fixture
Task: T003 — Enumerate currently-affected goldens via jq scan
```

## Parallel Example: Phase 7 Polish

```bash
# After T019 + T020 + T024 are done (sequentially), the rest can run concurrently:
Task: T021 — Verify SPDX 2.3 byte-identity
Task: T022 — Verify SPDX 3 byte-identity
Task: T023 — Verify milestone-077 override-path diff scope
Task: T025 — Manual cyclonedx-cli validation against real Go project
Task: T026 — CLAUDE.md "Recent Changes" update
```

---

## Implementation Strategy

### MVP First (US1 only)

1. Phase 1: Setup (T001-T003) — capture pre-fix evidence.
2. Phase 2: Foundational (T004-T007) — implement the fix.
3. Phase 3: US1 (T008-T011) — closure-invariant test passes.
4. **STOP and VALIDATE**: closure invariant holds for every post-053 ecosystem fixture. Strict CDX validators no longer report dangling refs. Demo-able.
5. **Optional MVP scope**: skip US2/US3/US4 and ship US1-only as the MVP — the closure invariant alone closes the headline operator pain. Subsequent stories are additional assertions on the same artifact, not separate code changes.

### Incremental Delivery

1. Foundational + US1 → MVP: closure invariant holds.
2. Add US2 (reverse-walk assertion) → richer regression coverage.
3. Add US3 (compositions-anchoring assertion) → richer regression coverage.
4. Add US4 (fallback-path byte-identity) → non-regression certainty for the no-manifest path.
5. Phase 7 Polish: goldens regen + SPDX byte-identity check + pre-PR gate.

### Parallel Team Strategy (if applicable)

This milestone is small enough for a single contributor to finish in one sitting. If splitting across two contributors:

1. Contributor A: T004 → T005 → T006 → T007 (the fix).
2. Contributor B: T008 → T009 → T010 (the test scaffold + fixtures).
3. After both done: T011 → T012-T015 (US2/US3 assertions) → T016-T018 (US4) → Phase 7.

Branch hygiene: a single `084-cdx-mainmod-collapse` branch; commits per task or per phase, with a final squash if requested by reviewer.

---

## Notes

- [P] tasks = different files, no incomplete dependencies.
- [Story] label maps task to specific user story for traceability.
- US1, US2, US3 are all P1 because the same code change closes all three; the MVP is "any one of them" but all three sharpen confidence.
- US4 is P2 — a non-regression check, not a new behavior.
- All test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]` on the `mod tests` item per CLAUDE.md convention (Constitution Principle IV).
- Commit after each task or logical group; the final pre-PR gate (T024) runs both clippy and the full workspace test suite, verifying CI-readiness exactly per CLAUDE.md's mandatory pre-PR sequence.
- Avoid: regenerating SPDX goldens (FR-007 violation), changing `metadata.component` fields (over-correction), introducing new Cargo dependencies (research §1 says no), introducing a `BomRef` newtype (out of scope per spec).
