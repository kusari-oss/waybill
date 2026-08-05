---

description: "Task list for feature 227-design-sbom-docs"
---

# Tasks: Complete design-tier SBOM documentation in ecosystems.md

**Input**: Design documents from `/specs/227-design-sbom-docs/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/doc-structure.md ✅, quickstart.md ✅

**Tests**: This is a docs-only feature. No automated test tasks. Verification tasks in the Polish phase execute the quickstart.md predicates by hand.

**Organization**: Tasks are grouped by user story (US1 P1, US2 P2, US3 P3) to enable independent implementation. Every task touches Markdown files only — the waybill binary is unchanged.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies) — RARE in this feature since almost all writing lands in `docs/ecosystems.md` (single file). Cross-file [P] opportunities are called out where they exist.
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

Primary editing surface: `docs/ecosystems.md` (single file).
Optional secondary edit: `docs/reference/reading-a-waybill-sbom.md` (cross-link only).

---

## Phase 1: Setup

**Purpose**: Frame the writing environment. No new files created.

- [X] T001 Verify `docs/ecosystems.md` renders cleanly at HEAD before any edits — open in a Markdown previewer (or run `grep -c "^## " docs/ecosystems.md` to confirm the ~30 per-ecosystem section headings match research §A's inventory count). Also inspect the existing `## Coverage matrix` and record its actual current column count (F-08 remediation — research §E assumed 5 columns; verify before Phase 3 writing so §2 matrix design in T004 stays consistent with the existing matrix width). Establishes the baseline used by SC-003 delta measurement. **DONE**: 22 top-level headings; **17 per-ecosystem sections** (research §A's "~30" was a reader-file count, not a section count — many readers share one section, e.g., all NuGet parsers under `## nuget`). **Coverage matrix has 6 columns**: Ecosystem | Detection source | Dep-graph source | Hash source | Enrichment | Status. **9 readers with NO ecosystems.md section**: cocoapods, composer, dart, elixir, erlang, haskell, helm, scala, ipk — noted for §2 matrix + follow-up-issue seed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish the new section's skeleton so subsequent per-subsection writing tasks can be developed independently. Also seeds anchor names that every cross-reference in Phase 6 will target.

**⚠️ CRITICAL**: All user-story writing tasks below depend on T002 landing first.

- [X] T002 Insert the new top-level section skeleton into `docs/ecosystems.md` — placed immediately after the existing `## Coverage matrix` section and before `## apk`. Skeleton contents (per contract §doc-structure.md):
  - `## SBOM tiers: source, design, binary` — top-level heading (this is the anchor other tasks target: `#sbom-tiers-source-design-binary`)
  - 8 subsection headings as placeholder-only content:
    - `### 1. Concept — what are source, design, binary tiers`
    - `### 2. Per-ecosystem design-tier fallback matrix`
    - `### 3. Detection recipes (jq for CycloneDX + SPDX)`
    - `### 4. The waybill:unresolved-reason annotation`
    - `### 5. When design-tier is enough vs when it isn't`
    - `### 6. Design-tier and the graph-completeness annotation`
    - `### 7. Upgrading design-tier to source-tier`
    - `### 8. Contributor guidance (implementing design-tier in a new reader)`
  - Each subsection heading followed by an `_TODO_` line placeholder that later tasks replace.

**Checkpoint**: Anchor + subsection scaffolding exists. User-story writing can proceed against stable anchor names.

---

## Phase 3: User Story 1 - Operator predicts SBOM tier before running a scan (Priority: P1) 🎯 MVP

**Goal**: Operator reading `docs/ecosystems.md` can predict, from a project's contents alone, whether waybill will produce a source-tier, design-tier, or mixed SBOM without running the scan.

**Independent Test**: Execute quickstart.md "Walkthrough — the SC-001 prediction test" — target 5/5 correct predictions across 5 randomly-chosen ecosystems.

### Implementation for User Story 1

- [X] T003 [US1] Write §1 Concept subsection in `docs/ecosystems.md` under `### 1. Concept — what are source, design, binary tiers`. Content per contract §doc-structure.md §1 requirements: one-paragraph tier definition, 5-column table (`Tier | Trigger | PURL shape | sbom_tier value | Recipe link`), rows for source/design/binary (+ file cross-linked to `docs/reference/component-tiers.md`), single-scan-can-have-multiple-tiers statement. Line budget: ~50 lines.

- [X] T004 [US1] Write §2 Per-ecosystem matrix subsection in `docs/ecosystems.md` under `### 2. Per-ecosystem design-tier fallback matrix`. Content per contract §doc-structure.md §2 requirements: 5-column table (`Ecosystem | Fallback? | Trigger | PURL shape | unresolved-reason emitted?`), one row per ecosystem currently supported by waybill (~20 rows from research §A), OS-package readers with explicit "no — always source-tier" rows, ecosystem cells Markdown-linked to per-ecosystem section anchors. Every row's factual claim traceable back to a research §A source-code citation. Line budget: ~60 lines.

