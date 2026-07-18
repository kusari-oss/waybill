# Implementation Plan: Fix `--no-deps-dev` Flag UX — Aggregate Disable

**Branch**: `207-no-deps-dev-aggregate` | **Date**: 2026-07-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/207-no-deps-dev-aggregate/spec.md`

## Summary

One-line semantic change in `mikebom-cli/src/cli/scan_cmd.rs::resolve_enrich_sources` at line 1642:

**Pre-m207**:
```rust
deps_dev_graph: !args.no_deps_dev_graph,
```

**Post-m207**:
```rust
deps_dev_graph: !args.no_deps_dev && !args.no_deps_dev_graph,
```

That's the entire behavioral fix. `--no-deps-dev` becomes an aggregate disable per FR-001. Add a new fine-grained flag `--no-deps-dev-license` per FR-003 (opting in to the pre-m207 "license only" semantic; existing scripts relying on that behavior can migrate by prefixing their invocation with a search-and-replace). Add a WARN log per FR-006 when `--no-deps-dev` is passed alone (migration signal). Update `--help` text on both flags per FR-005.

Reconnaissance findings (per m199-m206 lesson):

- `EnrichConfig` struct at `scan_cmd.rs:1615-1620` — 3 bool fields (`deps_dev`, `clearly_defined`, `deps_dev_graph`).
- `resolve_enrich_sources` fn at `scan_cmd.rs:1631-1645` — pure function; documented as "testable as a pure function." Existing unit tests presumably exist for it; will re-verify at T003.
- `--no-deps-dev` flag defined at `scan_cmd.rs:599` (`pub no_deps_dev: bool`).
- `--no-deps-dev-graph` flag defined at `scan_cmd.rs:636` (`pub no_deps_dev_graph: bool`).
- Enrichment gates on `enrich_cfg.deps_dev` (line 2723) + `enrich_cfg.deps_dev_graph` (line 2759). Only these two boolean call sites need to be affected — no downstream plumbing changes.
- The `--enrich-sources` allowlist branch (`resolve_enrich_sources` line 1632-1637) is UNCHANGED per FR-004. When operators use allowlist mode, none of the `--no-*` flags apply.
- Test invocation site at `scan_cmd.rs:4178-4179` exists (`assert!(!parsed.inner.no_deps_dev); assert!(!parsed.inner.no_deps_dev_graph);`) — a "default-off" test we can extend with a `no_deps_dev_license` assertion.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–206; no nightly).
**Primary Dependencies**: Existing only — `clap` (workspace, `Args` derive picks up the new flag), `tracing` (workspace, FR-006 WARN log). **Zero new Cargo dependencies.**
**Storage**: N/A — CLI-flag semantic change; no state, no persistence.
**Testing**: Unit tests in `scan_cmd.rs::tests` covering the new `resolve_enrich_sources` truth table (4 flag combinations × 2 modes = ~8 cases). Regression test in a NEW file `mikebom-cli/tests/scan_no_deps_dev.rs` asserting the reporter's exact invocation shape (SC-001).
**Target Platform**: Same as mikebom itself. No new host requirements.
**Project Type**: CLI UX fix. ~50 LOC total: ~15 LOC in `scan_cmd.rs` (1 new flag + 1-line semantic change + 1 WARN log branch + updated doc-comments), ~40 LOC unit tests, ~30 LOC integration regression test.
**Performance Goals**: SC-005 wall-clock: same or faster (skipping the dep-graph fetch under `--no-deps-dev` is strictly less work than pre-m207). No explicit target; pre-PR delta ≤ 5s per SC-006.
**Constraints**: (a) zero new Cargo deps; (b) zero wire-format change (no emitter touched); (c) `--enrich-sources` allowlist semantic UNCHANGED per FR-004; (d) `--no-deps-dev-graph` semantic UNCHANGED per FR-002 (still disables ONLY the graph path); (e) backward-compat migration path per FR-003 + FR-006.
**Scale/Scope**: 1 source file edit (scan_cmd.rs). No changes to mikebom-common. No changes to mikebom-ebpf. No changes to any emitter. No parity catalog change. No new annotation.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. All new Rust code.
- **II. eBPF-Only Observation** — ✅ N/A. CLI-flag semantics; no discovery-mechanism change.
- **III. Fail Closed** — ✅ PASS. The semantic change is strictly MORE restrictive under `--no-deps-dev` alone (disables one more enrichment path). Cannot introduce new failure modes; only fewer components emitted per FR-007.
- **IV. Type-Driven Correctness** — ✅ PASS. New flag is a typed `bool` field on `ScanArgs`; `EnrichConfig` struct fields typed correctly.
- **V. Specification Compliance** — ✅ PASS. Zero new `mikebom:*` annotations; zero wire-format change. The `mikebom:source-files: ["deps.dev"]` annotation surface is UNCHANGED — it simply won't appear when `--no-deps-dev` is set post-fix (because the dep-graph enrichment path won't run to stamp it). No Principle V audit needed.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes stay in `mikebom-cli`.
- **VII. Test Isolation** — ✅ PASS. Unit tests are pure-function; no external dependencies. The one integration regression test uses a small synthetic project (or an existing public_corpus fixture) — no network, no daemon.
- **VIII. Completeness** — ✅ PASS. Doesn't change what mikebom discovers; changes only when the deps.dev enrichment paths run. Under `--no-deps-dev`, mikebom already discovered every component via the primary discovery mechanism; skipping enrichment leaves those components UNCHANGED in the emitted SBOM. Under-completeness for the enrichment path is what the operator explicitly asked for.
- **IX. Accuracy** — ✅ PASS. `--no-deps-dev` post-fix accurately reflects the operator's intent per the flag's name. Pre-fix behavior was arguably inaccurate (name-vs-semantic mismatch); post-fix corrects that.
- **X. Transparency** — ✅ PASS. FR-006 codifies a migration WARN log so operators upgrading through the semantic change see it in stderr. `--help` text updated per FR-005 so future operators find the correct semantic without reverse engineering.
- **XI. Enrichment (DX)** — ✅ PASS. This IS a DX fix — the flag now does what its name suggests. Fine-grained control preserved via new `--no-deps-dev-license` + existing `--no-deps-dev-graph` + `--enrich-sources` allowlist per FR-002 / FR-003.
- **XII. External Data Source Enrichment** — ✅ N/A. deps.dev IS an external data source, but the fix doesn't touch the enrichment implementation — only the flag semantics gating whether it runs.
- **Strict Boundary §5 (file-tier)** — ✅ N/A.

**Result**: All principles PASS. No violations. No Complexity Tracking entries needed.

**Post-Phase-1 re-check**: N/A — Phase 1 introduces no new entities beyond what's above (1 new flag field + 1 modified pure function + 1 WARN log line + help-text updates). Constitution gate remains PASS.

## Project Structure

### Documentation (this feature)

```text
specs/207-no-deps-dev-aggregate/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 2 mechanical decisions
├── data-model.md        # Phase 1 output — EnrichConfig delta + new flag + WARN log
├── quickstart.md        # Phase 1 output — 3 reproducers
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

