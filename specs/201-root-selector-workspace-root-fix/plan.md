# Implementation Plan: Root-Selector Workspace-Root Disambiguation

**Branch**: `201-root-selector-workspace-root-fix` | **Date**: 2026-07-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/201-root-selector-workspace-root-fix/spec.md`

## Summary

Stamp a new internal-only annotation `mikebom:is-cargo-workspace-toplevel` at cargo m064 main-module emission time — set to `true` ONLY for the crate whose Cargo.toml is at rootfs (the workspace top-level manifest, distinct from workspace-member manifests). Downstream at `scan_fs/mod.rs:922-942`, the `is_workspace_root` stamping consumes this annotation as a positive-identifier override for cargo main-modules — sidestepping the shared-`Cargo.lock`-path collision that causes both `vaultwarden` and `macros` to look identical to the current filesystem-based heuristic.

Zero new `mikebom:*` annotations in emitted SBOMs (the new annotation is internal-emission-only, filtered by extending `is_internal_emission_key` at `root_selector.rs:437-439`). Bounded change surface: 2 source files (cargo.rs + scan_fs/mod.rs) + 1 extended fixture (m200's `root_package_lifecycle/` gains an npm sub-project) + 1 extended integration test.

Reconnaissance findings (per m199/m200 empirical-verification lesson):
- Both `vaultwarden` and `macros` in the test-vaultwarden reproducer end up with `evidence.source_file_paths = ["Cargo.lock"]` AND `mikebom:source-files = "[\"Cargo.lock\"]"` — identical values. No per-crate Cargo.toml path is preserved in the augmented main-module entries.
- The m200 `root_names` accumulator records EVERY workspace-member's [package].name (not just the workspace top-level's). Distinguishing "which name is THE workspace root" requires new signal — hence the new internal annotation.
- Golden regen expected 0 files (verified at implement time per m199/m200 lesson).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–200; no nightly).
**Primary Dependencies**: Existing only — `toml = "0.8"` (already used pervasively by cargo.rs; needed to detect `[workspace]` block presence in a Cargo.toml), `std::collections::HashSet` / `HashMap` (already used), `serde`/`serde_json` (annotation values), `tracing`, `anyhow`/`thiserror`. **No new crates.** No subprocess calls. No network access.
**Storage**: N/A — all state in-process per scan; matches every reader milestone since 002.
**Testing**: New integration test scenarios in `mikebom-cli/tests/cargo_workspace_root_lifecycle_m200.rs` (extending the m200 file) that scan an EXTENDED m200 fixture with an added npm sub-project (`sub/package.json`). The extended fixture produces 3 main-module candidates (cargo-root `app`, cargo-member `helper`, npm-nested `sub`), and the test asserts `metadata.component.purl` post-m201 is the cargo-root via the RepoRoot heuristic.
**Target Platform**: Same as mikebom itself.
**Project Type**: Cargo reader classifier + root-selector consumer fix. ~15 LOC in cargo.rs (emit new annotation) + ~10 LOC in scan_fs/mod.rs (consume new annotation as positive-identifier override) + ~5 LOC in root_selector.rs (extend `is_internal_emission_key` to filter the new annotation out of SBOM output) + ~40 LOC fixture extension + ~50 LOC test additions. **Roughly 120 LOC total.**
**Performance Goals**: No perf regression beyond FR-006 (`./scripts/pre-pr.sh` wall-clock delta ≤ 5s per SC-006).
**Constraints**: (a) zero new Cargo deps; (b) new annotation is internal-emission-only (filtered from CDX/SPDX output); (c) fix MUST NOT alter root election for existing scans (FR-004 guardrail); (d) fix MUST distinguish workspace-TOP-LEVEL from workspace-MEMBER cargo crates via a positive identifier, not filesystem-based heuristics that fail under cargo m064's augment-in-place.
**Scale/Scope**: 3 source files touched (cargo.rs, scan_fs/mod.rs, root_selector.rs) + 1 fixture extended + 1 test file extended. Small, focused change surface.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. Pure Rust throughout; no new deps.
- **II. eBPF-Only Observation** — ✅ N/A. User-space classifier disambiguation.
- **III. Fail Closed** — ✅ PASS. New annotation is a positive-identifier signal; absence falls back to existing filesystem-based logic — same as pre-m201 (safe degradation for non-cargo cases).
- **IV. Type-Driven Correctness** — ✅ PASS. Uses existing `HashSet<String>` machinery, `serde_json::Value::Bool` for the internal-only annotation value.
- **V. Specification Compliance** — ✅ PASS. **New annotation is INTERNAL-EMISSION-ONLY** (filtered by `is_internal_emission_key`). It never appears in emitted CDX / SPDX 2.3 / SPDX 3 output, so no wire-format contract is affected. Constitution Principle V's "standards-native fields take precedence over `mikebom:*` properties" applies to emitted-output annotations; internal-only routing signals don't need audit citation because they don't reach consumers. Documented in FR-007.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes stay in `mikebom-cli`.
- **VII. Test Isolation** — ✅ PASS. New fixture uses `tests/fixtures/cargo/root_package_lifecycle/` (extended in-tree checked fixture, matches m200 convention).
- **VIII. Completeness** — ✅ PASS. Improves accuracy of `metadata.component` identity; does not omit components.
- **IX. Accuracy** — ✅ PASS. Correcting the root-election tie-break IS the definition of Accuracy improvement.
- **X. Transparency** — ✅ PASS. The root-election result + heuristic name are already surfaced in the scan log (`"root-component selected via heuristic"` at `cli/scan_cmd.rs`); post-m201 the log now reports `heuristic = "repo-root"` instead of `"ecosystem-priority"` for the vaultwarden pattern, providing more informative provenance.
- **XI. / XII. Enrichment** — ✅ N/A. No external data source touched.
- **Strict Boundary §5 (file-tier)** — ✅ N/A.

**Result**: All principles PASS. No violations.

**Post-Phase-1 re-check**: N/A here — Phase 1 introduces no new entities beyond the internal-only annotation already documented above. Constitution gate trivially remains PASS post-design.

## Project Structure

### Documentation (this feature)

```text
specs/201-root-selector-workspace-root-fix/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 4 mechanical decisions
├── data-model.md        # Phase 1 output — new annotation semantics + is_workspace_root override logic
├── quickstart.md        # Phase 1 output — 5 reproducers (vaultwarden verify, fixture extension, regression, SC-003 losers, pre-pr delta)
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