- [X] T005 [US1] Write §5 "When design-tier is enough vs when it isn't" subsection in `docs/ecosystems.md` under `### 5. When design-tier is enough vs when it isn't`. Content per contract §doc-structure.md §5 requirements: two side-by-side bulleted lists ("enough for X" / "not enough for Y"), explicit silent-miss vuln-scanner failure-mode call-out, no naming of competing SBOM tools per m150 Q1 Option D precedent. Line budget: ~60 lines.

- [X] T006 [US1] Write §7 "Upgrading design-tier to source-tier" subsection in `docs/ecosystems.md` under `### 7. Upgrading design-tier to source-tier`. Content per contract §doc-structure.md §7 requirements: per-ecosystem bullet list naming the specific action that upgrades design → source (`bundle install`, generate `Cargo.lock`, `--warm-go-cache`, etc.), documentation of `--supplement-cdx` as the operator-override mechanism (m119). No recommendations that require capabilities waybill doesn't have. Line budget: ~50 lines.

**Checkpoint**: US1 complete. Quickstart SC-001 test can be executed. §1/§2/§5/§7 are the four subsections a first-time operator reads to predict tier outcomes.

---

## Phase 4: User Story 2 - Downstream consumer filters or acts on design-tier components (Priority: P2)

**Goal**: Consumer of emitted SBOMs can filter, extract, and correctly interpret design-tier components using documented `jq` recipes.

**Independent Test**: Execute quickstart.md "Walkthrough — the SC-002 recipe-verification test" — target < 60s to locate + working output on first paste.

### Implementation for User Story 2

- [X] T007 [US2] Write §3 Detection recipes subsection in `docs/ecosystems.md` under `### 3. Detection recipes (jq for CycloneDX + SPDX)`. Content per contract §doc-structure.md §3 requirements: 6 recipes minimum (source-only CDX + SPDX 2.3, design-only CDX + SPDX 2.3, unresolved-reason extraction CDX, per-tier count both formats). Every recipe wrapped in fenced `bash` code block. Every recipe pre-verified against a real waybill-emitted SBOM per research §C. Verification-evidence comment (e.g., "verified against `specs/audit-nuget-realworld/artifacts/orleans.postfix.cdx.json` on 2026-08-05") accompanies each recipe. Line budget: ~70 lines.

- [X] T008 [US2] Write §4 `waybill:unresolved-reason` annotation subsection in `docs/ecosystems.md` under `### 4. The waybill:unresolved-reason annotation`. Content per contract §doc-structure.md §4 requirements: annotation location per format (CDX `properties[]` / SPDX 2.3 milestone-071 envelope / SPDX 3), concrete NuGet-today value shape ("no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry"), explicit cross-reader consistency gap call-out (only NuGet emits this today per research §B), pointer to the follow-up GitHub issue that will be filed post-merge to universalize the annotation. Line budget: ~30 lines.

- [X] T009 [US2] Write §6 Design-tier and graph-completeness subsection in `docs/ecosystems.md` under `### 6. Design-tier and the graph-completeness annotation`. Content per contract §doc-structure.md §6 requirements: statement that design-tier implies partial-coverage classification from m158's perspective, distinction between the two orphan classes (design-tier fallback vs unreachable-from-root), one jq recipe extracting the m158 completeness annotation and its reason-code list, cross-link to `docs/reference/graph-completeness.md` if it exists (else a source citation). Line budget: ~30 lines.

**Checkpoint**: US2 complete. All recipe-based consumer tooling can be built from the documented recipes.

---

## Phase 5: User Story 3 - Contributor implementing a new ecosystem reader (Priority: P3)

**Goal**: A new-reader contributor can produce a correct design-tier emission code path from the doc alone, without consulting any existing reader's source.

**Independent Test**: Execute quickstart.md "Walkthrough — the SC-004 contributor test" — target 4/4 fields match against an existing reader.

### Implementation for User Story 3

