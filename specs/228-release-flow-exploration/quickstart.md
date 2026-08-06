# Quickstart: verifying the survey works

**Feature**: 228-release-flow-exploration
**Phase**: 1

Executes the SC-001 + SC-002 + SC-004 + SC-006 + SC-007 + SC-008 predicates against the finished deliverable so the writing phase has a concrete acceptance test.

## Setup

Have the finished `docs/design/2026-08-05-release-flow-survey.md` open in a Markdown previewer or terminal viewer. No SBOM fixtures needed (this deliverable doesn't emit consumer SBOMs).

## Walkthrough — SC-001 peer-pattern recall (US1)

1. Open the finished doc as a first-time reader.
2. Skim for 5 minutes.
3. Close the doc.
4. From memory, name at least 4 distinct release-track patterns and one tradeoff each.
5. Compare against §2 of the doc — target 4/4 correct pattern names + tradeoffs.

Expected pattern examples the doc should surface:
- Rust-style nightly/beta/stable with automatic promotion cadence
- Trivy-style single-track frequent releases + patch cadence
- Bat/Ripgrep-style single-track manual release
- Node.js-style LTS/current with even/odd major-version discrimination
- Argo-CD-style quarterly-minor + patch cadence

## Walkthrough — SC-002 project + category coverage

```bash
grep -c "^### 2\." docs/design/2026-08-05-release-flow-survey.md
```

Target ≥ 5 (per FR-002 minimum; expected 6 per Phase 0 §A). Then eyeball §2's entries to confirm they collectively cover ≥3 of the 5 FR-002 categories with each project's category-membership called out in its entry.

## Walkthrough — SC-004 recommendation actionability (US2)

1. Open §4 Recommendation in the finished doc.
2. Without reading §1, §2, §3, or the rest of the doc, write down 5 executable tasks for a follow-up implementation spec (e.g., "T1: bump workspace version to `v0.2.0-nightly.20260807`"; "T2: add `.github/workflows/nightly.yml` with cron `0 6 * * *`"; etc.).
3. If you can produce 5 executable tasks with no ambiguity, SC-004 passes.
4. If any task ends with "TBD" or "unclear from doc", the recommendation is under-specified — revise §4.

## Walkthrough — SC-006 source-citation spot check

1. Pick 5 randomly-chosen claims from §2 (e.g., "trivy nightly = per-commit skipped on docs-only merges").
2. For each claim, click the corresponding source URL from the entry's `**Sources**:` line.
3. Verify the claim against the source page/YAML/docs.
4. Target 5/5 verified. Any miss → the doc has fabricated content (memory `feedback-verify-research-empirical-claims`) and must be corrected before merge.

## Walkthrough — SC-007 line budget

```bash
wc -l docs/design/2026-08-05-release-flow-survey.md
```

Target ≤ 800. If over, trim per Phase 0 §F's compression order: compress per-project sections in §2 first; then compress the tradeoff-matrix interpretive prose.

## Walkthrough — SC-008 consumer channel-choice test (US3)

Persona: security-team operator running waybill in a CI pipeline for production SBOM generation.

1. Show the persona only §4 Recommendation (specifically the per-channel audience statements).
2. Ask them to pick a channel their CI pipeline should track.
3. Ask them to justify the choice.
4. Target: a defensible channel choice that matches the recommendation's intended segmentation for security-team-operator audiences (e.g., "stable — my pipeline runs quarterly SBOM audits; nightly's daily churn is more risk than my compliance flow can absorb").

## Post-writing checklist

- [ ] SC-001 recall test: 4/4 correct.
- [ ] SC-002 count: 5 projects, 3 categories, project category-membership called out per entry.
- [ ] SC-003 matrix: 6×6 (or more) grid; no blank cells.
- [ ] SC-004 pseudocode test: 5 executable tasks without ambiguity.
- [ ] SC-005: §4 explicitly addresses all 4 FR-007 concerns (CISA 2026 signing per channel, cache invalidation, RELEASE_TAG_TOKEN, reproducibility).
- [ ] SC-006 spot check: 5/5 claims verified against sources.
- [ ] SC-007 line count: ≤ 800.
- [ ] SC-008 persona test: persona picks a defensible channel from §4 alone.
- [ ] Pre-PR gate: `./scripts/pre-pr.sh` exit 0.
- [ ] Visual verification of markdown rendering on GitHub after push.
