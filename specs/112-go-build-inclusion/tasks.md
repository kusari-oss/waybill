# Tasks: Go Build-Inclusion Clarity

**Input**: Design documents from `/specs/112-go-build-inclusion/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included — the spec's success criteria (SC-001..SC-005) are test-shaped (golden byte-identity, degrade matrix, parity), matching the repo's TDD-leaning golden discipline.

**Organization**: Grouped by user story. US1 (unknown markers) is the MVP; US2 (toolchain classification) and US3 (degrade/compat hardening) layer on top.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [X] T001 Sync with PR #332: verify `mikebom:lifecycle-scope-derivation` tagging (test-only-closure) exists in `mikebom-cli/src/scan_fs/package_db/mod.rs` on this branch; if PR #332 (`fix/go-test-closure-propagation`) is not yet merged to main, rebase `112-go-build-inclusion` onto that branch and note the ordering in the eventual PR description

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the typed status and its format emission — required by every story.

- [X] T002 Add `BuildInclusion` enum (`Unknown` | `NotNeeded`, serde kebab-case per data-model.md) and `build_inclusion: Option<BuildInclusion>` field (default `None`) to the component model in `mikebom-common/src/resolution.rs`, alongside `lifecycle_scope`; include unit tests for serde round-trip in the same file's test module
- [X] T003 Thread `build_inclusion` from `PackageDbEntry` through the PackageDbEntry → ResolvedComponent mapping in `mikebom-cli/src/scan_fs/mod.rs` (same flow path as `lifecycle_scope`), defaulting `None` everywhere so all existing output is byte-identical
- [X] T004 Emit `mikebom:build-inclusion` CDX component property from the typed field in `mikebom-cli/src/generate/cyclonedx/builder.rs` (near the extra_annotations loop at ~line 928): `unknown`/`not-needed` string values per contracts/annotations.md; NO scope change in this task
- [X] T005 [P] Emit `mikebom:build-inclusion` as SPDX 2.3 Package annotation in `mikebom-cli/src/generate/spdx/annotations.rs` (existing annotation-bag path)
- [X] T006 [P] Emit `mikebom:build-inclusion` as SPDX 3 element annotation in `mikebom-cli/src/generate/spdx/v3_annotations.rs` (existing annotation-bag path)

**Checkpoint**: workspace compiles, full test suite green, zero golden drift (field is `None` everywhere).

---

## Phase 3: User Story 1 — Consumer-visible "build inclusion unknown" signal (P1) 🎯 MVP

**Goal**: fallback-discovered, unconfirmed Go components carry `mikebom:build-inclusion: unknown` in all three formats; everything else untouched.

**Independent Test**: offline hermetic Go fixture scan → every go-sum-fallback / flat-attached component carries the marker in CDX, SPDX 2.3, SPDX 3; confirmed components don't; component count unchanged.

- [X] T007 [US1] Unit tests for the unknown-marker pass in `mikebom-cli/src/scan_fs/package_db/mod.rs` tests module (reuse `make_go_entry` helpers): entry with `mikebom:resolver-step: go-sum-fallback` → `Unknown`; entry with `mikebom:orphan-reason: flat-attached-fallback` → `Unknown`; BuildInfo-confirmed entry (binary present, no `mikebom:not-linked`) → exempt (FR-010); `mikebom:component-role: main-module` → exempt; graph-resolved entry (no fallback annotations) → exempt; already test-scoped entry → exempt (mutual exclusion per data-model.md)
- [X] T008 [US1] Implement `apply_go_build_inclusion_unknown_markers(entries: &mut [PackageDbEntry])` in `mikebom-cli/src/scan_fs/package_db/mod.rs` per data-model.md rules and wire it into `read_all()` AFTER `apply_go_linked_filter` (~line 546) and `apply_go_production_set_filter` (~line 554) — it must run last; make T007 pass
- [X] T009 [US1] Integration test in new file `mikebom-cli/tests/go_build_inclusion.rs`: hermetic Go fixture (go.mod + go.sum with sum-only modules, same shape as `go_transitive_edges.rs` fixtures) scanned with `--offline`; assert the marker on fallback components and its absence on resolved ones, in all three output formats (CDX property, SPDX 2.3 annotation, SPDX 3 annotation); assert unknown-marked CDX components carry NO `scope` field (FR-002); assert component count unchanged vs a pre-feature expectation
- [X] T010 [US1] Regenerate Go goldens (`MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression`, `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --test spdx_regression`) in `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/golang.*`; verify the diff contains ONLY the new `mikebom:build-inclusion` entries and that non-Go goldens have zero drift (SC-004)
- [X] T011 [US1] Add the `mikebom:build-inclusion` row to `docs/reference/sbom-format-mapping.md` per contracts/annotations.md (including the SPDX parity-bridge justification naming the missing native excluded-scope field) and confirm `mikebom-cli/src/parity/catalog.rs` parses it (run `cargo test -p mikebom --test parity_cmd`); extend cross-format assertions in `mikebom-cli/tests/transitive_parity_go.rs`

**Checkpoint**: US1 shippable — Part B works on every host, no toolchain involved.

---

## Phase 4: User Story 2 — Package-level build-graph classification (P2)

**Goal**: with a `go` toolchain on PATH, `go mod why -m -vendor` classifies modules: not-needed → kept with CDX `scope: "excluded"` + derivation, test-only → `LifecycleScope::Test` + derivation, prod → untouched; classified modules never carry the unknown marker.

**Independent Test**: stub-toolchain integration scan produces all three verdicts end-to-end; build-list modules never excluded.

- [X] T012 [US2] Add `--no-go-mod-why` boolean flag + `MIKEBOM_NO_GO_MOD_WHY` env-var bridge (same dual pattern as `MIKEBOM_OFFLINE`, main.rs:207–211) in `mikebom-cli/src/main.rs` per contracts/cli-flags.md, plumbed toward the package_db read path
- [X] T013 [P] [US2] Create `mikebom-cli/src/scan_fs/package_db/golang/mod_why.rs`: subprocess runner per contracts/go-toolchain-invocation.md — chunks of 20 module paths, shared 60s budget (`budget − elapsed` per chunk), spawn-thread + `mpsc::recv_timeout` pattern copied from `golang/go_mod_graph.rs:81–158`, offline env pinning (`GOPROXY=off`, `GOFLAGS=-mod=mod`, `GOTOOLCHAIN=local`), per-main-module `go list all` reliability preflight (gates all chunks — `go mod why` silently reports false not-needed when resolution fails; preflight failure → skip with NO verdicts accepted), failure classes (`no-toolchain`/`disabled`/`subprocess-error`/`budget-exhausted`/`unresolvable-packages`) that never error the scan, `MIKEBOM_GO_MOD_WHY_BUDGET_MS` test-only budget override
- [X] T014 [P] [US2] In `mikebom-cli/src/scan_fs/package_db/golang/mod_why.rs`: output parser → `GoModWhyVerdict` (`ProdNeeded`/`TestOnly`/`NotNeeded`/`Unresolved`) with unit tests using the canned section formats from the contract (`# module` headers, not-needed via the `(main module does not need` PREFIX — matches both `does not need module X` and the `-vendor` phrasing `does not need to vendor module X`, `.test`-suffix chain nodes, empty/garbled sections)
- [X] T015 [US2] Implement `apply_go_mod_why_classification(...)` in `mikebom-cli/src/scan_fs/package_db/mod.rs`, wired into `read_all()` BETWEEN the existing filters and the T008 unknown pass: per-main-module invocation (multi-module: needed-by-ANY wins), match verdicts to entries by module path, apply data-model.md precedence (BuildInfo-confirmed exempt; never downgrade existing test tags; main-module exempt; `TestOnly` → `LifecycleScope::Test` + `mikebom:lifecycle-scope-derivation: go-mod-why`; `NotNeeded` → `BuildInclusion::NotNeeded` + `mikebom:build-inclusion-derivation: go-mod-why`); plumb workspace root + offline + disable flag parameters; unit tests with injected verdict maps
- [X] T016 [US2] In `mikebom-cli/src/generate/cyclonedx/builder.rs` (~line 599): emit `scope: "excluded"` unconditionally for `BuildInclusion::NotNeeded` components (independent of the include-dev gate) plus the derivation property; unit/golden assertions that NotNeeded components are never dropped by scope filtering (clarification 2026-06-11)
- [X] T017 [US2] Set `MIKEBOM_NO_GO_MOD_WHY=1` suite-wide in the shared integration-test env helper (`mikebom-cli/tests/common/` — same place `apply_fake_home_env` lives) so all existing goldens/tests are host-toolchain-independent; verify full suite green on a host WITH `go` installed
- [X] T018 [US2] Stub-toolchain integration tests in `mikebom-cli/tests/go_build_inclusion.rs` (`#[cfg(unix)]`): temp-dir `go` shell script (quickstart.md pattern — branches on subcommand: `list` → exit 0 preflight pass, `mod why` → canned verdicts) prepended to PATH; assert end-to-end: not-needed component kept with CDX scope `excluded` + both properties, test-chain component test-scoped with `go-mod-why` derivation, prod component byte-unchanged, and NO component carries the unknown marker after classification (SC-002 shape); include a two-main-module fixture (two go.mod dirs) asserting a module needed by only ONE main module is NOT excluded (needed-by-ANY, spec edge case)
- [X] T019 [US2] Add `mikebom:build-inclusion-derivation` row and amend the `mikebom:lifecycle-scope-derivation` value enum (+`go-mod-why`) in `docs/reference/sbom-format-mapping.md`; extend `mikebom-cli/tests/transitive_parity_go.rs` cross-format assertions
- [X] T020 [US2] Emit the FR-013 observability lines in `mikebom-cli/src/scan_fs/package_db/golang/mod_why.rs` + caller: info summary `go-mod-why classification: analyzed=.. prod=.. test=.. not_needed=.. unresolved=.. unknown_marked=.. skipped=.. elapsed_ms=..` and per-degrade warn lines per contracts/go-toolchain-invocation.md

