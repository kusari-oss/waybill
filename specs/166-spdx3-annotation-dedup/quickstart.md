# Quickstart: milestone 166 — SPDX 3 annotation dedup fix

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Contributor onboarding for milestone 166.

## 1. Prerequisites

- Rust stable toolchain (workspace-managed).
- `spdx3-validate==0.0.5` at `.venv/spdx3-validate/bin/spdx3-validate` (per memory `reference_spdx3_validator`).
- Optional: cached copies of Kubernetes + ArgoCD upstream clones for SC-001/SC-002 empirical closure verification (see milestone-165 quickstart).

## 2. Reproduce the bug (before implementing the fix)

```bash
# Build post-m165 baseline
cargo +stable build --release -p mikebom
./target/release/mikebom --version   # expect 0.1.0-alpha.52 or later

# Regenerate Kubernetes SPDX 3 SBOM (needs a K8s clone at /tmp/k8s/kubernetes or similar)
./target/release/mikebom --offline sbom scan \
    --path /tmp/k8s/kubernetes \
    --format spdx-3-json \
    --output /tmp/mikebom-k8s.spdx3.json \
    --no-deep-hash

# Verify duplicate exists
python3 -c "
import json
d = json.load(open('/tmp/mikebom-k8s.spdx3.json'))
graph = d.get('@graph') or []
from collections import Counter
c = Counter((n.get('spdxId'), n.get('type')) for n in graph if n.get('type') == 'Annotation')
dupes = [(k, v) for k, v in c.items() if v > 1]
print(f'Annotation dupes: {len(dupes)}')
if dupes:
    print(f'Sample: {dupes[0]}')
"

# Run spdx3-validate — MUST FAIL pre-166 with `More than 1 values on ns1:statement`
.venv/spdx3-validate/bin/spdx3-validate --json /tmp/mikebom-k8s.spdx3.json --quiet
echo "Exit code: $?"   # non-zero pre-166
```

## 3. Implementation overview

Milestone 166 is targeted at 2 files:

- `mikebom-cli/src/generate/spdx/v3_annotations.rs` — add `dedup_annotations_by_spdx_id` helper (per data-model.md E1).
- `mikebom-cli/src/generate/spdx/v3_document.rs` — call the helper at the annotation-merge point, replacing the existing `sort_by` step (per data-model.md E2). Add the FR-007 tracing field (per data-model.md E3).

Plus 1 new integration test file at `mikebom-cli/tests/spdx3_annotation_dedup.rs` (per data-model.md validation §SC-009).

**Total surface**: 3 files (2 edited, 1 new). ~30-40 line implementation diff + ~150 line test.

## 4. Step-by-step implementation

### 4a. Add `dedup_annotations_by_spdx_id` helper (T003)

At the end of `mikebom-cli/src/generate/spdx/v3_annotations.rs`:

```rust
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

### 4b. Update merge point at `v3_document.rs:754-820` (T004)

Replace:

```rust
annotations.sort_by(|a, b| {
    let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
    key(a).cmp(&key(b))
});
for anno in annotations {
    graph.push(anno);
}
```

With:

```rust
let (annotations, spdx3_annotation_duplicates_dropped) =
    super::v3_annotations::dedup_annotations_by_spdx_id(annotations);
for anno in annotations {
    graph.push(anno);
}
```

### 4c. Add FR-007 tracing log (T005)

Verify existing SPDX 3 emission info logs first (`grep -n 'tracing::info!' mikebom-cli/src/generate/spdx/v3_document.rs`). Either extend an existing log with the new field, or add a new log at the end of `build_v3_document`:

```rust
tracing::info!(
    doc_iri = %doc_iri,
    graph_element_count = graph.len(),
    spdx3_annotation_duplicates_dropped = spdx3_annotation_duplicates_dropped,
    "spdx3 document emitted"
);
```

### 4d. Write unit tests (T006-T010)

At end of `v3_annotations.rs`'s `#[cfg(test)] mod tests` block. See tasks.md for the enumerated 5+ tests per SC-008.

### 4e. Write integration test (T012)

Create `mikebom-cli/tests/spdx3_annotation_dedup.rs` per SC-009 — synthesize a scan producing duplicate annotations + assert `@graph[]` has no duplicates + FR-007 log fires.

### 4f. Regenerate goldens if needed (T014)

Run `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test` and inspect the diff. Post-166 SPDX 3 goldens MAY drift if any fixture previously contained duplicates. Verify diff is limited to REMOVED entries + `_dropped` log field addition. NO new entries; NO content changes to retained annotations. If any fixture golden shows non-dedup-related changes, that's a bug.

## 5. Testing

```bash
# Full pre-PR gate
./scripts/pre-pr.sh

# Unit tests
cargo +stable test --bin mikebom generate::spdx::v3_annotations

# Integration test
cargo +stable test --test spdx3_annotation_dedup

# SPDX 3 conformance regression
cargo +stable test --test spdx3_conformance

# Annotation fidelity regression
cargo +stable test --test spdx3_annotation_fidelity
```

## 6. Verify the fix on Kubernetes + ArgoCD (SC-001 + SC-002)

```bash
# Rebuild
cargo +stable build --release -p mikebom

# Re-scan Kubernetes (assuming /tmp/k8s/kubernetes exists)
./target/release/mikebom --offline sbom scan \
    --path /tmp/k8s/kubernetes \
    --format spdx-3-json \
    --output /tmp/mikebom-k8s-post166.spdx3.json \
    --no-deep-hash

# Validate — MUST PASS post-166
.venv/spdx3-validate/bin/spdx3-validate --json /tmp/mikebom-k8s-post166.spdx3.json --quiet
echo "Exit code: $?"   # 0 post-166

# Same for ArgoCD
```

## 7. Common pitfalls

- **Forgetting to remove the explicit sort**: post-166, `BTreeMap` iteration is already lex-sorted. Leaving the redundant `sort_by` step doesn't break anything but wastes cycles and confuses readers.
- **Passing `Value` reference to `BTreeMap`**: `map.insert(key, anno)` moves the Value; can't borrow it before moving. Structure the code to consume the Vec by iteration.
- **Missing `spdxId` field**: defensive — the existing code uses `.as_str().unwrap_or("")` for the sort key. Keep the same pattern in the dedup helper. Malformed entries (no spdxId) collapse to a single empty-string-keyed entry — unchanged behavior from pre-166.
- **Golden byte-identity confusion**: SPDX 3 goldens MAY drift post-166 (per SC-006); CDX + SPDX 2.3 goldens MUST NOT drift (per SC-005). If a CDX or SPDX 2.3 golden changes, that's a bug.
