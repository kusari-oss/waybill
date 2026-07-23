---

description: "Task list for m217 — skip GOROOT stdlib as a Go main-module candidate (closes waybill#631)"
---

# Tasks: Skip GOROOT stdlib as Go main-module

**Input**: Design documents from `/specs/217-goroot-stdlib-skip/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Test tasks INCLUDED. Every user story is verified by targeted unit + integration tests; the fixture is small enough to author + own alongside the code.

**Organization**: 2 user stories. US1 (P1) is the bug fix — walker filter + fixture + integration test. US2 (P2) is the transparency annotation — reuses US1's threading + adds emission propagation + parity extractors. Both stories share the foundational `ScanArtifacts` field addition (Phase 2).

**Solo-dev sequencing note**: this milestone is small (~50 LOC production + ~200 LOC test + 1 fixture). Strict sequential execution is the simplest path — parallelism overhead exceeds the benefit.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1, US2)
- File paths absolute-from-repo-root

---

## Phase 1: Setup

**Purpose**: Sanity checks + branch verification + reference-code reading.

- [X] T001 Verify branch `217-goroot-stdlib-skip` is checked out and up-to-date with `main` post-#632 merge. Confirm HEAD is the plan-phase commit via `git log -1 --oneline`.
- [X] T002 Verify pre-feature single-SBOM emit path is green: `cargo test -p waybill --test cdx_regression --test spdx_regression --test spdx3_regression`. Should print `11 passed 0 failed` × 3 suites. Confirms SC-004 baseline is currently passing.
- [X] T003 Verify no existing fixture uses `module std` or `module cmd` — pre-implementation grep guarantees the filter won't accidentally regress a real fixture: `grep -rE "^module (std|cmd)$" waybill-cli/tests/fixtures 2>/dev/null | head`. Expects empty output.
- [X] T004 Read `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:2487-2551` (existing `candidate_project_roots` + `should_skip_descent`) — the surface the filter integrates into. Also read `legacy.rs:188-215` (existing `parse_go_mod` + `GoModDocument` shape) — the parser the filter reuses.
- [X] T005 Read `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:1601` (single caller of `candidate_project_roots` in `read()`) — the site where the new return-tuple accumulator gets consumed + threaded outward.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Thread the new `ScanArtifacts.go_toolchains_detected: Option<&'a [PathBuf]>` field through the Go reader → scan_fs → generate pipeline so BOTH stories can rely on it. US1 needs the empty-vec case (default `None`); US2 emits when populated.

**⚠️ CRITICAL**: `cargo check -p waybill --all-targets` MUST pass at Phase 2 completion.

- [X] T006 Add the new `pub go_toolchains_detected: Option<&'a [PathBuf]>` field on `ScanArtifacts` in `waybill-cli/src/generate/mod.rs`. Doc-comment cites waybill#631 + names the m173 `go_cache_warming` shape precedent. Default at every construction site is `None`; update the test-helper construction site (there's one in `mod.rs::tests` or a nearby location — grep for `ScanArtifacts {` occurrences and add `go_toolchains_detected: None,` to each).
- [X] T007 Extend `ScanArtifacts::narrow` in `waybill-cli/src/generate/mod.rs` to copy the new field into narrowed projections (m215 pattern — every borrowed field on the base flows through the same-lifetime narrow helper unchanged).
- [X] T008 Wire the field through the caller. Locate the `ScanArtifacts { ... }` construction site in `waybill-cli/src/cli/scan_cmd.rs` (single site around line 3310+) and add `go_toolchains_detected: go_toolchains_detected.as_deref(),` where `go_toolchains_detected: Option<Vec<PathBuf>>` is a new local var initialized to `None`. The Phase-3 wire-up (T012) will populate this local from the Go reader's return value.

**Checkpoint**: `cargo check -p waybill --all-targets` clean; new field exists at every relevant site; nothing emits it yet (US2 territory).

---

## Phase 3: User Story 1 — CI scan of Go-toolchain image produces clean log + correct SBOM (Priority: P1) 🎯 MVP

