# Data Model: milestone 165 — Kubernetes + ArgoCD audit

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Phase 1 data model. Since this is a docs+measurement milestone, the "data model" is the STRUCTURE of the report + the intermediate metrics records. No Rust types touched.

## Data entities

### E1 — TargetSnapshot (per audit target)

**Location**: recorded in the report's per-target Snapshot subsection AND in `specs/165-k8s-argocd-audit/artifacts/<target>/snapshot.json`.

**Fields**:
- `target_name`: `"kubernetes"` | `"argocd"`
- `upstream_url`: `"https://github.com/kubernetes/kubernetes"` | `"https://github.com/argoproj/argo-cd"`
- `commit_sha`: 40-char hex SHA of HEAD at clone time
- `clone_size_bytes`: `du -sb <clone>` output
- `clone_date_utc`: ISO 8601 timestamp
- `scan_wall_clock_seconds`: per-tool, keyed by `mikebom` | `trivy` | `syft`

**Reproducibility rule**: Given identical `commit_sha` + tool versions, the metrics at E2 are reproducible modulo clock-skew on wall-clock times.

### E2 — PerToolMetrics (per target × per tool)

**Fields**:
- `target`: E1 reference
- `tool`: `"mikebom"` | `"trivy"` | `"syft"`
- `tool_version`: exact version string
- `sbom_format`: `"cyclonedx-json"` (primary); `"spdx-2.3-json"` and `"spdx-3-json"` for FR-006 mikebom-only spot-checks
- `total_components`: `.components | length`
- `edges`: `.dependencies | map(.dependsOn) | flatten | length`
- `bfs_reachable`: count reachable from `metadata.component.purl` via BFS
- `bfs_reachability_pct`: `bfs_reachable / (total npm+golang components) * 100`
- `ecosystem_breakdown`: dict keyed by ecosystem (`"golang"`, `"npm"`, `"other"`), values = component count
- `empty_version_purls`: `[.components[].purl | select(test("^pkg:(npm|golang)/[^@]+@$"))] | length`
- `phantom_edges`: `[.dependencies[].dependsOn[] | select(test("^pkg:(npm|golang)/[^@]+@$"))] | length`

**Validation rule**: for mikebom on npm-heavy targets, `empty_version_purls` MUST equal 0 (milestone 163 SC-004 invariant) and `phantom_edges` MUST equal 0 (milestone 163 SC-002 invariant). If either > 0, the audit surfaces this as a regression — a critical finding that goes at the TOP of the "Recommended Follow-Ons" section.

### E3 — RootCauseBucket (per target × per mikebom failure mode)

**Location**: per-target "mikebom Failure Modes" subsection.

**Fields**:
- `bucket_name`: kebab-case identifier from research §R6 (or a new name if the empirical pattern isn't in R6's list)
- `count`: number of components/edges in this bucket
- `example_purl`: one representative PURL from the bucket (spot-check anchor)
- `example_source_path`: if applicable, the source path attribution (e.g., `staging/src/k8s.io/api/...`)
- `disposition`: enum `"fix-in-follow-on-milestone"` | `"accept-as-is-with-rationale"`
- `disposition_rationale`: 1-3 sentence justification
- `follow_on_milestone_hint`: if `disposition == "fix-in-follow-on-milestone"`, a rough scope estimate referencing an analogous prior milestone

**Validation rule**: every bucket MUST have `disposition` set. No "TBD" or "needs-investigation" — the audit resolves each bucket before shipping the report.

### E4 — ToolComparisonDelta (per target × per ecosystem)

**Location**: per-target "Tool Comparison Delta" subsection + full details in `specs/165-k8s-argocd-audit/artifacts/<target>/delta_full.json`.

**Fields**:
- `target`: E1 reference
- `ecosystem`: `"golang"` | `"npm"` | `"cross"` (cross-ecosystem view only for ArgoCD US2)
- `mikebom_advantage`: sorted list of PURLs mikebom finds that neither Trivy nor Syft finds (capped to first 20 in report; full list in `delta_full.json`)
- `trivy_advantage`: same for trivy
- `syft_advantage`: same for syft
- `mikebom_trivy_only`: PURLs both mikebom + Trivy find but Syft misses
- `mikebom_syft_only`: PURLs both mikebom + Syft find but Trivy misses
- `trivy_syft_only`: PURLs both Trivy + Syft find but mikebom misses
- `all_three_intersect`: count of PURLs all 3 tools agree on

**Sample cap**: in the report, each `_advantage` list shows first 20 sorted-lex + `... and N more` line. Full lists in artifacts JSON.

