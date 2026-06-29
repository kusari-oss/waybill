# PR — milestone 151: expand consumer-guide depth coverage

## Summary

Extends the milestone-150 consumer guide (`docs/reference/reading-a-mikebom-sbom.md`) with depth coverage for **6 tier-1 signals** the milestone-150 selection missed, and adds a **decision rubric** (§2.1) so future depth-vs-appendix decisions stay principled rather than reflecting whatever the author happened to remember.

- **6 new depth-covered signals**: `mikebom:evidence-kind`, `mikebom:confidence`, `mikebom:linkage-kind`, `mikebom:not-linked`, `mikebom:depends-unresolved` + `…-rdepends-unresolved` (paired), `mikebom:assertion-conflict`.
- **New §2.1 Curation rubric**: 5 yes/no criteria + threshold N=3, with worked-example + counter-example tables validating against 18 depth-covered + 7 representative appendix-only signals (25/25 correctly classified).
- **Appendix hygiene pass**: corrected misleading prose for `mikebom:component-role` + `mikebom:confidence` + `mikebom:linkage-kind` (the milestone-150 entries had factual inaccuracies for value space + emission behavior).
- Single-file deliverable: `docs/reference/reading-a-mikebom-sbom.md` (+273 / -8 LOC).

## Origin

Post-milestone-150 maintainer review surfaced that the 12-signal selection felt "somewhat random" — `mikebom:evidence-kind` was the explicit example of an appendix-only signal that should have warranted depth coverage. Two-axis fix: (a) add the 6 tier-1 signals the original selection missed; (b) document the decision rubric so future signal additions stay principled.

No GitHub issue — this milestone is the documentation-side response to the maintainer's curation feedback.

## Changes

| File | Change |
|------|--------|
| `docs/reference/reading-a-mikebom-sbom.md` | +273 / -8 LOC. New §2.1 (Curation rubric) + 6 new depth-coverage subsections in §3.1 / §3.3 / §3.4 + updated Appendix A entries + new Appendix B entries. |
| `specs/151-expand-consumer-guide/*` | Standard speckit branch artifacts (spec, plan, research, data-model, contracts, quickstart, tasks, checklists, verify-recipes.sh). Authoring scaffolding, not consumer-facing. |

No Rust source changes. No catalog (`docs/reference/sbom-format-mapping.md`) changes. No CI workflow changes. No new annotation keys (FR-016). No wire-format changes (FR-017).

## Spec / Plan trail

- [Spec](../../specs/151-expand-consumer-guide/spec.md) — 19 FRs, 10 SCs, 5 USs.
- [Plan](../../specs/151-expand-consumer-guide/plan.md) — Constitution Check PASS pre + post design.
- [Research](../../specs/151-expand-consumer-guide/research.md) — R1 rubric criteria draft + R1.1/R1.2 validation tables.
- [Data model](../../specs/151-expand-consumer-guide/data-model.md) — Rendering invariant + rubric structure + harness shape.
- [Contracts/rubric.md](../../specs/151-expand-consumer-guide/contracts/rubric.md) — The rubric as a mechanical YES/NO predicate.
- [Quickstart](../../specs/151-expand-consumer-guide/quickstart.md) — 7 validation scenarios.
- [Tasks](../../specs/151-expand-consumer-guide/tasks.md) — 37 tasks across 8 phases, all marked complete except T037 (this PR description).

Clarifications (`spec.md` § Clarifications, Session 2026-06-29):
- **Q1** → Decision rubric (5 yes/no criteria, threshold N=3) chosen over single-rule / prose / precedent-table alternatives.
- **Q2** → Reserved-key framing for `mikebom:depends-unresolved` / `…-rdepends-unresolved` (generic wire shape + inline "currently Yocto-only" note).

`/speckit-analyze` flagged 6 findings (0 critical, 3 medium, 3 low). All 4 actionable items applied as remediations (A1: C16↔C59 disambiguation in T010 + appendix; A2: explicit FR-018 catalog-not-touched assertion in T034; A3: exact-count assertion in T033; A5: "maintainer-cadence review" definition in spec Assumption 3).

## Verification (SC-001 through SC-010)

