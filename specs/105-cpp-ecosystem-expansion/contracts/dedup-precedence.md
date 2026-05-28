# Contract: Cross-reader dedup precedence pipeline (FR-015)

**Maps to**: FR-015, SC-010 | **New module**: `mikebom-cli/src/scan_fs/dedup.rs`

## Position in the pipeline

```
[ readers ] → Vec<DetectionRecord> → [ dedup.rs ] → DedupResult → [ SBOM emitter ]
```

Each reader's output (existing `Vec<PackageDbEntry>` per-reader convention) is
wrapped in a `DetectionRecord` (see `data-model.md`) that pairs each entry
with its source-mechanism. All records are collected into a single
`Vec<DetectionRecord>`, sorted by canonical-PURL string (lexicographic, for
determinism), and fed into the dedup pipeline.

## Algorithm

```rust
pub fn dedup(records: Vec<DetectionRecord>) -> DedupResult {
    // 1. Sort by canonical_purl (lex), then by deterministic tie-break.
    records.sort_by(|a, b| {
        a.canonical_purl.cmp(&b.canonical_purl)
            .then_with(|| precedence_rank(&a).cmp(&precedence_rank(&b)))
            .then_with(|| a.source_mechanism.discriminant_str().cmp(b.source_mechanism.discriminant_str()))
    });

    // 2. Group by canonical_purl.
    let groups = records.into_iter().chunk_by(|r| r.canonical_purl.clone());

    // 3. Within each group, the lowest-ranked (highest-priority) record is the winner.
    //    The remaining records' source-mechanisms become the `also_detected_via` list.
    DedupResult {
        winners: groups.map(|(purl, group)| {
            let mut group: Vec<_> = group.collect();
            let winner = group.remove(0); // already sorted; first is highest priority
            let mut losers: Vec<SourceMechanism> = group.into_iter().map(|r| r.source_mechanism).collect();
            losers.sort_by_key(|sm| sm.canonical_str()); // lexicographic per FR-015

            DedupedComponent {
                canonical_purl: purl,
                winning_source_mechanism: winner.source_mechanism,
                winning_reader_output: winner.reader_output,
                also_detected_via: losers,
            }
        }).collect(),
    }
}
```

The `precedence_rank` function returns a `u8` derived from the two-stage table
in `data-model.md` (Stage 1 tier + Stage 2 PURL specificity). Smaller rank
wins.

## Determinism guarantees (SC-010)

- Sorting is total (canonical-PURL → precedence-rank → source-mechanism discriminant string). No floating point, no `HashMap` iteration order.
- Filesystem walk order has been removed from the inputs by the initial sort.
- A dedicated unit test shuffles a fixed `Vec<DetectionRecord>` (using a seeded RNG) 100 times and asserts the same `DedupResult` byte output across all 100 runs.
- An integration test (`tests/dedup_precedence_determinism.rs`) runs the same `golden_inputs/dedup_collision/` fixture from a randomized walk order via two distinct codepath orderings and asserts byte-identity.

## CDX hybrid emission (per R1)

Each `DedupedComponent` translates to a CDX component whose `evidence.identity[0].methods[]` carries:

```json
[
  {"technique": "manifest-analysis", "confidence": 0.95, "mikebom-source-mechanism": "<winner>"},
  {"technique": "manifest-analysis", "confidence": 0.85, "mikebom-source-mechanism": "<loser-1>"},
  {"technique": "manifest-analysis", "confidence": 0.85, "mikebom-source-mechanism": "<loser-2>"}
]
```

The first entry's confidence is 0.95 (winner); losers are 0.85. The
`mikebom-source-mechanism` field is added additively to existing
`{technique, confidence}` blocks per R7.

SPDX 2.3 and SPDX 3.0.1 emit the parity-bridging `mikebom:also-detected-via`
annotation (FR-015) carrying the lexicographically-sorted losers list.

## Parity-extractor contract (C56)

```rust
// cdx.rs
pub fn c56_cdx(component: &Value) -> BTreeSet<String> {
    component
        .pointer("/evidence/identity/0/methods")
        .and_then(|v| v.as_array())
        .map(|methods| {
            // Skip the winning method (first entry); collect losers.
            methods.iter().skip(1)
                .filter_map(|m| m.get("mikebom-source-mechanism").and_then(|v| v.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

// spdx2.rs / spdx3.rs
pub fn c56_spdx23(package: &Value) -> BTreeSet<String> {
    extract_mikebom_annotation_values(package, "mikebom:also-detected-via")
        .into_iter()
        .filter_map(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .flatten()
        .collect()
}
// (spdx3 is structurally identical.)
```

`Directionality::SymmetricEqual` invariant holds because both extractors
produce the same `BTreeSet<String>` of losing source-mechanism values.
