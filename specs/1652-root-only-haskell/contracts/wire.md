# Contract: wire changes in spec 1652

All three are emitted only on a scan that filtered: `--project-discovery` is `root-only` or `strict`, and the #1045 fall-back did not fire. Under `all`, every document is byte-identical to `main`.

## C244 `waybill:project-discovery-dropped` (new row)

- **Scope:** document.
- **Present** iff at least one main module was dropped.
- **Value:** a canonical JSON string, with keys sorted and `main-modules` sorted:

```json
{"count":2,"main-modules":["pkg:hackage/waybill-fixture-a@0.1.0","pkg:hackage/waybill-fixture-b@0.1.0"],"reason":"project-discovery-scope"}
```

| Format | Carrier |
|---|---|
| CycloneDX | `metadata.properties[]` `{"name":"waybill:project-discovery-dropped","value":"<string above>"}` |
| SPDX 2.3 | document `Annotation` on `SPDXRef-DOCUMENT`, `MikebomAnnotationCommentV1` envelope, `field` = the key, `value` = the string |
| SPDX 3.0.1 | `Annotation` element on the `SpdxDocument`, same envelope |

**Parity:** `SymmetricEqual`, `order_sensitive: false`, with extractors in all three formats.

**Principle V:** KEEP-NO-NATIVE (research R5).

## `waybill:graph-completeness-reason` (C105): a new code

```
project-discovery-scope: <N> project(s) dropped from <ecosystem>[, <ecosystem>]*
```

- **The value:** `waybill:graph-completeness` (C104) is `partial` whenever this code is present, including when nothing else remains in the document.
- **CycloneDX `compositions[]`:** each named ecosystem is degraded, so its aggregate is not `complete`.
- **Joining:** with other codes by `; `, in the existing order. The new code is appended after `manifest-unread` on each path.

## C200 `waybill:hpack-read-outcome`: a new key

```json
"manifests-dropped-by-scope": ["<entry of manifests-read>", ...]
```

- Sorted, and present only when non-empty.
- Every entry is also in `manifests-read`, which is unchanged.

## Log (not wire, documented)

The FR-012 INFO line keeps its fields. `nested_projects_ignored` is now the number of main modules actually dropped.
