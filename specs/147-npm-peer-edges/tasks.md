---
description: "Task list for milestone 147 — npm peerDependencies edge emission + peer-kind annotation (closes Trivy-comparison orphan gap)"
---

# Tasks: milestone 147 — npm peerDependencies edge emission + peer-kind annotation

**Input**: Design documents from `/specs/147-npm-peer-edges/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: Spec mandates tests via SC-002 + SC-003 + SC-004 + SC-005. Test tasks are included inline alongside implementation tasks (per the project's existing Rust convention of in-file `#[cfg(test)] mod tests`).

**Organization**: US1 (edge emission) and US2 (peer-kind annotation) are **shippable independently**. US1 alone closes the orphan gap (5 → 0 on the audit corpus); US2 layers the annotation on top. Phase 3 ships US1 standalone; Phase 4 ships US2 as a pure addition. No foundational types or signature plumbing needed (zero new types per data-model.md).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Maps to user story from spec.md (US1, US2)
- Paths absolute under repo root `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

Single Rust source file edited: `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs`. Tests in the same file's `#[cfg(test)] mod tests` block. Parity-catalog row in `mikebom-cli/src/parity/extractors/`. Docs row in `docs/reference/sbom-format-mapping.md`. Three potential golden refreshes under `mikebom-cli/tests/fixtures/golden/`.

---

## Phase 1: Setup

**Purpose**: Verify baseline before any code change.

- [ ] T001 Confirm baseline pre-PR gate is green on branch `147-npm-peer-edges`. Run `./scripts/pre-pr.sh` from repo root. Expected: clippy `--workspace --all-targets -- -D warnings` clean; `cargo test --workspace` passes except for the pre-existing local-environment `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` failure documented in milestone-144 T001. If anything ELSE fails, halt and investigate before proceeding.

---

## Phase 2: Foundational

**Purpose**: None required for this milestone. US1 and US2 share the same code site (npm `parse_package_lock`) but US2 is purely additive on top of US1 — they're sequenced via the per-story phases, not via foundational plumbing. Zero new types, zero new Cargo dependencies (per data-model.md).

(No tasks in this phase. Proceed directly to Phase 3.)

---

## Phase 3: User Story 1 - npm peerDependencies emit as DEPENDS_ON edges (Priority: P1) 🎯 MVP

**Goal**: Extend the npm `parse_package_lock` section-list to walk `peerDependencies` alongside the three existing sections, closing the orphan gap (5 → 0 on the looker-frontend audit corpus). Rewrite the misleading comment.

**Independent Test**: Build a minimal lockfile with a peer-dep that's also installed (e.g., `mlly` declares `pathe` as peer; `pathe` is in `packages` map). Run the reader. Assert the consumer's `depends` Vec contains the peer.

### Implementation for User Story 1

