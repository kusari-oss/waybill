# Data Model: milestone 166 — SPDX 3 annotation dedup fix

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Phase 1 data model. Milestone 166 is a pure implementation fix — no new component types, no new annotations, no new parity-catalog rows. This document catalogs the small Rust surface: one new helper function + one call-site update + one info-log field extension.

## Rust types

### E1 — `dedup_annotations_by_spdx_id` (NEW helper function)

**Location**: `mikebom-cli/src/generate/spdx/v3_annotations.rs` — add near end of file, adjacent to existing `sort_by_spdx_id`.

**Signature**:

```rust
/// Milestone 166 (implements m166 FR-001 through FR-006) — dedup a
/// vector of SPDX 3 Annotation JSON values by `spdxId`. Preserves LAST-
/// writer-wins semantics (per research §R2) so builder order determines
/// which entry survives when duplicates exist. Also returns the drop
/// count for FR-007 tracing observability.
///
/// Empirically-motivated fix for the duplicate-Annotation-spdxId bug
/// surfaced by milestone 165's audit on `github.com/kubernetes/kubernetes`
/// (2 duplicates of 4477 annotations, 0.04%) and `github.com/argoproj/argo-cd`
/// (1 duplicate). `spdx3-validate` FAILS on any document with duplicate
/// spdxIds because SPDX 3.0.1 SHACL constraint `Annotation.statement`
/// is max-1-per-subject.
///
/// The BTreeMap iteration order is lexicographic by spdxId string —
/// eliminating the need for the prior explicit `sort_by` step at the
/// call site (research §R3).
pub(crate) fn dedup_annotations_by_spdx_id(
    annotations: Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, usize) {
    let mut map: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    let mut dropped: usize = 0;
    for anno in annotations {
        let key = anno["spdxId"].as_str().unwrap_or("").to_string();
        if map.insert(key, anno).is_some() {
            dropped += 1;
        }
    }
    (map.into_values().collect(), dropped)
}
```

**Fields**: pure function. Input `Vec<Value>` of arbitrary size; output `(Vec<Value>, usize)` — deduped vector (lex-sorted by spdxId) + drop count.

**Relationships**: called by `build_v3_document` at `v3_document.rs:754-820` merge point. Returned vector replaces the current `annotations` variable; drop count feeds the FR-007 tracing log field.

**Validation rules**:
- Empty input → empty output, 0 drops (validated by SC-008 sub-test d).
- Single element → single-element output, 0 drops.
- Two identical-spdxId entries → 1-element output, 1 drop.
- Two different-spdxId entries → 2-element output, 0 drops.
- LAST-writer-wins on duplicate: the returned Annotation for a duplicated spdxId is the LAST one in input order (validated by SC-008 sub-test e).
- Malformed input (missing `spdxId`): all entries with missing spdxId collapse to a single empty-string key entry — matches existing `sort_by_spdx_id` behavior at `v3_document.rs:815` (defensive; not expected in practice).

### E2 — `v3_document.rs` merge-point update (EDITED)

**Location**: `mikebom-cli/src/generate/spdx/v3_document.rs:754-820`.

**Pre-166** (current shape):

```rust
let mut annotations: Vec<Value> = Vec::new();
annotations.extend(super::v3_annotations::build_component_annotations(...));
annotations.extend(super::v3_annotations::build_document_annotations(...));
annotations.extend(super::v3_annotations::build_supplement_service_annotations(...));
if scan.user_metadata.metadata_comment.is_some() || !scan.user_metadata.annotations.is_empty() {
    // user-supplied --metadata-comment + --annotator loops emit here
}
annotations.sort_by(|a, b| {
    let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
    key(a).cmp(&key(b))
});
for anno in annotations {
    graph.push(anno);
}
```

**Post-166**:

```rust
let mut annotations: Vec<Value> = Vec::new();
annotations.extend(super::v3_annotations::build_component_annotations(...));
annotations.extend(super::v3_annotations::build_document_annotations(...));
annotations.extend(super::v3_annotations::build_supplement_service_annotations(...));
if scan.user_metadata.metadata_comment.is_some() || !scan.user_metadata.annotations.is_empty() {
    // user-supplied --metadata-comment + --annotator loops emit here (unchanged)
}
// Milestone 166 (T004): dedup by spdxId at merge point per FR-001-FR-006.
// BTreeMap iteration is already lex-sorted by spdxId (per research §R3)
// so the prior explicit sort_by step is unnecessary — the dedup helper
// returns a pre-sorted Vec.
let (annotations, spdx3_annotation_duplicates_dropped) =
    super::v3_annotations::dedup_annotations_by_spdx_id(annotations);
for anno in annotations {
    graph.push(anno);
}
```

