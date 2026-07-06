# Phase 1 Data Model: m170 Graph-Completeness Dedup

**Feature**: 170-graph-completeness-dedup
**Date**: 2026-07-06

Three entities change shape between pre-m170 and post-m170. Each is documented as a before/after pair with the transition semantics that mediate the change.

## Entity 1 — `SbomEmission` struct

**Location**: `mikebom-cli/src/generate/mod.rs`

### Pre-m170 shape

```rust
pub struct SbomEmission<'a> {
    // …existing fields (omitted for brevity)

    /// Milestone 061 (closes #119): doc-level Go graph-completeness
    /// signal. `None` when no Go scan produced a completeness verdict.
    pub go_graph_completeness:
        Option<crate::scan_fs::package_db::GraphCompleteness>,

    /// Optional reason string carried when `go_graph_completeness ==
    /// Some(Partial)` — free-form list summarizing why the Go graph is
    /// partial per m061 conventions.
    pub go_graph_completeness_reason: Option<&'a str>,

    /// Milestone 160 (T034/T035): distinct doc-scope Go-transitive
    /// coverage signal. Distinct from `go_graph_completeness` per
    /// research.md R1.
    pub go_transitive_coverage:
        Option<&'a GoTransitiveCoverage>,

    // …remaining fields
}
```

### Post-m170 shape

```rust
pub struct SbomEmission<'a> {
    // …existing fields (unchanged)

    /// Milestone 160 (T034/T035): doc-scope Go-transitive coverage
    /// signal, the canonical home for the "did Go transitive edges
    /// resolve?" question post-m170.
    pub go_transitive_coverage:
        Option<&'a GoTransitiveCoverage>,

    // …remaining fields (unchanged)
}
```

**Deltas**:
- `go_graph_completeness` field: **removed**
- `go_graph_completeness_reason` field: **removed**
- All other fields: unchanged

**Blast radius** (call sites needing update):
- `mikebom-cli/src/cli/scan_cmd.rs:1975-1976, 2616-2617` (construction site)
- `mikebom-cli/src/generate/openvex/mod.rs:246-247` (`None`-stub)
- `mikebom-cli/src/generate/spdx/mod.rs:388-389` (`None`-stub)
- `mikebom-cli/src/generate/spdx/packages.rs:724-725` (test-harness stub)
- `mikebom-cli/src/generate/spdx/relationships.rs:345-346` (test-harness stub)
- `mikebom-cli/src/generate/spdx/document.rs:462-463, 492-493, 1169-1170` (2 construction + 1 test stub)
- `mikebom-cli/src/generate/spdx/v3_document.rs:99-100` (threading site)

Total: 8 files besides `mod.rs` itself.

## Entity 2 — `EXTRACTORS` const slice

**Location**: `mikebom-cli/src/parity/extractors/mod.rs`

### Pre-m170 shape

Length: 116 rows. Contains a duplicate `label = "mikebom:graph-completeness"` collision between:

```rust
// Row at mod.rs:256
ParityExtractor { row_id: "C44",  label: "mikebom:graph-completeness", cdx: c44_cdx,  spdx23: c44_spdx23,  spdx3: c44_spdx3,  directional: Directionality::SymmetricEqual, order_sensitive: false },