**Goal**: Waybill scans any container image with a Go toolchain WITHOUT (a) emitting `pkg:golang/std@*` or `pkg:golang/cmd@*` false-positive main-modules, and (b) flooding stderr with 100+ "use of internal package … not allowed" lines that GitHub Actions' Go problem-matcher converts to `##[error]` annotations. SC-001 + SC-002.

**Independent Test**: On the fixture `waybill-cli/tests/fixtures/goroot_stub/` (mini-GOROOT + companion user project), a `waybill sbom scan --path <fixture> --format cyclonedx-json` invocation asserts (a) zero `pkg:golang/std` or `pkg:golang/cmd` components in the emitted SBOM, (b) zero `use of internal package` lines in the process's stderr, (c) the companion user project's main-module IS emitted (FR-004 non-regression).

### Implementation for User Story 1

- [X] T009 [US1] Add the filter block to `candidate_project_roots` in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` per contracts/goroot-skip.md filter-predicate section + data-model.md E4 pseudo-code. Inside the existing `if path.is_dir() && path.join("go.mod").is_file()` branch: read the go.mod, call `parse_go_mod`, match on `doc.module_path.as_deref()`: `Some("std")` → skip + record `<go.mod>.parent().parent()` in accumulator; `Some("cmd")` → skip + record `<go.mod>.parent().parent().parent()` in accumulator; `_` → existing behavior (push to `out`). Each skip emits a `tracing::debug!` per contracts/goroot-skip.md log-level section. Guard both `.parent()` chains with `Option` — pathological paths that yield `None` still fire the skip but don't push to the accumulator.
- [X] T010 [US1] Change the return type of `candidate_project_roots` in `legacy.rs` from `Vec<PathBuf>` to `(Vec<PathBuf>, Vec<PathBuf>)` — first element unchanged (candidate project roots); second is the toolchain-observation accumulator, sorted lex + deduplicated at function exit. Update the single caller at `legacy.rs:1601` to destructure the tuple.
- [X] T011 [US1] Thread the toolchain-observation vec from `read()` in `legacy.rs` outward: either (a) add a new field to `read()`'s return type (currently `DbScanResult` or similar — locate it), OR (b) accept a `&mut Vec<PathBuf>` parameter passed from the caller. Choice depends on which is least invasive to `read()`'s ~10 caller sites — verify at implementation. Match the precedent set by whichever of m053/m055/m160/m161/m172/m173 does the closest thing.
- [X] T012 [US1] In `waybill-cli/src/cli/scan_cmd.rs`, capture the toolchain-observation vec from the Go reader's output at the same location the m161 `go_workspace_mode` and m173 `go_cache_warming` are captured (search for `let go_workspace_mode = ` — they cluster). Assign to a new `let go_toolchains_detected: Option<Vec<PathBuf>> = ...;` local; feed via `.as_deref()` into the `ScanArtifacts.go_toolchains_detected` field constructed in T008.

### Test tasks for User Story 1

- [X] T013 [P] [US1] Add unit tests in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs::tests` covering the filter predicate:
  - `candidate_project_roots_skips_module_std` — tempdir with `src/go.mod` declaring `module std`; call the walker; assert empty candidates AND toolchain-observation entry present.
  - `candidate_project_roots_skips_module_cmd` — tempdir with `src/cmd/go.mod` declaring `module cmd`; same assertions.
  - `candidate_project_roots_keeps_user_project` — tempdir with `app/go.mod` declaring `module example.com/app`; walker returns 1 candidate; toolchain-observation empty.
  - `candidate_project_roots_dedups_multiple_toolchains` — tempdir with TWO mini-GOROOTs (one at `usr/local/go/src/go.mod`, one at `opt/go/src/go.mod`, both `module std`); walker returns 0 candidates; toolchain-observation contains BOTH roots (sorted lex).
  - `candidate_project_roots_install_path_independence` — same test as above but explicitly verifies that neither `/usr/local/go` nor `/opt/go` is hardcoded (both paths get treated identically because the filter is on `module_path`, not install location).
