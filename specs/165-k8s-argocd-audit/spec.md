# Feature Specification: Empirical audit of mikebom against Kubernetes + ArgoCD (Round 3 measurement)

**Feature Branch**: `165-k8s-argocd-audit`
**Created**: 2026-07-05
**Status**: Draft
**Input**: Discovery-driven follow-on after milestone 164 delivered essentially all of milestone-158's ≥99% BFS-reachability aspirational target for the npm ecosystem on podman-desktop (99.6% post-164). Extend the audit pattern to two large real-world targets that exercise different ecosystems + failure modes: Kubernetes (Go monorepo at scale, ~2M+ LOC, tests milestones 160/161's Go transitive-edge + workspace-mode work at scale) and ArgoCD (polyglot Go + JavaScript, tests both ecosystems together).

## Motivation

Milestones 160/161/162/163/164 each fixed a specific bug class surfaced by measuring mikebom against a single testbed (test-podman-desktop). The measurement → root-cause → fix → merge pattern surfaced 5 bug classes in 5 milestones. Post-164, the npm ecosystem is essentially at parity with best-in-class SBOM tools (99.6% BFS reachability on podman-desktop, zero empty-version PURLs, zero phantom edges).

Two open questions:

1. **Does the milestone-164 outcome generalize?** Does a different pnpm-monorepo (or an npm-monorepo entirely) surface any classes we haven't seen? Or is podman-desktop's shape representative?

2. **What is mikebom's current quality vs Trivy + Syft on Go-heavy and polyglot targets?** Milestone 158's T035 measured 3 tools on podman-desktop (npm-heavy). We have no equivalent baseline on a Go monorepo (Kubernetes) or a polyglot Go+JavaScript codebase (ArgoCD).

Both targets are canonical:

- **Kubernetes** (`github.com/kubernetes/kubernetes`): the reference Go monorepo. ~2M+ lines of Go across 400+ modules. Exercises milestones 055 (Go transitive edges), 160 (Go transitive coverage), 161 (Go workspace-mode) at real scale. Also has staging repos, generated code, vendored third-party — corner cases that a smaller Go fixture wouldn't hit.

