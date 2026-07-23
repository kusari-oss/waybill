# Contract: Cross-ecosystem annotation payload shapes

**Feature**: 218-cross-ecosystem-edges | **Related**: FR-005, FR-007, FR-011, parity C137/C138/C139

## C137 — `waybill:cross-ecosystem-inference` (per-edge, base)

### Scope
Per-edge. Emitted on EVERY DEPENDS_ON edge produced by the cross-ecosystem bridge (FR-005). Silent for same-ecosystem edges (FR-006).

### Value shape
Canonical JSON of `CrossEcosystemInferencePayload` (data-model E1). Compact, alphabetic field order.

```json
{"from_eco":"generic","lookup_via":"gemfile-lock-dependencies","target_purl":"pkg:gem/fastlane@2.220.0","to_eco":"gem"}
```

### Per-format landing slots

| Format      | Landing slot                                             | Envelope                            |
|-------------|----------------------------------------------------------|-------------------------------------|
| CycloneDX   | `dependencies[i].properties[]` on the source-side entry  | `{name, value}` where value is the canonical JSON string above |
| SPDX 2.3    | `Package.annotations[]` on the source Package            | `MikebomAnnotationCommentV1` envelope; `field: "waybill:cross-ecosystem-inference"`; `value` is the canonical JSON string |
| SPDX 3      | `Annotation` element with `subject` = Relationship IRI   | Standard Annotation with `field` + `value` matching the SPDX 2.3 envelope shape |

### Disambiguation strategy
CDX + SPDX 2.3 have no per-target-within-source annotation slot. When a source has N targets and M are cross-ecosystem crosses, the source Package/Component gets M property/annotation objects — each disambiguated by its `target_purl` field.

## C138 — `waybill:cross-ecosystem-inference-ambiguous` (per-edge, ambiguous variant)

### Scope
Per-edge. Emitted on every edge that is part of an ambiguous multi-ecosystem match (FR-003 emit-all path). Every affected edge ALSO carries C137.

### Value shape
Canonical JSON of `CrossEcosystemInferenceAmbiguousPayload` (data-model E2). Extends the base C137 payload with an `alternates: [{target_purl, to_eco}]` field enumerating sibling matches.

```json
{"alternates":[{"target_purl":"pkg:npm/json@1.0.0","to_eco":"npm"},{"target_purl":"pkg:pypi/json@0.1.1","to_eco":"pypi"}],"from_eco":"generic","lookup_via":"gemfile-lock-dependencies","target_purl":"pkg:gem/json@2.7.1","to_eco":"gem"}
```

### Per-format landing slots
Same as C137. Both C137 and C138 emit on the same edge; consumers checking for ambiguity look for C138's presence.

### Validation
`alternates.len() >= 1`; sorted lex by `target_purl`; self-consistency (the current edge's own `{target_purl, to_eco}` MUST NOT appear in `alternates`).

## C139 — `waybill:cross-ecosystem-inference-unresolved` (doc-scope)

### Scope
Document-scope. ONE annotation per SBOM. Emitted iff FR-004 recorded ≥1 unresolved name during the scan; silent otherwise (FR-011 silence-on-absence — matches m217 C136 precedent).

### Value shape
Canonical JSON of `Vec<CrossEcosystemInferenceUnresolvedRecord>` (data-model E3). Sorted lex by `(source_purl, unresolved_name)`.

```json
[{"source_purl":"pkg:generic/my-app@0.0.0-unknown","unresolved_name":"nonexistent-gem"}]
```

### Per-format landing slots

| Format      | Landing slot                                              | Envelope                            |
|-------------|-----------------------------------------------------------|-------------------------------------|
| CycloneDX   | `metadata.properties[]` (document scope, existing slot)  | `{name: "waybill:cross-ecosystem-inference-unresolved", value: <json-string>}` |
| SPDX 2.3    | Document-level `Annotation` on SPDXRef-DOCUMENT           | `MikebomAnnotationCommentV1` envelope; `field` + `value` |
| SPDX 3      | `Annotation` element with `subject` = SpdxDocument IRI   | Standard Annotation with `field` + `value` |

Same landing pattern as m217 `waybill:go-toolchain-detected` (C136) and m176 `waybill:workspaces-detected` (C121).

## Parity-catalog registration

Three new C-rows registered in `docs/reference/sbom-format-mapping.md` following the C121–C136 KEEP-NO-NATIVE template. Each row cites:
- **Native alternatives considered + rejected** per Constitution Principle V.
- **Payload shape** per section above.
- **Per-format landing slot**.
- **Milestone 218 provenance clause**.

Three new extractor triplets registered in `waybill-cli/src/parity/extractors/`:
- `c137_cdx` / `c137_spdx23` / `c137_spdx3` — per-edge, source-Component/Package scope.
- `c138_cdx` / `c138_spdx23` / `c138_spdx3` — per-edge, source-Component/Package scope.
- `c139_cdx` / `c139_spdx23` / `c139_spdx3` — document scope.

Three new EXTRACTORS rows in `parity/extractors/mod.rs`. `every_catalog_row_has_an_extractor` bidirectional test asserts registration on both sides.

## Consumer-observed invariants

- **Invariant 1**: `annotation("waybill:cross-ecosystem-inference-ambiguous") on edge E` ⇒ `annotation("waybill:cross-ecosystem-inference") on edge E`. (C138 implies C137.)
- **Invariant 2**: Same-ecosystem edge (source and target belong to the same ecosystem per PURL type) ⇒ NO C137 AND NO C138 annotation. Enforced by bidirectional parity extractor (SC-004 gate).
- **Invariant 3**: `edge count carrying C137 == report.crossed_edges.len()` AND `edge count carrying C138 == report.ambiguous_edges.len()` per emit-time construction. Verified by an in-process integration test asserting `annotation_count == report_field_len`.
- **Invariant 4**: Doc scope C139 present ⇔ scan had ≥1 unresolved cross-ecosystem name. Verified by SC-008 integration test.

## Wire canonicalization

All three payloads serialize via `serde_json::to_string(&payload)` on structs whose fields are declared in alphabetic order. serde emits fields in declaration order, so output is deterministic without any sort step. This is the same pattern m134 `DivergenceRecord` uses.

Byte-identity across scan runs: for the same input scan + same flag state, the emitted annotation bytes MUST be identical. Verified by golden-file comparison at test time.
