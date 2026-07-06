# Contracts: milestone 166 — SPDX 3 annotation dedup fix

**No new external contracts.**

Milestone 166 is a pure implementation fix at the SPDX 3 emission code path. The emitted SBOM wire format is byte-identical in SHAPE — only DUPLICATE entries disappear from `@graph[]`. Every retained Annotation element is byte-identical to pre-166.

- **CLI**: no new flags.
- **Emitted SBOM shape**: no new element types, no new annotation vocabularies, no new parity-catalog rows. Consumers reading milestone-165-era SBOMs read milestone-166-era SBOMs identically (post-dedup).
- **Parity catalog**: no new rows.
- **`mikebom:*` annotations**: no new annotations.
- **Tracing convention**: adds 1 new field (`spdx3_annotation_duplicates_dropped=<N>`) to the SPDX 3 emission info log per FR-007. Backward-compat for regex consumers (new field appends, doesn't reorder existing fields).

## SBOM consumer-observable delta

**Pre-166** — `@graph[]` on a scan that triggers the bug (e.g., Kubernetes):

```json
{
  "@graph": [
    ...,
    {"type": "Annotation", "spdxId": "...anno-GJJZ6XAC7UZOZO57", "subject": "...doc-...", "statement": "...mikebom:graph-completeness=partial..."},
    ...,
    {"type": "Annotation", "spdxId": "...anno-GJJZ6XAC7UZOZO57", "subject": "...doc-...", "statement": "...mikebom:graph-completeness=partial..."},
    ...
  ]
}
```

Two entries with identical `spdxId`. `spdx3-validate` FAILS with `More than 1 values on ns1:statement`.

**Post-166** — same scan input:

```json
{
  "@graph": [
    ...,
    {"type": "Annotation", "spdxId": "...anno-GJJZ6XAC7UZOZO57", "subject": "...doc-...", "statement": "...mikebom:graph-completeness=partial..."},
    ...
  ]
}
```

Single entry per `spdxId`. `spdx3-validate` PASSES.

Consumers running any SPDX 3 validator on mikebom output benefit immediately, with zero code changes. Existing consumers indexing `@graph[]` by `spdxId` (map-lookup pattern) get correct behavior instead of silent last-write-wins overwrite.
