---

description: "Task list for milestone 218 — cross-ecosystem dep-name edge resolution (closes #633)"
---

# Tasks: Cross-ecosystem dep-name edge resolution

**Input**: Design documents from `/specs/218-cross-ecosystem-edges/`
**Prerequisites**: spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md — ALL committed on branch `218-cross-ecosystem-edges` (commit `9bc589f` and earlier).

**Tests**: Yes — TDD-style unit tests for the tie-break algorithm are called out in `contracts/tie-break-rule.md`; integration tests are gated by SC-001/SC-002/SC-007/SC-008/SC-009; parity extractors are gated by SC-005. Tests are NOT optional for this milestone.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story. The MVP is US1 alone (correctness fix behind the experimental flag); US2 (annotations) is required by SC-003/SC-004/SC-005; US3 (FR-009 ecosystem-agnosticism proof) is a synthetic-fixture test that validates the design without adding new production code.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- File paths in descriptions are absolute repository-relative.

## Path Conventions

Single-crate (`waybill-cli`) touch per plan.md's Project Structure section.
- Production code: `waybill-cli/src/**`
- Tests: `waybill-cli/tests/**` (integration) + `#[cfg(test)] mod tests` (unit)
- Docs: `docs/reference/**`
- Spec artifacts: `specs/218-cross-ecosystem-edges/**`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify branch state + establish pre-implementation baselines that every subsequent phase leans on.

