---

description: "Task list for milestone 157 — pnpm-lock v9 dep-graph fix"
---

# Tasks: pnpm-lock v9 dep-graph — parse `snapshots:` for edges (milestone 157)

**Input**: Design documents from `/specs/157-pnpm-v9-graph/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [quickstart.md](./quickstart.md)

**Tests**: Included. SC-007 requires ≥7 new unit tests; the spec + research inventory 8 unit tests + 1 integration test + 1 monotonic-additive helper test = 10 tests total.

**Organization**: One user story (US1 P1). No secondary US — the bug is a single behavior gap. Task list is deliberately lean given the single-file scope.

**Depends on**: milestone 156 (merged as commit `f43f9ff`). No cross-milestone code interaction; milestone 157 touches only `pnpm_lock.rs` and its test / fixture files.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 for user-story phase tasks
- Include exact file paths in descriptions

## Path Conventions

- Primary deliverable: `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`
- Integration test: `mikebom-cli/tests/npm_pnpm_v9_dep_graph.rs`
- Golden fixtures (pnpm-only, monotonic-additive regeneration): `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/npm.*`
- CHANGELOG: `CHANGELOG.md` at repo root
- No changes to: `mikebom-cli/src/generate/`, `mikebom-cli/src/parity/`, other npm sub-readers, other package_db readers, `mikebom-common/`, `mikebom-ebpf/`, `docs/reference/sbom-format-mapping.md`, non-pnpm golden fixtures

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Baseline verification. No project scaffolding needed — this is a single-file additive change to an existing crate.

- [X] T001 Verify baseline state: `git log -1 --oneline`, confirm branch `157-pnpm-v9-graph`, capture pre-milestone `pnpm_lock.rs` LOC (`wc -l mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`) + pre-milestone test count (`grep -cE "^\s+fn pnpm_" mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`), and confirm milestone 156 (PR #490) is on main (`git log main --oneline | head -3` should show `impl(156)` at or near the top).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add the shared constant + two helper functions that both v9 (snapshots pre-scan) and v6/v7 (packages inline) paths depend on. Both edit `pnpm_lock.rs` so sequential.

**⚠️ CRITICAL**: T002 + T003 must complete before US1 work begins.

- [X] T002 Add the shared constant + inline-path helper to `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`. Place both at the top of the file, immediately after the `use` block (currently ends at line 8):

  ```rust
  /// pnpm dep-section names walked by both the snapshots pre-scan (v9)
  /// AND the packages-inline path (v6/v7). Kept in one place so the
  /// SC-011 pnpm/npm parity assertion has a stable code anchor and so
  /// a future dep-section addition is a single edit.
  ///
  /// NOT identical to `package_lock.rs`'s 4-section list — pnpm encodes
  /// dev status via the per-package `dev: true` boolean at the entry
  /// level (handled at `pnpm_lock.rs:56`), NOT via a `devDependencies:`
  /// sub-mapping. So `PNPM_DEP_SECTIONS` walks only the three non-dev
  /// sections. The `dev: true` boolean continues to gate whole-package
  /// filtering when `include_dev = false`.
  const PNPM_DEP_SECTIONS: &[&str] = &[
      "dependencies",
      "peerDependencies",
      "optionalDependencies",
  ];

  /// Milestone 157: walk the three dep sub-mappings inside a single
  /// packages-entry table (v6/v7 inline path). Returns the sorted-
  /// deduped union of the sub-mappings' KEYS. Values are normalized
  /// via `parse_pnpm_key` on a synthesized `"<name>@<value>"` string
  /// to strip peer-dep suffixes; non-registry values (git URLs,
  /// tarballs, file paths) that fail `parse_pnpm_key` are dropped
  /// with a `tracing::debug!` log.
  fn walk_pnpm_dep_sections(entry_tbl: &serde_yaml::Mapping) -> Vec<String> {
      let mut deps: Vec<String> = Vec::new();
      for section in PNPM_DEP_SECTIONS {
          let Some(sub) = entry_tbl
              .get(serde_yaml::Value::String((*section).to_string()))
              .and_then(|v| v.as_mapping())
          else {
              continue;
          };
          for (dep_key, dep_value) in sub {
              let Some(dep_name) = dep_key.as_str() else { continue };
              let Some(dep_ver_raw) = dep_value.as_str() else { continue };
              let dep_pair_raw = format!("{dep_name}@{dep_ver_raw}");
              let stripped = dep_pair_raw
                  .strip_prefix('/')
                  .unwrap_or(&dep_pair_raw);
              let Some((canon_name, canon_ver)) = parse_pnpm_key(stripped) else {
                  tracing::debug!(dep = %dep_pair_raw, "pnpm-lock: skipping non-registry dep value");
                  continue;
              };
              deps.push(format!("{canon_name}@{canon_ver}"));
          }
      }
      deps.sort();
      deps.dedup();
      deps
  }
  ```

  Verify `cargo check -p mikebom` compiles cleanly (helper is unused-warned as `dead_code` until T003/T004 land — that's expected; the whole batch lands together as an atomic commit).

- [X] T003 Add the v9 snapshots pre-scan helper to `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`, immediately after `walk_pnpm_dep_sections`:

  ```rust
  /// Milestone 157: pre-scan the top-level `snapshots:` section
  /// (introduced in pnpm-lock.yaml v9) into a lookup table keyed by
  /// canonical `name@version` (peer-dep suffix stripped via
  /// `parse_pnpm_key`). Values are the sorted-deduped union of the
  /// three sub-mappings' keys, each normalized to canonical form.
  ///
  /// Returns empty HashMap when the top-level `snapshots:` key is
  /// missing or not a mapping (v6/v7 lockfiles, or anomalous v9
  /// lockfiles).
  fn build_snapshots_lookup(
      root: &serde_yaml::Value,
  ) -> std::collections::HashMap<String, Vec<String>> {
      let mut out = std::collections::HashMap::new();
      let Some(snapshots) = root
          .get("snapshots")
          .and_then(|v| v.as_mapping())
      else {
          return out;
      };
      for (key, entry) in snapshots {
          let Some(key_str) = key.as_str() else { continue };
          let stripped = key_str.strip_prefix('/').unwrap_or(key_str);
          let Some((name, version)) = parse_pnpm_key(stripped) else {
              tracing::debug!(snapshot_key = %key_str, "pnpm-lock: skipping non-registry snapshot key");
              continue;
          };
          let canonical = format!("{name}@{version}");
          let Some(tbl) = entry.as_mapping() else { continue };
          let deps = walk_pnpm_dep_sections(tbl);
          out.insert(canonical, deps);
      }
      out
  }
  ```

  Verify `cargo check -p mikebom` compiles cleanly (still `dead_code`-warned until T004 wires it in).

**Checkpoint**: Foundation ready — US1 implementation can now begin.

---

## Phase 3: User Story 1 — SBOM consumer of a pnpm v9 project sees a complete dep graph (Priority: P1) 🎯 MVP

**Goal**: `parse_pnpm_lock` pre-scans `snapshots:`, then per-packages-entry emission uses inline sub-mappings when populated (v6/v7 case) OR the snapshots lookup when inline is empty (v9 case). Per Q1 clarification, both paths walk the same three sub-mappings for full pnpm/npm parity.

**Independent Test**: Run the 8 in-module unit tests (T007) + the SC-008 integration test (T008); all pass. Plus the SC-001 manual argo-cd testbed (T013) shows 1329 components / ≥5000 edges.

### Implementation for User Story 1

- [X] T004 [US1] Refactor `parse_pnpm_lock` at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:20` to consume the new helpers. Specifically:
  - Add `let snapshots_lookup = build_snapshots_lookup(root);` at the top of the function body (immediately after `let mut out = Vec::new();` at line 25).
  - Replace the existing `depends` construction at lines 83-91 with (F3 remediation — distinguish "lookup hit but empty" from "lookup miss" so T005's `fell_back_count` is accurate):
    ```rust
    let inline_deps = walk_pnpm_dep_sections(tbl);
    let depends: Vec<String> = if inline_deps.is_empty() {
        // v9 path: pull from snapshots lookup keyed on canonical name@version.
        let canonical = format!("{name}@{version}");
        if let Some(snap_deps) = snapshots_lookup.get(&canonical) {
            fell_back_count += 1;   // T005 counter — only when lookup HIT.
            snap_deps.clone()
        } else {
            Vec::new()   // FR-005 leaf semantics: both empty → empty depends.
        }
    } else {
        // v6/v7 path: inline sub-mappings win (FR-004 precedence).
        inline_deps
    };
    ```
    Declare `let mut fell_back_count: usize = 0;` immediately after `let mut out = Vec::new();`. T005 emits the counter in its FR-007 info-level diagnostic.
  - Delete the pre-milestone-157 `depends` construction (lines 83-91 of the current file).
  - Verify: `cargo check -p mikebom --all-targets` compiles cleanly (no more `dead_code` warnings).

- [X] T005 [US1] Add FR-007 + FR-008 diagnostic emissions to `parse_pnpm_lock` in `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`. Note: `fell_back_count` counter declaration + increment logic is in T004 (F3 remediation); this task adds the emissions that CONSUME the counter. Add these at appropriate points inside the function body:
  - At the very top, after reading the root YAML mapping, add lockfile-version detection per research §R7:
    ```rust
    let lock_version: String = root
        .get("lockfileVersion")
        .and_then(|v| v.as_str().map(|s| s.to_string()).or_else(|| v.as_f64().map(|n| n.to_string())))
        .unwrap_or_default();
    let is_v9_or_later: bool = lock_version
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|major| major >= 9)
        .unwrap_or(false);
    ```
  - Counter tracking lives in T004 (F3 remediation) — this task consumes `fell_back_count` in the info-level diagnostic below.
  - At end of function body, emit FR-007 info-level diagnostic:
    ```rust
    tracing::info!(
        lockfile = %source_path,
        lockfile_version = %lock_version,
        packages_count = packages.len(),
        snapshots_count = snapshots_lookup.len(),
        fell_back_to_snapshots = fell_back_count,
        "pnpm-lock parsed"
    );
    ```
  - Additionally, emit FR-008 warn-level diagnostic if `is_v9_or_later && snapshots_lookup.is_empty()`:
    ```rust
    if is_v9_or_later && snapshots_lookup.is_empty() {
        tracing::warn!(
            lockfile = %source_path,
            lockfile_version = %lock_version,
            "pnpm-lock v9 with no snapshots section — dep-graph will be empty for all non-root components. Check lockfile validity."
        );
    }
    ```

- [X] T006 [US1] Update the module doc-comment at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:1-4` + the internal comment at lines 27-30 to reflect the milestone-157 shape. Replace the current 4-line module doc with:
  ```rust
  //! pnpm-lock.yaml parser.
  //!
  //! Handles v6/v7 (single `packages:` section with inline
  //! `dependencies` / `peerDependencies` / `optionalDependencies`)
  //! AND v9 (`packages:` for identity + `snapshots:` for edges).
  //! Milestone 157 (2026-07-03) added `snapshots:` support after the
  //! team reported argo-cd's pnpm v9 lockfile emitting 1329 components
  //! but only 110 dep-graph edges. Q1 clarification 2026-07-03 also
  //! brought pnpm to parity with `package_lock.rs`'s milestone-147
  //! behavior (walks all three non-dev dep sub-mappings — see
  //! `PNPM_DEP_SECTIONS` const).
  ```
  And update the comment at lines 27-30 (which describes the intended-but-unimplemented v9 shape) to reflect that the shape is now IMPLEMENTED per FR-001 + FR-002 (via `build_snapshots_lookup` + `walk_pnpm_dep_sections`).

- [X] T007 [US1] Add 9 unit tests to `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs` inside the existing `#[cfg(test)] mod tests` block (currently starts at line 156). All 9 test function names begin with `pnpm_v6_`, `pnpm_v9_`, or `pnpm_walks_` per SC-007 grep. Per research §R8 + F1 remediation:
  1. `pnpm_v9_minimal_dependencies_only_emits_edge` — fixture: minimal v9 lockfile with 1 `packages:` entry + 1 `snapshots:` entry with `dependencies: {bar: 2.0.0}`. Assert 1 emitted `PackageDbEntry` with `depends = ["bar@2.0.0"]`.
  2. `pnpm_v9_empty_snapshot_body_leaf_node` — fixture: `snapshots: {foo@1.0.0: {}}`. Assert `depends.is_empty()`.
  3. `pnpm_v9_peer_dep_suffix_normalized_in_key_and_value` — fixture: `snapshots: {foo@1.0.0(bar@2.0.0): {dependencies: {baz: 3.0.0(qux@4.0.0)}}}` + matching `packages:` entry. Assert emitted PURL is `pkg:npm/foo@1.0.0` (identity peer-suffix stripped) + `depends = ["baz@3.0.0"]` (value peer-suffix stripped).
  4. `pnpm_v9_orphaned_snapshot_skipped` — fixture: `packages:` empty + `snapshots: {foo@1.0.0: {dependencies: {bar: 2.0.0}}}`. Assert no `PackageDbEntry` emitted (FR-006).
  5. `pnpm_v9_all_three_sub_mappings_union_with_dedup` — fixture: snapshot with `dependencies: {a: 1.0.0, shared: 5.0.0}` + `peerDependencies: {b: 2.0.0, shared: 5.0.0}` + `optionalDependencies: {c: 3.0.0}`. Assert `depends = ["a@1.0.0", "b@2.0.0", "c@3.0.0", "shared@5.0.0"]` (4 entries, sorted, `shared@5.0.0` de-duped).
  6. `pnpm_v6_v7_inline_peer_and_optional_now_emit` — fixture: v6-style packages entry with inline `dependencies: {a: 1.0.0}` + `peerDependencies: {b: 2.0.0}` + `optionalDependencies: {c: 3.0.0}`, no snapshots section. Assert `depends = ["a@1.0.0", "b@2.0.0", "c@3.0.0"]` (Q1 clarification — v6/v7 path walks all three).
  7. `pnpm_v9_inline_wins_over_snapshots_fallback` — fixture: v9 lockfile where a packages entry ALSO carries inline `dependencies: {only-inline: 1.0.0}` while its matching snapshots entry has DIFFERENT `dependencies: {only-snapshots: 2.0.0}`. Assert `depends = ["only-inline@1.0.0"]` (inline wins per FR-004).
  8. `pnpm_walks_same_dep_sections_as_package_lock_non_dev` — SC-011 parity. Assert `PNPM_DEP_SECTIONS` contains exactly `["dependencies", "peerDependencies", "optionalDependencies"]` in that order. Doc-comment on the const documents the intentional divergence from `package_lock.rs`'s 4-section walk (dev status handled via `dev: true` boolean).
  9. `pnpm_v9_no_snapshots_scans_cleanly_with_empty_deps` — F1 remediation, SC-005 behavioral verification. Fixture: v9 header (`lockfileVersion: '9.0'`) with `packages:` section containing 2 entries, NO `snapshots:` key at all. Assert: (a) `parse_pnpm_lock` returns without panic; (b) both emitted `PackageDbEntry` instances have `depends.is_empty()`. Log-line format verification is out of scope per SC-005's downgraded automation claim — the log-string is documented in FR-008 for operators.

  All 9 tests use the same `serde_yaml::from_str(...).unwrap() → parse_pnpm_lock(&parsed, "/pnpm-lock.yaml", false)` pattern as the existing `pnpm_lock_v6_style_parses` test at line 156.

- [X] T008 [US1] Create the SC-008 integration test at `mikebom-cli/tests/npm_pnpm_v9_dep_graph.rs`. Test invokes the release binary via `Command::new(env!("CARGO_BIN_EXE_mikebom"))` against a temp-dir-synthesized 5-package v9 testbed (all files created at test-runtime via `tempfile::tempdir` + `std::fs::write` — no vendored fixture files under `tests/fixtures/`). The synthesized `pnpm-lock.yaml` includes:
  - Root project (from a matching `package.json`).
  - 5 packages entries + 5 matching snapshots entries.
  - At least one entry with a peer-dep suffix on the snapshots key.
  - At least one entry with all three sub-mappings populated.
  - At least one entry that's a leaf (empty snapshot body).

  Assertions:
  - Emitted CDX contains ≥5 npm components (identity path unchanged).
  - Emitted CDX contains ≥1 `dependencies[]` entry with a non-trivial `dependsOn` list matching the expected graph shape.
  - The peer-dep-suffixed entry's PURL is canonical (no `(...)` in the PURL).
  - The leaf entry's `dependsOn` is `[]`.

  Uses the same `run_scan` pattern as milestone-156's `cmake_walker_depth_deep_emission.rs` integration test.

- [X] T009 [US1] Add the monotonic-additive golden diff helper at the top of `mikebom-cli/tests/npm_pnpm_v9_dep_graph.rs` (inline in the same file as T008 — single-use helper, no separate common module needed). Signature:
  ```rust
  fn assert_monotonic_additive(old: &serde_json::Value, new: &serde_json::Value) {
      // Index each doc's dependencies[] by ref.
      // For every ref in OLD, assert its dependsOn is a subset of NEW's dependsOn for the same ref.
      // Extra entries in NEW that aren't in OLD are permitted (additive).
      // Missing refs in NEW that were in OLD fires an assertion failure.
  }
  ```
  Add a second test function `assert_monotonic_additive_pnpm_golden_diff_catches_missing_edge` inside the same integration test file that:
  - Constructs a synthetic OLD document with `dependencies: [{ref: "pkg:npm/foo@1.0.0", dependsOn: ["pkg:npm/bar@2.0.0"]}]`.
  - Constructs a synthetic NEW document with `dependencies: [{ref: "pkg:npm/foo@1.0.0", dependsOn: []}]` (edge removed — INVALID monotonic-additive change).
  - Asserts `assert_monotonic_additive` panics with a message containing `monotonic-additive violation`.
  - Uses `std::panic::catch_unwind` to catch the panic + assert-on-message.

  This second test proves the helper catches the failure mode it's designed to detect.

- [X] T010 [US1] Regenerate the 3 pnpm golden fixtures (npm.cdx.json / npm.spdx.json / npm.spdx3.json) — F2-remediated procedure with automated monotonic-additive verification against real pre-157 goldens.
  Step 1: Snapshot the pre-157 goldens via `git show` BEFORE regeneration:
  ```bash
  mkdir -p /tmp/mikebom-m157-pre-goldens
  git show main:mikebom-cli/tests/fixtures/golden/cyclonedx/npm.cdx.json  > /tmp/mikebom-m157-pre-goldens/npm.cdx.json
  git show main:mikebom-cli/tests/fixtures/golden/spdx-2.3/npm.spdx.json  > /tmp/mikebom-m157-pre-goldens/npm.spdx.json
  git show main:mikebom-cli/tests/fixtures/golden/spdx-3/npm.spdx3.json   > /tmp/mikebom-m157-pre-goldens/npm.spdx3.json
  ```
  Step 2: Regenerate:
  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test -p mikebom --test cdx_regression -- cdx_regression_npm
  MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test -p mikebom --test spdx_regression -- npm_byte_identity
  MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test -p mikebom --test spdx3_regression -- npm_byte_identity
  ```
  Step 3: Real-golden monotonic-additive verification via the T009 helper. Write a one-shot verification test at `mikebom-cli/tests/npm_pnpm_v9_dep_graph.rs` under a new test fn `monotonic_additive_real_goldens_from_snapshot`:
  ```rust
  #[test]
  fn monotonic_additive_real_goldens_from_snapshot() {
      let snapshot_dir = std::env::var("MIKEBOM_PRE157_SNAPSHOT_DIR")
          .unwrap_or_else(|_| "/tmp/mikebom-m157-pre-goldens".to_string());
      let pre_path = std::path::PathBuf::from(&snapshot_dir).join("npm.cdx.json");
      if !pre_path.exists() {
          eprintln!("skip: {} not present; Step 1 of T010 not run", pre_path.display());
          return;
      }
      let old: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pre_path).unwrap()).unwrap();
      let new_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
          .join("tests/fixtures/golden/cyclonedx/npm.cdx.json");
      let new: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&new_path).unwrap()).unwrap();
      assert_monotonic_additive(&old, &new);
      // Diagnostic: measure edge growth for PR description.
      let old_count: usize = old["dependencies"].as_array().unwrap()
          .iter().map(|d| d["dependsOn"].as_array().map(|a| a.len()).unwrap_or(0)).sum();
      let new_count: usize = new["dependencies"].as_array().unwrap()
          .iter().map(|d| d["dependsOn"].as_array().map(|a| a.len()).unwrap_or(0)).sum();
      println!("pnpm CDX golden edges: {old_count} → {new_count} (Δ +{})", new_count - old_count);
  }
  ```
  Step 4: Run the verification test with the snapshot dir set:
  ```bash
  MIKEBOM_PRE157_SNAPSHOT_DIR=/tmp/mikebom-m157-pre-goldens \
      cargo test -p mikebom --test npm_pnpm_v9_dep_graph -- \
      monotonic_additive_real_goldens_from_snapshot --nocapture
  ```
  Step 5: Paste the printed edge-count summary (`edges: X → Y (Δ +Z)`) into the PR description as SC-002's real-golden verification receipt.
  Step 6: Verify non-pnpm goldens (apk, bazel, cargo, cmake, deb, gem, golang, maven, pip, rpm) show ZERO changes via `git diff --name-only mikebom-cli/tests/fixtures/golden/ | grep -v npm` (expect empty).
  Step 7: Run `cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression` (WITHOUT the update env vars) and confirm all 33 tests pass.

  Note: the `monotonic_additive_real_goldens_from_snapshot` test is designed to be a one-shot verification — it gracefully skips when the snapshot dir isn't populated (so it doesn't fail CI after merge). Post-merge, the test remains in-tree as documentation of the verification procedure.

**Checkpoint**: At this point, US1 is fully functional. `mikebom sbom scan --path /tmp/argo-cd/ui` should produce ≥5000 dep-graph edges (SC-001 manual test in T013 below).

---

## Phase 4: Polish & Cross-Cutting Concerns

- [X] T011 Add CHANGELOG.md entry under `## [Unreleased]` per research §R9 + SC-009. Entry names:
  - The pnpm-lock v9 `snapshots:` support fix.
  - The team's bug report against `kusari-sandbox/argo-cd` + empirical reproduction date (2026-07-03).
  - The argo-cd testbed impact (110 → ≥5000 edges).
  - The Q1 clarification bringing pnpm to full parity with npm's `package_lock.rs` (walks `dependencies:` + `peerDependencies:` + `optionalDependencies:` per milestone 147).
  - The monotonic-additive pnpm v6/v7 golden regeneration (pre-existing edges preserved; new peer + optional edges added).
  - Consumer jq recipe for verifying edge presence per research §R9.

