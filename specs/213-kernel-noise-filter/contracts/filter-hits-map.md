# Contract: `FILTER_CATEGORY_HITS` kernel↔user PerCpuArray<u64>

**Feature**: 213-kernel-noise-filter
**Kind**: Kernel↔user map contract
**Consumers**: kernel-side classifier (write-side), userspace `counters::read_filter_category_hits` (read-side).

## Map declaration

```rust
// mikebom-ebpf/src/maps.rs
#[map]
pub static FILTER_CATEGORY_HITS: PerCpuArray<u64> = PerCpuArray::with_max_entries(4, 0);
```

- **Type**: `BPF_MAP_TYPE_PERCPU_ARRAY` (aya-ebpf `PerCpuArray`).
- **Key type**: `u32` (implicit for array maps; index only).
- **Value type**: `u64`.
- **Slot count**: `4` — one slot per `FilterCategoryTag::ALL` variant. See [filter-category-tag.md](./filter-category-tag.md).
- **Flags**: `0` (default; matches m212's `FILE_EVENT_DROPS` posture).

## Slot indexing

| Slot | `FilterCategoryTag` variant | Increment site (kernel)                    |
|------|-----------------------------|--------------------------------------------|
| 0    | `System`                    | `file_ops::path_matches_filter_category` when System pattern matches |
| 1    | `UserCache`                 | same, UserCache pattern match              |
| 2    | `Ephemeral`                 | same, Ephemeral pattern match              |
| 3    | `CargoFingerprint`          | same, CargoFingerprint pattern match       |

Slot index MUST equal the `FilterCategoryTag` discriminant `as u32`.

## Kernel-side write semantics

```rust
// Inside path_matches_filter_category (mikebom-ebpf/src/programs/file_ops.rs)
if let Some(cat) = matched_category {
    increment_filter_category_hit(cat as u8);
    return Some(cat);   // classifier returns Some → caller skips FILE_EVENTS.reserve
}
```

Helper:

```rust
// mikebom-ebpf/src/helpers.rs
#[inline(always)]
pub fn increment_filter_category_hit(cat: u8) {
    let idx = cat as u32;
    if let Some(counter) = FILTER_CATEGORY_HITS.get_ptr_mut(idx) {
        unsafe { *counter = (*counter).saturating_add(1); }
    }
}
```

- **Saturation**: `saturating_add(1)` matches m212's `increment_drop_counter` pattern verbatim. Saturation at u64::MAX is effectively impossible in a single trace.
- **Bounds**: `cat` is always < 4 because `FilterCategoryTag` has 4 variants; `get_ptr_mut(idx)` returns `None` for `idx >= max_entries` which the closure gracefully skips (no-op).
- **Zero cost when classifier returns None**: the helper is only called on the drop path, not on the pass-through path.

## Userspace read semantics

```rust
// mikebom-cli/src/trace/counters.rs
pub fn read_filter_category_hits(bpf: &mut aya::Ebpf) -> FilterCategoryHitsSummary {
    let mut per_category = BTreeMap::new();
    let mut attach_failures = Vec::new();
    match read_percpu_slot_sum(bpf, "FILTER_CATEGORY_HITS", 0) {
        Ok(v) => { per_category.insert(FilterCategoryTag::System, v); }
        Err(_) => attach_failures.push("filter_category_hits".to_string()),
    }
    // ... slots 1, 2, 3 similarly (or via ALL.iter())
    FilterCategoryHitsSummary { per_category, attach_failures }
}
```

- The `read_percpu_slot_sum` helper is a m213-side sibling to m212's `read_percpu_sum` at `counters.rs`: same aya `PerCpuArray::get(&idx, 0)` call, sum across `Vec<u64>` result (one entry per online CPU).
- Attach failure on ANY category slot registers `"filter_category_hits"` as a single entry in `attach_failures` (not one entry per slot) — the whole map either attached or it didn't.

## Failure semantics (R9)

- **Map fails to attach at load-time**: `bpf.map_mut("FILTER_CATEGORY_HITS")` returns `None`. Userspace surfaces `"filter_category_hits"` in `TraceIntegrity.kprobe_attach_failures[]`. The kernel side's `if let Some(counter) = FILTER_CATEGORY_HITS.get_ptr_mut(idx)` closure gracefully no-ops — the classifier still runs but the count is silently lost, so `filter_categories_applied[]` will be `[]`. Operators see the attach failure in `kprobe_attach_failures[]` and can diagnose.
- **Partial failure** (e.g., one slot read fails but others succeed): treat as full failure — either the map is usable end-to-end or it isn't. Any read error → all slots contribute 0 to the aggregate.
