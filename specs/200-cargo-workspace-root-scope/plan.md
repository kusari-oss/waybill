# Implementation Plan: Cargo Workspace-Root [package] Runtime Classification

**Branch**: `200-cargo-workspace-root-scope` | **Date**: 2026-07-16 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/200-cargo-workspace-root-scope/spec.md`

## Summary

Fix the cargo BFS-seed gap at `mikebom-cli/src/scan_fs/package_db/cargo.rs::parse_cargo_toml` (line 721+) that causes workspace-root `[package]` entries to fall through to `LifecycleScope::Development`. Two-line addition: read the `[package].name` key from each parseable workspace Cargo.toml and insert it into `CargoTomlSections.prod_deps`. Post-fix, the BFS prod-set closure includes the workspace root as a Runtime seed → root component tags as Runtime → CDX `scope: null` → m127 root-selector picks the actual application, not a proc-macro helper.

Zero new Cargo dependencies. Bounded change surface: 1 function edit in `cargo.rs`, 1 new fixture directory, 1 new integration test file (per FR-006). Regression risk minimal — the seed change is additive only.

Reconnaissance surfaced (per the m199 empirical-verification lesson): the m083 audit fixture is a virtual workspace (only `[workspace]`, no `[package]`) so it's orthogonal to this fix. The `produces_binaries/cargo/workspace/` fixture is also virtual. The rust-ripgrep public-corpus golden does NOT currently emit a `pkg:cargo/ripgrep` component at all — needs implement-time re-verification to determine whether the fix newly SURFACES the ripgrep root (which would drift that golden). Golden regen scope: **most likely 0, at most the 3 rust-ripgrep files** — verified at implement time before final tasks estimation.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–199; no nightly).
**Primary Dependencies**: Existing only — `toml = "0.8"` (already used pervasively by cargo.rs), `std::collections::HashSet` (already used), `serde`/`serde_json` (annotation values), `tracing` (existing parse-error warn logs), `anyhow`/`thiserror`. **No new crates.** No subprocess calls. No network access.
**Storage**: N/A — all state in-process per scan; matches every reader milestone since 002.
**Testing**: New integration test at `mikebom-cli/tests/cargo_workspace_root_lifecycle_m200.rs` (or as an addition to `scan_cargo.rs` if that file exists) that scans a new fixture at `mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/`. Existing cargo integration tests (`transitive_parity_cargo.rs`, `scan_cargo.rs`, etc.) MUST pass byte-identically for non-root entries per FR-003.
**Target Platform**: Same as mikebom itself (all mikebom-supported hosts).
**Project Type**: Cargo classifier bug fix. ~5 LOC change in `cargo.rs::parse_cargo_toml` + ~40 LOC new fixture (Cargo.toml + Cargo.lock + a member sub-crate stub) + ~100 LOC new integration test. **Roughly 150 LOC total.**
**Performance Goals**: No perf regression beyond FR-007 (`./scripts/pre-pr.sh` wall-clock delta ≤ 5s per SC-006).
**Constraints**: (a) zero new Cargo deps; (b) additive-only seed change — MUST NOT reclassify any non-root entry; (c) MUST no-op for virtual workspaces; (d) MUST NOT cross-seed across independent workspaces in a multi-workspace scan.
**Scale/Scope**: 1 source file edit + 1 new fixture directory + 1 new integration test. Small, focused change surface.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. Pure Rust throughout; no new deps.
- **II. eBPF-Only Observation** — ✅ N/A. User-space classifier bug fix.
- **III. Fail Closed** — ✅ PASS. No new failure surface; existing `parse_cargo_toml` warn-and-skip semantics preserved on parse errors.
- **IV. Type-Driven Correctness** — ✅ PASS. Uses existing `HashSet<String>`, `LifecycleScope` enum, no new stringly-typed boundaries introduced.
- **V. Specification Compliance** — ✅ PASS. Zero new `mikebom:*` annotations. This milestone CORRECTS an existing value in an existing `mikebom:lifecycle-scope` annotation (Runtime instead of Development), which maps to existing CDX 1.6 `scope` + SPDX 2.3 `Dev/Build/Test_DEPENDENCY_OF` + SPDX 3 `lifecycleScope` per m052's audit. No Principle-V audit needed — no new annotation introduced.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes stay in `mikebom-cli/src/scan_fs/package_db/cargo.rs`.
- **VII. Test Isolation** — ✅ PASS. New fixture uses `tempfile`-scoped fixtures OR checked-in `tests/fixtures/` (both unprivileged patterns matching existing convention).
- **VIII. Completeness** — ✅ PASS. Improves accuracy of an existing component's lifecycle-scope; does not omit components or introduce new ones (except the previously-suppressed `pkg:cargo/vaultwarden`-like entries that surface into metadata.component instead of components[]).
- **IX. Accuracy** — ✅ PASS. Correcting a misclassification IS the definition of Accuracy improvement (Principle IX rationale: "An SBOM bloated with phantom dependencies erodes consumer trust… Accuracy preserves the signal-to-noise ratio that makes SBOMs actionable"). Same principle applies to wrong-scope entries.
- **X. Transparency** — ✅ PASS. No new transparency annotations needed; existing `mikebom:lifecycle-scope` value now more accurately reflects the classifier's intent.
- **XI. / XII. Enrichment** — ✅ N/A. No external data source touched.
- **Strict Boundary §5 (file-tier)** — ✅ N/A. Not a file-tier change.

**Result**: All principles PASS. No violations.

**Post-Phase-1 re-check**: N/A here — Phase 1 introduces no new entities beyond the additive seed change already documented above. Constitution gate trivially remains PASS post-design.

## Project Structure

### Documentation (this feature)

```text
specs/200-cargo-workspace-root-scope/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 4 mechanical decisions
├── data-model.md        # Phase 1 output — CargoTomlSections + prod_set closure semantics
├── quickstart.md        # Phase 1 output — 3 reproducers (fixture, ripgrep drift check, override baseline)
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

