---
description: "Task list for milestone 150 — SBOM consumer-facing reading guide documenting mikebom annotations and differentiators"
---

# Tasks: milestone 150 — SBOM consumer-facing reading guide

**Input**: Design documents from `/specs/150-sbom-consumer-guide/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: This is a pure-docs milestone. There are no Rust source code tests to write. Quality is validated via operator-cadence read-through audit (SC-001), mechanical audits for appendix coverage / index linkback / cluster count / signal count (SC-002 / SC-003 / SC-005 / SC-006), and jq-recipe verification at doc-authoring time (SC-004). The pre-PR gate (SC-007) is essentially a no-op since no Rust source changes. The reverse-link audit (SC-008) is mechanical.

**Organization**: US1 (P1) ships the bulk of the doc — opening positioning + 12 depth-covered signals across 4 clusters + envelope + cross-format reading + cross-refs. US2 (P2) layers the "For tool authors" section on top. The appendix index (102 entries) is polish-phase since it's mechanical bulk after the depth-coverage signals stabilize.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different sections of the doc, no dependencies)
- **[Story]**: Maps to user story from spec.md (US1, US2)
- Paths absolute under repo root `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

Touched files (docs-only milestone, narrow scope):

- `docs/reference/reading-a-mikebom-sbom.md` — NEW single-file deliverable; every authoring task appends or edits this file
- `docs/index.md` — ONE-line update in the Reference material section
- `specs/150-sbom-consumer-guide/verify-recipes.sh` — NEW authoring artifact (not shipped to public docs); lists scan-then-jq invocations for SC-004 verification

---

## Phase 1: Setup

**Purpose**: Verify baseline; create the new doc file skeleton.

- [ ] T001 Confirm baseline pre-PR gate is green on branch `150-sbom-consumer-guide`. Run `./scripts/pre-pr.sh` from repo root. Expected: clippy `--workspace --all-targets -- -D warnings` clean; `cargo test --workspace` passes except for the pre-existing local-environment `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` failure documented in milestone-144 T001. This milestone is docs-only so the gate is essentially a no-op for behavior validation — it's a smoke-test confirming nothing in the current main was broken by the branch creation. If anything ELSE fails, halt and investigate before proceeding.

- [ ] T002 Create the new doc file at `/Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md` with the 10-section TOC scaffolding per `contracts/doc-structure.md`. Each section gets a placeholder header (`## 1. Opening positioning`, etc.) + a `[TODO: filled in by T0XX]` marker. The TOC at the top of the doc lists all 10 sections (with anchor links). After this task, subsequent authoring tasks fill in each section.

---

## Phase 2: Foundational

**Purpose**: None required. This is a docs milestone — no foundational types, no shared infra to land first. The doc scaffold from T002 is the only prerequisite for authoring tasks.

(No tasks in this phase. Proceed directly to Phase 3.)

---

## Phase 3: User Story 1 - Compliance engineer reads a mikebom SBOM for the first time (Priority: P1) 🎯 MVP

**Goal**: Author all content needed for the SC-001 5-question read-through audit — opening positioning, 12 depth-covered signals across 4 thematic clusters, envelope schema documentation, cross-format reading patterns, stability statement, and cross-reference section. After this phase, an SBOM consumer can read the doc + answer all 5 SC-001 questions without consulting other docs.

**Independent Test**: A reviewer reads the doc end-to-end and answers the 5 SC-001 questions (`mikebom:lifecycle-scope` meaning, finding dev-only deps in SPDX 2.3, finding OCI layer attribution, trace-observed vs declared-not-cached source-type, envelope shape) without opening any other file.

### Implementation for User Story 1

- [ ] T003 [US1] Author the Section 1 "Opening positioning" content (~30 lines) at `/Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md`. Per `contracts/doc-structure.md` §1 contract: state mikebom strict-conforms to CDX 1.6 / SPDX 2.3 / SPDX 3.0.1; most data lives in spec-native fields; `mikebom:*` annotations are parity-bridges per Constitution Principle V (introduced ONLY when no native field carries the signal); the doc's job is to tell consumers what each parity-bridge means and how to use it. Per the 2026-06-29 clarification (Q1 Option D) the section MUST NOT name specific competing SBOM tools — use phrasing like "the CDX/SPDX spec baseline" or "standard SBOM output" when contrast is needed.

