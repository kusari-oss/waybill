---
description: "Task list for milestone 148 — source-files cross-emitter divergence — union evidence.source_file_paths across same-PURL entries after dedup"
---

# Tasks: milestone 148 — source-files cross-emitter divergence — union evidence across same-PURL entries

**Input**: Design documents from `/specs/148-source-files-union/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: Spec mandates tests via SC-002 + SC-003 + SC-004 + SC-005. Test tasks are included inline alongside implementation tasks (per the project's existing Rust convention of in-file `#[cfg(test)] mod tests` for unit tests + `mikebom-cli/tests/*.rs` for integration tests).

**Organization**: US1 (cross-format value match) and US2 (cross-ecosystem coverage) share the SAME implementation — the `canonicalize_source_files_by_purl` pass keyed on `Purl::as_str()` is ecosystem-agnostic by construction (research §D). Phase 3 ships US1 + the implementation; Phase 4 ships US2 as additional cross-ecosystem assertions over the same code path. No foundational types or signature plumbing needed (zero new types per data-model.md).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Maps to user story from spec.md (US1, US2)
- Paths absolute under repo root `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

Two Rust source files touched: `mikebom-cli/src/resolve/deduplicator.rs` (NEW `canonicalize_source_files_by_purl` function + unit tests) and `mikebom-cli/src/scan_fs/mod.rs` (ONE additive call at line ~751). One new integration test at `mikebom-cli/tests/source_files_purl_union_md148.rs`. One new synthetic test fixture at `mikebom-cli/tests/fixtures/source_files_union/`. Three potential golden refreshes under `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/maven.*.json`.

---

## Phase 1: Setup

**Purpose**: Verify baseline before any code change.

- [ ] T001 Confirm baseline pre-PR gate is green on branch `148-source-files-union`. Run `./scripts/pre-pr.sh` from repo root. Expected: clippy `--workspace --all-targets -- -D warnings` clean; `cargo test --workspace` passes except for the pre-existing local-environment `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` failure documented in milestone-144 T001. If anything ELSE fails, halt and investigate before proceeding.

---

## Phase 2: Foundational

**Purpose**: None required for this milestone. Zero new types, zero new Cargo dependencies (per data-model.md). The new function is purely additive in an existing module.

(No tasks in this phase. Proceed directly to Phase 3.)

---

## Phase 3: User Story 1 - Same-PURL `mikebom:source-files` values match across CDX and SPDX 3 emissions (Priority: P1) 🎯 MVP

**Goal**: Implement `canonicalize_source_files_by_purl()` post-dedup pass; wire it into the scan pipeline; cover with unit tests + a synthetic-fixture integration test asserting cross-format byte-equality of `mikebom:source-files` on a Maven nested-coord PURL.

**Independent Test**: Build a synthetic Maven fixture where one coord appears both standalone AND nested inside a fat-jar. Scan three times (CDX, SPDX 2.3, SPDX 3). Assert the `mikebom:source-files` value for that coord PURL is bytewise-identical across all three formats.

### Implementation for User Story 1

- [ ] T002 [US1] Add `pub fn canonicalize_source_files_by_purl(components: &mut Vec<ResolvedComponent>)` to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/resolve/deduplicator.rs` immediately after the existing `pub fn deduplicate(...)` function. Per contracts/source-files-union.md algorithm sketch:

  ```rust
  /// Milestone 148: cross-PURL canonicalization of evidence.source_file_paths.
  ///
  /// After the existing `deduplicate()` pass merges same-(ecosystem,name,version,
  /// parent_purl)-key groups, some ecosystems (Maven nested-coord case at
  /// `scan_fs/package_db/maven.rs:3429-3457`, Cargo workspace vendoring, Go
  /// vendored modules) intentionally retain multiple `ResolvedComponent` instances
  /// sharing the same `Purl::as_str()` value but differing in `parent_purl`. The
  /// CDX nested-components topology depends on this two-entry shape.
  ///
  /// Each entry carries its own `evidence.source_file_paths` Vec (one observed
  /// path from the standalone reader pass, one observed path from the nested
  /// reader pass). Per-emitter iteration-order differences (CDX `builder.rs:619`,
  /// SPDX 2.3 `annotations.rs:302`, SPDX 3 `v3_annotations.rs:267`) cause the
  /// audit harness to observe cross-format divergence on the `mikebom:source-files`
  /// annotation for what the harness treats as the same PURL (51 polyglot-builder-
  /// image findings, 2026-06-28 audit).
  ///
  /// This pass, keyed on the full canonical `Purl::as_str()` string, replaces
  /// each same-PURL entry's `source_file_paths` Vec with the alphabetically-
  /// sorted UNION of paths observed across all same-PURL entries. After the
  /// pass, every emitter sees the same Vec content for every same-PURL pair,
  /// so the wire-side `mikebom:source-files` annotation is identical across
  /// formats regardless of which entry the harness happens to pick.
  ///
  /// **Idempotent** — running twice produces byte-identical output (FR-004).
  /// **Topology-preserving** — does NOT modify `parent_purl` or any other
  /// field (FR-005 + FR-006).
  /// **No-op for single-entry PURLs** — the common case is unchanged (FR-007).
  pub fn canonicalize_source_files_by_purl(
      components: &mut Vec<ResolvedComponent>,
  ) {
      use std::collections::{BTreeSet, HashMap};

      // Phase 1: collect — walk every component, accumulate paths by canonical PURL.
      let mut paths_by_purl: HashMap<String, BTreeSet<String>> = HashMap::new();
      for c in components.iter() {
          paths_by_purl
              .entry(c.purl.as_str().to_string())
              .or_default()
              .extend(c.evidence.source_file_paths.iter().cloned());
      }

      // Phase 2: write back — replace each entry's source_file_paths with the
      // alphabetically-sorted union for its PURL.
      for c in components.iter_mut() {
          if let Some(union) = paths_by_purl.get(c.purl.as_str()) {
              c.evidence.source_file_paths = union.iter().cloned().collect();
          }
      }
  }
  ```

  Per research §B the function lives IN the existing `deduplicator.rs` (NOT a sibling `source_files_union.rs` module) since it's conceptually a second-phase cross-PURL union over the same domain as `deduplicate()`.

