# Implementation Plan: SBOM consumer-facing reading guide — documenting mikebom annotations and differentiators

**Branch**: `150-sbom-consumer-guide` | **Date**: 2026-06-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/150-sbom-consumer-guide/spec.md`

## Summary

Pure docs milestone. Deliverable is a single new file at `docs/reference/reading-a-mikebom-sbom.md` (~600–900 lines estimated) plus a one-line addition to `docs/index.md`'s **Reference material** list. No Rust source code change, no CLI flag change, no wire-format change. Per the 2026-06-29 clarification (Q1 Option D), the doc avoids naming specific competing SBOM tools — the framing is "what mikebom emits and how to use it" rather than competitive comparison. This sidesteps the verification burden of pinning external tool versions / behaviors AND keeps the doc evergreen against external-tool drift.

Phase 0 research (§A through §D below) confirmed:
1. **102 unique `mikebom:*` annotation keys** exist in the catalog at milestone-150 ship time (catalog spans C1–C102 plus a handful of duplicate-named annotations across catalog rows). The appendix index covers all 102. Depth coverage is reserved for a curated subset — the 8–15 most consumer-actionable signals per SC-006.
2. **Envelope schema canonical location**: `mikebom-cli/src/generate/spdx/annotations.rs:31-67` defines `ENVELOPE_SCHEMA_V1 = "mikebom-annotation/v1"` + `MikebomAnnotationCommentV1` struct (fields `schema`, `field`, `value`). The decoder is at `mikebom-cli/src/parity/extractors/common.rs:185`. The doc links to BOTH as canonical references; no new JSON Schema artifact is published in this milestone.
3. **Thematic clustering**: 4 clusters land naturally per spec FR-003 — (a) vulnerability scanning, (b) compliance auditing, (c) build provenance, (d) transparency/completeness gaps. Each cluster pulls 2–4 specific signals from the 102-key catalog.
4. **Cross-references**: 5 existing reference docs get linked from the new doc — `sbom-format-mapping.md` (catalog), `identifiers.md` (`mikebom:identifiers` + `repo:`/`git:`/`image:`), `sbom-types.md` (`--sbom-type` + per-tier semantics), `component-tiers.md` (file-tier coverage + `mikebom:component-tier`), `cross-tier-binding.md` (`mikebom:source-document-binding`). The doc summarizes each topic in ~1 paragraph + delegates depth to the linked refs.

Total code surface: zero Rust LOC. Two docs files touched (`reading-a-mikebom-sbom.md` NEW + `index.md` 1-line addition). Estimated effort: ~600–900 lines of Markdown across the new doc.

## Technical Context

**Language/Version**: N/A — Markdown documentation. The mikebom binary is unchanged.
**Primary Dependencies**: None. The doc is static reference Markdown.
**Storage**: N/A.
**Testing**: `cargo +stable test --workspace` is a no-op for this milestone (no Rust source change), but the pre-PR gate (`./scripts/pre-pr.sh`) is still expected per project convention (SC-007). The `sbom_format_mapping_coverage` test continues to pass (it asserts every emitted `mikebom:*` field has a catalog row — the catalog itself is unchanged).
**Target Platform**: Markdown rendering on GitHub + any standard Markdown renderer.
**Project Type**: Documentation reference — consumer-onboarding surface for mikebom-emitted SBOMs.
**Performance Goals**: N/A.
**Constraints**:
- **Per the 2026-06-29 clarification (Q1 Option D)**: the doc MUST NOT name specific competing SBOM tools. Framing is consumer-centric ("what mikebom emits") rather than competitive ("what other tools omit").
- **Constitution Principle V** (standards-native > `mikebom:*`): the doc reinforces this in its opening positioning passage — `mikebom:*` annotations are parity-bridges introduced ONLY when no native field carries the signal. This positioning is critical for consumer trust.
- **Single-file deliverable** (spec Assumption 8): one Markdown file at `docs/reference/reading-a-mikebom-sbom.md`. No multi-file split.
- **Appendix-as-snapshot** (spec Assumption 4): the appendix index reflects the catalog at milestone-150 ship time. Future annotations land in the catalog only; the guide's appendix is best-effort current.
- **`jq` recipes verified runnable** (spec FR-011 + SC-004): each recipe in the doc must produce the documented output against a real mikebom-emitted SBOM at doc-authoring time.
- **Pre-PR gate**: `./scripts/pre-pr.sh` MUST exit 0 before PR open (modulo the documented pre-existing `sbomqs_parity` env-only failure).
**Scale/Scope**: Reaches every external mikebom-SBOM consumer; reduces the cost of onboarding from "read mikebom source OR walk 102-row catalog" to "skim the new doc". Effort budget: ~600–900 lines of Markdown + 5–10 hours of authoring.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ N/A | Documentation only. |
| II | eBPF-Only Observation | ✅ N/A | |
| III | Fail Closed | ✅ N/A | |
| IV | Type-Driven Correctness | ✅ N/A | |
| V | Specification Compliance | ✅ **REINFORCES** | The new doc's opening passage EXPLICITLY reinforces Principle V — strict spec conformance first, `mikebom:*` annotations only when no native field exists. This is part of mikebom's consumer-facing positioning. The doc IS a Principle V documentation artifact. |
| VI | Three-Crate Architecture | ✅ N/A | |
| VII | Test Isolation | ✅ N/A | |
| VIII | Completeness | ✅ IMPROVES (indirectly) | The doc helps consumers find existing completeness signals (e.g., `mikebom:component-tier` for file-tier orphan coverage, `mikebom:file-inventory-mode` for override-mode awareness). Doesn't add new completeness signals; surfaces existing ones. |
| IX | Accuracy | ✅ IMPROVES (indirectly) | Consumers reading the doc can identify mikebom's accuracy signals (`mikebom:confidence`, `mikebom:evidence-kind`, `mikebom:fingerprint-confidence`) and filter / threshold appropriately. |
| X | Transparency | ✅ DIRECTLY ADVANCES | The doc IS a transparency artifact — it tells consumers exactly what every `mikebom:*` signal means and how to interpret it. Section dedicated to "transparency / completeness gaps" cluster. |
| XI | Enrichment | ✅ N/A | |
| XII | External Data Source Enrichment | ✅ N/A | |
| SB-1 | No lockfile-based discovery | ✅ N/A | |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ N/A | |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | |

**All gates pass.** Principle V is REINFORCED (not violated) — the doc's opening positioning passage is itself an expression of Principle V's consumer-facing implications. Constitution X (Transparency) is DIRECTLY ADVANCED by the milestone.

## Project Structure

### Documentation (this feature)

```text
specs/150-sbom-consumer-guide/
├── plan.md              # This file
├── research.md          # Phase 0 — envelope schema location + key inventory + clustering rationale + jq-recipe-verification plan
├── data-model.md        # Phase 1 — doc structure entities (sections + appendix-entry shape) + per-signal rendering invariants
├── quickstart.md        # Phase 1 — operator-facing read-through walkthrough (5-question SC-001 audit reproduction)
├── contracts/
│   └── doc-structure.md # Phase 1 — TOC + per-section content contract for the new doc
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify (all items ✅)
```

### Source Files (repository root)

Touched files (narrow scope — docs-only):

```text
docs/
├── reference/
│   └── reading-a-mikebom-sbom.md            # NEW — single-file consumer-onboarding doc
└── index.md                                   # Update — add 1-line entry in "Reference material" section
```

No code files touched. No fixtures. No test files. No parity-catalog rows. No CLI flag changes.

**Structure Decision**: Single new Markdown file + one-line index update. The new doc lives at `docs/reference/reading-a-mikebom-sbom.md` (the same directory as the other reference docs — `identifiers.md`, `sbom-types.md`, `component-tiers.md`, `cross-tier-binding.md`, `sbom-format-mapping.md`, `conformance-harness-guide.md`). This placement signals it's a peer reference doc to those existing ones, not a new doc category.

## Complexity Tracking

*Not applicable.* All Constitution gates pass cleanly. Pure docs milestone — no design tradeoffs to track. The cross-tool-comparison-avoidance decision (Q1 Option D) is the only real design call; it's recorded in the spec's Clarifications + FR-010 + SC-006 + Assumption 6.
