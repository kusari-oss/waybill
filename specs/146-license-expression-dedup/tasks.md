---
description: "Task list for milestone 146 — SPDX license expression operand dedup (closes issue #470)"
---

# Tasks: milestone 146 — SPDX license expression operand dedup

**Input**: Design documents from `/specs/146-license-expression-dedup/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: Spec mandates tests via SC-002 + SC-004 + SC-005. Test tasks are included inline alongside implementation tasks (per the project's existing Rust convention of in-file `#[cfg(test)] mod tests`).

**Organization**: US1 (AND-chain) + US2 (OR-chain) share a single dedup helper in `SpdxExpression::try_canonical` (the helper handles both operators symmetrically). So Phase 2 (Foundational) lands the helper, and Phases 3 + 4 are pure test additions per story. Phase 5 polishes + commits.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Maps to user story from spec.md (US1, US2)
- Paths are absolute under `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

All paths absolute under repository root. Single Rust source file touched in `mikebom-common/`; tests in same file's `#[cfg(test)] mod tests` block plus one integration test in `mikebom-cli/tests/`.

---

## Phase 1: Setup

**Purpose**: Verify baseline before any code change.

- [X] T001 Confirm baseline pre-PR gate is green on branch `146-license-expression-dedup`. Run `./scripts/pre-pr.sh` from repo root. Expected: clippy `--workspace --all-targets -- -D warnings` clean; `cargo test --workspace` passes except for the pre-existing local-environment `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` failure documented in milestone-144 T001. If anything ELSE fails, halt and investigate before proceeding.

---

## Phase 2: Foundational — dedup helper + wire into `try_canonical`

**Purpose**: Add the single private helper that BOTH US1 (AND-chain dedup) and US2 (OR-chain dedup) depend on. After this phase, the behavior is fully changed for all consumers (CDX builder, SPDX 2.3 emitter, SPDX 3 emitter); US1 + US2 phases then add the test coverage that asserts the new behavior.

**⚠️ CRITICAL**: BOTH user stories depend on Phase 2 completion. Foundational changes ARE the behavior change in this milestone (the helper IS the dedup logic); the per-story phases are purely tests.

- [X] T002 Add private helper `fn dedupe_top_level_operands(expr: &spdx::Expression) -> String` to `/Users/mlieberman/Projects/mikebom/mikebom-common/src/types/license.rs` per `contracts/spdx-expression-dedup.md` and research §B. Use the algorithm sketched in research.md §B:
  1. Walk `expr.iter()` collecting `Vec<&ExprNode>`.
  2. If empty, return `expr.to_string()` (single-operand no-op).
  3. Determine outermost connector from `nodes.last()`; if not an `Op`, return `expr.to_string()` (single-operand no-op per Invariant 4).
  4. Verify all `Op` nodes match the outermost connector; if mixed (e.g., expression contains both `And` and `Or`), return `expr.to_string()` unchanged (Invariant 5).
  5. Collect unique `req.req.to_string()` values via a `BTreeSet<String>` for tracking + a `Vec<String>` for first-occurrence ordering.
  6. If only one unique operand survives, return it bare (no separator).
  7. Otherwise rejoin with `" AND "` or `" OR "` based on the outermost connector.

  Place the helper as a free function in the same module (private — `fn` not `pub fn`). Include a doc-comment block referencing the milestone (`// Milestone 146 (closes #470): ...`) + Constitution Principle V audit reference + the four contract invariants (1-4) the helper directly satisfies.

- [X] T003 Modify `try_canonical` at `/Users/mlieberman/Projects/mikebom/mikebom-common/src/types/license.rs:51-64` to call the new helper. Pattern:

  ```rust
  pub fn try_canonical(raw: &str) -> Result<Self, LicenseError> {
      let trimmed = raw.trim();
      if trimmed.is_empty() {
          return Err(LicenseError::Empty);
      }
      match spdx::Expression::parse(trimmed) {
          Ok(expr) => {
              // Milestone 146 (closes #470): dedupe byte-identical
              // top-level operands in homogeneous AND-/OR-chains
              // before storing. SPDX 2.x defines `X AND X ≡ X` and
              // `X OR X ≡ X` as canonical equivalences; the spdx
              // crate's parse + Display round-trip preserves
              // duplicates verbatim, so we dedupe here.
              Ok(Self(dedupe_top_level_operands(&expr)))
          }
          Err(e) => Err(LicenseError::Invalid(e.to_string())),
      }
  }
  ```

  Preserve the existing `SpdxExpression::new` constructor untouched (per FR-006 — the lenient constructor MUST NOT apply dedup).