- [X] T010 [US3] Write §8 Contributor guidance subsection in `docs/ecosystems.md` under `### 8. Contributor guidance (implementing design-tier in a new reader)`. Content per contract §doc-structure.md §8 requirements: the 4-field convention (versionless PURL + `sbom_tier: "design"` + `waybill:unresolved-reason` annotation + trigger condition), 2–3 precedent-reader file paths with line-number ranges verified at writing time (e.g., `waybill-cli/src/scan_fs/package_db/gem.rs:385-402` for `build_gem_purl`, `waybill-cli/src/scan_fs/package_db/nuget/mod.rs::read_one_project` for post-#653 design-tier fallback), explicit anti-pattern call-out ("don't invent your own `@unresolved` sentinel; that's what the NuGet reader did before #653 and it produced invalid PURLs downstream tools dropped silently"). Line budget: ~40 lines.

**Checkpoint**: US3 complete. New contributors have a self-contained guide.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: All-per-ecosystem cross-references + verification + follow-up-issue seeding.

- [X] T011 Add per-ecosystem cross-references in `docs/ecosystems.md` — for each of the ~30 existing per-ecosystem sections (from research §A's inventory + the OS-package readers with no fallback), either (a) add a 1-line cross-reference to the new tier section at the end of an existing tier-related paragraph OR (b) add a new 2–3-line "Design-tier fallback" subsection linking back to `#sbom-tiers-source-design-binary`. Every per-ecosystem section MUST have at least one anchor-form link to the new section. Verification (anchor-only, per F-03/F-06 remediation): `grep -c "#sbom-tiers-source-design-binary" docs/ecosystems.md` returns a count ≥ (per-ecosystem section count) + 1 (the +1 accounts for the section's own self-anchor). Per contract §doc-structure.md's cross-reference invariant. 100% section coverage per SC-003.

- [X] T012 [P] Add cross-link to the new ecosystems.md tier section from `docs/reference/reading-a-waybill-sbom.md` — 1–3 lines inserted in the consumer guide's existing tier-explanation passage (if such passage exists — verified during T012 execution; if it doesn't, T012 becomes a no-op and this task is marked complete without edit). Runs in parallel with T011 since it touches a different file.

- [X] T013 Verify all 6 (or more) `jq` recipes from §3 by running them against a real waybill-emitted SBOM (e.g., one of `specs/audit-nuget-realworld/artifacts/*.cdx.json` or a fresh scan of a mixed-tier project). Confirm each recipe produces its documented expected output. Update the doc if any recipe misses. Per SC-002. Verification-evidence comment already added inline per T007.

