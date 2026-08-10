# Implementation Plan: Fix `go list all` preflight failure in Go workspace mode

**Branch**: `231-fix-go-work-preflight` | **Date**: 2026-08-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/231-fix-go-work-preflight/spec.md`

## Summary

The Go reader's `mod_why` preflight sets `GOFLAGS=-mod=mod` in `apply_offline_env` (`waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:134-140`), which Go's workspace mode rejects. Any offline scan against a project with a `go.work` file in the module's ancestor chain fails the preflight and downgrades every Go component to `waybill:build-inclusion: unknown` — verified 469-module degradation on `github.com/grafana/grafana`. The fix detects workspace mode (walking up from each main-module directory for a `go.work`, plus honoring the `GOWORK` env var) and, when active, omits the `-mod=mod` flag so Go's workspace default (`-mod=readonly`) applies. Non-workspace scans preserve the pre-231 `-mod=mod` behavior verbatim.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–230; no nightly required for this user-space-only bug fix).
**Primary Dependencies**: Existing only — `std::path::{Path, PathBuf}`, `std::env`, `std::process::Command`, `tracing`, `anyhow`. **Zero new Cargo dependencies.** No new subprocess types beyond the existing `Command`-with-timeout pattern at `mod_why.rs:154-173`. No network. No filesystem writes.
**Storage**: N/A — all state in-process per scan.
**Testing**: `cargo +stable test --workspace` — new unit tests colocated with `mod_why.rs::tests` (workspace detection + env-var handling) plus one integration test at `waybill-cli/tests/golang_workspace_mode_preflight.rs` scanning a synthetic workspace fixture end-to-end. The Grafana verification (SC-002) is a manual step, not automated.
**Target Platform**: All platforms waybill already builds on (Linux, macOS, Windows). No platform-specific code — the ancestor walk uses `std::path`, which is cross-platform.
**Project Type**: Single-crate bug fix inside `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`. Two files touched (mod_why.rs + one new test file); no new modules.
**Performance Goals**: The workspace-detection ancestor walk runs once per main-module (typically ≤10 per scan; unbounded but capped by scan-tree depth). Each walk is stdlib `fs::metadata` on `<dir>/go.work` up the chain — negligible cost (single-digit-ms max). No perf-relevant threshold.
**Constraints**: FR-003 (byte-identical child-process env when workspace mode NOT active). Must NOT alter the preflight's behavior on single-module projects — the historical majority case. Verified via SC-003.
**Scale/Scope**: Real workspace scans range from 2 modules (small polyrepos) to 20+ (Grafana). Detection cost scales linearly with main-module count; classification-quality improvement scales with total-module count (Grafana: 469 → 0 unknowns projected).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Evaluated against `.specify/memory/constitution.md` v2.1.0. **All principles PASS.**

- **I. Pure Rust, Zero C**: PASS. Zero new C, zero new deps, no new toolchain requirements.
- **II. eBPF-Only Observation** + **XII. External Data Source Enrichment**: PASS with note. The bug lives in `scan_fs/package_db/golang`, which is waybill's static-scan mode — sanctioned by every sibling milestone and by the constitution's XII carve-out. This fix does NOT introduce a new dependency source; it corrects a subprocess invocation that already exists.
- **III. Fail Closed**: PASS. FR-005 preserves the existing warn-and-skip behavior when the preflight still fails post-fix; the scan never errors closed on this failure class.
- **IV. Type-Driven Correctness**: PASS. Ancestor walk returns `Option<PathBuf>`; env-var parsing returns a small enum (`WorkspaceMode::{Auto, Off, ExplicitPath(PathBuf)}`); no raw String types cross function boundaries for domain values. No `.unwrap()` in production paths.
- **V. Specification Compliance**: PASS with explicit audit. This milestone does NOT introduce any new `waybill:*` annotation. The existing `waybill:build-inclusion` annotation (parity catalog registered under m112) is the sole quality signal changed — its values shift from `unknown` fallbacks to `prod` / `test` / `not-needed` / `unresolved` for workspace-mode scans. Every value is one the catalog already documents. Audit result: no new fields; no new catalog rows; no new `sbom-format-mapping.md` entries.
- **VI. Three-Crate Architecture**: PASS. No new crates. Change is contained inside `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`.
- **VII. Test Isolation**: PASS. All new tests are pure-logic unit tests + one fixture-driven integration test. No eBPF, no root, no CAP_BPF.
- **VIII. Completeness**: PASS. This fix directly restores completeness — 469 modules on Grafana currently mark `unknown` (partial completeness signal); post-fix they get definitive verdicts.
- **IX. Accuracy**: PASS. The fix produces verdicts Go's own toolchain would produce in workspace mode. No new heuristics; no fabricated classifications.
- **X. Transparency**: PASS. FR-006 adds a workspace-active counter to the existing `INFO: go-mod-why classification:` log line so operators can see how many modules had workspace mode active. The existing WARN path is preserved for residual failures.
- **XI. Enrichment** + **XII. External Data Source Enrichment**: PASS. No external data source added; the fix corrects an existing subprocess invocation.

No violations. No entries needed in Complexity Tracking below.

## Project Structure

### Documentation (this feature)

```text
specs/231-fix-go-work-preflight/
├── plan.md                        # This file (/speckit.plan output)
├── research.md                    # Phase 0 output
├── data-model.md                  # Phase 1 output
├── quickstart.md                  # Phase 1 output
├── contracts/
│   └── go-work-detection.md       # Contract for workspace-detection behavior
├── checklists/
│   └── requirements.md            # From /speckit.specify
├── spec.md                        # Feature spec (with Clarifications section)
└── tasks.md                       # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/golang/
├── mod_why.rs                     # Existing preflight. This milestone adds:
│                                  #   - detect_workspace_mode(main_module_dir) → WorkspaceMode
│                                  #     helper (ancestor walk + GOWORK env parse)
│                                  #   - Modified apply_offline_env() that takes a
│                                  #     workspace-mode parameter and conditionally
│                                  #     omits/adjusts GOFLAGS
│                                  #   - New WorkspaceMode enum
│                                  #   - Colocated unit tests for the detector
├── mod.rs                         # UNCHANGED (dispatch layer)
└── go_mod_graph.rs                # UNCHANGED — a sibling subprocess runner that
                                   # uses the same offline-env pattern. Consider
                                   # extending in a follow-up if it exhibits the
                                   # same bug (Research §R3 investigates).

waybill-cli/tests/                 # Existing test surface. This milestone adds:
└── golang_workspace_mode_preflight.rs  # NEW — integration test scanning a
                                        # synthetic workspace fixture end-to-end
                                        # via `waybill sbom scan` subprocess.

waybill-cli/tests/fixtures/golden_inputs/golang/workspace_mode/  # NEW fixture:
├── go.work
├── module-a/
│   ├── go.mod
│   └── main.go
└── module-b/
    ├── go.mod
    └── lib.go
```

**Structure Decision**: In-place extension of `mod_why.rs`. The workspace-detection logic is a small pure function that the existing `apply_offline_env` consults before setting `GOFLAGS`. No new module files needed; helper + enum live alongside the existing offline-env-pinning code so the two are read together. Fixture placement mirrors the existing NuGet fixture layout at `waybill-cli/tests/fixtures/golden_inputs/nuget/`.

## Complexity Tracking

> No constitution violations to justify. Section intentionally empty.
