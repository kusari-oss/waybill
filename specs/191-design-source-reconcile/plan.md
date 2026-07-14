# Implementation Plan: Design-Tier / Source-Tier Reconciliation

**Branch**: `191-design-source-reconcile` | **Date**: 2026-07-14 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/191-design-source-reconcile/spec.md`

## Summary

Two-fix milestone bundled at user direction:

- **US1 / #560 — architectural reconciliation**: at emission time, collapse each design-tier component into its matching source-tier sibling when one exists in the same workspace scope. Attach the design-tier metadata (`mikebom:requirement-range`, `mikebom:source-manifest`) to the surviving source-tier component as multiple property entries per manifest (per Q1 answer B). Rewrite any incoming `dependsOn` graph edges to target the survivor.
- **US2 / #558 — spec-clean versionless PURL**: for standalone design-tier components with no source-tier match, emit a purl-spec-canonical `pkg:<type>/<name>` string (no trailing `@`, no version segment). Omit the format-specific version field entirely (CDX `.version` omitted, SPDX 2.3 `versionInfo: "NOASSERTION"`, SPDX 3 `software_packageVersion` omitted). Bom-ref/SPDXID/spdxId is the versionless PURL as-is (per Q3 answer A).

Technical approach: a new `reconcile_design_source_tiers` pass runs immediately after the existing `deduplicate` pass in `mikebom-cli/src/cli/scan_cmd.rs` (currently at line 2742, second dedup call). The new pass groups by `(ecosystem, name, workspace-scope)` — a DIFFERENT key from the existing deduplicator's `(ecosystem, name, version, parent_purl)` — so it matches ACROSS version differences (empty design-tier version vs concrete source-tier version). Per-ecosystem PURL builders (e.g., `build_npm_purl`) get a small local fix to emit versionless PURLs when passed an empty version. INFO-level summary + DEBUG-level per-component logging per Q4 answer B.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–190; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `serde` / `serde_json` (annotation values), `tracing` (INFO summary + DEBUG per-component logs per FR-020 / Q4), `anyhow` / `thiserror` (error propagation), `mikebom_common::types::purl::Purl` (PURL construction + validation), `mikebom_common::resolution::ResolvedComponent` (the type being reconciled), `std::collections::HashMap` / `BTreeMap` (grouping). **Zero new Cargo dependencies.** No subprocess calls. No network access.

**Storage**: N/A — the reconciliation pass is a pure in-memory transformation over a `Vec<ResolvedComponent>` executed once per scan. No caches, no persistence. Matches every milestone since 002.

**Testing**: `cargo +stable test --workspace` (workspace-wide unit + integration). New assertions:

- Unit tests co-located with `reconcile_design_source_tiers` in a new module `mikebom-cli/src/resolve/reconciler.rs` — matching, annotation transfer, multi-declaration preservation per FR-004, graph-edge rewriting per FR-005.
- Unit tests co-located with each per-ecosystem PURL builder (npm, pip, cargo, maven, gem, pnpm, yarn, composer) — versionless PURL emission per FR-009.
- Integration tests scanning synthetic fixture directories:
  - `mikebom-cli/tests/design_source_reconcile.rs` — happy-path reconciliation + multi-declaration edge case + workspace-parent walk (Q2) + graph-edge rewriting + `optionalDependencies` "declared-but-not-installed" standalone path.
  - `mikebom-cli/tests/design_tier_versionless_purl.rs` — PURL round-trip stability for standalone design-tier + CDX `.version` omission + SPDX 2.3 `NOASSERTION` + SPDX 3 `software_packageVersion` omission.
- Schema validators: SPDX 2.3 JSON schema + SPDX 3.0.1 JSON schema + `spdx3-validate==0.0.5` all clean on m191 fixture set.
- Regression: every existing golden without design-tier/source-tier pairs is byte-identical; every golden WITH such pairs is regenerated per FR-016 as documented drift.

**Target Platform**: Linux + macOS host builds; behavior is host-agnostic (pure in-memory transformation over emitted JSON).

**Project Type**: CLI (existing `mikebom sbom scan` subcommand path).

**Performance Goals**: The reconciliation pass is O(N) on the component set (single HashMap-group pass + O(E) graph-edge rewrite where E is the total edge count). For the customer React Native scan (1998 components, 101 reconciliation pairs), expected runtime overhead is <10ms — well under the noise threshold for a full scan (typical scan takes multiple seconds). No perf regression concerns; no new perf test target needed.

**Constraints**: Byte-identity for every golden WITHOUT design/source pairs (SC-006). Byte-identity relaxation permitted for goldens WITH such pairs (FR-016) — those regenerate once and re-commit as documented milestone drift. No new `mikebom:*` annotations (FR-018 / Principle V). No opt-in preservation flag (FR-019 / #560 recommendation).

**Scale/Scope**: One new module (`mikebom-cli/src/resolve/reconciler.rs`, ~200 LOC) + small deltas in per-ecosystem PURL builders (~8-10 files, ~5 LOC each) + wiring into `scan_cmd.rs` at line 2742. Estimated 20-25 tasks across US1/US2 phases.

## Constitution Check

Post-Phase-0 recheck below. Initial pass:

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | PASS | No new deps; no C. |
| II. eBPF-Only Observation | N/A | User-space `sbom scan` path — orthogonal to eBPF trace. |
| III. Fail Closed | PASS | Reconciliation failures (e.g., a design-tier component with no valid PURL) fall through to standalone emission — never silently drops the component. |
| IV. Type-Driven Correctness | PASS | Reuses existing `ResolvedComponent` + `Purl` newtypes; no new `.unwrap()` in production code (new module uses `Result` throughout + guarded test-mod attribute per convention). |
| V. Specification Compliance | PASS + explicitly enforced by FR-018. Standards-native fields (CDX `.components[].properties[]` — multiple entries per FR-004, SPDX 2.3 `Annotation` + `Comment`, SPDX 3 `annotation` graph elements) carry all multi-declaration ranges. Zero new `mikebom:*` annotations. Audit result: `mikebom:requirement-range` + `mikebom:source-manifest` already exist (m127 / m179 / m183 / m184) — this milestone only changes WHERE they attach (source-tier survivor) and HOW MANY entries carry them (one per declaring manifest per Q1). |
| VI. Three-Crate Architecture | PASS | Changes limited to `mikebom-cli` (new `resolve/reconciler.rs` module) + shared type reuse from `mikebom-common`. No new crate. |
| VII. Test Isolation | PASS | All new tests are pure-Rust unit + integration; no eBPF privilege required. |
| VIII. Completeness | PASS | No components dropped — the reconciled design-tier component's metadata survives on the source-tier survivor; the standalone case emits the design-tier component with a spec-clean PURL. FR-005 explicitly forbids dangling graph edges. |
| IX. Accuracy | PASS | Reconciliation MATCH is deterministic (canonical PURL name + workspace scope per Q2). No heuristic guessing. |
| X. Transparency | PASS | FR-020 emits INFO summary + DEBUG per-component logs per Q4 answer B. Consumers dig into DEBUG level to trace individual reconciliation decisions. |
| XI. Enrichment | N/A | No external data source touched. |
| XII. External Data Source Enrichment | N/A | No external data source touched. |
| Strict Boundary 1 (No lockfile discovery) | N/A | Lockfiles are the source-tier discovery path, unchanged; the reconciliation pass consumes already-collected `ResolvedComponent`s. |
| Strict Boundary 2 (No MITM proxy) | N/A | Not a network change. |
| Strict Boundary 3 (No C code) | PASS | No C. |
| Strict Boundary 4 (No .unwrap() in production) | PASS | New module uses `Result` and `Option` patterns; test-mod guarded per convention. |
| Strict Boundary 5 (No file-tier duplicates in default mode) | N/A | Not a file-tier-emission change. |

**No violations. Proceed to Phase 0.**

## Project Structure

### Documentation (this feature)

```text
specs/191-design-source-reconcile/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output — reconciliation-shape + PURL-shape contracts
├── checklists/
│   └── requirements.md  # Created by /speckit-specify
└── tasks.md             # Created by /speckit-tasks (NOT this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── resolve/
│   │   ├── mod.rs                  # EXTEND: add `pub mod reconciler;`
│   │   ├── deduplicator.rs         # UNCHANGED — pre-existing (ecosystem, name, version, parent_purl) dedup
│   │   ├── pipeline.rs             # UNCHANGED — pipeline continues to call deduplicate() as before
│   │   └── reconciler.rs           # NEW: reconcile_design_source_tiers pass — group by
│                                    # (ecosystem, name, workspace-scope), transfer design-tier
│                                    # metadata onto source-tier survivor, rewrite dep-graph edges,
│                                    # emit INFO summary + DEBUG per-component logs.
│   ├── cli/
│   │   └── scan_cmd.rs             # WIRE: insert reconcile_design_source_tiers() call
│                                    # after the existing dedup at line ~2742, before graph-
│                                    # completeness / emission.
│   ├── scan_fs/
│   │   ├── mod.rs                  # WIRE: insert reconciler call after the first deduplicate
│                                    # at line ~807 for the scan-fs code path.
│   │   └── package_db/
│   │       ├── npm/mod.rs          # FIX: build_npm_purl — emit `pkg:npm/<name>` (no @) when
│                                    # version is empty. Same edit repeated per ecosystem below.
│   │       ├── pip/                # FIX (mod.rs or equivalent build_pypi_purl)
│   │       ├── cargo.rs            # FIX (build_cargo_purl)
│   │       ├── maven.rs            # FIX (build_maven_purl)
│   │       ├── gem.rs              # FIX (build_gem_purl)
│   │       ├── pnpm_lock.rs        # (design-tier PURL construction if the pnpm reader has its own)
│   │       ├── yarn_lock.rs        # (same for yarn)
│   │       └── composer.rs         # FIX (build_composer_purl)
│   └── parity/
│       └── extractors/
│           ├── cdx.rs              # LIKELY-CLEAN: C20 (mikebom:requirement-range) is an
│                                    # annotation-array extractor; if the row already returns
│                                    # `Vec<Value>` (all matching properties), no change needed.
│                                    # If it returns only the first match, adjust.
│           ├── spdx2.rs            # Same check.
│           └── spdx3.rs            # Same check.
└── tests/
    ├── design_source_reconcile.rs           # NEW: happy-path + multi-declaration + workspace-
                                             # parent walk (Q2) + graph-edge rewriting + standalone
                                             # optionalDep path.
    ├── design_tier_versionless_purl.rs      # NEW: US2 PURL-shape assertions across CDX/SPDX2.3/SPDX3.
    └── existing goldens/                    # UNCHANGED: byte-identity where no design/source pairs;
                                              # regenerate where such pairs exist per FR-016.
```

**Structure Decision**: New `resolve/reconciler.rs` module co-located with the existing `deduplicator.rs`. The reconciliation is conceptually a second dedup pass with a different key + different merge rules (info transfer vs identity-hash merge). Distinct-module approach:

- Makes the two-pass architecture obvious in the file tree.
- Simplifies reviewer digestibility (the two rulesets aren't interleaved).
- Enables independent unit testing without cross-contamination.
- Matches the m149 / m179 / m183 / m184 pattern of "one new module per emit-time transformation".

## Complexity Tracking

*No constitution violations — table intentionally empty.*