**Checkpoint**: After Phase 2, `cargo +stable build -p mikebom-common` compiles clean. The helper is exercised by EVERY downstream consumer (CDX emitter, SPDX 2.3 emitter, SPDX 3 emitter) the next time they construct an `SpdxExpression` via `try_canonical` from a raw input — including existing test cases. Any pre-fix test that asserted `try_canonical("MIT AND MIT").as_str() == "MIT AND MIT"` would now fail (which is expected — that's the wire-output break the spec explicitly accepts). Phase 3 + Phase 4 add the new tests that assert the corrected behavior.

---

## Phase 3: User Story 1 - Top-level AND-operand dedup (Priority: P1) 🎯 MVP

**Goal**: Add test coverage asserting Invariants 1, 3, 4 (AND-chain dedup; WITH-clause atomicity; no-op for single-operand / already-deduped / mixed-operator inputs).

**Independent Test**: `cargo test -p mikebom-common types::license::tests::try_canonical_dedupes_two_identical_and_operands` — passes if the new helper from T002 + T003 collapses `MIT AND MIT` to `MIT`.

### Implementation for User Story 1

(No US1-only implementation tasks — T002 + T003 from Phase 2 cover the AND-chain dedup. All US1 tasks are tests.)

- [X] T004 [P] [US1] Add unit test `try_canonical_dedupes_two_identical_and_operands` to `/Users/mlieberman/Projects/mikebom/mikebom-common/src/types/license.rs#[cfg(test)] mod tests` (existing block). Asserts `SpdxExpression::try_canonical("MIT AND MIT").unwrap().as_str() == "MIT"`. Covers US1 scenario 1 + SC-002 (anchor test).

- [X] T005 [P] [US1] Add unit test `try_canonical_dedupes_with_distinct_operand_preserved` in the same `mod tests`. Asserts `SpdxExpression::try_canonical("MIT AND Apache-2.0 AND MIT").unwrap().as_str() == "MIT AND Apache-2.0"`. Covers US1 scenario 2.

- [X] T006 [P] [US1] Add unit test `try_canonical_dedupes_multiple_occurrences_preserves_first_order` in the same `mod tests`. Asserts `SpdxExpression::try_canonical("GPL-2.0-only AND GPL-2.0-only AND LGPL-2.1-or-later AND GPL-2.0-only").unwrap().as_str() == "GPL-2.0-only AND LGPL-2.1-or-later"`. Covers US1 scenario 3 + FR-002 first-occurrence-order guarantee.

- [X] T007 [P] [US1] Add unit test `try_canonical_already_deduped_unchanged` in the same `mod tests`. Asserts `SpdxExpression::try_canonical("MIT AND Apache-2.0").unwrap().as_str() == "MIT AND Apache-2.0"`. Covers US1 scenario 4 + FR-004 no-op guarantee for already-canonical inputs.

- [X] T008 [P] [US1] Add unit test `try_canonical_with_clauses_preserved_atomic` in the same `mod tests`. Two assertions in one test:
  1. SAME WITH-clause both sides → dedupes: `SpdxExpression::try_canonical("GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later WITH Classpath-exception-2.0").unwrap().as_str() == "GPL-2.0-or-later WITH Classpath-exception-2.0"`.
  2. DIFFERENT operands (one with WITH, one without) → no dedupe: `SpdxExpression::try_canonical("GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later").unwrap().as_str() == "GPL-2.0-or-later WITH Classpath-exception-2.0 AND GPL-2.0-or-later"`.

  Covers SC-005 (WITH-atomicity guard for FR-003).

- [X] T009 [P] [US1] Add unit test `try_canonical_single_operand_unchanged` in the same `mod tests`. Asserts `SpdxExpression::try_canonical("MIT").unwrap().as_str() == "MIT"`. Covers FR-004 (single-operand no-op).

- [X] T010 [P] [US1] Add unit test `try_canonical_mixed_operators_unchanged` in the same `mod tests`. Asserts `SpdxExpression::try_canonical("MIT OR Apache-2.0 AND MIT").unwrap().as_str()` equals whatever the `spdx` crate's canonical form produces (NOT a deduped form — mixed-operator case deferred per spec Out of Scope §1). The test should record the actual canonical-string output as the expected value rather than asserting on a hardcoded string (so a future spdx-crate canonicalization tweak doesn't break the test for the wrong reason). Pattern:

  ```rust
  #[test]
  fn try_canonical_mixed_operators_unchanged() {
      let input = "MIT OR Apache-2.0 AND MIT";
      let canonical_baseline = spdx::Expression::parse(input).unwrap().to_string();
      let our_output = SpdxExpression::try_canonical(input).unwrap().as_str().to_string();
      assert_eq!(
          our_output, canonical_baseline,
          "mixed-operator expression must not be deduped (recursive dedup out of v1.0 scope per spec Out of Scope §1)"
      );
  }
  ```

  Covers Invariant 5 (out-of-scope guard).