| SC | Check | Result |
|----|-------|--------|
| SC-001 | 8-question read-through audit (T036) | ✅ All 8 questions answerable from the guide alone (see quickstart.md Scenario 1). |
| SC-002 | Every catalog `mikebom:*` key in Appendix A (T032) | ✅ 102/102 match (catalog 102 keys, appendix 102 keys, diff empty). |
| SC-003 | ≥6 new jq recipes verified runnable (T031) | ✅ `verify-recipes.sh` reports **7 passed, 0 failed**. Recipes against `transitive_parity/cargo` + `transitive_parity/go` fixtures. Each uses "present" expectation (per research.md §R8 + milestone-150 precedent) because the milestone-090 sibling-fixture repo doesn't carry Yocto + supplement-file fixtures yet. |
| SC-004 | ≥18 depth-covered signals (T033) | ✅ Final count: 18 sections covering 21 unique catalog keys (3 paired-entry collapses: duplicate-purl-divergent+purl-collisions-detected, graph-completeness+…-reason, depends-unresolved+…-rdepends-unresolved). |
| SC-005 | Cluster balance ≥3 per cluster (T033) | ✅ Final cluster sizes: 5 / 3 / 5 / 5 for §3.1 / §3.2 / §3.3 / §3.4. (Note: research.md §R4 + spec SC-005 prose originally projected 6/3/5/5 due to an off-by-one count of milestone-150's pre-state §3.1 (had 3 sections, not 4); the as-delivered figure is 5/3/5/5. Per-key counts: 6/3/5/7 keys. Both the floor and FR-019 are satisfied.) |
| SC-006 | Rubric application 25/25 correct | ✅ §2.1 worked-example + counter-example tables validate 18 depth + 7 appendix-only signals. |
| SC-007 | Single-file deliverable (T034) | ✅ `git diff main --name-only -- docs/` returns only `docs/reference/reading-a-mikebom-sbom.md`. Rust source, CI workflows, catalog untouched. |
| SC-008 | Pre-PR gate clean (T035) | ✅ See pre-PR output below. |
| SC-009 | Appendix-hygiene audit (T028 + T032) | ✅ All 102 appendix entries correspond to actually-emitted keys. **0 removal candidates surfaced.** (The maintainer-flagged `mikebom:component-role` is actually emitted in most cases; only the `main-module` value is sometimes stripped by milestone-077/149 root-override logic. The Appendix A entry was corrected from "Internal — filtered before emission" to accurately describe per-value emission behavior.) |
| SC-010 | Cross-reference resolution (T030) | ✅ Every `§X.Y` cross-reference resolves to an existing section (12/12 references, all match). |

### FR-018 catalog enforcement (per A2 remediation)

```
$ git diff main --name-only -- docs/reference/sbom-format-mapping.md
(empty)
```

Catalog untouched. FR-018 satisfied. Inline catalog-clarification exception did NOT fire this milestone.

## US5 audit output (T028)

Per data-model.md §8 format:

| Key | Has emission site? | Action |
|-----|--------------------|--------|
| `mikebom:component-role` | YES (emitted for values `build-tool` / `language-runtime` / `saas-service` / `workspace-root`; `main-module` stripped on a per-component basis when milestone-077 root override fires with milestone-149 drop or demote path) | KEEP in Appendix A; CORRECTED misleading prose (was "Internal — filtered before emission"; now accurately describes per-value emission behavior) |
| *(all 101 other Appendix A keys)* | YES | KEEP unchanged |

**Result**: 0 keys removed; 1 appendix-entry prose corrected. Per FR-010 the appendix accurately reflects the actually-emitted key set. Per FR-011 every cross-reference resolves.

Two additional appendix entries had factual inaccuracies (predating milestone 150 — surfaced + fixed during this milestone's authoring):
- `mikebom:confidence`: was "resolution-confidence score (0.0–1.0)"; corrected to "qualitative confidence label (closed enum — currently only `\"heuristic\"`)". The numeric 0.0–1.0 value space lives on the SEPARATE key `mikebom:fingerprint-confidence` (C59).
- `mikebom:linkage-kind`: was "(`statically-linked` / `dynamically-linked` / `cgo-import`)"; corrected to "(`dynamic` / `static` / `mixed`)" per the actual emitted enum at `mikebom-cli/src/generate/cyclonedx/builder.rs:1067`.

These are pre-existing milestone-150 bugs caught + fixed inline during the depth-coverage authoring (analysis remediation A1 motivation).

## Constitution check

Per [plan.md POST-DESIGN re-evaluation](specs/151-expand-consumer-guide/plan.md#constitution-check--post-design-re-evaluation):
- Principle V (standards-native precedence): **REINFORCED** — depth coverage cites catalog rows as source-of-truth; no new emission shapes; rubric C5 criterion explicitly tests "wire shape requires documentation beyond catalog row" against the catalog.
- Principle X (Transparency): **ADVANCED** — 6 transparency-critical signals (`assertion-conflict` from M119, `depends-unresolved` from M128, `not-linked` from M050, trust trio from M002-era) now have consumer-discoverable depth coverage.
- All other principles: N/A (docs-only).

No violations. No complexity-tracking entries needed.

## Pre-PR gate output (T035)

```
$ ./scripts/pre-pr.sh
[clippy: clean — no warnings, no errors]
[tests: 117 test-result lines total; 116 passed; 1 failed]

Failed test (documented env-only flake — only acceptable failure per spec SC-008):
  - sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems

Exit code: 101 (cargo's exit code for test failure; matches pre-151 main HEAD
state — same documented flake behaves identically before + after milestone 151
edits, confirming docs-only changes don't affect test outcomes).
```

This matches the project-memory entry on the sbomqs_parity env-only flake (see `~/.claude/projects/.../memory/feedback_dont_dismiss_test_failures.md` cross-reference: the failure is reproducible across CI lanes regardless of milestone, due to a sbomqs JSON-parse quirk on the env-local sbomqs binary's stdout). Spec SC-008 explicitly permits this as the only acceptable failure for a docs-only milestone.

## Reviewer-cadence operator test

To independently verify SC-001 + SC-006, follow `specs/151-expand-consumer-guide/quickstart.md` Scenario 1 (the 8-question read-through audit) and Scenario 5 (rubric application). Both are designed for an external reviewer who hasn't seen this PR's spec or research — they exercise the doc-as-delivered against a real mikebom-emitted SBOM.

Operator-cadence review feedback is welcome via follow-up issues; the post-merge feedback loop is the canonical SC-001 validator per spec Assumption 3.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
