# Contract: what the documentation promises

**Feature**: `1376-count-emitted-components` | **Issue**: #1385

**No emitted interface changes.** This contract is about a sentence.

---

## 1. The claim today, and why it is false

`docs/reference/sbom-format-mapping.md`, C209 row:

> The five **partition the document's components** — they sum to
> `.components | length` exactly, **with no filtering rule the reader has to
> know**, which is the property a consumer can check with `jq`.

True only for a CycloneDX document whose root is synthetic. False for:

- **CDX with a real promoted root** — `.components | length` is short by 1.
- **SPDX 3, always** — short by `excluded_files`.
- **SPDX 2.3** — the expression does not apply; there is no `.components`.

## 2. The claim after

> The five counts partition the components waybill resolved. Each format
> arranges those components differently, so the expression that recovers the
> total differs per format:
>
> | format | expression |
> | --- | --- |
> | CycloneDX | `len(components[])`, **plus 1** when `metadata.component` carries `waybill:*` properties and its `bom-ref` is not already in `components[]` |
> | SPDX 2.3 | `len(packages[])` |
> | SPDX 3 | `Package` nodes **plus** `File` nodes |
>
> The CDX adjustment exists because a scan finding exactly one main module
> promotes it to `metadata.component`; a synthetic root carries no `waybill:*`
> properties and is not a resolved component, so it is not counted.

**The phrase "with no filtering rule the reader has to know" is removed.**
There is a rule, it is short, and pretending otherwise is what made the claim
wrong.

## 3. Source comments

| location | correction |
| --- | --- |
| `total()` doc comment | drop "against the document as it stands"; state that the sum is over resolved components and that the per-format expressions live in the C209 row |
| the `debug_assert` comment | say it checks the **slice** — that nothing was classified twice or not at all — and explicitly that it does **not** check the document |

## 4. Verified expectations

| document | row | CDX expression | agrees |
| --- | --- | --- | --- |
| `slack-web`, promoted root | 200 | 200 | ✅ |
| `slack-web --root-name`, synthetic | 200 | 200 | ✅ |
| live `moat`, duplicated root | 977 | 977 | ✅ |
| 15 of 16 corpus targets with the row | — | — | ✅ |
| `pants-example-javascript` | 304 | 303 | ❌ — **filtered golden**, #1380 |

## 5. What does not change

| | |
| --- | --- |
| every emitted document, byte for byte | **unchanged** (SC-004) |
| the five counts, their names, the schema version | **unchanged** (FR-002) |
| `metadata.component` promotion behaviour | **unchanged** |
| SPDX 3 `File`-node emission | **unchanged** |
| CLI surface | **no new flags** |