**Checkpoint**: US2 shippable — cyclonedx-gomod-parity classification with the flag-off path still byte-stable.

---

## Phase 5: User Story 3 — Graceful degrade and pre-feature compatibility (P3)

**Goal**: every failure mode degrades to Part B with logs; scans never fail; byte-identity envelope holds.

**Independent Test**: degrade matrix (no toolchain / exit-1 / hang / partial output / offline) all exit 0 with warns; full golden suite green.

- [X] T021 [US3] Degrade-matrix integration tests in `mikebom-cli/tests/go_build_inclusion.rs` (`#[cfg(unix)]` stub variants): (a) PATH with no `go` → skip reason `no-toolchain`, Part B markers applied; (b) stub exits 1 → `subprocess-error`; (c) stub `sleep 120` → `budget-exhausted`, shortened via `MIKEBOM_GO_MOD_WHY_BUDGET_MS` (defined in contracts/go-toolchain-invocation.md) so the test stays fast; (d) stub emitting partial output → classified modules keep verdicts, rest fall to unknown; (e) stub whose `list` subcommand exits 1 (preflight failure) → skip reason `unresolvable-packages`, ZERO verdicts accepted, all fallback modules carry unknown markers (silent-false-negative guard); ALL cases: scan exit status 0, valid SBOM (SC-003)
- [X] T022 [US3] Offline env-pinning test in `mikebom-cli/tests/go_build_inclusion.rs`: stub `go` script dumps its environment to a temp file; scan with `--offline`; assert child saw `GOPROXY=off`, `GOFLAGS=-mod=mod`, `GOTOOLCHAIN=local` (FR-012)
- [X] T023 [US3] Byte-identity sweep: run the FULL workspace test suite (`cargo +stable test --workspace`) and confirm zero drift outside the T010 Go-golden regeneration; add an explicit regression assertion to `mikebom-cli/tests/go_build_inclusion.rs` that a fixture WITHOUT fallback-discovered modules scanned with `MIKEBOM_NO_GO_MOD_WHY=1` is byte-identical to its pre-feature golden (FR-008/SC-004)
- [X] T024 [US3] Env-gated real-toolchain e2e test (`MIKEBOM_GO_TOOLCHAIN_E2E=1`, skip-by-default like the docker-daemon test) in `mikebom-cli/tests/go_build_inclusion.rs`: scan a small real Go module with the host toolchain; assert at least one not-needed/test verdict lands and the scan exits 0

