# Feature Specification: SPDX 3 duplicate-Annotation-spdxId dedup fix

**Feature Branch**: `166-spdx3-annotation-dedup`
**Created**: 2026-07-05
**Status**: Draft
**Input**: Milestone-165 audit follow-on. Milestone 165's SPDX 3 conformance validation via `spdx3-validate==0.0.5` FAILED on both Kubernetes and ArgoCD emissions with error `More than 1 values on <anno-*>->ns1:statement`. Root cause: mikebom's SPDX 3 annotation build pipeline merges multiple builder outputs (`build_component_annotations`, `build_document_annotations`, `build_supplement_service_annotations`, user-supplied `--metadata-comment` + `--annotator` pairs) into a single `@graph` array WITHOUT deduplicating by `spdxId`. When two builder call paths derive the same `hash(subject_iri | field)` (e.g., a document-scope + component-scope emitter both target the SpdxDocument IRI with the same field), both Annotation elements land in `@graph` with the same `spdxId`, violating SPDX 3.0.1 cardinality (`Annotation.statement` — max 1 per subject).

## Motivation

Milestone-165 audit measurement (2026-07-05, live upstream `github.com/kubernetes/kubernetes` @ `688614f2` + `github.com/argoproj/argo-cd` @ `f02203d0`):

- Kubernetes: **2 of 4477 annotations** duplicate by `spdxId` (0.04%). Whole document fails `spdx3-validate`.
- ArgoCD: **1 duplicate** — reproduces the same failure mode on a smaller polyglot target.
- Concrete example: `anno-GJJZ6XAC7UZOZO57` (containing the `mikebom:graph-completeness=partial` annotation) appears twice in `@graph` with identical content.

**This is a Constitution Principle IX (Accuracy) failure**: emitted SPDX 3 documents fail their own SHACL conformance gate. Downstream consumers running any SPDX 3 validator on mikebom output cannot use the document without either (a) accepting a schema-invalid document or (b) manually deduping before validation.

Small footprint (0.04% of annotations) but load-bearing: **the WHOLE document fails validation** because ANY duplicate `Annotation.statement` violates SPDX 3 cardinality. Milestone 166's fix restores clean SPDX 3 conformance across all emitted documents.

## Distinction from milestones 010, 011, 078, 079

- **Milestone 010** (SPDX Output Support): introduced SPDX 2.3 emission.
- **Milestone 011** (SPDX 3 output support): introduced SPDX 3 emission + the `build_annotation` helper at `v3_annotations.rs:156-186`.
- **Milestone 078** (SPDX 3 conformance): introduced `spdx3-validate==0.0.5` as the CI gate.
- **Milestone 079** (SPDX 3 identifier vocabulary): fixed identifier-vocab conformance.
- **Milestone 166** (this): fixes the DUPLICATE-Annotation-spdxId emission bug surfaced by milestone 165's audit on real upstream Go monorepos. Different failure class from prior SPDX 3 fixes — this is a merge-time dedup issue, not an emission-shape issue.

## User Scenarios & Testing

### User Story 1 — SPDX 3 conformance validator passes on real upstream targets (Priority: P1)

An SBOM consumer runs `spdx3-validate` (or any SHACL-based SPDX 3 conformance tool) on mikebom's emitted SPDX 3 document. Pre-166, the validator FAILS with `More than 1 values on <anno-*>->ns1:statement` on any scan that emits a duplicate Annotation-spdxId — most common on scans emitting document-scope annotations (like `mikebom:graph-completeness`) that coincidentally get emitted by both `build_document_annotations` and a downstream builder (or twice by the same builder). Post-166, the validator PASSES on every emitted SPDX 3 document.

**Why this priority**: Constitution Principle IX (Accuracy). Small footprint (0.04% of annotations) but load-bearing — a single duplicate breaks whole-document conformance. Consumers running validators can't use mikebom's SPDX 3 output today.

**Independent Test**: Regenerate the milestone-165 audit's Kubernetes + ArgoCD SPDX 3 SBOMs post-166. Run `spdx3-validate` on both. Both MUST return PASS. Plus every milestone-090 fixture's SPDX 3 golden re-validates clean (SC-006 conformance CI gate persists).

**Acceptance Scenarios**:

1. **Given** a mikebom scan against `github.com/kubernetes/kubernetes` HEAD (or a saved milestone-165 SBOM regenerated post-166), **When** `spdx3-validate --json <path>` runs, **Then** the exit code MUST be 0 (PASS) — no `More than 1 values on ns1:statement` error.

2. **Given** a mikebom scan against `github.com/argoproj/argo-cd`, **When** `spdx3-validate` runs, **Then** exit code 0 (PASS).

