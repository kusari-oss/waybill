# Contract: `setup.py` + `setup.cfg` static reader

**File**: `waybill-cli/src/scan_fs/package_db/pip/setup_py.rs` + `pip/setup_cfg.rs`
**FRs covered**: FR-006, FR-007, FR-014, FR-015, FR-016
**Called by**: `pip/mod.rs::dispatch` when a `setup.py` or `setup.cfg` is discovered

## Constitutional constraint (Principle I, FR-015)

**Zero Python code execution.** The reader is a static regex + token-balancing scanner. It does NOT:
- Invoke `python`, `python3`, or any interpreter
- Use `rustpython-parser`, `RustPython`, or any embedded Python
- Shell out to `pip`, `setuptools`, or `egg_info`

## `setup.py` reader

### Static-parse strategy

The reader finds the top-level `setup(` call and extracts `install_requires=[<literal list>]` and `extras_require={<literal dict>}` from that call's argument list.

**Steps**:

1. Load file as UTF-8 (fallback: latin-1) string; if invalid, warn+skip
2. Regex-locate the top-level `setup(` call: `\bsetup\s*\(` at zero indentation
3. Bracket-balance forward from the `(` to find the matching `)`
4. Within that region, regex-locate `install_requires\s*=\s*\[` — find the matching `]`
5. Within the list, extract each `"..."` or `'...'` literal (skip variable references, function calls, f-strings, concatenations)
6. Same for `extras_require\s*=\s*\{` — extract `"key"` → `[list of strings]` map, one entry per extra

### Emission

For each extracted string like `"requests>=2.28"`:
- Parse as PEP 508 line (reusing `requirements_txt.rs::parse_pep508_line`)
- Emit as `pkg:pypi/<name>@<version-or-unresolved>` with `Main` scope (or the extras-key name for extras)

If `setup(install_requires=...)` uses a variable / function-call / f-string:
- `tracing::debug!("setup.py at {} uses dynamic install_requires; skipping declared deps", path.display())`
- Emit ONLY the main-module component from `name=` / `version=` at the setup() call (if extractable as literals)
- FR-006 acceptance scenario 2: safe under-detection, no fabrication

### Not-in-scope for the static parser

- Nested `setup()` calls (rare; only top-level considered)
- `setup_requires`, `tests_require`, `python_requires` — MAY be added in a follow-up; not in v1
- Multiple `setup()` calls in one file — parse only the first at zero-indentation

### Illustrative implementation sketch

```rust
static SETUP_CALL: OnceLock<Regex> = OnceLock::new();
fn setup_call() -> &'static Regex {
    SETUP_CALL.get_or_init(|| Regex::new(r"(?m)^setup\s*\(").unwrap())
}
static INSTALL_REQUIRES: OnceLock<Regex> = OnceLock::new();
fn install_requires() -> &'static Regex {
    INSTALL_REQUIRES.get_or_init(|| Regex::new(r"install_requires\s*=\s*\[").unwrap())
}
static STRING_LITERAL: OnceLock<Regex> = OnceLock::new();
fn string_literal() -> &'static Regex {
    STRING_LITERAL.get_or_init(|| Regex::new(r#""([^"\\]*(?:\\.[^"\\]*)*)"|'([^'\\]*(?:\\.[^'\\]*)*)'"#).unwrap())
}

fn extract_bracket_balanced(input: &str, start: usize, open: char, close: char) -> Option<&str> {
    // stdlib bracket-balance; O(n); handles nested lists / dicts
}
```

## `setup.cfg` reader

Simpler: INI-style file with `[options]` section and `install_requires = ...` multiline scalar.

### Parse strategy

Regex-locate `\[options\]` section header. Scan lines within that section until the next `[section]` or EOF. Look for `install_requires\s*=` followed by multiple indented lines. Each indented line is one dep (PEP 508 shape).

Same for `[options.extras_require]` (INI subsection).

### Emission

Same as setup.py: each PEP 508 line → one component. Extras go under `Optional { scope_name: <extras-key> }`.

## Precedence

If BOTH `setup.py` and `setup.cfg` exist in the same directory (common in legacy projects — the .cfg is the declarative alternative):
- Both readers fire
- m191 reconciler dedups on PURL
- Component-per-source-file evidence preserved

If BOTH `pyproject.toml` and `setup.py` exist (modern-migrated projects):
- Both readers fire
- pyproject.toml `[project.name/version]` is authoritative for the main-module component's identity if present
- setup.py-extracted deps merge with pyproject-declared deps at m191

## Error behavior

- File is not valid UTF-8 (rare, but possible for non-ASCII source) → try latin-1; if that fails, warn+skip
- Regex bracket-balance overflows (malformed file) → warn+skip
- No `setup(` call found → return `Ok(vec![])` (setup.py may just be `from setup_helpers import main; main()`, which is dynamic; safe under-detection)
- **Never** panic

## Annotations emitted

Same catalog rows as requirements reader. Plus:
- `waybill:unresolved-reason = "python-setup-py-dynamic"` — new locked reason string added to the m236 vocabulary for the dynamic-setup.py case

## Test coverage

Unit tests in `pip/setup_py.rs #[cfg(test)] mod tests`:
- `parses_simple_setup_py` — 20-line canonical shape
- `parses_setup_py_with_extras_require`
- `parses_setup_py_with_leading_imports_and_comments`
- `handles_dynamic_install_requires` — variable reference → emits only main-module
- `handles_nested_lists` — `install_requires` with a helper-function call inside → emits only literal-string entries
- `handles_multiple_setup_calls` — only first at column 0 parsed
- `skips_file_without_setup_call`
- `skips_malformed_bracket_structure`

Unit tests in `pip/setup_cfg.rs #[cfg(test)] mod tests`:
- `parses_options_install_requires`
- `parses_options_extras_require`
- `skips_setup_cfg_without_options_section`
- `handles_ini_comments_and_blanks`
