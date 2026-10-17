# Data model — feature 1156

Two entities are new. One existing entity loses rows. No runtime state is
added anywhere: the marker is computed at read time and written into the
emitted document, and the registry is committed test data.

## 1. The version-absence marker (wire)

| Field | Value |
|---|---|
| Key | `waybill:version-absence` |
| Value domain | closed enum; v1 recognises exactly `by-construction` |
| Scope | per-component |
| Cardinality | at most one per component; single-valued, never an array |
| Emitted iff | the component is a synthetic Pants resolve anchor |
| Absent on | every other component, so pre-feature emission is byte-identical except on the seven anchors |

**Meaning**: no version exists to be found for this component; the absence
is not a resolution failure.

**Invariants**

- Mutually exclusive with C151 `waybill:unresolved-reason` on the same
  component. C151 means *looked and failed*; this means *there was nothing
  to look for*. Both present is a contradiction and an emitter defect.
- Independent of C160 `waybill:component-kind`. An anchor carries both:
  C160 says what it is, C197 says why it has no version. One fact per
  annotation — this is why the value is not `not-a-package`.
- Survives deduplication unchanged. Resolve membership is singular on a
  pre-dedup entry and may be plural on an emitted component; this field is
  a scalar fact about the component and must not be multiplied or joined.
- Not promoted or rewritten by split modes. An anchor is never a split
  document's subject (`--split=resolve` selects by membership filter and a
  convention-discovered resolve has no anchor at all), so an anchor appears
  in such a document as an ordinary member carrying the marker.

**Per-format carriers** — see `contracts/c197-version-absence.md`.

## 2. Must-pass entry (committed test data)

One row of `waybill-cli/tests/corpus_harness_195/invariant-must-pass.toml`.
Deliberate mirror of `Declaration`, because the reviewer reading one should
be able to read the other without relearning the shape.

| Field | Type | Compared? | Notes |
|---|---|---|---|
| `invariant` | string | yes | must name a live invariant in `REGISTRY` |
| `target` | string | yes | must name a target in the corpus manifest |
| `reason` | string | no, but required non-empty | *what a `NotApplicable` here would mean* — not "it passes today" |
| `issue` | optional u32 | no | provenance, as on declarations |

**Semantics**: a listed cell MUST report `Pass`. `NotApplicable` and `Fail`
both fail the lane. `Fail` is already caught as `UndeclaredFailure` by the
existing verdict table, so the registry's distinct contribution is turning
a green `OkNotApplicable` into a lane failure for cells where applicability
is the thing being asserted.

**Validation** (all before any comparison, mirroring `declarations::validate`)

1. `invariant` names a live, non-retired invariant.
2. `target` is in the corpus manifest.
3. `reason` is non-empty after trimming.
4. No duplicate `(invariant, target)`.
5. Sorted ascending by `(invariant, target)` — line stability, the
   milestone 117 retrofit precedent.
6. **No overlap with `invariant-declarations.toml`.** A cell in both files
   is told to pass and recorded as failing. The lane rejects the overlap
   rather than letting either win.

**Seed contents**: the four Pants I8 cells —
`pants-clojure-polyglot`, `pants-example-django`, `pants-example-jvm`,
`pants-example-python`.

**Deliberately not seeded**: every other currently-passing cell. A registry
populated by sweeping whatever is green today would record no judgement and
would re-create, one level up, exactly the vacuity it exists to prevent
(validation rule 3 is the guard). Entries are added when a cell is shown to
be load-bearing.

## 3. Declaration (existing, loses rows)

Unchanged in shape. Four rows deleted in the acceptance change:

| invariant | target | kind | fate |
|---|---|---|---|
| I8 | `pants-clojure-polyglot` | `versionless-and-unexplained` | **deleted** → becomes a must-pass entry |
| I8 | `pants-example-django` | `versionless-and-unexplained` | **deleted** → becomes a must-pass entry |
| I8 | `pants-example-jvm` | `versionless-and-unexplained` | **deleted** → becomes a must-pass entry |
| I8 | `pants-example-python` | `versionless-and-unexplained` | **deleted** → becomes a must-pass entry |
| I8 | `image-postgres16` | `versionless-and-unexplained` | **kept** — population 2, out of scope |

Each of the four moves from one file to the other, which is the clearest
possible statement of what the feature did: the cell stopped being a known
failure and started being a known-meaningful pass.

## 4. Cell outcome (existing, unchanged)

Quoted because the whole feature turns on the distinction:

```rust
pub enum CellOutcome {
    Pass,
    Fail(Vec<Violation>),
    NotApplicable { reason: String },
    Error { message: String },
}
```

`NotApplicable` is not a pass (feature 1155 FR-003). The existing
`verdict()` maps both `Pass` and `NotApplicable` to non-lane-failing
verdicts (`Ok`, `OkNotApplicable`), which is correct in general and
insufficient for a cell whose applicability is load-bearing. The registry
supplies the missing per-cell judgement without changing `verdict()`.

## 5. State transition — one cell, across the two changes

| Stage | I8 on a Pants target | Declaration | Must-pass | Verdict |
|---|---|---|---|---|
| Today, on `main` | `Fail(versionless-and-unexplained)` | present, kind matches | — | `AcceptedKnownFailure` → green |
| After change one (emission + goldens) | `Fail(versionless-and-unexplained)` — marker on the wire but not yet accepted | present, kind still matches | — | `AcceptedKnownFailure` → green |
| After change two (acceptance) | `Pass` | deleted | present | `Ok` + registry satisfied → green |
| Regression: marker dropped | `Fail` | absent | present | `UndeclaredFailure` → red |
| Regression: anchors vanish from goldens | `NotApplicable` | absent | present | registry pass → red *(the hole this closes; green before this feature)* |

The middle row is why the work can ship in two pieces: the emission change
does not move the cell, so the declaration describing it stays true.
