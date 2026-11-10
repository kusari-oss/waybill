# Contract: C216 `waybill:closure-scope-method`

**Scope**: document. **Encoding**: a JSON object serialized as a string, with keys sorted, the same as C204.
**Carriers**:
- CycloneDX: `metadata.properties[]`, `{"name": "waybill:closure-scope-method", "value": "<json>"}`
- SPDX 2.3: a document-scope `annotations[]` entry
- SPDX 3: a document-scope `Annotation`
**Parity**: `SymmetricEqual`, with an extractor in each format.
**Present when**: the `--nix-closure` tier ran (C184 present). Absent otherwise, including under `--offline`, where C190 already reports `offline-requested`.

## Shape

```json
{
  "method": "runtime-closure" | "reachability-walk" | "closure-role",
  "store": "local" | "<substituter URL>" | null,
  "verified": true | false | null,
  "unavailable-reason": null | "not-in-any-store" | "verification-failed" | "query-timed-out" | "no-evaluated-package" | "query-failed",
  "static-link-source": "declared-edges" | "derivation-fields" | "none" | null,
  "static-c-linking-detected": false,
  "unmatched-runtime-paths": <non-negative integer> | null,
  "schema": 1
}
```

## Rules

- `method = runtime-closure`:
  - `store` and `verified` are non-null;
  - `verified` is `true`, because an unverified answer is never used;
  - `unavailable-reason` is null.
- `method = reachability-walk`:
  - `store` and `verified` are null;
  - `unavailable-reason` is non-null.
- `method = closure-role`:
  - the tier ran, but the scope pass could not use either evidence source (e.g. `no-evaluated-package` and the walk has no root);
  - `unavailable-reason` is non-null.
- `static-link-source` is `none` when the runtime closure contains no Haskell-builder member, and null for `closure-role`.
- `static-c-linking-detected` is always `false`. It exists so the document says, rather than implies, that static C linking is not detected (spec edge case).
- `unmatched-runtime-paths` counts runtime store paths that joined to no emitted component, including a path whose name matches a component but whose version does not (the join is by output path, never by name). It is null unless `method = runtime-closure`.

## Catalog row (sbom-format-mapping.md)

- **Classification**: KEEP-NO-NATIVE.
- **Justification**: CycloneDX 1.6, SPDX 2.3 and SPDX 3.0.1 have no document-scope construct for how component scope was decided (research R5).
