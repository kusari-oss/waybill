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

On merging `other` into `best`, `absorb_method` considers each technique in `{other.technique} ∪ other.also`. It adds the technique to `best.also` only when all of these hold:
- `BinaryAnalysis` is among `best.technique`, `best.also`, `other.technique` and `other.also`;
- the technique differs from `best.technique`;
- the technique is not already in `best.also`.

Otherwise nothing changes.

## Wire

| Format | Single method (most components) | Two or more methods |
|---|---|---|
| CycloneDX `evidence.identity[0].methods` | `[{technique, confidence}]`, unchanged except a binary match's technique | `[{primary}, {also…}]` |
| SPDX 2.3 / SPDX 3 D1 `evidence.identity` | `{"technique", "confidence"}`, unchanged except a binary match's technique | the same flat keys plus `"methods": [{primary}, {also…}]` |

The SPDX technique strings are `url-pattern`, `hash-match`, `package-database`, `local-cache-hit`, `file-path-pattern`, `hostname-heuristic`, and the new `binary-analysis`.