- [ ] T004 [US1] Author the Section 2 "How to read this doc" content (~20 lines). Reader-navigation map: vulnerability scanning → §3.1; compliance auditing → §3.2; tool authors → §7; appendix lookup for unknown keys → Appendix A; full per-format wire-shape catalog → `sbom-format-mapping.md`. Establishes the doc's flow + sets reader expectations.

- [ ] T005 [P] [US1] Author Section 3.1 — Vulnerability scanning cluster (3 signals: `mikebom:lifecycle-scope`, `mikebom:layer-digest`, `mikebom:duplicate-purl-divergent` + `mikebom:purl-collisions-detected` as the document-scope companion). Each signal renders per the per-signal invariant in `data-model.md` §2 (5 fields + a verified `jq` recipe + expected output). Cluster intro (~5 lines) explains why these signals matter for vulnerability scanners. Total ~120 lines.

- [ ] T006 [P] [US1] Author Section 3.2 — Compliance auditing cluster (3 signals: `mikebom:license-concluded-source`, `mikebom:component-tier` for file value, `mikebom:demoted-from-main-module`). Same per-signal rendering invariant. Cluster intro explains why these signals matter for license / compliance auditors. Total ~120 lines.

- [ ] T007 [P] [US1] Author Section 3.3 — Build provenance cluster (3 signals: `mikebom:source-type`, `mikebom:generation-context`, `mikebom:source-document-binding`). Same rendering invariant. Cluster intro explains why these signals matter for consumers verifying build-time provenance vs lockfile-derived enrichment. Total ~120 lines.

- [ ] T008 [P] [US1] Author Section 3.4 — Transparency / completeness gaps cluster (3 signals: `mikebom:file-inventory-mode`, `mikebom:graph-completeness` + `mikebom:graph-completeness-reason` paired, `mikebom:peer-edge-targets`). Same rendering invariant. Cluster intro explains why these signals matter for consumers evaluating SBOM completeness + provenance gaps. Total ~120 lines.

- [ ] T009 [US1] Author Section 4 "The mikebom-annotation/v1 envelope" (~40 lines) per `contracts/doc-structure.md` §4. Include: the envelope schema (3 fields: `schema`, `field`, `value`) shown inline; one example per format embedded — CDX `properties[]` flat string carrier, SPDX 2.3 annotation `comment` envelope, SPDX 3 annotation `statement` envelope; pointer to canonical Rust sources (`mikebom-cli/src/generate/spdx/annotations.rs:31-67` encoder + `mikebom-cli/src/parity/extractors/common.rs:185` decoder); statement that the envelope schema string is the stability anchor (future evolutions would bump version to `mikebom-annotation/v2`).

- [ ] T010 [US1] Author Section 5 "Cross-format reading patterns" (~60 lines) per `contracts/doc-structure.md` §5. Table or list showing the SAME signal in 3 formats for 4–5 representative depth-covered signals (recommended: `mikebom:lifecycle-scope`, `mikebom:layer-digest`, `mikebom:source-type`, `mikebom:demoted-from-main-module`); demonstrates the per-format carrier-shape variation. Pointer to `sbom-format-mapping.md` as the canonical wire-shape source. Note about SPDX 3 subject-routing quirks where applicable (e.g., `mikebom:demoted-from-main-module` routes to synth-root IRI per milestone-149 C102 docs).

- [ ] T011 [US1] Author Section 6 "Stability" (~40 lines) per `contracts/doc-structure.md` §6. Cover: every `C*` row in the catalog is a stable wire shape; the row number is the durable identifier; the `mikebom-annotation/v1` envelope shape is stable; explicit list of opt-in / experimental flags affecting emission (`--file-inventory=full`, `--preserve-manifest-main-module`, `--include-dev`, etc.); versioning statement (mikebom emissions follow `v*-alpha.*` tag sequence; map binary version → signal availability via Appendix B milestone citations).

