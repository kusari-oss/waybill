# Contract: emitted metadata

**Feature**: `1034-nix-eval-tier` (issue **#971 part A**)

Five proposed catalogue rows, continuing
`docs/reference/sbom-format-mapping.md` (highest current row: **C176**). The
Principle V audit for each is in [`../data-model.md`](../data-model.md).

| Row | Field | Scope | Value |
|---|---|---|---|
| C177 | `waybill:nix-eval-superseded-version` | component | the file-parsed version that evaluation overrode |
| C178 | `waybill:nix-eval-tier` | document | `{revision, system, resolved, superseded, evaluated-only, degraded-reason}` |
| C179 | `waybill:nix-eval-system` | document | the system evaluated for |
| C180 | `waybill:nix-eval-degraded` | document | one `DegradationReason` wire form |
| C181 | `waybill:nix-eval-origin` | component | `evaluated` \| `file-parsed` |

## Three-format carriage

Unchanged from the established pattern for every row in the catalogue:

- **CycloneDX 1.6** — component-scope `components[].properties[]`;
  document-scope `metadata.properties[]`.
- **SPDX 2.3** — `packages[].annotations[].comment` / `bom.annotations[]`, via
  the `MikebomAnnotationCommentV1` envelope.
- **SPDX 3** — `Annotation.statement` subject to the package or the
  `SpdxDocument`; same shape as SPDX 2.3.

## Gates these rows must clear

1. **Extractor parity.** Every C-row needs a matching entry in
   `parity/extractors/mod.rs::EXTRACTORS`, or
   `parity::extractors::tests::every_catalog_row_has_an_extractor` and
   `holistic_parity` fail. Rows and extractors land in the **same** change —
   never doc-first (memory: `feedback_sbom_format_mapping_extractor_gate`).
2. **Shape stability.** An extractor can silently compensate for a wire shape,
   so if an annotation's shape changes later, read the extractor before changing
   it (memory: `feedback_extractor_compensates_for_wire_shape`).
3. **Catalogue edits are exact-string, not anchored-regex.** A DOTALL `.*?`
   anchored on one row has silently edited the *next* row in this file before
   (memory: `feedback_anchored_regex_edits_drift`). Edit by exact match and diff
   the whole field.

## Byte-identity requirement

With `--nix-eval` off, **none** of C177–C181 is emitted and every committed
corpus golden is unchanged (spec SC-002). The flag-off path starts no `nix`
process, so there is no partial state that could leak an annotation.

## Regenerating goldens

If goldens change, regenerate **all six** golden-writing test files, not only
the three `*_regression` ones (memory:
`feedback_release_bump_regen_all_golden_tests`), and verify churn with a
normalized, sorted diff that masks content-addressed IDs including `rel-` and
`anno-` prefixes (memory: `feedback_verify_golden_churn_normalized`). Corpus
goldens must be **CI-generated**, not produced locally.
