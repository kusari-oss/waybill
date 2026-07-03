# Tasks: Workspace-root peer linkage + graph-completeness annotations (milestone 158)

**Input**: Design documents from `/specs/158-graph-completeness/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Included per spec (SC-007 requires ≥10 unit tests + SC-008 requires integration test).

**Organization**: Tasks are grouped by user story from spec.md (US1 P1, US2 P2, US3 P3) so each can be implemented + tested + delivered independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies).
- **[Story]**: Which user story (US1 / US2 / US3). Setup + Foundational + Polish have no story tag.
- Every task includes an exact file path.

## Path Conventions

Rust workspace (`Cargo.toml` at repo root). Source under `mikebom-cli/src/`, tests under `mikebom-cli/tests/` (integration) or inline `#[cfg(test)] mod tests` (unit). All paths absolute from `/Users/mlieberman/Projects/mikebom/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Baseline capture + pre-158 snapshot for SC-001 / SC-002 verification later.

- [ ] T001 Verify baseline state: `git log -1 --oneline` on branch `158-graph-completeness`; confirm milestone 157 (PR #491, commit `474354c`) is at or near main tip; capture pre-158 `mikebom-cli/src/generate/` LOC counts for delta reporting at PR close.

- [ ] T002 Snapshot pre-158 SBOMs for the 5 `kusari-sandbox/test-*` repos into `/tmp/158-pre-snapshot/` — each ecosystem gets `<repo>.cdx.json`, `<repo>.spdx.json`, `<repo>.spdx3.json`. Command shape:
  ```bash
  mkdir -p /tmp/158-pre-snapshot
  for repo in test-podman test-kubernetes test-podman-desktop test-rails test-guac-visualizer; do
    ./target/release/mikebom sbom scan --path /tmp/kusari-audit/$repo \
      --no-deep-hash \
      --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
      --output cyclonedx-json=/tmp/158-pre-snapshot/${repo}.cdx.json,spdx-2.3-json=/tmp/158-pre-snapshot/${repo}.spdx.json,spdx-3-json=/tmp/158-pre-snapshot/${repo}.spdx3.json
  done
  ```
  These snapshots drive SC-004 empirical comparison at T027–T028.

- [ ] T003 Snapshot pre-158 golden fixtures via `git show main:mikebom-cli/tests/fixtures/golden/<format>/<ecosystem>.<ext>` into `/tmp/158-pre-goldens/` for all 33 files (11 ecosystems × 3 formats). Drives SC-002 byte-identity guard at T024–T025 (matches milestone 157's T010 pattern).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the `graph_completeness/` submodule — types + BFS + reason codes — so US1 + US2 + US3 tasks all have a shared source of truth.

**⚠️ CRITICAL**: No user story task (T010+) can begin until T004–T009 land.

- [ ] T004 [P] Create the submodule scaffolding at `mikebom-cli/src/generate/graph_completeness/mod.rs`. File contents:
  - Module declaration `pub mod bfs;`, `pub mod reason_codes;`, `#[cfg(test)] mod tests;`
  - Public re-exports of `GraphCompletenessResult`, `GraphCompletenessValue`, `ReasonCode`, `compute_graph_completeness`.
  - Wire the new module into `mikebom-cli/src/generate/mod.rs` by adding `pub mod graph_completeness;` next to the existing `pub mod root_selector;` line.

- [ ] T005 [P] Create `mikebom-cli/src/generate/graph_completeness/reason_codes.rs` with:
  - The `ReasonCode` enum (8 variants) exactly matching data-model.md.
  - `impl ReasonCode { pub fn to_reason_string(&self) -> String }` matching contracts/graph-completeness-vocabulary.md's detail-format column.
  - Helper `pub fn join_reason_codes(codes: &[ReasonCode]) -> String` that produces the `; `-joined output per FR-012.
  - **Unit tests** (inline `#[cfg(test)] mod tests`): 8 tests, one per variant, asserting each `to_reason_string()` output matches the vocabulary contract exactly (byte-precise).

- [ ] T006 [P] Create `mikebom-cli/src/generate/graph_completeness/bfs.rs` with:
  - `pub(crate) struct EcosystemRootSet { roots: HashSet<PurlKey>, per_ecosystem: HashMap<String, PurlKey>, ecosystems_without_root: Vec<String> }` per data-model.md.
  - `pub(crate) fn build_ecosystem_root_set(components, selection) -> EcosystemRootSet` implementing R5 Step 1 + Step 2.
  - `pub(crate) fn pick_ecosystem_top(candidates: &[&ResolvedComponent]) -> Option<&ResolvedComponent>` reusing the milestone-127 workspace-root-first + LCP-fallback ladder.
  - `pub(crate) fn multi_source_bfs(seeds: &HashSet<PurlKey>, edges: &HashMap<PurlKey, Vec<PurlKey>>) -> HashSet<PurlKey>` implementing R5 Step 3.
  - **Unit tests** (inline): 4 tests — (a) empty components → empty visited; (b) single-root single-ecosystem BFS matches naive traversal; (c) two-ecosystem seed set BFS returns union; (d) `pick_ecosystem_top` prefers `mikebom:is-workspace-root == true` over LCP.

- [ ] T007 Create `mikebom-cli/src/generate/graph_completeness/mod.rs` public API `pub fn compute_graph_completeness(components: &[ResolvedComponent], dependency_edges: &HashMap<PurlKey, Vec<PurlKey>>, selection: &RootSelectionResult) -> GraphCompletenessResult` per data-model.md + contracts/reachability-algorithm.md Step 4 (classification). Include:
  - `pub struct GraphCompletenessResult { value, reason_codes, total_count, reachable_count, orphan_count }` — public struct.
  - `pub enum GraphCompletenessValue { Complete, Partial, Unknown }` + `impl GraphCompletenessValue { pub fn as_str(&self) -> &'static str }`.
  - The classification logic: build seed set (T006) → BFS (T006) → count reachable vs orphans → dispatch to reason codes → apply Q1 caution-first fallback (if orphans exist but no classifier fired, emit `unknown` not `partial`).
  - **Unit tests** (T004's `mod tests`): 3 classification tests — (a) empty components → `Complete` + zero counts; (b) all reachable + no losers → `Complete`; (c) unclassifiable gap → caution-first `unknown` (constructive: manually pass a broken edges map that leaves an orphan without any classifier match — asserts value is `Unknown`, not `Partial`).

- [ ] T008 Wire `compute_graph_completeness` into the CDX emission call site at `mikebom-cli/src/cli/scan_cmd.rs` — invoke immediately AFTER `select_root(...)` at line ~2067 and BEFORE `build_metadata(...)`. Thread the resulting `GraphCompletenessResult` into `build_metadata` as a new required parameter `graph_completeness: &GraphCompletenessResult`. Similarly thread into `spdx/v3_document.rs:870` (SPDX 3 emission call site) and the SPDX 2.3 emission call site (grep for `annotations::build_document_annotations` or similar). All three callers pass the SAME `GraphCompletenessResult` — one BFS pass per scan; result reused across formats.

- [ ] T009 Add FR-013 tracing log line in `mikebom-cli/src/cli/scan_cmd.rs` at the same point where `compute_graph_completeness` is called. Log shape per research §R8:
  ```rust
  tracing::info!(
      value = %result.value.as_str(),
      reachable_count = result.reachable_count,
      total_count = result.total_count,
      orphan_count = result.orphan_count,
      reason_codes = ?result.reason_codes,
      "graph completeness computed"
  );
  ```

**Checkpoint**: At end of Phase 2, all types + BFS + emission wiring exist. Nothing yet reads the annotation into the emitted SBOM — that's Phase 4. Nothing yet links losers to root — that's Phase 3. Both can proceed in parallel.

---

## Phase 3: User Story 1 (P1) — SBOM consumer sees a fully-connected graph for workspace monorepos

**Story Goal**: SBOM consumers viewing a workspace monorepo (e.g. test-podman-desktop) see ≥99% of components reachable from the root via BFS, up from the pre-158 baseline of 19.5%.

**Independent Test**: `mikebom sbom scan --path test-podman-desktop --format cyclonedx-json`; python3 BFS from `.metadata.component.bom-ref` reaches ≥99% of npm components. Delivers value INDEPENDENTLY of US2 (annotations) and US3 (goldens byte-identity).

- [ ] T010 [US1] Modify the CDX dependency emitter to add `RootSelectionResult.losers` to the root's `dependsOn`. Locate the emitter (grep `mikebom-cli/src/generate/cyclonedx/dependencies.rs` OR wherever CDX `dependencies[]` is assembled — likely `cyclonedx/mod.rs`). Insert a step that, WHEN `selection.losers.is_empty() == false`:
  1. Look up the root component's current `dependsOn` list.
  2. For each loser Purl in `selection.losers`, add its bom-ref (`.as_str()`) to the list.
  3. Sort + dedup the final list (matches CDX ordering conventions per milestone-071 parity).

- [ ] T011 [US1] Same linkage in SPDX 2.3 relationships. Locate `mikebom-cli/src/generate/spdx/relationships.rs` (or wherever SPDX 2.3 `relationships[]` is assembled — check for `DEPENDS_ON` emission). For each loser Purl, add a `Relationship { spdxElementId: root_spdx_id, relationshipType: "DEPENDS_ON", relatedSpdxElement: loser_spdx_id }`. Reuse the existing PURL→SPDXID mapping from the SPDX 2.3 packages emitter.

- [ ] T012 [US1] Same linkage in SPDX 3 relationships. Locate `mikebom-cli/src/generate/spdx/v3_relationships.rs` (or v3_document.rs). For each loser, add a `Relationship` element with `relationshipType = "dependsOn"` and `from = root_spdxid, to = [loser_spdxid]` in the `@graph`.

- [ ] T013 [US1] Unit test in `mikebom-cli/src/generate/graph_completeness/tests.rs` — synthesize a 2-peer workspace (root + 2 peers, each peer with 2 deps). After running the full emission pipeline, assert:
  - Root's `dependsOn` in CDX contains both peer PURLs (sorted).
  - BFS reachability from root = 5/5 components (2 peers + 2 deps of peer1 + 2 deps of peer2, but peer2's deps may overlap peer1's — use unique dep names).
  - `GraphCompletenessResult.value == Complete`.

- [ ] T014 [US1] Unit test — synthesize a 12-peer workspace (test-podman-desktop-shape). Simulate the podman-desktop losers list: 12 unique peer PURLs. Assert:
  - CDX root's `dependsOn` contains all 12 peer PURLs.
  - Multi-root BFS visits root + 12 peers + their transitive closure.
  - Reachability count matches the synthesized total component count.

- [ ] T015 [US1] Unit test for peer-already-present dedup (test-podman-desktop currently already links 1 of 12 peers). Synthesize: root already has peer1 in dependsOn; losers list contains peer1 (again) + peer2 + peer3. Assert:
  - Final root dependsOn contains peer1, peer2, peer3 — each exactly once.
  - Sort order stable (peer1, peer2, peer3 in lex order).

- [ ] T016 [US1] Unit test for leaf peer (peer with zero deps of its own). Synthesize: root + 1 peer that has NO outbound edges. Assert:
  - Peer is in root's dependsOn.
  - BFS reaches the peer (it's a reachable leaf, not an orphan).
  - Reachability = 2/2 → `Complete`.

- [ ] T017 [US1] Unit test for URL-shaped peer version (milestone-157 `argo-ui@https://…tar.gz` regression). Synthesize a peer with `pkg:npm/argo-ui@https://codeload.github.com/foo/tar.gz/abc123`. Assert:
  - Peer emits correctly as a component (no PURL rejection).
  - Peer appears in root's dependsOn.
  - Reachable via BFS.

**Checkpoint**: End of US1. The concrete workspace-peer linkage works end-to-end. `test-podman-desktop` scan (run at T027 in Polish) should now show ≥99% BFS reachability.

---

## Phase 4: User Story 2 (P2) — SBOM consumer detects graph coverage programmatically via annotations

**Story Goal**: Every SBOM emitted by mikebom carries the `mikebom:graph-completeness` annotation at document scope. When `partial` or `unknown`, `mikebom:graph-completeness-reason` also present.

**Independent Test**: `jq -e '.metadata.properties | any(.name == "mikebom:graph-completeness")'` returns true on ANY mikebom-emitted CDX SBOM.

- [ ] T018 [US2] CDX metadata.rs: emit `mikebom:graph-completeness` document-scope property. Insert into `build_metadata` at `mikebom-cli/src/generate/cyclonedx/metadata.rs` immediately after the milestone-127 `mikebom:root-selection-heuristic` block (line ~436). Shape:
  ```rust
  properties.push(json!({
      "name": "mikebom:graph-completeness",
      "value": graph_completeness.value.as_str(),
  }));
  ```

- [ ] T019 [US2] CDX metadata.rs: emit `mikebom:graph-completeness-reason` conditionally. Immediately after T018:
  ```rust
  if graph_completeness.value != GraphCompletenessValue::Complete && !graph_completeness.reason_codes.is_empty() {
      properties.push(json!({
          "name": "mikebom:graph-completeness-reason",
          "value": reason_codes::join_reason_codes(&graph_completeness.reason_codes),
      }));
  }
  ```

- [ ] T020 [US2] SPDX 2.3 annotations.rs: emit document-scope `mikebom:graph-completeness` Annotation. Insert into `mikebom-cli/src/generate/spdx/annotations.rs` immediately after the milestone-127 `mikebom:root-selection-heuristic` block (line ~512). Shape:
  ```rust
  annotations.push(json!({
      "annotator": format!("Tool: mikebom-{}", env!("CARGO_PKG_VERSION")),
      "annotationDate": timestamp,
      "annotationType": "OTHER",
      "comment": format!("mikebom:graph-completeness={}", graph_completeness.value.as_str()),
  }));
  ```

- [ ] T021 [US2] SPDX 2.3 annotations.rs: emit `mikebom:graph-completeness-reason` conditionally, same file, same pattern as T019.

- [ ] T022 [US2] SPDX 3 v3_annotations.rs: emit document-scope `mikebom:graph-completeness` Annotation. Insert into `mikebom-cli/src/generate/spdx/v3_annotations.rs` immediately after the milestone-127 `mikebom:root-selection-heuristic` block (line ~474). SPDX 3 shape uses `type: "Annotation"`, `subject: "SPDXRef-DOCUMENT"`, `statement: "mikebom:graph-completeness=<value>"`.

- [ ] T023 [US2] SPDX 3 v3_annotations.rs: emit `mikebom:graph-completeness-reason` conditionally, same file, same pattern.

- [ ] T024 [US2] Register parity catalog rows C70 + C71 in `mikebom-cli/src/parity/extractors/mod.rs`. Insert after the existing C69 entry at line ~375:
  ```rust
  ParityExtractor { row_id: "C70", label: "mikebom:graph-completeness",        cdx: c70_cdx, spdx23: c70_spdx23, spdx3: c70_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
  ParityExtractor { row_id: "C71", label: "mikebom:graph-completeness-reason", cdx: c71_cdx, spdx23: c71_spdx23, spdx3: c71_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
  ```

- [ ] T025 [P] [US2] Add the 6 per-format parity extractors: 2 in `mikebom-cli/src/parity/extractors/cdx.rs` (using `cdx_anno!` macro, scope=`document`), 2 in `mikebom-cli/src/parity/extractors/spdx2.rs`, 2 in `mikebom-cli/src/parity/extractors/spdx3.rs`. All 6 follow the milestone-127 C69 pattern exactly.

- [ ] T026 [US2] Q1 caution-first unit test in `graph_completeness/tests.rs` — synthesize a `RootSelectionResult` with `subject = ResolvedRootSubject::MainModule(idx)` where `idx` points to a component NOT in `components[]` (impossible state; caution-first should catch it). Assert:
  - `GraphCompletenessResult.value == Unknown`.
  - No `partial` emission.

- [ ] T027 [US2] Q2 orphan-classification unit test — synthesize: root + 3 reachable components + 1 orphan component (in `components[]`, not in losers, no edges to it). Assert:
  - `value == Partial`.
  - `reason_codes` contains exactly one `OrphanedComponentsDetected { orphan_count: 1 }`.
  - `join_reason_codes(...)` produces `orphaned-components-detected: 1 component(s) not reachable from root`.

- [ ] T028 [US2] Q3 multi-root BFS unit test — synthesize 2-ecosystem repo (npm + gem), each with a main-module. npm root reaches 3 components; gem root reaches 2 components. Assert:
  - Total = 7 (1 npm root + 1 gem root + 3 npm deps + 2 gem deps).
  - Reachable = 7 via multi-source BFS.
  - `value == Complete`.
  - No reason codes.

- [ ] T029 [US2] Q3 combined-reason unit test — synthesize a repo where npm root can't be identified (no npm main-module component) + orphan exists. Assert:
  - `value == Partial`.
  - `reason_codes` contains BOTH `MultiEcosystemPartialRoot { ecosystems: vec!["npm".to_string()] }` AND `OrphanedComponentsDetected`.
  - `join_reason_codes(...)` produces `multi-ecosystem-partial-root: npm; orphaned-components-detected: N`.

- [ ] T030 [US2] SC-008 integration test at `mikebom-cli/tests/graph_completeness_workspace_bfs.rs`. Synthesize a `test-podman-desktop`-shape workspace testbed via `tempfile::tempdir()` + `std::fs::write` (mirrors milestone-157's `pnpm_v9_synthetic_argo_cd_shape` at line 58). Invoke the release binary; parse the emitted CDX; assert:
  - `.metadata.properties[] | select(.name == "mikebom:graph-completeness") | .value == "complete"`.
  - BFS from root reaches 100% of components (mirroring quickstart.md Scenario 1's Python check).
  - No `mikebom:graph-completeness-reason` property present (SC-003 conditional).

**Checkpoint**: End of US2. Every emitted SBOM carries the annotation. Milestone-071 parity catalog gate (`cargo test parity_symmetric`) passes symmetrically across CDX/SPDX 2.3/SPDX 3.

---

## Phase 5: User Story 3 (P3) — Single-package repos byte-identical + one property added

**Story Goal**: Milestone-090 non-workspace goldens diff pre-158 vs post-158 shows exactly ONE property added (`mikebom:graph-completeness = complete`) and zero other bytes changed.

**Independent Test**: `diff /tmp/158-pre-goldens/<eco>.cdx.json mikebom-cli/tests/fixtures/golden/cyclonedx/<eco>.cdx.json | grep -E '^[<>]' | wc -l == 2` (one `<`, one `>` for the property line).

- [ ] T031 [US3] Regenerate all 11 milestone-090 CDX goldens (`alpine`, `apk`, `cargo`, `cyclonedx-source`, `deb`, `gem`, `maven`, `npm`, `pip`, `rpm`, `spdx-source`) by running the existing `./scripts/regenerate-golden.sh <ecosystem>` (or equivalent — grep for `regenerate-golden` in `Makefile` / `scripts/` / `justfile`).

- [ ] T032 [US3] Regenerate all 11 milestone-090 SPDX 2.3 goldens (same 11 ecosystems).

- [ ] T033 [US3] Regenerate all 11 milestone-090 SPDX 3 goldens (same 11 ecosystems).

- [ ] T034 [US3] SC-002 byte-identity guard: for each of the 33 regenerated goldens, run `diff /tmp/158-pre-goldens/<path> <new-path> | grep -cE '^[<>]'` and confirm the count is EXACTLY 2 for CDX (one `<`, one `>` — the added property line pair) OR the equivalent minimal delta for SPDX 2.3/3. If ANY golden shows more than 2 diff lines (or the wrong delta), STOP and investigate — this signals an unintended byte change per SC-002.

**Checkpoint**: End of US3. Byte-identity guard verified. All 11 non-workspace goldens changed by exactly the addition of the new annotation.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Empirical verification, CHANGELOG, pre-PR gate, and PR closure.

- [ ] T035 SC-001 empirical: scan `test-podman-desktop` with the built binary. Compute BFS reachability from `.metadata.component.bom-ref`. Assert reachable-count ≥ 99% of npm components (target: 100%). Pre-158 baseline was 19.5% (552/2835). Record actual measurement + delta.

- [ ] T036 SC-004 empirical: scan all 5 `kusari-sandbox/test-*` repos with the built binary. For each, read `mikebom:graph-completeness` + optional `mikebom:graph-completeness-reason`. Verify against SC-004 predicted values:
  - test-podman-desktop: `complete`.
  - test-guac-visualizer: `complete`.
  - test-rails: `partial` with reason likely combining `orphaned-components-detected` + possibly `multi-ecosystem-partial-root: npm`.
  - test-podman: outcome per Q1 caution-first + `go-transitive-coverage-degraded` if applicable.
  - test-kubernetes: outcome per Q1 caution-first + `go-workspace-mode-anomaly` if applicable.

- [ ] T037 SC-006 pre-PR gate: run `./scripts/pre-pr.sh`. MUST pass `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace`. Per the milestone-157 `feedback_prepr_gate_bails_on_first_failure` memory: use `--no-fail-fast` if invoking `cargo test` manually, and enumerate every `^---- .+ stdout ----` line before claiming green.

- [ ] T038 SC-009 CHANGELOG entry: add a `[Unreleased]` section entry to `CHANGELOG.md` following the milestone-157 template + research §R9 shape. Include: bug summary + Q1/Q2/Q3 clarification summaries + SC-001 empirical numbers + consumer jq recipe (per R9) + wire-format-cleanliness note (parity catalog + no new Cargo deps).

- [ ] T039 SC-011 issue closure: verify the `impl(158)` commit message will include `closes #492`. Update the milestone-158 requirements checklist at `specs/158-graph-completeness/checklists/requirements.md` with implementation-completion notes (mirrors milestone-157's T015 pattern): measured SC-001 percentage from T035, SC-004 per-repo values from T036, SC-002 diff-line counts from T034, any surprises encountered during impl.

- [ ] T040 Commit `impl(158)`: `mikebom-cli/src/generate/graph_completeness/` + updated `metadata.rs` + `annotations.rs` + `v3_annotations.rs` + `dependencies.rs` + `relationships.rs` + `v3_relationships.rs` + parity catalog files + regenerated 33 goldens + integration test. Commit message: `impl(158): workspace-peer linkage + graph-completeness annotations (closes #492)`. Follow milestone-157's HEREDOC + `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` convention.

- [ ] T041 Commit `docs(158)`: `CHANGELOG.md` + `specs/158-graph-completeness/checklists/requirements.md` (updated with impl notes).

- [ ] T042 Push to `fork` remote: `git push -u fork 158-graph-completeness`. Then open PR against `kusari-oss/mikebom` `main` with title `impl(158): workspace-root peer linkage + graph-completeness annotations (#492)` and body per the milestone-157 PR body template — summary, T035 empirical table, test plan checklist, consumer jq recipe, wire-format cleanliness note.

**Checkpoint**: PR open. CI green. Ready for review + merge.

---

## Dependencies

**Story completion order**: US1 (P1) and US2 (P2) are logically independent but both are needed to hit SC-001 on test-podman-desktop AND to emit `complete` on that repo. US3 (P3) validates that non-workspace repos remain byte-identical.

**Task ordering constraints**:

- **Setup (T001–T003)** → blocks nothing except empirical measurement targets in Polish.
- **Foundational (T004–T009)** → blocks ALL of US1 + US2 + US3.
- **US1 (T010–T017)** → can be authored in parallel with US2 (different files) but MUST land together in the impl commit (single Rust compile unit).
- **US2 (T018–T030)** → can be authored in parallel with US1.
- **US3 (T031–T034)** → requires US1 + US2 landed so goldens regenerate with the new emission.
- **Polish (T035–T042)** → requires everything above landed.

**Parallel-execution opportunities**:

- T004 + T005 + T006 (three different new files, no cross-references at authoring time).
- T010 + T011 + T012 (three different format emitters, independent).
- T013 + T014 + T015 + T016 + T017 (five independent unit tests in the same tests.rs; MAY be parallelizable across separate commits but likely simpler to do in-place).
- T018 + T019 (both in metadata.rs — sequential).
- T020 + T021 (both in annotations.rs — sequential).
- T022 + T023 (both in v3_annotations.rs — sequential).
- T024 + T025 (mod.rs then per-format extractors — sequential).
- T031 + T032 + T033 (three format golden regenerations — parallel).

**Independent testing per US**:

- **US1**: T013–T017 unit tests + T035 empirical are all runnable without T018+ (US2's annotations don't affect BFS reachability of the linked peers).
- **US2**: T026–T030 unit tests + T036 empirical are runnable without T010–T017 landing IF a synthetic `RootSelectionResult` is passed in (the T007 API accepts any `RootSelectionResult`, not requiring linkage-already-happened).
- **US3**: T031–T034 requires US1 + US2 landed — the goldens reflect the full end-state.

## Implementation Strategy

**MVP scope**: US1 alone would deliver the primary bug fix — SBOM consumers on test-podman-desktop go from 19.5% → ≥99% reachability. But without US2 they have no programmatic signal to gate on completeness. The recommended shipping order:

1. Land Setup + Foundational + US1 in ONE PR (fixes the bug, no user-visible annotation yet). BUT — this would fail SC-003 (universal annotation presence). So NOT recommended as a split.
2. Land the whole milestone in one PR (Setup + Foundational + US1 + US2 + US3 + Polish). This is the milestone-157 model + matches how the plan.md is structured.

**Recommended**: Single-PR milestone matching milestone-157's shipping pattern. Total task count: 42 tasks organized across 6 phases.

**Post-merge follow-ups** (out of scope for milestone 158, per spec):

- Milestone 159+: implement pnpm/yarn npm-alias syntax handling (issue #493).
- Milestone 160+: fix Go workspace-mode false edges (issue #494) — completeness annotation would flip from `partial` (with `go-workspace-mode-anomaly` reason) to `complete` for test-kubernetes.
- Milestone 161+: fix Go transitive coverage gap (issue #495) — same as above for test-podman.
- Milestone 162+: emit synthetic components for declared Ruby built-in gems OR emit unresolved-edge markers (issue #496).

## Format Validation

Every task above has:

- ✅ Checkbox `- [ ]`
- ✅ Task ID `T001`–`T042`
- ✅ `[P]` marker where parallelizable
- ✅ `[US1]` / `[US2]` / `[US3]` labels on the correct story-phase tasks; no story label on Setup / Foundational / Polish tasks.
- ✅ Exact file paths in every description (either an existing file to modify OR a new file to create).

42 tasks total. 20 tasks parallelizable ([P] marker or in independent files per phase). 8 SC-001 through SC-011 verification steps embedded in Polish.

## Task counts per phase

- Phase 1 (Setup): 3 tasks (T001–T003).
- Phase 2 (Foundational): 6 tasks (T004–T009).
- Phase 3 (US1, P1): 8 tasks (T010–T017).
- Phase 4 (US2, P2): 13 tasks (T018–T030).
- Phase 5 (US3, P3): 4 tasks (T031–T034).
- Phase 6 (Polish): 8 tasks (T035–T042).

**Total**: 42 tasks.