- [X] T014 [US1] Author fixture at `waybill-cli/tests/fixtures/goroot_stub/` per research R5. Layout:
  - `usr/local/go/VERSION` — literal `go1.26.3\ntime 2026-05-04T20:36:18Z\n`
  - `usr/local/go/src/go.mod` — literal `module std\n\ngo 1.26\n`
  - `usr/local/go/src/cmd/go.mod` — literal `module cmd\n\ngo 1.26\n`
  - `app/go.mod` — literal `module example.com/app\n\ngo 1.22\n`
  - `app/go.sum` — empty file
  - `app/main.go` — literal `package main\n\nfunc main() {}\n`
- [X] T015 [US1] Create new integration test file `waybill-cli/tests/goroot_skip.rs` with scenario `goroot_stdlib_not_emitted_as_main_module` per contracts/goroot-skip.md + quickstart verification-checklist:
  - Uses the T014 fixture.
  - Invokes `waybill sbom scan --path <fixture> --format cyclonedx-json --output <tmp>.cdx.json --offline` via `Command` in an isolated `$HOME` (matches m216's `run_scan` env-isolation pattern).
  - Asserts exit code 0.
  - Asserts stderr contains ZERO lines matching `use of internal package .* not allowed` (SC-002 gate).
  - Asserts SBOM contains ZERO components whose PURL matches `^pkg:golang/(std|cmd)@` (SC-001 gate).
  - Asserts SBOM contains ONE main-module component identifying `example.com/app` (FR-004 non-regression).
- [X] T016 [US1] Extend `waybill-cli/tests/goroot_skip.rs` with scenario `install_path_independence_opt_go` — synthetic tempdir with the Go toolchain at `opt/go/src/go.mod` (NOT `/usr/local/go`) declaring `module std`. Scan it. Assert the filter fires (no `pkg:golang/std` component emitted) — proves FR-005 install-path independence.

**Checkpoint**: `cargo test -p waybill --test goroot_skip` passes green with 2 scenarios. Unit tests in `legacy.rs::tests` pass. Pre-feature regressions (SC-004) still pass (byte-identity guaranteed — no existing fixture matches the filter predicate).

---

## Phase 4: User Story 2 — Transparency annotation (Priority: P2)

**Goal**: When waybill's walker skips a toolchain-internal `go.mod`, the emitted SBOM's document-scope annotations include `waybill:go-toolchain-detected` naming the detected toolchain root path(s). Silent when no toolchain observed (byte-identity for non-Go and Go-project-only scans). SC-005.

**Independent Test**: On the T014 fixture, scan single-SBOM; assert `.metadata.properties[]` contains a `waybill:go-toolchain-detected` entry whose value (JSON-array-in-string) contains the toolchain root path (`usr/local/go`).

### Implementation for User Story 2

- [X] T017 [US2] Add the annotation-emission propagation block in `waybill-cli/src/generate/cyclonedx/metadata.rs` per data-model.md E3 CDX shape. Mirror the m176 `waybill:workspaces-detected` propagation block (search for `waybill:workspaces-detected` in metadata.rs — the pattern is `if let Some(paths) = artifacts.go_toolchains_detected { let value = serde_json::to_string(&paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()).unwrap_or_default(); properties.push(json!({"name": "waybill:go-toolchain-detected", "value": value})); }`). Silent when `None`.
- [X] T018 [US2] Add the same annotation to SPDX 2.3 emitter via the document-level annotation path. Locate the m176 workspaces-detected precedent in `waybill-cli/src/generate/spdx/` (grep for `waybill:workspaces-detected` in that tree) and mirror the same envelope shape (`MikebomAnnotationCommentV1` per contracts/goroot-skip.md).
- [X] T019 [US2] Add the same annotation to SPDX 3 emitter — analogous to T018, targeting the SpdxDocument root element's Annotation list.
- [X] T020 [US2] Add per-format parity extractors (all document-scope):
  - `c136_cdx` in `waybill-cli/src/parity/extractors/cdx.rs` (mirror `c121_cdx` shape — document-scope annotation)
  - `c136_spdx23` in `waybill-cli/src/parity/extractors/spdx2.rs`
  - `c136_spdx3` in `waybill-cli/src/parity/extractors/spdx3.rs`
  - Register in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` with `row_id: "C136", label: "waybill:go-toolchain-detected", directional: SymmetricEqual, order_sensitive: false` — same shape as C121-C134 doc-scope rows. Update the `use super::{cdx,spdx2,spdx3}::{...}` lists.
  - **⚠️ COUPLED WITH T023** (per analyze-phase finding C1): the `parity::extractors::tests::every_catalog_row_has_an_extractor` test verifies BIDIRECTIONAL correspondence between EXTRACTORS entries and the catalog rows in `docs/reference/sbom-format-mapping.md`. T020 without T023 leaves the tree in a red state (`EXTRACTORS entries without catalog rows: ["C136"]`). Commit T023 FIRST (or bundle both in one commit); do NOT commit T020 alone. Same failure mode tripped m216's PR #632 pre-PR gate.

### Test tasks for User Story 2

- [X] T021 [US2] Extend `waybill-cli/tests/goroot_skip.rs` with scenario `go_toolchain_detected_annotation_present` per SC-005. Uses the T014 fixture. Invoke single-SBOM CDX scan. Assert `.metadata.properties[]` contains a `waybill:go-toolchain-detected` entry whose value (parsed as JSON array) contains a string ending with `usr/local/go`. Path may be scan-root-relative (`usr/local/go`) OR absolute depending on how the fixture is invoked; assert on `.ends_with()` for portability.
- [X] T022 [US2] Extend `waybill-cli/tests/goroot_skip.rs` with scenario `annotation_absent_when_no_toolchain` — synthetic tempdir with ONLY a user Go project (no toolchain). Scan it. Assert `.metadata.properties[]` does NOT contain any `waybill:go-toolchain-detected` entry. Verifies the "silence = not observed" contract.

**Checkpoint**: All 4 integration test scenarios in `goroot_skip.rs` pass green (T015 + T016 + T021 + T022). Parity extractors registered → `parity::extractors::tests::every_catalog_row_has_an_extractor` passes.

---

## Phase 5: Polish + docs + PR

**Purpose**: Docs update per Constitution Principle V, pre-PR gate, PR open.

- [X] T023 [P] Add ONE row to `docs/reference/sbom-format-mapping.md` for `waybill:go-toolchain-detected` per research R6. Format matches C121-C134 KEEP-NO-NATIVE rows: annotation name, per-format landing slot, KEEP-NO-NATIVE audit citation naming CDX `metadata.tools[]` / SPDX `creationInfo.creators[]` as REJECTED (producer-scope, not observation-scope). Milestone-217 citation clause. **⚠️ COUPLED WITH T020** (per analyze-phase finding C1): T023 MUST be committed together with T020 (or before T020) to keep the `every_catalog_row_has_an_extractor` bidirectional test green at every commit. Do NOT commit T020 alone.
- [ ] T024 Pre-PR gate per CLAUDE.md: `./scripts/pre-pr.sh` — `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero warnings) + `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`). Watch for the pre-existing podman env-var race (see `reference_podman_test_flake.md` memory); rerun failed lane once if it trips.
- [ ] T025 Verify the m214 CI grep gate stays green: `BADHITS=$(grep -rE '\bmikebom\b' waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml Dockerfile.ebpf-test scripts 2>/dev/null | grep -v '^Binary file' | grep -vE 'mikebom-test-fixtures' || true)`; expects zero output.
- [ ] T026 Push branch `git push origin 217-goroot-stdlib-skip`.
- [ ] T027 Open PR against `main` titled `impl(217): skip GOROOT stdlib as Go main-module candidate (closes #631)`. PR body includes: (a) summary + link to spec/plan + closing issue #631, (b) Test Plan enumerating 4 integration test scenarios + unit tests + pre-PR gate + SC-004 byte-identity regression + m214 grep gate, (c) migration/backward-compat note ("filter fires only on exact-string `module std` / `module cmd`; no fixture/user project affected"), (d) reference to the CI-noise-fix (waybill#631 stderr flood).

