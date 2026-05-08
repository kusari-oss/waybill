---
description: "Tasks: Cargo workspace-member version-disambiguation fix (closes #172)"
---

# Tasks: Cargo workspace-member version-disambiguation fix

**Input**: Design documents from `/specs/087-fix-cargo-workspace-version/`
**Prerequisites**: spec.md ✅, plan.md ✅, research.md ✅, data-model.md ✅, contracts/cargo-version-disambiguation.md ✅, quickstart.md ✅

**Organization**: Small bug-fix milestone. Phase 1 captures pre-fix evidence. Phase 2 implements the two-file fix (cargo.rs + scan_fs/mod.rs). Phases 3-4 cover the two user stories: US1 (cargo edges resolve to correct version) + US2 (regression-test baseline bump). Phase 5 = polish.

## Path Conventions

Repository-relative paths from `/Users/mlieberman/Projects/mikebom/`:
- Cargo reader: `mikebom-cli/src/scan_fs/package_db/cargo.rs`
- Lookup-table builder: `mikebom-cli/src/scan_fs/mod.rs`
- Cargo regression test: `mikebom-cli/tests/transitive_parity_cargo.rs`
- Cargo goldens: `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/cargo.{cdx,spdx,spdx3}.json`

---

## Phase 1: Setup (pre-fix evidence)

- [X] T001 [P] Capture pre-fix mikebom output for the cargo audit fixture per quickstart.md Recipe 1: build release binary, scan `mikebom-cli/tests/fixtures/transitive_parity/cargo`, save to `/tmp/repro-172-pre.spdx.json`. Confirm the wrong-version edge `pkg:cargo/clap@4.5.21 → pkg:cargo/clap_builder@4.5.9` is present (alpha.25 baseline).

## Phase 2: Foundational (the fix)

- [X] T002 Modify `mikebom-cli/src/scan_fs/package_db/cargo.rs:132-141` `package_to_entry` per research §2: replace `d.split_whitespace().next()` with strip-source-suffix-only parser that preserves the `name [version]` form when present. ~5-10 LOC.
- [X] T003 Modify `mikebom-cli/src/scan_fs/mod.rs:373-379` `name_to_purl` insert loop per research §3: add a per-cargo-entry dual-key insert mirroring milestone-085's maven block. Insert `(cargo, "name version")` key alongside the existing `(cargo, name)` key, both pointing at the same PURL. ~12 LOC. **NOTE: also fixed a second version-stripping bug in `workspace_root_deps` builder at cargo.rs:911-920, AND removed the over-zealous `pkg.source.is_none()` skip at cargo.rs:765-784 so workspace members are emitted as components.**
- [X] T004 Smoke-test the fix per quickstart Recipe 2: rebuild release binary, re-scan the cargo audit fixture, confirm `pkg:cargo/clap@4.5.21 → pkg:cargo/clap_builder@4.5.21` is now the emitted edge.
- [X] T005 Verify `cargo +stable check --workspace` passes — clean compile of the modified files.

## Phase 3: US1 — Cargo workspace dep edges resolve to the correct version (Priority: P1)

