# Data Model: Kernel-side trace-noise filter

**Feature**: 213-kernel-noise-filter
**Date**: 2026-07-21

Three-crate breakdown, matching the m212 layout convention.

## E1 — `FilterCategoryTag` (mikebom-common, kernel↔user boundary type)

**Location**: `mikebom-common/src/events.rs`
**Purpose**: u8-repr discriminant for the four filter categories, transported across the kernel↔user boundary via the `FILTER_CATEGORY_HITS` per-CPU array's slot index.

```rust
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum FilterCategoryTag {
    System = 0,
    UserCache = 1,
    Ephemeral = 2,
    CargoFingerprint = 3,
}

impl FilterCategoryTag {
    /// The 4 variants, in enum-discriminant order. Used by the userspace
    /// aggregator to iterate slots of the FILTER_CATEGORY_HITS map.
    pub const ALL: [FilterCategoryTag; 4] = [
        Self::System,
        Self::UserCache,
        Self::Ephemeral,
        Self::CargoFingerprint,
    ];

    /// Human-readable name emitted in TraceIntegrity.filter_categories_applied[].
    /// Values match the userspace ClassifyFilterCategory enum names per FR-007.
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::UserCache => "UserCache",
            Self::Ephemeral => "Ephemeral",
            Self::CargoFingerprint => "CargoFingerprint",
        }
    }
}

impl TryFrom<u8> for FilterCategoryTag {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(Self::System),
            1 => Ok(Self::UserCache),
            2 => Ok(Self::Ephemeral),
            3 => Ok(Self::CargoFingerprint),
            _ => Err(v),
        }
    }
}
```

**Invariants**:
- Discriminants 0-3 are STABLE. Adding a new category MUST use discriminant 4+; renumbering breaks kernel↔user compatibility.
- `name()` string values are pinned by FR-007 and MUST match the userspace `ClassifyFilterCategory` enum variant names verbatim.
- `TryFrom<u8>` on unknown discriminants returns `Err(u8)` so userspace can log the unknown value rather than silently panic.

## E2 — `FILTER_CATEGORY_HITS` (mikebom-ebpf, kernel-side counter map)

**Location**: `mikebom-ebpf/src/maps.rs`
**Purpose**: Per-CPU u64 counters, one slot per `FilterCategoryTag` variant, incremented every time the kernel-side classifier matches an open-syscall path to that category (and thus drops the event).

```rust
#[map]
pub static FILTER_CATEGORY_HITS: PerCpuArray<u64> = PerCpuArray::with_max_entries(4, 0);
```

**Invariants**:
- Slot count = 4 (matches `FilterCategoryTag::ALL.len()`). If a 5th category is added, this MUST be updated to 5 in the same PR.
- Each slot's per-CPU value is a monotonic u64 counter for the trace lifetime; on next trace-start the map handle is dropped and a new one allocated.
- Overflow at u64::MAX is effectively impossible (2^64 opens per CPU per trace).

## E3 — `FILTER_WIDEN` (mikebom-ebpf, kernel-side config map)

**Location**: `mikebom-ebpf/src/maps.rs`
**Purpose**: Per-CPU u8 flag, single slot. Written once at loader time; read at every open by the classifier to gate the System-category compare.

```rust
#[map]
pub static FILTER_WIDEN: PerCpuArray<u8> = PerCpuArray::with_max_entries(1, 0);
```

**Semantics**:
- Value `0` = filter fully active (default; matches `ScanArgs.include_system_reads == false`).
- Value `1` = System category disabled; UserCache/Ephemeral/CargoFingerprint remain active (matches `ScanArgs.include_system_reads == true`).
- Non-zero non-1 values MUST be treated as `1` by the classifier (fail-open widening on unknown config).

**Invariants**:
- Written once per trace at loader time by `mikebom-cli/src/trace/loader.rs`. Not written per-event.
- Per-CPU semantics mean each CPU has its own copy of the flag — no cross-CPU synchronization overhead on read.

## E4 — `FilterCategoryHitsSummary` (mikebom-cli, userspace aggregate)

**Location**: `mikebom-cli/src/trace/counters.rs`
**Purpose**: Struct returned by `read_filter_category_hits(bpf)` — parallel to m212's `RingBufferDropsSummary`.