**Checkpoint**: After Phase 3, US1 is fully covered by tests. `cargo test -p mikebom-common types::license::` runs green; the 7 new US1 tests fire green.

---

## Phase 4: User Story 2 - Top-level OR-operand dedup (Priority: P2)

**Goal**: Add test coverage asserting Invariant 2 (OR-chain dedup). No new code — the Phase 2 helper handles AND and OR symmetrically.

**Independent Test**: `cargo test -p mikebom-common types::license::tests::try_canonical_dedupes_or_operands` — passes if the new helper from T002 + T003 collapses `MIT OR MIT` to `MIT`.

### Implementation for User Story 2

(No US2-only implementation tasks — T002 + T003 from Phase 2 cover the OR-chain dedup symmetrically with AND.)

- [X] T011 [P] [US2] Add unit test `try_canonical_dedupes_or_operands` to `/Users/mlieberman/Projects/mikebom/mikebom-common/src/types/license.rs#[cfg(test)] mod tests`. Asserts `SpdxExpression::try_canonical("MIT OR MIT").unwrap().as_str() == "MIT"`. Covers US2 scenario 1.

- [X] T012 [P] [US2] Add unit test `try_canonical_dedupes_or_chain_distinct_preserved` in the same `mod tests`. Asserts `SpdxExpression::try_canonical("MIT OR Apache-2.0 OR MIT").unwrap().as_str() == "MIT OR Apache-2.0"`. Covers US2 scenario 2.

- [X] T013 [P] [US2] Add unit test `try_canonical_is_idempotent` in the same `mod tests`. Pattern:

  ```rust
  #[test]
  fn try_canonical_is_idempotent() {
      let e1 = SpdxExpression::try_canonical("MIT AND MIT").unwrap();
      let e2 = SpdxExpression::try_canonical(e1.as_str()).unwrap();
      assert_eq!(e1.as_str(), e2.as_str(), "second pass must be a no-op");
      assert_eq!(e1.as_str(), "MIT", "first pass already deduped");
  }
  ```

  Covers Invariant 7 (idempotence).

**Checkpoint**: After Phase 4, US2 is fully covered. `cargo test -p mikebom-common types::license::` shows the 3 new US2 tests fire green. Phase 3 + Phase 4 combined add 10 unit tests; the contract's 12-test surface is now 10/12 (the existing `try_canonical_empty_returns_error` + `try_canonical_invalid_returns_error` tests in the file already cover Invariant 6).

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Integration test, golden refresh, pre-PR gate, commit.

