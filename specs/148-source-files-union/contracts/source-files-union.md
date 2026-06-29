# Contract — `canonicalize_source_files_by_purl` pass

Phase 1 output. Defines the pure-function contract for the new post-dedup canonicalization pass.

## Function signature

```rust
/// Milestone 148: cross-PURL canonicalization of evidence.source_file_paths.
///
/// After the existing `deduplicate()` pass merges same-(ecosystem,name,version,
/// parent_purl)-key groups, some ecosystems (Maven nested-coord case at
/// `scan_fs/package_db/maven.rs:3429-3457`, Cargo workspace vendoring, Go
/// vendored modules) intentionally retain multiple `ResolvedComponent` instances
/// sharing the same `Purl::as_str()` value but differing in `parent_purl`. The
/// CDX nested-components topology depends on this two-entry shape.
///
/// Each entry carries its own `evidence.source_file_paths` Vec (one observed
/// path from the standalone reader pass, one observed path from the nested
/// reader pass). Per-emitter iteration-order differences (CDX `builder.rs:619`,
/// SPDX 2.3 `annotations.rs:302`, SPDX 3 `v3_annotations.rs:267`) cause the
/// audit harness to observe cross-format divergence on the `mikebom:source-files`
/// annotation for what the harness treats as the same PURL (51 polyglot-builder-
/// image findings, 2026-06-28 audit).
///
/// This pass, keyed on the full canonical `Purl::as_str()` string, replaces
/// each same-PURL entry's `source_file_paths` Vec with the alphabetically-
/// sorted UNION of paths observed across all same-PURL entries. After the
/// pass, every emitter sees the same Vec content for every same-PURL pair,
/// so the wire-side `mikebom:source-files` annotation is identical across
/// formats regardless of which entry the harness happens to pick.
///
/// **Idempotent** — running twice produces byte-identical output.
/// **Topology-preserving** — does NOT modify `parent_purl` or any other field.
/// **No-op for single-entry PURLs** — the common case is unchanged.
pub fn canonicalize_source_files_by_purl(components: &mut Vec<ResolvedComponent>);
```

## Contract requirements

| Requirement | Source spec | Test |
|---|---|---|
| Same-PURL multi-entry: all entries get the same alphabetically-sorted union Vec | FR-001 + FR-002 | SC-004 unit test (two entries) + SC-003 integration test (cross-format) |
| Single-entry PURL: no-op (byte-identical Vec pre/post) | FR-007 | Dedicated unit test in `deduplicator.rs#mod tests` |
| Keyed on full canonical PURL string (`Purl::as_str()`) | FR-003 | Edge Case 7 cross-ecosystem isolation unit test |
| Idempotent (two passes produce identical output) | FR-004 | SC-005 unit test |
| Preserves all other `ResolvedComponent` fields verbatim | FR-005 | SC-004 unit test asserts every named field unchanged |
| Preserves `parent_purl` specifically (topology intact) | FR-006 | SC-004 unit test |
| Preserves `evidence.source_connection_ids`, `evidence.hashes`, `evidence.technique`, `evidence.confidence`, `evidence.deps_dev_match` | FR-005 + Assumption 3 | SC-004 unit test |
| Three-or-more-entries case: full N-way union | Edge Case 1 | Optional N=3 unit test |
| Empty Vec on every same-PURL entry: empty union (annotation stays absent) | Edge Case 3 | Unit test |
| File-tier components: no-op (file_tier walker already aggregates) | Edge Case 5 + Out of Scope §8 | Implicit — file-tier PURLs are content-hash-unique, can't have multi-entry shape |
| No new Cargo dependencies | Assumption 5 | Cargo.toml diff review |
| No new `mikebom:*` annotation | FR-008 + Constitution V audit | docs/reference/sbom-format-mapping.md diff review |

## Algorithm sketch (illustrative; not normative)

