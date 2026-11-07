# Contract: emitted interfaces

**Feature**: `1374-exclude-members-from-coverage` | **Issue**: #1380

One row gains two fields and a version. No new rows, no new flags, no
component changes.

---

## 1. `C209 waybill:cpe-coverage` — `/v2` → `/v3`

**Before**

```json
{"schema":"waybill-cpe-coverage/v2","with_candidates":78,
 "addressable_without_cpe":166,"unaddressable":733}
```

**After**

```json
{"schema":"waybill-cpe-coverage/v3","with_candidates":78,
 "addressable_without_cpe":166,"unaddressable":194,
 "excluded_members":539,"excluded_files":0}
```

Carriers are **unchanged** — CDX `metadata.properties[]`, SPDX 2.3 and SPDX 3
document-scope annotations, `SymmetricEqual`, `order_sensitive: false`. The
extractors key on the row label rather than the payload, so none should need
editing; milestone 1373 verified that for a payload change of the same shape,
and a task verifies it again rather than inheriting the claim.

### The property a consumer can check

```
with_candidates + addressable_without_cpe + unaddressable
  + excluded_members + excluded_files  ==  (.components | length)
```

**Against the document as it stands**, with no filtering rule to know. Under
`/v2` the sum matched only a subset the reader had to derive — components
with a PURL and not of `type: "file"` — which is why 11 of 17 corpus targets
could not be checked without that rule.

### Compatibility

A consumer reading `/v2`'s three fields still finds all three, with the same
meanings. What changes is that `unaddressable` no longer counts things that
are not software — the correction — and the schema string says the shape
moved. `with_candidates` is unchanged in meaning and in value.

---

## 2. Expected values, measured before implementing

| target | cpe | addr | unadd | members | files | = total |
| --- | --- | --- | --- | --- | --- | --- |
| `nix-closure-moat` | 6 | 162 | **360** | **1,087** | 0 | 1,615 |
| `haskell-language-server` | 0 | 364 | 6 | 0 | **29** | 399 |
| `haskell-security-advisories` | 0 | 248 | 3 | 0 | 0 | 251 |
| `image-postgres16` | 145 | 0 | 1 | 0 | **117** | 263 |
| `haskell-aeson` | 0 | 61 | 0 | 0 | 2 | 63 |
| `rust-ripgrep` | 61 | 0 | 0 | 0 | **7** | 68 |
| `maven-guice` | 46 | 0 | 2 | 0 | 13 | 61 |
| `go-opentelemetry` | 333 | 0 | 0 | 0 | 1 | 334 |
| `npm-express` | 1 | 43 | 0 | 0 | 1 | 45 |
| `pants-clojure-polyglot` | 51 | 0 | 4 | 0 | 0 | 55 |
| `pants-example-django` | 33 | 12 | 1 | 0 | 1 | 47 |
| `pants-example-javascript` | 303 | 0 | 0 | 0 | 0 | 303 |
| `pants-example-jvm` | 27 | 0 | 1 | 0 | 1 | 29 |
| `pants-example-python` | 10 | 0 | 1 | 0 | 1 | 12 |
| `pants-example-golang` | 4 | 0 | 0 | 0 | 2 | 6 |
| `python-flask` | 108 | 0 | 0 | 0 | 0 | 108 |
| `go-cobra` | 8 | 0 | 0 | 0 | 0 | 8 |

**The sum closes on 17 of 17.** These are the expectation, not the contract;
a task records the implementation's own figures and a mismatch is a finding.

**aarch64-darwin differs and is not in this table.** There the row reads
`78 / 166 / 194 / 539 / 0 = 977`. The two hosts resolve different closures —
linux carries **twice** the members — and quoting one against the other is
the error milestone 1366 recorded once already.

---

## 3. What does not change

| | |
| --- | --- |
| any component's `purl`, `cpe`, `cpes` | **unchanged** (FR-006) |
| `with_candidates`, on every target | **unchanged** (FR-004) |
| `synthesize_cpes`, `GENERIC_LIBRARY_CPES`, `OSV_INDEXED_PURL_TYPES` | **unchanged** |
| the three carriers | **unchanged** |
| a scan with no closure | **one differing path** — the C209 value. `excluded_members` is 0; `excluded_files` need not be (`rust-ripgrep` has 7) |

> **`addressable_without_cpe` is not on that list**, and the omission is
> deliberate. It falls 163 → 162 on `nix-closure-moat` because one member
> there, `pkg:hackage/time` with role `fragment`, carries an OSV-addressable
> PURL. It is unchanged everywhere else. The spec asserted this count could
> not move until planning measured the linux golden.

---

## 4. Blast radius

| | files |
| --- | --- |
| local goldens | **33** regenerate |
| corpus goldens | **48**, across 16 of 17 targets |

`image-postgres16` carries no C209 at all and stays excluded for #1372.

> A `grep -rl` over the local golden directory reports **66**; half are
> `*.actual.json` diagnostics rather than tracked goldens. Counting those made
> a complete regeneration look half-applied in milestone 1373.

---

## 5. CLI

**No new flags.** The row is emitted on every scan and is not optional.
