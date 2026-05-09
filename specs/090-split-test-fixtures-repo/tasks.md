---
description: "Tasks: Split test fixtures into separate repo to remove security-scanner trigger surface"
---

# Tasks: Split test fixtures into separate repo

**Input**: Design documents from `/specs/090-split-test-fixtures-repo/`
**Prerequisites**: spec.md ✅, plan.md ✅, research.md ✅, data-model.md ✅, contracts/fixture-path-helper.md ✅, quickstart.md ✅

**Organization**: Substantial multi-repo migration (44 fixture directories + ~76 file rewrites + new external repo). Phase 1 = pre-fix evidence + new-repo bootstrap (REQUIRES USER AUTHORIZATION for the GitHub repo creation). Phase 2 = build.rs + helper + pin-file infrastructure (foundational; gates US2/US3/US4 wiring). Phase 3 = US2 (mechanical path rewrites + fixture deletions; regression net — must run BEFORE US1's scan-cleanliness verification because if tests break, the migration is wrong). Phase 4 = US1 (scan-cleanliness verification). Phase 5 = US3 (pin reproducibility). Phase 6 = US4 (offline-dev verification). Phase 7 = Polish.

## Path Conventions

Repository-relative paths from `/Users/mlieberman/Projects/mikebom/`:
- New external repo: `kusari-sandbox/mikebom-test-fixtures` (HTTPS clone URL: `https://github.com/kusari-sandbox/mikebom-test-fixtures.git`)
- Pin file: `tests/fixtures.rev`
- Build script: `mikebom-cli/build.rs`
- Helper: `mikebom-cli/tests/common/fixtures.rs`
- CI workflow: `.github/workflows/ci.yml`
- Audit doc to update: `specs/083-transitive-correctness/research.md`

---

## Phase 1: Setup (pre-fix evidence + new-repo bootstrap)

- [X] T001 [P] Capture the pre-fix trivy scan baseline: `trivy --quiet fs --scanners vuln --skip-dirs target --format json --output /tmp/pre-090.json .`. Record the total advisory count (expected: ≥38) and the histogram by-target. Records the baseline for FR-001 / SC-001 verification.
- [X] T002 [P] **REQUIRES USER AUTHORIZATION**: Create the new GitHub repo `kusari-sandbox/mikebom-test-fixtures` per quickstart Recipe 1, step 1. This is a shared-system action — confirm with the user before running `gh repo create kusari-sandbox/mikebom-test-fixtures --public --description "Intentionally-vulnerable test fixtures for the mikebom SBOM tool. See README.md."`. Verifies VR-090-001's prerequisite.
- [X] T003 Seed the new repo with the move-set content per quickstart Recipe 1, steps 2–4: copy 44 manifest-bearing directories from mikebom main repo to a working dir, write the README.md per VR-090-002, initial-commit + push to the new repo. Verifies VR-090-001 + VR-090-002 + VR-090-003.
- [X] T004 Capture the new repo's HEAD SHA after T003's push: `cd <work-dir> && git rev-parse HEAD`. Save for T005.

## Phase 2: Foundational (build.rs + helper + pin-file infrastructure)

- [X] T005 Create `tests/fixtures.rev` at the mikebom main repo root containing the SHA from T004 (single line, 40-char hex, trailing newline). Verifies VR-090-004 + VR-090-005.
- [X] T006 Write `mikebom-cli/build.rs` per quickstart Recipe 3: read `tests/fixtures.rev`, resolve cache target, cache-hit short-circuit, cache-miss `git clone --depth 1 + git fetch + git reset --hard <sha>`, emit `cargo:rustc-env=MIKEBOM_FIXTURES_DIR=<absolute-path>`, emit `cargo:rerun-if-changed=../tests/fixtures.rev`. Use `expect()` with structured panic messages per FR-007 (NOT `.unwrap()` — Constitution Principle IV applies to build scripts too). Verifies VR-090-006 + VR-090-009 + VR-090-010 + VR-090-011 + FR-007.
- [X] T007 Verify `mikebom-cli/Cargo.toml` has `[package] build = "build.rs"` entry (Cargo's default; usually not needed explicitly, but confirm `mikebom-cli/Cargo.toml` doesn't disable build scripts via `links = ""` or similar). If `[package]` lacks an explicit `build = ` line and `build.rs` exists at the crate root, Cargo picks it up automatically.
- [X] T008 Create `mikebom-cli/tests/common/fixtures.rs` (or extend `mikebom-cli/tests/common/mod.rs`) with the `fixture_path(rel: &str) -> std::path::PathBuf` helper per contract. Use `env!("MIKEBOM_FIXTURES_DIR")` for the compile-time check. Verifies VR-090-013 + VR-090-014.
- [X] T009 Smoke-test the build.rs + helper wiring: `rm -rf ~/.cache/mikebom/fixtures && cargo +stable build --workspace`. Confirm `cargo:warning=fetching mikebom-test-fixtures @ <sha>` appears in output, `~/.cache/mikebom/fixtures/<sha>/` exists post-build with the seeded content, and `MIKEBOM_FIXTURES_DIR` is set. Verifies VR-090-007 + VR-090-008 + FR-005 (≤30s wall-time).
- [X] T010 Verify build.rs error path: temporarily corrupt `tests/fixtures.rev` (e.g., replace SHA with `0000000000000000000000000000000000000000`) and run `cargo +stable build --workspace`. Confirm the build fails with the structured "Failed to fetch ..." panic message naming URL + cache path + workaround. Restore the correct SHA. Verifies FR-007 + Constitution Principle III.

## Phase 3: US2 — Test suite runs end-to-end (Priority: P1) — regression net + the bulk of the migration

**Goal**: Confirm `cargo +stable test --workspace` passes 0-failed after fixtures move out of mikebom main repo and test code is rewritten to use the new resolver.

**Independent Test**: From a fresh clone of mikebom main repo with no prior fixture cache, run `./scripts/pre-pr.sh`. Confirm build.rs auto-fetches the fixture repo to the cache AND every test suite reports `0 failed`.

**Why first among P1s**: if tests break, the migration is wrong and US1's scan-cleanliness payoff is moot.

### Implementation for User Story 2

- [X] T011 [US2] Update `mikebom-cli/tests/transitive_parity_common/mod.rs::fixture_path` (line ~50 per data-model.md VR-090-015) to delegate to the new common helper. Don't introduce parallel resolvers.
- [X] T012 [US2] Update `mikebom-cli/tests/common/mod.rs` fixture matrix path resolution to use the new `fixture_path()` helper. The 9-ecosystem `CASES` array (per memory of milestone-013 fixtures) consumes paths via the matrix; update the consumption pattern.
- [X] T013 [US2] Mechanical path-rewrite across `mikebom-cli/tests/*.rs` integration tests per data-model.md "Migration mapping" table. Use the find-and-replace pattern from quickstart Recipe 4. Cover all ~70 affected files. Imports: replace whatever `workspace_root`-anchored fixture-resolution calls each file uses with `use crate::common::fixtures::fixture_path;` or equivalent. Goldens / schemas / binaries / OS-package synthetic / `reference/` / `polyglot-rpm-binary/` / `gem-source-project/` / `polyglot-five/` / `sample-attestation.json` references MUST stay unchanged. Verifies VR-090-016 + VR-090-017 + VR-090-018 + FR-009.
- [X] T014 [US2] Mechanical path-rewrite across `mikebom-cli/src/*.rs` test modules (~6 files). Same pattern as T013 but for `#[cfg(test)] mod tests { ... }` blocks inside production-code files. Imports: use `super::common::fixtures::fixture_path` or test-helper-crate-style import.
- [X] T015 [US2] Delete the 44 manifest-bearing fixture directories from mikebom main repo per quickstart Recipe 2, step 2: `git rm -r mikebom-cli/tests/fixtures/{cargo-workspace,maven-multi-module-reactor,npm-scoped-package,npm-workspace,pip-pyproject-pep621,pip-pyproject-poetry-only,transitive_parity}` and `git rm -r tests/fixtures/{cargo,gem,go,maven,npm,polyglot-monorepo,python}`. Verifies FR-002.
- [X] T016 [US2] Run `cargo +stable test --workspace` and confirm every test suite reports `0 failed`. Specifically: the cdx_regression / spdx_regression / spdx3_regression goldens tests still pass (FR-008 invariant), the milestone-083 transitive_parity_* tests still resolve fixtures correctly via the new resolver, the milestone-006 attestation tests still pass (no impact since they don't touch fixtures). Verifies SC-002 + FR-009.

## Phase 4: US1 — Operators see clean security scans (Priority: P1)

**Goal**: trivy fs scan against post-090 mikebom main repo (without `--skip-dirs tests/fixtures`) flags zero advisories from manifest-bearing fake projects.

**Independent Test**: `trivy fs --scanners vuln --skip-dirs target` against a fresh clone of post-090 mikebom main repo. Result matches the production-deps-only result (≤4 advisories — the milestone-089 known-acceptances residuals only).

### Implementation for User Story 1

- [X] T017 [US1] Run trivy post-migration: `trivy --quiet fs --scanners vuln --skip-dirs target --format json --output /tmp/post-090.json .`. Compare against T001's `/tmp/pre-090.json`. Confirm: pre-090 had ≥38 advisories, post-090 has ≤4 (the rustls-webpki@0.102 residuals from milestone 089's known-acceptances). Verifies SC-001.
- [X] T018 [US1] Spot-check that fixture-vuln advisories from specific known-trigger paths are absent: `jq '[.Results[]? | select(.Target | contains("transitive_parity") or contains("polyglot-monorepo") or contains("lockfile-v3"))] | length' /tmp/post-090.json` returns 0. Verifies the FR-001 set-difference check.

## Phase 5: US3 — Maintainers can pin a fixture-repo revision (Priority: P2)

**Goal**: Pin-bump UX is a 1-line `tests/fixtures.rev` diff; build.rs re-fetches on pin change; cache co-exists multiple revisions.

**Independent Test**: Bump `tests/fixtures.rev` to a different SHA, run `cargo +stable build`, confirm build.rs re-fetches into a NEW cache subdirectory (the old SHA's cache stays intact). Restore the original pin.

### Implementation for User Story 3

- [X] T019 [US3] Verify pin-bump behavior: temporarily change `tests/fixtures.rev` to a different commit SHA in the mikebom-test-fixtures repo (e.g., create a no-op commit upstream + use that SHA). Run `cargo +stable build --workspace`. Confirm `~/.cache/mikebom/fixtures/<new-sha>/` is created AND `~/.cache/mikebom/fixtures/<original-sha>/` is unchanged. Restore the original SHA. Verifies VR-090-008's multi-rev co-existence + FR-004.

## Phase 6: US4 — Offline development survives a one-time clone (Priority: P2)

**Goal**: After a one-time online fixture-cache populate, tests run with zero network access.

**Independent Test**: Cache-warm path: disable network, run `cargo +stable test --workspace`. All tests pass.

### Implementation for User Story 4

- [X] T020 [US4] Verify offline behavior: with the fixture cache populated (post-T009), disable network access (`hostname` lookup blocked or similar) and run `cargo +stable test --workspace`. Confirm zero network attempts AND all tests pass. Verifies SC-004 + FR-006.

## Phase 7: Polish

- [X] T021 Add the `actions/cache@v4` step to all 3 lanes in `.github/workflows/ci.yml` per quickstart Recipe 7: `lint-and-test` (Linux), `lint-and-test-macos`, `lint-and-test-ebpf`. Step runs BEFORE `Clippy`. Cache key includes `runner.os` + `hashFiles('tests/fixtures.rev')`. Path: `~/.cache/mikebom/fixtures`. Verifies VR-090-019 + VR-090-020 + FR-010.
- [X] T022 [P] Update `specs/083-transitive-correctness/research.md` §8 — Ecosystem audit rows: replace fixture-path references like `mikebom-cli/tests/fixtures/transitive_parity/<eco>` with the new repo-relative paths (`<mikebom-test-fixtures>/transitive_parity/<eco>` or similar). Cross-reference milestone 090's research.md §4 for the inventory.
- [X] T023 [P] Update `mikebom-cli/Cargo.toml` if needed to reference build.rs explicitly (only if T007 surfaced a need). Otherwise no-op verify.
- [X] T024 Verify zero golden regenerations: `git status --short mikebom-cli/tests/fixtures/golden/` returns empty. Verifies FR-008. ANY golden regen indicates scope creep.
- [X] T025 Run `./scripts/pre-pr.sh`: zero clippy warnings + every test suite reports `0 failed`. Verifies SC-002 + the standard CLAUDE.md mandatory gate.
- [X] T026 Verify `git clone --depth 1` size shrinkage (SC-005): `git clone --depth 1 <pre-090-mikebom> /tmp/pre-090-clone && du -sh /tmp/pre-090-clone` vs `git clone --depth 1 <post-090-mikebom> /tmp/post-090-clone && du -sh /tmp/post-090-clone`. Confirm post-090 is ≥10 MB smaller.
- [X] T027 Update CLAUDE.md "Recent Changes" if the speckit infrastructure didn't auto-update it (verify with `grep "090-split-test-fixtures-repo" CLAUDE.md`).

---

## Dependencies & Execution Order

- T001 + T002 (Phase 1, parallel within phase) — pre-scan + new-repo creation are independent. **T002 requires user authorization** before executing.
- T003 → T004 (Phase 1, sequential) — seed depends on repo existence; SHA capture depends on push.
- T005 → T006 → T008 → T009 (Phase 2, sequential — pin file → build.rs → helper → smoke test).
- T007 (Cargo.toml verify) can run anywhere in Phase 2 — `[P]` with T006 + T008.
- T010 (build.rs error path verify) depends on T009 working (positive case) first.
- **Phase 2 MUST complete before Phase 3** — Phase 3 path rewrites assume MIKEBOM_FIXTURES_DIR is set.
- T011 + T012 (Phase 3 helper updates) — same files / same module, sequential.
- T013 + T014 (Phase 3 mechanical rewrites) — independent file groups, can parallel.
- T015 (deletions) sequential after T013 + T014 (deletion order matters: rewrite first, then delete the source).
- T016 (test verification) gates Phase 4+.
- **Phase 3 MUST complete before Phase 4** — scan-cleanliness verification assumes the move happened.
- T017 → T018 (Phase 4) — sequential scan + spot-check.
- T019 (Phase 5) independent of US1's verification.
- T020 (Phase 6) independent — runs against the populated cache from T009.
- T021 + T022 + T023 (Polish, parallel — different files).
- T024 → T025 → T026 → T027 (Polish verification — sequential).

## Parallel Opportunities

- **Phase 1**: T001 (pre-scan) + T002 (repo creation) parallel.
- **Phase 2**: T006 + T007 + T008 parallel after T005.
- **Phase 3**: T013 + T014 parallel (different file groups in different crates).
- **Polish**: T021 + T022 + T023 parallel (different files).

## Notes

- **`git rm` ordering**: T015 deletes the moved fixture directories. Run AFTER T013 + T014 (path rewrites) so that the deleted-fixture references in test files have already been updated. Otherwise the rewrites would happen against deleted files (uncommitted-edit hell).
- **PR diff target**: ~700+ deletions (44 fixture directories) + ~50 LOC build.rs + ~30 LOC helper + ~76 file rewrites (often single-line) + 1-line `tests/fixtures.rev` + ~30 LOC CI YAML + ~20 LOC research.md updates. Net: large deletion-heavy diff.
- **Suggested MVP scope**: Phases 1+2+3 (the migration + test continuity). Phase 4 (US1 scan-cleanliness verify) and Phase 5/6 (US3/US4 reproducibility/offline checks) ship in the same PR for atomicity but are independently testable. Polish runs last.
- **External-system dependency**: T002 + T003 require shared-system actions (creating + pushing to a new GitHub repo). Pause for user confirmation before executing T002. T003 follows immediately if T002 succeeds.
- **No new Cargo dependencies** at the lockfile level — build.rs shells out to system `git` (Constitution Principle I).
- **Zero golden regenerations** (test-infra refactor; no SBOM-emission code path touched). T024 enforces this.
- **Constitution III alignment**: T010 explicitly verifies the fail-closed behavior on fetch failure.
- **Historical reproducibility caveat**: pre-090 mikebom commits don't have `tests/fixtures.rev`. build.rs falls back via the structured "this commit predates the milestone-090 split; checkout post-090 OR reconstruct fixtures from pre-090 paths" message. Plan-level detail; not blocking.