- **ArgoCD** (`github.com/argoproj/argo-cd`): polyglot Go server + TypeScript/JavaScript UI. Tests the Go and npm ecosystems together, plus interaction between them (e.g., a workspace peer that vendors a Go binary via npm's `bin` field). Also has real-world Docker + Helm chart dependencies that exercise some polyglot readers.

**This is an audit milestone.** Deliverable: a persistable audit report + prioritized follow-on milestone recommendations. Zero production code changes. Matches milestone 158's T035, milestone 093's docs polish, and milestone 150/151's consumer-guide pattern (report-only milestones).

## Distinction from prior audit efforts

- **Milestone 158 T035**: measured mikebom vs Trivy + Syft on `test-podman-desktop` (npm-heavy). Surfaced 5 bug classes → milestones 160/161/162/163/164.
- **Milestone 083 audit harness**: cross-tool parity harness with pinned fixtures. Different scope — this is a live-upstream measurement, not a pinned regression fixture.
- **Milestone 165 (this)**: extends milestone 158's measurement to two new targets. Same tools (mikebom, Trivy, Syft). Same measurement methodology (BFS reachability, orphan classification, ecosystem coverage delta). Different targets exercising different ecosystems + scales.

## User Scenarios & Testing

### User Story 1 — Maintainer audits mikebom quality on a Go monorepo at scale (Priority: P1)

A mikebom maintainer needs to know whether mikebom's Go-ecosystem quality (post-milestones 055/091/160/161) holds on a real Go monorepo at scale, or whether new failure classes emerge that weren't visible on smaller test targets. Post-165, the maintainer can:
- Read a documented measurement of mikebom + Trivy + Syft on kubernetes source tree.
- See the delta between the three tools (component count, edge count, BFS reachability, per-tool advantages).
- See a root-cause classification of any orphans, phantom edges, or empty-version PURLs mikebom emits.
- See the top 3 highest-ROI follow-on milestone candidates the audit surfaces.

**Why this priority**: Kubernetes is the canonical Go project. If mikebom's Go quality is broken on Kubernetes, it's broken everywhere. High-ROI baseline measurement.

**Independent Test**: Clone `github.com/kubernetes/kubernetes` at a pinned commit; run mikebom, Trivy, Syft; produce a report with component/edge counts + BFS reachability from each tool + per-tool delta table + root-cause classification of mikebom's failure modes.

**Acceptance Scenarios**:

1. **Given** a freshly cloned `github.com/kubernetes/kubernetes` at HEAD (pinned commit recorded in the report), **When** the audit runs mikebom + Trivy + Syft against it, **Then** the report MUST document each tool's total component count, edge count, and BFS reachability percentage.

2. **Given** any orphans / empty-version PURLs / phantom edges mikebom emits, **When** the audit classifies them by root cause, **Then** the report MUST assign each class a specific reason bucket (e.g., "vendored third-party not on go.mod path", "generated code missing from module graph", "stale go.sum entries") AND a follow-on milestone recommendation (or "accept as-is with rationale").

---

### User Story 2 — Maintainer audits mikebom on a polyglot Go+npm codebase (Priority: P2)

A mikebom maintainer needs to verify that Go + npm work correctly TOGETHER on a real polyglot codebase. Cross-ecosystem interactions (e.g., an npm workspace peer that vendors a Go binary via its `bin:` field; a Go module that shells out to an npm-installed tool) are a source of edge-case bugs not visible when measuring each ecosystem in isolation.

**Why this priority**: Milestones 160-164 measured one ecosystem at a time. ArgoCD is a real polyglot codebase used by tens of thousands of teams. If mikebom has cross-ecosystem gaps, ArgoCD will expose them.

**Independent Test**: Clone `github.com/argoproj/argo-cd` at a pinned commit; run mikebom, Trivy, Syft; produce a report analogous to US1 but with per-ecosystem breakdowns (Go, npm) AND a cross-ecosystem-interaction section.

**Acceptance Scenarios**:

1. **Given** a freshly cloned `github.com/argoproj/argo-cd` at HEAD (pinned commit recorded), **When** the audit runs mikebom + Trivy + Syft, **Then** the report MUST break out counts per ecosystem (`components[]` filtered by `pkg:golang/`, `pkg:npm/`) and note any interactions where a component from one ecosystem edges into another.

2. **Given** any polyglot-specific failure modes surface, **When** the audit classifies them, **Then** the report MUST distinguish them from single-ecosystem failure modes (US1 classifications apply per ecosystem; US2 classifications apply cross-ecosystem).

---

### User Story 3 — Maintainer receives prioritized follow-on milestone recommendations (Priority: P3)

The audit's output feeds the next 1-3 milestones. A prioritized list is the input signal for what to fix next.

**Why this priority**: Without prioritization, the audit is just a snapshot. The recommendation list is what turns measurement into action.

**Independent Test**: Report includes a "Recommended follow-on milestones" section with:
- Top 3 bug classes ranked by (BFS-reachability impact, blast radius, effort estimate).
- For each: a one-paragraph problem statement, an evidence-based impact estimate, and a rough scope-of-fix note.
- Explicit "accept as-is" recommendations for classes that are honest signal rather than bugs (like the 12 residual podman-desktop orphans milestone 164 confirmed are stale lockfile entries, not mikebom bugs).

**Acceptance Scenarios**:

1. **Given** the audit surfaces N distinct failure classes across US1 + US2, **When** the report ranks them, **Then** the top 3 MUST include quantitative impact estimates (e.g., "+X pp BFS reachability", "N components affected", "M edges affected").

2. **Given** an audit-surfaced class turns out to be honest signal rather than a bug (e.g., stale lockfile entries, deliberate optional-only deps), **When** the report addresses it, **Then** the recommendation MUST be "accept as-is" with a rationale — not silently omitted.

### Edge Cases

- **Repo doesn't build / requires setup**: mikebom is a scanner — it operates on source trees without running the target's build. No build required for the audit.

- **External tool version drift**: Trivy and Syft change output shape between versions. Report MUST pin exact versions used (matching milestone 083's convention).

- **Scan takes hours**: Kubernetes is large (~230 MB source). Milestone-094's perf work suggests mikebom scans in seconds even on this scale; report notes actual wall-clock times.

- **Non-deterministic upstream**: HEAD moves. Report pins exact commit SHAs used for the measurement so numbers are reproducible.

- **License-related edge cases**: SPDX license expression handling was overhauled through milestones 146/152/153. Audit reports MAY include license-emission spot-checks (does every `Package.licenseConcluded` decode? are there `LicenseRef-*` placeholders that reveal the bug class milestone 154 addressed?).

- **File-tier component surge**: post-milestone 133, mikebom emits file-tier components for unattributed content. On Kubernetes (huge codebase), this could produce thousands. Report distinguishes "package-tier" from "file-tier" counts.

- **eBPF trace not applicable**: Kubernetes and ArgoCD audits are source-tree scans (`mikebom sbom scan --path <clone>`), NOT eBPF build traces. Constitution Principle VII is preserved (audit doesn't touch the eBPF path).

## Requirements

### Functional Requirements

- **FR-001**: The audit MUST clone `github.com/kubernetes/kubernetes` at a specific commit SHA (recorded in the report) and produce mikebom + Trivy + Syft SBOMs. All three tool invocations MUST use identical scope (source-tree scan, no build).

- **FR-002**: The audit MUST clone `github.com/argoproj/argo-cd` at a specific commit SHA (recorded in the report) and produce mikebom + Trivy + Syft SBOMs identically.

- **FR-003**: For each target, the report MUST document per-tool metrics: total component count, edge count, BFS reachability from `metadata.component`, per-ecosystem breakdown (`pkg:golang/`, `pkg:npm/`, other), scan wall-clock time.

- **FR-004**: For mikebom specifically, the report MUST classify orphans / empty-version PURLs / phantom edges into named root-cause buckets. Each bucket MUST have a concrete example (PURL of an affected component), a numeric count, and either a follow-on milestone recommendation OR an "accept as-is with rationale" disposition.

- **FR-005**: The report MUST include a Tool Comparison Delta section identifying, for each target: (a) components mikebom finds that Trivy misses; (b) components Trivy finds that mikebom misses; (c) components Syft finds that mikebom misses; (d) any tool that emits phantom edges or empty-version PURLs (baseline: milestone 164 established mikebom emits zero on the npm side; verify Go side + verify Trivy/Syft state).

- **FR-006**: The report MUST include per-ecosystem SPDX validation results (SPDX 3 conformance via `spdx3-validate==0.0.5` per memory `reference_spdx3_validator`; SPDX 2.3 conformance via the existing `jsonschema` gate).

- **FR-007**: The report MUST include a Recommended Follow-On Milestones section with the top 3 ranked classes. Ranking factors: (a) BFS-reachability impact in percentage points; (b) blast radius (how many components / edges affected); (c) rough effort estimate (referencing prior-milestone analogs).

- **FR-008**: The report MUST be self-contained — a reader who has never used mikebom before should understand what was measured, how, and what the numbers mean. Include a "how to reproduce" appendix with exact commands.

- **FR-009**: The report MUST be stored in `docs/audits/2026-07-05-kubernetes-argocd.md` (or similar dated path) so the mikebom repo becomes a persistent record of quality over time. Every future audit adds a new dated file.

- **FR-010**: The audit MUST NOT change any mikebom production code. If a critical bug is discovered mid-audit, note it in the report but scope the fix to a follow-on milestone. Milestone 165's deliverable is measurement + report, not a fix.

### Key Entities

- **Audit report**: `docs/audits/YYYY-MM-DD-kubernetes-argocd.md` — the durable deliverable. Follows a standard structure (Executive Summary, Per-Target sections, Comparison Delta, Root-Cause Classifications, Recommended Follow-Ons, Reproduction Appendix).

- **Target snapshot**: a `(repository URL, commit SHA, tool versions, scan-command, output-SBOM path)` tuple recording what was measured. Reproducible if `pnpm-lock.yaml`/`go.sum` haven't drifted (which they won't at a pinned commit).

- **Root-cause bucket**: a named class of mikebom orphans / phantom edges / empty-version PURLs with a concrete example, a count, and a disposition (`fix-in-follow-on-milestone` OR `accept-as-is-with-rationale`).

- **Tool Comparison Delta record**: per (target, ecosystem) pair, a `{mikebom_advantage: [PURLs], trivy_advantage: [PURLs], syft_advantage: [PURLs]}` structure.

## Success Criteria

### Measurable Outcomes

- **SC-001 (report exists + is dated)**: A file at `docs/audits/2026-07-05-kubernetes-argocd.md` (or similar dated path — actual date = merge date) is committed to the repo. Section headers match FR-008's structure.

- **SC-002 (both targets measured)**: The report contains per-tool metrics for BOTH `github.com/kubernetes/kubernetes` AND `github.com/argoproj/argo-cd`. Each target section includes: component count, edge count, BFS reachability, ecosystem breakdown, wall-clock time — for mikebom, Trivy, AND Syft (3 tools × 2 targets = 6 measurements).

- **SC-003 (mikebom failure modes classified)**: For each target, mikebom's orphans / phantom edges / empty-version PURLs are grouped into named root-cause buckets. Each bucket has: name, count, one concrete example (PURL + source-path if applicable), and disposition (fix vs accept).

- **SC-004 (tool comparison delta documented)**: For each target, a Tool Comparison Delta section shows which components/edges each tool detects that the others miss. Zero-effort validation: `jq` recipes that produce the numbers are included in the reproduction appendix.

- **SC-005 (SPDX validation results)**: The report documents SPDX 2.3 + SPDX 3.0.1 validation pass/fail status per target per tool (or "not attempted" where not applicable — e.g., Trivy's SPDX support may be limited).

- **SC-006 (top 3 recommended follow-ons ranked)**: The report includes a Recommended Follow-On Milestones section with the top 3 candidates. Each has: a problem statement, an evidence-based impact estimate (in BFS-reachability percentage points OR component-count OR edge-count), and a rough scope-of-fix estimate (referencing analogous prior milestones for effort calibration).

- **SC-007 (pre-PR gate — audit doesn't break code)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST both pass with zero errors — verifying FR-010 (no production code changes).

- **SC-008 (byte-identity of all goldens)**: 100% of the milestone-090 golden fixtures (all ecosystems × all formats) MUST be byte-identical to pre-165. Doc-only milestone — zero SBOM emission changes.

- **SC-009 (reproduction appendix is self-contained)**: A fresh contributor with only the repo checked out can run the exact commands in the Reproduction Appendix and reproduce all numbers in the report (subject to upstream drift disclosed in the appendix header).

- **SC-010 (pinned commit SHAs)**: Every measurement is anchored to specific commit SHAs of the upstream repos (recorded in the report header). Numbers are reproducible if run against those exact commits + the exact tool versions.

- **SC-011 (audit surfaces at least 1 actionable bug class OR concludes cleanly)**: EITHER the audit identifies ≥1 previously-unknown bug class that becomes the top follow-on milestone OR the audit concludes that mikebom's quality is at parity with Trivy/Syft on these targets and recommends "no immediate follow-on needed" with evidence. Both outcomes are valid; the report MUST reach one of them.

## Assumptions

- **Live upstream is the test data**: Both Kubernetes and ArgoCD are cloned live from `github.com/kubernetes/kubernetes` and `github.com/argoproj/argo-cd` at the audit's execution time. The commit SHAs used are recorded in the report header. This matches milestone 164's live-podman-desktop approach — the memory `feedback_cross_host_goldens` clarifies fixtures are only warranted when a specific bug's regression guard needs them.

- **Trivy + Syft version pins**: matching milestone 083's convention. Trivy 0.71.1 + Syft 1.44.0 (or newer — pinned at execution time and recorded in the report).

- **spdx3-validate is available**: per memory `reference_spdx3_validator`, `.venv/spdx3-validate/bin/spdx3-validate` at pinned version 0.0.5.

- **Fresh clones may take time**: kubernetes source tree is ~230 MB; argocd ~150 MB. Clone times are recorded in the reproduction appendix but not in scan-time metrics (scan wall-clock excludes clone time).

- **No mikebom code changes**: this is a doc-only milestone. All FRs are about what the REPORT says, not what mikebom does.

- **Post-164 baseline is the reference**: mikebom binary used = milestone-164 release build (`bfd0f6d`-descended). Comparison numbers presented AS OF the post-164 baseline.

- **Report can be revised**: if a follow-on milestone (166+) surfaces from milestone 165 and lands quickly, the audit report MAY be updated with a "post-166 update" section, but that's out of scope for milestone 165 itself.

- **Audit is one-shot**: milestone 165 produces ONE audit report. A future milestone 200 (or whatever) may repeat the audit as a Round-4 measurement.

## Out of Scope

- **Fixing anything discovered during the audit** — every fix is a follow-on milestone. Milestone 165's deliverable is measurement + report only.

- **Comparing beyond Trivy + Syft** — the reference set is mikebom vs Trivy vs Syft, matching milestone 158 T035. Other tools (Snyk, GitHub Dependency Graph, Sonatype, etc.) are out of scope for this round.

- **Additional targets beyond Kubernetes + ArgoCD** — the two-target scope is intentional. Adding rustdesk / next.js / rails / etc. is a future audit round.

- **Building or running the target repos** — mikebom is a static analyzer. The audit is source-tree scan only; no `go build`, no `pnpm install`, no `docker build`.

- **eBPF build-trace measurements** — this is a source-scan audit. Milestone-020 eBPF work is unmodified and unaudited by 165.

- **License-emission audit as a first-class deliverable** — license spot-checks are a bonus per FR-006 but not the primary focus. A dedicated license audit would be a separate milestone.

- **Automated CI-gating of audit metrics** — the report is a snapshot. Future milestones could add per-target audit tests behind opt-in env vars (matches milestone 164 T020 pattern), but that's out of scope for 165.

- **Follow-on milestone speculation beyond top 3** — the report ranks the top 3 candidates. A longer list may be tracked in the report's "Backlog observations" appendix but is not the primary deliverable.
