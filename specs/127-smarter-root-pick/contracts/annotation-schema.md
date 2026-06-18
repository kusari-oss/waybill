# Contract: `mikebom:root-selection-heuristic` annotation JSON shape

## Stable annotation envelope

This annotation follows the existing `mikebom-annotation/v1` envelope convention (milestones 011, 080, 119). Operators consuming SBOMs SHOULD parse the JSON value structurally and rely on the documented fields; the envelope's `schema` discriminator stays stable so consumers can version-gate.

## Schema

```json
{
  "schema": "mikebom-annotation/v1",
  "field": "mikebom:root-selection-heuristic",
  "value": {
    "heuristic": "<one-of-five-known-strings>",
    "confidence": <float-in-0-to-1>
  }
}
```

## Field reference

| Field path | Type | Description | Validity |
|---|---|---|---|
| `schema` | string | Envelope schema version | Always `"mikebom-annotation/v1"` |
| `field` | string | Annotation key | Always `"mikebom:root-selection-heuristic"` |
| `value.heuristic` | string | Which selection branch fired | One of: `"repo-root-main-module"`, `"ecosystem-priority"`, `"longest-common-prefix"`, `"maven-scan-target-coord"`, `"synthetic-placeholder"` |
| `value.confidence` | float | Operator-actionable trust score for the auto-pick | A fixed value per heuristic per the table below; modelable as a `Decimal(3, 2)` for downstream storage |

## Heuristic-to-confidence table

| Heuristic name | Confidence | When emitted |
|---|---|---|
| `repo-root-main-module` | 0.95 | Exactly one main-module's manifest sits at the scan root |
| `longest-common-prefix` | 0.80 | No main-module at the scan root; LCP of manifest paths matches exactly one main-module |
| `ecosystem-priority` | 0.70 | ≥2 main-modules at the scan root; fixed priority order picks one |
| `maven-scan-target-coord` | 0.60 | No main-module at the scan root, LCP failed to pick; JAR walker has a `scan_target_coord` |
| `synthetic-placeholder` | 0.30 | No main-module exists; falls back to `pkg:generic/<target>@0.0.0` |

These five enum variants are the only ones that produce the annotation. The two implicit "no annotation" cases:

- **single-main-module** (count==1 fast path): conceptual confidence 1.0; annotation suppressed to preserve byte-identity on all 33 alpha.48 goldens (SC-003).
- **operator-override** (`--root-name`, etc.): conceptual confidence 1.0; the milestone-077 override audit channel is the right surface.

## Forward compatibility

The heuristic-name enum is *open* — future tiebreakers may add variants. Consumers SHOULD treat an unknown `heuristic` value as "trust the confidence value, mark the selection as auto-picked, don't assume the heuristic name is exhaustive." The `mikebom-annotation/v1` schema version stays stable as long as fields are added (not removed or repurposed). Removing fields, repurposing existing names, or changing the confidence table requires a `v2` bump.

## Native-field bridge documentation (Principle V parity-catalog)

A new C-row in `docs/reference/sbom-format-mapping.md` carries:

```text
| C-row | Annotation key                     | CDX 1.6                              | SPDX 2.3                              | SPDX 3.0.1                            | Native field exists? |
|-------|------------------------------------|--------------------------------------|---------------------------------------|---------------------------------------|----------------------|
| C69*  | mikebom:root-selection-heuristic   | metadata.properties[]                | document-level annotations[]          | top-level annotations[]               | No (parity bridge)    |
```

*C-row number TBD at PR review per research R7.

The "Native field exists?" column is `No (parity bridge)` because the audit at research R1 verified that none of CDX 1.6, SPDX 2.3, or SPDX 3.0.1 has a native field carrying the "which heuristic selected the document subject" signal. CDX's `evidence.identity.confidence` is component-scoped (not document-scoped) and doesn't carry a heuristic name — it covers a different parity gap.
