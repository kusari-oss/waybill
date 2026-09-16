# Contract: Resolve anchoring

Each clause stated so it can be asserted in a test, with its status before
and after.

## A-1 — A resolve's contents are reachable from the root

> Following dependency edges from the document root MUST reach the contents
> of every anchored resolve.

**Today**: violated. 1 of 331 components reachable on the measured target,
with 760 correct edges sitting unreachable in the same document.

## A-2 — Ownership is declared, never guessed

> A resolve component MUST correspond to a resolve the project's own
> configuration names. Where nothing is declared, no ownership is asserted.

The feature reads a structure the project states. It does not infer ownership
from directory layout, filename, or proximity.

## A-3 — A resolve is not a package

> A resolve component MUST be distinguishable from a package by a consumer
> reading the document.

It has no upstream and no vulnerability surface. A consumer that cannot tell
the difference will try to fetch or scan it. The `pkg:generic/` PURL type is
not sufficient on its own — other real, fetchable things use it.

## A-4 — Classification prefers declaration over name

> Where a tool declares `install_from_resolve`, that declaration MUST
> determine the resolve's lifecycle. A name-based heuristic MAY apply only
> where nothing declares, and its use MUST be reported.

**Today**: violated in both directions. The shipped allowlist is the only
signal, and it misclassifies `coverage-py` (allowlist has `coverage` and
`coveragepy`) and `setuptools` on the measured target.

Note this clause is **weaker than FR-003a as originally written**, which
forbade the heuristic outright. Research R1 records why: removing it would
regress repositories whose tools do not declare, and no measurement supports
that trade. The spec needs amending to match.

## A-5 — Undeclared defaults to runtime, loudly

> A resolve nothing declares MUST be treated as runtime, and the number of
> resolves classified this way MUST be reported.

Asymmetric on purpose. Mis-marking a runtime resolve as build-time hides
packages from a consumer filtering for runtime risk; the converse
over-reports. Only the first failure is silent, so the default takes the loud
one and makes its own frequency visible.

## A-6 — Connecting invents nothing

> No package component may be created by anchoring.

Resolve components are added by design and correspond to declared resolves.
Packages are not. This feature changes what is connected, never what exists.

## A-7 — Nothing happens without a resolve

> A project declaring no resolve MUST emit output identical to before.

The property that makes this safe to land across a corpus of eighteen targets
where only one has resolves.

## A-8 — The verdict is measured, not self-reported

> Reachability claims MUST be asserted against the emitted document, not read
> back from the completeness classifier.

The classifier consumes the same edges this feature adds, so asking it
whether the graph improved is asking the change to grade itself. Milestone
866 shipped a document reporting `complete` over a graph it had itself filled
in with fabricated edges; this is the same trap one turn later.

## Status summary

| Clause | Today | After |
|---|---|---|
| A-1 contents reachable | **violated** — 1 of 331 | holds |
| A-2 declared, not guessed | n/a — no ownership exists | holds |
| A-3 resolve ≠ package | n/a | holds |
| A-4 declaration over name | **violated** — 2 of 8 misclassified | holds |
| A-5 undeclared ⇒ runtime, reported | half-holds — default right, unreported | holds |
| A-6 invents nothing | holds | must not regress |
| A-7 no resolve ⇒ unchanged | holds | must not regress |
| A-8 measured, not self-reported | n/a | holds |

## Verification

```bash
# A-1: the measured target
waybill --offline sbom scan --path <backend.ai> \
  --format cyclonedx-json --output cyclonedx-json=out.json \
  --root-name pants-backend-ai --root-version 809fcd3
# expect: reachable-from-root rises well above 1 of 331; depth > 1; not flat

# A-4: coverage-py and setuptools classify build-time
# A-6: component count rises by exactly the number of declared resolves
# A-7: every corpus target without resolves is byte-identical
```
