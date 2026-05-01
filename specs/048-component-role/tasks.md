---
description: "Task list — milestone 048 mikebom:component-role annotation"
---

# Tasks: `mikebom:component-role` annotation (build-tool / language-runtime / absent)

**Input**: spec.md ✅, plan.md ✅, checklists/requirements.md ✅. (No
research.md / data-model.md / contracts/ / quickstart.md — same
4-file tighter template milestones 021/022/023/042/046/047 use;
plan resolves the four small plan-level lookups inline in §Phase 0.)

**Tests**: included as inline unit tests on the classifier
(heuristic table coverage + glob matcher + three-state semantics)
and an inline pipeline test on the post-dedup hook. No new
end-to-end fixture needed — `holistic_parity` continues to pass
because no existing synthetic fixture has paths matching the
heuristic table (audit-verified).

**Organization**: Three user stories. US1 (build-tool, P1 MVP)
and US2 (language-runtime, P2) share the same classifier code
and post-dedup hook — they differ only in heuristic-table
entries. US3 (cross-format parity, P1 bundled with US1) is the
catalog row + parity-extractor wiring. Commit ordering: US1+US2
together (one commit, shared code), then US3, then CHANGELOG.

## Format: `[ID] [P?] [Story?] Description`

---

## Phase 1: Setup

- [ ] T001 Confirm clean working tree on branch `048-component-role`. `git status` shows only the un-tracked `specs/048-component-role/` scaffolding from `/speckit.specify` + the untracked `CLAUDE.md` from `/speckit.plan`.
- [ ] T002 `./scripts/pre-pr.sh` clean (baseline; should pass since no edits yet).

---

## Phase 2: Foundational (classifier + post-dedup hook — shared by US1 + US2)

