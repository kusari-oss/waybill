# Feature Specification: Empirical audit of mikebom against Rust + Python monorepos (Round 4 measurement)

**Feature Branch**: `168-rust-python-audit`
**Created**: 2026-07-06
**Status**: Draft
**Input**: Discovery-driven follow-on after milestone 165 (Round 3 — Kubernetes + ArgoCD) landed with a clean-pass-plus-one-bug outcome. Milestone 165's #3 top-3 recommendation named this milestone explicitly (see `docs/audits/2026-07-05-kubernetes-argocd.md:219-232`). Extend the audit pattern to two large real-world targets exercising ecosystems + failure modes mikebom has NEVER been measured at scale on: **Tauri** (Rust workspace + npm polyglot, exercises Cargo readers at scale + cross-ecosystem interactions with npm) and **Apache Airflow** (canonical Python monorepo — real-world pip + poetry + uv + PEP 621 pyproject usage across ~1000+ transitive deps).

## Motivation

The measurement → root-cause → fix → merge pattern from milestones 158/165 surfaced 6 bug classes across 3 rounds. Post-167, the Go + npm ecosystems are at parity with best-in-class SBOM tools (m164: 99.6% BFS on podman-desktop; m165: 92.0% on Kubernetes, 98.2% on ArgoCD; m167: extended C45 vocabulary to differentiate honest-signal orphans from bugs).

Two open questions:

1. **Does mikebom's quality generalize to Rust + Python?** Milestones 064/087 (Cargo main-module + transitive edges) and 066/106 (pip/uv/poetry ecosystem coverage) landed. Both ecosystems have local test coverage but neither has been measured against a real large-scale monorepo. Extrapolating m165's hit rate (Round 3 surfaced 1 bug class + strong positive quality validation), Round 4 likely surfaces 0-3 additional bug classes across these two ecosystems.

2. **Does mikebom's polyglot handling work on Rust + npm together?** ArgoCD (m165 US2) validated Go + npm polyglot correctness. Tauri is a real Rust + npm polyglot in wide production use (Rust workspace hosting a native shell; each app has an embedded npm frontend). Cross-ecosystem interactions (a Cargo crate that vendors an npm-bundled asset; a Rust binary that shells out to an npm-installed tool; alias-binding via milestone 111's `--pkg-alias` flow) are corner cases only real-world targets expose.

Both targets are canonical:

- **Tauri** (`github.com/tauri-apps/tauri`): the reference Rust + web polyglot framework. ~50 Rust crates in the workspace + a substantial npm dependency tree per app. Exercises Cargo workspace resolution (milestone 064 US1 main-module → milestone 087 transitive edges → milestone 088 procmacro-edges → milestone 116 produces-binaries), npm dependency graphs (milestones 065-067 + m164 pnpm v9 disambiguation), AND their cross-ecosystem interactions (milestone 111 alias-binding + milestone 116 produces-binaries emission).

- **Apache Airflow** (`github.com/apache/airflow`): the reference Python monorepo. ~1000+ transitive deps across a hybrid pip + Poetry + uv + PEP 621 pyproject.toml pattern (Airflow migrated to uv in 2025). Exercises Python ecosystem coverage at scale — mikebom emits Python components via milestone 066 (US1 requirements.txt reader) + milestone 106 (uv.lock parser) + related. Also exercises the SPDX 3 emission code paths on a large non-Go/non-npm target.

**This is an audit milestone.** Deliverable: a persistable audit report at `docs/audits/<date>-tauri-airflow.md` + prioritized follow-on milestone recommendations. Zero production code changes. Matches milestone 165's structure verbatim; matches milestone 158 T035, milestone 093 docs polish, and milestone 150/151 consumer-guide pattern (report-only milestones).

## Distinction from prior audit efforts

