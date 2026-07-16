# Implementation Plan: Reconciler Always-Array Shape + npm-Alias Resolved-Identity Matching

**Branch**: `199-reconciler-array-alias` | **Date**: 2026-07-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/199-reconciler-array-alias/spec.md`

## Summary

Rewrite the m191 reconciler's declaration-provenance transfer logic in-
place at `mikebom-cli/src/resolve/reconciler.rs:85-105` to emit JSON
arrays instead of scalar strings (US6 always-array shape), and extend
the same transfer path to alias-aware matching + `mikebom:declared-as`
accumulation (US5). Both stories land together because they touch the
same transfer-site code block and share the array-emission pattern.

Zero new Cargo deps. Reuses the existing `AliasResolution.local_name`
+ `AliasResolution.aliased_name` fields (m159 pattern) — the alias
data is already extracted; m199 just plumbs it into the design-tier
component's `mikebom:declared-as` annotation and teaches the reconciler
to match by resolved identity when that annotation is present.

**REVISED at implement-phase (2026-07-16)**: research R4's "0 goldens"
finding was empirically wrong — the initial grep pointed at
`fixtures/golden/` which doesn't exist. The correct golden directory
`fixtures/public_corpus/` (per m195/m196) contains 234 singular-scalar
hits across 9 files (python-flask + maven-guice + npm-express × 3
formats), and 3 emitter sites (`cyclonedx/builder.rs:1079`,
`spdx/annotations.rs:318`, `spdx/v3_annotations.rs:333`) plus 10 test-
site assertions reference the singular-scalar shape. Per user decision
at scope-drift disposition, m199 executes FULL SCHEMA ROTATION (Option 1):
- Rename `ResolvedComponent.requirement_range: Option<String>` →
  `requirement_ranges: Vec<String>` at `mikebom-common/src/resolution.rs:82`.
- Migrate ~25 default-init sites + the 3 emitter reads + the 2 direct
  writers (haskell + npm) + the 6 reconciler sites.
- Regenerate all 9 public-corpus goldens.
- Update all 10 test assertions.

**LOC revision**: original ~500-LOC estimate is superseded — actual scope
is ~1200-1500 LOC (dominated by 234 golden bytes + mechanical init-site
updates). See research.md R4 REVISED for the full breakdown.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–198; no nightly).
**Primary Dependencies**: Existing only — `mikebom_common::resolution::ResolvedComponent` (survivor type), `serde_json::Value` (annotation array construction), the m159 `AliasResolution` struct at `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs:37` with `local_name` (alias) + `aliased_name` (resolved) fields already distinct. Zero new crates.
**Storage**: N/A — all state is in-process per scan; matches every reader milestone since 002.
**Testing**: New unit tests in `reconciler.rs::tests` (always-array + alias-aware matching) + new integration fixtures under `mikebom-cli/tests/fixtures/npm/{alias,multi-declaration}/` + new scan_npm.rs integration tests per m197 US5/US6 acceptance scenarios. Zero existing goldens require regen (per plan reconnaissance).
**Target Platform**: Same as mikebom itself.
**Project Type**: Reconciler augmentation. ~150 LOC change to `reconciler.rs` + ~50 LOC change to `alias_mapping.rs` + npm reader plumbing to stamp `mikebom:declared-as` on design-tier + ~200 LOC new tests + ~100 LOC new fixtures. Roughly 500 LOC total.
**Performance Goals**: No perf regression beyond FR-009 (`./scripts/pre-pr.sh` wall-clock delta ≤ 5s vs pre-m199 baseline per SC-007).
**Constraints**: (a) zero new Cargo deps; (b) `mikebom:declared-as` new annotation audited under Principle V — no CDX/SPDX native alternative exists for alias-provenance semantic (audit inherited from m197 plan constitution check); (c) singular m191 scalars `mikebom:requirement-range` / `mikebom:source-manifest` MUST NOT appear anywhere in post-m199 emitted output (FR-001); (d) non-npm ecosystems unchanged per spec Assumption 3.
**Scale/Scope**: 1 reconciler file + 1 npm alias-mapping file + 1 npm design-tier-emission site + ~4 test files. Small, focused change surface.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. Pure Rust throughout.
- **II. eBPF-Only Observation** — ✅ N/A.
- **III. Fail Closed** — ✅ PASS. Annotation-emission failures propagate; no silent drops.
- **IV. Type-Driven Correctness** — ✅ PASS. Arrays typed as `serde_json::Value::Array(Vec<Value::String>)`; `mikebom:declared-as` stored via the same shape.
- **V. Specification Compliance** — ✅ PASS. `mikebom:declared-as` new annotation audit inherited from m197 plan — no CDX/SPDX native alternative for the alias-provenance semantic. Array-rotation of existing annotations is a shape change to an existing `mikebom:*` field, not a new construct.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes stay in `mikebom-cli`.
- **VII. Test Isolation** — ✅ PASS. Every new fixture is `tempfile::tempdir()`-scoped.
- **VIII. Completeness** — ✅ PASS. Closes two documented gaps (#564 alias handling, #565 multi-declaration preservation).
- **IX. Accuracy** — ✅ PASS. Reconciliation by resolved identity is MORE accurate than by alias name.
- **X. Transparency** — ✅ PASS. `mikebom:declared-as` provides transparency about the alias-vs-resolved-identity mapping; array-shape change is documented in FR-001 with consumer migration note.
- **XI. / XII. Enrichment** — ✅ N/A.
- **Strict Boundary §5 (file-tier)** — ✅ PASS.

**Result**: All principles PASS. No violations.

**Post-Phase-1 re-check**: N/A here — Phase 1 introduces no new entities beyond what m197's data-model already documented (E1 `mikebom:declared-as`, E2/E3 always-array pluralized fields, `AliasResolution` extension). Constitution gate trivially remains PASS post-design.

## Project Structure

### Documentation (this feature)

```text
specs/199-reconciler-array-alias/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 4 mechanical decisions
├── data-model.md        # Phase 1 output — annotation shapes (inherited from m197)
├── quickstart.md        # Phase 1 output — 3 reproducers per US
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