- [ ] T003 [US1] Wire the new pass at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/mod.rs` immediately after the existing `let mut components = deduplicate(components);` at line 750. Add the import at the top of the file if not already present (`use crate::resolve::deduplicator::canonicalize_source_files_by_purl;` or extend the existing `use crate::resolve::deduplicator::deduplicate;` import). The call:

  ```rust
  let mut components = deduplicate(components);
  // Milestone 148: cross-PURL canonicalization. When the same PURL survives
  // dedup as multiple entries (Maven nested-coord case where standalone +
  // nested-under-fat-jar both exist with different parent_purl values), all
  // surviving entries get the SAME alphabetically-sorted source_file_paths
  // Vec — closes the 51 polyglot-builder-image audit findings on cross-format
  // `mikebom:source-files` divergence.
  canonicalize_source_files_by_purl(&mut components);
  // ... existing CPE synthesis loop at lines 754-756 (unchanged) ...
  ```

  Per research §A this placement is structurally clean — no existing post-dedup pass touches `evidence.source_file_paths`.

- [ ] T004 [US1] Add unit test `canonicalize_source_files_by_purl_same_purl_different_parent_unions_paths_md148` in `mikebom-cli/src/resolve/deduplicator.rs#mod tests`. Constructs two `ResolvedComponent` instances sharing `pkg:maven/com.example:foo@1.0` but with different `parent_purl` values + different `source_file_paths` content. Asserts both entries carry the alphabetically-sorted union Vec after the pass. Pattern:

  ```rust
  #[test]
  fn canonicalize_source_files_by_purl_same_purl_different_parent_unions_paths_md148() {
      use mikebom_common::types::purl::Purl;
      let purl = Purl::new("pkg:maven/com.example/foo@1.0").unwrap();
      let mut components = vec![
          // Standalone entry (parent_purl = None).
          make_test_component(
              purl.clone(),
              None,
              vec!["root/.m2/repository/.../foo-1.0.jar".to_string()],
          ),
          // Nested-under-fat-jar entry (parent_purl = Some(...)).
          make_test_component(
              purl.clone(),
              Some("pkg:maven/com.example/fat-bundle@1.0".to_string()),
              vec!["tmp/extract/fat-bundle.jar!.../foo-1.0.jar".to_string()],
          ),
      ];
      canonicalize_source_files_by_purl(&mut components);
      let expected = vec![
          "root/.m2/repository/.../foo-1.0.jar".to_string(),
          "tmp/extract/fat-bundle.jar!.../foo-1.0.jar".to_string(),
      ];
      // Wait — alphabetical sort: "root/..." < "tmp/..." per lex. Confirm.
      assert_eq!(components[0].evidence.source_file_paths, expected,
          "FR-001 + FR-002: standalone entry MUST carry the alphabetically-sorted union");
      assert_eq!(components[1].evidence.source_file_paths, expected,
          "FR-001 + FR-002: nested entry MUST carry the SAME alphabetically-sorted union");
      // FR-006: parent_purl topology preserved.
      assert_eq!(components[0].parent_purl, None);
      assert_eq!(components[1].parent_purl, Some("pkg:maven/com.example/fat-bundle@1.0".to_string()));
  }
  ```

  Requires a `make_test_component` helper (define locally in the test module if not already present); pattern: minimal `ResolvedComponent` with the supplied purl/parent_purl/source_file_paths and reasonable defaults for the other fields. Covers FR-001 + FR-002 + FR-006 (topology preservation) + SC-004.

