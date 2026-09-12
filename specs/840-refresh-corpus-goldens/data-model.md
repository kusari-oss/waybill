# Data Model: Refresh the public-corpus goldens with verified drift

Feature: `840-refresh-corpus-goldens` · Spec: [spec.md](./spec.md)

No persistent runtime state. The entities below are review-time and
build-time artifacts; nothing here exists while waybill runs.

---

## Corpus target

One pinned upstream repository or image the lane scans.

| Field | Description |
|---|---|
| name | Directory name under `fixtures/public_corpus/`, e.g. `go-cobra` |
| pin | Commit SHA or image digest — fixed, so the input never drifts |
| gated | Whether the lane currently enforces this target (see state below) |

**Validation**
- A target's pin is immutable within this feature. Re-pinning changes the input, which would confound "did emission drift" with "did the input change" — the exact confusion that cost three days on #832.
- A target is either gated or explicitly removed with a tracked issue (FR-012). There is no third state, and in particular no "gated but permanently failing", which is the condition this feature exists to end.

**State transitions**

```
gated + passing ──emission changes──▶ gated + failing
gated + failing ──drift, attributed──▶ refreshed, gated + passing   (FR-006)
gated + failing ──non-drift cause────▶ repaired here, gated + passing   (FR-012)
                                    └▶ removed from gating + tracked issue   (FR-012)
```

The removal edge is the one to watch: it is how coverage shrinks, which is why FR-012a requires it be visible in lane output. Reaching 100% by removing targets must not look like reaching 100% by fixing them.

---

## Golden

The committed expected output for one target in one format.

| Field | Description |
|---|---|
| target | Owning corpus target |
| format | `cdx` \| `spdx-2.3` \| `spdx-3` |
| content | Pretty-printed JSON, **already masked** at write time |

**Validation**
- Stored masked (`layer2_golden.rs:51`), so the committed bytes already have per-scan timestamps, `/doc-` identifiers and embedded content hashes neutralised. A `git diff` is therefore normalised for those categories without further work.
- Byte-identical comparison is the gate. Anything that changes stored bytes changes what is asserted, so normalisation for *review* must never write back (see Normalised diff).
- One target, `pants-example-javascript`, has a JS-only filter applied before writing. Its goldens are not comparable in shape to other targets' and should not be used as a reference when judging whether another target's delta looks anomalous.

---

## Normalised diff

The difference between an old and a new golden after non-semantic churn is removed. **Review-only. Never written back.**

| Field | Description |
|---|---|
| target, format | Which golden |
| categories | Repeated shapes of change (see Delta attribution) |
| residual | Changes matching no category — the interesting part |

**Validation**
- Must neutralise ordering within unordered collections (FR-005). This is the one category masking cannot handle: masking substitutes values, it cannot reorder, and SPDX 3 `@graph` reordering otherwise presents as every element changing.
- Must not modify committed goldens. A normaliser that wrote back would make a reordered-but-equal golden compare equal, silently weakening the gate.
- An empty normalised diff for a target that the lane reports as failing means the normaliser is over-aggressive — it has masked a real change. That is a defect in the normaliser, not evidence the target is fine.

---

## Delta attribution

The mapping from an observed category of change to the merged change that caused it.

| Field | Description |
|---|---|
| category | A repeated shape, e.g. "every component gained an `externalReferences` entry of type `vcs`" |
| cause | The merge that produced it, by PR or milestone |
| targets | Which targets exhibit it |

**Validation**
- A change appearing exactly once is **not** a category. It is an individual delta and needs its own explanation (FR-007). Folding singletons into a category is how a regression gets absorbed into a large benign diff.
- Every category must name a cause. "Expected churn" is not a cause.
- Lives in the PR description, not the repository (FR-015).

**Cardinality note**: one cause commonly explains categories across many targets — m776's `externalReferences` work touches every ecosystem. One target exhibiting a category that no other target shows is the signal worth stopping on, which is why FR-013a requires cross-target comparability.