**Change surface**: replace the sort-then-push block with `dedup + push`. The BTreeMap's natural lex order preserves the sort behavior; explicit sort call is removed.

### E3 — FR-007 tracing log field extension (EDITED)

**Location**: `mikebom-cli/src/generate/spdx/v3_document.rs` — the info log at end of `build_v3_document`. If no such log exists today, one MUST be added per FR-007.

**Verification at implementation time**: `grep -n 'tracing::info!\|tracing::warn!' mikebom-cli/src/generate/spdx/v3_document.rs` to locate any existing SPDX 3 emission log. If found, extend with new field. If not found, add a new one:

```rust
tracing::info!(
    doc_iri = %doc_iri,
    graph_element_count = graph.len(),
    // Milestone 166 (T005, FR-007): dedup counter surfaces redundant
    // emitter code paths — zero on healthy scans; non-zero surfaces
    // future-milestone investigation candidates.
    spdx3_annotation_duplicates_dropped = spdx3_annotation_duplicates_dropped,
    "spdx3 document emitted"
);
```

**Rationale**: Grep-friendly per milestone-157-onwards observability convention. Zero-baseline case emits `spdx3_annotation_duplicates_dropped=0` so downstream CI parsers can always find the field.

## Wire types

**None.** Milestone 166 changes intermediate builder-merge-time state. The emitted SPDX 3 wire format shape is UNCHANGED — only DUPLICATE entries disappear from `@graph[]`. Every kept Annotation element's shape is byte-identical to pre-166.

## Relationships

```text
build_v3_document (v3_document.rs)
    ↓ calls
build_component_annotations   ─┐
build_document_annotations    ─┤
build_supplement_service_...  ─├─→ Vec<Value> (~5000 annotations, 0-N duplicates)
+ user metadata/annotator     ─┘
                               ↓
                          dedup_annotations_by_spdx_id  (NEW — Milestone 166 T003)
                               ↓
                          (Vec<Value>, usize)  — deduped + drop count
                               ↓
                          for anno in deduped_vec { graph.push(anno); }
                               ↓
                          tracing::info! with FR-007 field
                               ↓
                          Ok(json!({"@context": SPDX_3_CONTEXT, "@graph": graph}))
```

## State transitions

**None.** Milestone 166 is a pure post-processing pass at emission time. No lifecycle state.

## Data volume assumptions

- **Per-scan input**: 100-5000 annotations depending on scan target size. Podman-desktop ≈ 4477. Milestone-090 fixtures ≈ 100-500. Kubernetes ≈ 4477. ArgoCD ≈ smaller (fewer components).
- **Per-scan expected drop count**: 0 on milestone-090 fixtures (pre-166 conformance test passes); 1-3 on real upstream Go monorepos (empirically observed on K8s and ArgoCD).
- **BTreeMap memory**: ~200 bytes per entry × 5000 entries ≈ 1 MB peak; released after `into_values()` consumes the map.
- **Runtime**: O(N log N) BTreeMap inserts + O(N) final `into_values()`. On 5000 entries: microseconds.

## Validation rules (aggregated)

| Rule | Enforcement |
|------|-------------|
| No duplicate `spdxId` in emitted `@graph[]` (SC-004) | Enforced by construction — `BTreeMap` key uniqueness. Verified by unit test T007. |
| SPDX 3 goldens' conformance test passes (SC-003 + FR-008) | Enforced by existing `mikebom-cli/tests/spdx3_conformance.rs` (milestone 078). Post-166 regenerated goldens tested. |
| Byte-identity for CDX + SPDX 2.3 (SC-005 + FR-010) | Enforced by scope — only SPDX 3 emission path touched. Verified via existing golden tests continuing to pass. |
| LAST-writer-wins on duplicate (FR-004) | Enforced by `BTreeMap::insert`'s replace semantics. Verified by unit test T008. |
| FR-007 log fires per scan | Enforced by unconditional `tracing::info!` call. Verified by unit test T009 via `tracing::subscriber::fmt::TestWriter`. |
| No content change in retained Annotation elements (FR-003) | Enforced by construction — dedup only DROPS whole elements; doesn't modify retained ones. Verified by existing `spdx3_annotation_fidelity.rs` test (milestone 145) continuing to pass. |
| SPDX 3 conformance on Kubernetes + ArgoCD post-166 (SC-001 + SC-002) | Verified via integration test T012 + optional real-testbed audit T013 (m165 methodology reused). |
