# Pre-change baseline (T002, T003)

Binary: `waybill 0.9.0`, release build, pre-change. Preserved for the T030
teeth-check.

```
waybill sbom scan --path waybill-cli/tests/fixtures/pants_discovered_resolves \
  --split=resolve --output-dir <out> \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json --offline
```

## T002 — the defect, reproduced

Two documents, one convention-only repository:

| | `default.generic.cdx.json` | `lint.generic.cdx.json` |
|---|---|---|
| `metadata.component.name` | `pants_discovered_resolves` | `pants_discovered_resolves` |
| `bom-ref` | `pants_discovered_resolves@0.0.0` | `pants_discovered_resolves@0.0.0` |
| `waybill:resolve-ownership` | `{"declared":[],"discovered":["default","lint"],"unanchored_lockfiles":2,"weak_classification":0}` | *byte-identical* |
| components | **2** | **1** |

Both name the repository. The ownership annotation is **byte-identical**
across them — confirmed by comparison, not by inspection. A reader holding
one file sees two resolve names and nothing saying which file this is.

The only document-scope properties mentioning resolves are
`waybill:unresolved-declared-dep-count` and `waybill:resolve-ownership`.
Neither is document-scoped in meaning.

### The SC-007 / T017 baseline

Component counts per document are **default=2, lint=1**. T017 asserts these
are unchanged after the feature. If an implementation synthesises the anchor
m868 refused, these move — which is the only thing that catches it.

### Manifest cross-check (T001 Finding 1, confirmed empirically)

```
subproject_id=default.generic   root_purl=pkg:generic/default   components=2
subproject_id=lint.generic      root_purl=pkg:generic/lint      components=1
```

The manifest names the resolve for discovered entries too, via the
`or_else` fallback at `split.rs:279-281`. The gap is exactly the
document-without-manifest case. Note the value is the **bare** resolve name —
the same ambiguity FR-001a removes from the document.

## T003 — anchoring is Pex-only, which widens who needs this

Counted `lockfile-resolve` anchor components in the committed corpus goldens
(`waybill-cli/tests/fixtures/public_corpus/<target>/cdx.json`):

| Corpus target | anchor components |
|---|---|
| `pants-example-python` | 1 |
| `pants-example-django` | 1 |
| **`pants-example-jvm`** | **0** |
| `pants-example-golang` | 0 |
| `pants-example-javascript` | 0 |

**R3 confirmed.** A JVM Pants repository has no anchors at all, so every one
of its per-resolve documents is in the failing case *regardless of whether its
resolves were declared*.

The spec's framing — "a declared resolve's document does say so, a discovered
one does not" — is true **within Python** and false across the product. This
is why the JVM fixture (T011) is required rather than optional: Python-only
fixtures would let a Python-only implementation look complete while an entire
ecosystem stays broken.
