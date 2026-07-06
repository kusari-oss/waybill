# Implementation Plan: Empirical audit — Kubernetes + ArgoCD

**Branch**: `165-k8s-argocd-audit` | **Date**: 2026-07-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/165-k8s-argocd-audit/spec.md`

## Summary

Produce a persistable audit report at `docs/audits/2026-07-05-kubernetes-argocd.md` measuring mikebom + Trivy + Syft against two live upstream targets:
- **US1 P1**: `github.com/kubernetes/kubernetes` (Go monorepo at scale)
- **US2 P2**: `github.com/argoproj/argo-cd` (polyglot Go + npm)

Deliverable is a Markdown report with per-tool metrics, root-cause classification of mikebom failure modes, tool-comparison delta, SPDX validation results, and prioritized top-3 follow-on milestone recommendations (**US3 P3**).

**Zero production code changes.** All FRs are about report contents; SC-007 + SC-008 verify no code drift via pre-PR gate + golden byte-identity.

## Technical Context

**Language/Version**: N/A for mikebom production code (unchanged). Audit harness is shell script + Python 3.10+ analysis (matches milestone-078 precedent).
**Primary Dependencies**: External audit tools — mikebom (milestone-164 release build), Trivy (0.71.1 per milestone-083 pin, needs local install; research §R1), Syft (1.44.0 per milestone-083 pin, already installed locally), spdx3-validate (0.0.5 per memory `reference_spdx3_validator`, already at `.venv/spdx3-validate/bin/spdx3-validate`), `jq`, `git`, `python3`, `time`. Standard POSIX tools.
**Storage**: Single Markdown file at `docs/audits/2026-07-05-kubernetes-argocd.md`. Intermediate SBOMs stored under `specs/165-k8s-argocd-audit/artifacts/` (regenerable, not versioned).
**Testing**: SC-007 + SC-008 pre-PR gate + golden byte-identity guard (verified via existing `cargo test --workspace` run). No new tests; FR-010 forbids code changes.
**Target Platform**: macOS + Linux (developer host running the audit).
**Project Type**: Documentation + measurement milestone (matches milestones 082/093/150/151 posture). Deliverable is Markdown, not Rust code.
**Performance Goals**: Full audit (clone both repos + 6 scans + analysis + report) SHOULD complete in under 30 minutes on the developer host. Individual scan wall-clock times recorded in the report.
**Constraints**: FR-010 zero production code changes. SC-008 100% golden byte-identity — audit MUST NOT touch any file goldens depend on.
**Scale/Scope**: Kubernetes source ~230 MB, ArgoCD source ~150 MB. Combined SBOM output ~10-30 MB per tool. Report ~1000-1500 lines of Markdown.

## Constitution Check

**GATE**: Pass before Phase 0 research. Re-check after Phase 1 design.

Constitution v1.5.0 principles evaluated against milestone 165's audit-only scope:

- **I. Pure Rust, Zero C**: N/A — no Rust code changes. Audit harness is shell + Python.
- **II. Deterministic Scan Output**: N/A — mikebom binary unchanged; verified via SC-008 golden byte-identity guard.
- **III. Attestation-First**: N/A — no attestation code touched.
- **IV. No `.unwrap()` in Production**: N/A — no Rust code changes.
- **V. Specification Compliance (standards-native precedence)**: N/A — audit measures existing emission behavior.
- **VI. Three-Crate Architecture**: N/A — no crate touches.
- **VII. eBPF-Only Observation**: N/A — audit is source-scan only per Edge Cases § "eBPF trace not applicable"; eBPF path unmodified and unaudited by 165.
- **VIII. Completeness — Never Silently Drop**: N/A — audit measures existing emission behavior.
- **IX. Accuracy — No Fake Versions**: N/A — audit measures existing behavior.
- **X. Transparency — Explicit Signals**: **APPLIES** — the audit report itself is a transparency artifact. FR-004 root-cause classification + FR-005 tool-comparison delta + FR-007 top-3 recommendations all serve Principle X by making mikebom's quality state explicit and durable.
- **XI. Every Scan Produces an SBOM**: N/A — no scan-termination path added.
- **XII. Ecosystem Coverage**: N/A — audit measures existing coverage; doesn't extend it.

**Strict Boundaries** (v1.5.0):

- §1 (deterministic PURL): N/A.
- §2 (workspace layout): N/A — no crate changes.
- §3 (constitution amendment process): N/A.
- §4 (single source of truth): N/A.
- §5 (no duplicate file-tier components): N/A — audit measures existing behavior.

**Verdict**: 11 principles + 5 boundaries either N/A or trivially satisfied. Principle X (Transparency) is the load-bearing principle — the audit report itself is a transparency artifact serving future maintainers. No violations, no Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/165-k8s-argocd-audit/
├── plan.md              # This file
├── research.md          # Phase 0 output — tool pins, methodology decisions
├── data-model.md        # Phase 1 output — report structure + measurement record schemas
├── quickstart.md        # Phase 1 output — how to run the audit end-to-end
├── contracts/
│   └── README.md        # Empty stub — no new external interfaces
├── checklists/
│   └── requirements.md  # /speckit.specify output
└── tasks.md             # /speckit.tasks output (NOT created here)
```

### Deliverable (repository root)

```text
docs/
└── audits/                                        # ← NEW directory
    └── 2026-07-05-kubernetes-argocd.md            # ← THE deliverable

specs/165-k8s-argocd-audit/
└── artifacts/                                     # ← NEW dir for regenerable intermediates
    ├── kubernetes/
    │   ├── mikebom.cdx.json      # mikebom's CDX SBOM (task output)
    │   ├── trivy.cdx.json        # Trivy's CDX SBOM
    │   ├── syft.cdx.json         # Syft's CDX SBOM
    │   ├── mikebom.spdx23.json   # mikebom's SPDX 2.3 for FR-006 validation
    │   ├── mikebom.spdx3.json    # mikebom's SPDX 3 for FR-006 validation
    │   └── analysis.json         # Parsed per-tool metrics (task output)
    └── argocd/
        └── (same structure)
```

**Structure Decision**: Two-tier deliverable. The PRIMARY deliverable is `docs/audits/2026-07-05-kubernetes-argocd.md` — the persistable report staying in the repo as the longitudinal quality record. INTERMEDIATE artifacts (raw SBOMs, parsed metrics) live under `specs/165-k8s-argocd-audit/artifacts/` — they should be gitignored per milestone-090 fixture-stayset guidance (regenerable from the audit script + pinned commit SHAs, so no need to commit ~40 MB of JSON).

Zero mikebom production code paths touched. Zero cargo tests added. Zero goldens regenerated.

## Complexity Tracking

No entries required. All Constitution gates pass without justification. This is a doc-only measurement milestone with a load-bearing Transparency principle (X) mapping.