- [X] T014 Add new integration test file `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/license_dedup_integration_md146.rs`. Test name: `license_dedup_end_to_end_via_synthetic_rpm`. Build a synthetic RPM via `rpm::PackageBuilder::new(...).license("MIT AND MIT")...build().write_file(&path)` (mirror the milestone-144 T035 pattern at `mikebom-cli/src/scan_fs/package_db/rpm_file.rs:524-577`). Invoke `mikebom sbom scan` as a subprocess (via `env!("CARGO_BIN_EXE_mikebom")`) emitting all three formats. Assert:
  1. CDX: `.components[0].licenses[0].license.id == "MIT"` (single-id shape, NOT compound `.license.expression`). Side benefit: post-146 single-id shape improves CDX schema-validation rates per spec Out of Scope §7.
  2. SPDX 2.3: `.packages[0].licenseDeclared == "MIT"`.
  3. SPDX 3: the `software_Package` for this RPM has `software_declaredLicense == "MIT"` (locate via `@graph` filter by `software_packageUrl` starting with `pkg:rpm/`).

  Covers SC-004 end-to-end. Use `#[cfg_attr(test, allow(clippy::unwrap_used))]` per project convention.

- [X] T015 [P] Audit existing byte-identity goldens for pre-fix `X AND X` patterns. Run:

  ```bash
  grep -rEn '"([^"]+)" AND \1"|"licenseDeclared":"([^"]+) AND \2"|"id":"([^"]+) AND \3"' /Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/golden/ 2>/dev/null
  ```

  If any matches surface, document them in the PR description (operator-visible signal of pre-fix shape in goldens). If zero matches, the golden-refresh step (T016) should produce zero diffs — confirms US1 + US2 don't touch shipped fixtures, only fix wire output on real-world Yocto-shaped inputs.

