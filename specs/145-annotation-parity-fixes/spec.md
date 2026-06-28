# Feature Specification: Annotation-emission parity fixes from sbom-conformance audit (2026-06-26)

**Feature Branch**: `145-annotation-parity-fixes`
**Created**: 2026-06-27
**Status**: Draft
**Input**: User description: "Three annotation-emission parity fixes surfaced by sbom-conformance harness 2026-06-26 audit: (1) mikebom:file-paths emits a stringified array instead of native array (3112 findings); (2) mikebom:lifecycle-scope missing from SPDX 3 emitter (261 findings); (3) mikebom:source-files JAR vs rootfs-tempdir value drift on Maven deps in image scans (51 findings)."

## Origin

External sbom-conformance audit harness run on 2026-06-26 (post-milestone-144) flagged 3,424 cross-format-invariance (CFI) findings against mikebom. The milestone-144 PR (envelope-value coercion fix in `94e8434` / golden refresh in `7b5ed82`) cleared 23 findings (the `mikebom:not-linked` cluster + `mikebom:detected-cargo-auditable` value divergences). The 2026-06-26 follow-up run, with the harness's `<component-presence>` detector patched, identified three remaining mikebom-side issues totaling ~3,424 findings:

- **A — `mikebom:file-paths` double-encoded** (3,112 findings). The annotation's value is emitted as `Value::String("[\"path1\",\"path2\"]")` (a stringified array literal) instead of `Value::Array([String("path1"), String("path2")])`. Every other array-valued mikebom annotation (`mikebom:source-files`, `mikebom:cpe-candidates`, etc.) uses the native-array shape; `file-paths` is the lone outlier. Affects every file-tier component on every image-fixture scan — confirmed in `mikebom-cli/src/scan_fs/file_tier/mod.rs:232-234` (the constructor wraps `serde_json::to_string(&paths_str)` back into a `Value::String` via `json!()`).
- **B — `mikebom:lifecycle-scope` missing from SPDX 3** (261 findings, all on npm dev deps in `node-dev-vs-prod` fixture). Pattern `Y | Y | -` (CDX + SPDX 2.3 emit; SPDX 3 omits). Confirmed via grep: the SPDX 3 emitter files (`v3_packages.rs`, `v3_document.rs`) contain zero references to `lifecycle_scope` / `LifecycleScope`; the SPDX 2.3 emitter writes the annotation at `annotations.rs:236`.
- **C — `mikebom:source-files` value drift on Maven deps** (51 findings, all on `polyglot-builder-image` Maven deps). Same component carries different `source_file_paths` strings in CDX vs SPDX 3 output — CDX shows the JAR path (`root/.m2/repository/.../surefire-booter-3.2.2.jar`), SPDX 3 shows the image-extraction tempdir (`private/var/folders/.../mikebom-image-AylYTd/rootfs`). Both emitters consume `c.evidence.source_file_paths` from the same `ResolvedComponent`, so something is mutating or re-stamping the field between CDX emission and SPDX 3 emission.

All three are PURE EMISSION-LAYER bugs — none affect dependency discovery (Constitution Principle II), component-presence, PURL conformance (Principle V's purl-spec clause), or any other binding correctness invariant. They are SBOM-quality regressions that erode downstream-tooling interoperability (SPDX 3 consumers see a different SBOM shape than CDX consumers expect; PURL-keyed dedup of file-tier components fails because the value isn't a queryable array).

## US2 reverted post-investigation (2026-06-28)