- [X] T012 Run SC-006 pre-PR gate. **CRITICAL** — per the milestone-155 fix memory `feedback_prepr_gate_bails_on_first_failure.md`, both commands MUST be run:
  1. `./scripts/pre-pr.sh` — the mandatory gate.
  2. `cargo +stable test --workspace --no-fail-fast 2>&1 | grep -E '^---- .+ stdout ----'` — enumerate every failing test binary. Expected output: ONLY `sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` (documented env-only flake). Any other failure name → real regression; do NOT proceed. Reproduce the failure individually via `cargo test -p mikebom --test <name>` and fix.

- [X] T013 SC-010 wire-format guard verification. Run each guard command from quickstart.md Scenario 10 and confirm the expected empty output:
  ```bash
  git diff main --name-only -- mikebom-cli/src/generate/
  git diff main --name-only -- mikebom-cli/src/parity/
  git diff main --name-only -- docs/reference/sbom-format-mapping.md
  git diff main --name-only -- mikebom-common/ mikebom-ebpf/
  git diff main --name-only -- Cargo.toml Cargo.lock
  git diff main --name-only -- mikebom-cli/tests/fixtures/golden/ | grep -v npm
  # F5 remediation — verify FR-015 (npm reader dispatch order unchanged):
  git diff main --name-only -- mikebom-cli/src/scan_fs/package_db/npm/mod.rs
  # F5 also — verify sibling npm sub-readers (FR-010) UNCHANGED:
  git diff main --name-only -- mikebom-cli/src/scan_fs/package_db/npm/ \
      | grep -vE 'pnpm_lock\.rs$'
  ```
  Each MUST return empty (only npm.* pnpm goldens should have changed; the sibling-readers filter allows only `pnpm_lock.rs`). Also run `git diff main --name-only` and verify the shipped file-list matches plan.md's expected shape.

