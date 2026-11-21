# Data model: Record which readers found a merged component (C56)

All state is in the per-component annotation map (`BTreeMap<String, serde_json::Value>`), shared by `PackageDbEntry` and `ResolvedComponent`. Nothing persists beyond the emitted document.

## Annotations

| Key | Row | Type | Present when |
|---|---|---|---|
| `waybill:source-mechanism` | C55 | string | as today; unchanged by this feature (FR-003) |
| `waybill:also-detected-via` | C56 | JSON array of strings, sorted, unique | the component absorbed at least one member at a site in research R3 **and** carries C55 **and** at least one absorbed C55 differs from its own (FR-001, research R5) |
| `waybill:fingerprint-also-matched` | C242 (new) | JSON array of strings, sorted, unique | the symbol-fingerprint matcher matched this binary against more than one record (FR-007) |

## C56 rules

1. **Union.** `record(site, subject, survivor, absorbed)` adds `absorbed[C55]` and every value of `absorbed[C56]` to the survivor's set.
2. **Exclude self.** The survivor's own C55 is never in its C56.
3. **Anchor.** No C55 on the survivor means no C56 (research R5).
4. **Empty means absent.**
5. **Order.** Values are sorted lexicographically by byte. They are deduplicated.
6. **Survivor C55.** It is set only by S1's existing fold. `record` runs after that fold, per loser.

## Lifecycle

```text
reader output ──► S4 R5 ─► S5 R4 ─► S7 vendored ─► S6 R9          (CMake reader, PackageDbEntry)
                                                     │
scan_fs ─────► S8/S9 cmake_search_pair ─► S1/S2 deduplicate ─► S3 reconciler   (ResolvedComponent)
                                                     │
                                                emitters: C56 written generically as a property (CDX)
                                                and as an annotation (SPDX 2.3, SPDX 3)
```

Each arrow that absorbs a component calls `record`. C56 travels with its component through every later pass, including `--split` projection, unchanged.

## Removed

- `scan_fs/dedup.rs`: `SourceMechanism`, `DetectionRecord`, `precedence_rank`, `dedup`. They are unused outside the file (FR-008). T001's census uses `precedence_rank` once, before its removal (research R2).
- The `build_evidence` parameters `winning_source_mechanism` and `losing_source_mechanisms`, and the `waybill-source-mechanism` method key (FR-005).
