# Contract: Completeness invariants

What `waybill:graph-completeness` may and may not assert. Each invariant
is stated so it can be asserted in a test, with its current status.

## I-1 — `complete` excludes unknown ecosystem coverage

> A document MUST NOT declare `graph-completeness = complete` while any
> per-ecosystem coverage signal in the same document reports `unknown`
> or `partial`.

This is the invariant #829 asked for. **Violated today** by both Go
corpus targets, which emit:

```
waybill:graph-completeness     = complete
waybill:go-transitive-coverage = unknown
```

Independent of graph shape by design: it still holds if a future
fallback attaches components to something real but wrong, which
reachability would not catch.

## I-2 — `complete` excludes unattached components

> A document MUST NOT declare `complete` while any component is
> unattached.

Already expressed by the existing verdict
(`complete` iff `reason_codes.is_empty() && orphan_count == 0`). It is
satisfied vacuously today because the unbacked edges attach everything:
`kubernetes` reports `reachable_count=494 total_count=494
orphan_count=0`.

**No code change is expected for this invariant.** Once the unbacked
edges are gone the stranded components are genuinely unreachable and the
existing `OrphanedComponentsDetected` classifier fires unaided. The test
is worth writing anyway, because its current pass is accidental.

## I-3 — `complete` stays earnable

> A fully-resolved graph MUST still be able to declare itself
> `complete`.

**Holds today and must not regress.** The warm-cache control on
`go-cobra` resolves the real module graph, emits zero unbacked edges,
and correctly reports `complete` with `go-transitive-coverage =
complete`.

This is the invariant that prevents "fixing" I-1 and I-2 by never
emitting `complete` again — which would satisfy both and destroy the
field.

## I-4 — Shape is not the test

> A shallow or single-level graph MUST NOT, on its own, prevent
> `complete`.

Some projects genuinely have no transitive structure. Measured:
`cmake-nlohmann-json` and `uv-meilisearch-python` are flat because their
source formats (scattered cmake/bazel declarations; `Pipfile.lock`)
carry no parent-child topology at all. They emit no unbacked edges and
no coverage signal, and they are out of scope.

Flatness was the symptom #829 reported. Post-m860 it is no longer even
the right symptom — the Go graph is depth 2. Any check keyed on shape
would both miss the real defect and mislabel these two targets.

## I-5 — Declining must say why

> When `complete` is declined, the document MUST carry a reason from the
> documented vocabulary, distinguishing "the input carried no topology"
> from "topology existed but could not be resolved".

The two cases want different consumer responses. The first is final —
nothing more can be recovered. The second is recoverable: a resolvable
module graph produces the real topology, which the warm-cache control
demonstrates.

## I-6 — The verdict must not be self-certifying

> The project MUST measure emitted graph structure independently of the
> document's own self-report.

A self-report cannot validate the thing reporting. The m770 quality
corpus measures shape independently, which is the only reason this
defect was found at all — and it caught it three times
(`cmake-nlohmann-json`, `uv-meilisearch-python`, and the Go targets)
before anyone read the completeness field.

The committed probes must share no code with waybill, so a normalisation
bug cannot cancel out in exactly the comparison that matters.

## Invariant status summary

| Invariant | Today | After |
|---|---|---|
| I-1 coverage `unknown` excludes `complete` | **violated** — both Go targets | holds; asserted by FR-009 gate |
| I-2 unattached excludes `complete` | vacuously true (edges mask it) | holds for real |
| I-3 `complete` stays earnable | holds (warm control) | must not regress |
| I-4 shape is not the test | holds | holds; SC-007 guards it |
| I-5 declining says why | n/a — never declines for Go | holds |
| I-6 independent measurement | holds (m770) | extended by the committed probes |

## Verification

```bash
# Contradiction check across the committed goldens.
probe_completeness.py doc waybill-cli/tests/fixtures/public_corpus/*/cdx.json

# Shape vs self-report across the quality corpus.
probe_completeness.py corpus target/quality/run-*.json
```

Expected after this feature: the `doc` mode flags zero goldens. It
currently flags `go-cobra` and `pants-example-golang`, and passes nine —
four of which legitimately declare `complete`, which is what shows the
check is not vacuously failing everything.
