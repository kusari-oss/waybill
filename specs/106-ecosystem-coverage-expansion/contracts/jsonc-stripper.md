# Contract: JSONC comment stripper (US2 helper)

**New module**: `mikebom-cli/src/scan_fs/package_db/npm/jsonc.rs`

Standalone helper used by `bun_lock.rs` (FR-003) and potentially future readers for JSONC-format files (`tsconfig.json` if ever added, GitHub Actions `dependabot.yml` overrides, etc.).

## Public API

```rust
/// Strip C-style line comments (`//`) and block comments (`/* */`) from
/// a JSONC source string, returning a String suitable for passing to
/// `serde_json::from_str`.
///
/// String-literal contents are preserved verbatim — comments inside
/// `"..."` strings are NOT stripped.
///
/// Newlines from `//` and `/* */` are preserved as `\n` so line/column
/// info in serde_json error messages still points at correct positions.
pub fn strip_comments(input: &str) -> String
```

## Behavior

State machine over the input characters:

| State | Trigger | Next state | Output |
|---|---|---|---|
| `Normal` | `"` | `InString` | `"` |
| `Normal` | `//` | `LineComment` | (drop) |
| `Normal` | `/*` | `BlockComment` | (drop) |
| `Normal` | (any other char) | `Normal` | char |
| `InString` | `\"` (escaped) | `InString` | `\"` |
| `InString` | `\` followed by anything | `InString` | (keep both chars) |
| `InString` | `"` | `Normal` | `"` |
| `InString` | (any other char) | `InString` | char |
| `LineComment` | `\n` | `Normal` | `\n` (preserve for line info) |
| `LineComment` | (any other char) | `LineComment` | (drop) |
| `BlockComment` | `*/` | `Normal` | (drop) |
| `BlockComment` | `\n` | `BlockComment` | `\n` (preserve) |
| `BlockComment` | (any other char) | `BlockComment` | (drop) |

## Test cases (10 unit tests)

1. **`strip_line_comment_basic`**: `"// foo\nbar"` → `"\nbar"`.
2. **`strip_block_comment_basic`**: `"a /* x */ b"` → `"a  b"`.
3. **`preserves_strings`**: `r#""// not a comment""#` → unchanged.
4. **`preserves_strings_with_block_marker`**: `r#""/* still string */""#` → unchanged.
5. **`escaped_quote_in_string`**: `r#""he said \"hi\""//comment"#` → `r#""he said \"hi\"""#`.
6. **`multiline_block_preserves_newlines`**: `"a /* line1\nline2 */ b"` → `"a \n b"` (newline preserved for serde error positions).
7. **`unterminated_block_comment`**: `"/* unterminated"` → `""` (eats to EOF, no panic).
8. **`top_of_file_bun_marker`**: `"// bun: lockfileVersion: 1\n{}"` → `"\n{}"`.
9. **`adjacent_comment_types`**: `"// line\n/* block */text"` → `"\ntext"`.
10. **`empty_input`**: `""` → `""`.

## Edge cases handled

- **Unicode in strings**: bytes are passed through verbatim — the helper operates on `char` iteration over the `&str`. UTF-8 boundaries are respected by the underlying `Chars` iterator.
- **Trailing `/` at EOF**: if the input ends with `/` (not followed by `/` or `*`), the `/` is preserved as output (it's not a comment start).
- **String ends at EOF**: unterminated string literal → state stays `InString` to EOF; characters are passed through as-is (the downstream `serde_json` parse will produce a useful error).

## Rationale for from-scratch implementation

Per research R1, no existing JSONC handler is in-tree. Adding `serde_jsonc` or `json5` as a Cargo dep was considered and rejected per the workspace's no-new-deps posture for milestone 106. The ~20-LOC + 10-test helper is cheaper than dep churn and follows the proven patterns of `gem::strip_ruby_comment` (line comments) and `golang::legacy::strip_line_comment` (line comments) — this helper extends those patterns with block-comment + string-boundary awareness.
