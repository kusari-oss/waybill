# Wire contract: spec 1646

## Document changes

1. **CycloneDX** `components[].evidence.identity[0].methods[]`:
   - a component whose identity came from its binary's bytes has `technique: "binary-analysis"`;
   - a merged component that absorbed a binary match lists the winner's method and then `{"technique": "binary-analysis", "confidence": 0.85}`.
2. **SPDX 2.3 and SPDX 3**, Annotation `evidence.identity` (D1):
   - the flat `technique` and `confidence` keys hold the primary method, `binary-analysis` for a binary match;
   - when there are two or more methods, a `methods` array is added with the primary method first.

No new annotation, property, catalog row or field. C4, C16, C55 and C56 are unchanged.

## Census events (DEBUG, target `waybill::version_census`, log-only commit)

Each value is a JSON object with sorted keys.

- **`technique_change`**:
  ```json
  {"evidence_kind": "embedded-version-string", "new": "binary-analysis", "old": "package-database", "purl": "pkg:generic/bzip2@1.0.8"}
  ```
- **`binary_derived_kind`**:
  ```json
  {"evidence_kind": "dynamic-linkage", "purl": "…", "technique": "package-database"}
  ```
- **`binary_merge`**:
  ```json
  {"loser": {"confidence": 0.85, "evidence_kind": "embedded-version-string", "mechanism": null, "technique": "package-database"},
   "purl": "pkg:generic/gmp@6.3.0", "site": "S1",
   "winner": {"confidence": 0.85, "evidence_kind": null, "mechanism": "pkg-config", "technique": "package-database"}}
  ```

## Cases (`waybill-cli/tests/merged_binary_evidence.rs`)

| Case | Tree | Expected |
|---|---|---|
| B-1 banner + `.pc` | ncurses banner in `lib/libncursesw.so.6`, plus `ncursesw.pc` | one `ncurses@6.6.20251230`; CycloneDX methods `manifest-analysis` then `binary-analysis`; SPDX 2.3 and SPDX 3 D1 with `methods` holding `package-database` then `binary-analysis` |
| B-2 banner + CMake | `LLVM version 23.0.0git` in `lib/libLLVM.so`, plus `LLVMConfigVersion.cmake` | one `llvm@23.0.0git` with the same two methods |
| B-3 banner alone | an expat banner in `libwaybill-fixture-expat.so.1` | `expat@2.7.5`; one method, `binary-analysis`; SPDX D1 flat `binary-analysis`, no `methods`; C4 `embedded-version-string` and C16 `heuristic` unchanged |
| F-1 fingerprint alone | `libwaybill-fixture-z.so.1` defining the bundled zlib fingerprint's symbols | `zlib`; one method, `binary-analysis` |
| F-2 fingerprint + banner + `.pc` | a zlib banner and zlib symbols in `lib/libz.so.1`, plus `zlib.pc` | one `zlib@<v>`; exactly one `binary-analysis` method beside `manifest-analysis`; `fingerprint-symbols-matched` kept |
| R-1 record into record | `expat.pc` and `expat-config-version.cmake`, with no banner library | one `expat@<v>`; one method, `manifest-analysis`; SPDX D1 without `methods`, byte-identical to `main`; C56 as on `main` |
| P-1 parity | B-1, B-2, F-2 | `waybill sbom parity-check` shows no new gap |
| S-1 schema | B-1, F-2 | the CycloneDX document validates against the 1.6 schema (`tests/common/cdx_schema.rs`) |
| C-1 census | B-1 with `waybill::version_census=debug` | one `binary_merge` (site S1, winner mechanism `pkg-config`, loser kind `embedded-version-string`) and one `technique_change` |