```rust
#[derive(Debug, Default, Clone)]
pub struct FilterCategoryHitsSummary {
    pub per_category: BTreeMap<FilterCategoryTag, u64>,
    pub attach_failures: Vec<String>,
}

impl FilterCategoryHitsSummary {
    /// Names of categories with count > 0, sorted lexicographically for
    /// wire-shape stability per FR-006.
    pub fn applied_categories(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .per_category
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(cat, _)| cat.name().to_string())
            .collect();
        names.sort();
        names.dedup();
        names
    }
}
```

**Invariants**:
- `per_category` uses `BTreeMap` (not `HashMap`) so iteration order is stable — determinism matters for wire-shape byte-identity testing.
- `applied_categories()` result is ALWAYS sorted-deduplicated, even if the underlying map somehow contains duplicates (defensive — dedup on a sorted vec is O(n)).
- `attach_failures` contains category-map-attach failures only (`"filter_category_hits"`, `"filter_widen"`) — surfaced via `TraceIntegrity.kprobe_attach_failures[]` per R9.

## E5 — `TraceIntegrity.filter_categories_applied` (mikebom-common, on-wire field)

**Location**: `mikebom-common/src/attestation/integrity.rs`
**Purpose**: New additive field on the existing `TraceIntegrity` struct. Emitted in every attestation.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceIntegrity {
    // ... existing fields (ring_buffer_overflows, events_dropped, kprobe_attach_failures, ...)
    #[serde(default)]
    pub filter_categories_applied: Vec<String>,
}
```

**Invariants**:
- Field placement: at end of struct so pre-m213 attestations round-trip byte-identically (except for the added trailing key).
- `#[serde(default)]` gives back-compat: pre-m213 attestations deserialize with `filter_categories_applied = vec![]`.
- On serialization, empty state produces `[]` (never omitted, never null) per FR-009. The default `Vec<String>` serialization achieves this — no `serde(skip_serializing_if)` attribute.
- Values MUST be drawn from the closed set `{"System", "UserCache", "Ephemeral", "CargoFingerprint"}` per FR-007.
- Values MUST be sorted-deduplicated per FR-006 (guaranteed by `FilterCategoryHitsSummary::applied_categories`).

## Cross-crate relationships

```text
[mikebom-ebpf]                            [mikebom-cli]                           [mikebom-common]
──────────────                            ────────────                            ──────────────

FILTER_CATEGORY_HITS (E2)  ──── read at trace-end ────>  FilterCategoryHitsSummary (E4)
                                                              │
                                                              │ applied_categories()
                                                              ▼
FILTER_WIDEN (E3)          <── written at load ─── loader.rs::load()               TraceIntegrity (E5)
                                                                                        ▲
                                                                                        │
path_matches_filter_category ──── uses ────>  FilterCategoryTag (E1)  ────>  filter_categories_applied[]
     │                                                                             (emitted at trace end)
     │ increment_filter_category_hit(cat.into())
     ▼
FILTER_CATEGORY_HITS[cat] += 1
```

## State transitions (per-scan lifecycle)

1. **Load** (loader.rs): eBPF programs load, `FILTER_CATEGORY_HITS` + `FILTER_WIDEN` maps attach. Loader writes `FILTER_WIDEN[0] = if include_system_reads { 1 } else { 0 }`. If any map fails to attach, entry added to `TraceIntegrity.kprobe_attach_failures[]` per R9.
2. **Trace** (per-open): `try_do_filp_open` / `try_openat2` call `path_matches_filter_category(&path)`. If `Some(cat)`, increment `FILTER_CATEGORY_HITS[cat as usize]` and return early — no `FILE_EVENTS.reserve()`. If `None`, proceed as pre-m213 (reserve → submit or increment `FILE_EVENT_DROPS`).
3. **Trace-end** (scan.rs::execute_scan): call `counters::read_filter_category_hits(bpf)`, populate `TraceIntegrity.filter_categories_applied` via `.applied_categories()`, append any `attach_failures` to `TraceIntegrity.kprobe_attach_failures[]`.
4. **Emit** (attestation builder): `TraceIntegrity` serializes to JSON with the new `filter_categories_applied[]` field at struct end. Empty state serializes as `[]` per FR-009.
