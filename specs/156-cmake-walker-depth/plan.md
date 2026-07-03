# Implementation Plan: CMake walker depth extension

**Branch**: `156-cmake-walker-depth` | **Date**: 2026-07-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/156-cmake-walker-depth/spec.md`

## Summary

Extend `discover_cmake_files` at `mikebom-cli/src/scan_fs/package_db/cmake.rs:195` from depth-1 iteration to recursive descent under `<scan_root>/cmake/` and `<scan_root>/Modules/`. `<scan_root>/third_party/` stays depth-1 by default; a new opt-in flag `--cmake-third-party-recursive` (env var `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1`) extends recursion to `third_party/` too. Milestone-155's `find_package` / `pkg_check_modules` parse + emit pipeline runs unchanged against the longer discovered-files list.

**Approach**: reuse the milestone-054 `safe_walk` helper (`mikebom-cli/src/scan_fs/walk.rs:174`) for recursive descent — its `WalkConfig` shape delivers symlink-cycle protection, rootfs sandbox enforcement, milestone-113 `--exclude-path` integration, and deterministic skip logging out of the box. Zero new Cargo dependencies. Zero emitter changes. `discover_cmake_files` gains two parameters (`include_third_party_recursive: bool` — read via env var inside `cmake::read`; and `exclude_set: &ExclusionSet` — threaded from the two direct callers at `package_db/mod.rs:1533` + `binary/mod.rs:198`).

This milestone directly closes the debt from milestone-155's F1 remediation: Kamailio's identified-component count goes from **1** (post-155, walker-scope-honest floor) to **≥10** (all `find_package` calls in `cmake/defs.cmake` + `cmake/modules/Find*.cmake` reached).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–155; no nightly required).
**Primary Dependencies**: Existing only — the milestone-054 `safe_walk` helper (in-crate, at `mikebom-cli/src/scan_fs/walk.rs`), `tracing` (skip-decision debug logs already emitted by `safe_walk`), `anyhow`/`thiserror` (error propagation), `clap` (the new arg field via `#[arg(long)]` derive). Milestone-113 `ExclusionSet` (in-crate). No new Cargo crates.
**Storage**: N/A — visited-set is per-`safe_walk`-invocation `HashSet<PathBuf>` in-memory state; cleared at return. Mirrors every milestone since 002.
**Testing**: `cargo +stable test --workspace --no-fail-fast` per the mandatory pre-PR gate + the milestone-155 fix memory (bail-on-first-failure is real). Inline `#[cfg(test)] mod tests` in cmake.rs for 6 unit tests + 5 new `mikebom-cli/tests/cmake_walker_depth_*.rs` integration test binaries.
**Target Platform**: Cross-platform. `safe_walk` is std-only (`std::fs::read_dir` + `std::fs::canonicalize`); works on Linux + macOS + Windows.
**Project Type**: cli (mikebom is a single-binary CLI in the `mikebom-cli` crate of the three-crate workspace).
**Performance Goals**: Kamailio scan (~20 `.cmake` files, depth ≤3) completes in <200 ms — same order as milestone 155's Kamailio scan (measured 2026-07-02: ~120 ms). safe_walk's canonicalize call per descended directory is the dominant cost; realistic projects have <100 directories to descend.
**Constraints**: No new Cargo deps (FR-016). No wire-format changes (FR-015 + SC-010 — no new annotation keys, no catalog row changes, no emitter changes). No changes to any other reader (FR-013). No changes to milestone-155's parse or emit logic (FR-012). No changes to the milestone-133 file-tier walker (FR-014). No changes to `resolve::deduplicator`. Byte-identity guaranteed for depth-1-only fixtures (SC-002).
**Scale/Scope**: Single-file primary diff to `mikebom-cli/src/scan_fs/package_db/cmake.rs` (~+50 LOC net; existing 803→~850 LOC excluding tests). Two single-line call-site updates. One new CLI arg field. One env-var propagation block (mirrors milestone-102 `MIKEBOM_INCLUDE_VENDORED`). 5 new integration test files + 5 new fixture directories. CHANGELOG entry. CLAUDE.md auto-update.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Reviewed against `.specify/memory/constitution.md` v1.5.0 (ratified 2026-04-15, last amended 2026-06-20).

