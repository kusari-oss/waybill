# Contract: `--file-inventory-source-shapes=<comma-list>`

**File touched**: `waybill-cli/src/cli/scan_cmd.rs` + `waybill-cli/src/scan_fs/file_tier/source_shape.rs` (NEW)
**FRs covered**: FR-009

## Purpose

Restricts the FR-002 21-extension allowlist to a specified subset under `--file-inventory=source-tree`. Extensions outside the subset get `shape_skipped` (same code path as the default mode).

## Value syntax

Comma-separated list of extension names WITHOUT the leading dot. Case-insensitive.

**Accepted (per FR-002 allowlist)**:

```
py, pyi, c, cc, cpp, cxx, h, hh, hpp, rs, go, java, kt, js, ts, rb, php, cs, swift, m, mm
```

**Examples**:

```
--file-inventory-source-shapes=py               # Python only
--file-inventory-source-shapes=c,h,cpp,hpp      # C/C++ headers + implementation
--file-inventory-source-shapes=py,c,h           # cpython-audit shape
--file-inventory-source-shapes=py,py            # dedup silently → {py}
```

## Value parser

`clap` invokes a `value_parser` closure that calls `source_shape::parse_restriction(&str) -> Result<BTreeSet<SourceShape>, SourceShapeParseError>`.

### Parse steps

1. Split raw value on `,`. Trim whitespace on each token.
2. Reject empty result → `SourceShapeParseError::Empty`
3. For each token: normalize to lowercase, strip a leading `.` if present (defensively — operators may accidentally include it).
4. Lookup via `SourceShape::from_extension(token)`.
5. `None` → `SourceShapeParseError::UnknownExtension { actual: <original-token> }`
6. `Some(shape)` → insert into `BTreeSet<SourceShape>` (dedup by set semantic)
7. Return the populated set.

### Error messages

`SourceShapeParseError::UnknownExtension`:

```
unknown source-shape extension "md"; accepted extensions are:
  py, pyi, c, cc, cpp, cxx, h, hh, hpp, rs, go, java, kt, js, ts, rb, php, cs, swift, m, mm
  (case-insensitive; leading dot optional)
```

`SourceShapeParseError::Empty`:

```
empty --file-inventory-source-shapes value; pass a non-empty comma-separated list
```

## Interaction with `--file-inventory`

**REQUIRED**: the companion flag is only meaningful when `--file-inventory=source-tree` is active. If the operator passes `--file-inventory-source-shapes=<list>` under any other mode value:

```
--file-inventory-source-shapes is only meaningful when --file-inventory=source-tree
  (got: --file-inventory=orphan)
```

Enforced post-parse in `scan_cmd.rs` (clap's `requires`/`conflicts_with` alone don't express this cross-value dependency).

## Emission

The parsed `SourceShapeSet` surfaces in the C156 annotation's `restriction` field:

```json
{
  "mode": "source-tree",
  "restriction": ["c", "h", "py"]
}
```

Set values are lex-sorted (via `BTreeSet` iteration order) for deterministic output.

## Test coverage (mapped to spec SCs)

- Valid subset → correct file-tier component count (SC-006)
- Unknown extension → parse fail with FR-002 allowlist in diagnostic (FR-009)
- Empty value → parse fail (FR-009)
- Duplicate extensions → dedup silently (dedup design decision per data-model.md)
- Restriction flag under wrong mode → parse fail (FR-001)
- C156 annotation carries lex-sorted restriction array (SC-007)