No `contracts/` sub-directory — CLI flags ARE the contract; the flag's clap-derived help text serves as the interface documentation.

### Source Code (repository root)

```text
mikebom-cli/src/cli/
└── scan_cmd.rs                                         # MODIFIED — the entirety of m207:
                                                        #
                                                        # Line 599 (existing `--no-deps-dev` flag):
                                                        #   - Update doc-comment to state: "Disables
                                                        #     ALL deps.dev enrichment paths (both
                                                        #     license lookup AND transitive dep-graph
                                                        #     enrichment). Post-m207 aggregate
                                                        #     semantic per issue #596. Fine-grained
                                                        #     escape hatches: --no-deps-dev-license
                                                        #     (license only), --no-deps-dev-graph
                                                        #     (graph only), or --enrich-sources
                                                        #     <list> (allowlist mode overrides all
                                                        #     --no-* flags)."
                                                        #
                                                        # NEW field near line 599 (adjacent to
                                                        # existing no_deps_dev):
                                                        #   pub no_deps_dev_license: bool,
                                                        # Doc-comment: "Milestone 207 (#596) — skip
                                                        # deps.dev LICENSE enrichment only. Keeps
                                                        # dep-graph enrichment active. This is the
                                                        # pre-m207 semantic of `--no-deps-dev`;
                                                        # scripts that relied on that behavior can
                                                        # migrate by renaming."
                                                        #
                                                        # Line 636 (existing `--no-deps-dev-graph`
                                                        # flag): unchanged semantic.
                                                        #
                                                        # `EnrichConfig` struct at line 1615-1620:
                                                        # unchanged (no new field).
                                                        #
                                                        # `resolve_enrich_sources` fn at line
                                                        # 1631-1645: semantic change at line 1640-
                                                        # 1642. New logic:
                                                        #   deps_dev: !args.no_deps_dev && !args.no_deps_dev_license,
                                                        #   deps_dev_graph: !args.no_deps_dev && !args.no_deps_dev_graph,
                                                        # (`--no-deps-dev` disables BOTH; the new
                                                        # `--no-deps-dev-license` disables just the
                                                        # license path; the existing `--no-deps-dev-
                                                        # graph` disables just the graph path.)
                                                        #
                                                        # After resolve_enrich_sources (or at the
                                                        # scan entry point around line 2714): FR-006
                                                        # migration signal — WHEN `args.no_deps_dev`
                                                        # is set AND neither `no_deps_dev_license`
                                                        # nor `no_deps_dev_graph` are set (i.e., the
                                                        # operator is using the aggregate flag
                                                        # rather than fine-grained ones), emit ONE
                                                        # `tracing::info!` message stating "post-m207:
                                                        # --no-deps-dev now disables ALL deps.dev
                                                        # enrichment (previously license only). See
                                                        # --no-deps-dev-license for the pre-m207
                                                        # behavior."
                                                        #
                                                        # `scan_cmd.rs::tests`:
                                                        # New unit tests exercising the
                                                        # `resolve_enrich_sources` truth table:
                                                        #   - no_flags_default → all 3 paths on
                                                        #   - no_deps_dev_disables_both_deps_dev_paths (P1 acceptance)
                                                        #   - no_deps_dev_graph_still_disables_graph_only
                                                        #   - no_deps_dev_license_disables_license_only (P2 acceptance)
                                                        #   - no_deps_dev_and_license_are_equivalent_for_license_path
                                                        #   - no_deps_dev_wins_over_no_deps_dev_graph (composition)
                                                        #   - enrich_sources_allowlist_overrides_no_deps_dev (FR-004)
                                                        #   - clearly_defined_unaffected_by_no_deps_dev

