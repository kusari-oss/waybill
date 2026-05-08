---
description: "Tasks: Transitive dependency correctness audit per ecosystem"
---

# Tasks: Transitive dependency correctness audit per ecosystem

**Input**: Design documents from `/specs/083-transitive-correctness/`
**Prerequisites**: spec.md ✅, plan.md ✅, research.md ✅, data-model.md ✅, contracts/audit-harness.md ✅, quickstart.md ✅

**Organization**: Phase 1 sets up shared infrastructure. Phase 2 writes the audit harness. Phases 3+ run the audit per ecosystem (one phase per — they're independent). Final phase produces the consolidated findings + filings.

## Path Conventions

Repository-relative paths from `/Users/mlieberman/Projects/mikebom/`:
- Audit harness: `mikebom-cli/tests/transitive_parity_common.rs`
- Per-ecosystem regression tests: `mikebom-cli/tests/transitive_parity_<ecosystem>.rs`
- Vendored fixtures: `mikebom-cli/tests/fixtures/transitive_parity/<ecosystem>/`
- Research findings: `specs/083-transitive-correctness/research.md`

---

## Phase 1: Setup

- [X] T001 Verify trivy 0.69.3 + syft 1.27.0 installed (per research §1 pinned versions). Smoke-run each against any existing fixture to confirm working invocations.
- [X] T002 Create `mikebom-cli/tests/fixtures/transitive_parity/` directory with `.gitkeep` so the per-ecosystem subdirs can land separately.
- [ ] T003 [P] Add `MIKEBOM_REQUIRE_TRANSITIVE_PARITY` env-var documentation to `docs/reference/contributing.md` (or wherever the existing `MIKEBOM_REQUIRE_SPDX3_VALIDATOR` is documented). Mirrors milestone-078's strict-mode pattern.

## Phase 2: Foundational — audit harness

- [X] T004 Create `mikebom-cli/tests/transitive_parity_common.rs` with the data-model.md types: `Edge`, `EdgeDiff`, `AuditRow`, `AuditClassification`, `TransitiveParityFixture`. Include `Edge::is_unanimous()` + `Edge::requires_tiebreaker()` per VR-083-002.
- [X] T005 Implement `run_mikebom(fixture_path) -> Vec<Edge>` in `transitive_parity_common.rs` per contracts/audit-harness.md §"run_mikebom". Use `--offline --format spdx-3-json --output -` then extract `software_dependsOn[]` per `software_Package` element.
- [X] T006 Implement `run_trivy(fixture_path) -> Vec<Edge>` per contracts §"run_trivy". Shell out to `trivy fs --format spdx-json --output - <fixture>`, parse SPDX 2.3, filter `relationships[]` to `relationshipType: "DEPENDS_ON"`, resolve SPDX-IDs to PURLs.
- [X] T007 Implement `run_syft(fixture_path) -> Vec<Edge>` per contracts §"run_syft". Same shape as run_trivy but with `syft <fixture> -o spdx-json`.
- [ ] T008 Implement `run_source_format_direct(fixture_path, ecosystem) -> Vec<Edge>` tiebreaker per contracts §"run_source_format_direct". Per-ecosystem dispatch table.
- [X] T009 Implement `compute_edge_diff(mikebom, trivy, syft) -> EdgeDiff` set-theoretic comparison per data-model.md.
- [X] T010 Implement graceful-skip helper `assert_graceful_skip()` per research §5: skip when external tools missing AND `MIKEBOM_REQUIRE_TRANSITIVE_PARITY` env var unset; fail loudly when set.
- [X] T011 Implement PURL normalization helpers per VR-083-004: lowercase package types, alphabetical-sort `Vec<Edge>`, strip qualifiers when comparing.

## Phase 3: US1 — per-ecosystem audit (P1)

Each ecosystem is an independent unit of work. Per FR-002, each fixture must have ≥50 components AND ≥100 edges. Per research §2 the candidates are pre-shortlisted.

### Phase 3a: Cargo

- [X] T012 [US1] Pick + extract Cargo fixture per quickstart.md Recipe 2. Candidate from research §2: `clap-rs/clap` workspace. Vendor `Cargo.toml` + `Cargo.lock` to `mikebom-cli/tests/fixtures/transitive_parity/cargo/`.
- [X] T013 [US1] Create `mikebom-cli/tests/transitive_parity_cargo.rs` with `transitive_edges_match_baseline` + `cross_tool_parity_check` + `graceful_skip_when_tools_absent` tests per contracts §"Test contract".
- [X] T014 [US1] Run the audit, populate `EXPECTED_EDGE_COUNT` + `EXPECTED_REPRESENTATIVE_EDGES` from real output, write the per-ecosystem audit row to `research.md` §"Ecosystem: cargo".

### Phase 3b: npm

- [X] T015 [US1] Pick + extract npm fixture (candidate: `expressjs/express`). Vendor `package.json` + `package-lock.json`.
- [X] T016 [US1] Create `mikebom-cli/tests/transitive_parity_npm.rs`.
- [X] T017 [US1] Run audit + populate baseline + research.md row.

### Phase 3c: Maven

- [X] T018 [US1] Pick + extract Maven fixture (candidate: `apache/commons-lang`). Vendor `pom.xml` + parent POMs needed for `<parent>` resolution.
- [X] T019 [US1] Create `mikebom-cli/tests/transitive_parity_maven.rs`.
- [X] T020 [US1] Run audit + populate baseline + research.md row.

### Phase 3d: pip-poetry

- [X] T021 [US1] Pick + extract poetry fixture (candidate: `pypa/poetry` self-hosting). Vendor `pyproject.toml` + `poetry.lock`.
- [X] T022 [US1] Create `mikebom-cli/tests/transitive_parity_pip_poetry.rs`.
- [X] T023 [US1] Run audit + populate baseline + research.md row.

### Phase 3e: pip-pipfile

- [ ] T024 [US1] Pick + extract Pipfile fixture. Vendor `Pipfile` + `Pipfile.lock`.
- [ ] T025 [US1] Create `mikebom-cli/tests/transitive_parity_pip_pipfile.rs`.
- [ ] T026 [US1] Run audit + populate baseline + research.md row.

### Phase 3f: pip-plain

- [X] T027 [US1] Pick + extract plain requirements.txt fixture (smaller — degenerate case per FR-008 — likely zero transitive edges).
- [X] T028 [US1] Create `mikebom-cli/tests/transitive_parity_pip_plain.rs`.
- [X] T029 [US1] Run audit + document the FR-008 upstream-limitation in the research.md row + populate baseline.

### Phase 3g: gem

- [ ] T030 [US1] Pick + extract gem fixture (candidate: `rubocop/rubocop`). Vendor `Gemfile` + `Gemfile.lock`.
- [ ] T031 [US1] Create `mikebom-cli/tests/transitive_parity_gem.rs`.
- [ ] T032 [US1] Run audit + populate baseline + research.md row.

### Phase 3h: Go

- [X] T033 [US1] Pick + extract Go fixture (candidate: `kubernetes/cri-tools`). Vendor `go.mod` + `go.sum`.
- [X] T034 [US1] Create `mikebom-cli/tests/transitive_parity_go.rs`.
- [X] T035 [US1] Run audit + populate baseline + research.md row.

### Phase 3i: dpkg / rpm / apk (Linux-only)

- [ ] T036 [US1] Extract dpkg fixture from a Debian 12 base container — `/var/lib/dpkg/status` snapshot.
- [ ] T037 [US1] Create `mikebom-cli/tests/transitive_parity_dpkg.rs` with macOS-skip (per research §5).
- [ ] T038 [US1] Same for rpm — Fedora 39 base container `/var/lib/rpm/` extract.
- [ ] T039 [US1] Same for apk — Alpine 3.20 base container `/lib/apk/db/installed` extract.
- [ ] T040 [US1] Populate research.md rows for dpkg/rpm/apk with macOS-CI-skip caveat documented per FR-009.

## Phase 4: US2 — per-ecosystem regression tests (P1)

Already satisfied by the test files created in Phase 3 — each `transitive_parity_<ecosystem>.rs` IS the regression test pinning the audit's findings.

- [ ] T041 [US2] Verify all 11 per-ecosystem regression tests pass without `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1` set (they should graceful-skip when tools missing on macOS-only fixtures).
- [ ] T042 [US2] Run the regression tests with `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1` to confirm strict-mode behavior. dpkg/rpm/apk skip on macOS unconditionally; others should pass.

## Phase 5: US3 — indirect-vs-direct decisions (P2)

Per research §6 + FR-004. Decisions are decision-only; implementation work for any deferred case is out of scope per FR-010.

- [ ] T043 [US3] Decide Go indirect: implement (file follow-up) | document-as-divergence | defer. Per research §6 the recommendation is **defer** with a follow-up issue for re-evaluation.
- [ ] T044 [US3] Verify cargo + npm indirect-vs-direct already covered by milestone-052 lifecycle scope work — no new decision needed.
- [ ] T045 [US3] Document each per-ecosystem decision in `research.md` §6.

## Phase 6: US4 — follow-up issues for surfaced gaps (P2)

Per FR-005 + quickstart Recipe 5. Each `gap surfaced` audit row produces one filed issue.

- [ ] T046 [US4] After Phases 3a–3i complete, enumerate audit rows with classification = `gap surfaced`. Expected: 0–3 cases (most ecosystems should match expected; gaps are the milestone's primary deliverable when they exist).
- [ ] T047 [US4] For each gap-surfaced row, file a GitHub issue per quickstart.md Recipe 5 template — fixture used, per-tool counts, specific missing/extra edges, suggested fix shape, scope estimate.
- [ ] T048 [US4] Cross-reference the filed issues from `research.md`'s per-ecosystem rows.

## Phase 7: CI integration

- [ ] T049 Add trivy + syft install steps to `.github/workflows/ci.yml` linux-x86_64 job per contracts/audit-harness.md §"CI workflow contract".
- [ ] T050 Set `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1` in the CI lane so the strict-mode behavior runs CI-side.
- [ ] T051 macOS lane skips the new tests automatically (graceful-skip pattern + the Linux-only OS-package tests).

## Phase 8: Polish

- [ ] T052 Pre-PR gate: `./scripts/pre-pr.sh` clean — clippy zero warnings, every test suite `0 failed`. SPDX/CDX goldens stay byte-identical (FR-011 / VR-083-005).
- [ ] T053 Update CLAUDE.md "Recent Changes" if the speckit infrastructure didn't auto-update it.
- [ ] T054 Final research.md pass — confirm all 11 audit rows present, all classifications recorded, all follow-up dispositions documented.

---

## Dependencies & Execution Order

- Phase 1 → Phase 2 → Phase 3 (any order within Phase 3 — ecosystems are independent)
- Phase 3 → Phase 4 (regression tests reuse the audit findings)
- Phase 3 → Phase 5 (decisions need audit findings)
- Phase 3 + Phase 5 → Phase 6 (follow-up issues need both gap data + decisions)
- Phase 6 → Phase 7 → Phase 8

## Parallel opportunities

- T003 [P] in Phase 1 — different file from T001/T002.
- All Phase 3 sub-phases (3a–3i) are independent — different fixture dirs, different test files. Could be split across multiple contributors or done in any order.
- T043, T044 in Phase 5 — different docs.

## Notes

- Audit-only per FR-010: no per-ecosystem reader fixes ship in this milestone.
- Goldens stay byte-identical (FR-011): no regen needed in pre-PR.
- The 11 per-ecosystem fixture files (~10–50 KB each, ~500 KB total per spec) are vendored not generated.