**Resolution**: US2 was implemented + then REVERTED during the `/speckit-implement` phase after the `spdx3_annotation_fidelity::fidelity_maven` test caught a Constitution Principle V violation. The fidelity test (per issue #228) encodes the existing design contract: SPDX 3 already carries lifecycle scope natively via `LifecycleScopedRelationship.scope` (set in `v3_relationships.rs` for `Dev`/`Build`/`TestDependsOn` edges); adding `mikebom:lifecycle-scope` annotations on Package elements would be REDUNDANT with the native field, which Principle V forbids.

Local manual verification on the `maven` fixture confirmed SPDX 3 emits `LifecycleScopedRelationship` with `scope: "test"` for the `junit` dep — the native mechanism is working as designed.

**Conclusion**: the 261 audit findings under `mikebom:lifecycle-scope Y | Y | -` are **false positives** from the sbom-conformance harness misreading the SPDX 3 scope mechanism. The harness should be updated to honor `LifecycleScopedRelationship.scope` as the SPDX 3 equivalent of CDX's `mikebom:lifecycle-scope` property and SPDX 2.3's `mikebom:lifecycle-scope` annotation. No mikebom-side change is appropriate.

FR-005 / FR-006 / FR-007 / SC-004 / SC-005 / SC-006 in this spec are INVALIDATED by this resolution and remain only as historical record. US2 should be considered **closed without code change**; US1 + US3 continue as planned. Expected post-145 finding reduction drops from ~3,424 to ~3,163 (3,112 file-paths + 51 source-files).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - `mikebom:file-paths` is emitted as a native JSON array (Priority: P1)

A downstream consumer (sbom-conformance harness, vulnerability matcher, file-tier dedup tool) reads the `mikebom:file-paths` annotation from an emitted SBOM and expects the value to be a JSON array of path strings, matching every other array-valued mikebom annotation. Today the value is a JSON string containing the serialized form of an array — forcing every consumer to do a second `JSON.parse(value)` to extract the paths, AND breaking JSON-Path / jq queries that expect array semantics (e.g., `jq '.[] | select(.value | length > 0)'` returns wrong results because `.value` is a string, not an array). After this milestone, the value is a native JSON array; consumers can iterate it directly.

**Why this priority**: This is the dominant audit signal (3,112 of the 3,424 remaining findings — 91% of the cleanup ROI). It is also the smallest, lowest-risk fix: one line in `file_tier/mod.rs:233` plus one unit-test wording change. The downstream impact is silent SBOM-shape divergence today, fixed instantly on the next scan.

**Independent Test**: Scan any directory containing files that trigger file-tier emission (or use the existing file-tier test fixtures), emit CDX or SPDX, then `jq '.components[] | select(.properties[]? | .name == "mikebom:file-paths") | .properties[] | select(.name == "mikebom:file-paths") | .value | type'` MUST return `"array"` for every component (not `"string"`).

**Acceptance Scenarios**:

1. **Given** a scan that produces at least one file-tier component, **When** the operator inspects the emitted CDX JSON, **Then** the `mikebom:file-paths` property value on that component is a JSON array, not a JSON string.
2. **Given** the same scan output in SPDX 2.3 or SPDX 3 form, **When** the operator parses the `mikebom:file-paths` annotation envelope (the value inside the `MikebomAnnotationCommentV1` wrapper), **Then** that inner `value` field is a JSON array.
3. **Given** an existing file-tier test that asserts on the post-parse shape, **When** the test runs after the fix, **Then** it succeeds against the native-array shape (the test wording also updated as part of the milestone).
4. **Given** the existing parity catalog row C92 (`mikebom:file-paths`, `Directionality::SymmetricEqual`), **When** the cross-format parity check runs, **Then** all three formats emit byte-equivalent array values.

---

### User Story 2 - `mikebom:lifecycle-scope` is emitted in SPDX 3 output (Priority: P1)

An operator scanning an npm project with both dev and prod dependencies emits SBOMs in all three formats and queries `mikebom:lifecycle-scope` to identify dev-only packages. Today CDX and SPDX 2.3 both carry the annotation on every dev-marked component; SPDX 3 omits it entirely. Downstream tooling that keys off `mikebom:lifecycle-scope` for compliance gates (e.g., "production SBOMs must not include `lifecycle-scope = development` components") fails silently when fed an SPDX 3 document — the annotation isn't there, so the gate treats all components as prod-tier. After this milestone, the SPDX 3 emitter writes `mikebom:lifecycle-scope` with the same shape and timing as the SPDX 2.3 emitter.

**Why this priority**: 261 findings is the second-largest single cluster after `file-paths`, and the absence has real compliance impact (dev/build/test scope detection is the principal motivation for milestone-049 / milestone-052's lifecycle-scope work). The fix is additive — adding an emission path that already exists in the SPDX 2.3 sibling.

**Independent Test**: Emit SPDX 3 output for the `node-dev-vs-prod` fixture (or any equivalent npm-dev-marked component fixture). `jq '.["@graph"][] | select(.["mikebom:lifecycle-scope"]?) | .["mikebom:lifecycle-scope"]'` MUST return at least one match. Compare to the SPDX 2.3 output of the same scan: every component carrying `mikebom:lifecycle-scope` in SPDX 2.3 MUST also carry it in SPDX 3 with the same value.

**Acceptance Scenarios**:

1. **Given** a scan of an npm project containing dev-only dependencies, **When** the SPDX 3 output is emitted, **Then** each dev-only component has a `mikebom:lifecycle-scope` annotation with value `"development"`.
2. **Given** the same scan in SPDX 2.3, **When** the SPDX 2.3 output is compared component-by-component against the SPDX 3 output, **Then** every component carrying `mikebom:lifecycle-scope` in SPDX 2.3 also carries it in SPDX 3 with the same value (no extras, no omissions).
3. **Given** the parity catalog row C42 (`mikebom:lifecycle-scope`, `Directionality::SymmetricEqual`), **When** the parity check runs on the fixture, **Then** zero `Y | Y | -` findings remain.
4. **Given** the operator runs the same scan against a maven / pip / cargo / gradle project carrying `LifecycleScope::Build` / `Test` / `Runtime` values, **When** SPDX 3 is emitted, **Then** the annotation is present on those components too (parity with SPDX 2.3's behavior).

---

### User Story 3 - `mikebom:source-files` carries consistent values across CDX and SPDX 3 for Maven deps (Priority: P2)

A consumer comparing `mikebom:source-files` between the CDX and SPDX 3 outputs of the same scan expects identical values — both formats consume the same `ResolvedComponent.evidence.source_file_paths` field, after all. Today on `polyglot-builder-image` scans of Maven deps, CDX carries the JAR file path (`root/.m2/repository/.../surefire-booter-3.2.2.jar`) while SPDX 3 carries the absolute image-extraction tempdir (`private/var/folders/.../mikebom-image-AylYTd/rootfs`). 51 components affected. After this milestone, both formats carry the same value for the same component on the same scan — the canonical value being the JAR path relative to the rootfs (matching CDX today).

**Why this priority**: 51 findings is smaller than US1/US2, AND the issue requires a diagnosis phase before a fix can land (the per-emitter mutation site is not in the emitter code itself — both emitters consume `c.evidence.source_file_paths` from the same field, so something is re-stamping it). The investigation surface includes: where the Maven reader sets source-file-paths; whether there's a path-normalization pass running between CDX and SPDX 3 emission; whether the image-extract tempdir leaks via a fallback. Doing the diagnosis correctly is more valuable than rushing a one-line patch that masks the wrong root cause.

**Independent Test**: Scan the `polyglot-builder-image` test fixture (or any image-extraction scan with Maven deps), emit both CDX and SPDX 3, and `diff <(jq '.components[] | select(.purl | startswith("pkg:maven/")) | {purl, src: [.properties[]? | select(.name == "mikebom:source-files") | .value]}' cdx.json) <(jq '.["@graph"][] | select(.software_packageUrl? | startswith("pkg:maven/")) | {purl: .software_packageUrl, src: .["mikebom:source-files"]}' spdx3.json)`. The diff MUST be empty.

**Acceptance Scenarios**:

1. **Given** a scan of `polyglot-builder-image` that produces at least one Maven dep component with `mikebom:source-files` populated, **When** CDX and SPDX 3 outputs are compared component-by-component, **Then** the `mikebom:source-files` value matches exactly (same array of strings, same order).
2. **Given** the value is the JAR file path relative to the scan rootfs (e.g., `root/.m2/repository/.../surefire-booter-3.2.2.jar`), **When** the SPDX 3 output is emitted, **Then** it carries that same value — NOT the image-extraction tempdir path (`private/var/folders/.../mikebom-image-AylYTd/rootfs`).
3. **Given** the diagnosis phase identifies the per-emitter mutation site, **When** the fix lands, **Then** a unit test reproduces the original divergence (e.g., constructs a synthetic `ResolvedComponent` with a known source-file path, runs both emitters, asserts both produce the same output value).
4. **Given** the parity catalog row C18 (`mikebom:source-files`, `Directionality::SymmetricEqual`), **When** the parity check runs on a `polyglot-builder-image`-shaped fixture, **Then** zero `Y | - | Y` and zero value-drift findings remain.

---

### Edge Cases

- **Empty file-paths list** (US1) — `paths.is_empty() == true`: today the conditional at `file_tier/mod.rs:232` guards on `Ok(file_paths_json)` from `serde_json::to_string(&paths_str)`, which always succeeds for an empty Vec. The fix must still emit the empty-array `[]` (NOT a `Value::Null`, NOT a string `"[]"`), matching the existing test at line 405 that parses the field.
- **Path with embedded quote characters** (US1) — a file path containing `"` must be properly quoted inside the JSON array shape. Native-array emission delegates this to `serde_json`'s `Value::Array` serializer; the stringified-array shape today also delegates but at two levels. Both must produce identical wire bytes after round-trip; verify the wire encoding via byte-identity goldens.
- **`LifecycleScope::Runtime`** (US2) — the SPDX 2.3 emitter at `annotations.rs:233` deliberately returns `None` for Runtime (it's the default; no annotation needed). The SPDX 3 fix MUST preserve this same omission — emit the annotation ONLY for `Development` / `Build` / `Test`, NOT for `Runtime`.
- **`include_source_files = false`** (US3) — both emitters already gate on this flag (`annotations.rs:302` + `v3_annotations.rs:267`). The fix MUST preserve the gate; the value-drift only matters when the annotation is being emitted at all.
- **Components with empty `source_file_paths`** (US3) — both emitters skip emission today; this must remain unchanged.
- **Maven dep ALSO appearing as a file-tier component** (cross-US interaction) — file-tier and Maven readers can both observe the same JAR file. If milestone-133 dedup leaves both representations, US1's `file-paths` fix and US3's `source-files` alignment apply to different annotations on different (or the same, post-dedup) components. No conflict.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The file-tier reader (`mikebom-cli/src/scan_fs/file_tier/mod.rs`) MUST emit the `mikebom:file-paths` annotation value as a native JSON array of strings (e.g., `["usr/sbin/losetup", "usr/sbin/mount"]`), NOT as a JSON-string-encoding of that array (e.g., `"[\"usr/sbin/losetup\"]"`).
- **FR-002**: The fix MUST preserve the existing sort-stable ordering, the `FILE_PATHS_CAP` truncation behavior, and the `mikebom:file-paths-truncated = "true"` sidecar annotation (these are orthogonal correctness invariants; only the value-shape changes).
- **FR-003**: The existing file-tier unit test at `file_tier/mod.rs:398-405` (which parses the value as a string via `serde_json::from_str`) MUST be updated to parse the value as a native array via `serde_json::from_value(value.clone())` or direct iteration.
- **FR-004**: The fix MUST be observable in CDX 1.6, SPDX 2.3, and SPDX 3 outputs simultaneously — all three downstream emitters consume the same `extra_annotations` BTreeMap, so a single fix at the constructor site satisfies cross-format invariance.
- **FR-005**: The SPDX 3 emitter (`mikebom-cli/src/generate/spdx/v3_annotations.rs` or a sibling file in `mikebom-cli/src/generate/spdx/`) MUST emit the `mikebom:lifecycle-scope` annotation for every component whose `ResolvedComponent.lifecycle_scope` is `Some(LifecycleScope::Development | Build | Test)`, with the same value strings (`"development"`, `"build"`, `"test"`) the SPDX 2.3 emitter uses today at `annotations.rs:227-236`.
- **FR-006**: The SPDX 3 emitter MUST omit the annotation when `lifecycle_scope == Some(LifecycleScope::Runtime)` (matching SPDX 2.3's existing behavior — `Runtime` is the default, no annotation needed).
- **FR-007**: The SPDX 3 emitter MUST omit the annotation when `lifecycle_scope == None` (matching SPDX 2.3's existing behavior).
- **FR-008**: For US3, the milestone MUST include a diagnostic phase that identifies the specific code site responsible for the per-emitter `mikebom:source-files` value drift on Maven deps in image-extraction scans. The diagnostic finding MUST be documented in the milestone's research artifact before any code fix lands.
- **FR-009**: After US3's diagnostic phase identifies the mutation site, the fix MUST make `mikebom:source-files` carry the same value in CDX and SPDX 3 outputs of the same scan, for every Maven dep component on the `polyglot-builder-image` (and equivalent image-extraction fixtures). The canonical value SHOULD be the JAR file path relative to the rootfs (matching CDX today), UNLESS the diagnostic phase surfaces a compelling reason to canonicalize on the absolute path instead — in which case the spec is updated and CDX is brought into alignment with SPDX 3 rather than the other way around.
- **FR-010**: All existing byte-identity SBOM golden tests that capture the pre-fix shape of `mikebom:file-paths` (stringified-array) and the pre-fix absence of `mikebom:lifecycle-scope` in SPDX 3 MUST be refreshed in the same PR. The refresh diffs MUST be limited to the wire-shape changes named above; no unrelated golden drift.
- **FR-011**: All changes MUST preserve Constitution Principle V (standards-native > `mikebom:*`) — no new `mikebom:*` annotations are introduced; the milestone only fixes how three EXISTING annotations are emitted.

### Key Entities

- **`mikebom:file-paths` annotation** — Per-component annotation set by the file-tier reader (`file_tier/mod.rs`). Value is the sorted list of file paths that hash to the same SHA-256 (the file-tier component identity). Pre-145: emitted as `Value::String("[\"path1\",\"path2\"]")`. Post-145: emitted as `Value::Array([String("path1"), String("path2")])`. Sidecar `mikebom:file-paths-truncated = "true"` (unchanged) appears when the list was capped at `FILE_PATHS_CAP`.
- **`mikebom:lifecycle-scope` annotation** — Per-component annotation set by per-ecosystem readers (npm, maven, pip, cargo, gradle, etc.) for components marked as dev / build / test / runtime scope. Pre-145: emitted in CDX + SPDX 2.3, omitted in SPDX 3. Post-145: emitted in all three formats with consistent value strings (`"development"`, `"build"`, `"test"`; omitted for `"runtime"` per existing convention).
- **`mikebom:source-files` annotation** — Per-component annotation set from `ResolvedComponent.evidence.source_file_paths` listing the manifest/file paths that contributed to the component's identity. Pre-145: per-emitter value drift on Maven deps in image-extraction scans. Post-145: identical values across CDX and SPDX 3 for the same component on the same scan.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001** (US1): After this milestone, the sbom-conformance harness reports **zero `mikebom:file-paths` value-shape findings** in its CFI bucket (down from 3,112). Verifiable by running the harness against an emitted SBOM and grepping for the file-paths cluster.
- **SC-002** (US1): A unit test in `file_tier/mod.rs#mod tests` (or sibling) constructs a synthetic file-tier component with at least one path, runs it through `into_resolved_component()`, and asserts that the `extra_annotations.get("mikebom:file-paths").unwrap()` is a `serde_json::Value::Array` (NOT `Value::String`).
- **SC-003** (US1): Byte-identity golden tests for the file-tier emission cases (CDX + SPDX 2.3 + SPDX 3 — likely the existing golden tests under `mikebom-cli/tests/fixtures/golden/` that exercise image-scan fixtures with file-tier components) are refreshed; the diffs show ONLY the `mikebom:file-paths` value-shape change (single grep-able pattern: `"value": "[..."` → `"value": [...`).
- **SC-004** (US2): After this milestone, the sbom-conformance harness reports **zero `Y | Y | -` findings for `mikebom:lifecycle-scope`** (down from 261). Verifiable on the `node-dev-vs-prod` fixture or equivalent.
- **SC-005** (US2): A unit test in `mikebom-cli/src/generate/spdx/v3_annotations.rs#mod tests` (creating the test module if it doesn't exist) constructs a synthetic `ResolvedComponent` with `lifecycle_scope = Some(LifecycleScope::Development)`, runs the SPDX 3 annotation builder, and asserts the emitted JSON-LD contains `mikebom:lifecycle-scope` with value `"development"`. A second test asserts the annotation is OMITTED when `lifecycle_scope = Some(LifecycleScope::Runtime)`.
- **SC-006** (US2): Byte-identity golden tests that include components with non-Runtime `lifecycle_scope` (the SPDX 3 emission goldens specifically) are refreshed; CDX and SPDX 2.3 goldens are unchanged.
- **SC-007** (US3): The milestone's research artifact (`research.md`) MUST include a §titled "C — `mikebom:source-files` per-emitter drift diagnosis" naming the specific function and approximate line number where the per-emitter value mutation occurs, with a reproduction case.
- **SC-008** (US3): After this milestone, the sbom-conformance harness reports **zero value-drift findings for `mikebom:source-files`** on `polyglot-builder-image` (down from 51).
- **SC-009** (US3): A unit or integration test constructs a synthetic scan that triggers the per-emitter drift, runs both CDX and SPDX 3 emitters, and asserts byte-equivalent `mikebom:source-files` values.
- **SC-010**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` both pass clean — i.e., `./scripts/pre-pr.sh` exits 0 (excepting the pre-existing local sbomqs_parity env-only failure documented in milestone 144's T001).
- **SC-011**: Across the three user stories, cumulative cross-format-invariance findings drop by ≥3,424 (the combined 3,112 + 261 + 51 from the 2026-06-26 audit).
- **SC-012**: No regression in CFI findings outside the three target clusters. Verifiable via a before/after harness run on the same fixture set.

## Assumptions

- **The 2026-06-26 audit numbers (3,112 + 261 + 51) are stable across re-runs of the same harness against the same fixture set.** Confirmed for `file-paths` and `lifecycle-scope` (the underlying defects are deterministic emission-shape issues). The `source-files` 51 count was reported as a +43 delta from the previous run — possible measurement variance, but the underlying defect (per-emitter value drift) is real and code-confirmed.
- **The file-paths value-shape change is a wire-output break for any downstream tool that parses the value as a JSON-string-encoded array.** Such tools would need to update to parse it as a native array. We accept this break because (a) the stringified shape was never documented as the contract; (b) every other array-valued mikebom annotation already uses the native shape; (c) the affected file-tier components are a milestone-133-era addition (~6 months) — no long-standing consumer can have grown around the wrong shape.
- **The SPDX 3 lifecycle-scope addition is purely additive — no SPDX 3 consumer can break by GAINING an annotation that wasn't there before.** Adding the field cannot regress any document-level conformance test (it's a `mikebom:*` annotation, not a standard SPDX 3 field; addition is in the structured-properties bag).
- **Both emitters consume the same `ResolvedComponent.evidence.source_file_paths`.** Code-confirmed (`annotations.rs:306` + `v3_annotations.rs:271`). Therefore the per-emitter drift MUST come from a re-stamping site between scan-orchestrator output and SPDX 3 emission. The diagnosis phase (FR-008) will identify whether it's a path-normalization pass, a component clone-and-mutate, or an image-extract tempdir leak — all three are plausible.
- **The canonical value choice for `mikebom:source-files` is the rootfs-relative JAR path.** This matches CDX's current behavior and aligns with how every other ecosystem reader (apk, dpkg, npm, etc.) records source paths. If the diagnostic phase surfaces a compelling reason to canonicalize on the absolute path, the spec MUST be updated to reflect that; this assumption is conditionally accepted pending diagnosis.
- **No new Cargo dependencies needed.** All three fixes are touch-existing-emitter changes; no new crates introduced.
- **The sbom-conformance harness is the audit oracle.** This spec's SC-001 / SC-004 / SC-008 are framed in terms of harness-reported finding counts; verification requires re-running the harness post-fix. Local unit-tests (SC-002 / SC-005 / SC-009) provide code-level guards in the CI signal independent of harness availability.
- **Constitution Principle V (standards-native > `mikebom:*`) is preserved.** All three annotations affected (file-paths, lifecycle-scope, source-files) are EXISTING `mikebom:*` properties — already audited at their introducing milestones (133 for file-paths, 049/052 for lifecycle-scope, 005-era for source-files). This milestone fixes their emission, not their existence.

## Out of Scope

- The image-pseudo-component absent-from-spdx-json gap (2 cases on `go-vcs-buildinfo`, the audit's item #3). Smaller impact, distinct code area (image-component emission), deferred.
- The `mikebom:sbom-tier` per-component disagreement on Maven shaded/transitive deps (4-5 cases on `polyglot-builder-image`, the audit's item #4 including the `plexus-archiver` +1 delta). Distinct from US3's drift (sbom-tier is a STRING value disagreement, not a path-stamping drift). Deferred.
- New `mikebom:*` annotations.
- Restructuring of `extra_annotations`'s `BTreeMap<String, Value>` representation (the value-shape mismatch in US1 is fixable at the constructor; changing the type is unnecessary).
- The 3,112-finding `<component-presence>` cluster from the previous audit run — collapsed to 2 in this run after the harness's own detector patch. Not a mikebom issue.
- Performance optimization of file-tier emission (the milestone touches one constructor; no perf-budget change).
- Any change to how `ResolvedComponent.evidence.source_file_paths` is POPULATED by the Maven reader — US3's diagnosis may surface a population-time issue, in which case the FIX may need to land in the Maven reader; this is scope-bounded by the diagnostic phase.