3. **Given** any milestone-090 fixture's SPDX 3 golden regenerated post-166, **When** the existing `spdx3-conformance` test (milestone 078) runs, **Then** every fixture MUST pass validation.

---

### User Story 2 — Emitted `@graph[]` has no duplicate `spdxId` values (Priority: P2)

A downstream tool consuming mikebom's SPDX 3 output MUST be able to trust that each `spdxId` appears at most once in `@graph[]`. This is the standard SPDX 3 uniqueness guarantee that any Element-processing consumer relies on.

**Why this priority**: Regression guard + universal correctness property. Consumers indexing `@graph[]` by `spdxId` (map-lookup pattern) currently overwrite entries silently when duplicates exist.

**Independent Test**: For every emitted SPDX 3 document (via existing golden fixtures + integration tests), verify: `[.["@graph"][].spdxId] | group_by(.) | map(select(length > 1)) | length == 0`. All existing goldens post-166 MUST satisfy this invariant.

**Acceptance Scenarios**:

1. **Given** every milestone-090 fixture's SPDX 3 golden post-166, **When** running the jq uniqueness check above, **Then** the result MUST be 0 (zero duplicate spdxIds) for every fixture.

2. **Given** a scan that intentionally exercises the previously-buggy code path (e.g., a document with both graph-completeness AND supplement-service annotations), **When** SPDX 3 is emitted, **Then** each `spdxId` in `@graph[]` appears exactly once even if the underlying content should be identical (deterministic which-copy-wins per FR-004).

---

### User Story 3 — SPDX 2.3 + CDX output unchanged (Priority: P3)

Users emitting SPDX 2.3 or CycloneDX see byte-identical output vs pre-166.

**Why this priority**: Regression guard. The dedup fix is scoped to SPDX 3 emission only. Mirrors milestone-158/159/160/161/162/163/164/165 dual-side byte-identity precedent.

**Independent Test**: Regenerate all milestone-090 goldens with milestone-166 code. Diff against pre-166. Zero diff bytes on CDX + SPDX 2.3 goldens across all 11 ecosystems.

**Acceptance Scenarios**:

1. **Given** the milestone-090 cargo fixture, **When** mikebom scans, **Then** emitted CDX MUST be byte-identical to pre-166 AND SPDX 2.3 MUST be byte-identical to pre-166.

### Edge Cases

- **Two builders emit annotation with SAME content but same-hash spdxId** (the observed bug): dedup drops one; content preserved. This is the primary bug case.

- **Two builders emit annotation with DIFFERENT content but same-hash spdxId** (hypothetical — would indicate a real content ambiguity): dedup must be DETERMINISTIC (per FR-004) about which copy wins. Preferably surface a warning so the emission code can be audited.

- **User `--annotator` + `--annotation-comment` pairs**: user-supplied Annotation elements use a DIFFERENT IRI scheme (`{doc_iri}/annotation/{slug}-{hash}`) that includes an index or slug — should not collide with `build_annotation`'s `{doc_iri}/anno-{hash}` scheme. Verify at implementation time.

- **All Annotation subjects other than the SpdxDocument**: component-scope annotations use `hash(component_iri | field)` — collisions require the SAME component AND SAME field to be emitted twice, which is a code-path bug worth surfacing. Same dedup posture applies.

- **`--metadata-comment` annotation**: uses IRI `{doc_iri}/annotation/metadata-comment-{hash}` where hash includes the full comment text. Very unlikely to collide with `anno-*` scheme.

- **Empty `@graph` case**: dedup on empty list is a no-op; no correctness concern.

- **Malformed `spdxId`** (missing / non-string): the existing sort at `v3_document.rs:814-817` already coerces via `.as_str().unwrap_or("")` — dedup MUST use the same coercion for consistency.

## Requirements

### Functional Requirements

- **FR-001**: mikebom's SPDX 3 emission code MUST deduplicate `Annotation` elements by `spdxId` before merging them into the final `@graph[]` array. When two builder call paths produce elements with the same `spdxId`, only ONE element (deterministic — see FR-004) appears in the emitted `@graph[]`.

- **FR-002**: The dedup MUST apply to the union of ALL annotation builders (`build_component_annotations`, `build_document_annotations`, `build_supplement_service_annotations`, user-supplied `--metadata-comment` + `--annotator` pairs) at `mikebom-cli/src/generate/spdx/v3_document.rs:754-820`.

- **FR-003**: The dedup MUST NOT change the emitted `Annotation` element's content (statement, subject, annotationType, creationInfo). Only whole-element duplicates are dropped; no field merging or transformation.