```rust
use std::collections::{BTreeSet, HashMap};

pub fn canonicalize_source_files_by_purl(components: &mut Vec<ResolvedComponent>) {
    // Phase 1: collect — walk every component, accumulate paths by canonical PURL.
    let mut paths_by_purl: HashMap<String, BTreeSet<String>> = HashMap::new();
    for c in components.iter() {
        paths_by_purl
            .entry(c.purl.as_str().to_string())
            .or_default()
            .extend(c.evidence.source_file_paths.iter().cloned());
    }

    // Phase 2: write back — replace each entry's source_file_paths with the
    // alphabetically-sorted union for its PURL. Single-entry PURLs are a no-op
    // because the BTreeSet collected from one Vec roundtrips to an
    // equal-content Vec (modulo sort order; insertion-ordered single-entry
    // Vecs that happen to be already-sorted are byte-identical post-pass).
    for c in components.iter_mut() {
        if let Some(union) = paths_by_purl.get(c.purl.as_str()) {
            c.evidence.source_file_paths = union.iter().cloned().collect();
        }
    }
}
```

## Negative-space contract (what MUST NOT happen)

- The pass MUST NOT add new components.
- The pass MUST NOT remove components.
- The pass MUST NOT reorder components within the input Vec.
- The pass MUST NOT mutate `purl`, `name`, `version`, `parent_purl`, `hashes`, `lifecycle_scope`, `extra_annotations`, `sbom_tier`, `binary_role`, `binary_stripped`, `linkage_kind`, `confidence`, `requirement_range`, `source_type`, `co_owned_by`, `npm_role`, `raw_version`, `licenses`, `concluded_licenses`, `supplier`, `cpes`, `advisories`, `occurrences`, `buildinfo_status`, `evidence_kind`, `binary_class`, `binary_packed`, `detected_go`, `shade_relocation`, `build_inclusion`, or any other field on `ResolvedComponent`.
- The pass MUST NOT mutate `evidence.technique`, `evidence.confidence`, `evidence.source_connection_ids`, `evidence.deps_dev_match`. ONLY `evidence.source_file_paths` is touched.
- The pass MUST NOT introduce duplicate entries within the per-PURL union Vec (BTreeSet semantic guarantees uniqueness).
- The pass MUST NOT cross-pollinate paths across different ecosystems (FR-003 guards via `Purl::as_str()` keying — the ecosystem segment is part of the canonical string).

## Call-site contract

The function MUST be called from `mikebom-cli/src/scan_fs/mod.rs` immediately after the existing `let mut components = deduplicate(components);` call at line 750:

```rust
let mut components = deduplicate(components);
canonicalize_source_files_by_purl(&mut components);
// ... existing post-dedup passes (synthesize_cpes, etc.) ...
```

The placement is justified by research §A: no existing post-dedup pass touches `evidence.source_file_paths`, so ordering between the union pass and the subsequent CPE synthesis / scan-target-coord suppression / main-module tagging is non-load-bearing.

## Test surface

| Test | Asserts | Location |
|---|---|---|
| `canonicalize_source_files_by_purl_same_purl_different_parent_unions_paths_md148` | FR-001 + FR-002 + FR-006 | `deduplicator.rs#mod tests` |
| `canonicalize_source_files_by_purl_single_entry_is_noop_md148` | FR-007 | `deduplicator.rs#mod tests` |
| `canonicalize_source_files_by_purl_is_idempotent_md148` | FR-004 | `deduplicator.rs#mod tests` |
| `canonicalize_source_files_by_purl_preserves_other_fields_md148` | FR-005 | `deduplicator.rs#mod tests` (asserts every named field unchanged) |
| `canonicalize_source_files_by_purl_three_entries_full_union_md148` | Edge Case 1 | `deduplicator.rs#mod tests` |
| `canonicalize_source_files_by_purl_cross_ecosystem_isolation_md148` | Edge Case 7 + FR-003 | `deduplicator.rs#mod tests` |
| `same_purl_maven_nested_coord_emits_byte_identical_source_files_across_formats_md148` | SC-003 (cross-format invariance on synthetic Maven fixture) | `mikebom-cli/tests/source_files_purl_union_md148.rs` |
| Existing C18 parity-catalog row tests | SC-002 — `Directionality::SymmetricEqual` continues to hold | `mikebom-cli/tests/cross_format_byte_identity.rs`, `holistic_parity.rs` |