No `contracts/` sub-directory. The 3 annotation-shape wire contracts across CDX / SPDX 2.3 / SPDX 3 are already documented in `specs/197-purl-reconciler-followups/contracts/annotation-shapes.md` (still in the tree from the merged m197 spec); m199 inherits by reference.

### Source Code (repository root)

```text
mikebom-cli/src/resolve/
└── reconciler.rs                                # MODIFIED — US5 + US6:
                                                 #   Lines 85-105 transfer logic rewrite (US6 always-array)
                                                 #   Match-key alias-aware branch (US5)
                                                 #   `mikebom:declared-as` accumulation (US5)

mikebom-cli/src/scan_fs/package_db/npm/
├── alias_mapping.rs                             # POSSIBLY MODIFIED — US5:
                                                 #   Add `parse_package_json_alias()` for the
                                                 #   `"my-alias": "npm:actual@ver"` form (mirroring the
                                                 #   existing `detect_pnpm_alias` shape from m159)
├── package_lock.rs                              # POSSIBLY MODIFIED — US5:
                                                 #   Stamp `mikebom:declared-as: [<alias>]` on
                                                 #   design-tier component emission when alias detected
└── mod.rs                                       # POSSIBLY MODIFIED — US5:
                                                 #   Wire alias-emission into the m066 mainmod path
                                                 #   for package.json alias declarations

mikebom-cli/tests/fixtures/npm/
├── alias/                                       # NEW — US5 fixture per quickstart Reproducer 2
│   ├── package.json                             #   `"my-alias": "npm:actual-pkg@1.0.0"` declaration
│   └── package-lock.json                        #   resolving to `actual-pkg@1.0.0`
└── multi-declaration/                           # NEW — US6 fixture per quickstart Reproducer 3
    ├── package.json                             #   workspace root
    ├── packages/
    │   ├── foo/package.json                     #   `"commander": "^11.0"` declaration
    │   └── bar/package.json                     #   `"commander": "^11.1.0"` declaration
    └── package-lock.json                        #   root lockfile resolving both to commander@11.1.0

mikebom-cli/tests/
└── scan_npm.rs                                  # MODIFIED — 2 new integration tests:
                                                 #   scan_npm_alias_reconciles_by_resolved_identity (US5)
                                                 #   scan_npm_multi_declaration_preserves_all_ranges (US6)
```

**Structure Decision**: In-place augmentation of 3-4 existing source files + 2 new fixture directories + 2 new integration tests. Zero existing goldens require regen (per plan reconnaissance). Small, focused change surface.

## Complexity Tracking

No constitution violations. All principles pass on first check. `mikebom:declared-as` audit under Principle V inherited from m197 plan.
