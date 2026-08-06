# Implementation Plan: Survey peer-project release flows + recommendation for waybill's multi-track release strategy

**Branch**: `228-release-flow-exploration` | **Date**: 2026-08-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/228-release-flow-exploration/spec.md`

## Summary

Pure research + docs milestone. Deliverable is a single markdown document (~500–800 lines per spec SC-007) surveying at least 5 peer OSS projects' release-track models, scoring them across 5 tradeoff axes, and concluding with **one** decisive recommendation for waybill (per Q1 clarification). Recommendation is bounded to gh-release + OCI distribution surfaces today (per Q2) but must remain compatible with future crates.io / homebrew / cargo-binstall / apt-rpm-dnf expansion. Nightly cadences are recorded verbatim per project (per Q3) rather than normalized. No code changes, no CI workflow YAML changes, no `Cargo.toml` changes — implementing whichever model gets recommended is a separate follow-up spec (probably `229-release-flow-implementation`).

Placement decision (Phase 0): the survey lives at `docs/design/2026-08-05-release-flow-survey.md`. Rationale — `docs/audits/` is the wrong place (this isn't a per-target audit); `docs/reference/` is the wrong place (this is decision-informing, not evergreen reference material); `docs/design/` is the natural home for design surveys informing eventual code changes. If `docs/design/` doesn't exist, T-tasks create it.

Total code surface: zero Rust LOC. One new docs file (+ possibly one new directory `docs/design/`). Estimated effort: ~500–800 lines of Markdown across the survey document + recommendation + rejected-alternatives block + future-distribution-compatibility subsection + risks-and-open-questions block.

## Technical Context

**Language/Version**: N/A — Markdown documentation. The waybill binary is unchanged; every reader, every emitter, every CLI flag remains byte-identical pre/post merge. No workflow YAML changes.
**Primary Dependencies**: None new. Documentation targets GitHub-flavored Markdown rendered by the standard `docs/` viewer per existing convention. Research phase reads external OSS project docs, CHANGELOGs, release pages, and GitHub Actions workflows — no runtime dependency on those sources beyond source-citation at survey-authoring time.
**Storage**: N/A.
**Testing**: `cargo +stable test --workspace` and `cargo +stable clippy --workspace --all-targets -- -D warnings` are effectively no-ops for this milestone (no Rust source change), but per project convention `./scripts/pre-pr.sh` MUST still exit 0 before PR open. Survey-content correctness is verified by manual source-citation spot-check per spec SC-006 — no automated test verifies peer-project claims.
**Target Platform**: Markdown rendering on GitHub + any standard CommonMark renderer.
**Project Type**: Research + design deliverable — decision-informing document intended to feed a follow-up implementation spec.
**Performance Goals**: N/A.
**Constraints**:
- **Docs-only** (spec Assumptions §1 + FR-010): zero Rust source change; zero workflow YAML change; zero Cargo.toml change. If a proposed workflow-mechanic surfaces during writing (e.g., "we should have a cron for nightlies"), it becomes an implementation-spec seed, not part of this milestone.
- **Single decisive recommendation** (Q1 clarification + FR-005): the deliverable concludes with ONE preferred model. Alternatives explicitly-considered-and-rejected go into a "considered and rejected" subsection with brief rationale per alternative. NOT a menu of options for the maintainer to pick between.
- **Distribution-scope bounds + future-compatibility invariant** (Q2 clarification + FR-011 + FR-012): survey addresses only gh-release + OCI today; recommendation MUST NOT preclude future extension to crates.io / homebrew / cargo-binstall / apt-rpm-dnf. Channel names, tag/version conventions, and per-channel signing decisions MUST remain compatible with those downstream conventions. Include a "future-distribution compatibility" subsection listing which downstream surfaces the recommendation has been checked against.
- **Nightly-cadence variance recorded verbatim** (Q3 clarification + FR-003): the tradeoff matrix's nightly column records each peer's actual cadence ("per-commit", "1×/day scheduled", "1×/day if changes", "manual only") without normalization. The recommendation picks ONE cadence for waybill and justifies against the observed variance.
- **Constitution Principle V** (standards-native > waybill:\*): the recommendation MUST honor CISA 2026 signing per FR-007 — each channel's artifacts must be able to carry SBOM Author Signature via the existing m222 keyless flow. Channels for which signing would be impossible or meaningless (hypothetically — probably none) would need explicit rejection rationale.
- **Cross-reference invariant** (spec FR-006): every claim about a specific peer project's release model MUST cite a source (URL to release page, workflow YAML, or docs). No memory-based claims per memory `feedback-verify-research-empirical-claims`.
- **Line-budget** (spec SC-007): ≤ 800 lines of markdown for the full survey + recommendation.
- **Pre-PR gate**: `./scripts/pre-pr.sh` MUST exit 0 before PR open.
**Scale/Scope**: reaches waybill maintainer (US1 P1 + US2 P2 audiences) + downstream SBOM consumer (US3 P3 audience). Effort budget: ~500–800 lines markdown + peer-project research time (estimated ~1 hour per surveyed project for shallow-but-verified coverage = ~5–8 hours research + 3–4 hours writing).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ N/A | Documentation only. |
| II | eBPF-Only Observation | ✅ N/A | |
| III | Fail Closed | ✅ N/A | |
| IV | Type-Driven Correctness | ✅ N/A | |
| V | Specification Compliance | ✅ **REINFORCES** | The recommendation's per-channel signing subsection (FR-007a) explicitly ties every proposed release channel to CISA 2026 Author-Signature compliance. Cannot recommend a channel model that produces unsignable artifacts. |
| VI | Three-Crate Architecture | ✅ N/A | |
| VII | Test Isolation | ✅ N/A | |
| VIII | Completeness | ✅ N/A | Not a scan-emission signal. |
| IX | Accuracy | ✅ **REINFORCES** | Survey correctness is source-cited per FR-006 (memory `feedback-verify-research-empirical-claims`). No fabricated peer-project facts. |
| X | Transparency | ✅ **DIRECTLY ADVANCES** | The recommendation is a transparency artifact — makes waybill's release-channel policy explicit and consumer-visible per US3. Currently the single-alpha-channel model is transparent only implicitly (via looking at git tags). |
| XI | Enrichment | ✅ N/A | |
| XII | External Data Source Enrichment | ✅ N/A | |
| SB-1 | No lockfile-based discovery | ✅ N/A | |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ N/A | |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | |

**All gates pass.** Principles V, IX, X reinforced or directly advanced. Pure research + docs milestone with no code paths touched.

## Project Structure

### Documentation (this feature)

```text
specs/228-release-flow-exploration/
├── plan.md              # This file
├── research.md          # Phase 0 — research methodology + peer-project longlist + tradeoff-axis definitions + per-project source-citation checklist
├── data-model.md        # Phase 1 — survey-document entities (peer-project row shape, tradeoff-matrix cell shape, recommendation subsection shape, rejected-alternative shape)
├── quickstart.md        # Phase 1 — first-time reader walkthrough exercising SC-001 (peer-pattern recall test) + SC-008 (consumer channel-choice test)
├── contracts/
│   └── doc-structure.md # Phase 1 — TOC + per-subsection content contract for the survey document, mapping every SC to a specific subsection
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify (all items ✅)
```

### Source Files (repository root)

Touched files (narrow scope — docs-only):

```text
docs/
└── design/                                          # NEW directory if not present
    └── 2026-08-05-release-flow-survey.md            # NEW — single-file survey + recommendation deliverable
