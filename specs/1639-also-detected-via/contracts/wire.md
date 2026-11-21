# Wire contract: C56 and C242

## C56 `waybill:also-detected-via`

**CycloneDX 1.6.** A component `properties[]` entry, on `components[]` and on `metadata.component` alike:

```json
{"name": "waybill:also-detected-via", "value": "[\"cmake-fetchcontent-git\"]"}
```

The value is the JSON array serialised as the generic property path serialises every array-valued annotation (the same path C233 uses). There is no `waybill-source-mechanism` key anywhere in `evidence.identity[].methods[]`. The document validates against the vendored 1.6 schema.

**SPDX 2.3.** A Package annotation in the `MikebomAnnotationCommentV1` envelope, `field: "waybill:also-detected-via"`, with the same array.

**SPDX 3.0.1.** An Annotation element whose subject is the package, carrying the same envelope.

**Parity.** `SymmetricEqual`, read with `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` (research R8).

**Catalog row** (`docs/reference/sbom-format-mapping.md`, C56) is rewritten to say:
- what C56 records: the distinct C55 values of members merged into this component that differ from its own C55, sorted and unique, present only when the component carries C55;
- the merge sites (research R3);
- the CDX carrier as a property, with the audit of research R1 in the justification column, replacing the milestone 105 hybrid text;
- that the symbol-fingerprint collision is C242, not C56.

## C242 `waybill:fingerprint-also-matched` (new row)

The names of other libraries whose symbol fingerprints matched the same binary (milestone 108 FR-013), as a sorted, unique JSON array of strings.

- **CDX:** a component property.
- **SPDX 2.3 and 3:** an annotation, by the same three macros.
- **Parity:** `SymmetricEqual`.
- **Principle V audit:** no native carrier for "another library also matched these symbols" in any of the three formats. `evidence.identity[]` records a concluded identity, not a rival candidate. The doc row and its extractors land together, since the gate lives in the lib target.

## Rows edited

- **C58:** "Co-emitted with `waybill:also-detected-via` (already C56)" becomes C242.
- **C207:** its remark that "no production call site emits" C56's method key stays true, and now says the key was removed (spec 1639).
- **C233:** its Principle V clause ("C56 records another reader's *mechanism*") stays correct, and now adds that an R9 merge records both C233 and C56.

## Behaviour by case

These are the integration cases in `waybill-cli/tests/also_detected_via.rs`.

| Case | Input | Expected |
|---|---|---|
| A-1 | submodule + `FetchContent_Declare` of one repository at one commit (S1) | one component; C55 unchanged from `main`; C56 lists the other mechanism |
| A-2 | opkg-installed DB + image manifest of one package (S1; fixture from `cross_reader_dedup_ecosystem_107.rs`) | C55 `opkg-installed`; C56 `["yocto-image-manifest"]` |
| A-3 | `.pc` + CMake config package of one library (S1; `common::installed_lib`) | C55 as `main`; C56 the other |
| A-4 | three mechanisms in one group | C56 the two that are not C55, sorted |
| A-5 | two members, same mechanism | no C56 |
| A-6 | conditional fetch under a submodule of the same repository (S6) | C55 `git-submodule`; C233 as `main`; C56 `["cmake-fetchcontent-git"]` |
| A-7 | S6 survivor that S1 merges again | one C56, the union |
| A-8 | design-tier with C55 reconciled into a source-tier survivor with no C55 (S3); a reconciler unit test | no C56 (research R5) |
| A-9 | `cmake-vendored` folded into its project (S7) | C55 `cmake-project`; C56 `["cmake-vendored"]` |
| A-10 | package search paired with a submodule (S9) | C55 `git-submodule`; C56 `["cmake-find-package"]` |
| A-11 | a package search in a subdirectory named like the root project, scanned without `--root-name` (S9) | the project is the main component; CycloneDX `metadata.component` carries C56 `["cmake-find-package"]`; C55 and C56 agree across the formats |
| A-split | A-1's merge under one of two projects, `--split` | C56 on the component in every split document that holds it |
| A-ctl | a tree with no multi-mechanism merge | byte-identical to the golden generated from `main`'s binary before any source change |
| A-rep | every case | 10 scans byte-identical; parity-check agrees on C55, C56, C242; CDX validates; unique bom-refs |
| F-1 | entry mapping of a match with collisions (unit) | C242 written, no C56 |
| F-2 | component carrying C242 through all three emitters | extractors agree |
