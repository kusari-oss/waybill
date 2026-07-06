---
description: "Task list for milestone 165 — empirical audit of mikebom against Kubernetes + ArgoCD"
---

# Tasks: Empirical audit of mikebom against Kubernetes + ArgoCD

**Input**: Design documents from `/specs/165-k8s-argocd-audit/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/README.md, quickstart.md

**Tests**: NONE — this is a docs+measurement milestone. FR-010 forbids production code changes; SC-007 + SC-008 verify no code drift via existing pre-PR gate + golden byte-identity checks. No new mikebom tests added.

**Organization**: Tasks grouped by 3 user stories from spec.md (US1 P1 Kubernetes Go-at-scale audit, US2 P2 ArgoCD polyglot audit, US3 P3 prioritized follow-on recommendations). US1 is the load-bearing MVP; US2 extends coverage; US3 synthesizes both.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no dependencies on incomplete tasks — parallelizable.
- **[Story]**: US1 / US2 / US3 for user-story tasks. Setup, Foundational, Polish have NO story label.

## Path Conventions

Single-project workspace layout per plan.md §Project Structure. All paths absolute-relative to repo root `/Users/mlieberman/Projects/mikebom/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify audit toolchain per research.md R1-R3 before doing any scans.

- [X] T001 Verify Trivy 0.71.1 is installed: `trivy --version | head -1` MUST show `Version: 0.71.1`. If missing or newer, install via `brew install aquasecurity/trivy/trivy` (macOS) OR `go install github.com/aquasecurity/trivy/cmd/trivy@v0.71.1`. If newer than 0.71.1, record the actual version — the report will note the drift per research §R1.
- [X] T002 Verify Syft 1.44.0 is installed: `syft version | head -1` MUST show `Syft 1.44.0`. Pre-2026-07-05 measurement confirmed 1.44.0 is already installed on this host.
- [X] T003 Verify spdx3-validate 0.0.5 at `.venv/spdx3-validate/bin/spdx3-validate --version`. MUST show `0.0.5` per memory `reference_spdx3_validator`.
- [X] T004 Verify mikebom release build exists and is milestone-164-descended: `cargo +stable build --release -p mikebom` succeeds AND `git log --oneline -1` shows commit `de66352` (post-merge milestone 164) OR later main-branch descendant. Record the actual mikebom baseline SHA for the report header.

**Checkpoint**: All 4 tools available at their pinned versions. Any drift recorded for the report's Baseline section.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Create the deliverable directory structure + build the analysis script that both US1 and US2 depend on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T005 Create the directory structure per plan.md §Project Structure: `mkdir -p docs/audits specs/165-k8s-argocd-audit/artifacts/kubernetes specs/165-k8s-argocd-audit/artifacts/argocd specs/165-k8s-argocd-audit/scripts`. Create `specs/165-k8s-argocd-audit/artifacts/.gitignore` per quickstart.md §3b with `*.json`, `*.spdx.json`, `*.cdx.json` patterns — matches milestone-090 fixture-stayset guidance (regenerable intermediates aren't versioned).
- [X] T006 Write the analysis Python script at `specs/165-k8s-argocd-audit/scripts/analyze.py`. Per research §R5 + data-model.md E2 + E3 + E4: takes `--target-name`, `--sboms-dir`, `--commit-sha` args; reads all 3 CDX SBOMs (mikebom, trivy, syft) from `<sboms-dir>`; computes per-tool metrics (total components, edges, BFS reachability, ecosystem breakdown, empty-version PURLs, phantom edges); classifies mikebom's orphans into named buckets per research §R6; computes tool-comparison deltas per research §R7 for `ecosystem: "golang" | "npm" | "cross"` variants — **including cross-ecosystem edge detection** (for each `pkg:npm/...` component's `dependsOn`, check if any target is `pkg:golang/...` or vice versa; emit a `cross_ecosystem_interactions` field per data-model.md E4 cross variant). This upfront support unblocks parallel US1 and US2 execution — no mid-audit script extension needed. Emits an `analysis.json` structured per data-model.md schema. Python 3.10+; stdlib-only (`json`, `collections.deque`, `pathlib`); ~200-250 lines (bumped from 150-200 to accommodate cross-ecosystem logic).
- [X] T007 Write a simple runner shell script at `specs/165-k8s-argocd-audit/scripts/run-audit.sh` that glues together **clone + 5 scans per target only** (mikebom CDX + mikebom SPDX 2.3 + mikebom SPDX 3 + Trivy CDX + Syft CDX). Takes `--target <kubernetes|argocd>` + `--workdir <path>` args. Emits progress + wall-clock timings via `time` per scan. **Does NOT run `analyze.py` or SPDX validation** — those are separate standalone tasks (T012/T019 for analysis; T013/T020 for validation) so their invocations are auditable per-target and their outputs are independently reviewable.

