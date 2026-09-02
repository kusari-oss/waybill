# Contract — Pex-lockfile `//`-comment front-matter stripper

**Feature**: 672-pants-reader-follow-up
**Applies to**: `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs::strip_pants_frontmatter`

## Purpose

Recover the JSON body from Pants ≤ 2.29 lockfile files that prepend a
`//`-comment metadata block. The block is opaque to waybill; the
stripper skips it so `serde_json::from_slice` sees the JSON body.

## Function signature

```rust
fn strip_pants_frontmatter(bytes: &[u8]) -> &[u8];
```

- **Input**: raw file bytes as returned by `std::fs::read`.
- **Output**: subslice of the input starting at the first non-`//`
  non-whitespace line. May be empty if the entire input was `//`
  comments.

## Behavioral contract

### C1 — Consecutive-`//`-line skip (FR-001)

Every consecutive line at the start of `bytes` whose first
non-whitespace character is `//` MUST be skipped. Skipping stops at
the first line whose first non-whitespace character is NOT `//`.

**Line definition**: a byte sequence terminated by `\n`. Windows-shape
`\r\n` line endings are handled by treating `\r` as whitespace when
checking the `//` prefix — the trailing `\r\n` naturally lands the
line-terminator scan on the `\n`.

**Whitespace**: space (0x20) and tab (0x09) only. Other whitespace
bytes (form feed, vertical tab) are treated as non-whitespace
non-`/`, which terminates the strip loop.

### C2 — No content interpretation (FR-002)

The stripper MUST NOT interpret the `//` block's contents. Whatever
the comments say (Pants version, invalidation hashes, requirement
lists) is discarded verbatim.

### C3 — Prefix-only bounded scan (FR-003)

The stripper MUST run in O(prefix-length) time — proportional to the
length of the `//`-comment block, NOT the total file length. On
clean-JSON input (first non-whitespace byte is `{`), the function
returns after examining at most one line.

### C4 — Idempotence on clean input

`strip_pants_frontmatter(clean_json)` MUST return a slice that
`serde_json::from_slice` accepts iff `serde_json::from_slice(clean_json)`
would accept it. The stripper is a no-op preserving the input byte-slice
identity on clean-JSON files (implementation may return the exact input
slice via `&bytes[..]`).

### C5 — Uniform invocation (FR-001 + clarify Q3)

Every parse attempt in the reader MUST route through `strip_pants_frontmatter`
before `serde_json::from_slice`. There is NO retry-on-failure pattern —
the stripper runs unconditionally.

### C6 — Fully-commented input

If every line in the input starts with `//` (after optional
whitespace), `strip_pants_frontmatter` MUST return `&[]`. Downstream,
`serde_json::from_slice(&[])` fails with the standard EOF-error, which
the m223 fail-open contract handles (WARN + skip).

### C7 — Embedded `//` in JSON strings preserved

Any `//` bytes that appear AFTER the first non-`//` line are
untouched. The stripper is bounded to the leading prefix only.

**Illustrative example**:

Input:
```json
// leading comment
// another comment
{"pex_version": "2.10.0", "notes": "// this is inside a JSON string; must survive"}
```

Output (bytes starting at the `{`):
```json
{"pex_version": "2.10.0", "notes": "// this is inside a JSON string; must survive"}
```

## Test matrix

| Input shape | Output | Passes contract |
|---|---|---|
| `{"pex_version":"2.10.0"}` (clean JSON) | Same bytes (C4 idempotence) | C1, C3, C4, C5 |
| `// header\n{"pex_version":"2.10.0"}` | `{"pex_version":"2.10.0"}` | C1, C2, C3 |
| `  // indent\n{"pex_version":"2.10.0"}` | `{"pex_version":"2.10.0"}` | C1 (whitespace tolerated) |
| `\t\t// tabbed\n{"pex_version":"2.10.0"}` | Same as above | C1 (tab tolerated) |
| `// a\n// b\n// c\n{"pex_version":"2.10.0"}` | `{"pex_version":"2.10.0"}` | C1 (multi-line) |
| `// only comments` (no newline, no JSON) | `&[]` | C6 |
| `// hdr\n// hdr\n` (all commented, trailing newline) | `&[]` | C6 |
| `\n\n{"pex_version":"2.10.0"}` (leading blank lines, no `//`) | `\n\n{"pex_version":"2.10.0"}` | Loop bails at first non-`//` line (blank line qualifies) |
| `{"foo": "// not a comment"}` (embedded `//` in string) | Same bytes | C7 |
| `// pants ≤ 2.29 real-world shape` (adopter fixture) | JSON body slice starting at `{` | Real-world happy path |

## Failure modes

The stripper itself has NO failure path — it always returns a slice.
Downstream failures (invalid JSON after stripping) are handled by the
m223 fail-open contract at `contracts/pex-lockfile-schema.md` (WARN
naming the file path + parse error; skip the file; continue scan).

## Performance envelope

- **Clean JSON case**: bounded to `min(first-line-length, file-length)` bytes of scan. On a 100 MB clean lockfile, the stripper reads at most ~4 KB (the average first-line length).
- **`//`-commented case**: bounded to the total length of the leading `//`-comment block. Real-world Pants blocks are < 4 KB.
- **SC-007 budget**: 5 ms overhead vs clean-JSON parse of the same-size file. Comfortably met by the O(prefix-length) bound.