- **Milestone 158 T035**: measured mikebom vs Trivy + Syft on `test-podman-desktop` (npm-heavy). Round 1. Surfaced 5 bug classes → milestones 160/161/162/163/164.
- **Milestone 165**: extended the measurement to Kubernetes + ArgoCD (Go + polyglot). Round 3. Surfaced 1 bug class → milestone 166; plus the m167 vocabulary work as top-3 #2.
- **Milestone 168 (this)**: extends the pattern to Tauri + Airflow (Rust + Python). Round 4. Same tools (mikebom, Trivy, Syft). Same measurement methodology (BFS reachability, orphan classification, ecosystem coverage delta). Different targets exercising ecosystems mikebom has never been measured on at scale.
- **Milestone 083 audit harness**: cross-tool parity harness with pinned fixtures. Different scope — this is a live-upstream measurement, not a pinned regression fixture.

## Clarifications

### Session 2026-07-06

- Q: What is the Tauri scan target — repo root (widest scope, includes example apps' npm deps) OR `crates/` only OR repo root minus `examples/**`? → A: **Repo root** (Option A). Widest measurement; exercises the polyglot handling the milestone was designed to test (Rust workspace + npm example apps together). Cross-ecosystem edges + m111 alias-binding + m116 produces-binaries all in play. Trivy + Syft receive the same target for direct comparison.

- Q: What is the Airflow scan target — repo root (all Python sources including `providers/*`) OR excluding providers OR production-only (excluding providers + dev/tests)? → A: **Repo root** (Option A). Widest measurement; picks up top-level pyproject/uv.lock PLUS every `providers/*` subdir Python declaration. ~1000+ dep count is a feature — maximum LicenseRef-* + SPDX validation stress per FR-006. Symmetric with Q1 (Tauri).

- Q: How does FR-011 Cross-Round Trend Analysis handle m165 baselines potentially staled by m167's post-hoc emission changes — verbatim, verbatim+caveat, or freshness appendix? → A: **Verbatim + one-line caveat per affected metric** (Option B). m168 does NOT re-measure m165 targets (Out of Scope preserved); FR-011 uses m165's frozen numbers as trend baselines BUT explicitly notes per metric where m167's changes may have altered the baseline (e.g., orphan-reason counts on ArgoCD npm side would now be non-zero post-167 whereas m165 recorded zero emitted orphan-reasons on npm). Best transparency-per-effort ratio.

## User Scenarios & Testing

### User Story 1 — Maintainer audits mikebom quality on a Rust workspace at scale (Priority: P1)

A mikebom maintainer needs to know whether mikebom's Rust-ecosystem quality (post-milestones 064/087/088/116) holds on a real Rust workspace at scale, or whether new failure classes emerge that weren't visible on smaller test targets. Post-168, the maintainer can:

- Read a documented measurement of mikebom + Trivy + Syft on Tauri source tree.
- See the delta between the three tools (component count, edge count, BFS reachability, per-tool advantages).
- See a root-cause classification of any orphans, phantom edges, or empty-version PURLs mikebom emits on the Cargo side.
- See how milestone 111's alias-binding and milestone 116's produces-binaries emission perform on a real polyglot workspace.
- See the top 3 highest-ROI follow-on milestone candidates the audit surfaces.

**Why this priority**: Tauri is the most-visible Rust-based app-framework project in wide production use. If mikebom's Rust quality is broken on Tauri, it's broken across the Rust ecosystem. High-ROI baseline measurement.

**Independent Test**: Clone `github.com/tauri-apps/tauri` at a pinned commit; run mikebom, Trivy, Syft; produce a report with per-tool component/edge counts + BFS reachability + per-tool delta table + root-cause classification of mikebom's Rust-side failure modes + a cross-ecosystem-interaction section (Cargo ↔ npm edges, alias-binding hits, produces-binaries emissions).

**Acceptance Scenarios**:

1. **Given** a freshly cloned `github.com/tauri-apps/tauri` at HEAD (pinned commit recorded in the report), **When** the audit runs mikebom + Trivy + Syft against it, **Then** the report MUST document each tool's total component count, edge count, and BFS reachability percentage.

2. **Given** any orphans / empty-version PURLs / phantom edges mikebom emits on the Cargo side, **When** the audit classifies them by root cause, **Then** the report MUST assign each class a specific reason bucket (e.g., "workspace member missing from resolve", "path-dep with no version", "build-dep classified as runtime", "procmacro edge miscount") AND a follow-on milestone recommendation (or "accept as-is with rationale").

3. **Given** milestone 116's produces-binaries emission fires on Tauri's main-module Cargo components, **When** the audit inspects the emitted SBOM, **Then** the report MUST document the binary count + accuracy (spot-check that emitted names match `[[bin]]` + `src/main.rs` + `src/bin/*.rs` per FR-005 clarification from m116).

4. **Given** cross-ecosystem edges (Cargo ↔ npm) are possible on Tauri (e.g., a Cargo crate that vendors an npm-installed asset directory), **When** the audit inspects the emitted graph, **Then** the report MUST document whether any cross-ecosystem edges are emitted AND classify them as correct or spurious.

---

### User Story 2 — Maintainer audits mikebom on a Python monorepo at scale (Priority: P2)

A mikebom maintainer needs to verify that Python (pip + Poetry + uv + PEP 621 pyproject) works correctly on a real Python monorepo at scale. Airflow's dependency graph is one of the largest in the Python ecosystem (~1000+ transitive deps) and uses a hybrid dependency-declaration pattern that stresses multiple mikebom Python readers.

**Why this priority**: Python is one of mikebom's supported ecosystems but has never been measured at scale. If Python quality is broken at scale, Airflow will expose it. High-ROI baseline measurement for Python.

**Independent Test**: Clone `github.com/apache/airflow` at a pinned commit; run mikebom, Trivy, Syft; produce a report analogous to US1 but focused on Python — pip vs Poetry vs uv coverage, requirements.txt vs pyproject.toml source attribution, license-emission spot-check across the ~1000 deps (which includes many `LicenseRef-*` cases exercised by m146/152/153/154 SPDX license work).

**Acceptance Scenarios**:

1. **Given** a freshly cloned `github.com/apache/airflow` at HEAD (pinned commit recorded), **When** the audit runs mikebom + Trivy + Syft, **Then** the report MUST break out counts per source (pip requirements files, Poetry pyproject, uv.lock, PEP 621 pyproject) and per-tool total.

2. **Given** any Python-specific failure modes surface (e.g., extras-groups double-counting, editable installs with no version, git-URL deps missing PURLs), **When** the audit classifies them, **Then** the report MUST distinguish them from Rust-side classes (US1 classifications apply per ecosystem).

3. **Given** Airflow's dependency graph includes many `LicenseRef-*` licenses (Airflow itself + many transitive deps have non-standard license expressions), **When** the audit spot-checks license emission, **Then** the report MUST document milestone-146/152/153/154 SPDX license work behavior at scale — verify no dropped operands, no unresolved `LicenseRef-*` placeholders that break `spdx3-validate`.

---

### User Story 3 — Maintainer receives prioritized follow-on milestone recommendations (Priority: P3)

The audit's output feeds the next 1-3 milestones. A prioritized list is the input signal for what to fix next.

**Why this priority**: Without prioritization, the audit is just a snapshot. The recommendation list is what turns measurement into action. Matches m165 US3 verbatim.

**Independent Test**: Report includes a "Recommended follow-on milestones" section with:

- Top 3 bug classes ranked by (BFS-reachability impact, blast radius, effort estimate).
- For each: a one-paragraph problem statement, an evidence-based impact estimate, and a rough scope-of-fix note.
- Explicit "accept as-is" recommendations for classes that are honest signal rather than bugs.
- Cross-round trend analysis: does m168 surface classes m158/m165 also saw? (evidence that a pattern spans ecosystems is a stronger signal for a fix).

**Acceptance Scenarios**:

1. **Given** the audit surfaces N distinct failure classes across US1 + US2, **When** the report ranks them, **Then** the top 3 MUST include quantitative impact estimates (e.g., "+X pp BFS reachability", "N components affected", "M edges affected").

2. **Given** an audit-surfaced class turns out to be honest signal rather than a bug, **When** the report addresses it, **Then** the recommendation MUST be "accept as-is" with a rationale — not silently omitted.

3. **Given** a class m168 surfaces was also surfaced by m158 or m165, **When** the report highlights the pattern, **Then** the milestone recommendation MUST call out the cross-round evidence explicitly (drives urgency vs one-off classes).

### Edge Cases

- **Repo doesn't build / requires setup**: mikebom is a scanner — it operates on source trees without running the target's build. No build required for the audit.

- **External tool version drift**: Trivy and Syft change output shape between versions. Report MUST pin exact versions used (matching m083 + m165 convention).

- **Scan takes hours**: Tauri is medium (~50 MB source); Airflow is large (~200 MB). Milestone-094's perf work suggests mikebom scans in seconds even on this scale; report notes actual wall-clock times.

- **Non-deterministic upstream**: HEAD moves. Report pins exact commit SHAs used so numbers are reproducible.

- **Python virtual env not present**: mikebom scans source trees, not installed envs. If Airflow's uv.lock is present, the transitive graph will be authoritative. If not, mikebom falls back to requirements.txt / pyproject.toml (design-tier vs source-tier per m106). Report documents which tier each Python component came from.

- **Trivy's Python support**: Trivy detects pip requirements + poetry.lock + Pipfile.lock but has patchier PEP 621 pyproject handling. Report notes where Trivy under-counts vs mikebom's more-thorough source-tier readers.

- **License-related edge cases**: SPDX license expression handling was overhauled through milestones 146/152/153/154. Audit reports MUST spot-check license emission on Airflow's 1000+ deps — this is likely the fattest LicenseRef-* target in the mikebom ecosystem.

- **File-tier component surge**: post-milestone 133, mikebom emits file-tier components for unattributed content. On Airflow (many docs, tests, configs), this could produce hundreds. Report distinguishes package-tier from file-tier counts.

- **eBPF trace not applicable**: Tauri and Airflow audits are source-tree scans (`mikebom sbom scan --path <clone>`), NOT eBPF build traces. Constitution Principle VII is preserved.

- **Cargo workspace member confusion**: Tauri has multiple workspace members. The m127 root-selection heuristic + m064 main-module emission need to fire correctly on all of them. Report spot-checks that the correct main-module is selected + that per-member `produces-binaries` fires per m116.

## Requirements

### Functional Requirements

- **FR-001**: The audit MUST clone `github.com/tauri-apps/tauri` at a specific commit SHA (recorded in the report) and produce mikebom + Trivy + Syft SBOMs. All three tool invocations MUST use identical scope: **repo root as scan target** (per clarification 2026-07-06 Q1) — source-tree scan, no build, no path exclusions. This scope includes the Cargo workspace at root PLUS every example app's npm dep tree under `examples/`, so cross-ecosystem interactions (Rust workspace + npm example apps) are measured together per US1's polyglot-focus motivation.

- **FR-002**: The audit MUST clone `github.com/apache/airflow` at a specific commit SHA (recorded in the report) and produce mikebom + Trivy + Syft SBOMs identically: **repo root as scan target** (per clarification 2026-07-06 Q2) — source-tree scan, no build, no path exclusions. This scope includes the top-level `pyproject.toml` / `uv.lock` PLUS every `providers/*` subdir's own Python declarations, so mikebom's full Python-reader surface is exercised at real scale and the ~1000+ dep count stresses m146/152/153/154 SPDX license work per FR-006.

- **FR-003**: For each target, the report MUST document per-tool metrics: total component count, edge count, BFS reachability from `metadata.component`, per-ecosystem breakdown (`pkg:cargo/`, `pkg:npm/`, `pkg:pypi/`, other), scan wall-clock time.

- **FR-004**: For mikebom specifically, the report MUST classify orphans / empty-version PURLs / phantom edges into named root-cause buckets. Each bucket MUST have a concrete example (PURL of an affected component), a numeric count, and either a follow-on milestone recommendation OR an "accept as-is with rationale" disposition. Buckets MUST be aligned with the m167 C45 vocabulary where applicable — orphans matching `stale-go-sum-entry` / `dead-lockfile-entry` / `hoisted-unused` / `unresolved-indirect-require` / `flat-attached-fallback` MUST be counted against those codes; new codes MAY be proposed for Rust or Python failure modes not covered by m167.

- **FR-005**: The report MUST include a Tool Comparison Delta section identifying, for each target: (a) components mikebom finds that Trivy misses; (b) components Trivy finds that mikebom misses; (c) components Syft finds that mikebom misses; (d) any tool that emits phantom edges or empty-version PURLs (baseline: milestone 164 established mikebom emits zero on the npm side; verify Cargo + Python sides + verify Trivy/Syft state).

- **FR-006**: The report MUST include per-ecosystem SPDX validation results (SPDX 3 conformance via `spdx3-validate==0.0.5` per memory `reference_spdx3_validator`; SPDX 2.3 conformance via the existing `jsonschema` gate). Airflow's 1000+ Python dep license expressions MUST be exercised through the SPDX 3 validator to catch any m154 custom-license regressions at scale.

- **FR-007**: The report MUST include a Recommended Follow-On Milestones section with the top 3 ranked classes. Ranking factors: (a) BFS-reachability impact in percentage points; (b) blast radius (how many components / edges affected); (c) rough effort estimate (referencing prior-milestone analogs).

- **FR-008**: The report MUST be self-contained — a reader who has never used mikebom before should understand what was measured, how, and what the numbers mean. Include a "how to reproduce" appendix with exact commands.

- **FR-009**: The report MUST be stored in `docs/audits/<YYYY-MM-DD>-tauri-airflow.md` so the mikebom repo becomes a persistent record of quality over time. Every future audit adds a new dated file.

- **FR-010**: The audit MUST NOT change any mikebom production code. If a critical bug is discovered mid-audit, note it in the report but scope the fix to a follow-on milestone. Milestone 168's deliverable is measurement + report, not a fix.

- **FR-011**: The report MUST include a Cross-Round Trend Analysis section comparing m168's findings against m158 (Round 1) + m165 (Round 3) baselines. Baselines are used **verbatim from the pre-recorded m165/m158 audit reports** (no re-measurement — see Out of Scope) BUT the report MUST attach a one-line freshness caveat to every m165/m158 metric where a post-baseline milestone (166/167/168-itself) plausibly altered the number if re-measured (e.g., "m167 added orphan-reason emission on npm; m165's ArgoCD zero-emitted-orphan-reasons row would now be non-zero if re-measured"). Per clarification 2026-07-06 Q3. If a bug class recurs across ecosystems, the report MUST highlight the cross-round evidence and factor it into the top-3 ranking.

- **FR-012**: The report MUST include per-target measurement of milestone 167's new C45 orphan-reason vocabulary. For each Cargo/npm/Python orphan, verify the emitted `mikebom:orphan-reason` (if any) matches the audit's external classification. Delta between mikebom's emitted classification and external classification is itself a signal — either mikebom missed a category or the audit methodology needs refinement.

### Key Entities

- **Audit report**: `docs/audits/<YYYY-MM-DD>-tauri-airflow.md` — the durable deliverable. Follows the m165 structure (Executive Summary, Per-Target sections, Comparison Delta, Root-Cause Classifications, Recommended Follow-Ons, Reproduction Appendix, Cross-Round Trend Analysis).

- **Target snapshot**: a `(repository URL, commit SHA, tool versions, scan-command, output-SBOM path)` tuple recording what was measured. Reproducible if `Cargo.lock` / `uv.lock` / `pyproject.toml` haven't drifted at the pinned commit.

- **Root-cause bucket**: a named class of mikebom orphans / phantom edges / empty-version PURLs / license drops with a concrete example, a count, and a disposition (`fix-in-follow-on-milestone` OR `accept-as-is-with-rationale`).

- **Tool Comparison Delta record**: per (target, ecosystem) pair, a `{mikebom_advantage: [PURLs], trivy_advantage: [PURLs], syft_advantage: [PURLs]}` structure.

- **Cross-Round Trend record**: for each surfaced bug class in m168, a `{class: name, m158_seen: bool, m165_seen: bool, m168_seen: true, priority_multiplier: <1|2|3>}` structure.

## Success Criteria

### Measurable Outcomes

- **SC-001 (report exists + is dated)**: A file at `docs/audits/<YYYY-MM-DD>-tauri-airflow.md` is committed to the repo. Section headers match FR-008's structure (mirroring m165).

- **SC-002 (both targets measured)**: The report contains per-tool metrics for BOTH `github.com/tauri-apps/tauri` AND `github.com/apache/airflow`. Each target section includes: component count, edge count, BFS reachability, ecosystem breakdown, wall-clock time — for mikebom, Trivy, AND Syft (3 tools × 2 targets = 6 measurements).

- **SC-003 (mikebom failure modes classified)**: For each target, mikebom's orphans / phantom edges / empty-version PURLs are grouped into named root-cause buckets. Each bucket has: name, count, one concrete example (PURL + source-path if applicable), and disposition (fix vs accept). Where applicable, buckets align with the m167 C45 vocabulary.

- **SC-004 (tool comparison delta documented)**: For each target, a Tool Comparison Delta section shows which components/edges each tool detects that the others miss. Zero-effort validation: `jq` recipes that produce the numbers are included in the reproduction appendix.

- **SC-005 (SPDX validation results)**: The report documents SPDX 2.3 + SPDX 3.0.1 validation pass/fail status per target per tool. Airflow's 1000+ Python dep license expressions are exercised through `spdx3-validate` and results recorded.

- **SC-006 (top 3 recommended follow-ons ranked)**: The report includes a Recommended Follow-On Milestones section with the top 3 candidates. Each has: a problem statement, an evidence-based impact estimate (in BFS-reachability percentage points OR component-count OR edge-count), and a rough scope-of-fix estimate (referencing analogous prior milestones for effort calibration).

- **SC-007 (pre-PR gate — audit doesn't break code)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST both pass with zero errors — verifying FR-010 (no production code changes).

- **SC-008 (byte-identity of all goldens)**: 100% of the milestone-090 golden fixtures (all ecosystems × all formats) MUST be byte-identical to pre-168. Doc-only milestone — zero SBOM emission changes.

- **SC-009 (reproduction appendix is self-contained)**: A fresh contributor with only the repo checked out can run the exact commands in the Reproduction Appendix and reproduce all numbers in the report (subject to upstream drift disclosed in the appendix header).

- **SC-010 (pinned commit SHAs)**: Every measurement is anchored to specific commit SHAs of the upstream repos (recorded in the report header). Numbers are reproducible if run against those exact commits + the exact tool versions.

- **SC-011 (audit surfaces at least 1 actionable bug class OR concludes cleanly)**: EITHER the audit identifies ≥1 previously-unknown bug class that becomes the top follow-on milestone OR the audit concludes that mikebom's quality is at parity with Trivy/Syft on these targets and recommends "no immediate follow-on needed" with evidence. Both outcomes are valid; the report MUST reach one of them. If a cross-round pattern is confirmed (a class m168 sees that m158 or m165 also saw), it MUST be flagged as a stronger fix priority than a one-off finding.

- **SC-012 (m167 vocabulary applied to Rust + Python orphans)**: The report explicitly documents whether the m167 C45 orphan-reason vocabulary is sufficient for the Rust + Python classes surfaced, OR proposes vocabulary extensions for Rust/Python-specific classes. If extension is proposed, it becomes a candidate follow-on milestone (analogous to m167 itself being a follow-on to m165's #2 recommendation).

## Assumptions

- **Live upstream is the test data**: Both Tauri and Airflow are cloned live from `github.com/tauri-apps/tauri` and `github.com/apache/airflow` at the audit's execution time. The commit SHAs used are recorded in the report header. Matches m164/m165 live-upstream approach.

- **Trivy + Syft version pins**: matching m083/m165 convention. Trivy 0.71.1 + Syft 1.44.0 (or newer — pinned at execution time and recorded in the report). If m165's install-friction issues persist (brew tap serving old Trivy; `go install` requiring Go 1.26+), reproduction appendix documents the direct-binary-download fallback.

- **spdx3-validate is available**: per memory `reference_spdx3_validator`, `.venv/spdx3-validate/bin/spdx3-validate` at pinned version 0.0.5.

- **Fresh clones may take time**: Tauri source tree is ~50 MB; Airflow ~200 MB. Clone times are recorded in the reproduction appendix but not in scan-time metrics (scan wall-clock excludes clone time).

- **No mikebom code changes**: this is a doc-only milestone. All FRs are about what the REPORT says, not what mikebom does.

- **Post-167 baseline is the reference**: mikebom binary used = post-milestone-167 release build (`ccde910`-descended). Comparison numbers presented AS OF the post-167 baseline. Where the m167 orphan-reason vocabulary is applied to Rust or Python components, this is a NEW measurement (m167 emits only on Go + npm today per FR-001 scope).

- **Report can be revised**: if a follow-on milestone (169+) surfaces from milestone 168 and lands quickly, the audit report MAY be updated with a "post-169 update" section, but that's out of scope for milestone 168 itself.

- **Audit is one-shot**: milestone 168 produces ONE audit report. A future milestone (Round 5) may repeat the audit against different targets.

- **`analyze.py` reuse**: milestone-165's `specs/165-k8s-argocd-audit/scripts/analyze.py` is intentionally target-agnostic (per m165 backlog observation). Milestone 168 reuses it verbatim with new target names + SHAs. If Rust or Python surfaces novel classification needs, `analyze.py` extensions land as part of m168's audit-time work (not a mikebom production code change).

## Out of Scope

- **Fixing anything discovered during the audit** — every fix is a follow-on milestone. Milestone 168's deliverable is measurement + report only. Matches m165 verbatim.

- **Comparing beyond Trivy + Syft** — the reference set is mikebom vs Trivy vs Syft, matching m158/m165. Other tools (Snyk, GitHub Dependency Graph, Sonatype, cdxgen, etc.) are out of scope for this round.

- **Additional targets beyond Tauri + Airflow** — the two-target scope is intentional. Adding `rust-lang/cargo` or `home-assistant/core` or `denoland/deno` is a future audit round.

- **Building or running the target repos** — mikebom is a static analyzer. The audit is source-tree scan only; no `cargo build`, no `uv sync`, no `docker build`.

- **eBPF build-trace measurements** — this is a source-scan audit. Milestone-020 eBPF work is unmodified and unaudited by 168.

- **License-emission audit as a first-class deliverable** — license spot-checks are a bonus per FR-006 but not the primary focus. A dedicated license audit would be a separate milestone. (Note: Airflow is a strong candidate FOR such an audit given its LicenseRef-* scale, but that's a future milestone.)

- **Automated CI-gating of audit metrics** — the report is a snapshot. Future milestones could add per-target audit tests behind opt-in env vars (matches milestone 164 T020 pattern + m165 T037), but that's out of scope for 168.

- **Follow-on milestone speculation beyond top 3** — the report ranks the top 3 candidates. A longer list may be tracked in the report's "Backlog observations" appendix but is not the primary deliverable.

- **Re-measuring m158/m165 targets** — this round measures new targets; podman-desktop / Kubernetes / ArgoCD re-runs are a separate future audit round. The Cross-Round Trend Analysis section (FR-011) uses the PRE-RECORDED m158/m165 numbers as reference, not fresh measurements.