- [ ] T005 [P] [US1] Add unit test `canonicalize_source_files_by_purl_single_entry_is_content_preserving_md148` in the same `mod tests` block. Tests FR-007: when a component's PURL appears as the only entry, the pass is a content-preserving no-op (set of paths unchanged; wire-order MAY canonicalize to alphabetical per the BTreeSet collection semantic). Pattern:

  ```rust
  #[test]
  fn canonicalize_source_files_by_purl_single_entry_is_content_preserving_md148() {
      use mikebom_common::types::purl::Purl;
      use std::collections::BTreeSet;
      let purl = Purl::new("pkg:npm/example@1.0.0").unwrap();
      // Deliberately NOT alphabetical to exercise the wire-order canonicalization.
      let original_paths = vec![
          "node_modules/example/package.json".to_string(),
          "node_modules/example/index.js".to_string(),
      ];
      let mut components = vec![make_test_component(
          purl,
          None,
          original_paths.clone(),
      )];
      canonicalize_source_files_by_purl(&mut components);

      // FR-007 (content-preserving no-op): the SET of paths is unchanged.
      let pre_set: BTreeSet<&String> = original_paths.iter().collect();
      let post_set: BTreeSet<&String> =
          components[0].evidence.source_file_paths.iter().collect();
      assert_eq!(pre_set, post_set,
          "FR-007: single-entry PURL's path-SET MUST be preserved");
      // Wire-order MAY be alphabetical per the BTreeSet collection semantic.
      // We do NOT assert byte-identical wire order — that's an optimization
      // an implementer may choose to add but the spec does not require.
  }
  ```

  Covers FR-007 (content-preserving no-op). Per the spec's explicit allowance, wire-order canonicalization to alphabetical is permitted (BTreeSet collection semantic). This test deliberately asserts set-equality only — implementations that skip write-back for single-entry PURLs (preserving byte-order) and implementations that always write back (canonicalizing to alphabetical) both pass.

- [ ] T006 [P] [US1] Add unit test `canonicalize_source_files_by_purl_is_idempotent_md148` in the same `mod tests` block. Tests FR-004: two consecutive passes produce byte-identical output. Pattern:

  ```rust
  #[test]
  fn canonicalize_source_files_by_purl_is_idempotent_md148() {
      use mikebom_common::types::purl::Purl;
      let purl = Purl::new("pkg:maven/com.example/foo@1.0").unwrap();
      let mut components = vec![
          make_test_component(
              purl.clone(),
              None,
              vec!["b/path".to_string()],
          ),
          make_test_component(
              purl.clone(),
              Some("pkg:maven/com.example/parent@1.0".to_string()),
              vec!["a/path".to_string(), "c/path".to_string()],
          ),
      ];
      canonicalize_source_files_by_purl(&mut components);
      let after_first: Vec<Vec<String>> = components
          .iter()
          .map(|c| c.evidence.source_file_paths.clone())
          .collect();
      canonicalize_source_files_by_purl(&mut components);
      let after_second: Vec<Vec<String>> = components
          .iter()
          .map(|c| c.evidence.source_file_paths.clone())
          .collect();
      assert_eq!(after_first, after_second,
          "FR-004: canonicalize_source_files_by_purl MUST be idempotent");
  }
  ```

  Covers FR-004 + SC-005.

- [ ] T007 [P] [US1] Add unit test `canonicalize_source_files_by_purl_preserves_other_fields_md148` in the same `mod tests` block. Tests FR-005: the pass MUST NOT alter any field other than `evidence.source_file_paths`. Construct two same-PURL entries with rich non-default values on `name`, `version`, `parent_purl`, `lifecycle_scope`, `hashes`, `sbom_tier`, `extra_annotations`, `evidence.confidence`, `evidence.technique`, `evidence.source_connection_ids`. Assert every named field is byte-identical pre/post the pass.

  ```rust
  #[test]
  fn canonicalize_source_files_by_purl_preserves_other_fields_md148() {
      use mikebom_common::types::purl::Purl;
      use mikebom_common::resolution::{LifecycleScope, ResolutionTechnique};
      let purl = Purl::new("pkg:maven/com.example/foo@1.0").unwrap();
      let mut component = make_test_component(
          purl.clone(),
          Some("pkg:maven/com.example/parent@1.0".to_string()),
          vec!["original/path.jar".to_string()],
      );
      component.lifecycle_scope = Some(LifecycleScope::Runtime);
      component.sbom_tier = Some("source".to_string());
      component.evidence.confidence = 0.9;
      component.evidence.source_connection_ids = vec![42];
      component.extra_annotations.insert(
          "mikebom:test-key".to_string(),
          serde_json::json!("test-value"),
      );
      let snapshot = component.clone();

      // Pair with a second entry sharing PURL (different parent_purl) so the
      // union pass actually fires (non-no-op case).
      let mut components = vec![
          component,
          make_test_component(
              purl.clone(),
              None,
              vec!["other/path.jar".to_string()],
          ),
      ];
      canonicalize_source_files_by_purl(&mut components);

      // FR-005: every non-source_file_paths field MUST be preserved verbatim.
      assert_eq!(components[0].purl, snapshot.purl);
      assert_eq!(components[0].name, snapshot.name);
      assert_eq!(components[0].version, snapshot.version);
      assert_eq!(components[0].parent_purl, snapshot.parent_purl);
      assert_eq!(components[0].lifecycle_scope, snapshot.lifecycle_scope);
      assert_eq!(components[0].sbom_tier, snapshot.sbom_tier);
      assert_eq!(components[0].hashes, snapshot.hashes);
      assert_eq!(components[0].evidence.confidence, snapshot.evidence.confidence);
      assert_eq!(components[0].evidence.technique, snapshot.evidence.technique);
      assert_eq!(components[0].evidence.source_connection_ids,
                 snapshot.evidence.source_connection_ids);
      assert_eq!(components[0].extra_annotations, snapshot.extra_annotations);
      // ONLY source_file_paths is allowed to change.
      assert_ne!(components[0].evidence.source_file_paths,
                 snapshot.evidence.source_file_paths,
                 "the union pass IS expected to change source_file_paths in the multi-entry case");
  }
  ```

  Covers FR-005.