**Checkpoint**: Analysis script + runner script exist and are executable. Ready for per-target audit runs.

---

## Phase 3: User Story 1 — Kubernetes Go-monorepo audit (Priority: P1) 🎯 MVP

**Goal**: Measure mikebom + Trivy + Syft against `github.com/kubernetes/kubernetes` at scale. Produce the per-target Kubernetes section of the audit report.

**Independent Test**: `docs/audits/2026-07-05-kubernetes-argocd.md` contains a complete Target 1 (Kubernetes) section including: snapshot metadata (URL + commit SHA + clone size + scan wall-clocks), per-tool metrics table (mikebom | trivy | syft × {components, edges, BFS, per-ecosystem, wall-clock}), root-cause buckets with dispositions, tool-comparison delta lists, SPDX validation results.

### Tasks for User Story 1

- [X] T008 [US1] Clone `github.com/kubernetes/kubernetes` at HEAD via `./specs/165-k8s-argocd-audit/scripts/run-audit.sh --target kubernetes --workdir <tempdir>`. Record: (a) `KUBE_SHA` (commit SHA), (b) clone size in bytes (`du -sb`), (c) clone wall-clock time. These land in the report's Target 1 Snapshot section per data-model.md E1.
- [X] T009 [US1] Run mikebom against kubernetes source tree — CDX + SPDX 2.3 + SPDX 3 (3 output formats per SC-005). Record wall-clock per format. Outputs land at `specs/165-k8s-argocd-audit/artifacts/kubernetes/mikebom.{cdx,spdx23,spdx3}.json`. If ANY scan fails (crash, timeout > 10 min), stop and record the failure as a CRITICAL top-of-Recommendations finding — mikebom regression on Kubernetes source is milestone-blocking.
- [X] T010 [US1] Run Trivy against kubernetes source tree — CDX only. `trivy fs --format cyclonedx --output specs/165-k8s-argocd-audit/artifacts/kubernetes/trivy.cdx.json <clone-path>`. Record wall-clock.
- [X] T011 [US1] Run Syft against kubernetes source tree — CDX only. `syft <clone-path> --output cyclonedx-json > specs/165-k8s-argocd-audit/artifacts/kubernetes/syft.cdx.json`. Record wall-clock.
- [X] T012 [US1] Run `analyze.py` on Kubernetes SBOMs: `python3 specs/165-k8s-argocd-audit/scripts/analyze.py --target-name kubernetes --sboms-dir specs/165-k8s-argocd-audit/artifacts/kubernetes --commit-sha $KUBE_SHA > specs/165-k8s-argocd-audit/artifacts/kubernetes/analysis.json`. Verify: milestone-163 SC-004 invariants (`empty_version_purls == 0`, `phantom_edges == 0`) hold on mikebom's output. If NOT, that's a CRITICAL regression finding.
- [X] T013 [US1] Validate mikebom's SPDX 2.3 + SPDX 3 output for Kubernetes. **SPDX 3**: `.venv/spdx3-validate/bin/spdx3-validate specs/165-k8s-argocd-audit/artifacts/kubernetes/mikebom.spdx3.json` per memory `reference_spdx3_validator` + FR-006. **SPDX 2.3**: use the same vendored schema + validator that milestone-010's `mikebom-cli/tests/spdx_schema_validation.rs` uses — schema at `mikebom-cli/tests/fixtures/schemas/spdx-2.3.json`. Invoke via `python3 -c "import jsonschema, json; schema=json.load(open('mikebom-cli/tests/fixtures/schemas/spdx-2.3.json')); doc=json.load(open('specs/165-k8s-argocd-audit/artifacts/kubernetes/mikebom.spdx23.json')); jsonschema.validate(doc, schema); print('PASS')"` (or an analogous invocation). Record pass/fail per format per data-model.md E5.
- [X] T014 [US1] Write the Target 1 (Kubernetes) section of `docs/audits/2026-07-05-kubernetes-argocd.md` per data-model.md E7 + research §R8: (a) Snapshot subsection with URL + SHA + clone size + scan wall-clocks (T008 outputs); (b) Per-Tool Metrics table (T012 outputs); (c) mikebom Failure Modes subsection with named buckets + dispositions (T012 outputs — per research §R6 taxonomy); (d) Tool Comparison Delta subsection with mikebom_advantage / trivy_advantage / syft_advantage lists (capped to first 20 lex-sorted); (e) SPDX Validation subsection (T013 outputs).

