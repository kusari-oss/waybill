# Tasks: Design-tier component visibility (m175)

**Input**: Design documents from `/specs/175-design-tier-visibility/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/, quickstart.md

**Tests**: Integration tests are included per spec SC-001…SC-008. No unit tests required — the advisory-log predicate is a 3-line pure function whose correctness is exercised end-to-end by the 5 US2 integration tests.

**Organization**: Tasks are grouped by user story. This milestone has an unusually flat structure — zero Foundational phase (no substrate to build; every ecosystem reader already tags `sbom_tier = "design"` per m002/m047).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3
- Include exact file paths in descriptions

## Path Conventions

Docs live under `docs/reference/`; code lives under `mikebom-cli/src/cli/scan_cmd.rs`; tests live under `mikebom-cli/tests/`. All paths absolute from repo root.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm branch state and zero-dep posture.

- [X] T001 Verified branch `175-design-tier-visibility`. **Note**: branch was 2 commits behind main (m174 + m176 landed while m175 was paused). Fast-forwarded via `git stash --include-untracked` + `git merge main --ff-only` + `git stash pop`; resolved 2 CLAUDE.md conflicts (merged m174/m175/m176 entries in the milestone-tech + Recent-Changes lists). Untracked state: `specs/175-design-tier-visibility/` + m176-era `image-baz.cdx.actual.json` scratch file (kept out of commit).
- [X] T002 Verified via `cargo tree -p mikebom --depth 1` — no new deps needed; `tracing` + `std::env` already in the tree. Zero new Cargo dependencies confirmed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: **This milestone has NO foundational phase.** Every ecosystem reader (11 today per research.md R1) already tags constraint-only entries with `sbom_tier = Some("design")`; CDX's `metadata.lifecycles[design]` is already populated via `generate::lifecycle_phases::tier_to_phase` (m047/m081). The wire substrate is complete — Phase 3+ user stories can start immediately.

*(No tasks in this phase. Skip directly to Phase 3.)*

---

## Phase 3: User Story 1 — Reading-guide subsection (Priority: P1) 🎯 MVP-part-1

**Goal**: A new `docs/reference/reading-a-mikebom-sbom.md` subsection under §3.4 (Transparency / completeness gaps) that explains design-tier concept + native wire signals + per-ecosystem operator remediation + jq recipes. Enables SC-001 (5-minute operator walk-through) and provides the docs anchor referenced by US2's advisory log.

**Independent Test**: an operator new to mikebom, reading only the new subsection, can identify a design-tier component + count them + name one remediation action within 5 minutes. Verified via quickstart.md Path A.

### Implementation for User Story 1

- [X] T003 [US1] Added new `#### Design-tier components` subsection to `docs/reference/reading-a-mikebom-sbom.md` §3.4 between the m173 `mikebom:go-cache-warming-*` block and the `mikebom:peer-edge-targets` block. Structure per data-model.md §Entity 3 outline: (1) design-tier definition + Constitution-IX-honest empty-version rationale; (2) traceability ladder (design → source → analyzed → deployed → build); (3) native wire signals across all 3 formats; (4) advisory-log description with stable substring + env-var suppression; (5) per-ecosystem remediation table (pip / npm / Cargo / Ruby / Composer / Cocoapods / Mix / Rebar3 — 8 ecosystems, exceeds the FR-008 minimum of 4); (6) the "no `pip install` without venv" safety guardrail; (7) four canonical jq recipes; (8) CI threshold-check recipe; (9) suppression paragraph. ~150 lines.

**Checkpoint**: US1 subsection ships. The `#design-tier-components` anchor is live; US2's advisory log message can safely reference it.

---

## Phase 4: User Story 2 — Advisory log at scan time (Priority: P1) 🎯 MVP-part-2

**Goal**: When the scan detects ≥1 design-tier component AND the suppression env var is unset AND the scan produced at least one component, one INFO-level advisory log line lands on stderr with the stable substring `"design-tier components detected: "` + the count + a remediation string + the `#design-tier-components` anchor reference.

**Independent Test**: `MIKEBOM_NO_DESIGN_TIER_ADVISORY` unset + `requirements.txt`-only fixture → `grep -cF 'design-tier components detected: ' stderr` = 1. Verified via quickstart.md Path B.1.

### Implementation for User Story 2