### E5 — SPDXValidationRecord (per target × per format)

**Location**: per-target "SPDX Validation" subsection.

**Fields**:
- `target`: E1 reference
- `tool`: `"mikebom"` (per FR-006 scope — Trivy/Syft may not emit SPDX 3, marked "not attempted")
- `format`: `"spdx-2.3-json"` | `"spdx-3-json"`
- `validator_command`: exact command run
- `pass_fail`: `"pass"` | `"fail"` | `"not attempted"`
- `error_summary`: if `fail`, 1-2 sentence summary of the top error class

**Rationale**: milestone-078 established the SPDX 3 conformance gate. Reused here as a spot-check.

### E6 — RecommendedFollowOnMilestone (top 3 audit output)

**Location**: "Recommended Follow-On Milestones" section of the report.

**Fields**:
- `rank`: 1 | 2 | 3
- `title`: 4-8 word working title
- `problem_statement`: 1-2 sentence problem description
- `evidence`: quantitative impact estimate (BFS pp gain OR N components / M edges affected)
- `scope_estimate`: rough effort per analogous prior milestone (e.g., "small — analogous to milestone 087 cargo fix, ~20 tasks")
- `blast_radius`: which ecosystems / target patterns benefit
- `alternative`: brief note on why NOT to do it now (if applicable)

### E7 — AuditReport (top-level deliverable)

**Location**: `docs/audits/2026-07-05-kubernetes-argocd.md`

**Fields**:
- Metadata header: report date, mikebom baseline commit SHA, Trivy/Syft/spdx3-validate versions, machine + OS host
- Executive Summary: 3-5 sentences
- Per-target Snapshots (E1)
- Per-tool Metrics tables (E2)
- Per-target RootCauseBuckets (E3)
- Per-target ToolComparisonDelta (E4)
- Per-target SPDXValidationRecord (E5, mikebom only)
- Cross-ecosystem Interactions section (US2 only)
- RecommendedFollowOnMilestones (E6, top 3)
- Backlog Observations (E6-like but not top 3, no cap)
- Reproduction Appendix (exact commands)

**Validation rule**: the report is considered complete when every SC (SC-001 through SC-011) can be checked by reading the file. SC-001 (report exists), SC-002 (both targets measured), SC-003 (failure modes classified), SC-004 (comparison delta documented), SC-005 (SPDX validation results), SC-006 (top-3 ranked), SC-009 (reproduction appendix), SC-010 (pinned commit SHAs), SC-011 (either bug class OR clean pass).

## Wire types

**None.** Milestone 165 doesn't touch mikebom's emitted SBOM wire format. All data lives in the audit report Markdown + regenerable intermediate JSON.

## Relationships

```text
AuditReport (E7)
├── header: mikebom_baseline_SHA, tool_versions, host_info
├── per target:
│   TargetSnapshot (E1)
│   ├── per tool: PerToolMetrics (E2)
│   ├── per bucket: RootCauseBucket (E3)
│   ├── per ecosystem: ToolComparisonDelta (E4)
│   └── per format: SPDXValidationRecord (E5)
└── RecommendedFollowOnMilestone[] (E6, top 3)
```

## Data volume assumptions

- **Report size**: ~1000-1500 lines of Markdown. Well under GitHub's 1 MB soft limit for Markdown rendering.
- **Intermediate SBOM sizes**: ~5-20 MB per (target, tool, format) = ~60-180 MB total. Gitignored per research §R10.
- **Report generation time**: ~1-2 hours end-to-end on developer host (clone: ~5 min; scans: ~10 min total; analysis + report writing: ~1 hr). Compatible with plan.md's "< 30 minutes" performance goal ONLY if the report content itself is largely templated — deep analysis of surprising findings could extend this.

## Validation rules (aggregated)

| Rule | Enforcement |
|------|-------------|
| Report exists at `docs/audits/YYYY-MM-DD-kubernetes-argocd.md` | Manual file-existence check per SC-001 |
| Both targets measured | Table presence check per SC-002 |
| Every bucket has a disposition (no TBD) | E3 validation |
| SPDX validation results per target per format | E5 validation |
| Top 3 follow-ons with quantitative impact | E6 validation |
| Reproduction appendix contains exact commands | Manual review |
| Pinned commit SHAs recorded | Manual review + E1 field |
| mikebom's `empty_version_purls == 0` on npm side (milestone 163 invariant) | E2 automatic check; if > 0 → CRITICAL top-of-Recommendations finding |
| Post-165 golden byte-identity | SC-008 verified via `cargo test --workspace` post-audit |
