# Implementation Plan: pip / poetry / uv optional-dependency classification

**Branch**: `183-pip-extras-optional` | **Date**: 2026-07-10 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/183-pip-extras-optional/spec.md`

## Summary

Extends the m179+m180+m181 unified optional-dependency classification to the Python ecosystem's three pip-family lockfile / manifest sources. All three converge on the same `LifecycleScope::Optional` variant + the shared `mikebom:optional-derivation = "pip-optional-dependencies"` C122 parity annotation.

**Technical approach**: Small, additive changes at three isolated code sites in `mikebom-cli/src/scan_fs/package_db/pip/`:

- **poetry.rs (US1)** — Change the classifier at `poetry.rs:67` from a two-arm match on `poetry_is_dev` to a three-arm match that also consults `tbl.get("optional")` and applies dev-wins-over-optional precedence. Zero new helpers.
- **pip/mod.rs (US2)** — Split the flat `depends` list construction at `pip/mod.rs:474` into two lists (regular + optional), thread the optional-name set into a downstream classifier pass that marks matching child components as `LifecycleScope::Optional`. Reuses the m179 `apply_lifecycle_scope_to_edges` infrastructure.
- **uv_lock.rs (US3)** — Add a `[[package]].optional-dependencies.<extra>` sub-table walk to the existing `[[package]]` iteration. Reuses the same optional-name-set → downstream-classifier pass from US2.

Zero new production Cargo dependencies. The C122 parity catalog row registered by m179 gains one new expected value (`"pip-optional-dependencies"`), tracked as a value-set update, not a new catalog row.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–182; no nightly required for this user-space-only classification work).

**Primary Dependencies**: Existing only — `toml = "0.8"` (already used by cargo + pip parsers throughout `scan_fs/package_db/`), `serde`/`serde_json` (annotation values), `tracing` (info-level classifier-decision logs), `anyhow`/`thiserror` (error propagation). Reuses m179's `LifecycleScope::Optional` variant + `RelationshipType::OptionalDependsOn` + `SpdxRelationshipType::OptionalDependencyOf` + m180's C122 parity catalog row + m180's `apply_lifecycle_scope_to_edges` at `mikebom-cli/src/scan_fs/mod.rs:1261`. **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM via the existing `extra_annotations` channel + `lifecycle_scope` field. Matches every milestone since 002.

**Testing**: `cargo +stable test --workspace` (unit + integration tests), `cargo +stable clippy --workspace --all-targets -- -D warnings` (lint). Golden regen via `MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`.

**Target Platform**: Linux + macOS user-space (unchanged from prior milestones). Windows-host smoke covered by milestone 100/101 infra.

**Project Type**: CLI (Rust binary + shared common crate). Existing three-crate architecture: `mikebom-cli`, `mikebom-common`, `xtask`.

**Performance Goals**: Classifier change adds O(N) work per pip-family lockfile / manifest during scan; N = number of `[[package]]` entries or `[project.optional-dependencies].<extra>` array elements. No new subprocess calls, no new I/O, no network. Amortized per-scan cost is negligible relative to the existing lockfile parse.

**Constraints**: SC-004 byte-identity for non-pip fixtures + pip fixtures with zero optional-declared deps — the default classifier path (no `optional = true`) MUST produce byte-identical `PackageDbEntry` values compared to pre-m183. Golden regens gate this.

**Scale/Scope**: 3 user stories, 3 code sites, 1 shared derivation-annotation value. Estimated ~25-30 tasks across 6 phases.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Principle I (Pure Rust, Zero C)** — PASS. Zero new Cargo dependencies. Existing `toml` + `serde_json` + `tracing` cover all parsing + annotation-emission needs. No C transitives.

**Principle II (eBPF-Only Observation)** — N/A. m183 is user-space classifier work; `mikebom-ebpf` untouched.

**Principle III (Fail Closed)** — PASS. Every classifier decision has a documented default (no `optional` flag → Runtime, matching pre-m183 behavior). Unparseable poetry.lock / uv.lock entries continue to `warn!` and skip (existing behavior). No new fail-open code paths.

**Principle IV (Type-Driven Correctness)** — PASS. Classifier decisions flow through the existing `LifecycleScope` enum. No stringly-typed magic. FR-006's lockfile-precedence rule is codified as a comparator between two typed sources, not a string compare.

**Principle V (Specification Compliance + Native-first)** — PASS. Native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` is the primary signal (elevated from the current `DEPENDS_ON`). `mikebom:optional-derivation = "pip-optional-dependencies"` is the KEEP-BOTH supplement carrying WHICH pip-family construct produced the classification. Zero new `mikebom:*` annotations invented — the value-set of an existing C122 catalog row is extended.