- [X] T014 SC-001 manual operator-cadence argo-cd testbed verification per quickstart.md Scenario 1. Clone or point at `/tmp/argo-cd/ui`, build release binary `cargo +stable build --release -p mikebom`, run `./target/release/mikebom --offline sbom scan --path /tmp/argo-cd/ui --format cyclonedx-json --output cyclonedx-json=/tmp/mikebom-m157/argo-cd.cdx.json --no-deep-hash`. Run the SC-001 jq recipes:
  1. **Total edge count**: `jq '[.dependencies[] | .dependsOn // [] | length] | add' /tmp/mikebom-m157/argo-cd.cdx.json`. **Defensive floor**: ≥2500 (fail hard if not met — halt and investigate). **Aspirational target**: ≥5000 (documents expected shape; not-met → note in PR but still ship since defensive floor satisfied). **F4 remediation empirical revision**: whichever number is measured MUST be recorded inline in spec.md's SC-001 as the new observed floor. Follow milestone-156's F1 pattern of transparent inline revision.
  2. Verify `@actions/core@3.0.1`'s edges include the 2 known deps (`@actions/exec@3.0.0` + `@actions/http-client@4.0.1`).
  3. Verify `react@19.2.6` has empty `dependsOn` (leaf).
  4. Report format for the PR comment: `SC-001 result: X edges (defensive floor 2500 ✓ | aspirational 5000 [MET/NOT-MET]). @actions/core@3.0.1 shape ✓. react@19.2.6 leaf ✓.`
  If measured edge count < 2500: HALT, do not merge. Investigate via `RUST_LOG=info` re-scan looking for the FR-007 diagnostic — `fell_back_to_snapshots` should approach `packages_count`. If it's much less, there's a normalization bug between the packages-side canonical key and the snapshots-side canonical key. Reproduce with a minimal fixture + open a follow-up finding.

