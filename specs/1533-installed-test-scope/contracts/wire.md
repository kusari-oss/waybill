# Wire contract: test-scoped components and C226

## A test-scoped component

| | CycloneDX 1.6 | SPDX 2.3 | SPDX 3.0.1 |
|---|---|---|---|
| Scope | `scope: "excluded"` (existing m052 path) | none native | none on the element (existing lifecycle relationships are unchanged) |
| C42 `waybill:lifecycle-scope` | property `test` | annotation `test` | annotation `test` |
| C62 `waybill:lifecycle-scope-derivation` | property, one of the three values below | annotation | annotation |
| Purpose | `type` unchanged (A13) | `primaryPackagePurpose` unchanged (A13) | `software_primaryPurpose: "test"`; the A13 role, if any, moves to `software_additionalPurpose: ["<role>"]` |

**C62's new values.** C62 is an open enum that already holds `test-only-closure`, `go-mod-why` and `transitive`. This feature adds:
- `exported-targets-test-package`
- `ctest-declaration`
- `test-directory`

C62's catalog text is extended. It is not a new row.

**A13, SPDX 3.** When `software_primaryPurpose` is `test`, the role is read from `software_additionalPurpose`. The A13 SPDX 3 extractor is updated so that the cross-format role comparison still holds for scoped binaries.

**Unscoped components** are byte-identical to today's in all three formats.

## C226 `waybill:test-scope-coverage` (document scope)

A JSON object encoded as a string with sorted keys. It has the same carriers as C225:
- CycloneDX: `metadata.properties`;
- SPDX 2.3: a document annotation;
- SPDX 3: an annotation on the document.

```json
{"declarations":{"ctest":12,"exported-targets":1,"owned-by-os-package":0},
 "installed-tree":"recognised",
 "locations":{"not-shipped":0,"outside-root":0,"resolved":180,"unresolved":3},
 "scoped":{"ctest-declaration":160,"exported-targets-test-package":20,"test-directory":4000}}
```

- **`declarations`**: declaring files read, by kind, plus those skipped as OS-owned.
- **`installed-tree`**: whether path and name rules ran (FR-015).
- **`locations`**: the declared locations, by outcome.
- **`scoped`**: components scoped, by the C62 value recorded.

**Presence.** Emitted only when a declaration was read or a component was scoped. Absent otherwise (FR-011).

**Why a new row rather than C225 or C223** (FR-010):
- C225 is emitted only when installed-library records exist. An image scan with test directories and no `.pc` or version file would have nowhere to report.
- C223 is a decimal counter with a fixed meaning, the binaries a *directory* scan skipped. Turning it into an object breaks its consumers.