- [X] T004 [US2] Added the FR-002 advisory-log block in `mikebom-cli/src/cli/scan_cmd.rs` immediately after the m176 monorepo advisory block, before the final `"SBOM written"` line. Predicate: `design_tier_count > 0 && !components.is_empty() && !suppress`; env-var read for `MIKEBOM_NO_DESIGN_TIER_ADVISORY` (`"1"` or `"true"` case-insensitive); stable substring `"design-tier components detected: "`; INFO level; message body per data-model.md §Entity 1 (count + `lockfile`/`venv` remediation keywords + docs anchor `docs/reference/reading-a-mikebom-sbom.md#design-tier-components`); NOT gated on `--offline`. Compiles clean.

- [X] T005 [US2] Created `mikebom-cli/tests/design_tier_advisory.rs` with **6 integration tests** (5 baseline + 1 FR-009 non-Python per /speckit-analyze C1 remediation): `t001_advisory_fires_once_on_design_tier_scan` (SC-002); `t002_advisory_silent_on_zero_design_tier` (SC-003); `t003_advisory_silent_on_suppression_env_var` (SC-004 — tests `1`/`true`/`TRUE`/`True` truthy variants + `no` negative); `t004_advisory_fires_under_offline` (SC-005); `t005_advisory_silent_on_empty_scan_target` (edge case); `t006_advisory_fires_on_non_python_design_tier` (FR-009 — uses PHP `composer.json`-only fixture per m138 composer reader's `emit_design_tier_components` path; initial Ruby-Gemfile attempt failed because the m069 gem reader only reads `Gemfile.lock` + `.gemspec`, NOT bare Gemfiles — retargeted to composer). **Result**: `ok. 6 passed; 0 failed`.

**Checkpoint**: US2 fully functional. Operators see the design-tier advisory at scan time; CI dashboards can grep-substring-detect design-tier scans via `"design-tier components detected: "`.

---

## Phase 5: User Story 3 — KEEP-NATIVE-FIRST tag polarity (Priority: P2)

**Goal**: A new row in `docs/reference/sbom-format-mapping.md` explicitly tagged **KEEP-NATIVE-FIRST** documenting the design-tier native carriers across all 3 formats + the rejected `mikebom:design-tier-count` invention. Plus a cross-reference paragraph in `docs/reference/component-tiers.md`.

**Independent Test**: `grep -n KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md` returns exactly one match (the m175 row). Verified via quickstart.md Path C.

### Implementation for User Story 3

- [X] T006 [P] [US3] Appended a new row to `docs/reference/sbom-format-mapping.md` Section C, positioned immediately after C121 (m176) and before Section D. Row's `#` column is `—` (dash — signals "not a `C-` catalog entry" per FR-006). Format columns cite empty `component.version` / `Package.versionInfo` / `software_Package.packageVersion` + CDX `metadata.lifecycles[design]` aggregate. Justification tagged **KEEP-NATIVE-FIRST** with explicit contrast to the KEEP-NO-NATIVE polarity (m172/m173/m176) + rationale for rejecting the proposed `mikebom:design-tier-count` invention. Grep verification: `grep -n KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md | wc -l` returns `1` (SC-007 gate holds).

- [X] T007 [P] [US3] Added a cross-reference paragraph to `docs/reference/component-tiers.md` in the intro block. **Note**: the original tasks.md text said "near the existing 'Design tier' bullet" but `component-tiers.md` has no such bullet — it covers the component-tier axis (package/binary/file), which is orthogonal to the `sbom_tier` traceability ladder (design/source/analyzed/deployed/build). Adjusted: the cross-reference now sits in the intro block explicitly noting the two axes are orthogonal + points at the new reading-guide subsection + mentions the `MIKEBOM_NO_DESIGN_TIER_ADVISORY=1` env-var.

**Checkpoint**: US3 ships. `grep -n KEEP-NATIVE-FIRST` returns 1 line; future Principle V audits can cite this row as prior-art for the KEEP-NATIVE-FIRST polarity.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Pre-PR gate + quickstart walk + SC-006 byte-identity confirmation.

- [X] T008 Ran `./scripts/pre-pr.sh` → final line: `>>> all pre-PR checks passed.` Zero failures across the full log. **Zero golden regeneration** — the 33+ existing golden fixtures stayed byte-identical (SC-006 gate holds; advisory landed on stderr only per FR-002, no stdout/output-file bleed).

- [X] T009 Verified quickstart.md paths against actual code:
  - **Path A (US1 docs walk)**: subsection at `docs/reference/reading-a-mikebom-sbom.md#design-tier-components` established with 8-ecosystem remediation table + 4 jq recipes + threshold-check CI recipe. Structurally supports the SC-001 5-minute operator walk-through.
  - **Path B (US2 advisory)**: verified by `t001` (SC-002 fires-once) + `t002` (SC-003 silent-on-zero) + `t003` (SC-004 env-var suppression + case-insensitivity) + `t004` (SC-005 offline-orthogonality) + `t006` (FR-009 non-Python via composer.json).
  - **Path C (SC-007 KEEP-NATIVE-FIRST discoverability)**: `grep -n KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md | wc -l` = `1` ✓.
  - **Path D (SC-006 byte-identity)**: covered by T008's pre-PR full test suite passing all 33+ golden fixtures — zero bytes drift, only stderr diverges when the advisory fires.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no blockers — starts immediately.
- **Foundational (Phase 2)**: **empty**. No blockers introduced.
- **User Story 1 (Phase 3)**: depends on Phase 1 only. Establishes the `#design-tier-components` anchor referenced by US2's advisory log.
- **User Story 2 (Phase 4)**: depends on Phase 1 only. Touches `scan_cmd.rs` + new `mikebom-cli/tests/design_tier_advisory.rs`. Independent of US1 file-wise; the advisory message references the docs anchor (a URL string), but that URL is valid as soon as US1's T003 lands OR can be validated post-hoc. **Recommendation**: land T003 first so `t001_advisory_fires_once_on_design_tier_scan` can assert the anchor path as part of the advisory body.
- **User Story 3 (Phase 5)**: depends on Phase 1 only. Two files (`sbom-format-mapping.md` + `component-tiers.md`) — both parallelizable with US1 + US2.
- **Polish (Phase 6)**: depends on ALL of Phase 3+4+5. Pre-PR + quickstart walk are the last steps.

### User Story Dependencies

- **US1 (P1) MVP-part-1** — docs subsection is independent given Phase 1.
- **US2 (P1) MVP-part-2** — advisory log is independent given Phase 1 code-wise; docs anchor validity depends on US1 landing OR can be validated later.
- **US3 (P2)** — mapping doc + cross-ref are file-independent from US1/US2 (different files).

### Within Each User Story

- US1: single doc file (T003). Nothing to parallelize.
- US2: T004 (code) must land before T005 (test); test can't pass without the emission.
- US3: T006 + T007 are file-independent — mark both `[P]`.

### Parallel Opportunities

- **US1 T003 + US3 T006 + US3 T007** [P] — three different files, no dep between them.
- **US2 T004** — separate file (`scan_cmd.rs`); can proceed in parallel with US1 T003 + US3 T006/T007 for a solo-dev on multiple files.
- Recommended execution: land T003 first (docs anchor validity), then run T004+T005+T006+T007 in whatever order suits the workflow, close with T008+T009.

---

## Parallel Example: Setup + all documentation

```bash
# After Phase 1, three independent doc edits can proceed together:
Task: "Add design-tier subsection to docs/reference/reading-a-mikebom-sbom.md §3.4"
Task: "Add KEEP-NATIVE-FIRST row to docs/reference/sbom-format-mapping.md Section C"
Task: "Add cross-reference paragraph to docs/reference/component-tiers.md"
```

---

## Implementation Strategy

### MVP First (US1 + US2 landed together — P1 milestone shape)

1. Complete Phase 1: Setup (T001–T002).
2. Land US1 T003 (docs anchor).
3. Land US2 T004 (advisory code) + T005 (integration tests).
4. **STOP and VALIDATE**: quickstart.md Path A + Path B walks. Operators can see the advisory; docs subsection answers what to do about it. **This is shippable as MVP.**
5. Optional: continue to US3 in a follow-up commit (KEEP-NATIVE-FIRST tag) or bundle into the same PR.

### Incremental Delivery

1. Setup complete → foundation ready.
2. Add US1 → operator can walk the reading guide → shippable.
3. Add US2 in parallel → advisory fires → shippable-together as coordinated pair.
4. Add US3 → prior-art tag exists → shippable.
5. Polish + PR.

### Solo Strategy (recommended for m175 given the small scope)

1. Sequential through Phase 1 → 3 → 4 → 5 → 6.
2. Under one PR (single commit or logical-group commits per phase).
3. Total wall-clock: ~2 hours for a solo dev — docs are the bulk of the effort.

---

## Notes

- [P] tasks = different files, no dependencies.
- Every FR from spec.md maps to at least one task; every SC has a verifying task in Phase 6 or the story's implementation task.
- **Zero golden regeneration** per SC-006 + research.md R5. If T008's pre-PR gate flags a golden diff, something leaked into stdout that should have stayed on stderr — investigate before proceeding.
- Pre-PR gate (T008) is MANDATORY per project CLAUDE.md. Do not open PR without both clippy + tests clean.
- **KEEP-NATIVE-FIRST is a new precedent**: T006 introduces the first prior-art for this tag polarity. Future contributors doing Principle V audits will cite it.