| Principle | Status | Notes |
|-----------|--------|-------|
| I — Pure Rust, Zero C | ✅ PASS | Zero new deps. `safe_walk` is std-only. No FFI, no C toolchain. |
| II — eBPF-Only Observation | N/A | Scanner-tier work (source-tree readers); Principle II governs the `mikebom trace` command's dependency-discovery path, not `mikebom sbom scan`. Consistent with every scanner-tier milestone since 002. |
| III — Fail Closed | ✅ PASS | `safe_walk` is tolerant of unreadable directories (silent early-return per `read_dir().ok()`), matching the milestone-102 fail-open-for-unreadable-files behavior. If ALL scans fail, the SBOM contains fewer components — no false-success signal. |
| IV — Type-Driven Correctness | ✅ PASS | `Purl::new` validates every emitted PURL (unchanged from milestone 155). No `.unwrap()` in production. Test code follows the `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention. |
| V — Specification Compliance | ✅ PASS (audit trivial) | See §V audit below. |
| VI — Three-Crate Architecture | ✅ PASS | Zero new crates. All work lives inside `mikebom-cli`. |
| VII — Test Isolation | ✅ PASS | All new tests are pure-logic unit tests + `mikebom-cli/tests/*` integration tests. Run under `cargo test --workspace` without root or `CAP_BPF`. |
| VIII — Completeness | ✅ PASS | Directly improves Completeness — Kamailio 1 → ≥10 identified components. The milestone-133 file-tier walker's orphan behavior is unchanged; newly-emitting files stop being classified as orphans (natural signal-density improvement). |
| IX — Accuracy | ✅ PASS | No new PURL emission logic (milestone-155 pipeline unchanged). `safe_walk`'s rootfs sandbox refuses out-of-scan-root symlink targets (prevents leaked-host-content emissions). |
| X — Transparency | ✅ PASS | `safe_walk` emits `tracing::debug!` for every skip decision (cycle avoidance, exclude-path match, rootfs sandbox refusal). Operators can inspect why any file wasn't walked. |
| XI — Enrichment | N/A | This milestone doesn't perform enrichment. |
| XII — External Data Source Enrichment | N/A | No external sources introduced. Parsing is local to the scanned CMake files. |

### Principle V audit (standards-native fields first)

**No new `mikebom:*` annotation keys introduced** (per FR-015). The two existing keys milestone 156's emissions carry (`mikebom:source-mechanism`, `mikebom:cmake-find-package-name`) were fully audited and documented during milestone 155 (see `docs/reference/sbom-format-mapping.md` rows C55 + C103). Milestone 156's scope is walker discovery only; the emission wire format is byte-identical to milestone 155's post-emission shape.

**No new CDX properties, SPDX 2.3 annotations, or SPDX 3 annotation elements**. No new PURL types.

**Audit result**: trivially satisfied — this milestone introduces no new observable-in-SBOM data.

### Strict Boundaries audit

| Boundary | Status |
|----------|--------|
| §1 — No lockfile-based dependency discovery | ✅ PASS — CMake `find_package` declarations are manifest-declared package intents. Consistent with milestone 155's scanner-tier exemption. |
| §2 — No MITM proxy | ✅ PASS — no network activity. |
| §3 — No C code | ✅ PASS. |
| §4 — No `.unwrap()` in production | ✅ PASS — `safe_walk` uses `.unwrap_or_else(...)` on canonicalize failures; milestone-156 code uses `if let Ok(...)` patterns. Test-only `.unwrap()` guarded per convention. |
| §5 — No file-tier duplicates in default mode | ✅ PASS — no touch to file-tier walker. Newly-emitting `.cmake` files at depth-2+ are package-tier emissions; per milestone 133's dedup they naturally suppress overlapping file-tier orphans via `evidence.occurrences[].location` coverage. |

**Result**: ✅ Constitution Check gates pass. No violations to justify. `Complexity Tracking` section empty.

## Project Structure

### Documentation (this feature)

```text
specs/156-cmake-walker-depth/
├── plan.md                    # This file (/speckit.plan output)
├── spec.md                    # Feature spec (/speckit.specify + /speckit.clarify)
├── research.md                # Phase 0 output (/speckit.plan)
├── data-model.md              # Phase 1 output (/speckit.plan)
├── quickstart.md              # Phase 1 output (/speckit.plan)
├── contracts/                 # Phase 1 output (/speckit.plan)
│   └── cli-flag.md            # `--cmake-third-party-recursive` contract
├── checklists/
│   └── requirements.md        # /speckit.specify output
└── tasks.md                   # Phase 2 output (/speckit.tasks — NOT this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── cli/
│   │   └── scan_cmd.rs        # ADD: pub cmake_third_party_recursive: bool
│   │                          # ADD: MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1 env-var propagation
│   ├── scan_fs/
│   │   ├── binary/
│   │   │   └── mod.rs         # UPDATE: cmake::read call site (1-line signature update)
│   │   └── package_db/
│   │       ├── cmake.rs       # PRIMARY DELIVERABLE
│   │       │                  #   - Extend read() signature to accept &ExclusionSet
│   │       │                  #   - Extend discover_cmake_files() signature +
│   │       │                  #     switch cmake/ + Modules/ to recursive descent
│   │       │                  #   - Add collect_cmake_files_recursive() using safe_walk
│   │       │                  #   - Add collect_cmake_files_depth1() (extracted from old
│   │       │                  #     read_dir body; preserved for third_party/ default)
│   │       │                  #   - Add is_cmake_file() extraction helper
│   │       │                  #   - Read MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE env var
│   │       │                  #   - 6 new unit tests in mod tests block
│   │       └── mod.rs         # UPDATE: cmake::read call site (1-line signature update)
└── tests/
    ├── cmake_walker_depth_symlink_cycle.rs             # NEW — SC-003
    ├── cmake_walker_depth_deep_emission.rs             # NEW — SC-004
    ├── cmake_walker_depth_cross_depth_version.rs       # NEW — SC-005
    ├── cmake_walker_depth_exclude_path.rs              # NEW — SC-006
    ├── cmake_walker_depth_third_party_opt_in.rs        # NEW — SC-011
    └── fixtures/
        └── cmake-walker-depth/
            ├── symlink-cycle/         # SC-003 (contains cmake/loop -> ../cmake/)
            ├── depth3-emission/       # SC-004 (find_package at depth-3)
            ├── cross-depth-version/   # SC-005 (1.1.0 at depth-1, 3.0 at depth-3)
            ├── exclude-path-integration/  # SC-006 (2 depths; exclude one via --exclude-path)
            └── third-party-opt-in/    # SC-011 (find_package inside third_party at depth-3)
```

**Files intentionally NOT touched**:

- `mikebom-cli/src/generate/cyclonedx/**` — no emitter changes.
- `mikebom-cli/src/generate/spdx/**` — no emitter changes.
- `mikebom-cli/src/parity/extractors/**` — no new parity extractors (no new annotation keys).
- `mikebom-cli/src/scan_fs/package_db/*.rs` (except cmake.rs + mod.rs call site) — other readers untouched.
- `mikebom-cli/src/scan_fs/mod.rs` — no changes.
- `mikebom-common/**`, `mikebom-ebpf/**` — other crates untouched.
- `docs/reference/sbom-format-mapping.md` — no catalog row changes (milestone-155's C55 + C103 rows cover everything milestone 156 emits).
- `mikebom-cli/tests/fixtures/golden/**` — no golden regeneration (per SC-002 byte-identity guard).

**Files updated at plan phase**:
- `CLAUDE.md` — appended by `.specify/scripts/bash/update-agent-context.sh claude` per Phase 1 §3.
- `CHANGELOG.md` — updated during implementation phase per SC-009.

**Structure Decision**: single-crate mikebom-cli additive-only diff. The milestone lives entirely inside the existing three-crate architecture; no new crates, no new modules, no cross-crate refactor. The primary deliverable is a ~+80-LOC extension of `cmake.rs`'s `discover_cmake_files` helper (production ~+50 LOC; tests ~+30 LOC net inside the mod tests block).

## Phase 0 — research.md pointer

Complete. See [research.md](./research.md) — 10 sections (R1 through R10) covering `safe_walk` reuse, CLI flag wiring, `cmake::read` signature extension, extended `discover_cmake_files` implementation, byte-identity guard verification, test inventory (6 unit + 5 integration ≥6 floor cleared), CHANGELOG entry shape, per-SC verification approach, interaction with milestone-155's emission pipeline, zero-wire-format-changes verification.

**No NEEDS CLARIFICATION markers remain**; Q1 (third_party recursive walking policy) locked before Phase 0.

## Phase 1 — data-model.md + contracts + quickstart pointers

Complete. See:

- [data-model.md](./data-model.md) — `cmake::read` + `discover_cmake_files` signature changes, new CLI flag structure, internal helper types, `WalkConfig` shape, fixture layout for the 5 SC-integration testbeds. `PackageDbEntry` shape + `extra_annotations` UNCHANGED (per FR-015). Wire examples UNCHANGED (milestone-155 shapes preserved).
- [contracts/cli-flag.md](./contracts/cli-flag.md) — `--cmake-third-party-recursive` full contract with consumer + provider guarantees. Default-behavior stability. Env alias. No-interaction guarantees with `--include-vendored` and `--exclude-path`.
- [quickstart.md](./quickstart.md) — 11 verification scenarios covering all 11 success criteria. Scenario 1 is the manual operator-cadence Kamailio testbed; scenarios 2–11 are automated pre-PR. **Includes the milestone-155 `--no-fail-fast` lesson** — mandates explicit `cargo test --workspace --no-fail-fast` re-run before claiming pre-PR gate green.

**Agent context update**: run `.specify/scripts/bash/update-agent-context.sh claude` per Phase 1 §3 of the plan template — appends this milestone's technology row to `CLAUDE.md`'s Active Technologies list. Executed as part of this plan invocation.

## Post-Phase-1 Constitution Check

Re-checked after Phase 1 design (per plan template §Constitution Check §GATE). Result: unchanged from pre-Phase-0. All principles remain green; no violations discovered during data-model or contract authoring.

The strict-boundaries §5 (No file-tier duplicates in default mode) was explicitly considered during data-model authoring — newly-emitting depth-2+ `.cmake` files whose declarations produce package-tier components naturally cover their own paths in the milestone-148 union of `evidence.source_file_paths`, which the milestone-133 file-tier dedup consults to suppress file-tier orphans. No boundary-crossing scenario surfaced.

## Complexity Tracking

*Empty — Constitution Check passes without violations.*

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none) | — | — |

## Next command

- `/speckit.tasks` — generate the per-user-story task breakdown (US1 P1 recursive descent + US2 P2 byte-identity guard).
- Optionally: `/speckit.analyze` — read-only cross-artifact consistency check across spec.md + plan.md + tasks.md before `/speckit.implement`. Given the milestone-155 `--no-fail-fast` lesson, analyze is a valuable sanity check even for a narrow milestone like this.
