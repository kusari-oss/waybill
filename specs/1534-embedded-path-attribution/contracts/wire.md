# Wire contract: embedded-path attribution

## An attributed binary

**The file** joins the package component's existing evidence:
- CycloneDX `evidence.occurrences[]`, with its sha256 in `additionalContext`;
- C18 `waybill:source-files` in every format;

both exactly as with spec 1532 (exported targets).

**C227 `waybill:embedded-path-binaries`** goes on the package component: a component-scope JSON array, encoded as a string, sorted by path.

```json
[{"path":"lib/libMIOpen.so.1"},{"path":"lib/libx.so","shared-with":["waybill-fixture-q"]}]
```

| Format | Carrier |
|---|---|
| CycloneDX | component property |
| SPDX 2.3 | package annotation |
| SPDX 3 | annotation on the element |

**Principle V:** there is no native carrier for "this evidence file was attributed through its embedded source paths, not through the package's metadata".

## A bundled component

| | CycloneDX 1.6 | SPDX 2.3 | SPDX 3.0.1 |
|---|---|---|---|
| Identity | `pkg:generic/<name>`, version absent | the existing handling of a versionless component | the existing handling of a versionless component |
| Placement | nested in the binary component's `components[]` | `CONTAINS` from the binary | `contains` from the binary |
| Evidence | `waybill:evidence-kind` = `embedded-source-path`, a new value registered in `BINARY_EVIDENCE_KINDS` (`scan_fs/binary/mod.rs`), which the CycloneDX builder asserts against; `evidence.identity` follows the entry's `ResolutionTechnique`, as for every other binary-tier identity | the existing evidence annotations | the same |

The nesting follows the existing `parent_purl` handling for embedded identities. No new row is needed for the placement. The new `evidence-kind` value is added to that row's value list in `docs/reference/sbom-format-mapping.md`.

## C225, `embedded-paths` object

This extends the document-scope C225 record:

```json
"embedded-paths": {"attributed":N,"header-only":N,"no-versioned-match":N,"read":N,"shared":N,"toolchain-only":N,"with-paths":N}
```

| Key | Counts |
|---|---|
| `read` | binary file components examined: not already attributed, not OS-owned |
| `with-paths` | those carrying at least one absolute source path |
| `attributed` | those attributed to one package |
| `shared` | those attributed to two or more |
| `header-only` | those with only header paths naming a package |
| `toolchain-only` | those with only toolchain paths |
| `no-versioned-match` | those whose compiled-source paths name no versioned package |

**Presence:**
- The object is present only when `read` > 0.
- Absent the object, C225 is byte-identical to today's.
- A scan with no versioned installed-library components emits no C225, as today.

## C228 `waybill:bundled-source-trees` (document scope)

A JSON object encoded as a string with sorted keys:

```json
{"binaries":N,"components":N,"excluded":N}
```

| Key | Counts |
|---|---|
| `binaries` | binaries naming at least one bundled tree |
| `components` | bundled components emitted |
| `excluded` | bundled-tree names dropped by the exclusion list |

It is emitted on every binary scan when any binary names a bundled tree, with or without installed-library components. Absent otherwise.

| Format | Carrier |
|---|---|
| CycloneDX | `metadata.properties` |
| SPDX 2.3 | document annotation |
| SPDX 3 | document annotation |

**Why not C225:** C225 exists only alongside installed-library components, and bundled detection does not depend on them (analysis I1).