No `contracts/` sub-directory. This fix touches an existing INTERNAL classifier — no new wire-format contract, no new CLI flag, no new annotation. The public "contract" the fix modifies is the CDX `scope` field value on cargo workspace-root components, which is already documented in `docs/reference/sbom-format-mapping.md` per m052.

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/package_db/
└── cargo.rs                                        # MODIFIED — FR-001:
                                                    #   parse_cargo_toml at line 721+ — extract
                                                    #   [package].name from the root manifest and
                                                    #   insert into CargoTomlSections.prod_deps
                                                    #   alongside the existing [dependencies] pass.

mikebom-cli/tests/fixtures/cargo/
└── root_package_lifecycle/                         # NEW — FR-006 fixture:
    ├── Cargo.toml                                  #   Workspace root — [package] name="app"
                                                    #     + [dependencies] helper = { path = "helper" }
                                                    #     + [workspace] members = ["helper"]
    ├── Cargo.lock                                  #   Resolved lockfile — both app + helper as
                                                    #   [[package]] entries
    ├── src/main.rs                                 #   Stub — `fn main() {}`
    └── helper/
        ├── Cargo.toml                              #   name = "helper", no deps
        └── src/lib.rs                              #   Stub — `pub fn stub() {}`

mikebom-cli/tests/
└── cargo_workspace_root_lifecycle_m200.rs         # NEW — FR-006 integration test:
                                                    #   scan_cargo_workspace_root_is_runtime_m200
                                                    #     asserts pkg:cargo/app@<ver> has scope: null
                                                    #     + no mikebom:lifecycle-scope=development annot
                                                    #   scan_cargo_workspace_root_wins_root_election_m200
                                                    #     asserts metadata.component.name == "app"
```

**Structure Decision**: Single-file source edit + one new fixture directory + one new integration test file. Zero existing goldens require regen per plan reconnaissance (rust-ripgrep golden reconfirmed at implement time — if drift, regen via workflow_dispatch per m196/m199 pattern). Small, focused change surface.

## Complexity Tracking

No constitution violations. All principles pass on first check. The fix is inherently narrow: 5 LOC of Rust + a small fixture + one test file.