**Checkpoint**: US1 fully functional. The Kubernetes half of the audit report is written and complete. Reader can extract every SC-002 through SC-005 metric FOR KUBERNETES from `docs/audits/2026-07-05-kubernetes-argocd.md`.

---

## Phase 4: User Story 2 — ArgoCD polyglot audit (Priority: P2)

**Goal**: Measure mikebom + Trivy + Syft against `github.com/argoproj/argo-cd`. Produce the per-target ArgoCD section of the audit report + a cross-ecosystem-interactions subsection surfacing Go ↔ npm edge patterns.

**Independent Test**: `docs/audits/2026-07-05-kubernetes-argocd.md` contains a complete Target 2 (ArgoCD) section analogous to US1's Target 1 section, PLUS a "Cross-Ecosystem Interactions" subsection documenting any component edges spanning `pkg:golang/` ↔ `pkg:npm/` boundaries.

### Tasks for User Story 2

- [X] T015 [US2] Clone `github.com/argoproj/argo-cd` at HEAD via `./specs/165-k8s-argocd-audit/scripts/run-audit.sh --target argocd --workdir <tempdir>`. Record `ARGO_SHA` + clone size + wall-clock.
- [X] T016 [US2] Run mikebom against argocd source tree — CDX + SPDX 2.3 + SPDX 3. Outputs at `specs/165-k8s-argocd-audit/artifacts/argocd/mikebom.{cdx,spdx23,spdx3}.json`. Same failure-mode disposition as T009.
- [X] T017 [US2] Run Trivy against argocd source tree — CDX only. Same shape as T010.
- [X] T018 [US2] Run Syft against argocd source tree — CDX only. Same shape as T011.
- [X] T019 [US2] Run `analyze.py` on ArgoCD SBOMs. Same invariant checks as T012. The cross-ecosystem edge detection (Go↔npm edges) is emitted by `analyze.py` per T006 (already implemented upfront — no mid-audit script extension needed).
- [X] T020 [US2] Validate mikebom's SPDX 2.3 + SPDX 3 output for ArgoCD. Same shape as T013 (SPDX 3 via `.venv/spdx3-validate/bin/spdx3-validate`; SPDX 2.3 via the milestone-010 vendored schema at `mikebom-cli/tests/fixtures/schemas/spdx-2.3.json` invoked from Python). Replace `kubernetes` with `argocd` in paths.
- [X] T021 [US2] Write the Target 2 (ArgoCD) section of `docs/audits/2026-07-05-kubernetes-argocd.md` per data-model.md E7 + research §R8. Same subsections as T014, plus one additional subsection **"Cross-Ecosystem Interactions"** documenting any Go ↔ npm edges found (or noting "none observed" if the count is 0). If cross-ecosystem interactions are non-zero, this becomes a top-3 candidate for the Recommended Follow-Ons section per US3.

**Checkpoint**: US2 fully functional. Both target sections of the audit report are written. Reader can extract every SC-002 through SC-005 metric for BOTH targets.

---

## Phase 5: User Story 3 — Prioritized follow-on recommendations (Priority: P3)

**Goal**: Synthesize US1 + US2 findings into a top-3 ranked follow-on milestone recommendation list + Executive Summary + Backlog Observations. This is the load-bearing "what next?" section.

**Independent Test**: `docs/audits/2026-07-05-kubernetes-argocd.md` contains: (a) Executive Summary (3-5 sentences with headline outcome), (b) Recommended Follow-On Milestones section with top-3 ranked candidates (each has problem statement + evidence + scope estimate per data-model.md E6), (c) Backlog Observations section listing smaller findings.