mikebom-cli/tests/
└── scan_no_deps_dev.rs                                 # NEW — m207 integration regression test:
                                                        #
                                                        # fr001_no_deps_dev_produces_no_deps_dev_provenance:
                                                        #   - Scan a small synthetic project (e.g.,
                                                        #     the m205 US1 fixture `[dependencies]
                                                        #     serde = "1"`) with `--no-deps-dev
                                                        #     --offline`.
                                                        #   - Actually because `--offline` short-
                                                        #     circuits all network calls anyway,
                                                        #     use `--no-clearly-defined` (mirroring
                                                        #     the reporter's exact invocation)
                                                        #     WITHOUT --offline. Requires network
                                                        #     access for deps.dev lookups if the
                                                        #     flag doesn't suppress them.
                                                        #   - Actually simpler: use `--offline` +
                                                        #     `--no-deps-dev` and assert no
                                                        #     `mikebom:source-files: "deps.dev"`
                                                        #     annotation appears — under `--offline`,
                                                        #     if the fix works, the emit path is
                                                        #     symmetric with the network-fetch path
                                                        #     (both skipped).
                                                        #   - Assert emitted CDX contains ZERO
                                                        #     components with `.properties[]`
                                                        #     matching `mikebom:source-files` value
                                                        #     `["deps.dev"]`.
```

**Structure Decision**: 1 source file edit (scan_cmd.rs) + 1 new integration test file (scan_no_deps_dev.rs). Zero committed fixture additions; unit tests use synthetic `ScanArgs` structs; integration test scans a tiny in-process fixture. Zero non-fixture golden regen (no wire-format change per plan.md constraint (b)).

## Complexity Tracking

No constitution violations. All principles pass on first check. Trivial 1-line behavioral change + 1 new flag + 1 WARN log + doc-comment updates. No new architecture; no new patterns.
