# Contract: C191 / C192 deps.dev enrichment outcomes

Both are emitted only when the scan ran deps.dev enrichment online (not `--offline`, not disabled). Answers served from the disk cache count as online answers.

## C191 `waybill:deps-dev-outcome` (per component)

On a component in deps.dev's six ecosystems (cargo, npm, pypi, go, maven, nuget) that deps.dev did not enrich. The value is one of:

| value | meaning |
|---|---|
| `absent` | deps.dev said it has no such version: a 404, or a batch item returned without a version. A malformed query looks the same (measured), so this signal is how such bugs become visible. |
| `declined-invalid-license` | deps.dev returned licence strings, and every one failed SPDX canonicalisation. The strings are not copied into the document. |
| `transport-failure` | the request failed (network, non-2xx other than 404, unreadable response). |
| `not-queried:incomplete-coordinate` | no request was sent: the version is empty or a placeholder (`unknown`, `0.0.0-unknown`, `v0.0.0-unknown`, `noassertion`, `none`, `latest`). |

**Absent when:**
- deps.dev returned a record, whether or not it added a licence (matched);
- the component is outside the six ecosystems (counted in C192 only).

CycloneDX: `components[].properties[]` entry `{"name":"waybill:deps-dev-outcome","value":"absent"}`. SPDX 2.3 / SPDX 3: per-package annotation, `MikebomAnnotationCommentV1` envelope, same string value.

## C192 `waybill:deps-dev-outcomes` (document scope)

A canonical JSON object, carried as a string in CycloneDX and as an object in the SPDX envelopes. Keys are outcome values, plus `not-queried:unsupported-ecosystem`; values are component counts. Keys are sorted, and only non-zero counts are included. It is emitted iff at least one count is non-zero.

```json
{"absent":2,"not-queried:incomplete-coordinate":28,"not-queried:unsupported-ecosystem":1}
```

**Invariant:** for every key except `not-queried:unsupported-ecosystem`, the count equals the number of components carrying that C191 value (SC-005).

## Unchanged

- C158 `waybill:enrichment-degraded`, and the per-pass log line (FR-008).
- Offline and deps.dev-disabled scans: byte-identical (FR-007, FR-009).