### Tasks for User Story 3

- [X] T022 [US3] Aggregate US1 + US2 findings: read both `analysis.json` files, enumerate all named RootCauseBuckets (E3) from both targets, and manually rank by impact (BFS-reachability delta, blast radius, effort). Write draft top-3 list in a scratch file (`specs/165-k8s-argocd-audit/artifacts/ranking-scratch.md`) before committing to the report.
- [X] T023 [US3] Write the "Recommended Follow-On Milestones" section of the report per data-model.md E6 + research §R9. If both targets show mikebom at parity with Trivy + Syft (SC-011 clean-pass outcome), use the research §R9 clean-pass template. Otherwise, top-3 with: problem statement + evidence (quantitative impact) + scope estimate (referencing analogous prior milestones) + blast radius + optional alternative-rationale.
- [X] T024 [US3] Write the Executive Summary + Backlog Observations sections of the report. Executive Summary = 3-5 sentences with the headline: what was measured, the biggest finding, the top recommendation. Backlog Observations = smaller findings not making the top-3 (may include the "12 residual orphans" pattern from milestone 164 for cross-audit comparison).

**Checkpoint**: US3 fully functional. The complete audit report is written from Executive Summary through top-3 Recommendations to Backlog Observations. SC-011 satisfied (either bug-class OR clean-pass outcome documented).

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Finalize the report + Reproduction Appendix, verify SC-007 + SC-008, commit + PR.