- **FR-004**: Dedup determinism — when duplicates exist, the LAST-inserted entry wins (or alternatively, the FIRST-inserted — the choice is deterministic and documented; both preserve byte-identity as long as builder order is deterministic). Implementation choice justified in plan.md.

- **FR-005**: On real upstream Go monorepos (specifically the milestone-165 Kubernetes + ArgoCD targets), `spdx3-validate==0.0.5` MUST return exit code 0 (PASS) on mikebom's emitted SPDX 3 document. No `More than 1 values on ns1:statement` errors.

- **FR-006**: Emitted `@graph[]` MUST satisfy `[.["@graph"][].spdxId] | group_by(.) | map(select(length > 1)) | length == 0` on every mikebom SPDX 3 document. This is a universal invariant regardless of the input scan shape.

- **FR-007**: A per-scan info-level tracing log MUST fire unconditionally on SPDX 3 emission naming the dedup drop count. Grep-friendly per the milestone-157/158/159/160/161/162/163/164/165 observability convention. Field: `spdx3_annotation_duplicates_dropped=<N>`. This surfaces potential code-path bugs — a healthy scan shows 0; non-zero counts warrant investigation via `RUST_LOG=trace` re-run. Per research §R4, ONLY the count is logged (no per-drop example spdxId) to keep the summary line concise.

- **FR-008**: mikebom-cli's existing `mikebom-cli/tests/spdx3_conformance.rs` test (milestone 078) MUST continue to pass — every milestone-090 fixture's SPDX 3 output validates clean via `spdx3-validate`. Post-166 this test is strengthened: any regression in the dedup pass fails the CI gate.

- **FR-009**: `mikebom-cli/tests/spdx3_annotation_fidelity.rs` (milestone 145) MUST continue to pass — annotation content preservation is not affected by dedup (only duplicate-DROP, not content-modify).

- **FR-010**: No new mikebom-emitted annotations, no new PURLs, no new parity-catalog rows, no new CLI flags. Reuses existing SPDX 3 emission infrastructure end-to-end. FR-007's log line is the ONLY new observable output.

### Key Entities

- **Annotation element** (SPDX 3.0.1 spec): a JSON object with fields `type: "Annotation"`, `spdxId`, `creationInfo`, `subject`, `annotationType`, `statement`. Cardinality constraint: `Annotation.statement` MUST be single-valued per (subject, spdxId) — hence the SHACL violation when duplicates exist.

