# Contract: wire changes in spec 1653

All of them happen only on `--nix-closure` scans, on components carrying C218. Every other document is byte-identical to `main`.

## Edges

**The edges are dependency edges in every format:**
- CycloneDX: `dependencies[].dependsOn`;
- SPDX 2.3: `DEPENDS_ON`;
- SPDX 3: `dependsOn`.

**Where they run:** from a boot library to a boot library of the same compiler, as the dependency table records for that compiler's release, and only when the target is a component of the document. No component is added.

## C245 `waybill:boot-library-dependencies` (new row)

| Format | Carrier |
|---|---|
| CycloneDX | component `properties[]` `{"name":"waybill:boot-library-dependencies","value":"<JSON string>"}` |
| SPDX 2.3 | package `Annotation`, `MikebomAnnotationCommentV1` envelope |
| SPDX 3.0.1 | `Annotation` element whose subject is the package |

- **Values:** see `data-model.md`.
- **Parity:** `SymmetricEqual`, `order_sensitive: false`.
- **Principle V:** KEEP-NO-NATIVE (research R5).

## C105 `waybill:graph-completeness-reason`: a new code

```
boot-library-edges-incomplete: <N> hackage boot librar(ies), <I> with dependencies not in the document, <U> without dependency data
```

- **C104:** `partial` when the code is present.
- **CycloneDX `compositions[]`:** `hackage` is degraded, so no record claims it complete.
- **The vocabulary** goes from 11 to 12 codes.
