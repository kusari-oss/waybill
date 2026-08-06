---

description: "Task list for feature 228-release-flow-exploration"
---

# Tasks: Survey peer-project release flows + recommendation for waybill's multi-track release strategy

**Input**: Design documents from `/specs/228-release-flow-exploration/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/doc-structure.md ✅, quickstart.md ✅

**Tests**: This is a docs-only research feature. No automated test tasks. Verification tasks in the Polish phase execute the quickstart.md predicates by hand.

**Organization**: Tasks are grouped by user story (US1 P1, US2 P2, US3 P3) to enable independent implementation. Every task touches Markdown files only — the waybill binary is unchanged.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies) — RARE in this feature since almost all writing lands in one new file (`docs/design/2026-08-05-release-flow-survey.md`). Cross-file [P] opportunities are called out where they exist.
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

Primary editing surface: `docs/design/2026-08-05-release-flow-survey.md` (single new file; directory `docs/design/` created as part of T001 if missing).
Optional secondary edit: `docs/index.md` (only if it hosts a "Design docs" section).

---

## Phase 1: Setup

**Purpose**: Baseline check + directory scaffolding. No file content authored yet.

- [X] T001 Verify `docs/design/` directory state at HEAD (`ls docs/`). If missing, note it will be created implicitly when T002 writes the survey file (git preserves empty dirs poorly; the file's presence creates the dir). Also grep-verify the 6 shortlisted peer projects still have publicly-reachable release pages at survey-authoring time (curl the 6 URLs from research §A — `github.com/rust-lang/rust/releases`, `github.com/aquasecurity/trivy/releases`, `github.com/anchore/syft/releases`, `github.com/sharkdp/bat/releases`, `github.com/argoproj/argo-cd/releases`, `github.com/nodejs/node/releases`). Any URL returning 404 → swap to a research §A alternate before proceeding.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Insert the doc skeleton so subsequent per-section writing tasks target stable anchors. All US-phase writing tasks depend on T002.

**⚠️ CRITICAL**: All user-story writing tasks below depend on T002.

- [X] T002 Create `docs/design/2026-08-05-release-flow-survey.md` with the full section skeleton per contract §doc-structure.md. Skeleton contents:
  - H1: `# waybill release-flow survey (2026-08-05)`
  - Status line: `**Status**: Draft — informs the follow-up implementation spec (229-release-flow-implementation).`
  - TOC (7 sections × 1 line each)
  - Empty section headings with `_TODO_` placeholders:
    - `## 1. waybill context` + `_TODO_`
    - `## 2. Peer-project survey` + `_TODO_`
    - `### 2.1 rust-lang/rust` + `_TODO_`
    - `### 2.2 aquasecurity/trivy` + `_TODO_`
    - `### 2.3 anchore/syft` + `_TODO_`
    - `### 2.4 sharkdp/bat` + `_TODO_`
    - `### 2.5 argoproj/argo-cd` + `_TODO_`
    - `### 2.6 nodejs/node` + `_TODO_`
    - `## 3. Tradeoff matrix` + `_TODO_`
    - `## 4. Recommendation` + `_TODO_`
    - `## 5. Considered and rejected` + `_TODO_`
    - `## 6. Future-distribution compatibility` + `_TODO_`
    - `## 7. Risks and open questions` + `_TODO_`

**Checkpoint**: doc skeleton committed; per-section writing tasks can proceed against stable anchors.

---

## Phase 3: User Story 1 - Maintainer explores peer-project release-track patterns (Priority: P1) 🎯 MVP

**Goal**: Maintainer reading the survey can name ≥4 distinct patterns and understand each pattern's tradeoffs for waybill.

**Independent Test**: Execute quickstart.md "Walkthrough — SC-001 peer-pattern recall" — target 4/4 correct pattern names + tradeoffs from a 5-minute skim.

### Implementation for User Story 1

- [X] T003 [US1] Write §1 waybill context in `docs/design/2026-08-05-release-flow-survey.md` under `## 1. waybill context`. Content per contract §doc-structure.md §1 requirements: current version (verified via `grep "^version" Cargo.toml` at writing time — expected `0.1.0-alpha.70`), current release model (single sequential alpha channel), CI shape (3-lane, < 5 min per memory `project_ci_timing`), known blockers (RELEASE_TAG_TOKEN broken; release-bump PRs 30+ min; goldens regen requires 3 env vars), compliance target (CISA 2026 per constitution Principle V), signing posture (Sigstore keyless via m222 `--sign`). Every claim cites its verifying source (memory ID / file:line / commit SHA). Line budget: ~50 lines.