- [ ] T012 [US1] Author Section 8 "Cross-references" (~30 lines) per `contracts/doc-structure.md` §8. Linked-list of related docs: `docs/reference/sbom-format-mapping.md` (catalog), `docs/reference/identifiers.md`, `docs/reference/sbom-types.md`, `docs/reference/component-tiers.md`, `docs/reference/cross-tier-binding.md`, `../../CHANGELOG.md`. Each link has a 1-line description of what the linked doc covers.

**Checkpoint**: After Phase 3, US1 is fully covered. A reviewer can read sections 1–6 + 8 and answer all 5 SC-001 questions without consulting other docs. The 8-signal depth-covered floor (SC-006) is met with 12 signals; the 4-cluster floor (SC-005) is met. Appendix A (102 entries) is in Phase 5 since it's mechanical bulk after the depth-covered signals stabilize.

---

## Phase 4: User Story 2 - Vulnerability scanner author integrates mikebom-specific signals (Priority: P2)

**Goal**: Author the Section 7 "For tool authors" section — tool-author-specific summary covering envelope schema location, carrier-mapping table, stability statement, and suggested integration patterns. After this phase, a downstream tool author can implement a mikebom-signal integration from the doc alone without reading mikebom source.

**Independent Test**: A tool author reads Section 7 + the cross-referenced Sections 4 + 5 + 6 and can codify a filter or correlation rule using ONE mikebom-specific signal (e.g., suppress dev-deps from vulnerability alerts using `mikebom:lifecycle-scope`) without consulting mikebom source.

### Implementation for User Story 2

- [ ] T013 [US2] Author Section 7 "For tool authors" (~60 lines) per `contracts/doc-structure.md` §7. Include: envelope schema location pointer (cross-ref to §4); per-format carrier-mapping table (cross-ref to §5 + catalog); stability statement (cross-ref to §6); suggested integration patterns (filter dep-graph by `mikebom:lifecycle-scope` for production-vulnerability suppression / walk `mikebom:layer-digest` for layer-attribution in OCI scans / correlate `mikebom:duplicate-purl-divergent` for divergence-detection workflows / etc.); pointer to GitHub Issues for bug reports or signal-shape concerns.

**Checkpoint**: After Phase 4, US2 is fully covered. The doc now serves both consumer-onboarding (US1 sections 1–6 + 8) and tool-author leverage (US2 section 7).

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Author the appendix index + milestone-citation map; update `docs/index.md`; verify jq recipes; pre-PR gate; commit chain.

- [ ] T014 Author Appendix A "Annotation key index" (~200 lines for 102 entries) per `contracts/doc-structure.md` Appendix A. Generate the key list mechanically from `sbom-format-mapping.md`:

  ```bash
  grep -E "^\| C[0-9]+\b" /Users/mlieberman/Projects/mikebom/docs/reference/sbom-format-mapping.md \
      | grep -oE "mikebom:[a-z0-9-]+" | sort -u
  ```

  Returns 102 keys at milestone-150 ship time. For each key, author one entry per the data-model §3 invariant: `- **\`mikebom:<key>\`** — <one-line description> ([C<row>](sbom-format-mapping.md#c<row>-mikebom-<key>))`. The one-line description comes from the catalog's row description (paraphrased to fit one line, ~10-15 words). Alphabetically sorted.

- [ ] T015 [P] Author Appendix B "Milestone-citation map" (~30 lines) per `contracts/doc-structure.md` Appendix B. Table mapping each of the 12 depth-covered signals (from sections 3.1–3.4) to its introducing/stabilizing milestone with a brief verb. Lookup source: each signal's "Milestone" field from the per-signal rendering (filled in by T005–T008). Format: 3-column markdown table.

- [ ] T016 [P] Update `/Users/mlieberman/Projects/mikebom/docs/index.md` per the "Index update contract" in `contracts/doc-structure.md`. Add the one-line entry in the **Reference material** section, positioned to fit the existing list's topical ordering (recommended: between the existing `sbom-format-mapping.md` entry and `conformance-harness-guide.md` entry — the new doc is the consumer-facing companion to the catalog). The one-line description identifies it as the consumer-onboarding surface and cross-references the catalog.

