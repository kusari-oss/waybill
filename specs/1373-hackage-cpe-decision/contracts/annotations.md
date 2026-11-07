# Contract: emitted interfaces

**Feature**: `1373-hackage-cpe-decision` | **Issue**: #1375

One row changes shape. No new rows, no new flags, no component changes.

---

## 1. `C209 waybill:cpe-coverage` — v1 → v2

**Before**

```json
{"schema":"waybill-cpe-coverage/v1","with_candidates":78,"eligible_unmapped":284}
```

**After**

```json
{"schema":"waybill-cpe-coverage/v2","with_candidates":78,
 "addressable_without_cpe":166,"unaddressable":733}
```

| format | carrier — **unchanged** |
| --- | --- |
| CycloneDX | `metadata.properties[]` |
| SPDX 2.3 | document-scope annotation, `MikebomAnnotationCommentV1` envelope |
| SPDX 3 | document-scope `Annotation.statement`, same envelope |

`Directionality::SymmetricEqual`, `order_sensitive: false`. The extractors
already exist and key on the row label, not the payload, so **no extractor
changes are needed** — but that is a claim a task verifies rather than
assumes, because milestone 1372 recorded that an extractor can compensate for
a wire shape.

### Compatibility, stated plainly

A consumer reading `eligible_unmapped` gets **nothing** after this. That is
the point: the field is wrong in both directions, and the schema bump from
`/v1` to `/v2` is how a consumer learns the shape changed rather than
silently reading an absent key as zero.

`with_candidates` keeps its exact meaning and is unaffected.

---

## 2. The invariant a consumer can check

```
with_candidates + addressable_without_cpe + unaddressable == |components|
```

This is the property `eligible_unmapped` never had. A reader could not
previously tell whether 284 was most of the document or a tenth of it without
counting components themselves and knowing the eligibility rule.

---

## 3. Blast radius — the inverse of milestone 1372

| | files |
| --- | --- |
| local goldens carrying C209 | **33** |
| corpus goldens carrying C209 | **48** |
| corpus targets whose value changes | **16 of 17** |

**Every one of them changes.** C209 is not closure-gated: one production site
sets it and it is always `Some`. It is the exact opposite of milestone 1372,
where zero moved.

> A `grep -rl` over the golden directory reports 66, but half are
> `*.actual.json` diagnostics rather than tracked goldens. The tracked figure
> is 33, measured at implementation time after the spec had carried 66.

`image-postgres16` is the 17th target and stays **excluded**: its committed
golden predates milestone 1367 and carries no C209 at all, for the unrelated
layer-digest drift in #1372. Verified rather than assumed — the milestone-1372
regeneration artifact for that target does carry the row.

### The expected values, measured before implementing

| target | `with_candidates` | `addressable_without_cpe` | `unaddressable` |
| --- | --- | --- | --- |
| `nix-closure-moat` | 6 | 163 | 1,446 |
| `haskell-language-server` | 0 | 364 | 35 |
| `haskell-security-advisories` | 0 | 248 | 3 |
| `haskell-aeson` | 0 | 61 | 2 |
| `npm-express` | 1 | 43 | 1 |
| `rust-ripgrep` | 61 | 0 | 7 |
| `go-opentelemetry` | 333 | 0 | 1 |
| `python-flask` | 108 | 0 | 0 |
| `go-cobra` | 8 | 0 | 0 |
| `pants-example-javascript` | 303 | 0 | 0 |

These are the expectation, not the contract. A task records the
implementation's own figures and compares; a mismatch is a finding.

> `npm-express` is the sharpest illustration of why the row changes.
> It reads `eligible_unmapped: 0` today, which looks like complete coverage.
> In fact **1** of its 45 components carries a CPE; the other 43 are reachable
> only because npm is an OSV ecosystem. The new row says so.

---

## 4. CLI

**No new flags.** The row is emitted on every scan and is not optional.

---

## 5. What does not change

| | |
| --- | --- |
| any component's `cpes` array | **unchanged** (FR-004, SC-004) |
| `synthesize_cpes` and all its arms | **unchanged** |
| `GENERIC_LIBRARY_CPES` | **unchanged** |
| the carriers in all three formats | **unchanged** |
| `with_candidates` | **unchanged** |
| component, relationship, licence output | **unchanged** (SC-003) |