**Principle VI (Three-Crate Architecture)** — PASS. Changes are confined to `mikebom-cli/src/scan_fs/package_db/pip/`. Zero changes to `mikebom-common`, `mikebom-ebpf`, or `xtask`.

**Principle VII (Test Isolation)** — PASS. New unit tests in `poetry.rs`, `pip/mod.rs`, `uv_lock.rs` colocated with the code under test. Integration tests via existing pip regression fixtures + new fixtures for the three US flavors.

**Principle VIII (Completeness)** — PASS. m183 closes the remaining pip-family filter-parity gap. Deferred cases (setup.py, requirements.txt, workspace-member cross-reference) documented in spec.md.

**Principle IX (Accuracy)** — PASS. US1 corrects a silent misclassification bug (`optional = true` → Runtime) that has been misleading SBOM consumers since m179. SC-001/002/003 set-equality gates enforce filter-parity per pico's needs.

**Principle X (Transparency)** — PASS. C122 parity annotation carries the classifier's decision provenance across all three formats byte-identically. `evidence.source_file_paths` field points at the specific lockfile / manifest that produced the classification.

**Principle XI (Enrichment)** — N/A. No external-data enrichment for m183.

**Principle XII (External Data Source Enrichment)** — N/A. Same as XI.

**Result**: All 12 principles PASS. No violations to justify. No Complexity Tracking table needed.

## Project Structure

### Documentation (this feature)

```text
specs/183-pip-extras-optional/
├── plan.md              # This file
├── research.md          # Phase 0 output (5 decisions)
├── data-model.md        # Phase 1 output (classifier decision matrix + FR-002 plumbing choice)
├── quickstart.md        # Phase 1 output (operator + developer worked examples)
├── contracts/
│   ├── classifier-decision-matrix.md  # US1/US2/US3 canonical classification tables
│   └── derivation-value-set.md         # C122 catalog value-set update
├── checklists/
│   └── requirements.md   # 16/16 PASS from /speckit-specify
├── spec.md              # Feature specification
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           └── pip/
│               ├── mod.rs           # US2 — build_pip_main_module_entry split + downstream classifier pass
│               ├── poetry.rs        # US1 — classifier at line 67 consults `optional` field
│               ├── uv_lock.rs       # US3 — add [[package]].optional-dependencies.<extra> walk
│               ├── pipfile.rs       # UNCHANGED — no first-class optional syntax
│               ├── requirements_txt.rs  # UNCHANGED — no first-class optional syntax
│               └── dist_info.rs     # UNCHANGED — installed-package tier, not lockfile
└── tests/
    └── fixtures/
        └── golden/
            ├── cyclonedx/pip.cdx.json      # regen: additive on optional-classified deps
            ├── spdx-2.3/pip.spdx.json      # regen: net-increment on OPTIONAL_DEPENDENCY_OF
            └── spdx-3/pip.spdx3.json       # regen: additive annotations
```

**Structure Decision**: Single-crate, three-file scope inside `mikebom-cli/src/scan_fs/package_db/pip/`. No cross-crate coordination. Reuses the m180's `apply_lifecycle_scope_to_edges` post-pass at `mikebom-cli/src/scan_fs/mod.rs:1261` — no new plumbing at the workspace layer. Follows the m180+m181 shape verbatim.

## Complexity Tracking

*No violations to justify — all 12 constitution principles PASS.*