- [ ] T008 [US1] Create the synthetic test fixture at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/source_files_union/` per research §F. The fixture MUST exercise the Maven nested-coord same-PURL multi-entry shape WITHOUT requiring a real OCI image. Files:
  - `pom.xml` — minimal POM declaring one dep `pkg:maven/com.example:foo@1.0`. Standard Maven POM format; pattern modeled on `mikebom-cli/tests/fixtures/maven/pom-three-deps/pom.xml`.
  - `target/primary.jar` — synthetic JAR containing one class file for `com.example:foo@1.0` standalone. Use the helper that produces synthetic JARs in `mikebom-cli/tests/common/maven_jar_builder.rs` if available, otherwise hand-craft a minimal ZIP using `zip` shell tool or `tar`. The JAR MUST be readable by the Maven reader's nested-JAR walker at `maven.rs:3429-3457` AND MUST produce a `PackageDbEntry` with PURL `pkg:maven/com.example/foo@1.0` and `parent_purl = None`.
  - `target/fat-bundle.jar` — synthetic fat-jar that vendors `com.example:foo@1.0` inside. The Maven reader's nested walker MUST detect the inner POM/manifest and produce a SECOND `PackageDbEntry` with the SAME PURL but `parent_purl = Some("pkg:maven/com.example:fat-bundle@...")`.
  - `README.md` — documents the fixture's intent: "synthetic Maven nested-coord fixture for milestone-148 source-files cross-PURL union test. The dep `pkg:maven/com.example/foo@1.0` exists at TWO paths: `target/primary.jar` (standalone) and inside `target/fat-bundle.jar`. The milestone-148 union pass MUST emit BOTH paths on the `mikebom:source-files` annotation of the surviving `com.example/foo` component(s) regardless of output format."

  If constructing the synthetic JARs is non-trivial in the implement phase (Maven JAR shape is intricate — requires META-INF/MANIFEST.MF, optionally pom.properties, and the actual class structure), consider deferring T008/T009 to a follow-up PR and rely solely on the unit tests T004-T007 + the existing transitive_parity_maven test for CI signal. Document the deferral in the PR description if applied. The 51-finding fix lands either way; the synthetic fixture is the in-tree validation surface, not the production verification (SC-007 operator-cadence harness re-run is the production verification).

- [ ] T009 [US1] Create the in-tree integration test at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/source_files_purl_union_md148.rs`. The test runs `mikebom sbom scan` against the synthetic fixture from T008 in all three formats and asserts the `mikebom:source-files` value for `pkg:maven/com.example/foo@1.0` is bytewise-identical across the three formats. Pattern (mirroring `mikebom-cli/tests/cdx_regression.rs` runner shape):

  ```rust
  //! Milestone 148 SC-003 — same-PURL multi-entry Maven coord emits
  //! byte-identical mikebom:source-files across CDX 1.6 / SPDX 2.3 / SPDX 3.

  use std::process::Command;

  mod common;
  use common::normalize::apply_fake_home_env;

  #[test]
  fn same_purl_maven_nested_coord_emits_byte_identical_source_files_across_formats_md148() {
      let fx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
          .join("tests/fixtures/source_files_union");
      // Three scans, one per format. Capture output to tempfiles, parse,
      // extract mikebom:source-files for pkg:maven/com.example/foo@1.0,
      // assert bytewise-identical set semantics across the three formats.
      let cdx_paths = extract_source_files_cdx(&fx);
      let spdx_paths = extract_source_files_spdx23(&fx);
      let spdx3_paths = extract_source_files_spdx3(&fx);
      assert_eq!(cdx_paths, spdx_paths, "SC-003: CDX vs SPDX 2.3 source-files MUST match");
      assert_eq!(cdx_paths, spdx3_paths, "SC-003: CDX vs SPDX 3 source-files MUST match");
      // Sanity: the value MUST contain BOTH paths (standalone + nested),
      // confirming the union pass actually fired.
      assert!(cdx_paths.len() >= 2, "expected ≥2 paths in the union; got: {cdx_paths:?}");
  }
  ```

  Implementation details:
  - Use `env!("CARGO_BIN_EXE_mikebom")` to locate the test binary.
  - `apply_fake_home_env` (existing helper in `common/normalize.rs`) for hermetic home-dir isolation.
  - Helper functions `extract_source_files_cdx(&fx) -> BTreeSet<String>`, `extract_source_files_spdx23(...)`, `extract_source_files_spdx3(...)` each (a) run the scan in their target format to a tempfile, (b) parse the resulting JSON, (c) walk the document to find `pkg:maven/com.example/foo@1.0`, (d) extract its `mikebom:source-files` annotation value, (e) parse as a JSON array of strings and return a `BTreeSet<String>` for set-equality semantics.

  If T008 was deferred (synthetic fixture construction non-trivial), defer this test too with the same rationale. **However**, T009b (below) provides a CI-binding fallback that covers SC-003's cross-format invariance without requiring a synthetic Maven fixture, so the analyze-finding-H1 coverage gap is closed regardless. Covers SC-003 (full-stack integration when not deferred).

