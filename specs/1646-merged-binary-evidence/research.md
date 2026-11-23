# Research: spec 1646

Paths are relative to the workspace root. Code facts were read at `main` `e1148818`.

## R1. Where the technique is decided

`waybill-common/src/resolution.rs::ResolutionTechnique` has six variants. It is serialized `snake_case`, and none of them describes reading a binary.

Every `PackageDbEntry` becomes a `ResolvedComponent` with `technique: ResolutionTechnique::PackageDatabase` and `confidence: PACKAGE_DB_CONFIDENCE` (0.85). For database entries this happens at `waybill-cli/src/scan_fs/mod.rs` ~line 1248; `scan_fs/mod.rs` also sets `PackageDatabase` at ~3539 and ~3660. Three sites map the technique to a wire string, each an exhaustive `match`:
- `generate/cyclonedx/evidence.rs::build_evidence`: `PackageDatabase` becomes `manifest-analysis`;
- `generate/spdx/annotations.rs` and `generate/spdx/v3_annotations.rs` (D1): it becomes `package-database`.

**Decision**: add `ResolutionTechnique::BinaryAnalysis`.
- CycloneDX string: `binary-analysis`, CycloneDX 1.6's own term. SPDX D1 string: `binary-analysis`, in the same kebab style as its siblings.
- The conversion site sets it when the entry's `evidence_kind` is `embedded-version-string` or `symbol-fingerprint` (FR-003, clarification 2).
- Confidence stays `PACKAGE_DB_CONFIDENCE` (FR-004).
- The three `match` sites gain an arm; they are exhaustive, so the compiler lists them.
- Task T-conv greps every `ResolutionEvidence { technique: ResolutionTechnique::PackageDatabase` site that can receive a binary entry, so no conversion path is missed.

**Alternatives rejected**:
- *Choose the string at emission from `evidence_kind`*: the merged component no longer carries the loser's `evidence_kind`, which is the defect itself.
- *Reuse `FilePathPattern` (`filename`)*: wrong. The version comes from the bytes, not the file name.

## R2. Carrying a second method

`ResolutionEvidence` holds one `technique` and one `confidence`. It is built from struct literals at 70 sites in 51 files; `ResolvedComponent` at 165. No component field is internal-only. Every `extra_annotations` key is emitted, and the only filtered key is `waybill:source-files` (`root_selector::is_field_owned_annotation_key`).

**Decision**: add `pub also: Vec<IdentityMethod>` to `ResolutionEvidence`, where `IdentityMethod { technique, confidence }`, serialized `#[serde(default, skip_serializing_if = "Vec::is_empty")]`.
- It holds the techniques absorbed by merges that differ from the winner's own.
- The 70 literals gain `also: Vec::new()`. The edit is mechanical and the compiler enumerates every site.

**Alternatives rejected**:
- *An internal `extra_annotations` key filtered at emission*: there is no such convention today. It would have to be filtered in every emitter and the parity catalog, and a missed filter leaks a `waybill:*` property. Principle IV prefers the typed field.
- *A field on `ResolvedComponent`*: 165 literals, and the method belongs with the evidence.

## R3. Merge sites

In `waybill-cli/src/resolve/deduplicator.rs`:
- **S1**, the main merge (`deduplicate`, ~lines 139–370): groups by PURL, picks a winner by confidence with a stable sort (reader order breaks ties), and folds each loser into it. Binary matches merge into pkg-config/CMake records here, including spec 1645's rewritten pairs and spec 1644's anchored libraries.
- **S2**, `fold_declared_not_cached` (~line 516), folds a declared-but-not-cached entry into its on-disk twin(s). That twin can be a binary match.

**Decision**: one helper, `absorb_method(best: &mut ResolutionEvidence, other: &ResolutionEvidence)`, called at S1 and S2. It adds `other`'s technique, and its `also`, to `best.also` when all of these hold:
- the set of techniques on the two sides includes `BinaryAnalysis` (FR-002 and FR-008: merges without a binary match are untouched);
- the technique differs from `best.technique`;
- the technique is not already in `best.also`, so there is one method per technique.

The census (R6) logs every call, and each site's name, so FR-006's merge sites are observed rather than assumed.

## R4. Emission

- **CycloneDX** (`build_evidence`): `methods` is the primary method, then each `also` method in order. CycloneDX 1.6 allows several methods in `methods[]`. The test suite's schema validator (`tests/common/cdx_schema.rs`) checks the shape.
- **SPDX 2.3 and SPDX 3 (D1)**: the flat `technique` and `confidence` keys are unchanged, holding the primary method. When `also` is non-empty, a `methods` array is added with the primary method first, then `also` (clarification 1). D1 stays presence-only in the parity catalog (`parity/extractors/mod.rs` "D1"), so no extractor changes.
- **Order**: the primary method first, then `also` in a fixed order of the `ResolutionTechnique` variants. That order is deterministic whatever the readers' order.

## R5. What does not change

- C4 (`evidence_kind`), C16 (`confidence`), C55 and C56 (FR-005).
- The deduplicator's winner choice, tier, PURL, version and edges (FR-008).

## R6. Census (FR-001)

DEBUG events on target `waybill::version_census`, emitted only when that target is enabled, all on the log-only commit:

- **`technique_change`**: at the conversion site, one event per component whose `evidence_kind` is `embedded-version-string` or `symbol-fingerprint`, with its PURL, kind and the technique it would get.
- **`binary_derived_kind`**: one event per component carrying another binary-derived evidence:
  - `dynamic-linkage`, `embedded-source-path`, `cargo-auditable`, `elf-note-package`;
  - Go build info (`detected_go == Some(true)`).

  These are counted for the follow-up issue (clarification 2). Nothing changes for them.
- **`binary_merge`**: at S1 and S2, one event per absorption in which either side's evidence kind is a binary match. It records the site, the PURL, and each side's mechanism (C55), evidence kind, technique and confidence, plus which side won.

The fixture repository's `census/versions.py` parses all three event kinds, and its `control` job adds a positive-control tree: a banner match plus `.pc`, and a fingerprint-only library.

## R7. Fixtures

Built from `tests/common/installed_lib.rs`:
- banner trees reuse spec 1645's shapes: an ncurses banner with `ncursesw.pc`, and an `LLVM version …git` banner with `LLVMConfigVersion.cmake`;
- the fingerprint tree uses `elf_shared_lib_with_dynsym` defining the bundled zlib fingerprint's exported symbols, taken from `scan_fs/binary/symbol_fingerprint.rs` at implement time, not copied by hand;
- names are `waybill-fixture-*` except where the library's own banner or name is under test.

## R8. Mutations (SC-004)

| ID | Mutation | Must be killed by |
|---|---|---|
| M1 | `absorb_method` not called at S1 | the banner+`.pc` and banner+CMake cases |
| M2 | the banner technique stays `PackageDatabase` | the banner-alone case |
| M3 | the fingerprint technique stays `PackageDatabase` | the fingerprint-alone case |
| M4 | methods not deduplicated by technique | the fingerprint+banner+`.pc` case |
| M5 | SPDX `methods` array emitted for single-method components | the byte-identity case |
| M6 | the binary-only condition removed (any differing techniques absorbed) | a record-into-record case (expat's `.pc` plus CMake) |

## R9. Timing

The changes are an O(1) technique choice per entry and an O(methods) union per merge. SC-007 is measured by `measure-full`; no prediction is made here.