- [X] T016 Run the golden-refresh trifecta + inspect diffs:

  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression
  MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --test spdx_regression
  MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression
  ```

  Inspect `git diff --stat -- mikebom-cli/tests/fixtures/golden/`. Each affected line MUST be a license-string simplification (single grep-able pattern: `"<X> AND <X>"` → `"<X>"`) OR a CDX `expression` → `id` shape shift (compound license value compressed to single-id form, which routes CDX through `licenses[].license.id` instead of `licenses[].license.expression`). Reject any unrelated drift. If existing fixtures had no `X AND X` shapes (expected per T015 audit), the diff is empty.

- [X] T017 Run mandatory pre-PR gate per Constitution Development Workflow + memory `feedback_prepr_gate_full_output.md`: `./scripts/pre-pr.sh` from repo root. Both clippy + test steps MUST pass clean (except the pre-existing `sbomqs_parity` env-only failure documented in milestone-144 T001 — CI will validate on a clean runner). If any OTHER test fails, scan the FULL output (do NOT grep on `^test result: FAILED` — known to drop multi-test-suite summaries). Covers SC-006.

- [ ] T018 Commit the milestone-146 changes. Per project convention (matching milestones 134/144/145), use 4 commits:
  - `spec(146): SPDX license expression operand dedup (closes #470)` — spec.md + checklists/requirements.md
  - `plan(146): dedup pass design + spdx-crate API verification + dedup contract` — plan + research + data-model + contracts + quickstart + CLAUDE.md
  - `tasks(146): 18 tasks across 5 phases for license expression dedup` — tasks.md
  - `impl(146): dedupe top-level operands in SpdxExpression::try_canonical (closes #470)` — `mikebom-common/src/types/license.rs` + `mikebom-cli/tests/license_dedup_integration_md146.rs` + any golden refresh from T016

  Do NOT commit until T017 passes clean. Use `git add <specific paths>` (never `-A`). Each commit ends with the standard `Co-Authored-By` trailer.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. Verifies baseline.
- **Phase 2 (Foundational)**: Depends on Phase 1. **Lands the actual behavior change** — both AND and OR dedup go live the moment T003 lands.
- **Phase 3 (US1)**: Depends on Phase 2. Pure test additions (no code change).
- **Phase 4 (US2)**: Depends on Phase 2. Pure test additions; independent of Phase 3.
- **Phase 5 (Polish)**: Depends on US1+US2 being complete (or whichever subset is being shipped as MVP).

### User Story Dependencies

- **US1 (P1, MVP)**: Standalone after Phase 2. T002 + T003 are the actual MVP code; US1's 7 tests (T004-T010) cover the AND-chain semantics.
- **US2 (P2)**: Standalone after Phase 2. T002 + T003 simultaneously enable OR-chain dedup; US2's 3 tests (T011-T013) cover the OR-chain semantics.

**Special note**: Unlike most milestones where US1 and US2 are separable code paths, milestone 146's US1 and US2 share IDENTICAL implementation — the dedup helper handles AND and OR symmetrically. If US2 were dropped from this milestone's scope, T002's `match outermost_op` arm for `Or` would still exist (for safety + symmetry), and Phase 4 tests would simply be omitted. There's no "ship US1 only without US2 code" option — they're coupled at the implementation level. The phase split exists for test-organization clarity, not code-shippability separation.

### Within Each User Story

- All test tasks marked [P] within a story — they're independent assertions in the same `mod tests` block; no write conflict.

### Parallel Opportunities

Phase 3 + Phase 4 can run in parallel after Phase 2 lands (different developers / different commits if desired). Within each phase, all test additions are [P] (independent assertions in the same `mod tests`).

Phase 5 polish is sequential (audit → refresh → pre-PR → commit), except T015 [P] (audit) which can run alongside T014 (integration test).

---

## Parallel Example: Phase 3 (US1) after T002 + T003 land

```bash
# After foundational change in license.rs lands:
Task T004: try_canonical_dedupes_two_identical_and_operands — anchor SC-002 test
Task T005: try_canonical_dedupes_with_distinct_operand_preserved
Task T006: try_canonical_dedupes_multiple_occurrences_preserves_first_order
Task T007: try_canonical_already_deduped_unchanged
Task T008: try_canonical_with_clauses_preserved_atomic  (SC-005 anchor)
Task T009: try_canonical_single_operand_unchanged
Task T010: try_canonical_mixed_operators_unchanged
```

All 7 tests can be added in one editor pass (single `mod tests` block; no write conflict).

---

## Implementation Strategy

### MVP First (US1 alone — but US2 code ships anyway)

1. Complete Phase 1: T001 baseline check.
2. Complete Phase 2: T002 + T003 — the dedup helper + wiring. **This is the actual behavior change.**
3. Complete Phase 3: T004-T010 — US1's 7 tests assert the AND-chain dedup contract.
4. **STOP and VALIDATE**: `./scripts/pre-pr.sh` clean. Quickstart §Scenario 1 confirms AND-chain dedup works on synthetic RPMs.
5. This alone is a shippable PR. US2 (OR-chain dedup) is functionally already enabled but not yet tested; skipping Phase 4 leaves it untested but working.

### Incremental / Recommended (single-PR delivery)

1. Phase 1 (T001) baseline.
2. Phase 2 (T002 + T003) foundational.
3. Phase 3 (T004-T010) US1 tests — 7 tests.
4. Phase 4 (T011-T013) US2 tests — 3 tests.
5. Phase 5 (T014-T018) polish — integration test + golden refresh + pre-PR + commit.

Total: 18 tasks. Estimated ~30-50 LOC of code + ~150 LOC of tests, single PR.

### Single-developer Note

This milestone is small enough that one developer can work through all phases in one session. The [P] markers exist primarily to signal "no cross-file write conflict" for parallel tooling (Aider, Cline, etc.) — they're not load-bearing for a human implementer.

---

## Notes

- Tests live in-file under `#[cfg(test)] mod tests` per the project's existing convention. The one out-of-source test is the integration test (T014) at `mikebom-cli/tests/license_dedup_integration_md146.rs`.
- The `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention applies to any test module per Constitution Principle IV.
- Memory `feedback_prepr_gate_full_output.md` is directly relevant: when verifying T017, scan the FULL output rather than greping on `^test result: FAILED`.
- Memory `feedback_dont_dismiss_test_failures.md` is relevant if any new test failures surface: verify reproducibility before calling anything "pre-existing flake".
- The commit-message convention (T018) follows the milestone-134/144/145 precedent: `spec(146):` / `plan(146):` / `tasks(146):` / `impl(146):`.
- Per spec SC-007 (operator-cadence harness re-run): document in the PR description that the operator should re-run the sbom-conformance harness against pre-/post-146 builds to confirm the 7 distinct `licenseDeclared X AND X` finding clusters drop to 0. The harness is NOT a CI gate; T004 + T011 + T014 are the CI-binding signal.