- [ ] T003 Create new module `mikebom-cli/src/resolve/component_role.rs`. Add `pub enum ComponentRole { BuildTool, LanguageRuntime }` with `pub fn as_str(&self) -> &'static str` returning `"build-tool"` and `"language-runtime"` respectively. Derive `Debug, Clone, Copy, PartialEq, Eq`.
- [ ] T004 In `mikebom-cli/src/resolve/component_role.rs`, add the curated heuristic table as `const HEURISTIC_TABLE: &[(&str, ComponentRole)]` with all FR-002 + FR-005 entries: `("/usr/share/maven/lib/", BuildTool)`, `("/usr/share/gradle/lib/", BuildTool)`, `("/opt/sbt/", BuildTool)`, `("/usr/lib/jvm/*/lib/", LanguageRuntime)`, `("/usr/lib/node_modules/", LanguageRuntime)`, `("/usr/lib/python*/site-packages/", LanguageRuntime)`, `("/usr/lib/python*/dist-packages/", LanguageRuntime)`. Document the open-enum extensibility in a module-level doc-comment.
- [ ] T005 In `mikebom-cli/src/resolve/component_role.rs`, add `fn matches_pattern(pattern: &str, path: &str) -> bool` — the single-segment-glob matcher. Algorithm: split both `pattern` and `path` on `/`, walk segments in parallel; a literal pattern segment must match exactly; a pattern segment containing `*` matches any one path segment whose literal prefix and suffix match (e.g., `python*` matches `python3.11`, `*-debian` matches `foo-debian`, bare `*` matches anything). Pattern with trailing `/` (path-prefix mode): the path matches if all pattern segments match in order AND the path has at least as many segments as the pattern (allows arbitrary trailing path components, e.g., `/usr/share/maven/lib/foo/bar.jar` matches pattern `/usr/share/maven/lib/`).
- [ ] T006 In `mikebom-cli/src/resolve/component_role.rs`, add `pub fn classify(occurrences: &[FileOccurrence]) -> Option<ComponentRole>`. Iterates `occurrences` in their natural order, calls `matches_pattern` for each `(pattern, role)` in `HEURISTIC_TABLE` against each occurrence's `location`, returns the FIRST role hit. Returns `None` if no occurrence matches any heuristic.
- [ ] T007 Add `#[cfg(test)] mod tests` to `mikebom-cli/src/resolve/component_role.rs` covering: each heuristic-table entry hits an expected path (one positive test per row); paths NOT under any prefix return `None`; deeper paths under a matched prefix still classify (e.g., `/usr/share/maven/lib/sub/foo.jar`); single-segment glob patterns work for the JVM `*` and Python `*` cases (e.g., `/usr/lib/jvm/java-21-openjdk/lib/foo.jar` matches `LanguageRuntime`); multi-occurrence components with mixed paths return the first heuristic hit; empty occurrences slice returns `None`. Use the existing `mikebom_common::resolution::FileOccurrence` struct directly with the minimum fields populated.
- [ ] T008 Register the new module in `mikebom-cli/src/resolve/mod.rs`: add `pub mod component_role;` next to the existing `pub mod deduplicator;` etc.
- [ ] T009 Edit `mikebom-cli/src/resolve/deduplicator.rs` (or wherever `deduplicate()` returns the final `Vec<ResolvedComponent>`): after dedup, run a classifier pass:
    ```rust
    for component in &mut components {
        if let Some(role) = crate::resolve::component_role::classify(&component.occurrences) {
            component.extra_annotations.insert(
                "mikebom:component-role".to_string(),
                serde_json::Value::String(role.as_str().to_string()),
            );
        }
    }
    ```
    Place the loop inside `deduplicate` so every caller benefits, OR in a new `classify_component_roles(&mut Vec<ResolvedComponent>)` function called immediately after `deduplicate` from each call site (whichever is more idiomatic in `deduplicator.rs`'s current shape).
- [ ] T010 Add an inline integration test (in `deduplicator.rs::tests` or alongside the new function): construct a synthetic `ResolvedComponent` with `occurrences[0].location = "/usr/share/maven/lib/maven-artifact-3.1.0.jar"` and an empty `extra_annotations`, run the dedup-then-classify path, assert `extra_annotations.get("mikebom:component-role") == Some(json!("build-tool"))`. Also a negative case: `/app/lib/foo.jar` produces NO annotation.

---

## Phase 3: Commit `feat(048/us1+us2)` — wire emission for both build-tool and language-runtime

**Goal**: Bundle US1 (build-tool tagging) and US2 (language-runtime tagging) into one commit. They share the classifier code and heuristic table from Phase 2; this phase verifies the emission flows through `extra_annotations` → CDX `properties[]` + SPDX 2.3/3 annotations correctly.

**Independent test**: SC-001 (jq finds `mikebom:component-role = "build-tool"` on a synthetic fixture) + SC-002 (same for `language-runtime`).

- [ ] T011 [P] [US1] Verify FR-001 via inline unit test in `component_role.rs::tests`: a component with `occurrences[0].location = "/usr/share/maven/lib/maven-artifact-3.1.0.jar"` classifies as `Some(ComponentRole::BuildTool)`. Already covered by T007's table-iteration test; this task confirms the BuildTool case is in the matrix explicitly.
- [ ] T012 [P] [US2] Verify FR-004 via inline unit test in `component_role.rs::tests`: a component with `occurrences[0].location = "/usr/lib/jvm/java-21-openjdk/lib/jrt-fs.jar"` classifies as `Some(ComponentRole::LanguageRuntime)`. Already covered by T007; explicit confirmation.
- [ ] T013 [P] [US1] [US2] Verify FR-003 via inline unit test in `component_role.rs::tests`: a component with `occurrences[0].location = "/app/lib/foo.jar"` classifies as `None` (three-state semantics: absence ≠ application).
- [ ] T014 [US1] Verify the post-dedup hook end-to-end: `cargo +stable test -p mikebom -- resolve::component_role resolve::deduplicator` passes including the new tests.
- [ ] T015 [US1] [US2] Verify FR-012 (goldens stay byte-identical when fixtures don't match heuristics): `cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression` all pass without regen. (No existing fixture has heuristic-matched paths per audit; this confirms.)
- [ ] T016 [US1] [US2] `./scripts/pre-pr.sh` clean.
- [ ] T017 [US1] [US2] Commit: `feat(048/us1+us2): mikebom:component-role classifier with build-tool + language-runtime path heuristics`.

---

## Phase 4: Commit `feat(048/us3)` — catalog row + parity-extractor wiring

**Goal**: Per FR-006/FR-007/FR-008/FR-009, document the new annotation in the catalog and register parity extractors so `holistic_parity` SymmetricEqual covers it across CDX + SPDX 2.3 + SPDX 3.

**Independent test**: SC-003 (`holistic_parity` 11/11 ok with new C40 in matrix) + SC-004 (`every_catalog_row_has_an_extractor` passes with C40 registered for all three formats).

- [ ] T018 [US3] Edit `docs/reference/sbom-format-mapping.md`: add a new C40 row after C39. Description names: (a) annotation = `mikebom:component-role` with values `build-tool` / `language-runtime` / absent (three-state, open-enum), (b) classification mechanism (filesystem-position heuristic), (c) cross-format placement (CDX `properties[]`, SPDX 2.3 `packages[].annotations[]`, SPDX 3 top-level `annotations[]`), (d) `Present` × 3 formats × `SymmetricEqual`.
- [ ] T019 [P] [US3] Edit `mikebom-cli/src/parity/extractors/cdx.rs`: add one line near the existing C-row extractors (around line 337 where C14 lives): `cdx_anno!(c40_cdx, "mikebom:component-role", component);`.
- [ ] T020 [P] [US3] Edit `mikebom-cli/src/parity/extractors/spdx2.rs`: mirror invocation: `spdx23_anno!(c40_spdx23, "mikebom:component-role", component);` near the existing C14 row.
- [ ] T021 [P] [US3] Edit `mikebom-cli/src/parity/extractors/spdx3.rs`: mirror invocation: `spdx3_anno!(c40_spdx3, "mikebom:component-role", component);` near the existing C14 row.
- [ ] T022 [US3] Edit `mikebom-cli/src/parity/extractors/mod.rs`: add a new `ParityExtractor` row to the `EXTRACTORS` table:
    ```rust
    ParityExtractor {
        row_id: "C40",
        label: "mikebom:component-role",
        cdx: c40_cdx,
        spdx23: c40_spdx23,
        spdx3: c40_spdx3,
        directional: SymmetricEqual,
    }
    ```
    Add the three new fn imports to the existing `use cdx::{...}`, `use spdx2::{...}`, `use spdx3::{...}` blocks at the top of `mod.rs`.
- [ ] T023 [US3] Verify SC-003: `cargo +stable test -p mikebom --test holistic_parity` passes 11/11 with C40 participating.
- [ ] T024 [US3] Verify SC-004: `cargo +stable test -p mikebom --test sbom_format_mapping_coverage` (or equivalent — the test that asserts every catalog row has an extractor for all three formats) passes with C40 registered.
- [ ] T025 [US3] `./scripts/pre-pr.sh` clean.
- [ ] T026 [US3] Commit: `feat(048/us3): catalog row C40 + parity extractors for mikebom:component-role`.

---

## Phase 5: Commit `chore(048)` — CHANGELOG + spec scaffolding

- [ ] T027 Edit `CHANGELOG.md` under `[Unreleased]` → `### Added`: add an entry naming the new `mikebom:component-role` annotation, what it tags (filesystem-position-classified components in `/usr/share/maven/lib/`, `/usr/lib/jvm/*/lib/`, etc.), the three-state semantics (`build-tool`, `language-runtime`, absent — absence ≠ application), and the cross-format parity (CDX property + SPDX 2.3/3 annotations).
- [ ] T028 Stage `specs/048-component-role/` (spec.md, plan.md, tasks.md, checklists/requirements.md) and `CLAUDE.md` (auto-updated by `update-agent-context.sh`).
- [ ] T029 `./scripts/pre-pr.sh` clean.
- [ ] T030 Commit: `chore(048): CHANGELOG entry + speckit spec/plan/tasks scaffolding`.

---

## Phase 6: Polish & PR

- [ ] T031 Verify SC-005 (zero diff on existing 27 byte-identity goldens): `git diff main..HEAD --stat -- mikebom-cli/tests/fixtures/golden/` is empty. If non-empty, audit why a fixture's path matched the heuristic — accept-and-document if legitimate, fix-classifier if false positive.
- [ ] T032 Verify SC-006 (pre-PR + CI green): final `./scripts/pre-pr.sh` clean from a fresh shell.
- [ ] T033 Push branch: `git push -u origin 048-component-role`.
- [ ] T034 Open PR titled `feat(048): mikebom:component-role annotation (build-tool / language-runtime / absent)`. Body covers: 3-commit summary, audit-grounded rationale (the polyglot-builder-image FP friction), 7 SC verification commands, out-of-scope reminders.
- [ ] T035 Verify SC-006 (CI lanes): all 3 CI lanes (linux x86_64, linux ebpf, macos-latest) green on the PR.

---

## Dependency graph

```text
T001-T002 (setup, baseline)
   │
   ▼
T003-T010 (foundational: classifier module + heuristic table +
           single-segment-glob matcher + classify() fn + post-dedup
           hook + inline tests — required by both US1 and US2)
   │
   ▼
T011-T017  [Commit 1: Phase 3 — US1+US2 verification + commit]
   │
   ▼
T018-T026  [Commit 2: Phase 4 — US3 catalog + parity wiring]
   │
   ▼
T027-T030  [Commit 3: Phase 5 — CHANGELOG + scaffolding]
   │
   ▼
T031-T035 (verify + push + PR)
```

**Why US1 and US2 share a phase/commit**: they're built from the
same classifier module + same heuristic table + same post-dedup
hook. The only per-story distinction is which heuristic-table
entries they bring (build-tool vs language-runtime). Splitting
into separate commits would require shipping a half-populated
table in commit 1 and adding to it in commit 2, which is awkward
and reviewer-hostile. One commit covering both, with story
labels on tests, is the honest representation.

**Why US3 is a separate commit**: per FR-006/FR-007/FR-008/FR-009
the catalog row and parity-extractor wiring are independent of
the classifier code. Splitting lets reviewers see "code that
emits the annotation" separately from "code that asserts cross-
format parity on it" without bundling.

## Parallel opportunities

| Bucket | Parallel-eligible tasks |
|---|---|
| Phase 2 | T003 + T004 + T005 + T006 (same file → sequential) || T007 (same file → sequential) || T008 (different file) || T009 + T010 (different file but same edit thread) |
| Commit 1 verification | T011 + T012 + T013 (independent unit tests in same file → sequential within commit but logically parallel) |
| Commit 2 wiring | T019 + T020 + T021 (three different files — fully parallel) |
| Commit 3 | T027 (CHANGELOG) + T028 (staging only — no edit) — fully parallel |

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Phase 1 (setup) | 5 min | Just baseline |
| Phase 2 (foundational) | 1.5 hr | New module + matcher + hook + ~50 LOC of unit tests |
| Phase 3 (US1+US2 verification) | 15 min | Re-run tests + commit |
| Phase 4 (US3 catalog + parity) | 30 min | Mechanical — 4 single-line additions + 1 catalog row + 1 EXTRACTORS row |
| Phase 5 (CHANGELOG + scaffolding) | 10 min | Mechanical |
| Phase 6 (verify + PR) | 15 min | Push + CI watch |
| **Total** | **~2.5 hr** | One focused session |

## MVP scope

**The MVP is US1+US3 bundled — the build-tool tagging + the
cross-format parity wiring.** US2 (language-runtime) is a
strict extension of the same mechanism with additional
heuristic-table entries; it ships in the same commits as
US1 because the code is shared.

If implementation surfaces a reason to split (e.g.,
language-runtime heuristics turn out to need a different
mechanism), the table extension can drop to a follow-on PR
without breaking US1's emission. Today, both ship together.

Conformance-suite consumption (the polyglot fixture's GT
declaring these 3 jars with `severity: advisory` keyed on
the annotation) is OUT of scope per spec — it ships in the
sbom-conformance repo as a follow-on once this milestone
merges.
