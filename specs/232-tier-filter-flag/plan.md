# Implementation Plan: `--tier=<mode>` output-filter flag

**Branch**: `232-tier-filter-flag` | **Date**: 2026-08-10 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/232-tier-filter-flag/spec.md`

## Summary

Add a `--tier=<mode>` CLI flag on `waybill sbom scan` with values `all` (default), `source-only`, `design-only`, `source-and-binary`. When set to a non-default value, apply a strict-literal `sbom_tier` filter over the resolved-component set before the format builders run. Because each format's graph-completeness computation lives INSIDE its own builder and takes `components` + `relationships` as inputs, filtering before dispatch means every downstream annotation (`waybill:graph-completeness`, `waybill:workspaces-detected`, tier-based document-scope counters) naturally reflects the filtered set without emitter-side changes.

Insertion point: `waybill-cli/src/cli/scan_cmd.rs` line 3168–3199 already implements exactly this shape for `--exclude-scope`. The tier filter is a sibling pass — same `components.retain(...)` + `relationships.retain(...)` shape, filtering on `sbom_tier` instead of `lifecycle_scope`.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–231; no nightly required).
**Primary Dependencies**: Existing only — `clap` (workspace, for the new `ValueEnum` derive + flag), `tracing` (INFO log for FR-008 empty-result path + FR-011 warn-and-continue on degenerate combos), `waybill_common::resolution::{ResolvedComponent, Relationship}` (the existing types the filter operates on). **Zero new Cargo dependencies.**
**Storage**: N/A — pure in-memory filter over the already-resolved component slice.
**Testing**: `cargo +stable test --workspace` — new unit tests colocated with the filter helper + one integration test per mode in `waybill-cli/tests/tier_filter_flag.rs`. Reuses the existing `common::bin` + `apply_fake_home_env` subprocess scaffold from m230's `nuget_main_module_parity.rs`.
**Target Platform**: All platforms waybill already builds on (Linux, macOS, Windows).
**Project Type**: Single-crate CLI-flag addition. Two files touched (scan_cmd.rs + one new test file); no new modules.
**Performance Goals**: The filter is a single-pass `Vec::retain` over `components` + `relationships`. Cost is O(N + M) where N is component count and M is edge count. Both are already scanned linearly downstream; the filter's incremental cost is negligible (single-digit-µs on 469-module Grafana-scale scans).
**Constraints**: FR-002 (byte-identical emission when `--tier=all` or flag omitted). Must NOT alter emitter internals. The filter runs strictly BEFORE the format-builder dispatch at scan_cmd.rs:3200+.
**Scale/Scope**: Same scale as any existing waybill scan — up to O(1M) components on the largest container-image scans. Filter is O(N) linear; no perf-relevant thresholds.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Evaluated against `.specify/memory/constitution.md` v2.1.0. **All principles PASS.**

- **I. Pure Rust, Zero C**: PASS. Zero new C, zero new deps.
- **II. eBPF-Only Observation** + **XII. External Data Source Enrichment**: PASS. This milestone adds a filter over already-emitted components; introduces no new dependency-discovery mechanism.
- **III. Fail Closed**: PASS. FR-008 says an empty-result scan exits 0 with a WARN — the filter is degrading emitted output at operator request, not silently failing on a genuine scan error.
- **IV. Type-Driven Correctness**: PASS. `TierMode` is a `clap::ValueEnum`; every match on it is exhaustive; no raw `String`-typed domain values cross function boundaries. No `.unwrap()` in production paths.
- **V. Specification Compliance**: PASS. This milestone does NOT introduce any new `waybill:*` annotation. It reads the existing `sbom_tier` field on `ResolvedComponent` and filters. Every emitted format continues to conform to its schema — the filter reduces the component set but does not alter component shape.
- **VI. Three-Crate Architecture**: PASS. Change contained inside `waybill-cli/src/cli/scan_cmd.rs`. No new crates.
- **VII. Test Isolation**: PASS. All new tests are pure-logic unit tests + one fixture-driven subprocess integration test. No eBPF, no root, no CAP_BPF.
- **VIII. Completeness**: PASS with note. This milestone gives operators a knob to DROP components at emission time. That's an operator-requested reduction, not a scan-side false-negative — the FR-007 requirement that document-scope annotations re-evaluate against the filtered set means the emitted SBOM's transparency signals correctly reflect the operator's filtering choice.
- **IX. Accuracy**: PASS. No new heuristics; no synthesized components. Filter drops existing components based on their existing `sbom_tier` marker.
- **X. Transparency**: PASS. FR-008 mandates a WARN log line when the filter drops every component. FR-007 requires document-scope annotations to re-evaluate against the filtered set — so consumers reading the SBOM see the same transparency signals they would have gotten from a scan whose input naturally produced only the filtered subset.
- **XI. Enrichment**: PASS. No enrichment path touched.

No violations. No Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/232-tier-filter-flag/
├── plan.md                        # This file (/speckit.plan output)
├── research.md                    # Phase 0 output
├── data-model.md                  # Phase 1 output
├── quickstart.md                  # Phase 1 output
├── contracts/
│   └── tier-filter-cli.md         # CLI-contract spec for --tier
├── checklists/
│   └── requirements.md            # From /speckit.specify
├── spec.md                        # Feature spec (with Clarifications)
└── tasks.md                       # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/cli/scan_cmd.rs   # Existing CLI entry point. This
                                   # milestone adds:
                                   #   - `TierMode` clap::ValueEnum
                                   #     (all/source-only/design-only/
                                   #     source-and-binary)
                                   #   - `tier: TierMode` field on
                                   #     `ScanArgs` with `#[arg(long)]`
                                   #     and default `TierMode::All`
                                   #   - `apply_tier_filter(&mut
                                   #     components, &mut relationships,
                                   #     mode)` helper, sibling to
                                   #     `--exclude-scope` at line 3175
                                   #   - Call site in scan_cmd.rs after
                                   #     `--exclude-scope` and before
                                   #     the format-builder dispatch

waybill-cli/tests/                 # New integration test file:
└── tier_filter_flag.rs            # 4 subprocess-based tests (one per
                                   # mode) reusing the m230
                                   # nuget_main_module_parity.rs
                                   # scaffold; asserts on component-set
                                   # membership + graph-completeness
                                   # annotation reflection
```

**Structure Decision**: Single-file addition in `scan_cmd.rs`. The filter helper lives alongside `--exclude-scope`'s existing pass at line 3175 — they're semantically identical shapes (retain-on-predicate + drop-dangling-edges + count-log). No new module needed; no changes to any file under `waybill-cli/src/generate/` (the format builders naturally consume whatever `components` slice they're given).

## Complexity Tracking

> No constitution violations to justify. Section intentionally empty.