- **`anno-*` spdxId scheme** (mikebom's derivation): `{doc_iri}/anno-{hash_prefix(subject_iri | field, 16)}`. Collision occurs when two builder call paths produce the SAME `(subject_iri, field)` tuple. Milestone 166 does NOT change this derivation — it deduplicates at merge time.

- **`@graph[]` array** (JSON-LD structure): the top-level container for all SPDX 3 elements in the emitted document. Milestone 166 ensures element uniqueness by `spdxId` within this array.

- **Dedup posture**: LAST-writer-wins OR FIRST-writer-wins — deterministic choice made in the plan phase. Both produce byte-identical output given deterministic builder ordering (which mikebom already has via milestone 017 dev-vs-CI byte-identity work + existing `sort_by_spdx_id` post-processing).

## Success Criteria

### Measurable Outcomes

- **SC-001 (K8s SPDX 3 conformance)**: Regenerated Kubernetes SPDX 3 SBOM (milestone-165 methodology, K8s commit `688614f2` or later HEAD) MUST pass `spdx3-validate --json`. Exit code 0.

- **SC-002 (ArgoCD SPDX 3 conformance)**: Same for ArgoCD (`f02203d0` or later HEAD).

- **SC-003 (milestone-090 fixture conformance preserved)**: Every milestone-090 fixture's SPDX 3 golden regenerated post-166 MUST pass `spdx3-validate`. `mikebom-cli/tests/spdx3_conformance.rs` MUST continue to pass in CI.

- **SC-004 (uniqueness invariant)**: For every emitted mikebom SPDX 3 document, `jq '[.["@graph"][].spdxId] | group_by(.) | map(select(length > 1)) | length'` MUST return 0. Verified via a new integration test.

- **SC-005 (dual-side byte-identity for CDX + SPDX 2.3)**: All milestone-090 fixtures' CDX + SPDX 2.3 goldens MUST be byte-identical to pre-166. Zero diff bytes on 11 ecosystems × 2 formats = 22 goldens.

- **SC-006 (SPDX 3 golden diff limited to dedup)**: Milestone-090 fixtures' SPDX 3 goldens MAY change post-166 IFF any fixture previously produced duplicate spdxIds. Diff MUST be limited to REMOVED duplicate Annotation entries. No new content, no reordering of remaining entries, no modification of other elements.

- **SC-007 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST both pass with zero errors.

- **SC-008 (unit test coverage)**: The new dedup code path MUST have at least 5 unit tests covering: (a) two builders emit same-spdxId same-content annotation → 1 element in output; (b) two builders emit same-spdxId DIFFERENT-content annotation → 1 element in output (LAST-writer-wins per FR-004; the drop is counted by the FR-007 log field but no separate WARN emission); (c) no duplicates → passthrough behavior unchanged; (d) empty `@graph[]` → no-op; (e) mix of duplicate + unique elements → uniques preserved, duplicates deduped.

- **SC-009 (integration test)**: A new integration test at `mikebom-cli/tests/spdx3_annotation_dedup.rs` MUST synthesize a scan that produces duplicate Annotation spdxIds (either by injecting supplement + graph-completeness at the same subject/field OR by using a debug harness that exercises the merge point) and assert: (a) `@graph[]` has no duplicate spdxIds; (b) FR-007 tracing log fires; (c) content of the retained Annotation matches the LAST-writer per FR-004.

- **SC-010 (CHANGELOG entry)**: `CHANGELOG.md` MUST document the fix + reference milestone 165 audit + include a jq recipe for consumers to verify dedup on their own SBOMs.

- **SC-011 (empirical closure)**: The impl commit MUST reference milestone 165 as the surface source (`implements milestone 166 — audit-surfaced fix from milestone 165`) and include a re-measured result: post-166 SPDX 3 validation on both Kubernetes + ArgoCD returns PASS.

## Assumptions

- **Live upstream is the test data**: SC-001 + SC-002 numbers are pinned to live clones of `github.com/kubernetes/kubernetes` and `github.com/argoproj/argo-cd` at the audit's execution time. Commit SHAs recorded per milestone-165 reproduction methodology. The bug is deterministic — same input at same SHA reproduces on both pre-166 and post-166 builds; only the OUTCOME changes.

- **spdx3-validate 0.0.5 remains the conformance gate**: per memory `reference_spdx3_validator`. Milestone 078 established this as CI's SPDX 3 validation authority.

- **No new Cargo dependencies**: following milestone-158/159/160/161/162/163/164/165 precedent. `HashMap<String, Value>` dedup via stdlib.

- **Milestone-090 goldens MAY drift on SPDX 3 side**: if any golden previously contained duplicate spdxIds, the golden regen removes those entries. Verified at authoring time via inspection. CDX + SPDX 2.3 goldens do NOT drift (FR-010 + SC-005 guarantee).

- **The FR-007 tracing log surfaces potential code bugs**: if the log fires with `spdx3_annotation_duplicates_dropped > 0` on ANY milestone-090 fixture, that's a signal that some annotation builder is emitting redundantly and should be investigated as a follow-on milestone. Milestone 166 fixes the SYMPTOM (dedup at merge); a future milestone would fix the ROOT CAUSE (redundant emitter) if the log surfaces significant volume.

- **Byte-identity for CDX + SPDX 2.3**: SPDX 3 is the ONLY output format touched. CDX + SPDX 2.3 goldens' byte-identity is enforced by SC-005 + existing golden tests.

## Out of Scope

- **Investigating the ROOT-CAUSE code paths** that emit the same (subject_iri, field) tuple — that's a future milestone if the FR-007 log surfaces significant volume. Milestone 166 fixes the SYMPTOM (dedup at merge point); the emitters that produce the duplicates continue to emit them (they're just silently deduped now).

- **SPDX 2.3 or CDX emission changes** — scope is SPDX 3 only per FR-010 + SC-005. If SPDX 2.3 has an analogous duplicate-spdxId problem (unlikely per the milestone-010 emission model), it's a separate future milestone.

- **spdx3-validate version bump** — pinned at 0.0.5 per memory. Any version bump is a separate infrastructure milestone.

- **New spdxId derivation scheme** — the existing `hash(subject_iri | field)` scheme is preserved. Fixing collisions by making spdxIds less collision-prone (e.g., adding a nonce, or including part of the statement content) is out of scope; the current scheme is byte-identity-safe per milestone 017 T013b and changing it would risk regression.

- **CI-gating on the FR-007 log threshold** — the log fires as observability; adding a CI test that FAILS when `spdx3_annotation_duplicates_dropped > N` is a policy decision for a future milestone.

- **Retroactive rewrite of pre-166 emitted SBOMs** — this is a scan-time fix; no consumer-side migration tooling is added.

- **Publishing an updated SBOM Consumer Guide entry** — the milestone-150/151 consumer guide may benefit from a note that "post-166 SPDX 3 outputs validate clean" but that's a docs milestone, not part of 166.