No `contracts/` sub-directory. This fix modifies an INTERNAL classifier + a related INTERNAL annotation-consumer. No new wire-format contract, no new CLI flag, no new user-facing annotation.

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/package_db/
└── cargo.rs                                        # MODIFIED — FR-001:
                                                    #   build_cargo_main_module_entry (line 363+)
                                                    #   detects whether the parsed manifest also has
                                                    #   a [workspace] block. When present, stamps
                                                    #   `mikebom:is-cargo-workspace-toplevel: true`
                                                    #   into the emitted PackageDbEntry's
                                                    #   extra_annotations bag. Absent → no stamp
                                                    #   (workspace members get no annotation).

mikebom-cli/src/scan_fs/
└── mod.rs                                          # MODIFIED — FR-001:
                                                    #   is_workspace_root stamping (line 944-947)
                                                    #   checks for `mikebom:is-cargo-workspace-toplevel`
                                                    #   annotation on the component BEFORE the
                                                    #   filesystem-based comparison. If present + true,
                                                    #   short-circuits to is_workspace_root = true.
                                                    #   Otherwise falls back to the existing
                                                    #   canonical_manifest_parent == canonical_root
                                                    #   check (preserving all non-cargo behavior).
                                                    #   For cargo mainmods WITHOUT the new annotation
                                                    #   (workspace members), the annotation is absent
                                                    #   → filesystem check runs → member's parent
                                                    #   dir != rootfs → is_workspace_root = false.

mikebom-cli/src/generate/
└── root_selector.rs                                # MODIFIED — internal-emission-only filter:
                                                    #   is_internal_emission_key (line 437+) extends
                                                    #   its match to include the new annotation key
                                                    #   `mikebom:is-cargo-workspace-toplevel`.
                                                    #   Post-fix, the CDX/SPDX 2.3/SPDX 3 emitters
                                                    #   filter it out — matches the existing
                                                    #   `mikebom:is-workspace-root` treatment.

mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/
├── (existing m200 files unchanged)
└── sub/                                            # NEW — FR-005 extension:
    ├── package.json                                #   `{"name": "sub", "version": "1.0.0"}` —
                                                    #   nested npm project introduces a 3rd main-
                                                    #   module candidate + reproduces the multi-
                                                    #   ecosystem shape from #587.
    └── (nothing else needed — npm reader emits its main-module from package.json alone)

mikebom-cli/tests/
└── cargo_workspace_root_lifecycle_m200.rs         # MODIFIED — extended per FR-005:
                                                    #   New test `scan_cargo_workspace_root_wins_multi_ecosystem_m201`:
                                                    #     scans the extended fixture, asserts
                                                    #     metadata.component.name == "app" AND
                                                    #     the root-election heuristic is
                                                    #     "repo-root" (not "ecosystem-priority").
```

**Structure Decision**: 3 source-file edits + 1 fixture-directory extension + 1 test-file addition. Zero existing goldens require regen per plan reconnaissance (re-verified at implement time). Small, focused change surface — matches the m200 pattern.

## Complexity Tracking

No constitution violations. All principles pass on first check. Fix is inherently narrow: adds one internal-only annotation, threads it through 3 sites, extends one fixture + one test.