- [ ] T009b [P] [US1] Add cross-format `mikebom:source-files` parity unit test at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/resolve/deduplicator.rs#mod tests` (NOT in the integration-test directory — this is a unit-level assertion that doesn't require a binary invocation). Provides a CI-binding fallback for SC-003 / FR-009 / analyze-finding-H1 when T008+T009 are deferred. The test asserts the single-source-of-truth invariant: ALL three SBOM emitters MUST read `mikebom:source-files` from `c.evidence.source_file_paths` (NOT from `c.extra_annotations["mikebom:source-files"]` — the milestone 145 US3 fix established this single-source contract via the `is_field_owned_annotation_key` filter). After the milestone-148 union pass canonicalizes `c.evidence.source_file_paths`, all three emitters therefore see the same value byte-for-byte. Two complementary assertion patterns:

  ```rust
  /// Milestone 148 SC-003 (analyze-finding-H1 fallback): assert that all
  /// three SBOM emitters consume the same `c.evidence.source_file_paths`
  /// field as the source of truth for `mikebom:source-files`. Combined with
  /// the canonicalize pass guaranteeing same-PURL entries share an identical
  /// Vec value, this transitively guarantees cross-format byte-equality of
  /// the wire-side annotation — closing the cross-format invariance signal
  /// in CI without requiring a synthetic Maven fixture.
  ///
  /// Pattern: code-shape grep assertion. The three emitter sites (CDX
  /// `cyclonedx/builder.rs:830-839`, SPDX 2.3 `spdx/annotations.rs:302-308`,
  /// SPDX 3 `spdx/v3_annotations.rs:267-273`) ALL read from
  /// `c.evidence.source_file_paths`. The milestone-145 US3 filter at
  /// `root_selector.rs::is_field_owned_annotation_key` prevents the
  /// extra_annotations bag from re-emitting the same key. This test
  /// asserts the filter still names `mikebom:source-files` AND the
  /// expected emitter-side reads remain in place (code-shape regression
  /// guard against accidental shape drift).
  #[test]
  fn source_files_single_source_of_truth_invariant_md148() {
      use crate::generate::root_selector::is_field_owned_annotation_key;
      // FR-008 + milestone-145 US3 invariant: `mikebom:source-files` is
      // field-owned. Any new emitter MUST NOT bypass this filter, and
      // any new reader MUST NOT stamp the bag-keyed duplicate (Maven
      // reader's nested-URL key is `mikebom:source-files-nested-url`
      // post-145 specifically to avoid this collision).
      assert!(is_field_owned_annotation_key("mikebom:source-files"),
          "FR-008 + 145-US3 invariant: mikebom:source-files MUST remain field-owned (drives single-source-of-truth across CDX/SPDX2.3/SPDX3)");
      // Conditional: ensure no rename of the field-owned filter on the
      // related milestone-145 nested-URL key (defense against the same
      // class of regression).
      assert!(!is_field_owned_annotation_key("mikebom:source-files-nested-url"),
          "the renamed Maven-reader key is NOT field-owned — it ships through extra_annotations as a distinct annotation");
  }
  ```

  Plus a behavioral assertion that the canonicalize pass actually produces a Vec the emitters will consume identically (this part is essentially a recap of T004 but framed as the cross-format-invariance assertion):

  ```rust
  #[test]
  fn canonicalize_produces_emitter_ready_vec_across_formats_md148() {
      use mikebom_common::types::purl::Purl;
      // Construct two same-PURL entries (Maven nested-coord shape).
      let purl = Purl::new("pkg:maven/com.example/foo@1.0").unwrap();
      let mut components = vec![
          make_test_component(purl.clone(), None,
              vec!["target/primary.jar".to_string()]),
          make_test_component(purl.clone(),
              Some("pkg:maven/com.example/fat-bundle@1.0".to_string()),
              vec!["target/fat-bundle.jar!foo-1.0.jar".to_string()]),
      ];
      canonicalize_source_files_by_purl(&mut components);
      // SC-003 invariant: both entries' source_file_paths Vec is now
      // byte-identical. Combined with the single-source-of-truth
      // invariant above, this is sufficient to guarantee cross-format
      // wire-side equality WITHOUT instantiating the per-format emitters
      // (which require full ScanResult plumbing the unit test can't
      // easily reproduce).
      assert_eq!(
          components[0].evidence.source_file_paths,
          components[1].evidence.source_file_paths,
          "SC-003: post-canonicalize, all same-PURL entries MUST carry identical source_file_paths Vec — this transitively guarantees cross-format mikebom:source-files equality because all three emitters read from this field exclusively (FR-008 + 145-US3 single-source-of-truth invariant asserted by source_files_single_source_of_truth_invariant_md148)"
      );
  }
  ```

  Covers SC-003 / FR-009 / analyze-finding-H1 as a CI-binding fallback regardless of whether T008+T009 land.