```

Optional file (only if pre-existing docs need to link out):

```text
docs/
├── index.md                                         # Optional 1-line addition pointing to the new design doc (verify during T-task whether index.md exists and hosts a "design docs" section)
└── contributing/
    └── (any existing release-process doc)           # Optional cross-reference if a pre-existing release-process doc exists that operators land on
```

No code files touched. No fixtures. No test files. No parity-catalog rows. No CLI flag changes. No workflow YAML changes. No `Cargo.toml` changes.

**Structure Decision**: single new Markdown file at `docs/design/2026-08-05-release-flow-survey.md`. Placement rationale documented in the Summary: `docs/audits/` is the wrong home (this isn't a per-target audit; audits are `2026-MM-DD-<target>.md` for a specific external target like RestSharp or kubernetes+argocd, not for waybill's own release model); `docs/reference/` is the wrong home (this document is decision-informing point-in-time research, not evergreen reference material like `component-tiers.md` or `sbom-format-mapping.md`); `docs/design/` is the natural home for design surveys informing eventual code changes and matches the convention used by other OSS projects for decision docs / ADR-adjacent artifacts. If `docs/design/` doesn't exist today (verified during T001 baseline), the T-task creates it as part of the same commit.

## Complexity Tracking

*Not applicable.* All Constitution gates pass cleanly. Pure research + docs milestone with clear per-project verification path (cite source, verify source is real, extract facts). The only design tradeoffs surface in Phase 0 research:

1. **Peer-project longlist vs shortlist** — spec FR-002 requires ≥5 projects across ≥3 of 5 categories. Phase 0 research produces a longlist of ~10–15 candidate projects and picks the final 5–7 based on (a) source-citation availability (public workflow YAML present?), (b) project-shape fit to waybill (small-to-medium OSS, not enterprise-scale), (c) category coverage. Resolved by Phase 0 selection criteria.
2. **How much per-project detail to include** — spec FR-001 lists 5 required per-project data points (name+link, channel model, cadence, project shape, "why this fits their project" note). Phase 0 confirms these fit in a 2–4-line matrix row + optional prose paragraph.
3. **Where to place the "considered and rejected" subsection** — options are (a) inline within the recommendation, (b) as a sibling subsection, (c) as an appendix. Resolved during Phase 1 doc-structure contract.
4. **How specific to make the recommendation** — spec SC-004 requires "specific enough that an engineer can write a follow-up implementation spec from the recommendation alone". Phase 1 contract enumerates the concrete fields the recommendation must fix (channel names, cadence, tag regex, signing decision per channel, promotion rules, changelog policy).