- [ ] T017 [P] Create the jq-recipe verification artifact at `/Users/mlieberman/Projects/mikebom/specs/150-sbom-consumer-guide/verify-recipes.sh`. Per `quickstart.md` Scenario 4: for each `jq` recipe in the doc, list (a) the `mikebom sbom scan` command that produces the source SBOM, (b) the exact `jq` recipe from the doc, (c) the documented expected output. The script is an authoring artifact (lives in the spec dir, NOT shipped to `docs/`). Operator-cadence reviewer can re-run post-merge to confirm recipes still produce documented output if doubt arises.

- [ ] T018 Run all jq recipes in the doc against real mikebom-emitted SBOMs and confirm outputs match the documented "Expected output" blocks. Per SC-004: at least 5 recipes verified runnable. Per FR-011: each recipe MUST be correct as written. Update the doc's "Expected output" blocks to match actual output where any mismatch is observed (or update the recipe — implementer's choice based on which is wrong). The verify-recipes.sh script from T017 makes this re-runnable.

- [ ] T019 Run mandatory pre-PR gate per Constitution Development Workflow + memory `feedback_prepr_gate_full_output.md`: `./scripts/pre-pr.sh` from repo root. Both clippy + test steps MUST pass clean (excepting the pre-existing local `sbomqs_parity` env-only failure documented in milestone-144 T001 — CI will validate on a clean runner). Since this milestone is docs-only, the gate is essentially a smoke-test confirming no inadvertent Rust source-tree disruption. Covers SC-007.

- [ ] T020 Verify the SC-002 appendix-coverage audit + SC-003 index-linkback audit + SC-005 cluster audit + SC-006 signal-count audit + SC-008 reverse-link audit per `quickstart.md` Scenarios 2 + 3 + 5 + 6 + 8 — mechanical shell-grep audits. All MUST pass before commit. Each audit's pass/fail outcome documented in the PR description.

- [ ] T021 Commit the milestone-150 changes. Per project convention (matching milestones 134/144/145/146/147/148/149), use the 4-commit chain:
  - `spec(150): SBOM consumer-facing reading guide — documenting mikebom annotations and differentiators` — spec.md + checklists/requirements.md
  - `plan(150): docs-only milestone with 12 depth-covered signals across 4 clusters + 102-key appendix + cross-refs` — plan + research + data-model + contracts + quickstart + CLAUDE.md
  - `tasks(150): 21 tasks across 5 phases for SBOM consumer guide` — tasks.md + verify-recipes.sh (the authoring artifact from T017 lives under the spec dir, ships with the spec)
  - `docs(150): publish reading-a-mikebom-sbom.md consumer guide` — `docs/reference/reading-a-mikebom-sbom.md` + `docs/index.md`

  Do NOT commit until T019 + T020 pass clean. Use `git add <specific paths>` (never `-A`). Each commit ends with the standard `Co-Authored-By` trailer.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 baseline; T002 doc scaffold. T002 must complete before any authoring task starts.
- **Phase 2 (Foundational)**: EMPTY — no foundational work required.
- **Phase 3 (US1)**: Depends on T002. T003 (opening positioning) + T004 (navigation map) are sequential since they set the doc's tone + reader expectations. T005–T008 [P] are the 4 cluster sections — different file regions, landable in any order or in parallel. T009–T012 are subsequent independent sections that can follow T005–T008.
- **Phase 4 (US2)**: Depends on Phase 3 — Section 7 cross-references Sections 4 + 5 + 6 which must exist first.
- **Phase 5 (Polish)**: Depends on US1 + US2 being functionally complete. Within Phase 5: T014 (appendix) + T015 [P] (milestone map) + T016 [P] (index.md) + T017 [P] (verify-recipes.sh) can land in parallel; T018 (verify recipes) requires T005–T008 + T017; T019 + T020 are the gate audits; T021 is the commit chain.

### User Story Dependencies

- **US1 (P1, MVP)**: Standalone after Phase 1. Delivers the consumer-onboarding surface. T002–T012 (~11 tasks). After this phase the doc can be reviewed for SC-001 read-through audit.
- **US2 (P2)**: Builds on US1 — T013 (Section 7) cross-references sections 4 + 5 + 6 from US1.

### Within Each User Story

- T003 → T004 → T005-T008 [P] → T009 → T010 → T011 → T012 (US1 sequential except for the 4 cluster sections which are [P]).
- T013 (US2) lands after T009 + T010 + T011 (which it cross-refs).

