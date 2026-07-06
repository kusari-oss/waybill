# Implementation Plan: Empirical audit — Tauri + Apache Airflow (Round 4)

**Branch**: `168-rust-python-audit` | **Date**: 2026-07-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/168-rust-python-audit/spec.md`

## Summary

Produce a persistable audit report at `docs/audits/2026-07-06-tauri-airflow.md` measuring mikebom + Trivy + Syft against two live upstream targets:

- **US1 P1**: `github.com/tauri-apps/tauri` (Rust workspace + npm polyglot; **repo root** as scan target per clarification 2026-07-06 Q1)
- **US2 P2**: `github.com/apache/airflow` (canonical Python monorepo; **repo root** as scan target per clarification 2026-07-06 Q2)

Deliverable is a Markdown report with per-tool metrics, root-cause classification of mikebom failure modes, tool-comparison delta, SPDX validation results, prioritized top-3 follow-on milestone recommendations (**US3 P3**), a **Cross-Round Trend Analysis** section (FR-011 — uses pre-recorded m165/m158 baselines with per-metric freshness caveats per Q3), and m167 vocabulary applicability assessment (FR-012 + SC-012).

**Zero production code changes.** All FRs are about report contents; SC-007 + SC-008 verify no code drift via pre-PR gate + golden byte-identity.

## Technical Context

**Language/Version**: N/A for mikebom production code (unchanged). Audit harness is shell script + Python 3.10+ analysis (matches m165 precedent verbatim).
**Primary Dependencies**: External audit tools — mikebom (**post-milestone-167 release build**; commit `ccde910`-descended), Trivy (0.71.1 per m083/m165 pin), Syft (1.44.0 per m083/m165 pin), spdx3-validate (0.0.5 per memory `reference_spdx3_validator`, at `.venv/spdx3-validate/bin/spdx3-validate`), `jq`, `git`, `python3`, `time`. Standard POSIX tools.
**Storage**: Single Markdown file at `docs/audits/2026-07-06-tauri-airflow.md`. Intermediate SBOMs stored under `specs/168-rust-python-audit/artifacts/` (regenerable, not versioned — same gitignore treatment as m165).
**Testing**: SC-007 + SC-008 pre-PR gate + golden byte-identity guard (verified via existing `cargo test --workspace` run). No new tests; FR-010 forbids code changes.
**Target Platform**: macOS + Linux (developer host running the audit).
**Project Type**: Documentation + measurement milestone (matches milestones 082/093/150/151/165 posture). Deliverable is Markdown, not Rust code.
**Performance Goals**: Full audit (clone both repos + 6 scans + analysis + report) SHOULD complete in under 30 minutes on the developer host (matches m165 target). Individual scan wall-clock times recorded in the report.
**Constraints**: FR-010 zero production code changes. SC-008 100% golden byte-identity — audit MUST NOT touch any file goldens depend on.
**Scale/Scope**: Tauri source ~50 MB, Airflow source ~200 MB. Combined SBOM output ~15-40 MB per tool. Report ~1000-1500 lines of Markdown (m165 was ~380 lines; m168 adds FR-011 Cross-Round Trend + FR-012 m167-vocab-applicability + a Python-side license spot-check per FR-006, so slightly larger expected).

## Constitution Check

**GATE**: Pass before Phase 0 research. Re-check after Phase 1 design.

Constitution v1.5.0 principles evaluated against milestone 168's audit-only scope:

- **I. Pure Rust, Zero C**: N/A — no Rust code changes. Audit harness is shell + Python.
- **II. Deterministic Scan Output**: N/A — mikebom binary unchanged; verified via SC-008 golden byte-identity guard.
- **III. Attestation-First**: N/A — no attestation code touched.
- **IV. No `.unwrap()` in Production**: N/A — no Rust code changes.
- **V. Specification Compliance (standards-native precedence)**: N/A — audit measures existing emission behavior.
- **VI. Three-Crate Architecture**: N/A — no crate touches.
- **VII. eBPF-Only Observation**: N/A — audit is source-scan only per Edge Cases § "eBPF trace not applicable"; eBPF path unmodified and unaudited by 168.
- **VIII. Completeness — Never Silently Drop**: N/A — audit measures existing emission behavior.
- **IX. Accuracy — No Fake Versions**: N/A — audit measures existing behavior.
- **X. Transparency — Explicit Signals**: **APPLIES** — the audit report itself is a transparency artifact. FR-004 root-cause classification + FR-005 tool-comparison delta + FR-007 top-3 recommendations + FR-011 cross-round trend + FR-012 m167 vocab applicability all serve Principle X by making mikebom's quality state explicit and durable.
- **XI. Every Scan Produces an SBOM**: N/A — no scan-termination path added.
- **XII. Ecosystem Coverage**: N/A — audit measures existing coverage; doesn't extend it.

**Strict Boundaries** (v1.5.0):

- §1 (deterministic PURL): N/A.
- §2 (workspace layout): N/A — no crate changes.
- §3 (constitution amendment process): N/A.
- §4 (single source of truth): N/A.
- §5 (no duplicate file-tier components): N/A — audit measures existing behavior.

**Verdict**: 11 principles + 5 boundaries either N/A or trivially satisfied. Principle X (Transparency) is the load-bearing principle. No violations, no Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/168-rust-python-audit/
├── plan.md              # This file
├── research.md          # Phase 0 output — tool pins reused from m165 + Rust/Python analyzer notes
├── data-model.md        # Phase 1 output — report structure + measurement record schemas
├── quickstart.md        # Phase 1 output — how to run the audit end-to-end
├── contracts/
│   └── README.md        # Empty stub — no new external interfaces
├── checklists/
│   └── requirements.md  # /speckit.specify output
├── scripts/             # ← audit harness (reuses m165's analyze.py verbatim + new run-audit.sh)
│   ├── analyze.py       # Copied/symlinked from m165 per Assumption
│   └── run-audit.sh     # NEW — pinned to Tauri + Airflow SHAs
└── tasks.md             # /speckit.tasks output (NOT created here)
```

### Deliverable (repository root)

```text
docs/
└── audits/                                        # ← EXISTS (m165 landed it)
    ├── 2026-07-05-kubernetes-argocd.md            # m165 report (unchanged)
    └── 2026-07-06-tauri-airflow.md                # ← THE m168 deliverable

specs/168-rust-python-audit/
└── artifacts/                                     # ← NEW dir for regenerable intermediates
    ├── tauri/
    │   ├── mikebom.cdx.json      # mikebom's CDX SBOM (task output)
    │   ├── trivy.cdx.json        # Trivy's CDX SBOM
    │   ├── syft.cdx.json         # Syft's CDX SBOM
    │   ├── mikebom.spdx23.json   # mikebom's SPDX 2.3 for FR-006 validation
    │   ├── mikebom.spdx3.json    # mikebom's SPDX 3 for FR-006 validation
    │   └── analysis.json         # Parsed per-tool metrics (task output)
    └── airflow/
        └── (same structure)
```

**Structure Decision**: Two-tier deliverable mirroring m165 verbatim. PRIMARY deliverable is `docs/audits/2026-07-06-tauri-airflow.md` — the persistable report staying in the repo as the longitudinal quality record. INTERMEDIATE artifacts (raw SBOMs, parsed metrics) live under `specs/168-rust-python-audit/artifacts/` — gitignored per m090 fixture-stayset guidance + m165 precedent.

Zero mikebom production code paths touched. Zero cargo tests added. Zero goldens regenerated.

## Complexity Tracking

No entries required. All Constitution gates pass without justification. This is a doc-only measurement milestone with a load-bearing Transparency principle (X) mapping.
