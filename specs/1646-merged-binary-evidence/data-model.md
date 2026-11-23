# Data model: spec 1646

Everything is in process, per scan.

## ResolutionTechnique (extended)

A new variant, `BinaryAnalysis`. It is emitted as `binary-analysis` in CycloneDX `evidence.identity[].methods[].technique` and in SPDX D1. It is set for entries whose `evidence_kind` is `embedded-version-string` or `symbol-fingerprint`, with confidence `PACKAGE_DB_CONFIDENCE` (0.85).

## ResolutionEvidence (extended)

| Field | Type | Meaning |
|---|---|---|
| `technique` | `ResolutionTechnique` | the winner's method (unchanged) |
| `confidence` | `f64` | the winner's confidence (unchanged) |
| `also` | `Vec<IdentityMethod>` | **new**: methods absorbed by merges whose technique differs from `technique`, one per technique, in variant order; empty unless a binary match took part in a merge |

`IdentityMethod { technique: ResolutionTechnique, confidence: f64 }`.

## Absorb rule (S1 and S2)

A side's **effective technique** is `BinaryAnalysis` when its `evidence_kind` is `embedded-version-string` or `symbol-fingerprint`, and its `technique` otherwise. Before T010 sets the technique, this is what makes a banner entry read as binary analysis, so the log-only census and the wired rule compute the same thing.

On merging `other` into `best`, `absorb_method` considers each technique in `{effective(other)} ∪ other.also`. It adds the technique to `best.also` only when all of these hold:
- either side is a binary side: its effective technique is `BinaryAnalysis`, or its `also` contains it;
- the technique differs from `effective(best)`;
- the technique is not already in `best.also`.

When `best` is itself the binary side and wins (its effective technique is `BinaryAnalysis`), the absorbed technique is the record's, for example `PackageDatabase`.

Otherwise nothing changes.

## Wire

| Format | Single method (most components) | Two or more methods |
|---|---|---|
| CycloneDX `evidence.identity[0].methods` | `[{technique, confidence}]`, unchanged except a binary match's technique | `[{primary}, {also…}]` |
| SPDX 2.3 / SPDX 3 D1 `evidence.identity` | `{"technique", "confidence"}`, unchanged except a binary match's technique | the same flat keys plus `"methods": [{primary}, {also…}]` |

The SPDX technique strings are `url-pattern`, `hash-match`, `package-database`, `local-cache-hit`, `file-path-pattern`, `hostname-heuristic`, and the new `binary-analysis`.
