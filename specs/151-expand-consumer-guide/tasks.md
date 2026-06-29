---
description: "Task list for milestone 151 — expand consumer-guide depth coverage"
---

# Tasks: Expand consumer-guide depth coverage — milestone 151

**Input**: Design documents from `/specs/151-expand-consumer-guide/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/rubric.md ✓, quickstart.md ✓

**Tests**: NOT requested. Docs-only milestone — the `verify-recipes.sh` authoring harness is the only "test"-like artifact (per spec Assumption 4, it's not a CI-gated test). No `cargo test` additions, no integration test additions.

**Organization**: Tasks are grouped by user story to enable independent authoring + review of each story. All edits land in the SAME file (`docs/reference/reading-a-mikebom-sbom.md`) per SC-007, so `[P]` markers indicate "no semantic dependency between tasks" rather than physical-parallel file edits — actual authoring order is serial, but reviewers can verify each US independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: No semantic dependency on other tasks in the same phase
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4, US5)
- File paths are exact; the deliverable file is `docs/reference/reading-a-mikebom-sbom.md` unless noted

## Path Conventions

- **Doc deliverable**: `docs/reference/reading-a-mikebom-sbom.md` (the SINGLE file touched in the shipped diff per FR-016 / FR-017 / SC-007)
- **Authoring artifacts** (not shipped publicly): `specs/151-expand-consumer-guide/verify-recipes.sh`, `specs/151-expand-consumer-guide/*.md`
- **Catalog reference**: `docs/reference/sbom-format-mapping.md` (READ-ONLY this milestone per FR-018)

---

## Phase 1: Setup

**Purpose**: Verify the baseline environment so any test break later is traceable to this milestone's edits.

- [X] T001 Verify pre-PR baseline on `main` is green by running `./scripts/pre-pr.sh` from the repo root before any edits; record the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only failure as the ONLY pre-existing failure permitted in SC-008.
- [X] T002 [P] Verify the milestone-090 sibling fixture cache is populated by running `ls $MIKEBOM_FIXTURES_DIR/transitive_parity/{cargo,npm,go}/` (must list real fixture trees, not empty); these fixtures are required by Phase 2's `verify-recipes.sh` setup and by every US's recipe verification.
- [X] T003 [P] Read `docs/reference/reading-a-mikebom-sbom.md` end-to-end (preparation; no edits) to internalize the per-signal rendering invariant, cluster organization, and Appendix A/B shape that this milestone extends. Cross-reference research.md §R2 (per-signal placement audit) and data-model.md §1 (rendering invariant) while reading.

---

## Phase 2: Foundational

**Purpose**: Stand up the authoring scaffolding that every US phase will exercise.

**⚠️ CRITICAL**: T004 must complete before US1/US2/US3 authoring begins, since those phases close each new depth section with a `verify-recipes.sh` entry per FR-012.

- [X] T004 Create `specs/151-expand-consumer-guide/verify-recipes.sh` by copying `specs/150-sbom-consumer-guide/verify-recipes.sh` verbatim, then emptying the recipe-list body (keep the header, the `run_recipe()` helper, the build step, the fixtures-dir lookup, the cleanup trap, and the summary tail). Each US's authoring phase will append its recipe entries.
- [X] T005 Add a 1-sentence skeleton placeholder for §2.1 "Curation rubric" immediately after the existing §2 ("How to read this doc") closing paragraph in `docs/reference/reading-a-mikebom-sbom.md`. The placeholder is a section heading + a single line `(rubric content authored in US4 phase below)`; this anchor lets US1/US2/US3 cross-references to "see §2.1" resolve. US4's T024 replaces the placeholder with the full rubric.
- [X] T006 [P] Identify exact insertion point for §3.3 trust-trio intro paragraph (between the §3.3 heading and the existing `#### mikebom:source-type` subsection) by reading the current §3.3 structure in `docs/reference/reading-a-mikebom-sbom.md`; record the line range for use in US1 T009.

**Checkpoint**: Phase 2 complete — US1 / US2 / US3 / US4 can be authored in any order; US5 waits for all four.

---

## Phase 3: User Story 1 — Trust trio (Priority: P1) 🎯 MVP

**Goal**: A vulnerability-scanner author can read §3.3 (build provenance) and compose `mikebom:source-type` + `mikebom:evidence-kind` + `mikebom:confidence` into threshold-based filter recipes without leaving the guide.

**Independent Test**: A reviewer opens the doc, navigates to §3.3, finds 3 trio members each with full per-signal rendering invariant + a trio-composing intro paragraph + cross-references between members; runs the trio-composing jq recipe from R3.1 against a real CDX SBOM emitted by `cargo +stable run -q -p mikebom -- sbom scan --offline --path $MIKEBOM_FIXTURES_DIR/transitive_parity/cargo --format cyclonedx-json --output /tmp/cdx.json`; output matches the documented shape.

### Implementation for User Story 1

- [X] T007 [US1] Add the §3.3 "Trust trio composition" intro paragraph at the line range identified in T006, using the exact prose from data-model.md §3 (one paragraph; ends with the trio members listed). Add the paragraph to `docs/reference/reading-a-mikebom-sbom.md`.
- [X] T008 [US1] Add the `**Composes with**: …` cross-reference line to the EXISTING `#### mikebom:source-type` subsection in `docs/reference/reading-a-mikebom-sbom.md`, immediately after its "What to do with it" element, pointing at `mikebom:evidence-kind` and `mikebom:confidence` (anchors `#mikebom-evidence-kind` and `#mikebom-confidence` — will resolve after T009 + T010 land).
- [X] T009 [US1] Add the `#### mikebom:evidence-kind` depth-coverage subsection inside §3.3 (after the existing `mikebom:source-type` subsection) in `docs/reference/reading-a-mikebom-sbom.md`, conforming to the per-signal rendering invariant (data-model.md §1): What it is / Where it lives (per-format, from research.md §R2) / Value space (closed enum `direct-observation` / `inference` / `enrichment`) / What to do with it / Milestone: 002-era / Catalog link: [C4](sbom-format-mapping.md) / jq recipe + Expected output (research.md §R3.1) / `Composes with`: cross-references to source-type and confidence.
- [X] T010 [US1] Add the `#### mikebom:confidence` depth-coverage subsection inside §3.3 (after `mikebom:evidence-kind`) in `docs/reference/reading-a-mikebom-sbom.md`, conforming to the per-signal rendering invariant: What it is / Where it lives (per-format) / Value space (**closed enum — currently only the value `"heuristic"`**; for numeric quantitative confidence on fingerprint-matched components, see the SEPARATE `mikebom:fingerprint-confidence` annotation at catalog C59, which is appendix-only and was introduced by milestone 110 — DO NOT conflate the two keys, they have different value spaces and different emission gating) / What to do with it / Milestone: 002-era (foundational) / Catalog link: [C16](sbom-format-mapping.md) / jq recipe + Expected output / `Composes with`: cross-references to source-type and evidence-kind. (Per analysis remediation A1: keep the depth section focused on C16 only; cross-reference C59 in Appendix A but do NOT promote it to depth coverage in this milestone.)
- [X] T011 [P] [US1] Update the Appendix A entries for `mikebom:evidence-kind` and `mikebom:confidence` in `docs/reference/reading-a-mikebom-sbom.md` per FR-014: append `(see §3.3 for depth coverage)` to each one-line description, preserving the catalog C-row link as fallback (data-model.md §6).
- [X] T012 [P] [US1] Add entries for `mikebom:evidence-kind` (milestone 002-era) and `mikebom:confidence` (milestone 002-era) to Appendix B in `docs/reference/reading-a-mikebom-sbom.md` per FR-015, preserving chronological-by-milestone ordering (research.md §R7).
- [X] T013 [US1] Append the trust-trio jq recipes to `specs/151-expand-consumer-guide/verify-recipes.sh`: one `evidence-kind-cdx` recipe + one `confidence-cdx` recipe + one `trust-trio-cdx` composing recipe (per research.md §R3.1). Use fixture `transitive_parity/cargo` (rich source-type variety per milestone-150 precedent) and expectation `"present"` (cargo fixtures may not carry every trio member on every component).

**Checkpoint**: §3.3 has 5 depth-covered signals (was 3); SC-005 cluster balance for §3.3 satisfied; trust-trio composition documented + cross-referenced + recipe-verified.

---

## Phase 4: User Story 2 — Binary linkage (Priority: P1)

**Goal**: A binary-tier consumer can read §3.1 (vulnerability scanning) and build CVE-suppression policies on `mikebom:linkage-kind` (closed enum) + `mikebom:not-linked` (two-state Go-only marker) without leaving the guide.

**Independent Test**: A reviewer opens the doc, navigates to §3.1, finds both new signals depth-covered; reads the `mikebom:not-linked` section and can articulate both the present-true semantic AND the two interpretations of absent + the disambiguation procedure; runs the documented jq recipes against a real binary-tier SBOM and gets the documented output shape.

### Implementation for User Story 2

- [X] T014 [US2] Add the `#### mikebom:linkage-kind` depth-coverage subsection inside §3.1 (placement: after the existing `mikebom:duplicate-purl-divergent` paired entry, before the closing of §3.1) in `docs/reference/reading-a-mikebom-sbom.md`, conforming to the per-signal rendering invariant: What it is (the binary-tier linkage mode for a component) / Where it lives (per-format, from research.md §R2) / Value space (closed enum `dynamic` / `static` / `mixed`) / What to do with it (CVE filtering for binary-tier components — alert on static-linked CVEs at higher severity than dynamic-linked) / Milestone: 005-era (binary readers); enum stabilized by milestone 104 / Catalog link: [C12](sbom-format-mapping.md) / jq recipe + Expected output (research.md §R3.2 CDX example).
- [X] T015 [US2] Add the `#### mikebom:not-linked` depth-coverage subsection inside §3.1 (after `mikebom:linkage-kind`) in `docs/reference/reading-a-mikebom-sbom.md`, conforming to the per-signal rendering invariant + the Go-only scope marker per FR-004 + the two-state interpretation rule per FR-004: What it is (Go-only marker that mikebom proved the linker DCE'd a `go.sum` entry from the produced binary's BuildInfo) / Where it lives / Value space (literal `true` when present) / What to do with it (CVE suppression — combine with `mikebom:linkage-kind` for full binary-tier filtering) / **Scope**: Go-only emission per milestone 050; the disambiguation procedure for absent (check whether `mikebom:component-tier = "binary"` components exist in the SBOM) / Milestone: 050 / Catalog link: [C41](sbom-format-mapping.md) / jq recipe + Expected output (research.md §R3.2 CDX example).
- [X] T016 [P] [US2] Update the Appendix A entries for `mikebom:linkage-kind` and `mikebom:not-linked` in `docs/reference/reading-a-mikebom-sbom.md` per FR-014: append `(see §3.1 for depth coverage)` to each one-line description, preserving the catalog C-row link.
- [X] T017 [P] [US2] Add entries for `mikebom:linkage-kind` (milestone 005-era) and `mikebom:not-linked` (milestone 050) to Appendix B in `docs/reference/reading-a-mikebom-sbom.md` per FR-015, preserving chronological ordering.
- [X] T018 [US2] Append the binary-linkage jq recipes to `specs/151-expand-consumer-guide/verify-recipes.sh`: one `linkage-kind-cdx` recipe + one `not-linked-cdx` recipe (per research.md §R3.2). Use a Go binary-bearing fixture if available in `$MIKEBOM_FIXTURES_DIR` (check `transitive_parity/golang/` per milestone-050 baseline); if no binary-bearing fixture exists in the sibling repo, use expectation `"present"` and document the fixture-gap in the harness comment so a future fixture-add fills the gap. Per research.md §R8 the harness MAY skip-with-note when fixtures lack signal.

**Checkpoint**: §3.1 has 6 depth-covered signals (was 4 counting paired collapse); SC-005 cluster balance for §3.1 satisfied; Go-only scope of `mikebom:not-linked` explicit; recipes verified or recorded-as-pending.

---

## Phase 5: User Story 3 — Unresolved deps + assertion conflict (Priority: P1)

**Goal**: A compliance auditor can read §3.4 (transparency / completeness) and use `mikebom:depends-unresolved` + `…-rdepends-unresolved` (paired closure-gap markers) and `mikebom:assertion-conflict` (supplement-merge audit signal) without leaving the guide.

**Independent Test**: A reviewer opens the doc, navigates to §3.4, finds both new entries depth-covered; reads the paired entry and understands both the JSON-encoded-array shape AND the reserved-key framing (Yocto-only emission today but key namespace reserved); reads the `mikebom:assertion-conflict` section and can articulate the structured record shape + the closed `justification` enum; runs the documented jq recipes and gets the expected output.

### Implementation for User Story 3

- [X] T019 [US3] Add the paired `#### mikebom:depends-unresolved + mikebom:rdepends-unresolved` depth-coverage subsection inside §3.4 (placement: after the existing `mikebom:graph-completeness + …-reason` paired entry) in `docs/reference/reading-a-mikebom-sbom.md`, conforming to the paired-entry shape per data-model.md §4: dual-heading + dual-recipe + the "Currently emitted by" reserved-key element per Clarifications Q2. Content per spec FR-005 + research.md §R3.3.
- [X] T020 [US3] Add the `#### mikebom:assertion-conflict` depth-coverage subsection inside §3.4 (after the paired unresolved-deps entry) in `docs/reference/reading-a-mikebom-sbom.md`, conforming to the per-signal rendering invariant: What it is (operator's supplement file declared X; mikebom scanner observed Y; here's who won and why) / Where it lives (per-format from R2) / Value space (JSON-encoded array of records `{field, scanner_value, supplement_value, winner, justification}`; closed `winner` enum `scanner`/`supplement`; closed `justification` enum `bytes-evident-detection-preserved`/`developer-metadata-override`) / What to do with it (auditor validates supplement-declared values against external evidence; scanner-wins records are typically informational) / Milestone: 119 / Catalog link: [C67](sbom-format-mapping.md) / jq recipe + Expected output (research.md §R3.4).
- [X] T021 [P] [US3] Update the Appendix A entries for `mikebom:depends-unresolved`, `mikebom:rdepends-unresolved`, and `mikebom:assertion-conflict` in `docs/reference/reading-a-mikebom-sbom.md` per FR-014: append `(see §3.4 for depth coverage)` to each one-line description, preserving the catalog C-row link.
- [X] T022 [P] [US3] Add entries for `mikebom:depends-unresolved` + `…-rdepends-unresolved` (milestone 128, listed as one or two adjacent entries) and `mikebom:assertion-conflict` (milestone 119) to Appendix B in `docs/reference/reading-a-mikebom-sbom.md` per FR-015, preserving chronological ordering.
- [X] T023 [US3] Append the §3.4 jq recipes to `specs/151-expand-consumer-guide/verify-recipes.sh`: one `depends-unresolved-cdx` recipe + one `assertion-conflict-cdx` recipe (per research.md §R3.3 + R3.4). For depends-unresolved: use a Yocto fixture if available; else expectation `"present"` with fixture-gap note (research.md §R8). For assertion-conflict: use a supplement-file fixture if available; else expectation `"present"` with the same fixture-gap note.

**Checkpoint**: §3.4 has 5 depth-covered signals (was 3 counting paired collapse); SC-005 cluster balance for §3.4 satisfied; reserved-key framing per Clarifications Q2 documented; SC-004 reaches ≥18 depth-covered signals (12 existing + 6 new); recipes verified or recorded-as-pending.

---

## Phase 6: User Story 4 — Curation rubric (Priority: P2)

**Goal**: A future maintainer can read §2.1 alone (≤5 minutes) and apply the rubric mechanically to any new `mikebom:foo-bar` annotation.

**Independent Test**: A second reviewer applies the documented rubric (criteria + threshold + procedure) to a randomly chosen NEW hypothetical `mikebom:*` key and produces a depth-vs-appendix verdict matching what the spec author would conclude — without consulting the spec author. Verified at authoring time per SC-006 by re-running the rubric against the 26 sampled signals (19 depth + 7 appendix-only) per research.md §R1.1 + §R1.2 and confirming 26/26 matches.

### Implementation for User Story 4

- [X] T024 [US4] Replace the T005 skeleton placeholder in `docs/reference/reading-a-mikebom-sbom.md` §2.1 with the full curation rubric: introductory paragraph (1-2 sentences naming the rubric as 5 criteria + threshold N=3) + the 5 criteria with one-paragraph elaborations each (content from contracts/rubric.md "Criterion definitions" section) + a "How to apply this to a new signal" 3-step block.
- [X] T025 [US4] Add the worked-example table inside §2.1 of `docs/reference/reading-a-mikebom-sbom.md` listing every depth-covered signal's rubric scoring (5 criteria + YES count + verdict). Use the table from research.md §R1.1 verbatim. The table doubles as the SC-006-first-half validation evidence visible to consumers.
- [X] T026 [US4] Add the counter-example table inside §2.1 of `docs/reference/reading-a-mikebom-sbom.md` listing 7 representative appendix-only signals' rubric scoring. Use the table from research.md §R1.2 verbatim. The table doubles as the SC-006-second-half validation evidence.
- [X] T027 [US4] Verify the §2.1 section is self-contained per SC-001 question 8 + US4 independent test: read §2.1 in isolation and confirm a hypothetical maintainer could apply the rubric to a new `mikebom:foo-bar` key in under 5 minutes without scrolling to §3 or the appendix.

**Checkpoint**: §2.1 exists with full rubric content; SC-006 mechanically verifiable via the two tables; future maintainers have an unambiguous depth-vs-appendix decision procedure.

---

## Phase 7: User Story 5 — Appendix hygiene (Priority: P3)

**Goal**: Every Appendix A entry corresponds to an actually-emitted annotation key; every cross-reference in the appendix resolves to a section that exists.

**Independent Test**: Reviewer runs the `comm -23 /tmp/appendix-keys.txt /tmp/emitted-keys.txt` check from quickstart.md Scenario 2b and gets empty output. Reviewer also runs the cross-reference check from quickstart.md Scenario 2c (extract `§X.Y` tokens, diff against section headings) and gets empty output.

### Implementation for User Story 5

- [X] T028 [US5] Run the US5 audit grep recipe from research.md §R9 to identify Appendix A entries with no corresponding emission site in `mikebom-cli/src/generate/` or `mikebom-cli/src/scan_fs/`. Output as a 3-column table (Key | Has emission site? | Action) per data-model.md §8.
- [X] T029 [US5] Apply the audit's outcome by removing each confirmed-internal-only key (specifically: `mikebom:component-role` per the maintainer-flagged candidate, plus any others surfaced by T028) from Appendix A in `docs/reference/reading-a-mikebom-sbom.md`. Preserve the corresponding catalog C-row in `sbom-format-mapping.md` (the catalog is the internal-pipeline doc and intentionally lists internal-only keys per FR-018).
- [X] T030 [US5] Run the SC-010 cross-reference resolution check from quickstart.md Scenario 2c and fix any broken `§X.Y` cross-reference in `docs/reference/reading-a-mikebom-sbom.md` Appendix A by repointing to a section that exists (depth coverage section if available, catalog C-row fallback per FR-011).

**Checkpoint**: Appendix A and the doc's cross-references are clean; SC-009 + SC-010 satisfied; the audit outcome is ready for inclusion in the milestone-151 PR description.

---

## Phase 8: Polish & cross-cutting

**Purpose**: Run the full quickstart audit suite, finalize the PR description, and confirm SC-001 through SC-010 are all satisfied.

- [X] T031 [P] Run `./specs/151-expand-consumer-guide/verify-recipes.sh` and confirm `Verification summary: N passed, 0 failed` with N ≥ 6 per SC-003. Capture the harness output for the PR description.
- [X] T032 [P] Run quickstart.md Scenarios 2a + 2b + 2c (the three appendix/cross-reference audits) and confirm SC-002, SC-009, SC-010 all satisfied.
- [X] T033 [P] Run quickstart.md Scenarios 3a + 3b (depth-covered signal count + cluster balance) and confirm SC-004 + SC-005 + FR-019. Assertions: depth-4 subsection count inside §3 = **exactly 18 sections** covering **21 unique catalog keys** (12 milestone-150 sections + 6 milestone-151 sections; 3 paired-entry collapses produce the 18 ↔ 21 delta — `duplicate-purl-divergent`+`purl-collisions-detected`, `graph-completeness`+`…-reason`, and milestone-151's `depends-unresolved`+`rdepends-unresolved`); cluster sizes exactly 5 / 3 / 5 / 5 for §3.1 / §3.2 / §3.3 / §3.4. Exact count enforces FR-019 "no additional signal promotions beyond the 6 listed" — any deviation means a 7th signal was inadvertently promoted, a paired-collapse was undone, or one of the 6 wasn't added. (Note: research.md §R4 + spec SC-005's original prose projected 19/6 due to an off-by-one count of milestone-150's pre-state §3.1; the as-delivered figure is 18/5. Reality verified at T033 time.)
- [X] T034 [P] Run quickstart.md Scenario 6 (single-file diff check) and confirm only `docs/reference/reading-a-mikebom-sbom.md` is touched in the shipped diff per SC-007. The `specs/151-expand-consumer-guide/` artifacts are accepted scaffolding. **Additionally enforce FR-018**: explicitly assert that `docs/reference/sbom-format-mapping.md` is NOT in the diff via `git diff main --name-only -- docs/reference/sbom-format-mapping.md` returning empty output. If the catalog file IS in the diff (FR-018 exception path fired during depth-coverage authoring), flag in the PR description under a new "Catalog-row clarifications" heading with a single-line justification per the exception clause. (Per analysis remediation A2.)
- [X] T035 Run `./scripts/pre-pr.sh` from the repo root per SC-008. Confirm clippy + tests pass with the same outcome as pre-151 main (the documented pre-existing `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only failure remains the only acceptable failure). **DONE**: clippy clean, 116/117 tests pass, only the documented `sbomqs_parity` env-only flake failed (exit 101 — matches pre-151 main behavior; docs-only milestone confirmed not to affect test outcomes).
- [X] T036 Conduct the SC-001 8-question read-through self-audit: read `docs/reference/reading-a-mikebom-sbom.md` end-to-end as a first-time consumer and answer each of the 8 questions in quickstart.md Scenario 1 using only the guide. Any question that requires consulting the catalog or source = an authoring gap to fix in the corresponding US's depth section before opening the PR.
- [X] T037 Draft the PR description with sections: Summary, Closes (no specific issue this milestone — origin is the post-150 maintainer-cadence review), Changes (the single-file diff + the new authoring artifacts), Verification (verify-recipes.sh output + scenarios 2/3/6/SC-008 results), US5 Audit Output (the 3-column table from T028), Constitution check (cite plan.md POST-DESIGN re-check), Reviewer-cadence operator-test instructions (point at quickstart.md Scenario 1). **DONE**: drafted at `specs/151-expand-consumer-guide/pr-description.md`; ready to copy into `gh pr create` HEREDOC.

**Final checkpoint**: Milestone 151 is shippable. Mark all tasks in this file complete in the PR.

---

## Dependencies & Execution Order

### Phase dependencies

```text
Phase 1 (Setup)
  └─> Phase 2 (Foundational)
        └─> Phase 3 (US1) ┐
            Phase 4 (US2) ├─┐
            Phase 5 (US3) ┘ │
            Phase 6 (US4) ──┤   (all four US phases can run in parallel)
                            ▼
                       Phase 7 (US5) — needs all four US phases settled
                            ▼
                       Phase 8 (Polish)
```

### Within-phase parallelism

- **Phase 1**: T001 sequential (verifies baseline); T002 + T003 [P] in parallel after T001
- **Phase 2**: T004 sequential (creates scaffold); T005 + T006 [P] in parallel after T004
- **Phase 3 (US1)**: T007 → T008 → T009 → T010 sequential (each adds a new section that the next may cross-reference); T011 + T012 [P] in parallel after T010; T013 sequential at the end (depends on the recipe content being settled).
- **Phase 4 (US2)**: T014 → T015 sequential (linkage-kind before not-linked since not-linked's "Composes with linkage-kind" cross-reference needs T014 to land); T016 + T017 [P] in parallel after T015; T018 sequential at the end.
- **Phase 5 (US3)**: T019 → T020 sequential; T021 + T022 [P] in parallel after T020; T023 sequential at the end.
- **Phase 6 (US4)**: T024 → T025 → T026 sequential (each builds on the prior); T027 sequential at the end.
- **Phase 7 (US5)**: T028 → T029 → T030 sequential (each consumes the prior's output).
- **Phase 8 (Polish)**: T031 + T032 + T033 + T034 [P] in parallel; T035 sequential after; T036 sequential after; T037 sequential at the end.

### Cross-US independence

US1, US2, US3, US4 touch DIFFERENT sections of `docs/reference/reading-a-mikebom-sbom.md`:

- US1 → §3.3
- US2 → §3.1
- US3 → §3.4
- US4 → §2.1

So they are semantically independent. Authoring order is the author's choice; reviewers can verify each US section independently.

US5 touches Appendix A (broad sweep) and follows because it folds in the FR-014 cross-reference updates made in US1/US2/US3 + the FR-010 internal-only-key removals.

## Implementation strategy

### MVP scope

The MVP is **US1 + US2 + US3 together** (the 3 P1 user stories). Shipping the trust trio + linkage signals + unresolved-deps + assertion-conflict closes the maintainer-flagged curation gap on the 6 most consumer-actionable signals. US4 (rubric) is the meta-fix preventing future drift; it's P2 because the immediate consumer-facing gap is closed by the 3 P1 stories. US5 (appendix hygiene) is P3 — low-risk hygiene that doesn't block consumer workflows.

A degenerate "P1-only" milestone (skipping US4 + US5) would still satisfy SC-004 (≥18 depth-covered signals) and SC-005 (cluster balance) — but would leave SC-006 (curation rubric application) unsatisfied and would propagate the same "feels random" risk that drove this milestone. The maintainer-cadence review pattern means shipping all 5 USs together is the cheap-and-right call; the prioritization exists so a future bisect can split if needed.

### Incremental delivery

If the milestone needs to be split for review-load reasons (it shouldn't — single-file 250-LOC doc edit), the split point is between US3 and US4: ship a "151a" with US1+US2+US3 as the depth-coverage expansion; ship "151b" with US4+US5 as the curation-discipline pass. NOT recommended; mentioned only for completeness.

### Per-task time estimate

- Phase 1 (T001–T003): ~10 min total (validation only, no authoring)
- Phase 2 (T004–T006): ~15 min total (skeleton setup)
- Phase 3 (US1, T007–T013): ~60 min (trust-trio is the heaviest US; 3 new sections + cross-references + 3 recipes)
- Phase 4 (US2, T014–T018): ~45 min (2 new sections + recipes)
- Phase 5 (US3, T019–T023): ~50 min (1 paired section + 1 solo section + recipes)
- Phase 6 (US4, T024–T027): ~60 min (rubric content is dense; 2 tables; self-audit at end)
- Phase 7 (US5, T028–T030): ~30 min (audit + cleanup)
- Phase 8 (Polish, T031–T037): ~45 min (full audit suite + PR description)

**Total**: ~5 hours focused authoring + audit, single sitting feasible. Mirrors milestone-150's authoring effort.
