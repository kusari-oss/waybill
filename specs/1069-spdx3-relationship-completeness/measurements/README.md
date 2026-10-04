# Measurements for #878 (m1069)

## Relationship shape (`relationship_shape.sh` → `relationship_shape.txt`)
In all 17 public-corpus SPDX 3 goldens, every `dependsOn` relationship has exactly one target (`max-to-len=1`). Most components with dependencies have several such relationships, for example opentelemetry-go 30 of 30, haskell-language-server 285 of 327, and ripgrep 25 of 39. No relationship carries `completeness`.

## Validator (`probe_validator.py` → `validator.txt`)
Pinned `spdx3-validate` 0.0.5, on a real unmasked SPDX 3 document (cobra, `--offline`).

| variant | result |
|---|---|
| baseline | pass |
| grouped (one relationship per `from`/type/scope, all targets) | pass |
| grouped plus `completeness: complete`, `incomplete` or `noAssertion` | pass, each |
| grouped plus `completeness: bogus` (control) | **fail**: the enum is really checked |
| `dependsOn → NoAssertionElement` (bare term, and full IRI) | pass, each |

Committed goldens cannot be the baseline: they mask document IRIs and timestamps, and the validator rejects both.

## JSON-LD context (`https://spdx.org/rdf/3.0.1/spdx-context.jsonld`)
- `to` is `@type: @vocab`, and the context maps the term `NoAssertionElement` to `https://spdx.org/rdf/3.0.1/terms/Core/NoAssertionElement`. The bare term therefore is the SPDX individual, not a relative IRI.
- `completeness` is `@type: @vocab` under the `RelationshipCompleteness/` vocabulary.

## Model text (spdx.github.io/spdx-spec/v3.0.1)
- `RelationshipCompleteness` defines `complete` ("known to be exhaustive"), `incomplete` ("known not to be exhaustive") and `noAssertion` ("no assertion can be made").
- **The model states no default for an absent `completeness`.** Treating absence as "no claim" rests on RDF's open-world reading, not on explicit text.
- `NoAssertionElement` is for when "the SPDX creator has attempted to but cannot reach a reasonable objective determination", which is the unknown-leaf case.

## CycloneDX claims the SPDX 3 output must agree with (`cdx_claims.txt`)
Per corpus target, the `compositions[]` records carrying `dependencies` (the trailing `complete=1` is the scan root). All four FR-003 cases occur:
- per-ecosystem `complete`, for example ripgrep 61 and image-postgres16 142;
- `unknown`, for example opentelemetry-go 333 and flask 108;
- the root record;
- unclaimed ecosystems: the haskell targets and nix-closure-moat carry only the root record.

A local `--offline` scan of cobra resolves fully (warm Go module cache): `complete` 8, no `unknown`. The cold CI golden is the degraded case the issue describes: `unknown` 8.
