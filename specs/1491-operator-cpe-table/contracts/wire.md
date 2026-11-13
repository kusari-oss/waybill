# Contract: wire

## Native CPE source (FR-007, FR-008)

### Existing strings (unchanged)

| Source | CycloneDX `evidence.identity[]` method (`field: "cpe"`) | SPDX 2.3 `externalRefs[].comment` and SPDX 3 `ExternalIdentifier.comment` |
|---|---|---|
| nixpkgs-declared | technique `manifest-analysis`, value `nixpkgs meta.identifiers.cpe` | `CPE declared by nixpkgs meta.identifiers.cpe` |
| curated (closure members only) | technique `other`, value `waybill curated CPE table` | `CPE from waybill's curated CPE table` |

### New strings

| Source | CycloneDX method | SPDX comment |
|---|---|---|
| `--root-cpe` | technique `other`, value `operator --root-cpe` | `CPE supplied by the operator with --root-cpe` |
| `--supplement-cdx` | technique `other`, value `operator --supplement-cdx` | `CPE supplied by the operator with --supplement-cdx` |
| `--cpe-map` | technique `other`, value `operator --cpe-map` | `CPE supplied by the operator with --cpe-map` |

The same `cpe_source` function feeds all three emitters, so the formats cannot disagree. This applies to every component an operator surface set, and to the subject, synthetic or not.

## C212 `waybill:cpe-provenance` (FR-009)

The value is a JSON string: `{"basis":"<b>","overrode":"<cpe>"}`, with `overrode` optional.

- **`basis`** is one of:
  - `drv` or `name-version`: nixpkgs-declared, unchanged;
  - `purl`: supplement;
  - `cpe-map`;
  - `root-cpe`: the subject, only when it is a scanned main module.
- **Carriers.** CycloneDX `properties[]`, including `metadata.component.properties` for a main-module subject (research R7). SPDX 2.3 annotation envelope. SPDX 3 `Annotation.statement`.
- **Parity.** `SymmetricEqual`, `order_sensitive: false`, unchanged.
- **Catalog.** The C212 row in `docs/reference/sbom-format-mapping.md` drops "emitted only on a closure component" and lists the five `basis` values.

## Not changed

- C209 `waybill:cpe-coverage`;
- C184;
- every other annotation;
- every document produced without the new flags or a supplement.