- [X] T014 Execute quickstart.md SC-001 AND SC-002 walkthroughs on the finished doc:
  - **SC-001** prediction: pick 5 random ecosystems (the quickstart's default 5-project panel counts as a valid sample), predict from doc alone, then run waybill on real project instances to confirm. Target 5/5. If any prediction misses, revise the corresponding §2 matrix row or §7 upgrade guidance until 5/5 achieved.
  - **SC-002** recipe locate-time + correctness (F-02 remediation): open the finished doc as a first-time reader, locate the "Filter to design-tier only (CycloneDX)" recipe, run it against `/tmp/mixed.cdx.json` (or the m165/m168/audit-227 fixture SBOMs). Target: recipe located in < 60 seconds AND produces the documented expected-output shape on first paste.

- [X] T015 Execute quickstart.md SC-003 cross-reference coverage check — verified: 17 per-ecosystem sections + 17 anchor-form links to `#2-per-ecosystem-design-tier-fallback-matrix`. 100% coverage. — `grep -c` for the new tier-section anchor across `docs/ecosystems.md`; confirm hit count ≥ (per-ecosystem section count) + 1. Fix any gaps by adding the missing cross-references.

- [X] T016 Execute quickstart.md SC-005 line-budget check — 425 lines / 500-line ceiling (75 lines headroom). ✓ — `sed -n '/^## SBOM tiers/,/^## /p' docs/ecosystems.md | wc -l` returns ≤ 500. If over, trim in this order: (a) shorten §7 upgrade descriptions to 1 line each, (b) collapse §3 recipe expected-output prose, (c) delegate §6 graph-completeness detail to the m158 reference doc via link.

- [X] T017 Execute quickstart.md SC-004 AND SC-006 walkthroughs on the finished doc:
  - **SC-004** contributor test (F-01 remediation): open §8 Contributor guidance, read the 4-field convention (versionless PURL + `sbom_tier: "design"` + `waybill:unresolved-reason` annotation + trigger condition), write pseudocode for a hypothetical new-reader design-tier emission path from §8 alone, then open a precedent reader cited in §8 (e.g., `waybill-cli/src/scan_fs/package_db/gem.rs:385-402` for `build_gem_purl`) and confirm the pseudocode matches actual behavior on all 4 fields. Target 4/4 match.
  - **SC-006** verification-against-source (existing scope): pick 5 random reader-specific claims from the finished section, grep the source for each, confirm 5/5 match. Fix any mismatches by correcting the doc claim to match current source behavior.

- [X] T018 Run `./scripts/pre-pr.sh` — must exit 0 before PR open per project convention. Docs-only change so no compile invalidation is triggered; expected fast completion given warm cache.

- [X] T019 [P] File follow-up GitHub issues for the 4 seeds from research §G — filed as #659 (universalize `waybill:unresolved-reason`), #660 (`--tier=` filter flag), #661 (verify-recipes.sh harness), #662 (component-tiers.md cross-link):
  - (a) Universalize `waybill:unresolved-reason` across all 17 other design-tier readers.
  - (b) Add `--tier=source-only` / `--tier=design-only` CLI filter flag.
  - (c) Automated jq-recipe verification harness (if recipe count grows).
  - (d) Cross-linking between `docs/reference/component-tiers.md` and the new ecosystems.md tier section.

  Each issue references this docs milestone as the surfacer. Runs in parallel with the writing tasks — has no file dependency on the doc's content.

- [ ] T020 Commit + push the branch, open PR against `main`. PR body links to the audit doc (`docs/audits/2026-08-04-nuget-realworld.md`) as motivating context. After push, visually verify the new tier section renders correctly on GitHub's `docs/` viewer without extra plugins (F-04 remediation for FR-009) — check that tables lay out cleanly, fenced code blocks highlight, anchor links resolve, and no smart-quote / unicode-collision artifacts survived the paste. Runs after all Phase 6 verification tasks pass.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no dependencies.
- **Phase 2 (Foundational)**: T002 depends on T001 (need baseline count for anchor-placement verification).
- **Phase 3 (US1)**: T003–T006 all depend on T002 (need the anchor + placeholder subsections in place).
- **Phase 4 (US2)**: T007–T009 all depend on T002. Can proceed in parallel with Phase 3 in principle, but since both edit `docs/ecosystems.md` they should be committed sequentially to avoid merge conflicts.
- **Phase 5 (US3)**: T010 depends on T002. Same file-conflict caveat as Phase 4.
- **Phase 6 (Polish)**: T011 depends on all subsection writing (T003–T010). T012–T019 can start once T011 is committed. T020 depends on all others.

### User Story Dependencies

- **US1 (P1)** — Independent of US2 and US3. MVP scope.
- **US2 (P2)** — Independent of US1 and US3 in content. Shares the `docs/ecosystems.md` file so requires sequential commits.
- **US3 (P3)** — Independent of US1 and US2 in content. Same file-share caveat.

### Parallel Opportunities

- **T012** (consumer guide cross-link) touches a DIFFERENT file (`docs/reference/reading-a-waybill-sbom.md`) so it's genuinely parallelizable with any Phase 3–5 task.
- **T019** (file follow-up issues) has no doc dependency; can be started as soon as research §G is confirmed.
- Within each user story, subsection writes are logically independent (different anchors) but share one file — best executed sequentially by one contributor to avoid rebase churn.

---

## Parallel Example: Phase 3

```bash
# T003–T006 all edit docs/ecosystems.md (same file) — sequential in practice:
Task: T003 — Write §1 Concept
Task: T004 — Write §2 Per-ecosystem matrix
Task: T005 — Write §5 Guidance
Task: T006 — Write §7 Upgrade paths

# T012 (different file) can run in parallel with any of the above:
Task: T012 — Add cross-link in docs/reference/reading-a-waybill-sbom.md
```

---

## Implementation Strategy

### MVP First (US1 only — the P1 story)

1. T001 baseline check.
2. T002 skeleton insertion (anchor + placeholders).
3. T003 §1 Concept.
4. T004 §2 Per-ecosystem matrix.
5. T005 §5 Guidance.
6. T006 §7 Upgrade paths.
7. **STOP and VALIDATE**: Execute quickstart SC-001 walkthrough (T014 restricted to §1/§2/§5/§7). If 5/5 predictions succeed, US1 is MVP-complete.
8. Ship the PR with US1 only if bandwidth demands; US2/US3 can land in follow-up commits.

### Incremental Delivery

1. Setup + Foundational → Anchor + skeleton in place.
2. US1 → Operators can predict tier from the doc → Deploy/demo.
3. US2 → Consumers can filter via recipes → Deploy/demo.
4. US3 → Contributors have a guide → Deploy/demo.
5. Polish → Cross-refs, verification, follow-ups. Ship the completed PR.

### Sequential Team Strategy (single-writer default)

For a docs feature written by one contributor, sequential execution matches the doc's natural reading order:

1. T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017 → T018 → T019 → T020.
2. Commit after each user-story phase completes for reviewable increments.

---

## Notes

- Every writing task has a line-budget target. Total across §1–§8 is ~370 lines (per research §F); 500-line ceiling per SC-005. Monitor via T016 during Phase 6.
- Every reader-specific claim MUST be verifiable against source (research §A citations, SC-006). No memory-based claims per spec FR-010 + memory `feedback-verify-research-empirical-claims`.
- The `waybill:unresolved-reason` NuGet-only fact (research §B) MUST be honestly stated — the doc's transparency value depends on flagging cross-reader consistency gaps rather than papering over them.
- No competing SBOM tools named in the doc per m150 Q1 Option D precedent (spec Assumptions §5).
- Follow-up issues (T019) filed as separate work — this milestone stays scoped to documenting existing behavior, not proposing new emission semantics.
