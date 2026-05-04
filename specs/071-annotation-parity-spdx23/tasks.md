---
description: "Task list for milestone 071 — cross-format SBOM annotation parity (close SPDX 2.3 emitter gap; document inherent format-only fields)"
---

# Tasks: Cross-format SBOM annotation parity

**Input**: Design documents from `/specs/071-annotation-parity-spdx23/`
**Prerequisites**: spec.md ✅, plan.md ✅, research.md ✅, data-model.md ✅, contracts/parity-catalog-row.md ✅, quickstart.md ✅

## Format: `[ID] [P?] [Story?] Description`

## Phase 1: Setup

- [X] T001 Confirm working tree clean and on branch `071-annotation-parity-spdx23`. Confirm `cargo +stable test --workspace` passes baseline before any edits (so any new failure is attributable to this milestone).

## Phase 2: Foundational (blocking prerequisites for all user stories)

- [X] T002 Add `pub order_sensitive: bool` field to the `ParityExtractor` struct at `mikebom-cli/src/parity/extractors/common.rs:31`. Default = `false`. Update every existing `ParityExtractor { ... }` literal across `mikebom-cli/src/parity/extractors/mod.rs` (68 rows) to include the field — use a small sed/text-replacement pass so each row gains `order_sensitive: false,` before the closing `}`. Verify `cargo +stable check -p mikebom` builds clean.
- [X] T003 Add `canonicalize_for_compare(value: &serde_json::Value, order_sensitive: bool) -> String` helper to `mikebom-cli/src/parity/extractors/common.rs`. Algorithm per contract C-3: recursively sort object keys lex; recursively sort arrays lex when `!order_sensitive`; serialize via `serde_json::to_string` (compact, no pretty). Add a unit test in `common.rs` covering: (a) nested objects, (b) mixed array/object, (c) the `order_sensitive=true` path preserving array order, (d) equal-after-canonicalization for two structurally-different-but-semantically-equivalent inputs, (e) **empty/null/absent equivalence** — assert that an empty array `[]`, an empty string `""`, JSON `null`, and an absent key all canonicalize to a representation that compares equal under SymmetricEqual via the `BTreeSet<String>` extractor outputs (this is the spec.md edge case "Empty / null / absent: which one is the canonical 'absent' representation?").

## Phase 3: User Story 1 — Symmetric `mikebom:*` emission across all three formats (P1) 🎯 MVP

### Implementation — fix the 5 SPDX 2.3 emission-guard bugs

- [ ] T004 [P] [US1] Audit `mikebom:source-files` SPDX 2.3 emission. Read `mikebom-cli/src/generate/spdx/annotations.rs:208`-area where the source-files key is pushed through `MikebomAnnotationCommentV1`. Confirm the call is reached for every component whose `c.source_files` is non-empty (i.e., not gated behind a narrow `extra_annotations`-only conditional). Fix any over-narrow guard. Add a unit test in `annotations.rs` `mod tests` block (gated by `#[cfg_attr(test, allow(clippy::unwrap_used))]`) that constructs a `ResolvedComponent` with `source_files = vec!["go.sum"]` and asserts the produced JSON contains an annotation with `field: "mikebom:source-files"` carrying the array.
- [ ] T005 [P] [US1] Audit `mikebom:sbom-tier` SPDX 2.3 emission. Read `mikebom-cli/src/generate/spdx/annotations.rs:142`-area. Same pattern as T004 — confirm the push is reached for every component with a populated `sbom_tier`. Fix guard. Add unit test asserting the field appears in the per-Package annotations array for a component with `sbom_tier = Some("source")`.
- [ ] T006 [P] [US1] Audit `mikebom:deps-dev-match` SPDX 2.3 emission. Read `mikebom-cli/src/generate/spdx/annotations.rs:132`-area. Same pattern. Fix guard. Add unit test asserting the field appears for a component with `deps_dev_match = Some("npm:foo@1.2.3")`.
- [ ] T007 [P] [US1] Audit `mikebom:npm-role` SPDX 2.3 emission. Read `mikebom-cli/src/generate/spdx/annotations.rs:164`-area. Same pattern. Fix guard. Add unit test asserting the field appears for a component with `npm_role = Some("dependencies")`.
- [ ] T008 [P] [US1] Audit `mikebom:cpe-candidates` SPDX 2.3 emission. Read `mikebom-cli/src/generate/spdx/annotations.rs:217`-area. Same pattern. Fix guard. Add unit test asserting the field appears as a JSON array (NOT comma-joined) for a component with `cpe_candidates = vec!["cpe:2.3:a:foo:bar:1.0:*:*:*:*:*:*:*"]`.
- [ ] T009 [US1] Promote catalog row C19 (`mikebom:cpe-candidates`) from `Directionality::PresenceOnly` to `Directionality::SymmetricEqual` in `mikebom-cli/src/parity/extractors/mod.rs:127`. **Do NOT change CDX emission shape.** CDX 1.6 schema requires `properties[].value` to be a string, so `mikebom-cli/src/generate/cyclonedx/builder.rs:381-384` correctly emits the `" | "`-joined form; SPDX 2.3 + SPDX 3 already emit `json!(c.cpes)` (JSON array) per `spdx/annotations.rs:217` and `spdx/v3_annotations.rs:233`. To reconcile shapes at the *parity layer* (per FR-007), update `c19_cdx` in `mikebom-cli/src/parity/extractors/cdx.rs` to split the `" | "`-joined string into individual entries when populating the returned `BTreeSet<String>`. The SPDX-side extractors already walk the JSON array element-by-element. After both sides return per-CPE set entries, the `SymmetricEqual` invariant holds. Update the C19 inline rationale comment to: `// SymmetricEqual: CDX 1.6 schema forces properties[].value to be a string, so the CDX emitter pipe-joins the candidate list. The c19_cdx extractor splits on " | " to canonicalize at parity-layer; SPDX sides return per-element sets natively. Standards-native: A12 CPE primary on metadata.component / Package.externalRef.`

