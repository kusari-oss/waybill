# Contract: emitted interfaces

**Feature**: `1366-closure-ecosystem-purl` | **Issue**: #1365

waybill's external interface is the documents it emits plus its CLI. This
feature changes both: two new catalog rows, one changed PURL population, and no
new flags.

---

## 1. The identifier itself (FR-002, FR-004)

A **confirmed** candidate's component identifier changes from

```text
pkg:generic/urllib3@2.6.3
```

to

```text
pkg:pypi/urllib3@2.6.3
```

Constraints:

- Built through `waybill_common::types::purl::Purl::new`, so Constitution
  Principle V's *"invalid PURLs MUST NOT appear in output"* is enforced by the
  type, not by review. A coordinate that fails to parse declines the candidate.
- `name` is PEP 503-normalised via `pip::normalize_pypi_name_for_purl`
  (research R4). `version` is the closure's string, verbatim.
- The component's `name` and `version` fields are **unchanged**. Only `purl`
  moves. This keeps `merge_closure_components`' `(name, version)` key stable
  and keeps invariant I2 — which compares `(name, version, type)` — reading the
  same groups it reads today.
- A **declined**, **unchecked** or non-candidate component is byte-identical to
  today.

---

## 2. `C207 waybill:closure-ecosystem-identity` — per-component

Records that an identifier was **derived and confirmed**, and on what basis.
Emitted **only** on components this pass confirmed. Absent on declined,
unchecked and non-candidate components — so its presence is the signal, and
US3's second acceptance scenario is testable by absence.

**Hybrid emission** on the C56 template (research R5): CDX carries it
natively, SPDX bridges it.

### CycloneDX 1.6 — native

An entry appended to `component.evidence.identity[]`:

```json
{
  "field": "purl",
  "confidence": 0.8,
  "concludedValue": "pkg:pypi/urllib3@2.6.3",
  "methods": [
    {
      "technique": "manifest-analysis",
      "confidence": 0.8,
      "waybill-closure-ecosystem-evidence": "versioned-interpreter-prefix"
    }
  ]
}
```

- `technique: "manifest-analysis"` — the closure's derivation record is the
  manifest read. From CDX 1.6's closed technique enum; `attestation` and
  `hash-comparison` would both be false.
- `confidence: 0.8` is a deliberate, documented choice, not a tuned number.
  It states what research R9 establishes and no more: a published project of
  that name exists at that version, which is necessary and not sufficient to
  prove this derivation *is* that project. `1.0` would assert the sufficiency
  the spec's Assumptions explicitly decline to claim.
- `waybill-closure-ecosystem-evidence` is an extension key on the method
  object, exactly as milestone 105 added `waybill-source-mechanism` for C56.

### SPDX 2.3 — parity bridge

Per-Package `annotations[].comment`, `MikebomAnnotationCommentV1` envelope:

```json
{
  "schema": "waybill-annotation/v1",
  "field": "waybill:closure-ecosystem-identity",
  "value": "versioned-interpreter-prefix"
}
```

### SPDX 3.0.1 — parity bridge

Per-Package `Annotation.statement`, same envelope and value.

### Parity

`Directionality::SymmetricEqual`, `order_sensitive: false`. The CDX extractor
reads `evidence.identity[*].methods[*].waybill-closure-ecosystem-evidence`; the
SPDX extractors read the annotation value. Both normalise to the same closed
set, so the three formats compare equal.

> A row added to `docs/reference/sbom-format-mapping.md` without a matching
> entry in `parity/extractors/mod.rs::EXTRACTORS` fails
> `every_catalog_row_has_an_extractor` and `holistic_parity`. The doc row and
> the extractor land in the same change.

---

## 3. `C208 waybill:closure-ecosystem-identity-outcome` — document scope

A JSON object, encoded as a string, on the C192 / C204 template.

```json
{
  "schema": "waybill-closure-ecosystem-identity/v1",
  "ecosystem": "pypi",
  "candidates": 75,
  "confirmed": 61,
  "declined": 14,
  "unchecked": 0
}
```

With confirmation unavailable — **the corpus path** (research R7):

```json
{
  "schema": "waybill-closure-ecosystem-identity/v1",
  "ecosystem": "pypi",
  "candidates": 75,
  "confirmed": 0,
  "declined": 0,
  "unchecked": 75,
  "reason": "disabled"
}
```

- `reason` present iff `unchecked > 0`. Closed set:
  `disabled` | `offline` | `unreachable` | `throttled` | `unkeyable` |
  `collision`.
- `candidates + 0` with every other count zero and **no** `reason` is the
  "closure holds no Python packages" document — distinguishable from the
  degraded one, which is the whole of FR-008.
- Emitted wherever C204 is emitted (i.e. a closure was produced). Absent
  otherwise.
- Carriers: CDX `metadata.properties[]`; SPDX 2.3 and SPDX 3 document-scope
  annotation. `Directionality::SymmetricEqual`.

**The counts in the example above are illustrative shape, not measurements.**
`confirmed: 61` is a placeholder for a number SC-001 requires be measured; no
artifact may quote it as data.

---

## 4. CLI

**No new flags.** The feature is reached by `--nix-closure`, and suppressed by
the deps.dev flags that already exist:

| flag | effect on this pass |
| --- | --- |
| `--nix-closure` | required — the pass runs only where a closure exists |
| `--no-deps-dev` | suppresses confirmation → `unchecked`, `reason: "disabled"` |
| `--no-deps-dev-license` | same |
| `--enrich-no-cache`, `--enrich-cache-max-age` | inherited through `DepsDevSource` (FR-011) |
| `--offline` | refused earlier by `closure::admission`; no closure, no pass |

Adding a dedicated opt-out was considered and rejected: the operator who turns
off deps.dev has already expressed this preference, and a second flag that can
disagree with the first is a bug surface.

---

## 5. Compatibility

- **Additive for consumers reading `pkg:generic`**: a consumer matching on the
  generic identifier of a confirmed component will stop matching. That is the
  intended effect of the feature and the reason US3 exists — the change is
  announced in-document rather than inferred.
- **No component is added or removed.** Constitution Principle XII constraint
  1 holds by construction: the pass mutates identifiers on an existing set.
- **No dangling edges, by ordering.** `Relationship.from` / `.to` are **PURL
  strings** (`waybill-common/src/resolution.rs:413-418`), so a late identifier
  change could orphan every edge pointing at it. It does not, because
  `closure::graph::relationships` builds its join table from the **live**
  component slice — `(name, version) → c.purl.as_str()`
  (`closure/graph.rs:247-250`) — and runs at `scan_cmd.rs:4833`, after the
  pass. The edges are therefore built from the corrected identifiers.
  **The ordering in research R11 is the mechanism, not a convenience**; a task
  in `tasks.md` asserts it with a test that fails if the pass is moved.
