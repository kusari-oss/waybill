# Contract: Declared-dependency resolution

Stated so each clause can be asserted in a test, with the status it must hold
before and after.

## D-1 — A recorded ecosystem is authoritative

> When a reader records the ecosystem of its dependency names, resolution
> MUST search that ecosystem and no other.

Covers FR-002 and FR-006 together. FR-006's ambiguity class does not need
arbitration because this clause removes it: a candidate in another ecosystem
is never reached, so it can never compete.

**Today**: violated for every reader whose component PURL type differs from
its dependency names' ecosystem. Measured: `bitwarden/android` @ `d817f6b`
resolves 0 of 9.

## D-2 — No recording means no change

> An entry with no recorded ecosystem MUST resolve exactly as it does today.

The load-bearing clause for per-reader adoption. It is what lets SC-004a be
argued from construction rather than from exhaustive testing, and it is why
the field is optional rather than defaulted.

**Today**: holds vacuously — nothing records anything. Must continue to hold
for every unadopted reader after the change.

## D-3 — Normalisation follows the lookup ecosystem

> The name normalisation applied MUST use the same ecosystem as the lookup.

`normalize_dep_name` is ecosystem-keyed. Splitting these — searching the gem
ecosystem with a generically-normalised name — would produce a miss that
looks like a genuine absence and would be counted as one under FR-005,
manufacturing false evidence of a gap.

## D-4 — A miss invents nothing

> An unresolved declared dependency MUST NOT produce an edge and MUST NOT
> cause a component to be created.

Principle IX. This feature changes which candidates are considered, never
whether a component may be invented. The scan either saw the package or it
did not.

## D-5 — A miss is visible in the document

> An unresolved declared dependency MUST be counted at document scope, and
> the count MUST be emitted even when zero.

Principle X, and the clause the whole feature argues for. Silence is what let
this defect survive across releases and two readers. Zero must be emitted
because an absent count cannot be distinguished from a document that had no
declarations to resolve — which is exactly the distinction an auditor needs.

## D-6 — Inference stays separate and stays opt-in

> Edges produced under this contract MUST be distinguishable from edges
> produced by the opt-in cross-ecosystem inference capability, and enabling
> that capability MUST NOT change any edge produced here.

FR-007 and FR-008. The two carry different confidence: one is what a manifest
says, the other is what a search guessed. A consumer that cannot tell them
apart cannot weight them, and Principle X exists to prevent exactly that.

Satisfying FR-008 is close to trivial after this change, because the default
path resolves before the fallback is reached. It is asserted anyway —
"obviously true" is the category of claim that produced the original defect.

## D-7 — The verdict is not self-certifying

> The before/after claims MUST be measured against emitted documents, not
> read from the resolver's own reporting.

The unresolved count is produced by the same code path being changed. A test
that only checks the count would be asking the change to grade itself. Edge
presence must be asserted against the emitted graph, and the corpus target
holds the independent before/after.

## Status summary

| Clause | Today | After |
|---|---|---|
| D-1 authoritative recorded ecosystem | **violated** — 0 of 9 measured | holds |
| D-2 no recording ⇒ no change | holds vacuously | holds by construction |
| D-3 normalisation follows lookup | **violated** — generic rules on gem names | holds |
| D-4 a miss invents nothing | holds | must not regress |
| D-5 a miss is visible | **violated** — silent at every log level | holds |
| D-6 inference separate and opt-in | holds | must not regress |
| D-7 not self-certifying | n/a | holds |

## Verification

```bash
# D-1 / D-3: the measured target, no optional flags.
waybill --offline sbom scan --path <bitwarden-android> \
  --format cyclonedx-json --output cyclonedx-json=out.json \
  --root-name gradle-bitwarden-android --root-version d817f6b
# expect: the application main module carries 9 outgoing edges

# D-6: same scan with the inference flag on
#      expect: byte-identical dependency graph

# D-2 / SC-004a: the committed corpus
#      expect: byte-identical output for every unadopted reader
```
