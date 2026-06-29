# Feature Specification: source-files cross-emitter divergence — union evidence across same-PURL entries

**Feature Branch**: `148-source-files-union`
**Created**: 2026-06-28
**Status**: Draft
**Input**: User description: "milestone 148: source-files cross-emitter divergence — union evidence.source_file_paths across same-PURL entries after dedup to fix the Maven nested-coord audit finding (51 polyglot-builder-image cases)"

## Origin & Context

The sbom-conformance audit harness (operator-cadence; not a CI gate) compared mikebom-emitted CDX 1.6 / SPDX 2.3 / SPDX 3 documents for the `polyglot-builder-image` fixture on 2026-06-28 (post-merge of milestone 145's annotation-emission parity fixes in `8a0660b`). The harness reported that 51 components carry a `mikebom:source-files` value that differs across CDX and SPDX 3 outputs for what the harness treats as the SAME component PURL (matched by `pkg:maven/<groupId>:<artifactId>@<version>` identity).

Investigation in the conversation thread leading to this milestone established:

1. **Where it lives**: the Maven nested-JAR walker at `mikebom-cli/src/scan_fs/package_db/maven.rs:3429-3457` intentionally creates **two `PackageDbEntry` instances** for the same Maven coord when that coord appears both standalone (e.g., at `/root/.m2/repository/.../surefire-3.2.2.jar`) AND nested inside a fat-jar (e.g., extracted from `/tmp/extract-XXX/some-fat.jar!surefire-3.2.2.jar`). The two entries differ only in their `parent_purl` field — one is `None` (standalone), the other is `Some(fat-jar-purl)` (nested).

2. **Why dedup doesn't merge them**: the deduplicator at `mikebom-cli/src/resolve/deduplicator.rs:34-46` groups components by `(ecosystem, name, version, parent_purl)`. Different `parent_purl` values → different groups → never merged. The CDX nested-components model (a child can appear under multiple parents) DEPENDS on this — collapsing by PURL alone would destroy the dep-graph topology.

3. **Why the audit harness sees a divergence**: both entries survive into the per-format emit pipeline. Each entry carries its own `evidence.source_file_paths` Vec (one points at the standalone JAR path; the other at the extract-tempdir path). Each format emitter (CDX `builder.rs:619` iteration, SPDX 2.3 `annotations.rs:302`, SPDX 3 `v3_annotations.rs:267`) emits BOTH entries as separate components, each with its own `mikebom:source-files` value. The user's harness then groups by PURL on the audit side and observes "same PURL, different source-files values across formats" — the values it picks per format depend on whichever entry happens to be the first or last match in its post-processing pipeline, which differs by format because the wire surface presents the entries in different orders.

4. **Why milestone 145's US3 fix didn't move these 51 findings**: 145 US3 addressed a DIFFERENT case — the same component's `mikebom:source-files` annotation was being double-emitted (once from `evidence.source_file_paths` field, once from `extra_annotations["mikebom:source-files"]` bag stamped by the Maven reader). The fix renamed the bag-stamped key to `mikebom:source-files-nested-url` (eliminating the within-component drift). The 51 polyglot-builder-image cases are a DIFFERENT shape: cross-`PackageDbEntry`-instance drift on the field-derived source itself.

The remediation should preserve the topology-bearing two-entry shape while making the `evidence.source_file_paths` Vec carry the SAME content on every entry that shares a PURL. Once every same-PURL entry's Vec contains the union of all observed paths, the emitter-iteration-order-dependent harness divergence dissolves: regardless of which entry the harness picks per format, the `mikebom:source-files` value is identical.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Same-PURL `mikebom:source-files` values match across CDX and SPDX 3 emissions (Priority: P1)

A compliance engineer runs `mikebom sbom scan` on `polyglot-builder-image` and emits both CDX 1.6 and SPDX 3 outputs. They run a cross-format audit query that, for every Maven coord PURL appearing in both documents, compares the `mikebom:source-files` annotation values. Today (post-145) the query reports 51 PURLs with cross-format divergence. After this milestone, the count drops to 0 (or near-zero) — every PURL's `mikebom:source-files` value contains the SAME union of observed paths regardless of whether the auditor reads the CDX or SPDX 3 view.

**Why this priority**: this is the singular value-add of this milestone — closing the last cluster of cross-format `mikebom:source-files` divergence on the polyglot-builder-image audit corpus. Without it, downstream compliance tooling that does cross-format reconciliation continues to report 51 false-divergence findings, masking real cross-format issues.

**Independent Test**: scan the polyglot-builder-image fixture in both CDX and SPDX 3 formats; for every shared Maven PURL, assert the `mikebom:source-files` value parsed as a sorted set is bytewise-identical across the two formats. SC-001 captures the expected count delta (51 → 0 or near-zero on the audit corpus).

**Acceptance Scenarios**:

1. **Given** the polyglot-builder-image fixture is scanned and produces both CDX 1.6 and SPDX 3 outputs, **When** the auditor extracts `mikebom:source-files` values from every Maven coord PURL that appears in both documents, **Then** every same-PURL pair carries the SAME set of file paths (as parsed alphabetically-sorted sets).

2. **Given** the same fixture is scanned and produces both CDX 1.6 and SPDX 2.3 outputs, **When** the auditor performs the equivalent cross-format comparison, **Then** every same-PURL pair carries the SAME set of file paths. (SPDX 2.3 is structurally identical to SPDX 3 for this annotation; the parity must hold across all three formats.)

3. **Given** a Maven coord that exists ONLY as a top-level entry (no fat-jar nesting), **When** the scan emits all three formats, **Then** the `mikebom:source-files` value continues to contain the single rootfs-relative path it carried pre-148 — the union-with-self degenerates to the identity case.

4. **Given** a Maven coord that exists ONLY as a nested entry under a fat-jar (no standalone counterpart in the rootfs), **When** the scan emits all three formats, **Then** the `mikebom:source-files` value carries the single extract-tempdir path it carried pre-148 — the union-with-self again degenerates to the identity case.

5. **Given** a Maven coord that exists in BOTH shapes (standalone + nested under a fat-jar), **When** the scan emits all three formats, **Then** EVERY surviving entry's `mikebom:source-files` value contains the FULL union of both paths (standalone AND nested), sorted alphabetically.

---

### User Story 2 - Cross-ecosystem coverage (non-Maven same-PURL multi-entry cases) (Priority: P2)

The Maven nested-coord pattern is the most prominent same-PURL multi-entry case in the polyglot-builder-image audit corpus, but the deduplicator's `(ecosystem, name, version, parent_purl)` group-key admits the same shape for ANY ecosystem where a reader sets `parent_purl` on some entries and leaves it `None` on others for what the wire surface treats as the same component PURL. After this milestone, cross-format `mikebom:source-files` divergence MUST be zero for any such ecosystem (npm peer-dep edge cases, Go vendored module cases, Rust workspace cargo-vendored cases — wherever the same coord can survive both as top-level AND under a parent_purl).

**Why this priority**: the fix lands in the cross-ecosystem post-dedup pipeline (not in the Maven reader specifically), so coverage of non-Maven ecosystems comes for free. Documenting it as a separate user story ensures the success criteria call out the cross-ecosystem invariant explicitly so future ecosystem readers can rely on it.

**Independent Test**: enumerate every (ecosystem, PURL) combination in the polyglot-builder-image fixture; for each ecosystem that has ≥1 PURL with >1 surviving `ResolvedComponent` instance post-dedup, assert the same cross-format `mikebom:source-files` invariant from US1 scenario 1 holds.

**Acceptance Scenarios**:

1. **Given** a fixture contains a Cargo crate that exists both as a top-level workspace member AND as a vendored sub-crate under a workspace parent, **When** the scan emits all three formats, **Then** the crate's `mikebom:source-files` value carries the union of both paths across all three formats.

2. **Given** a fixture contains a Go module that exists both as a `go.mod`-tracked direct dep AND as a vendored copy under `vendor/` (with `parent_purl` set on the vendored entry), **When** the scan emits all three formats, **Then** the module's `mikebom:source-files` value carries the union of both observed paths.

3. **Given** a fixture has no ecosystem with same-PURL multi-entry shape, **When** the scan emits all three formats, **Then** no component's `mikebom:source-files` value changes from its pre-148 emission — the union-with-self degenerates to identity for every component.

---

### Edge Cases

- **Same PURL, three or more surviving entries**: e.g., one standalone + two nested under different fat-jars. The union pass MUST merge across all three (and N more for general cases), producing a single alphabetically-sorted union Vec.

- **Same PURL, same `source_file_paths` content on every entry**: union-with-self produces an unchanged Vec (the dedup-by-content set semantic guarantees no duplicate paths appear).

- **Same PURL where one entry has empty `source_file_paths`**: the union still works — empty Vec contributes no elements, the merged Vec equals the non-empty entry's content.

- **PURL that survives dedup as a single entry**: union-with-self is identity; the Vec content is unchanged byte-for-byte from pre-148.

- **File-tier components** (PURL = `pkg:generic/file-tier?content-sha256=<hex>`): out of scope for this milestone. File-tier components already carry the FULL multi-path union via the existing `push_path` aggregation in `file_tier/walker.rs`; the union pass is an idempotent no-op for them (the dedup-by-content `BTreeSet` collapses the input back to the same set).

- **Components where `evidence.source_file_paths` is empty on every same-PURL entry**: union produces empty Vec; the existing `if !c.evidence.source_file_paths.is_empty()` emission gates at each emitter site keep the annotation absent.

- **PURL collision across ECOSYSTEMS** (theoretical — e.g., a `pkg:pypi/<name>@<v>` and a `pkg:maven/<name>@<v>` happening to share the bare-PURL string after some normalization step): MUST NOT cross-pollinate paths. The union pass MUST key on the full canonical PURL string (`Purl::as_str()`), which includes the ecosystem segment — preserving ecosystem isolation.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: After the existing `deduplicate()` pass at `scan_fs/mod.rs:750`, mikebom MUST run a post-dedup canonicalization pass that, for every PURL appearing on multiple `ResolvedComponent` instances, replaces each instance's `evidence.source_file_paths` Vec with the **union** of paths observed across all same-PURL entries.

- **FR-002**: The union MUST be a set (no duplicate paths) collected in alphabetical (lex-ascending) order. This matches the milestone-145 `mikebom:file-paths` precedent (`paths_str.sort()` at `file_tier/mod.rs:230`).

- **FR-003**: The union pass MUST key on the canonical PURL string (`component.purl.as_str()`), NOT on `(name, version)` or `(ecosystem, name, version, parent_purl)`. The canonical PURL string includes the ecosystem segment, preventing cross-ecosystem path cross-pollination per Edge Case 7.

- **FR-004**: The union pass MUST be IDEMPOTENT. Running it twice on the same input produces byte-identical output. Specifically: every entry that already carries the canonical union Vec is unchanged after the second pass.

- **FR-005**: The union pass MUST NOT alter ANY field of `ResolvedComponent` other than `evidence.source_file_paths`. Specifically: `purl`, `name`, `version`, `parent_purl`, `hashes`, `lifecycle_scope`, `extra_annotations`, the rest of `evidence.*` (technique, confidence, source_connection_ids, deps_dev_match), and every other field MUST be preserved verbatim.

- **FR-006**: The fix MUST preserve the dep-graph topology that the deduplicator INTENTIONALLY retains via the `parent_purl` group-key. Specifically: the two entries for `pkg:maven/<groupId>:<artifactId>@<version>` (one with `parent_purl = None`, one with `parent_purl = Some(fat-jar-purl)`) MUST both continue to exist post-148; only their `evidence.source_file_paths` Vecs are touched.

- **FR-007**: When a component's PURL appears as the only entry post-dedup (the common case — single-entry PURLs are the overwhelming majority of every scan), the union pass MUST be a **content-preserving no-op** for that entry: the *set* of paths MUST be unchanged. The wire-order MAY be canonicalized to alphabetical (lex-ascending) — the union pass uses a `BTreeSet<String>` collection, so an insertion-ordered single-entry Vec that wasn't already alphabetically sorted pre-148 will be sorted post-148. This is non-breaking because `mikebom:source-files` value-order has no documented semantic and consumers parse the value as a set. (The implementer MAY tighten the no-op to *byte-identical* by skipping write-back when the BTreeSet content equals the existing Vec content — purely an optimization; the spec requirement is content-preservation only.)

- **FR-008**: The fix MUST NOT introduce any new `mikebom:*` annotation key. The existing `mikebom:source-files` (field-derived) and `mikebom:source-files-nested-url` (Maven-reader-stamped) keys cover the wire surface. The fix operates entirely on the in-process `evidence.source_file_paths` field; no wire-level addition.

- **FR-009**: Cross-format byte-equivalence MUST hold for the `mikebom:source-files` annotation across all three formats (CDX 1.6, SPDX 2.3, SPDX 3) on every test fixture that exercises the same-PURL multi-entry case. The parity-catalog row C18 (`mikebom:source-files`, `Directionality::SymmetricEqual`) MUST continue to pass on all existing byte-identity goldens AND on any new fixture that exercises the post-148 union pass.

- **FR-010**: Existing byte-identity goldens under `mikebom-cli/tests/fixtures/golden/` (CDX + SPDX 2.3 + SPDX 3) MUST refresh through the standard env-var trifecta (`MIKEBOM_UPDATE_CDX_GOLDENS=1`, `MIKEBOM_UPDATE_SPDX_GOLDENS=1`, `MIKEBOM_UPDATE_SPDX3_GOLDENS=1`). Any golden drift MUST be limited to `mikebom:source-files` value changes on components that previously carried a non-canonical single-path Vec (specifically: the Maven nested-coord components in the maven-bearing fixtures). NO unrelated field MUST drift.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the polyglot-builder-image audit corpus (per the 2026-06-28 harness run), the count of Maven PURLs reporting cross-format `mikebom:source-files` divergence drops from **51 → 0** (or near-zero — single-digit residuals are acceptable only if they come from a different bug class, documented post-merge by the operator-cadence harness re-run).

- **SC-002**: The C18 parity-catalog row's `Directionality::SymmetricEqual` invariant continues to hold on every byte-identity golden under `mikebom-cli/tests/fixtures/golden/`. The CI-binding `cross_format_byte_identity` and `holistic_parity` tests MUST pass.

- **SC-003**: A new in-source integration test (placed in `mikebom-cli/tests/source_files_purl_union_md148.rs` or similar) MUST exercise a fixture with ≥1 known same-PURL multi-entry shape (Maven nested-coord) and assert the cross-format invariant from US1 acceptance scenario 1 — same PURL → same `mikebom:source-files` value across all three formats. The test MUST fail on the pre-148 codebase and pass post-148.

- **SC-004**: A new unit test in the deduplicator module (`mikebom-cli/src/resolve/deduplicator.rs#mod tests`) MUST construct two `ResolvedComponent` instances sharing a PURL but differing in `parent_purl`, with different `evidence.source_file_paths` content, and assert that after the union pass both entries carry the same alphabetically-sorted union Vec. The test MUST cover FR-001 + FR-002 + FR-005 + FR-006.

- **SC-005**: An idempotence unit test MUST run the union pass twice on the same input and assert byte-equality of the output (FR-004).

- **SC-006**: The full pre-PR gate (`./scripts/pre-pr.sh`) MUST pass — both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` — except the documented pre-existing `sbomqs_parity` env-only failure per memory `feedback_prepr_gate_full_output.md` + milestone-144 T001 note.

- **SC-007**: Operator-cadence verification (post-merge, not CI-gated): the operator re-runs the sbom-conformance audit harness on the polyglot-builder-image fixture and confirms the 51 findings are gone. Documented in the PR description as a manual follow-up.

## Assumptions

1. **The Maven nested-coord pattern is the dominant same-PURL multi-entry shape on the polyglot-builder-image audit corpus.** Per the diagnostic in the conversation thread, all 51 audit findings cluster on Maven coords. Other ecosystems may also exhibit the pattern; the fix's cross-ecosystem coverage (US2) addresses them generically, but the SC-001 51 → 0 metric is Maven-specific.

2. **The deduplicator's `(ecosystem, name, version, parent_purl)` group-key is correct and must NOT be changed.** Collapsing the group-key to `(ecosystem, name, version)` would destroy the CDX nested-components topology that Maven shade-plugin fat-jar handling depends on (per the doc-comment at `maven.rs:3450`). The fix MUST be additive: a post-dedup union pass that leaves the dedup logic untouched.

3. **`source_file_paths` is the only `evidence.*` field that needs cross-entry union.** Other fields like `source_connection_ids` and `hashes` are MERGED by the deduplicator's existing logic (lines 69-83) WITHIN each `(ecosystem, name, version, parent_purl)` group. They don't need cross-`parent_purl` union because their semantics are per-instance (a hash measured at this specific path; a connection-id from this specific reader).

4. **The CDX `bom-ref` uniqueness concern is out of scope.** When two same-PURL entries with `parent_purl = None` both reach the CDX builder, they both get bom-ref = plain PURL string, which violates CDX 1.6's bom-ref uniqueness spec. This is a SEPARATE issue (not introduced by this milestone) and the harness in the audit thread doesn't flag it. Future milestone N+M can address it; this milestone scope is the `mikebom:source-files` value-drift fix only.

5. **No new Cargo dependencies.** The fix uses `std::collections::HashMap` + `std::collections::BTreeSet` (both pervasive in the codebase). No new crate additions.

6. **No new `mikebom:*` annotation.** Per FR-008. The fix is purely an in-process evidence-merge operation; the wire surface is unchanged at the annotation key level (values change for affected components).

7. **Operator-cadence audit re-run is sufficient SC-007 verification.** Per the existing project convention (milestones 144 / 145 / 147 all rely on operator-cadence re-runs of the harness as the post-merge confirmation; the in-tree integration test SC-003 is the CI-binding signal).

8. **The Maven reader stays unchanged.** Per FR-006 — the Maven reader's two-entry intent is preserved. The fix lands in `scan_fs/mod.rs` (or a sibling module under `mikebom-cli/src/resolve/`), NOT in `mikebom-cli/src/scan_fs/package_db/maven.rs`.

## Out of Scope

1. **CDX `bom-ref` uniqueness enforcement.** Two same-PURL components both ending up with the same bom-ref is a CDX 1.6 spec violation but is NOT introduced by this milestone — it's pre-existing. Out of scope; a separate issue can track the fix.

2. **Per-emitter component-instance dedup.** Some emitters might benefit from collapsing two same-PURL components into one wire entry (CDX nested-components vs SPDX 2.3 flat Packages vs SPDX 3 `software:Package` elements). This milestone does NOT change per-emitter dedup; it only canonicalizes the `evidence.source_file_paths` Vec across same-PURL entries so the emitted values agree.

3. **New `mikebom:source-files-*` annotations.** Per FR-008 — no new annotation. The fix is the value canonicalization; the wire surface key set is unchanged.

4. **Changes to milestone 145's US3 fix.** That fix (renaming the Maven bag-stamped key to `mikebom:source-files-nested-url`) stays as-is. This milestone is additive — different code path, different bug class.

5. **Changes to the deduplicator's group-key.** Per Assumption 2 — the deduplicator stays untouched. This milestone is a post-dedup pass.

6. **Changes to `evidence.source_connection_ids`, `evidence.hashes`, or any other `evidence.*` field.** Per Assumption 3 — only `source_file_paths` needs the cross-entry union.

7. **Investigation of issue #1 (JVM symlink paths) or issue #2 (SPDX 3 lifecycle-scope).** Per the conversation triage:
   - Issue #1 needs an operator-cadence raw-output diagnostic before scoping a fix; the hypothesis (harness shows only first array element) is plausible but unconfirmed.
   - Issue #2 is a deliberate Principle V decision (SPDX 3's native `LifecycleScopedRelationship.scope` is the carrier); not a mikebom bug. Future harness updates should look at the relationship-side, not the package annotation.

8. **File-tier components.** Per Edge Case 5, the file-tier walker already aggregates per-hash. The union pass is idempotent for them; no change in behavior.

9. **A new parity-catalog row.** The C18 row already covers `mikebom:source-files` with `Directionality::SymmetricEqual`. SC-002 reuses the existing row.

## Constitution V audit

This milestone introduces NO new `mikebom:*` annotation (FR-008). The fix is purely a value-canonicalization operation on the in-process `evidence.source_file_paths` field that ALL three emitters already consume via their existing `c.evidence.source_file_paths` reads. The C18 parity-catalog row's audit (recorded at `docs/reference/sbom-format-mapping.md` C18) is unchanged.

Constitution Principle V (standards-native > `mikebom:*`) is satisfied vacuously: no annotation is added; the existing `mikebom:source-files` annotation's parity-bridging rationale is unaffected.