- [ ] T002 [US1] Extend the section list at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs:177-181` to include `"peerDependencies"` as a fourth entry. Specifically, replace the current 3-element slice:

  ```rust
  for section in &[
      "dependencies",
      "devDependencies",
      "optionalDependencies",
  ] {
  ```

  with the 4-element form:

  ```rust
  for section in &[
      "dependencies",
      "devDependencies",
      "optionalDependencies",
      "peerDependencies",
  ] {
  ```

  The order matters per FR-003 precedence: regular sections first → a name appearing in BOTH peerDependencies and a regular section gets resolved via the regular section first, and the existing `BTreeMap::entry(...)` pattern at lines 211-223 prevents double-emission (the regular section's insert wins; the subsequent peerDependencies iteration's `Entry::Occupied` branch only upgrades bare-name → version-pinned, never overwrites with a peer-section value of the same shape). This satisfies FR-003 for free.

- [ ] T003 [US1] Rewrite the doc-comment at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs:168-176` per research §E. Replace the current comment (which incorrectly asserts "Trivy and syft also skip peer-edges" and contradicts the upper comment at lines 149-160 which already documents intent to walk ALL FOUR sections). Use the exact text from research §E:

  ```rust
  // Walk all four standard npm dep sections. peerDependencies were
  // historically skipped (matching Syft's behavior pre-147), but
  // milestone 147 enables them to close the orphan gap surfaced by
  // the Trivy comparison on the looker-frontend lockfile (Trivy
  // emits peer-edges as DEPENDS_ON; 5 mikebom orphans dropped to 0
  // matching Trivy).
  //
  // The install-vs-functional distinction is preserved via a
  // mikebom:peer-edge-targets annotation on the source component
  // listing the PURLs of peer-driven edges (Constitution Principle V
  // parity-bridging — CDX/SPDX 2.3/SPDX 3 all lack a native carrier
  // for per-edge peer-kind metadata). Documented in
  // docs/reference/sbom-format-mapping.md.
  //
  // FR-002 (no phantom edges for unmet peers) is satisfied for free
  // via resolve_dep_via_node_modules_walk returning None when the
  // peer isn't installed at any level.
  ```

- [ ] T004 [US1] Replace the existing unit test at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs:680-711` (`peer_dependencies_are_skipped_declarative_not_install`) with a new test asserting the milestone-147 behavior. Rename to `peer_dependencies_emit_edges_md147`. The test setup remains the same (mlly declares pathe as peer; pathe is installed); but the assertions flip from `mlly.depends.is_empty()` to `mlly.depends` containing the version-pinned `pathe 2.0.3` entry:

  ```rust
  #[test]
  fn peer_dependencies_emit_edges_md147() {
      // Milestone 147 (closes Trivy-comparison orphan gap):
      // peerDependencies emit as DEPENDS_ON edges when the peer is
      // actually installed in the lockfile. Pre-147 the edge was
      // skipped (matching Syft); post-147 mikebom matches Trivy's
      // npm-7+ auto-install convention.
      //
      // Reproducer: mlly declares `pathe` ONLY via peerDependencies
      // (no regular dependency) AND a `node_modules/mlly/node_modules/
      // pathe` install exists. Post-147 the edge emits to the
      // nested install (per the existing resolve_dep_via_node_modules_walk
      // ladder).
      let lockfile = serde_json::json!({
          "lockfileVersion": 3,
          "packages": {
              "node_modules/pathe": { "version": "1.1.2" },
              "node_modules/mlly": {
                  "version": "1.0.0",
                  "peerDependencies": { "pathe": "^2.0.0" }
              },
              "node_modules/mlly/node_modules/pathe": { "version": "2.0.3" }
          }
      });
      let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
      let mlly = entries.iter().find(|e| e.name == "mlly").expect("mlly");
      assert_eq!(
          mlly.depends,
          vec!["pathe 2.0.3".to_string()],
          "milestone-147: `mlly` MUST emit a peer-edge to pathe@2.0.3; got: {:?}",
          mlly.depends
      );
  }
  ```

  Note: the US2 annotation assertion is added in a separate test in Phase 4 (T009).

- [ ] T005 [P] [US1] Add unit test `unmet_peer_emits_no_edge_md147` in the same `mod tests` block. Tests FR-002: when a peer is declared but NOT present in the lockfile's `packages` map, no edge is emitted. Pattern:

  ```rust
  #[test]
  fn unmet_peer_emits_no_edge_md147() {
      let lockfile = serde_json::json!({
          "lockfileVersion": 3,
          "packages": {
              "node_modules/mlly": {
                  "version": "1.0.0",
                  "peerDependencies": { "pathe": "^2.0.0" }
              }
              // No node_modules/pathe entry — unmet peer.
          }
      });
      let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
      let mlly = entries.iter().find(|e| e.name == "mlly").expect("mlly");
      assert!(
          mlly.depends.is_empty()
              || !mlly.depends.iter().any(|d| d.contains("pathe")),
          "FR-002: unmet peer must NOT emit an edge; got: {:?}",
          mlly.depends
      );
  }
  ```

  Covers SC-005.

- [ ] T006 [P] [US1] Add unit test `peer_already_in_regular_deps_takes_precedence_md147` in the same `mod tests` block. Tests FR-003: when the same name appears in BOTH `peerDependencies` AND a regular section (`dependencies` here), the regular declaration wins (one edge emitted, not two). Pattern:

  ```rust
  #[test]
  fn peer_already_in_regular_deps_takes_precedence_md147() {
      let lockfile = serde_json::json!({
          "lockfileVersion": 3,
          "packages": {
              "node_modules/foo": { "version": "1.0.0" },
              "node_modules/parent": {
                  "version": "1.0.0",
                  "dependencies": { "foo": "^1.0.0" },
                  "peerDependencies": { "foo": "^1.0.0" }
              }
          }
      });
      let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
      let parent = entries.iter().find(|e| e.name == "parent").expect("parent");
      // Exactly one edge to foo (not two from the duplicate sections).
      let foo_edges: Vec<&String> = parent.depends.iter().filter(|d| d.contains("foo")).collect();
      assert_eq!(
          foo_edges.len(),
          1,
          "FR-003: duplicate dep across regular+peer must emit ONCE; got: {:?}",
          parent.depends
      );
  }
  ```

  Covers SC-004.

**Checkpoint**: After Phase 3, US1 is fully functional. `cargo +stable test -p mikebom --bin mikebom package_lock` MUST pass (the replaced test from T004 + new T005/T006 tests fire green). Manual smoke: scan the looker-frontend audit corpus and confirm 5 orphans drop to 0 (per quickstart §Scenario 1).

---

## Phase 4: User Story 2 - peer-driven edges are annotated so consumers can filter (Priority: P2)

**Goal**: Layer the `mikebom:peer-edge-targets` annotation on top of US1's edge emission so consumers can distinguish peer-driven edges from regular dependencies.

**Independent Test**: Scan a lockfile with a known peer-edge (e.g., from T004's fixture). Inspect the source component's `extra_annotations` and assert `mikebom:peer-edge-targets` is present with the peer's PURL as an array element.

### Implementation for User Story 2

- [ ] T007 [US2] Extend `parse_package_lock` to track peer-edge PURL targets alongside `depends_set`. Add a `BTreeSet<String>` (one per per-entry iteration) named `peer_edge_targets` declared near `depends_set` at `package_lock.rs:166-167`.

  **Implementation pattern** (resolves /speckit-analyze findings M1 + M2): T002 extends the existing unified section-iteration loop with `"peerDependencies"` as a 4th element. Inside that loop body, gate the peer-target tracking on `*section == "peerDependencies"` AND refactor the resolve call for the peer iteration to use an explicit `if let Some(version) = resolve_dep_via_node_modules_walk(...)` block — NOT the existing `.unwrap_or_else(|| dep_name.clone())` pattern that the three regular sections use. The two gating conditions differ because unmet peers per FR-002 must produce NO edge AND NO peer-target tracking entry, whereas the existing `.unwrap_or_else` pattern inserts unmet bare-names into `depends_set` (then drops them downstream).

  Concrete pattern inside the section loop:

  ```rust
  if *section == "peerDependencies" {
      // Milestone 147 US2: gate both edge emission AND peer-target
      // tracking on the Some/None return — FR-002 (no phantom edges
      // for unmet peers) + FR-004 (peer-target annotation only for
      // ACTUALLY-resolved peers).
      if let Some(version) = resolve_dep_via_node_modules_walk(
          path_key, dep_name, &path_versions,
      ) {
          let resolved = format!("{dep_name} {version}");
          // Existing Entry::Vacant/Occupied handling — regular
          // section wins via the existing pattern if dep_name is
          // already in depends_set (FR-003 precedence). ONLY add
          // to peer_edge_targets in the Vacant arm.
          use std::collections::btree_map::Entry;
          if let Entry::Vacant(v) = depends_set.entry(dep_name.clone()) {
              v.insert(resolved);
              // Peer is installed AND not already in regular
              // sections → track the PURL for the annotation.
              peer_edge_targets.insert(format!("pkg:npm/{dep_name}@{version}"));
          }
          // Entry::Occupied: regular section already handled this
          // name; skip peer-target tracking per FR-003.
      }
      // resolve returned None → unmet peer; no edge, no annotation
      // entry per FR-002. Skip the regular-section unwrap_or_else
      // bare-name fallback for the peer section specifically.
      continue;
  }
  // Existing 3-section handling for dependencies/devDependencies/
  // optionalDependencies — unchanged from pre-147.
  let resolved = resolve_dep_via_node_modules_walk(
      path_key, dep_name, &path_versions,
  )
  .map(|version| format!("{dep_name} {version}"))
  .unwrap_or_else(|| dep_name.clone());
  // ... rest of existing Entry::Vacant/Occupied handling ...
  ```

  Important: the `continue` skips the regular `.unwrap_or_else` path for peer iterations specifically. This means T005's `unmet_peer_emits_no_edge_md147` test asserts BOTH the FR-002 edge gate AND the FR-004 annotation gate are correctly tied to the same Some/None return value. The `BTreeSet` collection gives sort + dedupe for free per research §A.

- [ ] T008 [US2] After the section-walk loop completes, stamp the annotation conditionally. Insert between the loop's closing `}` and the subsequent code (around line 225 or wherever the loop ends post-T002):

  ```rust
  if !peer_edge_targets.is_empty() {
      let sorted_arr: Vec<serde_json::Value> = peer_edge_targets
          .into_iter()
          .map(serde_json::Value::String)
          .collect();
      // BTreeSet iteration yields lex-ascending strings → array is
      // naturally sorted (research §A + milestone-145 paths_str.sort()
      // precedent).
      extra_annotations.insert(
          "mikebom:peer-edge-targets".to_string(),
          serde_json::Value::Array(sorted_arr),
      );
  }
  // When peer_edge_targets is empty: OMIT the annotation key per FR-005.
  ```

  The `extra_annotations` BTreeMap is the existing per-`PackageDbEntry` channel that all three SBOM emitters iterate (CDX `cyclonedx/builder.rs:1086-1098`, SPDX 2.3 `spdx/annotations.rs:371`, SPDX 3 `spdx/v3_annotations.rs:332`) — zero call-site changes needed elsewhere.

- [ ] T009 [P] [US2] Add unit test `peer_edge_targets_annotation_present_md147` in `package_lock.rs#mod tests`. Builds on the same `mlly`-declares-`pathe` fixture as T004; asserts the annotation:

  ```rust
  #[test]
  fn peer_edge_targets_annotation_present_md147() {
      let lockfile = serde_json::json!({
          "lockfileVersion": 3,
          "packages": {
              "node_modules/pathe": { "version": "1.1.2" },
              "node_modules/mlly": {
                  "version": "1.0.0",
                  "peerDependencies": { "pathe": "^2.0.0" }
              },
              "node_modules/mlly/node_modules/pathe": { "version": "2.0.3" }
          }
      });
      let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
      let mlly = entries.iter().find(|e| e.name == "mlly").expect("mlly");
      let anno = mlly
          .extra_annotations
          .get("mikebom:peer-edge-targets")
          .expect("milestone-147: mlly MUST carry mikebom:peer-edge-targets annotation");
      assert_eq!(
          anno,
          &serde_json::json!(["pkg:npm/pathe@2.0.3"]),
          "FR-004: peer-edge-targets value must be a native JSON array of PURL strings; got: {anno:?}"
      );
  }
  ```

  Covers SC-003.

- [ ] T010 [P] [US2] Add unit test `peer_annotation_omitted_when_set_empty_md147` in the same `mod tests`. Tests FR-005: a component with no peer-driven edges has NO `mikebom:peer-edge-targets` key in `extra_annotations`:

  ```rust
  #[test]
  fn peer_annotation_omitted_when_set_empty_md147() {
      // Package with regular deps only — no peerDependencies declared.
      let lockfile = serde_json::json!({
          "lockfileVersion": 3,
          "packages": {
              "node_modules/foo": { "version": "1.0.0" },
              "node_modules/parent": {
                  "version": "1.0.0",
                  "dependencies": { "foo": "^1.0.0" }
              }
          }
      });
      let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
      let parent = entries.iter().find(|e| e.name == "parent").expect("parent");
      assert!(
          !parent.extra_annotations.contains_key("mikebom:peer-edge-targets"),
          "FR-005: components with zero peer-driven edges MUST NOT carry the annotation; got: {:?}",
          parent.extra_annotations
      );
  }
  ```

- [ ] T011 [P] [US2] Add unit test `peer_edge_targets_array_is_sorted_alphabetically_md147` in the same `mod tests`. Tests research §A's alphabetical-sort guarantee:

  ```rust
  #[test]
  fn peer_edge_targets_array_is_sorted_alphabetically_md147() {
      let lockfile = serde_json::json!({
          "lockfileVersion": 3,
          "packages": {
              "node_modules/axios":   { "version": "1.0.0" },
              "node_modules/lodash":  { "version": "4.0.0" },
              "node_modules/react":   { "version": "17.0.0" },
              "node_modules/parent": {
                  "version": "1.0.0",
                  "peerDependencies": {
                      "react":  "^17.0.0",
                      "lodash": "^4.0.0",
                      "axios":  "^1.0.0"
                  }
              }
          }
      });
      let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
      let parent = entries.iter().find(|e| e.name == "parent").expect("parent");
      assert_eq!(
          parent.extra_annotations.get("mikebom:peer-edge-targets").expect("annotation present"),
          &serde_json::json!([
              "pkg:npm/axios@1.0.0",
              "pkg:npm/lodash@4.0.0",
              "pkg:npm/react@17.0.0"
          ]),
          "research §A: peer-edge-targets MUST be alphabetically sorted (BTreeSet collection order)"
      );
  }
  ```

**Checkpoint**: After Phase 4, US2 is fully covered. `cargo test -p mikebom --bin mikebom package_lock::tests::peer_` shows 5 tests fire green (4 milestone-147 tests + the unmet-peer guard from T005).

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Parity-catalog row addition, docs row, golden audit + refresh, pre-PR gate, commit.

- [ ] T012 [P] Add a new parity-catalog row for `mikebom:peer-edge-targets` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/parity/extractors/`. First grep `grep -n "row_id:" mikebom-cli/src/parity/extractors/mod.rs | tail -3` to find the next available C-number (likely C97). Add the row in `mod.rs` per the milestone-105/144 pattern:

  ```rust
  ParityExtractor {
      row_id: "C97",  // verify next number; bump if collision
      label: "mikebom:peer-edge-targets",
      cdx: c97_cdx,
      spdx23: c97_spdx23,
      spdx3: c97_spdx3,
      directional: Directionality::SymmetricEqual,
      order_sensitive: false,
  },
  ```

  Then add three sibling `c97_cdx` / `c97_spdx23` / `c97_spdx3` extractor functions in `cdx.rs` / `spdx2.rs` / `spdx3.rs`. Each extractor follows the `cdx_anno!(c97_cdx, "mikebom:peer-edge-targets", component)` macro pattern used by neighboring annotation-extraction rows (search for nearby `_anno!` invocations in each file to match the exact macro syntax). Covers SC-002.

- [ ] T013 [P] Update `/Users/mlieberman/Projects/mikebom/docs/reference/sbom-format-mapping.md` to add a new row for `mikebom:peer-edge-targets`. Per Constitution Principle V's documentation requirement for parity-bridging `mikebom:*` properties: cite the milestone (`milestone 147 / issue #470-comparison-derived`), the per-format wire shape, and the explicit standards-native-alternatives-rejected justification (CDX 1.6 no per-edge metadata slot; SPDX 2.3 no `PEER_DEPENDENCY_OF` typed relationship; SPDX 3 `LifecycleScopedRelationship.scope` enum lacks `peer` value). Use the existing row format in that doc as the template.

- [ ] T014 Audit existing byte-identity goldens for npm-bearing fixtures. Run `grep -rl 'peerDependencies' mikebom-cli/tests/fixtures/golden/ /Users/mlieberman/.cache/mikebom/fixtures/*/npm*/package-lock.json /Users/mlieberman/.cache/mikebom/fixtures/*/polyglot-monorepo/frontend/package-lock.json 2>/dev/null`. Per research §C audit performed at /speckit-plan time, NO existing fixture contains a `peerDependencies` declaration — golden refresh will likely produce empty diffs. If the audit confirms zero matches, document that finding in the PR description and skip to T016. If the audit surfaces matches (sibling-fixture-repo changed between /plan and /implement), refresh + inspect those goldens per T016's pattern.

- [ ] T015 Per /speckit-analyze finding C2: **extend an existing npm fixture's `package-lock.json` to include a peerDependencies declaration on at least one package**, so byte-identity goldens exercise the milestone-147 code path. Per T014 audit, no existing fixture currently has peer-deps — without this extension, T015b's golden refresh produces empty diffs (vacuously satisfies SC-006) and the new C97 parity catalog row registers but never gets exercised in goldens (vacuously satisfies SC-002).

  Recommended target fixture: a sibling-repo fixture (per milestone 090 fixture-cache layout at `~/.cache/mikebom/fixtures/<sha>/`) since those are the ones our golden tests scan. Identify the smallest npm fixture that emits a checked-in CDX/SPDX 2.3/SPDX 3 golden and add ~10 lines to its `package-lock.json`:

  ```jsonc
  // Inside the chosen fixture's package-lock.json `packages` object:
  "node_modules/peer-test-target": { "version": "1.0.0" },
  "node_modules/peer-test-consumer": {
      "version": "1.0.0",
      "peerDependencies": { "peer-test-target": "^1.0.0" }
  }
  ```

  Also add a `dependencies: { "peer-test-consumer": "^1.0.0" }` entry on the root so the consumer is reachable from root and the peer-target chain shows up in the golden's dep graph.

  Caveat: if extending the fixture requires the sibling-repo `mikebom-test-fixtures` (per milestone 090) to be modified upstream, defer the fixture extension to a sibling-repo PR + update the fixture-cache SHA pin. In that case, document the deferral in the milestone-147 PR description noting that the fixture-extension follow-up is tracked separately, and the in-tree unit tests (T004-T011) remain the CI-binding signal.

- [ ] T015b Refresh affected goldens via the standard env-var trifecta:

  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1   cargo test --test cdx_regression
  MIKEBOM_UPDATE_SPDX_GOLDENS=1  cargo test --test spdx_regression
  MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression
  ```

  Inspect `git diff --stat -- mikebom-cli/tests/fixtures/golden/`. Each affected line MUST be either (a) a new `dependsOn` / `DEPENDS_ON` / `Relationship` entry for a peer-driven edge OR (b) a new `mikebom:peer-edge-targets` property/annotation. Reject any unrelated drift. After T015's fixture extension lands, the diff should contain ≥1 of each pattern on the chosen fixture's CDX + SPDX 2.3 + SPDX 3 goldens. If the fixture extension was deferred per T015's caveat, an empty diff is acceptable.

- [ ] T016 Run mandatory pre-PR gate per Constitution Development Workflow + memory `feedback_prepr_gate_full_output.md`: `./scripts/pre-pr.sh` from repo root. Both clippy + test steps MUST pass clean (excepting the pre-existing local `sbomqs_parity` env-only failure documented in milestone-144 T001 — CI will validate on a clean runner). If any OTHER test fails, scan the FULL output (do NOT grep on `^test result: FAILED` — known to drop multi-test-suite summaries). Covers SC-007.

- [ ] T017 Commit the milestone-147 changes. Per project convention (matching milestones 134/144/145/146), use the 4-commit chain:
  - `spec(147): npm peerDependencies edge emission + peer-kind annotation` — spec.md + checklists/requirements.md
  - `plan(147): peer-edge emission design + spdx-crate API verification + parity-bridging audit` — plan + research + data-model + contracts + quickstart + CLAUDE.md
  - `tasks(147): 17 tasks across 5 phases for npm peer-edge emission` — tasks.md
  - `impl(147): emit npm peerDependencies as DEPENDS_ON edges + mikebom:peer-edge-targets annotation` — `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs` + `mikebom-cli/src/parity/extractors/*.rs` + `docs/reference/sbom-format-mapping.md` + fixture extension from T015 + golden refresh from T015b

  Do NOT commit until T016 passes clean. Use `git add <specific paths>` (never `-A`). Each commit ends with the standard `Co-Authored-By` trailer.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. Verifies baseline.
- **Phase 2 (Foundational)**: EMPTY — no foundational work required.
- **Phase 3 (US1)**: Depends on Phase 1. Code change at T002 IS the behavior change for US1 (orphan gap closes immediately after T002 + T003).
- **Phase 4 (US2)**: Depends on Phase 3 — specifically T002 (the peer-section iteration). US2's annotation tracks WHICH edges came from the peer section, so it MUST be added INSIDE the same loop T002 establishes. T007/T008 modify the same code block as T002.
- **Phase 5 (Polish)**: Depends on US1 + US2 being functionally complete (or whichever subset is being shipped as MVP).

### User Story Dependencies

- **US1 (P1, MVP)**: Standalone after Phase 1. Delivers the orphan-gap closure (5 → 0). T002 + T003 + T004-T006 (4 tests).
- **US2 (P2)**: Builds on US1. T007 + T008 add the annotation IN THE SAME LOOP as T002's peer-section iteration. Tests at T009/T010/T011 layer on top.

### Within Each User Story

- T002 + T003 are sequential (both edit the same code block; T003 is the comment rewrite). T004-T006 are tests and can land in parallel after T002+T003.
- T007 + T008 are sequential (T007 declares the BTreeSet, T008 stamps the annotation after the loop). T009-T011 are tests and can land in parallel after T007+T008.

### Parallel Opportunities

- Phase 3: T005 + T006 [P] (different tests, same `mod tests` block).
- Phase 4: T009 + T010 + T011 [P].
- Phase 5: T012 [P] + T013 [P] (different files — parity catalog vs docs).

---

## Parallel Example: Phase 3 (after T002 + T003 land)

```bash
# Three tests can be added in one editor pass:
Task T004: peer_dependencies_emit_edges_md147           (replaces pre-147 skip test)
Task T005: unmet_peer_emits_no_edge_md147               (FR-002 guard)
Task T006: peer_already_in_regular_deps_takes_precedence_md147   (FR-003 guard)
```

## Parallel Example: Phase 4 (after T007 + T008 land)

```bash
Task T009:  peer_edge_targets_annotation_present_md147        (anchor — annotation shape)
Task T010:  peer_annotation_omitted_when_set_empty_md147      (FR-005 guard)
Task T011:  peer_edge_targets_array_is_sorted_alphabetically_md147   (research §A)
```

---

## Implementation Strategy

### MVP First (US1 only — ships the orphan-gap closure)

1. Complete Phase 1: T001 baseline check.
2. Complete Phase 3: T002 (section list) + T003 (comment rewrite) + T004-T006 (tests).
3. **STOP and VALIDATE**: `./scripts/pre-pr.sh` clean. Manual smoke per quickstart §Scenario 1: looker-frontend orphan count drops from 5 to 0.
4. This alone is a shippable PR. US2 (annotation) is a nice-to-have for downstream consumers wanting the kind distinction; skipping it leaves mikebom matching Trivy on edges but without the differentiator metadata.

### Incremental / Recommended (single-PR delivery)

1. Phase 1 (T001) baseline.
2. Phase 3 (T002-T006) US1 — orphan gap closes.
3. Phase 4 (T007-T011) US2 — annotation layer.
4. Phase 5 (T012-T017) polish — parity row + docs + golden refresh + pre-PR + commit.

Total: 17 tasks. Estimated ~15 LOC of reader change + ~120 LOC of tests + 1 parity-row + 1 docs row.

### Single-developer Note

This milestone is small enough that one developer can work through all phases in one session. The [P] markers exist primarily to signal "no cross-file write conflict" — useful for tooling that automates task execution but not load-bearing for a human implementer.

---

## Notes

- Tests live in-file under `#[cfg(test)] mod tests` per the project's existing convention. No out-of-source integration test for this milestone — the existing parity-catalog framework + reader unit tests provide complete coverage.
- The `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention applies to any test module per Constitution Principle IV (the existing `package_lock.rs#mod tests` already has it; no change needed).
- Memory `feedback_prepr_gate_full_output.md` is directly relevant: when verifying T016, scan the FULL output rather than greping on `^test result: FAILED`.
- Memory `feedback_dont_dismiss_test_failures.md` is relevant if any new test failures surface during golden refresh: verify reproducibility before calling anything "pre-existing flake".
- The commit-message convention (T017) follows the milestone-134/144/145/146 precedent: `spec(147):` / `plan(147):` / `tasks(147):` / `impl(147):`.
- Per spec SC-008 + SC-009 (operator-cadence cross-tool comparison): document in the PR description that the operator should re-run the 3-tool comparison (Trivy + Syft + mikebom) on the looker-frontend lockfile post-merge to confirm orphan counts (trivy=0, syft=151, mikebom=0). The harness is NOT a CI gate; the in-tree tests are the CI-binding signal.
- Per spec FR-009 + plan §V parity-bridging audit: the new `mikebom:peer-edge-targets` annotation is the third `mikebom:*` annotation introduced under Constitution V's parity-bridging carve-out (alongside `mikebom:lifecycle-scope` and `mikebom:source-files-nested-url`). The Principle V audit is documented in plan.md Complexity Tracking + spec FR-009; T013 enshrines it in `docs/reference/sbom-format-mapping.md`.
