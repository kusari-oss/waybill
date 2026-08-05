# Implementation Plan: Complete design-tier SBOM documentation in ecosystems.md

**Branch**: `227-design-sbom-docs` | **Date**: 2026-08-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/227-design-sbom-docs/spec.md`

## Summary

Pure docs milestone. Primary deliverable is a new dedicated section inside the existing `docs/ecosystems.md` explaining the three SBOM tiers (source / design / binary), the per-ecosystem design-tier fallback matrix, consumer-facing `jq` recipes, and the "when design-tier is enough vs when it isn't" guidance. Plus a per-ecosystem link from each of the ~30 existing per-reader sections back to the new tier section, so operators reading a single ecosystem section can find the design-tier semantics for that ecosystem without hunting.

The three-tier model already exists in code (the `sbom_tier: Some("design")` pattern across 15+ readers, and the milestone-158 graph-completeness annotation depends on it). This milestone documents the model as-is — no new tier taxonomy, no new CLI flags, no new emission behavior. The 2026-08-04 NuGet audit and its three follow-up PRs (#656/#657/#658) already touched every gap on the NuGet side; that work motivates a general documentation catch-up for every ecosystem.

Total code surface: zero Rust LOC. One primary docs file touched (`docs/ecosystems.md`) plus optional light updates to `docs/reference/reading-a-waybill-sbom.md` for cross-linking. Estimated deliverable: ~300–500 lines of new markdown inside `ecosystems.md` (per spec SC-005 upper bound of 500 lines for the new tier section), plus per-ecosystem inline cross-references.

## Technical Context

**Language/Version**: N/A — Markdown documentation. The waybill binary is unchanged; every reader, every emitter, every CLI flag remains byte-identical pre/post merge.
**Primary Dependencies**: None new. Documentation targets GitHub-flavored Markdown rendered by the standard `docs/` viewer per existing convention.
**Storage**: N/A.
**Testing**: `cargo +stable test --workspace` and `cargo +stable clippy --workspace --all-targets -- -D warnings` are effectively no-ops for this milestone (no Rust source change), but per project convention `./scripts/pre-pr.sh` MUST still exit 0 before PR open. Any `jq` recipe embedded in the doc MUST be verified against a real waybill-emitted SBOM at doc-authoring time (spec FR-003 + SC-002); the recipe verification runs manually, not as an automated test.
**Target Platform**: Markdown rendering on GitHub + any standard CommonMark renderer.
**Project Type**: Documentation reference — per-ecosystem operator+contributor reading surface, complemented by the consumer-facing `docs/reference/reading-a-waybill-sbom.md` (m150–151) for downstream-tool authors.
**Performance Goals**: N/A.
**Constraints**:
- **Docs-only** (spec Assumptions §1): zero Rust source change; no CLI flag change; no emission change. If a gap in operator ergonomics is discovered during writing (e.g., "we should have a `--tier=design-only` filter flag"), it becomes a **follow-up issue**, not part of this milestone.
- **Three-tier model is settled** (spec Assumptions §2): `source` / `design` / `binary` are the labels used in code today. Documentation aligns with this vocabulary; it does NOT propose a new tier taxonomy.
- **Constitution Principle V** (standards-native > `waybill:*`): the new section MUST be explicit that `waybill:sbom-tier` is a `waybill:*` property — introduced because no native CDX 1.6 / SPDX 2.3 / SPDX 3 field carries the "was this version resolved or declared-only" semantic. The doc points to `docs/reference/sbom-format-mapping.md` for the parity-catalog row justifying this.
- **Cross-reference invariant** (spec FR-010): every claim about a specific reader's design-tier behavior in the new documentation MUST be verifiable against the current state of waybill's source code as of the merge time. The memory `feedback-verify-research-empirical-claims` applies — a grep or a Read of the actual source code, not a recall from memory, backs every reader-specific claim.
- **Line-budget** (spec SC-005): the new dedicated tier section stays ≤ 500 lines of markdown. Per-ecosystem inline cross-references count against the existing per-ecosystem sections, not the new-section budget.
- **jq recipes target jq 1.6+** (spec Assumptions §4): same convention as m150 / m151 consumer guide.
- **Pre-PR gate**: `./scripts/pre-pr.sh` MUST exit 0 before PR open. No `jq` behavior is asserted by the CI suite — recipes are hand-verified.
**Scale/Scope**: Reaches every waybill operator scanning a source tree AND every downstream tool consuming a waybill-emitted SBOM AND every future contributor implementing a new ecosystem reader. Effort budget: ~300–500 lines of new markdown + ~30 cross-reference edits to per-ecosystem sections.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ N/A | Documentation only. |
| II | eBPF-Only Observation | ✅ N/A | No dependency-discovery code touched. |
| III | Fail Closed | ✅ N/A | No emission behavior change. |
| IV | Type-Driven Correctness | ✅ N/A | No Rust code touched. |
| V | Specification Compliance | ✅ **REINFORCES** | The new section EXPLICITLY explains why `waybill:sbom-tier` is a `waybill:*` property (no native CDX/SPDX field carries the design-vs-source-vs-binary semantic). This makes Principle V's standards-native-precedence rule visible to consumers. |
| VI | Three-Crate Architecture | ✅ N/A | |
| VII | Test Isolation | ✅ N/A | No test suite change. |
| VIII | Completeness | ✅ **IMPROVES (indirectly)** | The documentation helps consumers correctly interpret the m158 graph-completeness annotation — specifically, that "partial completeness" can be caused by design-tier fallback OR by unreachable-from-root orphans, and how to distinguish the two classes. This surfaces existing completeness signals without adding new ones. |
| IX | Accuracy | ✅ **REINFORCES** | The doc's "when design-tier is enough vs when it isn't" subsection is a Principle IX artifact — it explicitly warns that running exact-version CVE matches on design-tier components produces false-negative silent misses. Consumers acting on this guidance preserve waybill's accuracy signal-to-noise ratio. |
| X | Transparency | ✅ **DIRECTLY ADVANCES** | The doc IS a transparency artifact — it names the `waybill:sbom-tier` and `waybill:unresolved-reason` signals, tells consumers exactly how to read them, and connects them to the m158 graph-completeness annotation. Principle X specifically calls for structured metadata that informs consumers of limitations; this doc closes the operator-facing loop of that principle. |
| XI | Enrichment | ✅ N/A | |
| XII | External Data Source Enrichment | ✅ N/A | |
| SB-1 | No lockfile-based discovery | ✅ N/A | Doc explains that lockfile-driven resolution is enrichment per Principle XII, not discovery — reinforcing SB-1 to consumers. |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ N/A | |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | File-tier is out of scope for this milestone; the new section covers source-vs-design tier only. File-tier semantics remain documented at `docs/reference/component-tiers.md`. |

**All gates pass.** Principles V, VIII, IX, X are REINFORCED or DIRECTLY ADVANCED. This milestone is the docs analog of milestone 150's consumer guide — per-ecosystem operator+contributor reference rather than consumer-onboarding narrative.

## Project Structure

### Documentation (this feature)

```text
specs/227-design-sbom-docs/
├── plan.md              # This file
├── research.md          # Phase 0 — per-reader design-tier trigger conditions (30+ readers grepped), waybill:unresolved-reason value inventory, jq recipe verification runbook
├── data-model.md        # Phase 1 — section-structure entities: tier concept, per-ecosystem row shape, jq-recipe shape, cross-reference-link contract
├── quickstart.md        # Phase 1 — first-time-reader walkthrough that exercises the 5-ecosystem SC-001 prediction test
├── contracts/
│   └── doc-structure.md # Phase 1 — TOC + per-subsection content contract for the new ecosystems.md tier section
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify (all items ✅)
```

### Source Files (repository root)

Touched files (narrow scope — docs-only):

```text
docs/
├── ecosystems.md                              # PRIMARY UPDATE — new dedicated tier section (~300–500 lines) + per-ecosystem cross-references from each existing reader section (~30 edits of 1–3 lines each)
└── reference/
    └── reading-a-waybill-sbom.md              # OPTIONAL cross-reference addition — 1–3 lines pointing to the new ecosystems.md tier section from the tier-concept passage in the consumer guide (if such a passage exists; verified during research phase)
