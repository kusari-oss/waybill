# Implementation Plan: Expand consumer-guide depth coverage — milestone 151

**Branch**: `151-expand-consumer-guide` | **Date**: 2026-06-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/151-expand-consumer-guide/spec.md`

## Summary

Extend `docs/reference/reading-a-mikebom-sbom.md` (the milestone-150 consumer guide) with depth coverage for 6 tier-1 signals that the milestone-150 selection missed — `mikebom:evidence-kind`, `mikebom:confidence`, `mikebom:linkage-kind`, `mikebom:not-linked`, `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved` (paired), `mikebom:assertion-conflict` — and add a written **decision rubric** (3–5 yes/no criteria with a documented threshold N) so future maintainers can apply the depth-vs-appendix decision mechanically. This is a docs-only milestone, single-file edit, mirroring milestone-150's shape (per-signal rendering invariant, 4-cluster organization, jq recipes, `verify-recipes.sh` authoring harness). Authoring artifact at `specs/151-expand-consumer-guide/verify-recipes.sh` validates the new jq recipes against real mikebom-emitted SBOMs at authoring time.

## Technical Context

**Language/Version**: N/A — Markdown documentation only. No Rust source touched. (The mikebom binary is unchanged; its CLI surface, library APIs, and emitted SBOM wire formats are all stable across this milestone per FR-016 / FR-017.)
**Primary Dependencies**: Existing only — `jq` (for recipe verification at authoring time), the released `mikebom` binary at workspace HEAD (for generating real SBOMs the recipes run against), `bash` (for `verify-recipes.sh`). No new Cargo, Python, or Node dependencies.
**Storage**: N/A — purely documentation. The verify-recipes.sh harness writes scratch SBOMs to `mktemp -d` and cleans up on exit (matches milestone 150's harness pattern).
**Testing**: N/A — no Rust test suite changes. The `verify-recipes.sh` harness is an authoring artifact, NOT a CI-gated test (mikebom doesn't ship public CI gates for jq-recipe correctness per Assumption 4 in the spec).
**Target Platform**: Markdown rendered by GitHub + mdBook (docs.kusari.dev). Same as milestone 150.
**Project Type**: Single-file documentation update + a Bash authoring artifact in the milestone's `specs/` directory.
**Performance Goals**: N/A — no runtime perf considerations. The `verify-recipes.sh` harness aims for ≤2 minutes wall time end-to-end (mirroring milestone 150) so authoring iteration stays tight.
**Constraints**: SC-007 single-file deliverable — only `docs/reference/reading-a-mikebom-sbom.md` is edited in the shipped diff. The verify-recipes.sh + standard speckit branch artifacts (spec/plan/research/data-model/quickstart/tasks/contracts/checklists) are accepted as scaffolding around the deliverable. No other doc files, no Rust source, no CI workflows touched.
**Scale/Scope**: Adds ~250 LOC to the existing ~586-line doc (6 new depth-covered sections × ~30 LOC each + ~50-line rubric section + ~20 LOC of cross-reference updates). Final doc size ≈ 850 lines, still well under the 2000-line read-without-tools threshold for SC-001 maintainer-cadence review.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.5.0 evaluation against this milestone's deliverable:

| Principle | Applicability | Status | Notes |
|-----------|---------------|--------|-------|
| I. Pure Rust, Zero C | N/A | PASS | Docs-only milestone; no source code changes. |
| II. eBPF-Only Observation | N/A | PASS | No discovery/enrichment logic touched. |
| III. Fail Closed | N/A | PASS | No emission paths touched. |
| IV. Type-Driven Correctness | N/A | PASS | No Rust code added or modified. |
| **V. Specification Compliance** | **APPLIES** | **PASS** | The depth-coverage sections MUST accurately reflect the per-format placement documented in catalog rows C4 / C12 / C16 / C41 / C67 / C77 / C78 — the catalog is source of truth, this milestone consumes it, doesn't extend it (FR-018). The "standards-native fields take precedence over `mikebom:*`" rule (added in v1.4.0) is reinforced by the depth-coverage sections, which note the parity-bridge justification for each annotation where applicable (matches the catalog row's existing Principle V audit clause). |
| VI. Three-Crate Architecture | N/A | PASS | No crates added; no Cargo.toml touched. |
| VII. Test Isolation | N/A | PASS | No test suite changes; verify-recipes.sh is an authoring artifact, not a CI-gated test. |
| VIII. Completeness | N/A | PASS | No discovery logic touched. |
| IX. Accuracy | N/A | PASS | No emission logic touched. |
| **X. Transparency** | **APPLIES** | **PASS** | The 6 newly-depth-covered signals ARE part of the transparency surface (especially `mikebom:assertion-conflict` from milestone 119, `mikebom:depends-unresolved` from milestone 128, `mikebom:not-linked` from milestone 050, and the trust trio). Better-documented transparency signals reinforce Principle X by giving consumers the tools to assess SBOM trust at scale. |
| XI. Enrichment | N/A | PASS | No enrichment logic touched. |
| XII. External Data Source Enrichment | N/A | PASS | No enrichment paths touched. |
| Strict Boundary 5 (no file-tier duplicates in default mode) | N/A | PASS | The depth-coverage for `mikebom:file-inventory-mode` shipped in milestone 150 already establishes the consumer-side documentation of the boundary; this milestone doesn't disturb it. |

**Gate Outcome**: PASS. No violations. No complexity-tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/151-expand-consumer-guide/
├── plan.md                       # This file
├── research.md                   # Phase 0 — rubric criteria draft + per-signal placement audit
├── data-model.md                 # Phase 1 — rendering invariant + rubric structure + harness shape
├── quickstart.md                 # Phase 1 — operator-cadence read-through + recipe-verification flow
├── contracts/
│   └── rubric.md                 # Phase 1 — the rubric's formal yes/no shape as a contract
├── checklists/
│   └── requirements.md           # Spec validation checklist (from /speckit-specify)
├── verify-recipes.sh             # Authoring artifact — extends milestone 150's harness pattern
└── tasks.md                      # Phase 2 — generated by /speckit-tasks (NOT created here)
```

