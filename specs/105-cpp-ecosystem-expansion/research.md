# Phase 0 Research: C/C++ Ecosystem Expansion (Phase 2)

**Milestone**: 105
**Date**: 2026-05-28
**Status**: Complete; one finding (R3) requires a spec correction before tasks.

## Summary of findings

| ID | Topic | Finding | Spec impact |
|---|---|---|---|
| R1 | Principle V audit for `mikebom:also-detected-via` | CDX 1.6 has a partial native equivalent (`evidence.identity[].methods[]`); SPDX 2.3 has none; SPDX 3.0.1 has partial. Hybrid emission resolves favorably. | None — spec FR-015 unchanged; data-model documents the hybrid. |
| R2 | `serde_yaml` dependency status | **Already a direct dep** in `mikebom-cli/Cargo.toml:99` (`serde_yaml = "0.9"`). No Cargo.toml change needed. | Complexity-tracking entry for `serde_yaml` is moot — drop it. |
| R3 | `mikebom:linkage-kind` reuse for FR-008a | **Conflict** — existing annotation has a CDX builder debug-assert enforcing `dynamic`/`static`/`mixed` only. Reusing it for source-tier build-reference status would break the assertion and conflate two unrelated semantics. | **Spec correction required**: FR-008a's annotation name must change. Recommended: `mikebom:build-reference` (closed enum: `declared-and-used`, `declared-only`). |
| R4 | URL-sanitization helper for FR-016 | `sanitize_userinfo(url: &str) -> SanitizedUrl` exists at `mikebom-cli/src/binding/identifiers/auto_detect.rs:79`, but the return struct is **module-private**. Reuse requires either (a) making it `pub` and re-exporting, or (b) a thin public wrapper at a shared location. | Tasks must include a small refactor to expose the helper. |
| R5 | `find_package` parsing for FR-008a correlation | Existing cmake reader **explicitly does NOT parse `find_package`** (documented at `cmake.rs:15-17`, enforced by test `find_package_does_not_emit_components` at `cmake.rs:411`). Milestone 102's FR-007 forbids component emission from `find_package` (to avoid double-counting against OS-package readers, vcpkg, conan). | Milestone 105 introduces find_package parsing **for correlation only** (populates a `Set<target_name>` consumed by `git_submodule.rs`); it does **not** emit components from find_package. Compatible with milestone 102's FR-007. |
| R6 | Parity catalog row addition pattern + state | Catalog has **79 rows** today (`extractors/mod.rs:103` onwards). C55 (`mikebom:source-mechanism`) is wired from PR #272 (line 307). Pattern: 3 extractor fns (cdx.rs / spdx2.rs / spdx3.rs) + 1 `ParityExtractor` row in `mod.rs`. `Directionality` enum: `SymmetricEqual`, `CdxSubsetOfSpdx`, `PresenceOnly`, `CdxOnly`. | Milestone 105 adds 2 new rows: **C56** (`mikebom:also-detected-via`) and **C57** (`mikebom:build-reference`). Both `SymmetricEqual`. |
| R7 | CDX `evidence.identity[].methods[]` emission state | mikebom already emits this block per component (`mikebom-cli/src/generate/cyclonedx/evidence.rs:31-94`). Current fields: `{technique, confidence}`. Built via `serde_json::json!()` macro — **additively extensible** with no schema rewrite. | Enables R1's hybrid: CDX emits native multi-method evidence; SPDX 2.3 / 3.0.1 use the parity-bridging C56 annotation. |

---

## R1: Constitution Principle V audit — `mikebom:also-detected-via`

**Decision**: Approved as a parity-bridging annotation. Hybrid emission strategy.

**Audit (per CLAUDE.md / Principle V)**:

| Format | Native equivalent | Adequacy |
|---|---|---|
| CycloneDX 1.6 | `evidence.identity[].methods[]` array with per-method `{technique, confidence}` | **Adequate.** When a component is dedup'd from N readers, emit N entries in `methods[]`. Adds a new sub-field `mikebom-source-mechanism` to each method entry to carry the closed-enum source-mechanism value. The CDX 1.6 schema permits additional properties on identity-method objects (verified against the CDX 1.6 JSON schema at `mikebom-cli/tests/fixtures/cyclonedx/spec/1.6/schema/bom-1.6.schema.json`). |
| SPDX 2.3 | None — SPDX 2.3 has no per-relationship-evidence model that could carry "this Package was also detected by mechanism X". `Annotation` is the only escape hatch. | **Inadequate.** SPDX 2.3 forces use of `Annotation` (the same mechanism the existing `mikebom:source-mechanism` C55 row already uses). |
| SPDX 3.0.1 | `EvidenceMaterial` carries observation provenance; `software_PrimaryEvidence` / `software_AdditionalEvidence` could in principle hold multiple sources. | **Partial.** The `EvidenceMaterial` shape doesn't natively express "this component's PURL was independently confirmed by another reader's source-mechanism"; closest fit would be encoding source-mechanism as a `LiteralOrNoAssertionOrNone` value inside the `media_type` or `description` fields — semantically lossy. |