- [X] T004 [US1] Write §2.1 rust-lang/rust peer-project entry under `### 2.1 rust-lang/rust`. Content per contract §doc-structure.md §2 requirements: 3-source citation block (`**Sources**:` line with release page URL + release-triggering workflow YAML URL + release-policy docs URL), project shape (age ~2015 stable release, thousands of contributors, systems language, downstream developer + toolchain profile), channel model (nightly / beta / stable with 6-week promotion cadence), cadence per channel (verbatim per Q3), tag/version convention per channel, signing/attestation posture, "why this fits their project" one-sentence note (canonical multi-track reference — waybill's mental model likely anchors here). Line budget: ~80 lines.

- [X] T005 [US1] Write §2.2 aquasecurity/trivy peer-project entry under `### 2.2 aquasecurity/trivy`. Same content requirements as T004 (3-source citation, project shape, channel model, cadence verbatim, tag convention, signing, why-fits). Trivy is the closest peer profile to waybill — SBOM/vuln tool with moderate contributor pool. Line budget: ~80 lines.

- [X] T006 [US1] Write §2.3 anchore/syft peer-project entry under `### 2.3 anchore/syft`. Same content requirements. Syft is a sibling SBOM tool with different release cadence than trivy — the comparison illuminates SBOM-ecosystem variance. Line budget: ~80 lines.

- [X] T007 [US1] Write §2.4 sharkdp/bat peer-project entry under `### 2.4 sharkdp/bat`. Same content requirements. Bat is a small-to-medium OSS Rust CLI with single-track model — shows what waybill's current state looks like scaled-up. Line budget: ~80 lines.

- [X] T008 [US1] Write §2.5 argoproj/argo-cd peer-project entry under `### 2.5 argoproj/argo-cd`. Same content requirements. Argo-CD's quarterly-minor + patch cadence is a K8s-ecosystem convention reference. Line budget: ~80 lines.

- [X] T009 [US1] Write §2.6 nodejs/node peer-project entry under `### 2.6 nodejs/node`. Same content requirements. Node.js is the LTS/current reference — the only surveyed project with a formal support-window contract. Include a note that Node is a runtime not a CLI but its LTS model is the informative axis. Line budget: ~80 lines.

- [X] T010 [US1] Write §3 Tradeoff matrix under `## 3. Tradeoff matrix` in `docs/design/2026-08-05-release-flow-survey.md`. Content per contract §doc-structure.md §3 requirements: 6-row × 6-column table (6 projects from T004–T009 × 6 axes from research §B: maintainer time cost, downstream trust signal, breaking-change management, artifact-availability latency, SBOM reproducibility, nightly-cadence verbatim). One paragraph of interpretive prose beneath the table naming which axes matter most for waybill (referencing §1). Footnote references for any per-project caveats. Every cell filled — use "N/A" or "unknown-source" explicitly for empty cells. Line budget: ~50 lines.

**Checkpoint**: US1 complete. SC-001 peer-pattern recall test executable against §1–§3.

---

## Phase 4: User Story 2 - Maintainer has a single decisive recommendation (Priority: P2)

**Goal**: One decisive recommendation with 5 required fields — specific enough that a follow-up implementation spec (229) can be written from §4 alone.

**Independent Test**: Execute quickstart.md "Walkthrough — SC-004 recommendation actionability" — target 5 executable tasks from §4 alone without ambiguity.

### Implementation for User Story 2

- [X] T011 [US2] Write §4 Recommendation under `## 4. Recommendation` in `docs/design/2026-08-05-release-flow-survey.md`. Content per contract §doc-structure.md §4 requirements (5 required fields): (1) **Channel manifest** — named channels + one-line audience each; (2) **Per-channel cadence** — verbatim; (3) **Per-channel tag/version convention** — SemVer pre-release syntax preferred per Q2 compatibility invariant (e.g., `v0.2.0-nightly.YYYYMMDD`, `v0.2.0-beta.N`, `v0.2.0`); (4) **Per-channel signing decision** — per FR-007a, whether each channel gets Sigstore keyless signature (m222 flow) with rationale; (5) **Migration path from `v0.1.0-alpha.70`** — either explicit next-tag OR "no migration; new model starts at v0.2.0". Every decision ties back to a §3 matrix axis-column it optimizes for. Q1 clarification requires SINGLE decisive recommendation (not menu). Line budget: ~80 lines.

- [X] T012 [US2] Write §5 Considered-and-rejected under `## 5. Considered and rejected` in `docs/design/2026-08-05-release-flow-survey.md`. Content per contract §doc-structure.md §5 requirements: ≥ 2 alternatives from the surveyed set (T004–T009). Per alternative: one-line "why this looked good" + one-line "why not for waybill" tied to a specific §1 waybill constraint (contributor pool, CI cadence, compliance target, or memory-documented blocker). MUST NOT include alternatives that weren't in the §2 survey. Line budget: ~40 lines.

- [X] T013 [US2] Write §6 Future-distribution compatibility under `## 6. Future-distribution compatibility` in `docs/design/2026-08-05-release-flow-survey.md`. Content per contract §doc-structure.md §6 requirements: table with columns `Surface | Common convention | Recommendation-compatibility note`, rows for at minimum crates.io, homebrew, cargo-binstall, apt/rpm/dnf. If any surface has a known convention that conflicts with the recommended tag/version syntax (e.g., homebrew's issues with SemVer pre-release dashes — see research §G risk #4), an explicit "how the recommendation stays compatible" note. MUST NOT introduce plans for expanding to those surfaces (FR-011 keeps them out of scope). Line budget: ~30 lines.

- [X] T014 [US2] Write §7 Risks and open questions under `## 7. Risks and open questions` in `docs/design/2026-08-05-release-flow-survey.md`. Content per contract §doc-structure.md §7 requirements: ≥ 3 items (research §G surfaced 4 already — 229-spec-timing, CISA 2026 per-channel IdP, per-commit-vs-reproducibility tension, homebrew SemVer-pre-release compatibility). Per item: one-line risk statement + deferred-to designation (usually "229-release-flow-implementation"). MUST NOT resolve any risks in-line. Line budget: ~30 lines.

**Checkpoint**: US2 complete. SC-004 recommendation-actionability test executable against §4.

---

## Phase 5: User Story 3 - Downstream consumer can predict release-channel semantics (Priority: P3)

**Goal**: A first-time waybill adopter can decide which channel their SBOM-generation pipeline should track from the recommendation's consumer-facing content alone.

**Independent Test**: Execute quickstart.md "Walkthrough — SC-008 persona channel-choice test" — persona picks a defensible channel from §4 alone.

### Implementation for User Story 3

- [X] T015 [US3] Audit §4's per-channel audience lines in `docs/design/2026-08-05-release-flow-survey.md` for consumer-actionability. For each channel named in the manifest, verify the audience line answers all three of: (a) which risk tolerance this channel fits, (b) what stability guarantees the channel provides, (c) how a consumer detects channel-promotion events for pipeline-update planning. This is a review-and-revise task, not a new-writing task — if a channel's audience line is missing any of the three, edit §4 in-place until all three are covered. Content is served by T011's §4 write; T015 ensures US3's specific consumer-actionability requirement is met. Line budget: 0 new lines (edits are within §4's existing budget).

**Checkpoint**: US3 complete. SC-008 persona test executable.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Verification walkthroughs from quickstart.md + cross-linking + follow-up issue seeding + commit + PR.

- [X] T016 Execute quickstart SC-001 (peer-pattern recall) AND SC-008 (persona channel-choice) walkthroughs on the finished doc. **PASS**: SC-001 6/6 patterns nameable; SC-008 persona picks stable defensibly. SC-001: skim §1–§3 for 5 min, then name ≥4 distinct release-track patterns + tradeoffs from memory. Target 4/4. SC-008: hand §4 to a persona (security-team operator running waybill in CI for production SBOM generation) and verify they can pick a defensible channel + justify. If any test misses, revise the corresponding section until it passes.

- [X] T017 Execute quickstart SC-002 (project + category count), SC-003 (matrix dimensions), SC-007 (line budget) checks. **PASS**: SC-002 = 6 projects, 4 categories tagged (a/b/c/d/e — 4 unique); SC-003 = 6×6 matrix, no blank cells; SC-007 = 342 lines, well under 800 ceiling. SC-002: `grep -c "^### 2\." docs/design/2026-08-05-release-flow-survey.md` returns ≥ 5 (expected 6). SC-003: verify §3 table is 6-row × 6-column with no blank cells. SC-007: `wc -l docs/design/2026-08-05-release-flow-survey.md` returns ≤ 800. If over budget, trim per research §F compression order (compress per-project sections in §2 first; then compress §3's interpretive prose).

- [X] T018 Execute quickstart SC-004 (recommendation actionability) walkthrough. **PASS**: 5/5 executable tasks (T-A nightly.yml cron + skip-if-unchanged; T-B WAYBILL_VERSION override; T-C bump to v0.2.0 stable + retire alpha.N; T-D --sign integration in release.yml; T-E delete auto-tag-release.yml) written from §4 alone with no ambiguity. Open §4 alone (don't read the rest of the doc), attempt to write 5 executable tasks for a follow-up implementation spec from §4 content only. Target: 5/5 tasks written with no "TBD" or "unclear from doc". Any miss → §4 is under-specified; revise until an engineer can pseudocode-implement from it.

- [X] T019 Execute quickstart SC-005 verification — grep results: (a) CISA 2026 signing = 5 mentions; (b) cache invalidation = 3; (c) RELEASE_TAG_TOKEN = 2; (d) reproducibility = 1. All 4 concerns present in §4. **PASS**. — grep §4 in `docs/design/2026-08-05-release-flow-survey.md` for explicit mentions of all 4 FR-007 waybill-specific concerns: (a) CISA 2026 signing per channel, (b) SBOM golden-fixture cache invalidation on version bumps, (c) `RELEASE_TAG_TOKEN` auto-tag brokenness, (d) reproducibility (byte-identical artifacts across builds). Target 4/4 mentioned. If any missing, add a subsection or bullet to §4 addressing the gap.

- [X] T020 Execute quickstart SC-006 verification — 5 random claims across §2.1/§2.2/§2.3/§2.5/§2.6; every source URL returns 200 OK. Every claim maps back to a fetched-during-research source. **PASS**. — pick 5 randomly-chosen claims from §2 (peer-project entries). For each, click the corresponding source URL from the entry's `**Sources**:` line and verify the claim against the source page/YAML/docs at the time of check. Target 5/5 match. Any miss → the doc has fabricated content per memory `feedback-verify-research-empirical-claims`; correct the claim to match current source-state or drop it before merge.

- [X] T021 [P] Add cross-link to the new survey from `docs/index.md` if it hosts a "Design docs" section — verify during T021 execution; if `docs/index.md` doesn't have a Design-docs section (or doesn't exist), T021 is a no-op and marked complete without edit. Runs in parallel with any Phase 6 verification task since it touches a different file.

- [X] T022 Run `./scripts/pre-pr.sh` — must exit 0 before PR open per project convention. Docs-only change so no compile invalidation is triggered; expected fast completion given warm cache.

- [X] T023 [P] File follow-up GitHub issues for the 4 seeds from research §G — filed as #665 (229 impl spec), #666 (Sigstore OIDC per-workflow), #667 (reproducibility contract docs), #668 (Homebrew SemVer pre-release compatibility):
  - (a) Spec + implement `229-release-flow-implementation` immediately after this merges (survey drift risk if delayed).
  - (b) Per-channel Sigstore Fulcio identity provider decisions may need different trust roots for nightly vs stable.
  - (c) Reproducibility semantics per channel — nightly cadence "per-commit" implies non-reproducible builds for docs-only changes.
  - (d) Homebrew SemVer-pre-release compatibility if formula publishing is added later.

  Each issue references this docs milestone as the surfacer. Runs in parallel with the writing/verification tasks — no file dependency on the doc's content.

- [ ] T024 Commit + push branch `228-release-flow-exploration`, open PR against `main`. PR body links to constitution v2.1.0 (Principle V CISA 2026) + m221/m222 CISA 2026 signing PRs as the compliance context motivating multi-channel signing decisions. After push, visually verify markdown renders correctly on GitHub (tables lay out cleanly, code blocks highlight, anchor links resolve, no smart-quote artifacts survived paste). Runs after all Phase 6 verification tasks pass.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no dependencies.
- **Phase 2 (Foundational)**: T002 depends on T001 (need baseline check of `docs/design/` state + verified peer-project URLs).
- **Phase 3 (US1)**: T003–T010 all depend on T002 (need doc skeleton in place). Sequential within phase (same-file writes).
- **Phase 4 (US2)**: T011–T014 depend on T002. Also logically depend on Phase 3 completion — the recommendation (§4) ties back to §3 matrix; without §2/§3 written, §4 has nothing to cite. Sequential within phase.
- **Phase 5 (US3)**: T015 depends on T011 (audits §4 which T011 writes).
- **Phase 6 (Polish)**: T016–T020 depend on all prior writing (T003–T015). T021 (cross-link) depends on nothing beyond T002. T022 (pre-PR) depends on all writing being committed. T023 (follow-up issues) has no doc dependency. T024 (commit+PR) depends on all others.

### User Story Dependencies

- **US1 (P1)** — independent of US2 and US3 in content; MVP scope. Writes §1–§3.
- **US2 (P2)** — logically depends on US1 (§4 cites §3 matrix). Shares the file `docs/design/2026-08-05-release-flow-survey.md` so requires sequential commits.
- **US3 (P3)** — depends on US2's §4 (T015 audits it). Doesn't add new content; only revises.

### Parallel Opportunities

- **T021** (index.md cross-link) touches a DIFFERENT file (`docs/index.md`) so it's genuinely parallelizable with any Phase 3–5 task.
- **T023** (file 4 follow-up issues) has no doc dependency; can be started as soon as research §G is confirmed.
- Within a phase, subsection writes are logically independent (different anchors) but share one file — best executed sequentially by one contributor to avoid rebase churn.

---

## Parallel Example: Phase 3

```bash
# T003–T010 all edit docs/design/2026-08-05-release-flow-survey.md (same file) — sequential in practice:
Task: T003 — Write §1 waybill context
Task: T004 — Write §2.1 rust-lang/rust entry
Task: T005 — Write §2.2 aquasecurity/trivy entry
Task: T006 — Write §2.3 anchore/syft entry
Task: T007 — Write §2.4 sharkdp/bat entry
Task: T008 — Write §2.5 argoproj/argo-cd entry
Task: T009 — Write §2.6 nodejs/node entry
Task: T010 — Write §3 Tradeoff matrix

# T021 (different file) can run in parallel with any of the above:
Task: T021 — Add cross-link in docs/index.md (if applicable)
```

---

## Implementation Strategy

### MVP First (US1 only — the P1 story)

1. T001 baseline check.
2. T002 skeleton insertion.
3. T003 §1 waybill context.
4. T004–T009 §2.1–§2.6 peer-project entries.
5. T010 §3 tradeoff matrix.
6. **STOP and VALIDATE**: Execute quickstart SC-001 walkthrough (T016 restricted to §1–§3). If 4/4 patterns recallable, US1 is MVP-complete.
7. Ship the PR with US1 only if bandwidth demands; US2/US3 can land in follow-up commits.

### Incremental Delivery

1. Setup + Foundational → doc skeleton in place.
2. US1 → maintainer can survey peer patterns → Deploy/demo.
3. US2 → maintainer has single decisive recommendation → Deploy/demo.
4. US3 → consumer can pick a channel → Deploy/demo.
5. Polish → verification, cross-refs, follow-ups. Ship the completed PR.

### Sequential Team Strategy (single-writer default)

For a research + docs feature written by one contributor, sequential execution matches the doc's natural reading order:

1. T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017 → T018 → T019 → T020 → T021 → T022 → T023 → T024.
2. Commit after each user-story phase completes for reviewable increments.

---

## Notes

- Every writing task has a line-budget target. Total across §1–§7 is ~770 lines (per research §F, front matter ~30 + §1 ~50 + §2 ~480 + §3 ~50 + §4 ~80 + §5 ~40 + §6 ~30 + §7 ~30 + closing ~10 = 800 at ceiling; disciplined writing lands ~680 lines with headroom).
- Every peer-project claim in §2 MUST cite a source URL (release page + workflow YAML + docs); verifiable per SC-006. No memory-based claims per spec FR-006 + memory `feedback-verify-research-empirical-claims`.
- Q1 clarification: single decisive recommendation, NOT menu of options. §5 hosts the rejected alternatives.
- Q2 clarification: distribution scope bounded to gh-release + OCI; §6 verifies future-compatibility invariant.
- Q3 clarification: nightly cadences recorded verbatim in §3's matrix; recommendation in §4 picks one and justifies.
- Follow-up issues (T023) filed as separate work — this milestone stays scoped to the survey + recommendation.