- [X] T025 Write the Reproduction Appendix section of `docs/audits/2026-07-05-kubernetes-argocd.md` per FR-008 + SC-009: exact commands used (from quickstart.md §3), pinned tool versions (T001-T004 outputs), commit SHAs (T008 + T015 outputs), expected wall-clock ranges. A fresh contributor MUST be able to reproduce every number by running the appendix commands. Also verify the report's header block includes: report date, mikebom baseline SHA (T004), Trivy/Syft/spdx3-validate versions, and host OS.
- [X] T026 Run `./scripts/pre-pr.sh` — MUST pass clean per SC-007 + SC-008. `cargo +stable clippy --workspace --all-targets -- -D warnings` clean + `cargo +stable test --workspace --no-fail-fast` passes with the same count as pre-165 (SC-008 golden byte-identity — every existing golden test unchanged since FR-010 mandates zero production code changes).
- [X] T027 Commit + open PR. Commit message: `docs(165): empirical audit of mikebom against Kubernetes + ArgoCD (implements milestone 165)`. Include the empirical findings summary in the commit body (top-3 recommendations at a glance). No upstream GitHub issue — this is an audit milestone follow-on to milestone 164's podman-desktop measurement. PR body includes the Executive Summary from the report.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No prerequisites. Verifies tools.
- **Phase 2 (Foundational)**: Depends on Phase 1. Creates dirs + analysis script.
- **Phase 3 (US1 Kubernetes)**: Depends on Phase 2. Load-bearing MVP path.
- **Phase 4 (US2 ArgoCD)**: Depends on Phase 2. Independent of Phase 3 — can run in parallel with US1 with a second contributor.
- **Phase 5 (US3 Recommendations)**: Depends on Phases 3 + 4 (needs both targets' analysis outputs).
- **Phase 6 (Polish)**: Depends on Phase 5.

### Within Each User Story

- **US1**: T008 (clone) → T009 + T010 + T011 (scans, parallel) → T012 (analyze) → T013 (SPDX validate) → T014 (write report section).
- **US2**: T015 (clone) → T016 + T017 + T018 (scans, parallel) → T019 (analyze) → T020 (SPDX validate) → T021 (write report section).
- **US3**: T022 (aggregate) → T023 (top-3) → T024 (Exec Summary + Backlog).

### Parallel Opportunities

Genuine `[P]` (different files, no dependencies):

- **Phase 3 T009 + T010 + T011** — 3 scans against Kubernetes could run in parallel with 3 separate shell invocations, but they compete for disk I/O + CPU. In practice, run them SEQUENTIALLY to get clean wall-clock timings.
- **Phase 4 T016 + T017 + T018** — same shape as Phase 3.
- **Phase 3 vs Phase 4** — Kubernetes and ArgoCD audits can run in parallel on separate contributors OR sequentially on one. Phase 3 comes first because US1 is P1.

**Foundational tasks T006-T007 (Python script + shell runner) are the load-bearing critical path** — get these right first; everything downstream depends on them.

---

## Parallel Example: Phase 3 vs Phase 4 (two-contributor)

```bash
# Contributor A: US1 Kubernetes path
./specs/165-k8s-argocd-audit/scripts/run-audit.sh --target kubernetes --workdir /tmp/audit-k8s
# ... produces artifacts/kubernetes/*.json → writes Target 1 section

# Contributor B: US2 ArgoCD path (in parallel)
./specs/165-k8s-argocd-audit/scripts/run-audit.sh --target argocd --workdir /tmp/audit-argo
# ... produces artifacts/argocd/*.json → writes Target 2 section

# Both converge for US3 (Contributor A or B — whoever finishes first)
# Aggregate findings + rank top-3 + write Executive Summary
```

---

## Implementation Strategy

### MVP scope

**US1 alone is the shippable MVP** — a Kubernetes-only audit report is still valuable and lets the maintainer make one prioritized recommendation. If time constrains, US2 + US3 can slip to a follow-on milestone (166), with US1's report published as `docs/audits/2026-07-05-kubernetes-only.md`.

Ship order:

1. Phase 1 (Setup) — 15 min. Toolchain verification.
2. Phase 2 (Foundational) — 1-2 hrs. `analyze.py` is the load-bearing artifact.
3. Phase 3 (US1) — 2-3 hrs. Kubernetes clone (~5 min) + 5 scans (~10 min) + analysis (~15 min) + T014 report section writing (~1.5-2 hrs).
4. **STOP + VALIDATE**: SC-011 clean-pass or bug-class outcome documented for Kubernetes.
5. Phase 4 (US2) — 2-3 hrs. ArgoCD run, same shape as US1.
6. Phase 5 (US3) — 1 hr. Synthesis + top-3 ranking + Executive Summary + Backlog.
7. Phase 6 (Polish) — 30 min. Repro appendix + pre-PR gate + commit.

### Total effort

~27 tasks. Estimated **6-10 focused hours end-to-end**. Report writing (T014 + T021) is the dominant time sink — each per-target section is a ~500-750 line Markdown block populated by hand from `analysis.json` outputs. Larger than milestone 164 (23 tasks, ~2 hrs) because of the two-target measurement + synthesis, but smaller than milestone 163 (40 tasks) in terms of code volume — the effort just shifts from code-writing to report-writing.

### Empirical revision escape hatch

Per spec.md SC-011, either "found bug class" OR "clean parity" is a valid outcome. If the audit surfaces a CRITICAL regression (mikebom crashes on Kubernetes; SC-004 milestone-163 invariants violated), that becomes the top-of-Recommendations finding and Phase 6 proceeds normally — the audit's job is to REPORT, not to fix.

### Parallel team strategy

Two-contributor optimal:

- **Contributor A**: Phase 1 → Phase 2 → Phase 3 US1 (Kubernetes) → Phase 5 US3 synthesis
- **Contributor B**: (after A finishes Phase 2) Phase 4 US2 (ArgoCD) → Phase 6 polish

Total wall-clock could compress to ~2-3 hrs with two contributors.

---

## Notes

- **Zero production code changes.** FR-010 + SC-007 + SC-008 enforce this. If a task tempts you to modify a mikebom source file, STOP — the finding belongs in the report as a follow-on milestone recommendation.
- **Report is the deliverable.** All FRs are about report contents; no wire-format changes, no annotation additions, no parity-catalog updates, no CLI flag additions.
- **Longitudinal record.** `docs/audits/2026-07-05-kubernetes-argocd.md` joins any future dated audit files as the persistent quality record. Cross-audit comparison is the future accountability mechanism.
- **Analysis script is reusable.** `analyze.py` should be written cleanly enough that milestone 200 (or whatever the next audit round is) can reuse it for future audits against different targets. Minimal target-specific logic; parametrize where possible.
- **License-emission spot-checks are OPTIONAL bonus per FR-006 + Edge Cases.** If time allows, include them; otherwise defer to a future audit round.
- **Delivers on Constitution Principle X (Transparency)** — the audit report itself is a transparency artifact making mikebom's quality state explicit and durable for future maintainers.
