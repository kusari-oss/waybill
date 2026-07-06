# Research: milestone 168 — Rust + Python monorepos audit (Round 4)

**Date**: 2026-07-06
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Phase 0 research. All NEEDS CLARIFICATION items from Technical Context are resolved below. Where a decision was already made in m165 and remains valid for m168, the m165 rationale is referenced by-link rather than re-derived.

## R1 — External audit tool pins

**Decision**: mikebom = post-milestone-167 release build (commit `ccde910` on `main`, HEAD at m168 spec time). Trivy = `0.71.1` (m165's pin, still current). Syft = `1.44.0` (m165's pin, still current). spdx3-validate = `0.0.5` (memory `reference_spdx3_validator`).

**Rationale**: Version parity with m165 keeps the Cross-Round Trend Analysis (FR-011) apples-to-apples on the tool side. mikebom advances one revision (m165 used m164-descended; m168 uses m167-descended) so the trend section can attribute any observed baseline drift to m166/167 changes rather than tool churn. All 3 external tools install exactly as documented in m165 §R1 (including the Trivy direct-binary-download fallback if brew tap serves stale versions).

**Alternatives considered**:
- **Bump Trivy/Syft to latest**: rejected — introduces a confounding variable between m165 and m168 measurements. If Trivy 0.71.1 has a known bug fixed in 0.72+, the report notes it but preserves the pin.
- **Test against multiple Trivy/Syft versions**: out of scope — the audit's job is to measure mikebom, not sweep external tool versions.

## R2 — Target repo + commit pin selection

**Decision**: Tauri = `github.com/tauri-apps/tauri` HEAD at execution time (SHA recorded in report header). Airflow = `github.com/apache/airflow` HEAD at execution time (SHA recorded in report header). Both cloned live via `git clone --depth 1` — full history not needed since mikebom doesn't consult git log.

**Rationale**: Matches m164/m165 live-upstream approach. Pinning to a specific historical SHA would be more reproducible but less representative — the audit's value is measuring current mikebom quality on current representative real-world targets. The report header records the exact SHA so a re-run against the same SHA reproduces numbers.

**Alternatives considered**:
- **Pin to a stable release tag** (e.g., Tauri v2.x.0, Airflow 2.x.x): rejected — release tags are stale; HEAD better represents the "what a maintainer scanning today would see" question the audit answers.
- **Pre-download source tarballs**: rejected — clone is fast enough (< 30s each even on slow connections) and matches how a real user would prep the audit.

## R3 — Scan scope decision (Q1 + Q2 clarifications)

**Decision**: Both targets scanned at repo root — no path exclusions. Tauri picks up Cargo workspace + example apps' npm deps together; Airflow picks up top-level Python sources + every `providers/*` subdir Python declaration.

**Rationale**: Both clarification Q1 (Tauri repo root) and Q2 (Airflow repo root) chose Option A — widest measurement. This maximizes the exercised mikebom-reader surface and produces the richest signal for FR-004 root-cause classification. Symmetric handling of both targets simplifies the harness (single `mikebom sbom scan --path <clone>` invocation).

**Alternatives considered**:
- **Sub-tree targeting** (`crates/` only on Tauri; excluding `providers/**` on Airflow): rejected via clarification.
- **Multiple scans per target with different scopes**: out of scope for a Round-4 audit; a future dedicated ecosystem-coverage milestone could measure that.

## R4 — Cross-Round Trend Analysis baseline handling (Q3 clarification)

**Decision**: Use m165's frozen numbers (from `docs/audits/2026-07-05-kubernetes-argocd.md`) verbatim as the trend baseline. For each m165 metric where a post-baseline milestone (m166 or m167) plausibly altered the number if re-measured, attach a one-line freshness caveat.

**Rationale**: Q3's Option B strikes the best transparency-per-effort balance. Actually re-measuring m165's ArgoCD + K8s with post-167 mikebom would be a Round-5 audit's job; the m168 report shouldn't sprawl into it. Known-affected metrics:

| m165 metric | Post-167 expected change |
|---|---|
| ArgoCD "zero-emitted-orphan-reason on npm side" | Would now be non-zero (m167 emits `hoisted-unused` / `dead-lockfile-entry` on npm) |
| K8s "1 emitted-orphan-reason on Go side" | Would now count `pkg:golang/stdlib@v1.26.1` at minimum (m167 empirical golden change) |
| ArgoCD/K8s BFS reachability % | Marginal change expected — m167 doesn't affect edge count, only annotation emission |
| SPDX 3 dedup outcome | m165 recorded `spdx3-validate` FAIL on both K8s + ArgoCD; m166 fixed this — the freshness caveat notes "post-166 status expected: PASS" |