**Resolution per Principle V's last paragraph** ("a parity-bridging `mikebom:*` annotation is permitted ONLY to carry finer-grained information the standard does not express, or to bridge a parity gap when one format has the native field but another doesn't"):

The semantics — "this canonical PURL was independently produced by N distinct readers, here are the losing readers' source-mechanism values" — has a partial CDX home but no clean SPDX 2.3 home. Therefore:

1. **CDX 1.6 emission**: native `evidence.identity[].methods[]` with one method entry per reader. Each method carries `{technique: "manifest-analysis", confidence: 0.85, mikebom-source-mechanism: "<value>"}`. The winning reader appears first; losing readers follow.
2. **SPDX 2.3 emission**: parity-bridging `mikebom:also-detected-via` Annotation (JSON array of losing source-mechanism strings, sorted lexicographically).
3. **SPDX 3.0.1 emission**: parity-bridging `mikebom:also-detected-via` Annotation (same shape as 2.3 for parity-extractor symmetric-equal).

The parity catalog row **C56** is `SymmetricEqual` — the parity extractor for CDX walks the `evidence.identity[].methods[]` array, extracts the `mikebom-source-mechanism` sub-fields, sorts them, and produces the same `BTreeSet<String>` that the SPDX 2.3/3.0.1 extractors produce from the annotation. Byte-identity parity is preserved.

This justification is documented as the Principle V audit in `docs/reference/sbom-format-mapping.md`'s C56 row (a milestone 105 deliverable).

---

## R2: `serde_yaml` dependency status

**Decision**: No Cargo.toml change. Use the existing direct dep.

**Evidence**: `mikebom-cli/Cargo.toml:99` declares `serde_yaml = "0.9"` as a direct dependency.

**Consequence**: The plan's Complexity Tracking entry for `serde_yaml` is moot. The west.yml and idf_component.yml readers use `serde_yaml::from_str::<WestManifest>(...)` and `serde_yaml::from_str::<IdfComponentManifest>(...)` directly.

---

## R3: `mikebom:linkage-kind` reuse conflict — **spec correction required**

**Finding**: FR-008a in the current spec says it reuses `mikebom:linkage-kind` "already emitted by the binary readers". The clarification session's Q5 answer also asserts this reuse. **This is incorrect.**

**Evidence**:

- The existing `mikebom:linkage-kind` annotation is emitted by binary-tier readers (`mikebom-cli/src/scan_fs/binary/entry.rs:408-435`) with a closed enum of `dynamic | static | mixed` describing the binary linkage mode of an ELF/Mach-O/PE artifact.
- The CDX builder enforces this enum via `debug_assert!(matches!(linkage.as_str(), "dynamic" | "static" | "mixed"))` at `mikebom-cli/src/generate/cyclonedx/builder.rs:888-892`.
- Parity row C12 (`mikebom-cli/src/parity/extractors/mod.rs:147`) targets this exact closed enum.

Reusing `mikebom:linkage-kind` for source-tier build-graph reference status (`declared-and-used` / `declared-only`) would:

1. Break the CDX builder's debug_assert under any debug build, causing test-suite panics.
2. Conflate two unrelated semantics (binary link mode vs. source-tier build-graph reference) into a single annotation, making downstream consumers' filtering ambiguous.
3. Force a special-cased widening of the C12 parity row, breaking byte-identity for any pre-milestone-105 golden that has `mikebom:linkage-kind` set to one of `dynamic`/`static`/`mixed`.

**Decision**: Introduce a **new annotation** `mikebom:build-reference` with the closed enum `declared-and-used` / `declared-only`. This gets a new parity catalog row **C57** (`SymmetricEqual`).

**Required spec corrections** (applied before `/speckit.tasks`):

