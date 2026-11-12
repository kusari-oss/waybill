# Contract: C218 `waybill:boot-library-compiler`

**Scope:** component. **Encoding:** a plain string.

**Carriers:** CycloneDX `components[].properties[]`; SPDX 2.3 per-Package `annotations[].comment` envelope (`MikebomAnnotationCommentV1`); SPDX 3 per-element `Annotation`. **Parity:** `SymmetricEqual`, with an extractor in each format.

**Present when:** the component is a GHC boot library and its version was taken from the boot-library table for this compiler. **Absent** on every other component, including a boot library that stayed `compiler-supplied` because no compiler was identified or the compiler is absent from the table (C219 says which).

## Value

The compiler release as the closure names it: `ghc-9.10.3`, `ghc-9.6.6`, `ghc-binary-9.8.4`. Pattern: `^ghc(-binary)?-[0-9]+(\.[0-9]+)+$`.

## Rules

- The component's `version` equals the table's version for `(release, name)` on the closure's system.
- A component carrying C218 carries none of C170, C171, or a C151 "no version" reason.
- Two components of one library may both carry C218, with different values, only when their versions differ (FR-009). A shared version is one component, and it carries the project compiler's label.
- `sbom-tier` is unchanged by this row (`design` stays `design`, clarified 2026-11-12).

## Catalog row

KEEP-NO-NATIVE. The audit is in research R6. CycloneDX `evidence.identity[].methods[].technique` has no term for a published per-release table. SPDX 2.3 and SPDX 3.0.1 have no version-provenance field. C170, C171, C175 and C181 were each audited and rejected.
