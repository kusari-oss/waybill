# Tasks: SPDX 3 native dependency completeness

**Input**: `specs/1069-spdx3-relationship-completeness/` (spec.md, plan.md, research.md, data-model.md, contracts/spdx3-completeness.md, quickstart.md, measurements/)

**Tests**: requested (SC-001…SC-005). Write each test first and see it fail. For enforcement tests, also show they fail with the feature switched off.

`G/` = `waybill-cli/src/generate/`.

**Rules carried over:**
- Catalogue rows go on **one line**.
- Gate with `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo EXIT=$?`.
- Corpus targets run only in CI, so check `tests/corpus_harness_195/manifest.rs` before assuming any target is unaffected (m1068 lesson).
- Corpus run IDs come from the dispatch output.

## Phase 1: Setup

- [X] T001 Baseline: run `cargo test -p waybill --bin waybill generate::cyclonedx`, `cargo test -p waybill --bin waybill generate::spdx`, `cargo test -p waybill --lib parity`, and `cargo test -p waybill --test spdx3_regression --test cdx_regression --test spdx_regression`. Record them green.

## Phase 2: Foundational — one predicate for both formats (research R4)

- [X] T002 In `G/cyclonedx/compositions.rs`, extract `pub(crate) struct DependencyClaims { complete: HashSet<String>, unknown: HashSet<String>, root_complete: bool }` and `pub(crate) fn dependency_claims(components, complete_ecosystems, reachable_set: Option<&HashSet<String>>, degraded_ecosystems, integrity) -> DependencyClaims`. Rules:
  - **all-or-nothing per ecosystem:** an ecosystem in `complete_ecosystems` is resolved iff all its components are reachable and it is not degraded;
  - **unclaimed:** components outside those ecosystems are in neither set;
  - **root:** `root_complete = target_aggregate(integrity) == "incomplete_first_party_only" && !components.is_empty()`.

  Also move the degraded-ecosystem derivation from `G/cyclonedx/builder.rs` (~983–1003) into a `pub(crate) fn degraded_ecosystems(&GraphCompletenessResult) -> HashSet<String>` beside it.
  - **`build_compositions`:** rewrite it to consume `DependencyClaims`. Its output is byte-identical: `cargo test -p waybill --test cdx_regression` and the CycloneDX tests pass with no golden change.
- [X] T003 [P] Unit tests for `dependency_claims` in `G/cyclonedx/compositions.rs::tests`:
  - an ecosystem fully reachable and not degraded → all `complete`;
  - one unreachable component → the whole ecosystem `unknown`;
  - a degraded ecosystem (`GoTransitiveCoverageDegraded`) → `unknown` even though reachable;
  - an ecosystem not in `complete_ecosystems` → in neither set;
  - `root_complete` true for clean integrity, false with attach failures or no components.

**Checkpoint**: CycloneDX unchanged; one predicate available to SPDX 3.

## Phase 3: User Story 1 — standards-only readers see completeness (Priority: P1) 🎯 MVP

**Goal**: SPDX 3 dependency relationships are grouped per `(from, type, scope)` and carry `completeness` per the contract table, agreeing with CycloneDX.

**Independent test**: one artifact set with a complete ecosystem, an unknown ecosystem (one leaf without edges), an unclaimed ecosystem and a root, emitted to CycloneDX and SPDX 3. Every component's SPDX 3 completeness matches its CycloneDX claim (SC-002).

### Tests for User Story 1

- [X] T004 [P] [US1] In `G/spdx/v3_relationships.rs::tests`, grouping-pass unit tests for `group_dependency_relationships(rels, &claims, root_iri, purl_by_iri)`:
  - three per-edge `dependsOn` from one component → one relationship, `to` sorted and deduplicated;
  - a `LifecycleScopedRelationship` (`scope: development`) from the same component stays a separate grouped relationship;
  - non-`dependsOn` relationships (`contains`, `describes`) are untouched;
  - the grouped IRI is deterministic and differs between different target sets;
  - each row of `contracts/spdx3-completeness.md`:
    - complete → `complete`;
    - unknown → `incomplete`;
    - unknown leaf → one added `to: ["NoAssertionElement"]`, `noAssertion`;
    - root with `root_complete` → `complete`, and unqualified when false;
    - unclaimed → no `completeness` and nothing added;
    - complete leaf → nothing added.