### SPDX 3 audit (likely no changes — confirms the user's data point)

- [ ] T010 [US1] Audit SPDX 3 emission for the same 5 keys at `mikebom-cli/src/generate/spdx/v3_annotations.rs:150`-area (deps-dev-match), `:160` (sbom-tier), and analogous lines for source-files / cpe-candidates / npm-role. Per research §7, SPDX 3 is in lockstep with CDX so this is a confirmation pass — expect zero code changes. Document the audit conclusion in a one-line code comment at the top of `v3_annotations.rs` ("Audited 071-annotation-parity-spdx23: SPDX 3 emission for the 6 alpha.13-CFI keys is correct as-is.").

### Wire parity extractors

- [ ] T011 [P] [US1] Update `mikebom-cli/src/parity/extractors/spdx2.rs` so each per-key extractor (`c3_spdx23`, `c5_spdx23`, `c9_spdx23`, `c18_spdx23`, `c19_spdx23`) returns a `BTreeSet<String>` populated by decoding the `MikebomAnnotationCommentV1` envelope via the existing `extract_mikebom_annotation_values()` helper in `common.rs`. If the existing extractors already do this, verify they correctly return non-empty sets after T004-T008's emission fixes by adding a unit test that runs each extractor against a synthesized SPDX 2.3 doc carrying the envelope.
- [ ] T012 [P] [US1] Update `mikebom-cli/src/parity/extractors/cdx.rs` for the cpe-candidates extractor (`c19_cdx`) per T009: split the CDX `properties[].value` (a `" | "`-joined string) into individual entries when populating the returned `BTreeSet<String>`. CDX emission shape is unchanged. Add a unit test against a synthesized CDX doc with two CPE candidates asserting the extractor returns 2 set entries (not 1 pipe-joined string).

## Phase 4: User Story 2 — Conformance harness CFI count drops ≥95% (P1)

### Verification (no separate code changes — emerges from US1)