- [X] T015 Update the requirements checklist at `specs/157-pnpm-v9-graph/checklists/requirements.md` with implementation-completion notes: measured argo-cd edge count from T014, pre-PR gate result from T012, monotonic-additive golden diff summary from T010, any surprises encountered during impl.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1, T001)**: No dependencies. Runs first.
- **Foundational (Phase 2, T002-T003)**: Depends on T001. Sequential (both edit `pnpm_lock.rs`).
- **User Story 1 (Phase 3, T004-T010)**: All depend on Phase 2. T004-T007 edit `pnpm_lock.rs` sequentially (same-file conflict). T008-T009 edit a NEW integration test file — can be authored in parallel with T007 if desired, but the plan.md tree keeps them sequential for clarity. T010 (golden regeneration) MUST run after T004 (production code lands) — regenerating before code lands would produce pre-fix goldens.
- **Polish (Phase 4, T011-T015)**: T011 can run at any time after T004. T012 (pre-PR gate) MUST run after all US1 tasks + T011. T013-T015 sequential after T012.

### Parallel Opportunities

- T007 (pnpm_lock.rs unit tests) + T008 (integration test file creation) — different files. Could be authored in parallel but the plan lists sequentially for narrative clarity.
- T011 (CHANGELOG) + T014 (argo-cd verification) — independent; both post-code. T014 needs the release binary which T012 doesn't rebuild (already built during pre-PR).