- [X] T005 [P] [US1] In `G/spdx/mod.rs::tests`, a cross-format agreement test (SC-002).
  - **Setup:** build artifacts with `mk_artifacts`, setting `complete_ecosystems` (for example `["cargo", "npm"]`) and components spanning the four cases:
    - **complete:** a cargo component reached from the root;
    - **unknown:** an npm component with no incoming edge, which makes npm unreachable and so `unknown`, plus one npm leaf;
    - **unclaimed:** a pypi component, since pypi is not in `complete_ecosystems`;
    - **root:** the main module.

    Degradation by reason code is exercised in T003 only.
  - **Serialize:** to CycloneDX and SPDX 3.
  - **Map both to component PURLs:** CycloneDX `compositions[].dependencies` by aggregate; SPDX 3 `completeness` per `from`.
  - **Assert the FR-003 table:**
    - every CycloneDX `unknown` component is `incomplete` or `noAssertion` in SPDX 3;
    - every CycloneDX `complete` component with a relationship is `complete`;
    - everything else is unqualified.
  - **Root override:** repeat with `arts.root_override` set, so the main module is dropped. The root relationship's completeness still follows `root_complete`.
  - **Same reachable set (analysis I1):** assert that the `reachable_set` SPDX 3 computes equals the one CycloneDX computes, for these artifacts and for the root-override case. Agreement by construction holds only if both formats run the shared predicate on the same reachability.
  - Show it fails with T008's pass disabled (local edit, reverted).
- [X] T006 [P] [US1] Conformance: extend the existing SPDX 3 conformance test path (the milestone-078 `spdx3-validate` integration) so a document carrying grouped relationships, all three `completeness` values and a `NoAssertionElement` relationship is validated, using the T005 artifacts written to a tempdir.
  - **Done via the existing gate:** `spdx3_conformance::every_existing_golden_passes_validator` validates every in-repo SPDX 3 golden. After T010 they carry `complete` (cargo, gem and others), `incomplete` and `noAssertion` plus `NoAssertionElement` (golang), so no new harness was needed. It passed 17/17 with `WAYBILL_REQUIRE_SPDX3_VALIDATOR=1`.
  - If that suite only validates fixture scans, add one in-crate-built document to it. If no harness accepts an in-memory document, run `measurements/probe_validator.py` on a regenerated document in T017 instead, and note it here.

### Implementation for User Story 1

