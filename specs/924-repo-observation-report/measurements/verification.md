# Verification record — milestone 924 (#932)

## T031 — the schema gate has teeth

A schema gate that stubs `$ref` resolution validates nothing while reporting
green. This repository has shipped that failure before, so SC-006's test is
not trusted on a passing run alone: the **failing** direction was observed.

Both mutations were applied to the emitting code, run, and reverted.

| mutation | result |
|---|---|
| emit a field absent from the schema | **FAILED** — `Additional properties are not allowed ('undocumented_teeth_probe' was unexpected)` |
| emit an enum value outside the schema's set | **FAILED** — `"totally_made_up" is not one of "claimed", "unclaimed" or "excluded_by_policy"` |
| restored | 7 passed |

The first mutation is the one that matters for SC-006's second half — "the
schema describes every field emitted". `additionalProperties: false` on every
object is what makes that checkable rather than aspirational: a field added to
the report without a matching schema entry fails here instead of drifting
quietly.

## T022a — the FR-010 guard

FR-010 forbids this feature widening the file-tier `SourceShape` allowlist,
which governs SBOM emission. It is a **negative** requirement with no other
enforcement — nothing else in the suite fails if the allowlist grows. The test
pins the variant count at 21 and names the consequence in its failure message.

## T018 — the ecosystem table cannot rot silently

Proven by mutation: adding `Cargo.toml` to `ecosystems.data` fails with

```
ecosystems.data is STALE — a reader now claims these markers, so the table is
telling maintainers a closed gap is still open. Delete the offending lines:
  Cargo.toml (cargo-but-we-do-support-this) is claimed by ["cargo"]
```

## SC-007 — two earlier versions passed without exercising anything

Recorded because the pattern is the point, not the specific bug.

| version | fixture | why it passed |
|---|---|---|
| 1 | no Go module at all | the network-capable path never ran |
| 2 | Go module already in the local module cache | the resolver short-circuited before the proxy tier |
| 3 (current) | uncached module, `GOMODCACHE` empty | the proxy tier is the only option left |

Version 3 additionally asserts a **wall-clock bound**: a run against an
unroutable proxy that takes longer than 3s attempted a fetch and waited for
the failure. Measured control on the same fixture — `sbom scan`, which does
not force offline — takes **5.196s** and logs `connection refused`; the report
path takes **0.039s** with `proxy_count=0`.

**A control is what made the defect findable.** The report path alone looked
correct on both sides. Running the same fixture through a command that does
*not* force offline is what separated "did not need the network" from "was
never going to use it".

## Harness bugs found, recorded so they are not rediscovered

Two of my own, both of which produced failures that looked like product
defects:

1. **Shared output path keyed on process id.** Every test in a binary shares a
   pid and cargo runs them in parallel, so they overwrote each other's reports
   and failed in whichever order they finished.
2. **Report written into the directory being scanned.** The second run then
   observed the first run's output file and the determinism test failed. This
   is also a real hazard for operators: `--output` inside the scan root
   perturbs the very thing being reported on.