---

## Parallel Example: US1 test authoring

Once T006 (module doc + code) is done:

```bash
Task T007: Add 8 unit tests to pnpm_lock.rs mod tests block
Task T008: Create integration test file mikebom-cli/tests/npm_pnpm_v9_dep_graph.rs
```

Different files; independent.

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001)
2. Complete Phase 2: Foundational (T002-T003 — helpers)
3. Complete Phase 3: US1 (T004-T010 — the whole primary deliverable + golden regen)
4. **STOP + VALIDATE**: Run `cargo test -p mikebom scan_fs::package_db::npm::pnpm_lock` (all 8 unit tests pass) + `cargo test --test npm_pnpm_v9_dep_graph` (2 integration tests pass) + `cargo test --test cdx_regression` (11 tests pass, npm regenerated).
5. Optional MVP ship: this alone fixes the reported bug.

### Suggested commit shape

Following the project's per-milestone convention (matches milestones 155/156 pattern):

- `spec(157): ...` — spec + clarify session (already committed via /speckit.specify + /speckit.clarify).
- `plan(157): ...` — plan.md + research.md + data-model.md + quickstart.md + CLAUDE.md.
- `tasks(157): ...` — this tasks.md file.
- `impl(157): ...` — T002-T010 production + test code + golden regens.
- `docs(157): ...` — T011 CHANGELOG + T015 checklist update.

Per the milestone-155 `feedback_prepr_gate_bails_on_first_failure.md` memory: BEFORE claiming pre-PR gate green, MUST enumerate every `^---- <name> stdout ----` line via `cargo test --workspace --no-fail-fast`.

---

## Notes

- [P] tasks = different files, no dependencies. Milestone 157 has minimal parallelism given the single-file scope.
- All `pnpm_lock.rs` edits are sequential (same file).
- SC-010 wire-format guard: 6 diff checks all MUST return empty (only npm.* goldens allowed to change per Q1 monotonic-additive policy).
- Do NOT touch `docs/reference/sbom-format-mapping.md` (no new annotation keys).
- Do NOT touch other npm sub-readers (`package_lock.rs`, `bun_lock.rs`, `yarn_lock.rs`) — SC-011 parity is asserted by a code-level constant, not a cross-file refactor.
- SC-001 (argo-cd testbed) requires a checkout at `/tmp/argo-cd/ui` — the maintainer's testbed. Fine for manual verification even without vendoring into the repo.
- The monotonic-additive helper (T009) is single-use; keep it inline in the integration test file rather than promoting to `tests/common/`.