```

No code files touched. No fixtures. No test files. No parity-catalog rows. No CLI flag changes.

**Structure Decision**: All primary changes land inside the existing `docs/ecosystems.md` file. The new tier section is a top-level `##` heading placed near the top (after the coverage matrix, before the first per-ecosystem section) so operators reading the doc top-to-bottom encounter the conceptual framing before the per-ecosystem detail. Every per-ecosystem section that already discusses "design-tier" behavior in-line (e.g., pants shell, pants Go, kotlin) keeps its inline description AND adds a cross-reference back to the new section. Every per-ecosystem section that does NOT yet discuss its design-tier behavior gets a new 1–3-line "Design-tier fallback" subsection that links to the new tier section for the general framing. No new files are created outside the `specs/227-design-sbom-docs/` planning directory.

## Complexity Tracking

*Not applicable.* All Constitution gates pass cleanly. Pure docs milestone with clear per-ecosystem verification path (grep the reader source, verify the claim). The only real design tradeoffs surface in Phase 0 research:
1. **How much per-ecosystem detail to inline vs delegate to the new tier section** — resolved during Phase 0 by picking a per-reader "brief mention + link" template that keeps per-ecosystem sections skimmable without duplicating tier-concept content.
2. **Whether to add a Coverage-matrix column for design-tier trigger conditions** — spec FR-002 offers "populated column added to the existing matrix" as one option; resolved during Phase 0 based on matrix column-count width (adding a 5th column may overflow on narrow-screen renders; if so, the per-ecosystem detail lives in the new section only, and the matrix gets a single "see tier section" indicator column).