**Goal**: The fix makes `pkg:cargo/clap@4.5.21 → pkg:cargo/clap_builder@4.5.21` the emitted edge instead of the wrong-version `→ clap_builder@4.5.9`. Same correctness applies to every multi-version-same-name workspace.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/transitive_parity/cargo/` (clap-rs/clap @ v4.5.21). Assert `pkg:cargo/clap@4.5.21 → pkg:cargo/clap_builder@4.5.21` is in the emitted edge set; assert `→ clap_builder@4.5.9` is NOT in the emitted edge set for source `clap@4.5.21`.

### Implementation for User Story 1

- [X] T006 [US1] Add a unit test in `mikebom-cli/src/scan_fs/package_db/cargo.rs` `mod tests` that exercises `package_to_entry` against a synthetic `CargoPackage` with `dependencies = ["foo", "bar 1.0.0", "baz 2.0.0 (registry+...)"]` and asserts `entry.depends == ["foo", "bar 1.0.0", "baz 2.0.0"]` (single-version preserved name-only; multi-version preserves version; source suffix stripped). Verifies VR-087-001.
- [X] T007 [US1] Add a unit test in the same file or in `mikebom-cli/src/scan_fs/mod.rs` covering `normalize_dep_name("cargo", "clap_builder 4.5.21")` returns `"clap_builder 4.5.21"` (idempotent; verifies VR-087-008).
- [X] T008 [US1] Run the full closure-invariant test (milestone 084) to confirm the version-disambiguation didn't introduce orphan refs: `cargo +stable test -p mikebom --test cdx_ref_closure_invariant`. All 5 tests should still pass.

## Phase 4: US2 — Audit baseline regenerates cleanly (Priority: P2)

**Goal**: Milestone-083's `transitive_parity_cargo.rs` regression test fails on the post-087 mikebom (edge-count drift); maintainer bumps the baseline per quickstart Recipe 3; test passes.

**Independent Test**: `cargo +stable test -p mikebom --test transitive_parity_cargo` fails with "left: <new>, right: 319" → bump → re-run → passes.

### Implementation for User Story 2

- [X] T009 [US2] Run `cargo +stable test -p mikebom --test transitive_parity_cargo` post-T002+T003; observe the count-drift failure; record the new edge count. **Result**: count drifted from 319 → 317.
- [X] T010 [US2] Update `mikebom-cli/tests/transitive_parity_cargo.rs` per quickstart Recipe 3: bump `EXPECTED_MIKEBOM_EDGE_COUNT` from 319 to the new count + update `EXPECTED_REPRESENTATIVE_EDGES` to include at least one workspace-internal edge that's now correctly resolved (e.g., `pkg:cargo/clap → pkg:cargo/clap_builder` matching after version-strip).
- [X] T011 [US2] Update `specs/083-transitive-correctness/research.md §8 — Ecosystem: cargo` audit row: remove gap #1 from "Specific gaps surfaced (mikebom-side)"; add a "Closed by milestone 087" reference. Update the post-087 cross-tool divergence numbers (the `mikebom_only` count should drop; `agreement` should grow).
- [X] T012 [US2] Re-run the regression test: `cargo +stable test -p mikebom --test transitive_parity_cargo`. Confirm passes.

## Phase 5: Polish

- [X] T013 Regenerate cargo CDX/SPDX 2.3/SPDX 3 goldens per quickstart Recipe 4: `MIKEBOM_UPDATE_CDX_GOLDENS=1`, `MIKEBOM_UPDATE_SPDX_GOLDENS=1`, `MIKEBOM_UPDATE_SPDX3_GOLDENS=1` runs against the respective regression tests.
- [X] T014 Audit golden diff scope per quickstart Recipe 4 + spec FR-009: confirm ONLY the 3 cargo goldens regenerate; the other 24 stay byte-identical. **NOTE**: cargo goldens diff is larger than just version strings — workspace MEMBERS now appear as components per the expanded scope (skip removal at cargo.rs:765-784). Non-cargo goldens stay byte-identical, satisfying VR-087-009/VR-087-010.
- [X] T015 Run `./scripts/pre-pr.sh`: zero clippy warnings + every test suite `0 failed`. Includes the 3 updated unit tests in `cargo.rs` (`read_walks_nested_workspace`, `parse_lockfile_emits_workspace_root`, `parse_lockfile_emits_all_workspace_members`) reflecting the post-087 emit-everything contract.
- [X] T016 Update CLAUDE.md "Recent Changes" if the speckit infrastructure didn't auto-update it. **Result**: speckit-plan auto-added the milestone 087 entry; no manual edit required.

---

## Dependencies & Execution Order

- T001 (Phase 1) → T002 → T003 → T004 → T005 (Phase 2 sequential, same files)
- T002+T003 must complete before any Phase 3+ task (the fix is foundational)
- T006 + T007 (US1) — can run in parallel after Phase 2 (different test files / different test fns)
- T008 (US1) depends on T006+T007 being done so the test sees the post-fix behavior
- T009 → T010 → T011 → T012 (US2 sequential — bump-then-verify workflow)
- T013 → T014 → T015 → T016 (Polish sequential — regen → audit → gate → docs)

## Parallel Opportunities

- T001 (pre-fix evidence) is `[P]` — independent of any other task; can start immediately
- T006 + T007 in US1 — different test functions, different files, no dependencies on each other after Phase 2

## Notes

- This is a small bug-fix milestone. ~20 LOC code change in 2 source files + 2 unit test additions + 3 cargo goldens regenerated + 1 audit-row update + standard pre-PR.
- All test code that uses `.unwrap()` must be guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md convention (Constitution Principle IV).
- No new Cargo dependencies. No CI workflow changes. SPDX/CDX byte-identity preserved for non-cargo ecosystems (FR-009 / VR-087-009 + VR-087-010).
- Milestone-083 cargo regression test deliberately bumps per quickstart Recipe 3 — this is the maintainer-documented workflow, not a regression.