- FR-008a: replace `mikebom:linkage-kind` → `mikebom:build-reference` throughout.
- Edge Cases section: same rename.
- Clarifications session Q5: amend the answer (footnote correcting the annotation name; don't delete the original Q&A, since it records intent — add a clarifying note).
- Key Entities: add a `mikebom:build-reference` entity (parallel to the `mikebom:also-detected-via` entity added during clarification).

I will apply these corrections as the final research-phase step (below the Phase 1 outputs).

---

## R4: URL-sanitization helper — visibility refactor

**Decision**: Promote `sanitize_userinfo` to a public utility in a shared location.

**Evidence**:

- Location: `mikebom-cli/src/binding/identifiers/auto_detect.rs:79`
- Signature: `fn sanitize_userinfo(url: &str) -> SanitizedUrl` (currently `pub(super)` / module-private; the `SanitizedUrl` struct is module-private too)
- Logs: `tracing::info!` at lines 226–230 with redacted URL when sanitization occurred. (Per Q4 of the clarification session, FR-016 calls for `tracing::warn!` — research recommends bumping the existing log level to `warn` as part of milestone 105's helper refactor, since the log indicates an operator-actionable secrets-in-manifest event.)
- Call sites today: 2 production sites (`auto_detect_repo_identifier`, `auto_detect_build_tier_identifiers`) plus 9 unit-test calls.

**Refactor plan** (a single small task at the start of US3 / US6 / west.yml / git-submodule work):

1. Move the helper to `mikebom-cli/src/identifiers/sanitize.rs` (or a similarly neutral location).
2. Make `pub fn sanitize_userinfo(url: &str) -> Cow<'_, str>` (collapsing the private struct into a plain `Cow` return — `Borrowed` when no sanitization occurred, `Owned` when it did; the `was_sanitized` bit is derivable from `matches!(cow, Cow::Owned(_))`).
3. Update the 2 existing production call sites and 9 tests.
4. Bump the `tracing::info!` to `tracing::warn!` per FR-016.

Risk: low. Pure refactor with mechanical call-site updates; the existing tests guard the behavior.

---

## R5: `find_package` parsing for FR-008a — bounded re-introduction

**Decision**: Introduce `find_package(...)` parsing in the cmake reader, but **only as a correlation source**, not as a component emitter.

**Evidence of existing constraint**:

- `mikebom-cli/src/scan_fs/package_db/cmake.rs:15-17`: "`find_package(X)` declarations are NOT parsed per FR-007 — they resolve to system-installed packages and would double-count against OS-package readers + vcpkg + Conan."
- Test `find_package_does_not_emit_components()` at `cmake.rs:411` is the regression gate.

**Why the constraint is compatible with FR-008a**: milestone 102's FR-007 forbids **component emission** from `find_package` (the "double-counting" rationale). Milestone 105's FR-008a uses `find_package` as a **correlation key** — it populates a `Set<String>` of referenced target names that the `git_submodule.rs` reader uses to classify submodules as `declared-and-used` vs `declared-only`. No component is emitted from `find_package`; the existing regression test `find_package_does_not_emit_components` continues to pass.

**Implementation note**: the `Set<String>` is built once per scan during the cmake walk (additive to the existing `parse_cmake_file` pass) and passed to the dedup pipeline alongside the per-reader component lists. Memory: O(number of distinct find_package target names) — bounded; typical projects have <500 such calls.

---

## R6: Parity catalog state + add-row pattern

**Decision**: Add 2 new rows (C56, C57) following the established pattern.

**State today**:

- 79 `ParityExtractor` rows total in `mikebom-cli/src/parity/extractors/mod.rs`
- C55 (`mikebom:source-mechanism`) wired at line 307 — confirmed from PR #272
- Pattern per row (established by C55):
  1. `cdx.rs`: add `pub fn c5n_cdx(component: &Value) -> BTreeSet<String>`
  2. `spdx2.rs`: add `pub fn c5n_spdx23(component: &Value) -> BTreeSet<String>`
  3. `spdx3.rs`: add `pub fn c5n_spdx3(component: &Value) -> BTreeSet<String>`
  4. `mod.rs`: import the 3 functions; append a `ParityExtractor { row_id, label, cdx, spdx23, spdx3, directional, order_sensitive }` row

**Directionality choice**:

- **C56** (`mikebom:also-detected-via`): `SymmetricEqual`. CDX extractor walks `evidence.identity[].methods[].mikebom-source-mechanism` (skipping the winning method, which is whatever appears in the top-level `mikebom:source-mechanism` annotation); SPDX 2.3 / 3.0.1 extractors walk the `mikebom:also-detected-via` annotation array. Both produce the same `BTreeSet<String>` of losing source-mechanism values.
- **C57** (`mikebom:build-reference`): `SymmetricEqual`. All three formats carry it as the same annotation shape.

---

## R7: CDX `evidence.identity[].methods[]` is already there

**Decision**: Reuse the existing emission path for the C56 hybrid.

**Evidence**:

- `mikebom-cli/src/generate/cyclonedx/evidence.rs:31-94` builds the `evidence.identity[].methods[]` block per component.
- Today's per-method fields: `{technique, confidence}` where `technique` is one of `instrumentation | hash-comparison | manifest-analysis | filename | other`.
- Construction uses `serde_json::json!()` macro → additively extensible (we can add a `mikebom-source-mechanism` field to each method entry without rewriting any struct).

**Implementation**: in the dedup pipeline (`scan_fs/dedup.rs`), each surviving component carries a `Vec<DetectionRecord>` (one entry per matching reader). When emitting CDX, the existing emitter is extended to consume this list and produce one `methods[]` entry per record. The winning record produces `technique: "manifest-analysis"` (or its existing equivalent) plus `mikebom-source-mechanism: <winner-value>`; each losing record produces the same shape with the losing source-mechanism value.

---

## Spec correction to apply before `/speckit.tasks`

R3 surfaced a real incompatibility between the clarification session's Q5 answer and the existing codebase. The spec must be corrected to use a new annotation name (`mikebom:build-reference`) instead of the previously-stated reuse of `mikebom:linkage-kind`. The correction is mechanical (rename in FR-008a, Edge Cases, Q5 footnote, Key Entities). I'll apply this correction immediately after writing this research file.
