# Data model: deps.dev enrichment outcomes

## Outcome (per lookup)

```
enum Outcome {
    Matched,                         // record returned; emits nothing
    Absent,                          // 404, or batch item without `version`
    Declined,                        // ≥1 licence string, all failed canonicalisation
    TransportFailure,                // per-key Err (incl. 429), or batch slot unanswered after fallback
    NotQueried(IncompleteCoordinate),// empty or placeholder version: no request sent
}
```

- **The fetch result changes.** `fetch_many` returns one `LookupResult` per key in place of `Option<VersionInfo>`, distinguishing `Found(VersionInfo)`, `Absent` and `Failed`. Classification into `Matched` or `Declined` happens in `apply_version_info`.
- **Unsupported ecosystems** have no lookup at all. They are counted at emission from the component's PURL type.

## Batch slot state (FR-010)

`fetch_chunk_batched` keeps one slot per *distinct* coordinate:
- `Answered(Some(v))`;
- `Answered(None)` → `Absent`;
- `Unanswered` → per-key fallback, neither cached nor recorded as absent.

Duplicate positions share a slot.

## On the component

`extra_annotations["waybill:deps-dev-outcome"]` (C191) is set per pass; a `Matched` result removes it. The final pass wins (research R5).

## Document count (C192)

Computed at emission over the final components:
- C191 values, counted;
- plus components whose PURL type is outside the six ecosystems, which count as `not-queried:unsupported-ecosystem`.

Emitted only if the scan's deps.dev pass ran online.

## Placeholder predicate

`is_placeholder_version(v)`: case-insensitive `v` ∈ {`""`, `unknown`, `0.0.0-unknown`, `v0.0.0-unknown`, `noassertion`, `none`, `latest`}.