### Parallel Opportunities

- Phase 3: T005 + T006 + T007 + T008 [P] — 4 cluster sections, ~120 lines each, landable in parallel.
- Phase 5: T015 + T016 + T017 [P] — milestone-citation map + index.md update + verify-recipes.sh script (3 different files).

---

## Parallel Example: Phase 3 (after T002 + T003 + T004 land)

```bash
# Four cluster sections can be authored in parallel:
Task T005: §3.1 Vulnerability scanning   (3 signals × ~40 lines + intro = ~120 lines)
Task T006: §3.2 Compliance auditing      (3 signals × ~40 lines + intro = ~120 lines)
Task T007: §3.3 Build provenance         (3 signals × ~40 lines + intro = ~120 lines)
Task T008: §3.4 Transparency gaps        (3 signals × ~40 lines + intro = ~120 lines)
```

## Parallel Example: Phase 5

```bash
Task T015: Appendix B milestone-citation map  (specs/150-sbom-consumer-guide/... → docs/reference/...)
Task T016: docs/index.md update               (one-line addition)
Task T017: specs/150-sbom-consumer-guide/verify-recipes.sh   (authoring artifact)
```

---

## Implementation Strategy

### MVP First (US1 only — ships the consumer-onboarding surface)

1. Complete Phase 1: T001 baseline + T002 scaffold.
2. Complete Phase 3: T003 (opening) + T004 (nav) + T005-T008 (4 cluster sections, [P]) + T009 (envelope) + T010 (cross-format) + T011 (stability) + T012 (cross-refs).
3. **STOP and VALIDATE** with operator-cadence reviewer running the SC-001 5-question audit per `quickstart.md` Scenario 1.
4. This is a shippable PR. Appendix A + milestone-citation map + tool-author section can land in a follow-up PR if the operator-cadence review surfaces structural concerns.

### Incremental / Recommended (single-PR delivery)

1. Phase 1 (T001-T002) setup.
2. Phase 3 (T003-T012) US1 sections.
3. Phase 4 (T013) US2 tool-author section.
4. Phase 5 (T014-T021) polish: appendix + milestone map + index update + jq verification + pre-PR + commit chain.

Total: 21 tasks. Estimated ~600–900 lines of Markdown + 1-line `docs/index.md` update + ~50 lines of `verify-recipes.sh` script (authoring artifact, lives in spec dir).

### Single-developer Note

This milestone is bulk-authoring rather than chunked implementation. One developer can work through all phases in one session if focused; estimated 5–10 hours of authoring + 1–2 hours of audits + commit chain. The [P] markers signal "no cross-section content conflict" but the single-file deliverable means parallel-authoring is best done by drafting each cluster in a scratch file then merging.

---

## Notes

- The doc lives at `docs/reference/reading-a-mikebom-sbom.md`. Single file per spec Assumption 8.
- Per the 2026-06-29 clarification (Q1 Option D), the doc does NOT name specific competing SBOM tools — framing is consumer-centric ("here's what mikebom emits and how to use it") rather than competitive.
- Memory `feedback_prepr_gate_full_output.md` is directly relevant for T019: scan the FULL output rather than greping on `^test result: FAILED`.
- The commit-message convention (T021) follows the milestone-134/144/145/146/147/148/149 precedent: `spec(150):` / `plan(150):` / `tasks(150):` / `docs(150):` (note the 4th uses `docs(150)` rather than `impl(150)` since this is a docs-only milestone).
- Per spec FR-013 + Edge Case 6: every depth-covered signal MUST cite the milestone that introduced or stabilized it so consumers can map binary version → signal availability.
- Per spec FR-011 + SC-004: jq recipes MUST be verified runnable at doc-authoring time (T018). The verify-recipes.sh script from T017 makes this re-runnable post-merge if doubt arises.
- The appendix-coverage audit (T020 + SC-002) is mechanical — `diff /tmp/catalog-keys.txt /tmp/guide-keys.txt` per `quickstart.md` Scenario 2.
- This milestone is ENTIRELY about consumer-facing documentation. Zero Rust source code change. Zero CLI flag change. Zero wire-format change. The mikebom binary is unchanged.