### Final gates

- [ ] T028 CI-side verification: all 20 CI checks (linux-x86_64 default + ebpf-tracing, macOS, Windows, Kusari Inspector, 15 rootfs/language scanners) MUST pass. Merge blocked until all green. Watch for the pre-existing podman env-var race documented in `reference_podman_test_flake.md`; rerun the failed CI job once before treating as a real regression.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: no dependencies — starts immediately. T001-T005 all reads/verification, can be done in one pass.
- **Foundational (Phase 2)**: depends on Setup. T006 → T007 → T008 sequential (all touch `mod.rs` + `scan_cmd.rs`).
- **US1 (Phase 3)**: depends on Foundational. T009 (walker filter) → T010 (return type) → T011 (thread through `read()`) → T012 (capture in scan_cmd) sequential. T013 unit tests can run after T009/T010. T014 (fixture) parallelizable with T009-T012. T015-T016 integration tests sequential in the same file after fixture + wire-up exist.
- **US2 (Phase 4)**: depends on Foundational + US1 T011/T012 (needs the field populated to test annotation emission). T017-T020 emission propagation + parity extractors — mostly parallelizable across different files but do them sequentially for review clarity. T021-T022 integration tests after emission wire-up.
- **Polish (Phase 5)**: depends on all preceding phases. T023 (docs) parallelizable. T024-T028 sequential release-prep.

