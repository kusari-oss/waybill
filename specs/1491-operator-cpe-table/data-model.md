# Data Model: Operator-supplied CPEs, and the curated CPE table as data

## CpeTable (built-in and operator)

One TOML document. The built-in file is `waybill-cli/src/generate/cpe_table.toml`. An operator file passed with `--cpe-map` has the same schema.

| Field | Type | Built-in | Operator | Rule |
|---|---|---|---|---|
| `row` | array of `CpeRow` | ≥ 1 | ≥ 1 | Keys are unique. The built-in file is sorted by `key` (byte order). |

### CpeRow

| Field | Type | Rule |
|---|---|---|
| `key` | string | A versionless PURL: no `@version`, qualifiers or subpath, and it parses with `Purl::new`. Built-in rows are all `pkg:generic/<slug>`. |
| `pair` | array of `CpePair`, ≥ 1 | The first is the primary; the rest are secondary. Order is emission order. |

### CpePair

| Field | Type | Built-in | Operator | Rule |
|---|---|---|---|---|
| `vendor` | string | required | required | Non-empty, no `*` or `?`, and `format_cpe(vendor, product, "1.0")` parses as `Cpe`. |
| `product` | string | required | required | Same rule as `vendor`. |
| `dictionary_entries` | integer | required | optional | > 0 (FR-013). The file at `dictionary_evidence` holds the pair at exactly this count. |
| `observed` | date `YYYY-MM-DD` | required | optional | The date the evidence records. |
| `dictionary_evidence` | path | required | optional | Relative to the repository root, and the file exists. |
| `title_evidence` | path | required | optional | Exists, and contains `=== <product> (`. |

There is deliberately no field for CVE counts or advisories (FR-011, research R10).

**Shared pairs.** A pair that appears in two rows (only `python:python` today, in the `python3` and `python3-minimal` rows) carries identical evidence fields in both. A test asserts that.

### Validation failures

For an operator file, each failure is a usage error raised before scanning (FR-006):
- the file cannot be read, or is not TOML;
- an unknown field, a missing required field or a duplicate `key`;
- a key that is not a versionless PURL;
- a pair that does not form a valid `Cpe`.

For the built-in file, the same failures are test failures (R3).

## Operator marker (internal)

`waybill:operator-cpe` is an internal, never-emitted annotation with value `root-cpe`, `supplement` or `cpe-map`. Every CPE pass that runs after it treats the component's `cpes` as final (research R5).

## CpeSource (moved to `generate/cpe_source.rs`)

```
CpeSource = Operator(OperatorSurface) | NixpkgsDeclared | Curated
OperatorSurface = RootCpe | Supplement | CpeMap
```

The source is resolved in this order: the operator marker, then C212 with a `drv`/`name-version` basis, then the curated marker, then `None`.

## C212 `waybill:cpe-provenance` (extended)

| `basis` | Source | Emitted on |
|---|---|---|
| `drv`, `name-version` | nixpkgs-declared (unchanged) | closure components |
| `purl` | `--supplement-cdx` | a component the supplement set, when it displaced a different primary |
| `cpe-map` | `--cpe-map` | a component a row matched, when it displaced a different primary |
| `root-cpe` | `--root-cpe` | the subject, when it is a scanned main module that had a different primary |

`overrode` is the primary CPE the component would have emitted without this source. It is omitted when equal or absent; for an operator basis, C212 is then omitted entirely (FR-009, amended 2026-11-13).

## Precedence (FR-005)

```
root-cpe (subject) > supplement (exact PURL) > cpe-map row > nixpkgs-declared > curated row > ecosystem heuristic
```

The winning source supplies the whole list.