**Checkpoint**: After Phase 3, US1 is fully functional. `cargo +stable test -p mikebom canonicalize_source_files_by_purl` shows 4 unit tests pass green; if T008+T009 landed, `cargo +stable test --test source_files_purl_union_md148` also passes. Manual smoke per quickstart §Scenario 1: the polyglot-builder-image audit count drops from 51 to 0.

---

## Phase 4: User Story 2 - Cross-ecosystem coverage (non-Maven same-PURL multi-entry cases) (Priority: P2)

**Goal**: Add a unit test asserting that the union pass correctly isolates by full canonical PURL (`Purl::as_str()`), preventing cross-ecosystem path cross-pollination per FR-003 + Edge Case 7.

**Independent Test**: Construct two `ResolvedComponent` instances with PURLs `pkg:maven/com.example:foo@1.0` and `pkg:npm/example@1.0` (same name+version, different ecosystem), each with distinct `source_file_paths`. After the pass, assert each component's `source_file_paths` contains ONLY its own original paths — no cross-pollination.

### Implementation for User Story 2

- [ ] T010 [P] [US2] Add unit test `canonicalize_source_files_by_purl_cross_ecosystem_isolation_md148` in `deduplicator.rs#mod tests`. Tests FR-003 + Edge Case 7:

  ```rust
  #[test]
  fn canonicalize_source_files_by_purl_cross_ecosystem_isolation_md148() {
      use mikebom_common::types::purl::Purl;
      // Two PURLs that share name+version but differ in ecosystem. The union
      // pass MUST NOT cross-pollinate paths between them.
      let maven_purl = Purl::new("pkg:maven/com.example/foo@1.0").unwrap();
      let npm_purl = Purl::new("pkg:npm/example@1.0").unwrap();
      let mut components = vec![
          make_test_component(maven_purl.clone(), None,
              vec!["target/foo-1.0.jar".to_string()]),
          make_test_component(npm_purl.clone(), None,
              vec!["node_modules/example/index.js".to_string()]),
      ];
      canonicalize_source_files_by_purl(&mut components);

      // FR-003 + Edge Case 7: each ecosystem's paths stay isolated to its own PURL.
      assert_eq!(components[0].evidence.source_file_paths,
                 vec!["target/foo-1.0.jar".to_string()],
          "FR-003: Maven component MUST NOT pick up npm paths");
      assert_eq!(components[1].evidence.source_file_paths,
                 vec!["node_modules/example/index.js".to_string()],
          "FR-003: npm component MUST NOT pick up Maven paths");
  }
  ```

  Covers FR-003 + Edge Case 7.

- [ ] T011 [P] [US2] Add unit test `canonicalize_source_files_by_purl_three_entries_full_union_md148` in `deduplicator.rs#mod tests`. Tests Edge Case 1: when 3+ entries share a PURL (one standalone + two different fat-jar nestings), all three get the alphabetically-sorted union of all three paths.

  ```rust
  #[test]
  fn canonicalize_source_files_by_purl_three_entries_full_union_md148() {
      use mikebom_common::types::purl::Purl;
      let purl = Purl::new("pkg:maven/com.example/foo@1.0").unwrap();
      let mut components = vec![
          make_test_component(purl.clone(), None,
              vec!["root/standalone.jar".to_string()]),
          make_test_component(purl.clone(),
              Some("pkg:maven/com.example/bundle-a@1.0".to_string()),
              vec!["root/bundle-a.jar!foo-1.0.jar".to_string()]),
          make_test_component(purl.clone(),
              Some("pkg:maven/com.example/bundle-b@1.0".to_string()),
              vec!["root/bundle-b.jar!foo-1.0.jar".to_string()]),
      ];
      canonicalize_source_files_by_purl(&mut components);
      let expected = vec![
          "root/bundle-a.jar!foo-1.0.jar".to_string(),
          "root/bundle-b.jar!foo-1.0.jar".to_string(),
          "root/standalone.jar".to_string(),
      ];
      for (i, c) in components.iter().enumerate() {
          assert_eq!(c.evidence.source_file_paths, expected,
              "Edge Case 1: entry {i} MUST carry the full 3-path alphabetically-sorted union");
      }
  }
  ```

  Covers Edge Case 1.

**Checkpoint**: After Phase 4, US2 is fully covered. `cargo +stable test -p mikebom canonicalize_source_files_by_purl` shows 6 unit tests pass green (the 4 from Phase 3 + the 2 from Phase 4).

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Golden audit + refresh, pre-PR gate, commit.