### Cross-story parallelism

- After Phase 2 completes: US1's T009-T012 is the critical path; T014 fixture can be authored in parallel.
- T017 || T018 || T019 (US2 emission propagation across 3 different emitters).
- T023 (docs) can start any time after US2 completes.

### Within each user story

- Foundational field addition (T006-T008) written FIRST.
- Filter + walker changes (T009-T012) sequential within legacy.rs.
- Fixture (T014) parallelizable with wire-up.
- Integration tests (T015+, T021+) after wire-up + fixture exist.

---

## Parallel Example: US2 Emission Phase

```bash
# After T012 (US1 threading) completes, launch T017 + T018 + T019 in parallel:
Task: "Add CDX metadata.properties[] propagation for waybill:go-toolchain-detected"   # T017
Task: "Add SPDX 2.3 document-level Annotation for waybill:go-toolchain-detected"      # T018
Task: "Add SPDX 3 SpdxDocument Annotation for waybill:go-toolchain-detected"          # T019
```

---

## Implementation Strategy

### MVP (US1 P1 only)

1. Complete Phase 1 (Setup).
2. Complete Phase 2 (Foundational — new ScanArtifacts field + narrow-safe threading).
3. Complete Phase 3 (US1 — walker filter + fixture + 2 integration scenarios).
4. **STOP + VALIDATE**: run the CI reproducer (scan any Go-toolchain-carrying image) and confirm zero `##[error]` annotations + zero `pkg:golang/std@*` components. This closes waybill#631 without US2.

### Full delivery (US1 + US2)

5. Complete Phase 4 (US2 — annotation propagation + parity extractors + 2 more integration scenarios).
6. Complete Phase 5 (Polish + PR).

### Solo-dev sequencing (recommended)

T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017 → T018 → T019 → T023 → T020 → T021 → T022 → T024 → T025 → T026 → T027 → T028.

(**T023 pulled forward before T020** per analyze-phase C1: `every_catalog_row_has_an_extractor` is a bidirectional test; committing T020 without T023 leaves the tree red between the two. Same failure that tripped m216 PR #632. Alternative: bundle both in one commit.)

(Strict sequential — this milestone is small enough that parallelism overhead exceeds the benefit.)

---

## Notes

- [P] tasks = different files, no dependencies.
- No golden regeneration is needed. Pre-feature fixtures don't contain `module std` or `module cmd` go.mods (SC-004 gate + T003 pre-implementation grep).
- Fixture authoring (T014) is small (~6 files, <100 lines total). Hand-authored `go.mod` files mimic real Go 1.26 output; the `VERSION` file mirrors a verified local install.
- **Constitution Principle V audit**: the new `waybill:go-toolchain-detected` annotation MUST be documented in `docs/reference/sbom-format-mapping.md` (T023) with a KEEP-NO-NATIVE justification citing that CDX `metadata.tools[]` / SPDX `creationInfo.creators[]` describe SBOM-producer tools, NOT observed-in-rootfs toolchains. Reviewers will reject the PR without this row.
- **Podman test flake caveat** (T024, T028): the `podman_source::discover_storage_root_falls_back_to_default_when_no_config` test occasionally fails on CI due to a pre-existing env-var race. See `reference_podman_test_flake.md` memory for the diagnostic playbook. Rerun the failed CI job once before treating as a real regression.