- [X] T007 [US1] In `G/spdx/v3_document.rs`, move `compute_graph_completeness` (~864) above the relationship build (~673). Its inputs, `scan.components` and `m194_classifier_relationships`, already exist there.
  - Derive `degraded_ecosystems(...)` and `dependency_claims(...)` with the same arguments CycloneDX uses: `scan.complete_ecosystems`, `Some(&gc.reachable_set)`, `scan.integrity`.
  - Keep passing the same `GraphCompletenessResult` to `build_document_annotations`, so the annotations are unchanged (FR-004).
  - **If the reachable sets differ** (T005's equality assertion): SPDX 3 runs reachability over a different graph than CycloneDX (`metadata_relationships_augmented` vs `m194_classifier_relationships`). Reconcile the inputs here, not in the predicate. Do not paper over it by recomputing claims per format.
- [X] T008 [US1] In `G/spdx/v3_relationships.rs`, implement `group_dependency_relationships` per `contracts/spdx3-completeness.md` and research R6.
  - **Where:** call it in `G/spdx/v3_document.rs` on `all_relationships` after every producer has pushed (dependency builder, #236 fallback, #1009 supplement anchor) and before sorting.
  - **IRI:** `hash_prefix` over `from|dependsOn|<scope or "">|<sorted targets joined ",">`, 16 characters.
  - **Completeness:** maps from `from`'s PURL (reverse of `package_iri_by_purl`) and the root IRI.
  - **Then:** run T004–T005 to green.

**Checkpoint**: US1 complete.

## Phase 4: User Story 2 — the annotations stay (Priority: P1)

- [X] T009 [P] [US2] In the T005 test, assert that `waybill:graph-completeness`, `waybill:graph-completeness-reason` and each `waybill:orphan-reason` have the same values in SPDX 3 as in the same artifacts' SPDX 2.3 document (byte-unchanged by FR-005), via the parity extractors for those rows. On real output, T015's corpus diff must show no change to these annotations in any `spdx-3.json` (FR-004, SC-005).

## Phase 5: User Story 3 — changes only where specified (Priority: P1)

- [X] T010 [US3] Regenerate in-repo SPDX 3 goldens: `WAYBILL_UPDATE_SPDX3_GOLDENS=1 cargo test -p waybill --test spdx3_regression`.
  - **Review** the `git diff` on `waybill-cli/tests/fixtures/golden/spdx-3/`, checked by a small script: `jq` over old and new with `dependsOn` relationships and the document namespace removed must be identical.
  - **Allowed changes:** dependency relationships regrouped, `completeness` added, `NoAssertionElement` relationships added, and the document namespace/IRI hash if it covers element ids.
  - **Unchanged:** CycloneDX and SPDX 2.3 goldens (`cdx_regression`, `spdx_regression` pass untouched).
- [X] T011 [US3] Fix tests that assumed one relationship per edge: those indexing `to[0]` or counting `dependsOn` relationships as edges (candidates from research R7: `tests/document_integrity.rs`, `tests/identifiers_root_component_override.rs`, `tests/ipk_m190_parity.rs`, `tests/supplement_cdx_integration.rs`, `tests/pants_coursier_jvm_reader.rs`, `tests/corpus_harness_195/*`). Change only the counting, so each still checks the same edges. Found by running the full suite.

## Phase 6: Polish & Cross-Cutting

- [X] T012 [P] In `docs/reference/sbom-format-mapping.md`, update the dependency-edge row's SPDX 3 column, **on one line**: grouped per `(from, type, scope)`; `completeness` from the same predicate as CycloneDX `compositions[]`; `NoAssertionElement` for unknown leaves (FR-007). Run `cargo test -p waybill --lib parity` and `--test sbom_format_mapping_coverage`.
- [X] T013 [P] Add an Unreleased entry to `CHANGELOG.md` covering:
  - SPDX 3 dependency relationships are grouped per component and kind;
  - native `completeness`, agreeing with CycloneDX `compositions[]`;
  - `NoAssertionElement` for unknown dependencies;
  - SPDX 3 consumers counting relationships as edges should count targets;
  - CycloneDX and SPDX 2.3 unchanged.
- [X] T014 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo EXIT=$?`. Require `EXIT=0`, the walker-audit OK line, the passed line, and no failing `test result`.
- [X] T015 Push. Regenerate corpus goldens with two `regen_goldens=true` dispatches (run IDs from the dispatch output).
  - **Determinism:** `diff -r` the two artifacts.
  - **Scope:** for every target, only `spdx-3.json` may differ. `cdx.json` and `spdx-2.3.json` must be byte-identical.
  - **Allowed changes:** check each `spdx-3.json` with `xtask corpus-diff` and the T010 script.
  - **Install:** `rsync`, commit, and re-run read-only.
- [X] T016 Real output, per corpus target, recorded in `measurements/agreement.txt`. Use the unmasked SBOMs the CI corpus run uploads (`corpus-emitted-sboms` artifact) where IRIs and timestamps matter.
  - **Agreement (SC-001, SC-002):** quickstart §1. The CycloneDX `unknown` set must equal the SPDX 3 `incomplete`/`noAssertion` set, and CycloneDX `complete` must equal SPDX 3 `complete`. go-cobra (cold) must show its 8 unknown components marked in SPDX 3.
  - **Shape (SC-003):** quickstart §2. At most one `dependsOn` relationship per `(from, type, scope)`.
  - **Conformance (SC-004):** `spdx3-validate` passes on every unmasked emitted SPDX 3 document.
- [X] T017 Re-run `measurements/probe_validator.py` on a regenerated real SPDX 3 document, since it now carries the shapes natively, and append to `measurements/README.md`.
- [X] T018 Open the PR (closes #878), merge when green, then run `cargo clean`.

## Dependencies & Execution Order

- **Phase order:** Setup → Foundational (T002 → T003) → US1 (T004–T006 tests; T007 → T008) → US2 (T009) → US3 (T010 → T011) → Polish.
- **US2 and US3** verify US1's output; they cannot start before T008.

### Parallel opportunities

- **Phase 2:** T003 ∥ T002's CycloneDX golden run.
- **US1 tests:** T004 ∥ T005 ∥ T006.
- **Polish:** T012 ∥ T013.

## Implementation Strategy

- **MVP:** Phase 2 and US1.
- **Main verification:** the golden regeneration (T010, T015) and its reviewed diff are the bulk of the PR, and the main evidence that nothing else moved.