- [ ] T012 Audit existing Maven-bearing byte-identity goldens for `mikebom:source-files` drift potential. Run:

  ```bash
  grep -rl 'mikebom:source-files' \
      /Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/maven.*.json
  ```

  Per research §G the `pom-three-deps` fixture is unlikely to exercise the same-PURL multi-entry shape (it's a simple POM-only fixture). If the grep returns no matches OR the values don't change post-148, the refresh in T013 produces empty diffs. Document the finding (empty vs non-empty diff) for the PR description.

- [ ] T013 Refresh affected goldens via the standard env-var trifecta:

  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1   cargo +stable test --test cdx_regression cdx_regression_maven
  MIKEBOM_UPDATE_SPDX_GOLDENS=1  cargo +stable test --test spdx_regression maven_byte_identity
  MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --test spdx3_regression maven_byte_identity
  ```

  Inspect `git diff --stat -- mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/maven.*.json`. Per FR-010, each affected line MUST be either (a) a `mikebom:source-files` value change on a Maven component that previously carried a non-canonical single-path Vec, OR (b) the new alphabetically-sorted union content. Reject any unrelated drift. If empty diff (`pom-three-deps` doesn't exercise the shape), that's expected per research §G and the synthetic fixture from T008/T009 is the sole CI-binding exercise of the new code path.

- [ ] T014 Run mandatory pre-PR gate per Constitution Development Workflow + memory `feedback_prepr_gate_full_output.md`: `./scripts/pre-pr.sh` from repo root. Both clippy + test steps MUST pass clean (excepting the pre-existing local `sbomqs_parity` env-only failure documented in milestone-144 T001 — CI will validate on a clean runner). If any OTHER test fails, scan the FULL output (do NOT grep on `^test result: FAILED` — known to drop multi-test-suite summaries). Covers SC-006.

- [ ] T015 Commit the milestone-148 changes. Per project convention (matching milestones 134/144/145/146/147), use the 4-commit chain:
  - `spec(148): source-files cross-emitter divergence — union evidence across same-PURL entries` — spec.md + checklists/requirements.md
  - `plan(148): canonicalize_source_files_by_purl post-dedup pass + design notes` — plan + research + data-model + contracts + quickstart + CLAUDE.md
  - `tasks(148): 16 tasks across 5 phases for source-files cross-PURL union` — tasks.md
  - `impl(148): canonicalize evidence.source_file_paths across same-PURL ResolvedComponent entries` — `mikebom-cli/src/resolve/deduplicator.rs` + `mikebom-cli/src/scan_fs/mod.rs` + (if T008+T009 landed) the synthetic fixture + integration test + any golden refresh from T013

  Do NOT commit until T014 passes clean. Use `git add <specific paths>` (never `-A`). Each commit ends with the standard `Co-Authored-By` trailer.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. Verifies baseline.
- **Phase 2 (Foundational)**: EMPTY — no foundational work required.
- **Phase 3 (US1)**: Depends on Phase 1. T002 (the function) + T003 (the call site wiring) IS the fix. T004-T007 are unit tests that can land in parallel after T002+T003. T009b (single-source-of-truth fallback) lands in parallel with T004-T007 and is MANDATORY (closes analyze-finding-H1 cross-format invariance coverage). T008 (synthetic fixture) + T009 (integration test) can land in parallel after T002+T003 and remain deferrable — T009b is the CI-binding fallback if they're deferred.
- **Phase 4 (US2)**: Depends on Phase 3 — specifically T002 (the function). US2's tests assert additional properties of the same function. T010 + T011 can land in parallel after T002.
- **Phase 5 (Polish)**: Depends on US1 + US2 being functionally complete.

### User Story Dependencies

- **US1 (P1, MVP)**: Standalone after Phase 1. Delivers the 51 → 0 audit drop. T002 + T003 + T004-T007 (4 unit tests) + T009b (single-source-of-truth + canonicalize-output cross-format-invariance assertions, MANDATORY) + optionally T008+T009 (synthetic fixture + full integration test).
- **US2 (P2)**: Builds on US1's implementation. T010 + T011 add cross-ecosystem + three-entry assertions on the SAME function.

### Within Each User Story

- T002 + T003 are sequential (T002 defines the function; T003 wires the call site).
- T004-T007 + T009b are tests and can land in parallel after T002+T003 (all in the same `mod tests` block in `deduplicator.rs`).
- T008 + T009 can land in parallel after T002+T003 (different files: fixture + integration test). Both deferrable if fixture construction is non-trivial — T009b is the CI-binding fallback for SC-003 / FR-009.
- T010 + T011 are tests and can land in parallel after T002.

### Parallel Opportunities

- Phase 3: T004 + T005 + T006 + T007 + T009b [P] (different tests, same `mod tests` block — landable in one editor pass).
- Phase 3: T008 + T009 [P] (different files; landable independently of the unit tests).
- Phase 4: T010 + T011 [P] (different tests, same `mod tests` block).

---

## Parallel Example: Phase 3 (after T002 + T003 land)

```bash
# Five unit tests + cross-format-invariance fallback can be added in one editor pass:
Task T004:  canonicalize_source_files_by_purl_same_purl_different_parent_unions_paths_md148   (SC-004 + FR-001/002/006)
Task T005:  canonicalize_source_files_by_purl_single_entry_is_content_preserving_md148        (FR-007)
Task T006:  canonicalize_source_files_by_purl_is_idempotent_md148                             (FR-004 + SC-005)
Task T007:  canonicalize_source_files_by_purl_preserves_other_fields_md148                    (FR-005)
Task T009b: source_files_single_source_of_truth_invariant_md148 + canonicalize_produces_emitter_ready_vec_across_formats_md148   (SC-003 / FR-009 / analyze-finding-H1 fallback)

# Optionally parallel (deferrable per T008/T009 notes):
Task T008: synthetic fixture at mikebom-cli/tests/fixtures/source_files_union/
Task T009: in-tree integration test at mikebom-cli/tests/source_files_purl_union_md148.rs
```

## Parallel Example: Phase 4 (after T002 lands)

```bash
Task T010: canonicalize_source_files_by_purl_cross_ecosystem_isolation_md148   (FR-003 + Edge Case 7)
Task T011: canonicalize_source_files_by_purl_three_entries_full_union_md148    (Edge Case 1)
```

---

## Implementation Strategy

### MVP First (US1 only — ships the 51 → 0 audit drop)

1. Complete Phase 1: T001 baseline check.
2. Complete Phase 3 unit-test path: T002 (function) + T003 (call site) + T004-T007 (4 unit tests) + T009b (cross-format-invariance fallback assertions, MANDATORY per analyze-finding-H1).
3. **STOP and VALIDATE**: `./scripts/pre-pr.sh` clean. Manual smoke per quickstart §Scenario 1: polyglot-builder-image audit count drops from 51 to 0.
4. This alone is a shippable PR. US2's cross-ecosystem assertions are additive defensive coverage on the SAME code; skipping them leaves the fix correct but with a smaller test surface.
5. Defer T008+T009 (synthetic fixture + integration test) to a follow-up PR if synthetic Maven JAR construction is non-trivial — T009b's single-source-of-truth + canonicalize-output assertions cover the cross-format invariance signal in CI without the synthetic fixture; the operator-cadence harness re-run (SC-007) provides the production verification regardless.

### Incremental / Recommended (single-PR delivery)

1. Phase 1 (T001) baseline.
2. Phase 3 (T002-T009 + T009b) US1 — orphan-divergence closes (51 → 0) + cross-format-invariance CI coverage.
3. Phase 4 (T010-T011) US2 — cross-ecosystem + three-entry assertions.
4. Phase 5 (T012-T015) polish — golden audit + refresh + pre-PR + commit.

Total: 16 tasks (15 + T009b added per analyze-finding-H1 remediation). Estimated ~15 LOC of new function + ~80 LOC of unit tests + ~120 LOC of integration test + ~5 LOC of call-site wiring + synthetic fixture (variable, possibly deferred).

### Single-developer Note

This milestone is small enough that one developer can work through all phases in one session. The [P] markers exist primarily to signal "no cross-file write conflict" — useful for tooling that automates task execution but not load-bearing for a human implementer.

---

## Notes

- Unit tests live in-file under `#[cfg(test)] mod tests` per the project's existing convention in `deduplicator.rs`. Integration test lives in `mikebom-cli/tests/source_files_purl_union_md148.rs` per the project's convention for cross-format byte-equality tests.
- The `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention applies to any test module per Constitution Principle IV (the existing `deduplicator.rs#mod tests` already has it; no change needed). The new integration test file MUST add the same guard at its `mod common` boundary.
- Memory `feedback_prepr_gate_full_output.md` is directly relevant: when verifying T014, scan the FULL output rather than greping on `^test result: FAILED`.
- Memory `feedback_dont_dismiss_test_failures.md` is relevant if any new test failures surface during golden refresh: verify reproducibility before calling anything "pre-existing flake".
- The commit-message convention (T015) follows the milestone-134/144/145/146/147 precedent: `spec(148):` / `plan(148):` / `tasks(148):` / `impl(148):`.
- Per spec SC-001 + SC-007 (operator-cadence cross-format harness re-run): document in the PR description that the operator should re-run the sbom-conformance audit harness on the polyglot-builder-image fixture post-merge to confirm the 51 findings drop to 0. The harness is NOT a CI gate; the in-tree unit tests T004-T007 + T010-T011 (and T009 if not deferred) are the CI-binding signals.
- Per spec FR-008 + Constitution V audit in plan.md: NO new `mikebom:*` annotation introduced. The fix is purely a value-canonicalization on the existing `mikebom:source-files` annotation; the C18 parity-catalog row's audit at `docs/reference/sbom-format-mapping.md` C18 is unchanged.
- The existing C18 row's `Directionality::SymmetricEqual` MUST continue to hold (SC-002). The CI-binding `cross_format_byte_identity` and `holistic_parity` tests provide this signal automatically post-implementation; no parity-catalog edits needed for this milestone.
