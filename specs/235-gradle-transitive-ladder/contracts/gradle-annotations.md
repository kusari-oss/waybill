# Contract: Gradle Transparency Annotations (US4)

**Files**: `waybill-cli/src/generate/gradle_annotations.rs`,
`waybill-cli/src/parity/extractors/gradle_resolution_tier.rs`
**Consumers**: CDX / SPDX 2.3 / SPDX 3 emitters
**Parity catalog row**: NEW (e.g., C160 — assigned at implement time)

---

## Annotation vocabulary

| Annotation | Scope | Values | When emitted |
|---|---|---|---|
| `waybill:gradle-resolution-tier` | Document | `subprocess`, `cache`, `static`, `lockfile-only`, `mixed` | Every scan touching ≥1 Gradle project |
| `waybill:gradle-subproject-tier` | Per-component (Gradle main-module) | Same value set minus `mixed` | Only when doc-scope is `mixed` |
| `waybill:gradle-fallback-reason` | Document OR per-component | `timeout`, `missing-tool`, `parse-error`, `cache-miss`, `no-source-files`, `operator-opt-out`, `subprocess-error` | When a tier degraded (attaches to the tier that WON) |
| `waybill:cache-freshness` | Per-component | `fresh`, `stale` | Only when tier == `cache` |
| `waybill:gradle-platform-import` | Per-component (Gradle main-module) | BOM coordinate string | When US3 static parser sees a `platform(...)` call |

## Emission shape per format

### CycloneDX 1.6

- Document-scope annotations → `metadata.properties[]` entries with
  `name` = the full annotation key.
- Per-component annotations → `properties[]` on the specific
  component.

### SPDX 2.3

- Document-scope annotations → `annotations[]` at the document
  level with `annotator: "Tool: waybill-<version>"` and `comment:
  <mikebom-annotation-comment-v1 envelope>`.
- Per-component annotations → `annotations[]` on the specific
  SpdxPackage.

### SPDX 3

- Document-scope annotations → `Annotation` elements with
  `subject` pointing at the SBOM element IRI.
- Per-component annotations → `Annotation` elements with
  `subject` pointing at the component IRI.

## Parity extractor contract

The new C-row extractor at
`waybill-cli/src/parity/extractors/gradle_resolution_tier.rs`:

- **Catalog row**: `waybill:gradle-resolution-tier`
- **Directionality**: `SymmetricEqual` across CDX / SPDX 2.3 / SPDX 3
- **Emitter set**: all three formats emit the same value; the
  extractor asserts byte-equivalence of the canonicalized value
- **Mikebom-annotation-comment-v1 envelope**: matches milestone-071
  precedent
- Follows the existing catalog conventions per memory
  `feedback_sbom_format_mapping_extractor_gate` — the extractor MUST
  land in the same PR as the docs/reference/sbom-format-mapping.md
  row addition, else `every_catalog_row_has_an_extractor` and
  `holistic_parity` tests fail.

## Aggregation logic

The `gradle_annotations::emit` function:

1. Receives `GradleScanSummary { subprojects, aggregate_tier, aggregate_mixed }`.
2. If `aggregate_mixed == false`:
   - Emit document-scope `waybill:gradle-resolution-tier = <aggregate_tier>`.
   - Do NOT emit per-subproject annotations.
3. If `aggregate_mixed == true`:
   - Emit document-scope `waybill:gradle-resolution-tier = "mixed"`.
   - For each subproject, attach
     `waybill:gradle-subproject-tier = <tier>` to that subproject's
     main-module component.
4. If ANY subproject has a non-empty `fallback_history`:
   - Emit `waybill:gradle-fallback-reason` on the affected component
     (or document if the fallback affected the aggregate tier
     decision).
5. If any component came from `tier == Cache`:
   - Emit `waybill:cache-freshness = <fresh|stale>` per that
     component.

## Test surface

- Golden tests exercise all five value permutations of
  `waybill:gradle-resolution-tier` (one golden per US1 / US2 / US3
  / lockfile-only / mixed).
- Parity extractor test asserts the annotation value is byte-equal
  across CDX / SPDX 2.3 / SPDX 3 for each fixture.
- The `every_catalog_row_has_an_extractor` test picks up the new
  row automatically.
