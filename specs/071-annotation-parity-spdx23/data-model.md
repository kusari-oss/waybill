# Data Model — milestone 071 cross-format annotation parity

The milestone touches three existing data structures plus introduces one piece of optional metadata. No new top-level types.

## Entities

### `ParityExtractor` (existing — extended)

Located at `mikebom-cli/src/parity/extractors/common.rs:31`. Represents one row in the cross-format parity catalog.

**Existing fields**:

| Field | Type | Purpose |
|---|---|---|
| `row_id` | `&'static str` | Stable catalog identifier (e.g., `"A1"`, `"C18"`, `"C42"`). Drives publication into `docs/reference/sbom-format-mapping.md`. |
| `label` | `&'static str` | Human-readable name (e.g., `"mikebom:source-files"`, `"PURL"`). |
| `cdx` | `fn(&Value) -> BTreeSet<String>` | Per-doc extractor over a CDX 1.6 JSON document. |
| `spdx23` | `fn(&Value) -> BTreeSet<String>` | Per-doc extractor over a SPDX 2.3 JSON document. |
| `spdx3` | `fn(&Value) -> BTreeSet<String>` | Per-doc extractor over a SPDX 3.0.1 JSON document. |
| `directional` | `Directionality` | The cross-format invariant the catalog row asserts. |

**New field added by this milestone**:

| Field | Type | Default | Purpose |
|---|---|---|---|
| `order_sensitive` | `bool` | `false` | When the row's value payload contains a JSON array, `false` means the array is canonicalized by lexicographic sort before equality comparison. `true` preserves insertion order. Per Q2 clarification, the default is `false` and all six known problem keys use the default. |

**Validation**: A row marked `Directionality::SymmetricEqual` MUST yield `BTreeSet<String>` outputs that compare equal across all three formats after applying the canonicalization governed by `order_sensitive`. Other directionality variants impose their own invariants (see `Directionality` below).

### `Directionality` (existing — unchanged)

Located at `mikebom-cli/src/parity/extractors/common.rs:41`. The cross-format invariant a row asserts. **No new variants** — Q1/Q2/Q3 clarifications fit the existing 4-variant model.

| Variant | Invariant | Catalog usage |
|---|---|---|
| `SymmetricEqual` | `cdx_set == spdx23_set == spdx3_set` after canonicalization | Default for `mikebom:*` keys; ~60 of 68 rows. |
| `CdxSubsetOfSpdx` | `cdx_set ⊆ spdx23_set ∧ cdx_set ⊆ spdx3_set` | Native fields where SPDX may carry richer detail (A12 CPE). |
| `PresenceOnly` | All three sets are non-empty (no value comparison) | Rows where formats structurally diverge but all carry the datum (D1, E1, B4, C22). |
| `CdxOnly` | `cdx_set` non-empty; SPDX sides intentionally not asserted | Native fields where CDX is the only format expressing the signal because SPDX uses a native construct asserted *by a different catalog row* (C42 → B2). |

**Validation**: Adding a new variant requires a constitution-style amendment to this milestone's contracts, not a quiet code change.

### `MikebomAnnotationCommentV1` envelope (existing — unchanged)

Located conceptually at `mikebom-cli/src/generate/spdx/annotations.rs:31` (`ENVELOPE_SCHEMA_V1 = "mikebom-annotation/v1"`); JSON-Schema'd at `mikebom-cli/src/generate/spdx/contracts/mikebom-annotation.schema.json`. The canonical SPDX 2.3 carrier for every `mikebom:*` annotation key.

**Shape** (existing):

```json
{
  "schema": "mikebom-annotation/v1",
  "field": "mikebom:<key-name>",
  "value": <JSON value: string | array | object>
}
```

Carried as the `comment` of an `Annotation` row whose `annotationType: "OTHER"` and `annotator` carries the mikebom tool identifier. The decode helper `extract_mikebom_annotation_values()` at `parity/extractors/common.rs:86` is the canonical reader.

**This milestone does NOT modify the envelope shape.** It only ensures more keys flow through it.

### `Annotation key` (conceptual — not a Rust type)

A `mikebom:<name>` string appearing as either:

- A CDX `properties[].name` value on `components[]` entries.
- An SPDX 2.3 envelope `field` value inside `Package.annotations[].comment`.
- An SPDX 3 `Annotation` whose `subject` is a `Package` IRI and whose payload's `mikebom:<name>` field is set (per `extract_mikebom_annotation_values()` decode).

**Identity**: Two annotations are "the same key" iff the literal string after `mikebom:` matches. Case-sensitive, no hyphen/underscore equivalence.

### `CatalogRowRationale` (new — documentation-side artifact)

Not a Rust struct — a contract about inline doc-comments. For every `ParityExtractor` row whose `directional != SymmetricEqual`, the row's catalog entry MUST carry a Rust line-comment of the form:

```rust
// <Directionality>: <one-line rationale>. Standards-native: <which-row-or-field-supersedes>.
ParityExtractor { row_id: "C42", label: "mikebom:lifecycle-scope", ..., directional: Directionality::CdxOnly },
// CdxOnly: SPDX uses native dep-relationship types for lifecycle scope. Standards-native: B2 typed dep-relationship + SPDX 3 LifecycleScopeType.
```

A doc-sync test reads these comments and asserts they appear verbatim in `docs/reference/sbom-format-mapping.md` under the parity-catalog section.

## Relationships

```text
ParityExtractor (row)
   │
   ├── 1 ── identifies ──> Annotation key OR native field
   ├── 1 ── asserts ─────> Directionality invariant
   ├── 1 ── carries ─────> order_sensitive: bool
   ├── 3 ── runs ────────> per-format extractor fns (cdx, spdx23, spdx3)
   └── 1 ── (if non-SymmetricEqual) requires CatalogRowRationale comment

MikebomAnnotationCommentV1 envelope
   ├── 1..N ── carries ──> Annotation key (per Package)
   └── 1 ────── decoded by ──> extract_mikebom_annotation_values()

Component identity (parity-comparison)
   └── canonical PURL string is the cross-format join key (D3 in research.md)
```

## State / lifecycle

The parity catalog is static (compile-time constant `EXTRACTORS: &[ParityExtractor]`). No mutation, no migrations.

The `order_sensitive` field default of `false` means existing catalog rows compile unchanged after the field is added (Rust default-on-struct-literal-add requires a struct-update-syntax migration; an alternative is a `#[derive(Default)]` constructor — choice deferred to implementation per Tasks).

## Validation rules

- **VR-001**: Every `mikebom:*` literal string appearing in any of the three emitter modules (`generate/cyclonedx/`, `generate/spdx/`, `generate/spdx/v3_*`) on a component-level construct MUST have a `ParityExtractor` row with a matching `label`. The pre-PR gate test (D5) enforces this.
- **VR-002**: Every row with `directional == SymmetricEqual` MUST satisfy `cdx_set == spdx23_set == spdx3_set` after canonicalization on every fixture in the byte-identity golden set.
- **VR-003**: Every row with `directional != SymmetricEqual` MUST carry a Rust line-comment matching the `CatalogRowRationale` shape.
- **VR-004**: Every non-`SymmetricEqual` row's rationale MUST be present verbatim in `docs/reference/sbom-format-mapping.md` under the parity-catalog section. A doc-sync test enforces this.
- **VR-005**: When `order_sensitive == true`, the row's per-format extractors MUST return values whose JSON-array contents preserve the originally-emitted order. (Negative invariant: if order matters, sorting would discard semantics.)