**Checkpoint**: all three stories complete; operational envelope proven.

---

## Phase 6: Polish & Cross-Cutting

- [X] T025 [P] Document `--no-go-mod-why`, the degrade matrix, and the new annotations in the user-facing docs (`docs/` — wherever existing scan-flag reference lives; check `docs/reference/`) and the CLI `--help` text review
- [X] T026 [P] Manual anchor validation per quickstart.md against `~/Projects/kusari-cli`: SC-001 (unknown markers, analysis disabled), SC-002 (zero unknown + no build-list exclusions vs `cyclonedx-gomod mod -json`), record counts in the PR description
- [X] T027 Run the mandatory pre-PR gate `./scripts/pre-pr.sh` (host with broken docker DNS: `MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh`) — BOTH clippy and full workspace tests must pass clean before the PR opens

---

## Dependencies

```text
T001 (PR #332 sync)
  └─► Phase 2: T002 ─► T003 ─► T004, T005[P], T006[P]
        └─► Phase 3 (US1): T007 ─► T008 ─► T009 ─► T010 ─► T011
              └─► Phase 4 (US2): T012, T013[P], T014[P] ─► T015 ─► T016 ─► T017 ─► T018 ─► T019, T020
                    └─► Phase 5 (US3): T021, T022 ─► T023 ─► T024
                          └─► Phase 6: T025[P], T026[P] ─► T027
```

- US1 depends only on Foundational (independently shippable MVP).
- US2 depends on US1's unknown pass (T008) for the marker-clearing assertion, and on Foundational.
- US3 depends on US2's runner (T013) for the degrade matrix.

## Parallel Execution Examples

- After T004: T005 + T006 (different emitter files).
- Start of US2: T013 + T014 (same new file but separable: runner vs parser — sequence if one agent) alongside T012 (main.rs).
- Polish: T025 + T026 in parallel before T027.

## Implementation Strategy

**MVP = Phase 1 + 2 + 3 (US1)**: consumer-visible unknown markers, zero
toolchain dependence — shippable alone as one PR if review size matters.
Then US2 (classification) and US3 (hardening) as the follow-up increment.
Given the repo's slice-per-PR convention (milestone 110 precedent), a
two-PR split (US1 | US2+US3) is the suggested delivery shape; a single
PR is acceptable if the diff stays reviewable.