- [ ] T001 Verify branch `218-cross-ecosystem-edges` is checked out and up-to-date with `main` post-m217 merge. Confirm HEAD is the plan-phase commit via `git log -1 --oneline`. Expected: `9bc589f plan(218): cross-ecosystem edges — plan + research + data-model + contracts + quickstart`.
- [ ] T002 Capture the post-m216 baseline SBOM for the SC-009 flag-off byte-identity gate. Run `cargo +stable build -p waybill --release` (or use an existing release binary). Then run: `./target/release/waybill scan --path ~/.cache/waybill/fixtures/fffc00b50395e731650de09317a88972a49faac6/transitive_parity/gem --format cyclonedx-json --output /tmp/m218_baseline_flag_off.cdx.json`. This file becomes the golden for T032. Confirm it parses as JSON and contains `197` dependency edges via `jq '.dependencies | length'`.
- [ ] T003 Verify no existing fixture uses `pkg:generic/` with populated `depends[]` outside the m216 Gemfile path: `grep -rn "pkg:generic" waybill-cli/tests/fixtures/*/expected/ 2>/dev/null | head`. Confirms the flag-off byte-identity contract is testable on more than one fixture.
- [ ] T004 [P] Read `waybill-cli/src/scan_fs/mod.rs:530-810` (existing `name_to_purl` index construction + the same-ecosystem lookup loop). This is the surface the m218 fallback extends. Take notes on: how `ecosystem` is derived (`entry.purl.ecosystem()`), how `packages` is iterated, and how `Relationship` is constructed.
- [ ] T005 [P] Read `waybill-cli/src/scan_fs/package_db/gem.rs::build_main_module_entry` (search for `pkg:generic` — the m216 emitter site). Confirm it populates `entry.depends` with bare gem names AND sets `entry.is_main_module = true`. These are the two preconditions the FR-001 resolver check reads.
- [ ] T006 [P] Read the existing parity-extractor precedent for per-edge annotations. Grep for how m147 peer-edge annotations wire up: `grep -rn "peer" waybill-cli/src/parity/extractors/ | head`. Also `grep -rn "package-shape" waybill-cli/src/parity/extractors/` (m216 C135 — same doc-scope-annotation shape we're mirroring for C139).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Data model + emitter plumbing that BOTH US1 and US2 depend on. Nothing user-facing yet — this is the type surface and threading.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T007 Create the module directory + payload types file at `waybill-cli/src/generate/cross_ecosystem_edges/mod.rs`. Add `pub mod tie_break; pub mod normalize;` declarations. Add the three payload structs from data-model.md E1/E2/E3: `CrossEcosystemInferencePayload`, `AlternateMatch`, `CrossEcosystemInferenceAmbiguousPayload`, `CrossEcosystemInferenceUnresolvedRecord`. All fields in alphabetic declaration order (canonical JSON contract per data-model.md validation rules). All derive `Serialize, Deserialize, PartialEq, Eq, Clone, Debug`. Register the new module in `waybill-cli/src/generate/mod.rs` with `pub mod cross_ecosystem_edges;`.
- [ ] T008 In `waybill-cli/src/generate/cross_ecosystem_edges/mod.rs`, add the aggregate report type: `CrossEcosystemEdgesReport` (E4) with `crossed_edges: BTreeMap`, `ambiguous_edges: BTreeMap`, `unresolved: Vec`, and `summary: CrossEcosystemEdgesSummary`. Add `CrossEcosystemEdgesSummary` (E4) with three `usize` counters. Both derive `Debug, Default, Clone`.
- [ ] T009 Add `pub cross_ecosystem_edges_report: Option<&'a CrossEcosystemEdgesReport>` field to `ScanArtifacts` in `waybill-cli/src/generate/mod.rs`. Default at every construction site: `None`. Grep for `ScanArtifacts {` occurrences and add the field to each — same pattern as m217's `go_toolchains_detected` propagation. Update `ScanArtifacts::narrow` to copy the field through.
- [ ] T010 [P] Add `pub cross_ecosystem_edges_report: Option<CrossEcosystemEdgesReport>` field to `ScanDiagnostics` at `waybill-cli/src/scan_fs/package_db/mod.rs` (propagation intermediary). Default `None`.
- [ ] T011 [P] Add `pub cross_ecosystem_edges_report: Option<CrossEcosystemEdgesReport>` field to `ScanResult` at `waybill-cli/src/scan_fs/mod.rs`. Default `None`.
- [ ] T012 Add the new `--experimental-cross-ecosystem-edges` boolean flag to `ScanArgs` in `waybill-cli/src/cli/scan_cmd.rs` per data-model E5 + contracts/cross-ecosystem-flag.md. Use `ArgAction::SetTrue`; add env var `WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES`; help text references `docs/reference/cross-ecosystem-edges.md`. The flag field type is `pub experimental_cross_ecosystem_edges: bool`.

**Checkpoint**: Foundation ready — user story implementation can now begin. All three payload types exist as private symbols; the flag is parsed but unused; the report threads through `ScanArtifacts` but stays `None`.

---

## Phase 3: User Story 1 - Gemfile-only Ruby app SBOM has real outgoing edges (opt-in) (Priority: P1) 🎯 MVP

**Goal**: When the experimental flag is enabled, the resolver bridges `pkg:generic/` main-module `depends[]` names to matching non-generic components across every ecosystem present in the scan. Delivers the FR-001/002/003/004 correctness fix behind the FR-000 flag.

**Independent Test**: Scan the fastlane fixture (`~/.cache/waybill/fixtures/*/transitive_parity/gem/`) WITH the flag enabled. `jq '.dependencies[] | select(.ref | startswith("pkg:generic/")) | .dependsOn | length'` returns ≥ 24. Scan WITHOUT the flag; same jq returns `0`. Delivers SC-001, SC-002, and SC-009.

### Implementation for User Story 1

- [ ] T013 [US1] In `waybill-cli/src/generate/cross_ecosystem_edges/normalize.rs`, add a small `pub fn target_normalized_name(target_eco: &str, name: &str) -> String` helper that delegates to the existing `crate::scan_fs::mod::normalize_dep_name` at `mod.rs:1460`. Doc-comment cites FR-012 (target-ecosystem's normalization applies). Add 1 unit test asserting `target_normalized_name("pypi", "Requests-OAuth") == normalize_dep_name("pypi", "Requests-OAuth")` (invariant check — proves the helper doesn't diverge).
- [ ] T014 [US1] In `waybill-cli/src/generate/cross_ecosystem_edges/tie_break.rs`, implement `pub fn resolve_cross_ecosystem(...)` per `contracts/tie-break-rule.md` pseudo-code. Signature: `fn resolve_cross_ecosystem(dep_name: &str, source_purl: &str, candidate_matches: Vec<(String, String)>, sibling_ecosystems: &HashSet<String>, lookup_via: &str) -> Vec<EdgeEmission>` where `EdgeEmission` is a new enum with `Resolved(String, CrossEcosystemInferencePayload)` and `Ambiguous(String, CrossEcosystemInferencePayload, CrossEcosystemInferenceAmbiguousPayload)` variants. The function is pure (no side effects) so it's unit-testable standalone.
- [ ] T015 [P] [US1] Add the 7 unit tests from `contracts/tie-break-rule.md`'s test coverage matrix to `waybill-cli/src/generate/cross_ecosystem_edges/tie_break.rs::tests`: (1) single-candidate fast path; (2) single-candidate no siblings; (3) multi-candidate one sibling match; (4) multi-candidate two sibling matches; (5) multi-candidate zero sibling matches; (6) alternates self-exclusion invariant; (7) empty candidate list panics-or-is-not-called (assert with a doc-test note that the caller MUST short-circuit before invoking).
- [ ] T016 [US1] In `waybill-cli/src/scan_fs/mod.rs` around line 794 (immediately inside the existing `for dep_name in &entry.depends` loop), extend the `if let Some(to) = name_to_purl.get(&key)` block with an `else` branch. When the flag `experimental_cross_ecosystem_edges` is enabled AND `ecosystem == "generic"` AND the same-ecosystem lookup missed, invoke the R2 search over `name_to_purl.iter()` to build the candidate list, sort it deterministically per `contracts/tie-break-rule.md`, then call `resolve_cross_ecosystem(...)` from T014. For each returned `EdgeEmission`, push the appropriate `Relationship` AND record the payload into the local `CrossEcosystemEdgesReport` accumulator.
- [ ] T017 [US1] Precompute the `sibling_ecosystems: HashSet<String>` per data-model E7 ONCE before entering the `for (ecosystem, entry) in &packages` loop at `mod.rs:794`. Filter `packages` for entries whose `is_main_module == true` AND `purl.ecosystem() != "generic"`; collect ecosystems into the set. Pass by `&` reference into `resolve_cross_ecosystem`.
- [ ] T018 [US1] When the FR-001 resolver falls through with zero candidate matches (empty result set), record the `{source_purl, unresolved_name}` record into `report.unresolved` per FR-004. This is the short-circuit path that skips T014's `resolve_cross_ecosystem` per its contract note. Unresolved insertions are sorted at emission time (not insertion).
- [ ] T019 [US1] Thread the accumulated `CrossEcosystemEdgesReport` out of the resolver: at the end of the `for (ecosystem, entry) in &packages` loop, if `report.crossed_edges.len() + report.unresolved.len() > 0`, populate `report.summary` (three counters) and assign `report` to the propagation slot (via T010's `ScanDiagnostics.cross_ecosystem_edges_report`). When the flag is OFF, report stays `None`. When flag is ON but no cross-ecosystem lookups fired (no `pkg:generic/` main-modules in scan), report is `Some(default())` per FR-008 + contracts/cross-ecosystem-flag.md.
- [ ] T020 [US1] Thread the report end-to-end so ALL three emitters (CDX + SPDX 2.3 + SPDX 3) can read it. Steps: (a) `ScanDiagnostics` → `ScanResult` in `waybill-cli/src/scan_fs/mod.rs`; (b) `ScanResult` → `ScanArtifacts` construction in `waybill-cli/src/cli/scan_cmd.rs`; (c) `ScanArtifacts` → SPDX 3's ScanArtifacts constructor in `waybill-cli/src/generate/spdx/v3_document.rs` (SPDX 3 has its own ScanArtifacts shape; m217 established this dual-construction pattern for `go_toolchains_detected`); (d) `ScanArtifacts` → OpenVEX + SPDX packages/relationships as needed (grep every `ScanArtifacts {` construction site — m217 required updates to 5-6 test helpers + several real constructors). Template: m161 `go_workspace_mode` field propagation + m217 `go_toolchains_detected` for the v3_document.rs threading. Verify with `cargo +stable check --workspace` before proceeding to T021 — cascading compile errors on missing constructor fields are the failure mode this task exists to prevent (per m217 lessons learned).
- [ ] T021 [US1] Emit the FR-013 INFO log line at resolver-exit iff `report` is `Some(_)` AND the flag is ON. Format: `tracing::info!("cross-ecosystem edges: resolved={} ambiguous={} unresolved={}", report.summary.edges_resolved, report.summary.edges_ambiguous, report.summary.names_unresolved);`. Silence otherwise.

### Tests for User Story 1

- [ ] T022 [US1] Extend `waybill-cli/tests/transitive_parity_gem.rs` with a new `#[test] fn m218_flag_on_recovers_edges_from_pkg_generic_main_module()`. Runs the same fastlane fixture WITH the `--experimental-cross-ecosystem-edges` flag. Asserts total DEPENDS_ON edge count ≥ 221 (per R7 math: 197 + 24 minimum resolvable). Asserts count of DEPENDS_ON edges whose source is `pkg:generic/` ≥ 24. Do NOT modify the existing 197-edge test — SC-009 preserves the flag-off assertion verbatim. **Additionally per FR-010**: update the multi-line comment header above `EXPECTED_WAYBILL_EDGE_COUNT` (currently at `transitive_parity_gem.rs:20-45`) with a new paragraph explaining the m216 → m218 delta — mirror the existing m162/m216 comment block style (dated milestone reference, edge-count arithmetic, one-sentence rationale). Add a companion `EXPECTED_WAYBILL_EDGE_COUNT_FLAG_ON: usize = <exact-recomputed-value>` constant next to it, referenced by the new `#[test]` function so the flag-on baseline is as visible + auditable as the flag-off baseline.
- [ ] T023 [US1] Create `waybill-cli/tests/cross_ecosystem_edges.rs` with a helper `run_scan(fixture_path, flag_on: bool) -> serde_json::Value` following the milestone-217 `waybill-cli/tests/goroot_skip.rs` isolated-HOME env pattern. Then add `#[test] fn us1_flag_on_gemfile_fixture_emits_generic_source_edges()` — reuses the existing gem `Gemfile.lock`-only mini fixture at `waybill-cli/tests/fixtures/gemfile_application/` — asserts exactly 2 outgoing DEPENDS_ON edges from `pkg:generic/` (for `json` + `rack`), one to `pkg:gem/json@2.7.1` and one to `pkg:gem/rack@3.0.9`.

**Checkpoint**: US1 complete. Flag-on scans emit outgoing edges from `pkg:generic/` main-modules; flag-off scans preserve current byte-identity (verified by T032 later).

---

## Phase 4: User Story 2 - Cross-ecosystem edges are annotated for consumer trust (Priority: P2)

**Goal**: Every crossed edge carries a `waybill:cross-ecosystem-inference` per-edge annotation (C137); every ambiguous edge additionally carries `waybill:cross-ecosystem-inference-ambiguous` (C138); every scan with ≥1 unresolved cross-ecosystem name carries a doc-scope `waybill:cross-ecosystem-inference-unresolved` (C139). Three-format parity holds. Delivers SC-003, SC-004, SC-005.

**Independent Test**: Flag-on fastlane scan → parse CDX → for every `dependencies[i]` where `ref` starts with `pkg:generic/`, assert every entry in `dependsOn[]` corresponds to a `properties[]` entry named `waybill:cross-ecosystem-inference` whose payload's `target_purl` matches that specific dep target. Cross-check same edges in the SPDX 2.3 and SPDX 3 outputs; parity extractors report equality.

### Implementation for User Story 2 — docs first (per m216 C1-remediation precedent)

- [ ] T024 [US2] Add THREE new rows to `docs/reference/sbom-format-mapping.md` immediately after the existing C136 row (m217 go-toolchain-detected). Follow the C121-C136 KEEP-NO-NATIVE template: annotation name, per-format landing slot (verbatim from `contracts/annotation-payloads.md` table), payload shape (JSON example), audit citation of rejected native alternatives per Constitution Principle V (see plan.md Constitution Check §V for the exact rejection reasoning for each row), milestone-218 citation clause. Rows in numeric order: C137 (`waybill:cross-ecosystem-inference`, per-edge), C138 (`waybill:cross-ecosystem-inference-ambiguous`, per-edge), C139 (`waybill:cross-ecosystem-inference-unresolved`, document-scope). **⚠️ COUPLED WITH T025-T027**: docs row MUST be committed together with (or before) the parity-extractor registrations to keep the `every_catalog_row_has_an_extractor` bidirectional test green at every commit. Same pattern as m216 PR #632 C1 remediation and m217's T023↔T020 pairing.

### Implementation for User Story 2 — parity extractors

- [ ] T025 [US2] Register `c137_cdx`, `c138_cdx`, `c139_cdx` in `waybill-cli/src/parity/extractors/cdx.rs` — three new lines using the `cdx_anno!` macro. C137 + C138 are `component` scope (per-edge landing is on the source-Component's properties per contracts/annotation-payloads.md); C139 is `document` scope. Match the exact insertion pattern after `c136_cdx` (m217).
- [ ] T026 [US2] Register `c137_spdx23`, `c138_spdx23`, `c139_spdx23` in `waybill-cli/src/parity/extractors/spdx2.rs` — three new lines using the `spdx23_anno!` macro. C137 + C138 are `component` scope; C139 is `document`.
- [ ] T027 [US2] Register `c137_spdx3`, `c138_spdx3`, `c139_spdx3` in `waybill-cli/src/parity/extractors/spdx3.rs` — three new lines using the `spdx3_anno!` macro. Same component/document split.
- [ ] T028 [US2] Register three new `ParityExtractor` rows in the `EXTRACTORS` array at `waybill-cli/src/parity/extractors/mod.rs` — C137, C138, C139. All `Directionality::SymmetricEqual`, all `order_sensitive: false`. Add nine new use-list entries: `c137_cdx, c138_cdx, c139_cdx` in the `use cdx::{...}` block; same triples for spdx2 + spdx3.

### Implementation for User Story 2 — CDX emission

- [ ] T029 [P] [US2] In `waybill-cli/src/generate/cyclonedx/dependencies.rs` (search for existing `dependencies[i]` object construction — the loop that emits the CDX `dependencies` array), extend each per-source `dependencies[i]` object with a `properties[]` field populated from `artifacts.cross_ecosystem_edges_report`. For every `(source_purl, target_purl)` key in `report.crossed_edges` where `source_purl` equals this iteration's source, push a property `{"name":"waybill:cross-ecosystem-inference","value":"<canonical-json>"}`. Additionally, if the same key exists in `report.ambiguous_edges`, push a second property `{"name":"waybill:cross-ecosystem-inference-ambiguous","value":"<canonical-json>"}`. Silent when the report is None or has no crossed_edges for this source.
- [ ] T030 [P] [US2] In `waybill-cli/src/generate/cyclonedx/metadata.rs` (search for the m217 `waybill:go-toolchain-detected` doc-scope emission block installed by m217 — same insertion pattern), add the C139 doc-scope emission block. Silent when `report.unresolved.is_empty()` (FR-011). When non-empty, serialize the vec via `serde_json::to_string(&report.unresolved).unwrap_or_default()` and push `{"name":"waybill:cross-ecosystem-inference-unresolved","value":"<canonical-json>"}` onto `metadata.properties[]`.

### Implementation for User Story 2 — SPDX 2.3 + SPDX 3 emission

- [ ] T031 [US2] In `waybill-cli/src/generate/spdx/annotations.rs`, add per-Package + doc-scope emission analogous to T029/T030. Per-Package annotations MUST use the standard `MikebomAnnotationCommentV1` envelope. Per-Package precedent for per-EDGE annotations: grep for `waybill:peer-provided` (m178) or `waybill:optional-derivation` (m180) — both are per-edge annotations landing on the source Package with in-payload target correlation, matching C137/C138's landing pattern. Doc-scope precedent for C139: grep for `waybill:workspaces-detected` (m176) or `waybill:go-toolchain-detected` (m217) — same silence-when-empty envelope shape. **Silent-when-empty per FR-011**: for doc-scope C139, guard the emission with `if !report.unresolved.is_empty() { ... }` — matches T030's CDX-side gate; both formats MUST skip the annotation entirely (not emit an empty array) when unresolved is empty.
- [ ] T032 [US2] In `waybill-cli/src/generate/spdx/v3_annotations.rs`, add per-Relationship + doc-scope emission for the SPDX 3 format. C137/C138 emit as `Annotation` elements whose `subject` IRI is the specific `Relationship` element joining source + target. C139 emits doc-scope on the SpdxDocument root IRI — same template as m217 C136 (grep for `waybill:go-toolchain-detected` in v3_annotations.rs). **The v3_document.rs threading was completed in T020(c); this task only edits v3_annotations.rs.** Silent-when-empty for C139 per FR-011 (same guard as T031).

### Tests for User Story 2

- [ ] T033 [US2] Extend `waybill-cli/tests/cross_ecosystem_edges.rs` with `#[test] fn us2_flag_on_edges_carry_c137_annotation()` — flag-on scan of the mini gemfile fixture, assert every `dependencies[i]` where `ref` starts with `pkg:generic/` has a `properties[]` entry named `waybill:cross-ecosystem-inference` for EACH entry in `dependsOn[]`. Parse the property value as JSON; verify all four payload fields (`from_eco`, `lookup_via`, `target_purl`, `to_eco`) match expected values.
- [ ] T034 [US2] Add `#[test] fn us2_same_ecosystem_edges_lack_c137_annotation()` — flag-on scan of a pure-gem fixture (no `pkg:generic/` main-modules). Assert `properties[]` entries named `waybill:cross-ecosystem-inference` are ABSENT from every dependency object. Proves SC-004 (0% false-positive annotation rate).
- [ ] T035 [US2] Add `#[test] fn us2_three_format_parity_holds_for_crossed_edges()` — flag-on scan of the mini gemfile fixture, emit all three formats (CDX + SPDX 2.3 + SPDX 3), parse each, extract C137 annotations via the parity extractors, assert the three sets are equal. Proves SC-005.
- [ ] T036 [US2] Add `#[test] fn us2_ambiguous_match_emits_all_candidates_with_c138()` to `waybill-cli/tests/cross_ecosystem_edges.rs` using the **T037/T038 hand-constructed-`PackageDbEntry` pattern** (bypass filesystem readers — no on-disk fixture required). Construct four `PackageDbEntry` records in-process: (a) source main-module `pkg:generic/multi-eco-app@0.0.0-unknown` with `is_main_module: true` and `depends: vec!["json"]`; (b) `pkg:gem/json@2.7.1` (non-main-module); (c) `pkg:pypi/json@0.1.1` (non-main-module); (d) `pkg:npm/json@1.0.0` (non-main-module). Zero non-generic main-modules in the scan (so FR-003's tie-break intersection is empty). Invoke the resolver pass with the flag ON. Assert: (i) THREE DEPENDS_ON edges emitted from `pkg:generic/multi-eco-app` — one per matching ecosystem; (ii) each edge carries BOTH C137 AND C138 annotations; (iii) each edge's C138 `alternates[]` contains exactly the two OTHER matches (self-exclusion invariant), sorted lex by `target_purl`. **Rationale for hand-construction**: a filesystem fixture cannot produce `pkg:pypi/` components without a pip reader (no pip reader today); a filesystem fixture cannot produce `pkg:npm/` without a package.json+lockfile combo; hand-constructing bypasses this readerless-fixture impossibility while still exercising the same resolver + emitter code path.

**Checkpoint**: US2 complete. All three annotations emit correctly across three formats; parity gate green; ambiguity is transparent to consumers.

---

## Phase 5: User Story 3 - Fix generalizes beyond Ruby to future m216-alikes (Priority: P3)

**Goal**: Prove FR-009 ecosystem-agnosticism via a synthetic `pkg:generic/ → pkg:pypi/` cross-ecosystem lookup — demonstrating that the resolver works for any future m216-alike reader without further code changes. Delivers SC-007.

**Independent Test**: Hand-constructed `Vec<PackageDbEntry>` with a fake `pkg:generic/my-pip-app@0.0.0-unknown` main-module having `depends: ["requests", "click"]` alongside real `pkg:pypi/requests@2.31.0` + `pkg:pypi/click@8.1.7` components. Run the resolver pass (not the full scan). Assert 2 outgoing edges emitted from the pip-app main-module.

### Implementation for User Story 3

- [ ] T037 [US3] Extend `waybill-cli/tests/cross_ecosystem_edges.rs` with `#[test] fn us3_flag_on_synthetic_pip_app_produces_edges()` — hand-construct `Vec<PackageDbEntry>` per R9 (one `pkg:generic/my-pip-app@0.0.0-unknown` main-module with `depends: vec!["requests", "click"]`, two `pkg:pypi/` transitive components). Invoke the resolver pass directly (bypass filesystem readers). Assert 2 DEPENDS_ON edges emitted from the pip-app main-module, each carrying C137 with `to_eco: "pypi"` and `lookup_via` matching the synthetic reader-path identifier registered by the test.
- [ ] T038 [US3] Add `#[test] fn us3_unresolved_name_lands_in_doc_scope_annotation()` — synthetic scan where the `pkg:generic/my-pip-app@0.0.0-unknown` main-module has `depends: vec!["requests", "nonexistent-package-xyz"]` but only `pkg:pypi/requests@2.31.0` is in the resolver index. Assert 1 edge emitted (for `requests`) AND doc-scope C139 annotation containing `{source_purl: "pkg:generic/my-pip-app@0.0.0-unknown", unresolved_name: "nonexistent-package-xyz"}`. Proves SC-008.

**Checkpoint**: US3 complete. Ecosystem-agnosticism proven; SC-007 + SC-008 gates green.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Consumer-facing documentation (FR-014), byte-identity gate (SC-009), byte-identity for non-Ruby fixtures (SC-006), and pre-PR verification.

- [ ] T039 [P] Author `docs/reference/cross-ecosystem-edges.md` per FR-014 + research R6 (six sections, ~200 lines): what the flag does, when to enable it, interpreting the three annotations (with per-format landing-slot tables), decision tree for consumers, experimental status disclaimer, worked example. Link from `README.md` under a "SBOM interpretation" section (add if absent). Update the C137/C138/C139 rows in `docs/reference/sbom-format-mapping.md` (added by T024) to link back to this new doc.
- [ ] T040 [P] Commit the SC-009 baseline golden at `waybill-cli/tests/fixtures/cross_ecosystem/golden_flag_off.cdx.json` — the file captured by T002. Add `#[test] fn flag_off_preserves_current_post_m216_byte_identity()` to `waybill-cli/tests/cross_ecosystem_edges.rs` that scans the fastlane fixture WITHOUT the flag and asserts byte-equality against the golden. Regeneration gated by env var `MIKEBOM_UPDATE_CROSS_ECOSYSTEM_GOLDEN=1` per feedback_release_bump_regen_goldens.md memory.
- [ ] T041 SC-006 verification: run `cargo +stable test -p waybill --test cdx_regression --test spdx_regression --test spdx3_regression`. Every non-Ruby fixture MUST pass byte-identity without regen (11 fixtures × 3 formats = 33 assertions). If any fail, investigate — the FR-000 flag defaults OFF so failures indicate an unexpected code path was reached.
- [ ] T042 Pre-PR gate per Constitution: `./scripts/pre-pr.sh` — clippy `-D warnings` + `cargo test --workspace` (every suite `ok. N passed; 0 failed`). Watch for the pre-existing podman env-var race per `reference_podman_test_flake.md` memory. Read `feedback_prepr_gate_bails_on_first_failure.md` before treating any failure as a flake — use `--no-fail-fast` + enumerate every `^---- .+ stdout ----` line before claiming green.
- [ ] T043 m214 grep gate: `BADHITS=$(grep -rE '\bmikebom\b' waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml Dockerfile.ebpf-test scripts 2>/dev/null | grep -v '^Binary file' | grep -vE 'mikebom-test-fixtures' || true)`; expects zero output.
- [ ] T044 Push branch: `git push origin 218-cross-ecosystem-edges`.
- [ ] T045 Open PR against `main` titled `impl(218): cross-ecosystem dep-name edge resolution (closes #633)`. PR body includes: (a) summary + link to spec/plan + closing issue #633; (b) Test Plan enumerating every US1/US2/US3 integration test + all 7 unit tests + pre-PR gate + SC-006 byte-identity baselines + SC-009 flag-off golden gate + m214 grep gate; (c) Migration/backward-compat note (flag defaults OFF; existing consumers see NO change; opt-in via `--experimental-cross-ecosystem-edges`); (d) Docs link to `docs/reference/cross-ecosystem-edges.md`; (e) Explicit call-out that the annotation shapes are experimental and MAY evolve before flag graduation.
- [ ] T046 CI-side verification: all 20 CI checks (linux-x86_64 default + ebpf-tracing, macOS, Windows, Kusari Inspector, 15 rootfs/language scanners) MUST pass. Merge blocked until all green. Watch for the pre-existing podman env-var race documented in `reference_podman_test_flake.md`; rerun the failed CI job once before treating as a real regression.

---

## Dependency Graph

- **Phase 1** (T001-T006) — parallel-safe within phase; T001-T003 sequential, T004-T006 parallel with each other.
- **Phase 2** (T007-T012) — blocks all subsequent phases. T007 → T008 (sequential within same file); T009 depends on T007+T008; T010 || T011 (different files); T012 independent.
- **Phase 3 US1** (T013-T023) — depends on Phase 2 complete. T013 || T014 (different files); T015 depends on T014; T016 depends on T013+T014; T017 || T016 (same file, sequential); T018 depends on T017; T019 depends on T018; T020 depends on T019 AND covers threading through EVERY emitter's ScanArtifacts constructor (per analyze-phase F1 remediation — T020 is a broader task than the pre-remediation shape); T021 depends on T020; T022 depends on T021 (flag-on scan needs full pipeline); T023 depends on T021 (same reason).
- **Phase 4 US2** (T024-T036) — depends on Phase 3 US1 complete (needs report data flowing AND fully threaded per T020). **⚠️ T024 (docs C137/C138/C139 rows) MUST commit together with (or before) T028 (EXTRACTORS registration)** — bidirectional catalog test invariant per m216/m217 C1 pattern. T024 → T025 || T026 || T027 (all three format-side extractor registrations parallel) → T028 (mod.rs registration composes them). T029 || T030 (different files); T031 depends on T029+T030 (SPDX 2.3 mirrors CDX); T032 depends only on T031 for the SPDX 3 annotation shape (v3_document.rs threading was done in T020 per F1 remediation). T033-T036 test tasks depend on emission code (T029-T032) complete. **T036 uses hand-constructed `PackageDbEntry` records per analyze-phase C1 remediation** — no filesystem fixture; no `cross_ecosystem/ambiguous_multi_eco/` dir needed.
- **Phase 5 US3** (T037-T038) — depends on Phase 3 US1 + Phase 4 US2 complete. Both tasks parallel-safe (different `#[test] fn`s in same file).
- **Phase 6 Polish** (T039-T046) — T039 || T040 (different files); T041 depends on T032 complete (SPDX 3 emission finalized); T042 depends on everything else; T043 depends on T042; T044 depends on T043; T045 depends on T044; T046 is CI-side (blocks user-controlled merge, not local work).

## Parallel Execution Examples

- **After T007-T009**: T010 and T011 in parallel (different files).
- **After T014**: T015 unit tests parallel with T013 helper file work.
- **T024→T025+T026+T027**: three per-format extractor registrations in parallel; T028 composes them.
- **T029||T030**: CDX per-edge + doc-scope emissions in parallel.
- **T033-T036**: after emitters complete, four test-only additions in parallel (all in `cross_ecosystem_edges.rs`, so sequential commits but concurrent authoring).
- **T037||T038**: two US3 synthetic tests in parallel.
- **T039||T040**: docs authoring parallel with golden capture + SC-009 test authoring.

## Implementation Strategy

**MVP scope (US1 only)**: Ship Phase 1 + Phase 2 + Phase 3 (T001-T023). Delivers the correctness fix behind the flag; consumers who opt in see recovered edges but no annotations. This alone closes #633's core impact. Skip Phase 4+ ONLY if timeline pressure demands — but do NOT ship without US2 unless the PR body explicitly notes the annotation deferral.

**Recommended scope (US1 + US2)**: Ship through Phase 4 (T001-T036). Delivers correctness + transparency. This is the natural PR scope; ships as a single m218 PR titled `impl(218): cross-ecosystem dep-name edge resolution (closes #633)`.

**Full scope (US1 + US2 + US3)**: Ship through Phase 6. Adds the synthetic FR-009 ecosystem-agnosticism proof + polish. Total 46 tasks. Estimated ~1200 LoC production + ~800 LoC tests + ~200 LoC docs. Recommended target for the initial PR — locks in the design guarantees for future m216-alike readers.

## Format Validation

All 46 tasks follow the checklist format (`- [ ] TID [P?] [Story?] Description with file path`). Story labels present on all Phase 3-5 tasks (US1/US2/US3); absent on Phase 1/2/6 tasks per convention. File paths absolute-repository-relative throughout. Parallel markers `[P]` applied where independence is genuine.