**Alternatives considered**:
- **Re-measure m165 targets**: rejected — Out of Scope preserved; expensive relative to information gained.
- **Verbatim without caveat**: rejected — misleads the reader about baseline age.

## R5 — Analysis script reuse

**Decision**: Reuse `specs/165-k8s-argocd-audit/scripts/analyze.py` verbatim, symlinked/copied into `specs/168-rust-python-audit/scripts/analyze.py`. Any new classification buckets for Rust or Python failure modes are added inline via a small extension (target-agnostic per m165 backlog observation).

**Rationale**: m165's `analyze.py` was designed target-agnostic. The Rust + Python targets exercise the same output shapes (CDX 1.6 + SPDX 2.3 + SPDX 3 JSON), so the parser doesn't change. Only new classification buckets (if any) get added — those become part of m168's scripts/ deliverable, not a mikebom production code change.

**Alternatives considered**:
- **Write a new analyzer**: rejected — wasteful given m165's is target-agnostic by design.
- **Modify m165's in-place**: rejected — m165 is a historical artifact; modifying it breaks audit reproducibility.

## R6 — m167 vocabulary applicability assessment (FR-012 + SC-012)

**Decision**: For each orphan the audit surfaces in the Cargo + Python ecosystems, the report explicitly notes whether m167's C45 vocabulary covers it and, if not, proposes a candidate new code. This becomes SC-012's deliverable and folds into US3 (P3) top-3 recommendations if a vocabulary extension is warranted.

**Rationale**: m167 emits orphan-reason only on Go + npm today (per m167 FR-001 scope). Cargo + Python orphans surface WITHOUT the annotation, so the audit's classification is external. The interesting question is: do the 5 m167 codes describe Cargo/Python orphan patterns cleanly, or do Cargo/Python have novel patterns that need new codes (e.g., "path-dep without version", "editable-install-without-version")?

**Alternatives considered**:
- **Defer m167 applicability to a follow-on milestone**: rejected — the whole point of running Round 4 shortly after m167 lands is to measure the vocabulary's cross-ecosystem generality while it's fresh.
- **Add new codes speculatively in m167 without empirical measurement**: rejected via m167's Out of Scope (m167 was Go+npm only per its clarification; extension without measurement would be speculative).

## R7 — SPDX license spot-check on Airflow scale (FR-006 stress test)

**Decision**: Airflow's ~1000+ Python transitive deps are the largest LicenseRef-* concentration in mikebom's audit history. FR-006 requires spdx3-validate to PASS on Airflow's SPDX 3 output; the audit records the pass/fail status + first 3 failing license expressions (if any). Milestones 146/152/153/154 SPDX license work is functionally tested here at scale for the first time.

**Rationale**: The Python ecosystem's license distribution is heterogeneous — many packages declare custom or archaic license strings. If mikebom's m154 custom-license handling (LicenseRef- emission for non-SPDX-canonical licenses) has any edge-case regression, Airflow will expose it. The report becomes the empirical validation record for m146/152/153/154.

**Alternatives considered**:
- **Manual spot-check of 10 random deps**: rejected — insufficient sample for a "at-scale" claim per SC-005.
- **License audit as a separate deliverable**: deferred to Out of Scope (a dedicated license audit is a future milestone; m168's FR-006 is a stress-test spot-check, not a full audit).

## R8 — Report structure = m165 structure + FR-011 + FR-012 additions

**Decision**: m168 report structure = m165 report structure verbatim + 2 new sections:

1. **Cross-Round Trend Analysis** (FR-011) — inserted between "Recommended Follow-On Milestones" and "Backlog Observations" per m165 section order.
2. **m167 Vocabulary Applicability** (FR-012 + SC-012) — inserted within the "Recommended Follow-On Milestones" section as a distinct sub-section (either a candidate follow-on if vocab needs extension, or a "vocab is sufficient — no extension needed" note if not).

Executive Summary at report end (m165 pattern) explicitly calls out both new sections.

**Rationale**: Reader familiarity — anyone who read m165's report navigates m168's report structure by muscle memory. New sections are additive, not restructuring, so cross-round comparison stays intuitive.

**Alternatives considered**:
- **Restructure to lead with Cross-Round Trend**: rejected — cross-round trend is a synthesized deliverable that depends on per-target analysis; leading with it inverts the natural evidence-to-synthesis flow.
- **Split m167 applicability into its own top-level section**: rejected — it's a sub-question under "what should we do next," which is what "Recommended Follow-Ons" already frames.