### Source Code (repository root)

No Rust source paths touched. The shipped diff is one file:

```text
docs/reference/
└── reading-a-mikebom-sbom.md     # +~250 LOC: 6 new depth-coverage sections + rubric section + appendix cross-ref updates
```

**Structure Decision**: Single-file docs deliverable + authoring artifacts in `specs/151-expand-consumer-guide/`. Mirrors milestone 150's shape exactly. No new directories, no new file kinds, no precedent breaks.

## Constitution Check — POST-DESIGN re-evaluation

Phase 0 (research.md) + Phase 1 (data-model.md, contracts/rubric.md, quickstart.md) produced no surprises that change the constitution evaluation:

- The rubric (research.md §R1, contracts/rubric.md) consumes the catalog without extending it — Principle V's standards-native-precedence rule is preserved verbatim by the rubric's C5 criterion (which tests "wire shape requires documentation beyond the catalog row" against the catalog as source of truth).
- The depth-coverage sections (data-model.md §1) reuse the milestone-150 rendering invariant; no new emission shapes, no new annotation keys (FR-016), no new wire shapes (FR-017).
- The 6 newly-depth-covered signals (research.md §R2) are documented against existing catalog rows C4 / C12 / C16 / C41 / C67 / C77 / C78 — placement is cited, not invented.
- The US5 audit (research.md §R9) reads emission sites in `mikebom-cli/src/generate/` + `mikebom-cli/src/scan_fs/` but does not modify them — it informs the appendix-hygiene decision documented in the PR.
- The `verify-recipes.sh` extension (research.md §R8) is an authoring artifact in the milestone's `specs/` directory; per Principle VII it is NOT a CI-gated test, so test-isolation concerns do not apply.

**Post-design gate outcome**: PASS. No new violations surfaced. No complexity-tracking entries needed.

## Complexity Tracking

No constitution-gate violations to justify. This milestone is a strict extension of milestone 150's already-approved shape.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _(none)_  | _(none)_   | _(none)_                            |
