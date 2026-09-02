# Contract — PEX-lockfile content detection

**Feature**: 673-pants-lockfile-layouts
**Applies to**: `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs::is_pex_lockfile_content`

## Purpose

Discriminate PEX lockfiles (Pants pex-lockfile shape) from unrelated
`.lock` files (Cargo, Poetry, bun, npm, pnpm, unknown) at the
byte-level, without paying the full-schema parse cost. Used by
the wide-scope FR-001/FR-002 discovery paths in the m673 Pants
reader to avoid false-positive parses on repo-root and `lockfiles/`.

## Function signature

```rust
pub(crate) fn is_pex_lockfile_content(bytes: &[u8]) -> bool;
```

- **Input**: raw file bytes as returned by `std::fs::read`.
- **Output**: `true` iff the file is a valid PEX 2.x lockfile shape;
  `false` for any parse failure, missing field, wrong-type field,
  or wrong-version prefix.

## Behavioral contract

### C1 — Accept criterion (positive-case acceptance)

`is_pex_lockfile_content(bytes)` MUST return `true` iff:

1. After `strip_pants_frontmatter(bytes)` (m672 stripper), the
   remaining bytes parse as valid JSON via `serde_json::from_slice::<serde_json::Value>`.
2. The resulting JSON value is a JSON object.
3. The object has a `pex_version` field.
4. The `pex_version` field's value is a JSON string.
5. The string starts with the prefix `"2."`.

### C2 — Version-gate

PEX 1.x lockfiles (`pex_version == "1.9.0"`, etc.) MUST return `false`.
The `^2\.` prefix constraint matches m223's existing accept-criterion
in `parse()` — this keeps m223 + m673 accept-criteria aligned so a
file that passes content-detection also passes the full-schema
parse without a WARN.

### C3 — Non-JSON reject

Files whose bytes (after `//`-frontmatter stripping) fail
`serde_json::from_slice::<serde_json::Value>` MUST return `false`.
This handles:

- Cargo lockfiles (TOML, top-level `[metadata]`, invalid JSON at byte 0).
- Poetry lockfiles (TOML, invalid JSON).
- bun.lock files (JSONC — bun supports single-line comments in lockfiles;
  `serde_json` rejects them without an extension parser).
- Binary files (compiled artifacts with `.lock` extension).
- Empty files.

### C4 — Empty-input reject

`is_pex_lockfile_content(&[])` MUST return `false`. The empty slice
fails `serde_json::from_slice` with a `serde_json::Error` (EOF).

### C5 — Missing-field / wrong-type reject

Valid JSON without a `pex_version` field, OR with `pex_version` at
a non-string type (integer, null, object, array, boolean), MUST
return `false`.

**Illustrative examples**:

- `{}` (empty object) → `false` (missing field).
- `{"pex_version": 2}` (integer) → `false` (wrong type).
- `{"pex_version": null}` → `false` (wrong type).
- `{"pex_version": ["2.10"]}` (array) → `false` (wrong type).
- `{"pex_version": "2.10.0"}` → `true` (accept).

### C6 — `//`-frontmatter interop (m672)

Content-detection reuses the m672 `strip_pants_frontmatter` helper.
A file with the pre-Pants-2.30 `//`-comment metadata block PLUS a
valid PEX body MUST return `true` — the stripper removes the block
before the JSON parse.

### C7 — Purity + performance

The function MUST be pure — no allocation beyond the parse buffer,
no error path (returns `bool` only), no persistent state. Complexity
is O(file-size) linear parse. Sub-millisecond on non-JSON rejects
(parse errors early); < 5 ms on real PEX shapes (200 KB average).

## Test matrix

| Input | Expected return | Contract clause |
|---|---|---|
| `{"pex_version":"2.10.0","locked_resolves":[]}` (clean PEX) | `true` | C1 accept |
| `// ---\n{"pex_version":"2.10.0"}` (PEX with `//`-frontmatter) | `true` | C1 + C6 stripper interop |
| `{"pex_version":"2.0.0-rc.1"}` (PEX 2.0 pre-release) | `true` | C1 accept (prefix match) |
| `{"pex_version":"1.9.0"}` (Pex 1.9) | `false` | C2 version-gate |
| `{"pex_version":"3.0.0"}` (future Pex 3.x — not yet real) | `false` | C2 (prefix `^2\.` only) |
| Cargo.lock (TOML `[[package]]`) | `false` | C3 non-JSON reject |
| Poetry.lock (TOML `[metadata]`) | `false` | C3 non-JSON reject |
| bun.lock (JSONC with `//` comments interspersed AFTER the first non-comment byte) | `false` | C3 non-JSON reject (bun's `//` inside the body — different position from Pex frontmatter) |
| `` (empty file) | `false` | C4 empty-input reject |
| `{}` (empty JSON object) | `false` | C5 missing-field |
| `{"pex_version": 2}` (integer) | `false` | C5 wrong-type |
| `{"pex_version": null}` | `false` | C5 wrong-type |
| `[]` (JSON array at top) | `false` | C5 (top-level not an object; `.get("pex_version")` returns `None`) |
| `{"pex_version":"2.10.0","corrupted":` (unterminated JSON) | `false` | C3 non-JSON reject |
| Binary garbage (`\xff\xff\xff\xff`) | `false` | C3 non-JSON reject |
| `// only comments\n// no json body` (fully-commented file) | `false` | m672 stripper returns empty slice → C4 |

## Non-goals

- **NOT a full PEX schema validator**. The function only checks the
  presence + type + prefix of `pex_version`. Downstream `PexLockfile`
  deserialization catches any deeper schema drift and WARNs — but
  that's m223 behavior, not m673's concern.
- **NOT a version-comparison utility**. Only the string-prefix match
  is used. If a future Pants version emits `pex_version = "2.11.0"`,
  it accepts automatically. If it ever emits `pex_version = "3.0.0"`,
  m673 rejects → a future m*** would need to extend the prefix set.
- **NO logging inside the function**. Detection is a pure decision
  procedure; logging (or not) belongs to the caller (per FR-004
  silent-skip contract).