// Row at mod.rs:451
ParityExtractor { row_id: "C104", label: "mikebom:graph-completeness", cdx: c104_cdx, spdx23: c104_spdx23, spdx3: c104_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
```

### Post-m170 shape

Length: 115 rows. The C44 row is deleted. C104 stays as the sole owner of `label = "mikebom:graph-completeness"`.

**Deltas**:
- Row `C44 mikebom:graph-completeness`: **removed**
- All other rows: unchanged

**Import cleanup** (mirroring the m169 C116-addition pattern in reverse):
- `mikebom-cli/src/parity/extractors/mod.rs:62` — drop `c44_cdx` from imports
- `mikebom-cli/src/parity/extractors/mod.rs:73` — drop `c44_spdx23`
- `mikebom-cli/src/parity/extractors/mod.rs:84` — drop `c44_spdx3`

**Extractor helper cleanup**:
- `mikebom-cli/src/parity/extractors/cdx.rs` — drop the `cdx_property_values`-based `c44_cdx` helper (spans ~9 lines around line 538)
- `mikebom-cli/src/parity/extractors/spdx2.rs` — drop the `c44_spdx23` helper
- `mikebom-cli/src/parity/extractors/spdx3.rs` — drop the `c44_spdx3` helper

### Invariant (post-m170)

**Uniqueness invariant**: for every pair `(a, b)` in EXTRACTORS with `a != b`, `a.label != b.label`.

**Enforcement**: new unit test `mikebom-cli/src/parity/extractors/mod.rs::tests::extractors_have_unique_labels`. Walks EXTRACTORS, builds `HashMap<&str, Vec<&str>>` from label → row_ids, panics if any entry has `.len() > 1` with a message naming the collision.

## Entity 3 — Emitted document-scope annotations

### Pre-m170 shape (CDX 1.6 example, from `golang.cdx.json:902-947`)

```json
"properties": [
  { "name": "mikebom:generation-context",   "value": "filesystem-scan" },
  { "name": "mikebom:os-release-missing-fields", "value": "ID,VERSION_ID" },
  { "name": "mikebom:graph-completeness",   "value": "partial" },       // ← C44 (m061), Site 1
  { "name": "mikebom:trace-integrity-ring-buffer-overflows",         "value": "0" },
  { "name": "mikebom:trace-integrity-events-dropped",                "value": "0" },
  { "name": "mikebom:trace-integrity-uprobe-attach-failures",        "value": "0" },
  { "name": "mikebom:trace-integrity-kprobe-attach-failures",        "value": "0" },
  { "name": "mikebom:graph-completeness",   "value": "partial" },       // ← C104 (m158), Site 2
  { "name": "mikebom:graph-completeness-reason", "value": "orphaned-components-detected: 1 component(s) not reachable from root" },
  { "name": "mikebom:go-transitive-coverage",       "value": "unknown" },
  { "name": "mikebom:go-transitive-coverage-reason", "value": "offline-mode: transitive edges from proxy fetches unavailable" }
]
```

Two `mikebom:graph-completeness` entries at different indices. Order determined by emission-site sequence in `metadata.rs`.

### Post-m170 shape

```json
"properties": [
  { "name": "mikebom:generation-context",   "value": "filesystem-scan" },
  { "name": "mikebom:os-release-missing-fields", "value": "ID,VERSION_ID" },
  { "name": "mikebom:trace-integrity-ring-buffer-overflows",         "value": "0" },
  { "name": "mikebom:trace-integrity-events-dropped",                "value": "0" },
  { "name": "mikebom:trace-integrity-uprobe-attach-failures",        "value": "0" },
  { "name": "mikebom:trace-integrity-kprobe-attach-failures",        "value": "0" },
  { "name": "mikebom:graph-completeness",   "value": "partial" },       // ← C104 (m158), sole owner
  { "name": "mikebom:graph-completeness-reason", "value": "orphaned-components-detected: 1 component(s) not reachable from root" },
  { "name": "mikebom:go-transitive-coverage",       "value": "unknown" },
  { "name": "mikebom:go-transitive-coverage-reason", "value": "offline-mode: transitive edges from proxy fetches unavailable" }
]
```

Single `mikebom:graph-completeness` entry. `mikebom:graph-completeness-reason` follows it immediately (unchanged position because both C44 and C104 emissions were consecutive in the CDX metadata builder).

**Deltas**:
- Line count: -4 lines (the removed C44 emission's 4-line JSON object).
- Semantic: consumer's `.properties[] | select(.name == "mikebom:graph-completeness") | .value` returns exactly one value — always the universal reachability verdict from m158.

### SPDX 2.3 shape (analogous)

Pre-m170: two `annotations[]` entries with envelope-decoded `field == "mikebom:graph-completeness"`. Post-m170: one entry.

### SPDX 3.0.1 shape (analogous)

Pre-m170: two `@graph[]` typed Annotation elements targeting the SpdxDocument root IRI with `statement.field == "mikebom:graph-completeness"`. Post-m170: one element.

## Cross-entity invariants (post-m170)

1. `EXTRACTORS.iter().map(|e| e.label).collect::<HashSet<_>>().len() == EXTRACTORS.len()` — enforced by the new unit test.
2. For any emitted CDX SBOM: `jq '[.properties[] | select(.name == "mikebom:graph-completeness")] | length == 1'` — enforced by golden diff.
3. For any emitted SPDX 2.3 SBOM: the same uniqueness after envelope decoding — enforced by golden diff.
4. For any emitted SPDX 3.0.1 SBOM: the same uniqueness across the `@graph[]` Annotation elements — enforced by golden diff.

## State transitions

None. This is a stateless emission-code refactor. No lifecycle changes on any of the three entities.
