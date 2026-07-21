# Contract: `FilterCategoryTag` u8 discriminant → wire name mapping

**Feature**: 213-kernel-noise-filter
**Kind**: Wire-name contract (kernel u8 discriminant ↔ userspace JSON string)
**Consumers**: kernel-side `path_matches_filter_category` (write-side), userspace `counters::read_filter_category_hits` (read-side + name-emit), any downstream `jq` operator consuming `TraceIntegrity.filter_categories_applied[]`.

## Discriminant table (PINNED)

| Discriminant (u8) | Enum variant       | Wire JSON string    | Pattern examples                            |
|---|---|---|---|
| `0`             | `System`           | `"System"`          | `/etc/`, `/proc/`, `/sys/`, `/dev/`         |
| `1`             | `UserCache`        | `"UserCache"`       | any path containing `/.cache/` or `/.local/share/` as a dir component |
| `2`             | `Ephemeral`        | `"Ephemeral"`       | `/tmp/`, `/var/tmp/`                        |
| `3`             | `CargoFingerprint` | `"CargoFingerprint"`| any path containing `/fingerprint/`, `/deps/`, or `/incremental/` beneath a `target/` ancestor |

## Stability guarantees

- **Discriminants 0–3 are PERMANENT.** Renumbering breaks kernel↔user compatibility for any trace running a mismatched kernel-side + userspace-side pair.
- **Wire JSON strings are PERMANENT.** They match the userspace `mikebom-cli/src/trace/compiler_pipeline.rs::FilterCategory` enum variant names verbatim per FR-007. Extractor tooling MAY join across `trace_integrity.filter_categories_applied[]` and `compiler_pipeline.filter_categories_applied[]` with byte-identity comparison.
- **Unknown discriminants** (4+ from a future kernel-side extension, read by an older userspace) MUST be handled by `TryFrom<u8>` returning `Err(u8)`. Userspace MUST log the unknown value at WARN and skip that slot in the aggregation — no panic, no silent success.

## Adding a new category (v2 procedure)

1. Add the new enum variant at discriminant 4+ in `FilterCategoryTag`.
2. Extend `FilterCategoryTag::ALL` and `FilterCategoryTag::name` — 4 line change.
3. Bump `FILTER_CATEGORY_HITS` map's `max_entries` from 4 → 5.
4. Add the new pattern set to `path_matches_filter_category` in `file_ops.rs`.
5. Extend the userspace aggregator's iteration (auto-derived from `FilterCategoryTag::ALL`).
6. Extend `scripts/ebpf-integration-test.sh` assertions if the new category has a well-known noise signature on the SC-001 fixture.
7. Update this contract document with the new row.

Steps 1–5 are one PR; step 6 is one PR; step 7 is co-committed with step 1.
