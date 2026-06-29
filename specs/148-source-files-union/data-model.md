# Data Model — milestone 148

Phase 1 output. No new types introduced; this document describes the existing `ResolvedComponent.evidence.source_file_paths` semantic and the pre/post behavior table for affected components.

## Modified field

### `mikebom_common::resolution::ResolvedComponent.evidence.source_file_paths`

| Aspect | Value |
|---|---|
| Type | `Vec<String>` (unchanged) |
| Population sites (read-only context) | Artefact walker at `scan_fs/mod.rs:207`, Package-DB reader at `scan_fs/mod.rs:649`, and per-reader `PackageDbEntry::source_path` flows via `normalize_sbom_path_relative` (milestone 133 normalization pipeline) |
| **Existing** within-group merge | At `resolve/deduplicator.rs:74-78` — insertion-ordered `.contains()` dedupe within each `(ecosystem, name, version, parent_purl)` group (unchanged) |
| **New** cross-PURL union pass (milestone 148) | At `resolve/deduplicator.rs::canonicalize_source_files_by_purl` — alphabetically-sorted `BTreeSet<String>` union across all entries sharing a `Purl::as_str()` value (NEW) |
| Idempotence | Guaranteed by `BTreeSet` set-union semantics (research §C). Running the canonicalize pass twice on the same input produces byte-identical output. |
| Cross-ecosystem isolation | Guaranteed by `Purl::as_str()` keying — the canonical PURL string includes the ecosystem segment (research §D). |
| Wire surface | Consumed by all three emitters: CDX `cyclonedx/builder.rs:830-839` via `source_files_as_json_array`; SPDX 2.3 `spdx/annotations.rs:302-308` via `json!(c.evidence.source_file_paths)`; SPDX 3 `spdx/v3_annotations.rs:267-273` via `json!(c.evidence.source_file_paths)`. **No emitter-side change** in milestone 148. |

## New public function

### `canonicalize_source_files_by_purl(components: &mut Vec<ResolvedComponent>)`

| Aspect | Value |
|---|---|
| Location | `mikebom-cli/src/resolve/deduplicator.rs` (new sibling of `deduplicate`) |
| Visibility | `pub` (called from `scan_fs/mod.rs:751`) |
| Signature | `pub fn canonicalize_source_files_by_purl(components: &mut Vec<ResolvedComponent>)` |
| Return | `()` — mutates in place |
| Side effects | Replaces each entry's `evidence.source_file_paths` Vec with the alphabetically-sorted union of paths observed across all same-`Purl::as_str()` entries. No other field touched (FR-005). |
| Complexity | O(N · log K + N · P) where N = component count, K = unique-PURL count, P = avg paths per PURL. Dominant term in practice: O(N). |
| Allocations | Two `HashMap<&str, BTreeSet<String>>` (one for accumulating, one is the per-PURL Vec output via `.into_iter().collect()`). |
| Test fixture | `mikebom-cli/tests/fixtures/source_files_union/` — synthetic Maven nested-coord case per research §F. |

## Pre/post behavior table

| Component shape | Pre-148 `evidence.source_file_paths` | Post-148 `evidence.source_file_paths` |
|---|---|---|
| Single-entry-per-PURL (common case) | `["path/to/foo.jar"]` (within-group merge result, possibly multi-element if multiple readers detected the same path-context) | **Content-preserving no-op** per FR-007 — set of paths unchanged; wire-order MAY be canonicalized to alphabetical (matters only for multi-element single-entry Vecs whose pre-148 order was insertion-order, not alphabetical) |
| Same Maven coord, standalone + nested under fat-jar (two entries, different `parent_purl`) | Entry A: `["root/.m2/repo/.../foo.jar"]`; Entry B: `["tmp/extract/.../foo.jar"]` | Entry A: `["root/.m2/repo/.../foo.jar", "tmp/extract/.../foo.jar"]` (alphabetically sorted); Entry B: SAME value (alphabetically sorted union) |
| File-tier component (PURL = `pkg:generic/file-tier?content-sha256=<hex>`) | `["path/A", "path/B"]` (already aggregated via `file_tier/walker.rs::push_path`) | UNCHANGED — union-with-self is identity (file-tier components never have same-PURL multi-entry shape; the content-sha256-keyed PURL guarantees uniqueness per scan) |
| Same PURL with empty `source_file_paths` on every entry | `[]` (empty Vec on each entry) | UNCHANGED — empty union, every entry keeps the empty Vec |
| Three entries sharing a PURL (e.g., one standalone + two different fat-jar nestings) | Three different single-path Vecs | All three entries get the same alphabetically-sorted three-element Vec |

## Validation rules (consolidated from spec FRs)

| Input | Rule | Source |
|---|---|---|
| `components: Vec<ResolvedComponent>` | The pass MUST NOT add, remove, or reorder components. It only mutates `evidence.source_file_paths` on each entry. | FR-005 + FR-006 |
| Component with single-entry PURL | The *set* of paths MUST be unchanged pre/post the pass. Wire-order MAY canonicalize to alphabetical (FR-007 explicitly allows the BTreeSet sort). | FR-007 |
| Component shares PURL with N>1 other components | All N+1 components MUST have `source_file_paths` set to the alphabetically-sorted union of all observed paths across the N+1 entries. | FR-001 + FR-002 |
| PURL keying basis | `c.purl.as_str()` (full canonical PURL string) | FR-003 |
| Cross-ecosystem entries that share `(name, version)` but differ in ecosystem | MUST remain isolated — no path cross-pollination | FR-003 + Edge Case 7 |
| Idempotence | Two consecutive passes MUST yield byte-identical output | FR-004 |
| Fields other than `source_file_paths` | Every other field of `ResolvedComponent` MUST be preserved verbatim | FR-005 |
| `parent_purl` field | Specifically MUST NOT be modified — the dep-graph topology stays intact | FR-006 |
| `mikebom:*` annotation surface | NO new key introduced; existing `mikebom:source-files` value-content may change for affected components only | FR-008 |
| Cross-format byte-equivalence on emitted output | C18 parity-catalog row's `Directionality::SymmetricEqual` MUST hold on every byte-identity golden | FR-009 |
| Golden refresh scope | Maven-bearing goldens may experience `mikebom:source-files` value changes; NO unrelated drift permitted | FR-010 |

## Out of model

- **No new types**: no structs/enums/newtypes introduced.
- **No public API surface changes** outside the new `canonicalize_source_files_by_purl` function (additive only).
- **No call-site changes elsewhere**: emitters consume `c.evidence.source_file_paths` transparently via their existing reads.
- **No changes to `deduplicate()`** itself: the within-group merge stays unchanged (research §E).
- **No changes to any ecosystem reader**: Maven, Cargo, Go all stay as-is (FR-006 + Out of Scope §5).
- **No changes to any emitter**: CDX/SPDX 2.3/SPDX 3 stay as-is.
- **No changes to `evidence.source_connection_ids`, `evidence.hashes`, or any other field** (Out of Scope §6 + Assumption 3).