- [ ] T013 [US2] Regenerate the 27 byte-identity goldens to capture the new SPDX 2.3 emission shape (must run BEFORE T014's parity-check, which reads from these golden files): `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression`, `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test -p mikebom --test spdx_regression`, `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test spdx3_regression`. CDX goldens should be near-zero diff (T009 doesn't change CDX emission; only the parity extractor's split logic differs); SPDX 3 should also be near-zero (already in lockstep with CDX); SPDX 2.3 goldens will have substantial NEW annotation entries reflecting the closed emission-guard gaps from T004-T008. **Verification gate (per analyze C2)**: spot-check at least 3 ecosystems' SPDX 2.3 golden diffs (suggested: gem, maven, golang per the high pre-fix CFI counts) and confirm the diff is *additive only* — no pre-existing annotation channel disappears. If any pre-existing SPDX 2.3 annotation row is removed in the diff, halt and investigate the regression before continuing to T014.
- [ ] T014 [US2] Run the existing parity-check subcommand against the post-T013-regenerated goldens to confirm SymmetricEqual invariants hold post-US1: `cargo +stable run -p mikebom -- parity-check mikebom-cli/tests/fixtures/golden/cyclonedx/<eco>.cdx.json mikebom-cli/tests/fixtures/golden/spdx-2.3/<eco>.spdx.json mikebom-cli/tests/fixtures/golden/spdx-3/<eco>.spdx3.json` for each of the 9 ecosystems (apk, cargo, deb, gem, golang, maven, npm, pip, rpm). Document the per-ecosystem before/after CFI counts in a comment on the milestone PR. If any SymmetricEqual row fails, return to US1 to find the missing emission path.

## Phase 5: User Story 3 — Inherent asymmetries catalogued and documented (P2)

### Code-side rationale comments (FR-004)

- [ ] T015 [P] [US3] Audit every non-`SymmetricEqual` row in `mikebom-cli/src/parity/extractors/mod.rs` (current set per research §8: A12, B4, C19 [will be SymmetricEqual after T009], C22, C42, D1, E1) and ensure each carries an inline Rust line-comment matching the `CatalogRowRationale` template from `data-model.md`: `// <Directionality>: <one-line rationale>. Standards-native: <pointer>.` Add the comment if missing; tighten the wording if vague. Particular attention to C22 `mikebom:os-release-missing-fields` per research §8 — confirm the rationale is recorded.

### Doc-side rationale publication (FR-005)

- [ ] T016 [US3] Add a new section "Cross-format annotation parity catalog" to `docs/reference/sbom-format-mapping.md`. Use a markdown table with columns: `row_id | label | Directionality | Rationale | Standards-native superseding construct`. Include every non-`SymmetricEqual` catalog row from T015's audit. Cite the milestone (071) at the top of the section. Pin the C42 lifecycle-scope row prominently as the canonical example since it's named in Constitution Principle V.
- [ ] T017 [US3] Add a doc-sync test at `mikebom-cli/tests/parity_doc_sync.rs` that:
  1. Reads the `parity/extractors/mod.rs` source file.
  2. For every non-`SymmetricEqual` row, extracts the row_id and the inline rationale comment.
  3. Reads `docs/reference/sbom-format-mapping.md` and asserts every (row_id, rationale) pair is present in the parity-catalog section.
  Failure message names the missing row_id and the file path the entry should be added to. This satisfies contract C-5.

## Phase 6: User Story 4 — Pre-PR gate guardrail prevents future drift (P2)

- [ ] T018 [US4] Create `mikebom-cli/tests/parity_completeness.rs`. The test:
  1. Loads each of the 9 ecosystem-fixture triples (CDX/SPDX 2.3/SPDX 3) from `mikebom-cli/tests/fixtures/golden/`.
  2. For each format, walks `components[]`/`packages[]`/`@graph[Package]` and collects every literal `mikebom:*` key emitted on a component-level construct (deliberately ignores document-level `metadata.properties`/document `annotations[]` per contract C-7).
  3. For each collected key, asserts it has a `ParityExtractor` row in `mod.rs` whose `label` matches.
  4. For each row whose `directional == SymmetricEqual`, runs the canonicalization (T003) over per-format extracted values and asserts equality.
  5. For each row whose `directional` is `CdxSubsetOfSpdx` / `PresenceOnly` / `CdxOnly`, asserts the per-variant invariant from contract C-2.
  6. Failure messages match the format in contract C-4.
  Test name: `parity_completeness_27_fixtures` (so the name appears verbatim in the cargo test output and in operator messages).
- [ ] T019 [US4] Create `mikebom-cli/tests/parity_synthetic_drift.rs`. The test:
  1. Synthesizes a minimal CDX 1.6 JSON document with one component carrying a `mikebom:foo-experimental` property.
  2. Synthesizes a minimal SPDX 2.3 + SPDX 3 document with the same component but NO `mikebom:foo-experimental` annotation.
  3. Invokes the same completeness-check function used by `parity_completeness.rs` (factor it into a small public-in-crate helper if needed) and asserts the result is `Err`.
  4. Asserts the error message contains the substrings: `"uncatalogued mikebom:* key"`, `"mikebom:foo-experimental"`, `"emitted-by:"`, `"[cdx]"`.
  This is the proof-of-failure regression test for contract C-4. Test name: `synthetic_cdx_only_drift_is_rejected`.
- [ ] T020 [US4] Confirm `./scripts/pre-pr.sh` invokes the new tests automatically. Run the script end-to-end and grep the output for `parity_completeness_27_fixtures ... ok` and `synthetic_cdx_only_drift_is_rejected ... ok`. No script changes expected; if either name doesn't appear, debug why cargo isn't picking up the new integration test files.

## Phase 7: Polish

- [ ] T021 CHANGELOG.md `[Unreleased]` entry for milestone 071. Note: this is the first non-emission-shape milestone post-alpha.13 — most of the work is parity-extractor and doc layer. Reference the SC-001 ≥95% reduction target and the canonicalization rule (Q2 default-with-override). Mention that the milestone strengthens Constitution Principle V's enforcement at the cross-format-parity layer.
- [ ] T022 Update `docs/design-notes.md` with a new section "Cross-format annotation parity (milestone 071)" pointing to the new `docs/reference/sbom-format-mapping.md` "Cross-format annotation parity catalog" section and explaining the operator-visible behavior: where to look when adding a new annotation key, what the pre-PR gate test reports, and how external conformance harnesses can consume the published catalog to filter intentional asymmetries.
- [ ] T023 Run `./scripts/pre-pr.sh` end-to-end and confirm clippy clean + every test target reports `ok. N passed; 0 failed` (per memory rule: show full per-target output, do NOT grep for failure-only).
- [ ] T024 Open PR via `gh pr create` with title `feat(071): cross-format mikebom:* annotation parity (closes alpha.13 SPDX 2.3 emitter gap)`. Body cites the SC-001 / SC-002 measurement targets, the 6 keys closed by US1, and the C-1 through C-8 contract clauses now in force.

## Dependencies

```text
T001 (Setup)
   │
   ├── T002 → T003                          (Foundational — block T011, T012, T018; do NOT block T004-T010)
   │
   └── T004,T005,T006,T007,T008,T010        (US1 emission audits — parallel, can begin as soon as T001 completes)
            │                                 (T010 = SPDX 3 audit, expected zero changes per research §7)
            ├── T009                         (US1 — promote C19 to SymmetricEqual; depends on T008's cpe-candidates audit)
            └── T011,T012                    (US1 — extractor wire-up, depends on T002+T003 foundational + T004-T009 emission)
                       │
                       └── T013 → T014       (US2 — golden regen FIRST, then parity-check verification reads regenerated goldens)
                                │
                                ├── T015,T016 (US3 — parallel, code-comment audit + doc-table)
                                │       │
                                │       └── T017 (US3 doc-sync test, depends on T015+T016)
                                │
                                ├── T018 (US4 — parity_completeness test, depends on T011/T012 extractors wired)
                                ├── T019 (US4 — synthetic drift test, parallel with T018)
                                │       │
                                │       └── T020 (US4 — pre-PR gate verification, depends on T018+T019 landed)
                                │
                                └── T021,T022 → T023 → T024 (Polish — CHANGELOG, design-notes, gate, PR)
```

**Sequencing note (analyze F1 fix):** T013 (golden regen) MUST run before T014 (parity-check). The previous draft had them swapped, which would have caused T014 to fail against pre-fix goldens. Order is now: regen first, then verify.

## Format validation

All 24 tasks follow the required format. Setup (T001), Foundational (T002-T003), US1 (T004-T012), US2 (T013-T014), US3 (T015-T017), US4 (T018-T020), Polish (T021-T024). Every US-phase task carries the [US#] story label; every P-marked task is genuinely parallelizable (different files or independent audits).

## MVP scope

**US1 alone is the MVP.** It covers:
- SC-003 (the 6 alpha.13-driving keys closed) entirely.
- SC-001 / SC-002 (≥95% / ≥85% reduction) — emerges automatically when US1 lands; US2's tasks are *verification* of US1's output.
- SC-006 (consumer parity per component) — emerges from US1.

**US3 + US4 are the post-MVP guardrail.** They prevent regression and publish the catalog for external consumers.

US2 is not implementable without US1; US3 is not strictly blocking but lands in the same milestone for atomicity (per spec rationale "P2 because the CFI fix from US1 closes the *current* gap, but without US4 the next milestone reopens a fresh gap").

## Parallel execution opportunities

- **T004-T008** can all run in parallel (5 independent audits in different code regions of the same file — coordinate via short-lived branch commits, OR have one engineer batch them in one pass; either way the *task graph* is parallel).
- **T010** (SPDX 3 audit) is parallel with T004-T008.
- **T011 + T012** (extractor wire-up in spdx2.rs and cdx.rs) are parallel after T004-T009 complete.
- **T015 + T016** (US3 code-comment audit + docs entry) are parallel.
- **T018 + T019** (US4 parity_completeness + synthetic_drift tests) are parallel.

## Independent test criteria (per user story)

- **US1**: Each of the 5 unit tests added in T004-T008 + the parity-check run in T014 (against the T013-regenerated goldens) returns zero `SymmetricEqual` violations across the 27 fixtures.
- **US2**: Per-ecosystem CFI count documented in T014 shows ≥95% reduction vs alpha.13 baseline (or ≤556 absolute) on at least one realistic ecosystem (gem, maven, golang are good candidates given their high pre-fix counts).
- **US3**: T017 doc-sync test passes; manually inspect `docs/reference/sbom-format-mapping.md` and confirm every non-SymmetricEqual row has rationale + standards-native pointer.
- **US4**: T019 synthetic drift test fails when reverted to a pre-T018 state, confirming the regression test detects what it should.

## Closing alpha.13 conformance debt

Post-merge of this milestone:
- The headline CFI count drops from 11,130 → ≤556 component-level (per SC-001).
- The total finding count drops from 12,165 → ≤1,800 (per SC-002).
- All 6 specific alpha.13-driving annotation keys are resolved (5 by symmetric emission, 1 already correctly modelled with documented rationale).
- The pre-PR gate prevents the same gap from recurring in milestones 072+.
