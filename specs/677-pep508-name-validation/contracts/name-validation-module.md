# Contract — `name_validation` module

**Module**: `waybill-cli/src/scan_fs/package_db/name_validation.rs` (new)

## Purpose

Reader-agnostic name validation for waybill's ecosystem readers. First cut wires the pip reader with PEP 508 validation to reject phantom `pkg:pypi/*` components sourced from template directories (Cookiecutter, Cruft, Copier, etc.) per issue #768 and Constitution Principle IX.

## Public API

All items are `pub(crate)` — audience is `waybill-cli`-internal readers.

```rust
pub(crate) enum NameValidationError {
    Empty,
    Malformed { reason: String },
}

impl std::fmt::Display for NameValidationError;

/// PEP 508 predicate (boolean). See PEP 508 §"Names".
pub(crate) fn is_pep508_name(name: &str) -> bool;

/// PEP 508 structured validator. Returns `Ok(())` on match, or a
/// `NameValidationError` carrying the failure reason for structured logging.
pub(crate) fn validate_pep508_name(name: &str) -> Result<(), NameValidationError>;
```

## Semantic contract

### `is_pep508_name`

Returns `true` iff `name` matches the regex `^[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?$` (case-preserved character class matches both A-Z and a-z explicitly; no regex-flag toggle needed).

Semantic properties:

- **Empty string** → `false`
- **Whitespace-only** → `false`
- **Single alphanumeric char** (`x`, `X`, `0`) → `true`
- **Alphanumeric with interior `.`/`-`/`_`** (`my-pkg`, `my.pkg`, `my_pkg`, `zope.interface`) → `true`
- **Starts or ends with separator** (`.pkg`, `pkg-`) → `false`
- **Contains any non-`[A-Za-z0-9._-]` character** (`{{package-name}}`, `pkg@2`, `pkg name`) → `false`
- **Mixed-case is preserved** (`Django`, `PyYAML` accepted; PEP 508's case-insensitivity requirement is about equality-comparison downstream, not the character-class validity itself)
- **Result is deterministic** — no environment/RNG/timestamp dependency

### `validate_pep508_name`

Returns:

- `Ok(())` if `is_pep508_name(name)` returns `true`
- `Err(NameValidationError::Empty)` if `name.trim().is_empty()`
- `Err(NameValidationError::Malformed { reason })` otherwise, where `reason` is one of:
  - `"must start with alphanumeric character"` — first char is non-alphanumeric AND name is non-empty AND not whitespace-only
  - `"must end with alphanumeric character"` — last char is non-alphanumeric AND first char is alphanumeric
  - `"contains invalid character(s); allowed: A-Z a-z 0-9 . - _"` — otherwise (interior invalid char)

Reason-selection order is most-specific to least-specific: check start, then end, then interior. First match wins.

### Idempotency and thread-safety

Both functions are pure and thread-safe. `OnceLock`-backed regex compile is race-safe by construction.

## Testing contract

Unit tests live in `#[cfg(test)] mod tests { ... }` inside the module. Covering:

| # | Input | `is_pep508_name` | `validate_pep508_name` |
|---|---|---|---|
| 1 | `"django"` | `true` | `Ok(())` |
| 2 | `"Django"` | `true` | `Ok(())` |
| 3 | `"PyYAML"` | `true` | `Ok(())` |
| 4 | `"my-pkg"` | `true` | `Ok(())` |
| 5 | `"my.pkg"` | `true` | `Ok(())` |
| 6 | `"my_pkg"` | `true` | `Ok(())` |
| 7 | `"zope.interface"` | `true` | `Ok(())` |
| 8 | `""` | `false` | `Err(Empty)` |
| 9 | `"   "` | `false` | `Err(Empty)` |
| 10 | `"{{package-name}}"` | `false` | `Err(Malformed { reason: "must start with alphanumeric character" })` |
| 11 | `".pkg"` | `false` | `Err(Malformed { reason: "must start with alphanumeric character" })` |
| 12 | `"pkg-"` | `false` | `Err(Malformed { reason: "must end with alphanumeric character" })` |
| 13 | `"pkg@2"` | `false` | `Err(Malformed { reason: "contains invalid character(s); allowed: A-Z a-z 0-9 . - _" })` |
| 14 | `"pkg name"` | `false` | `Err(Malformed { reason: "contains invalid character(s); allowed: A-Z a-z 0-9 . - _" })` |

## Non-goals

- **NOT a PURL parser** — this module validates a bare name, not a complete PURL string. PURL validation lives in `waybill-common::types::purl::Purl::new`.
- **NOT case-normalization** — the module accepts case-preserved names. PyPI-canonical-form normalization is handled by `normalize_pypi_name_for_purl` at `pip/mod.rs:99` for PURL emission (separate concern per Constitution Principle V — standards-native comparison is downstream).
- **NOT a general PEP 508 parser** — validates the NAME segment only. Full PEP 508 requirement-string parsing (`pkg[extras]>=1.0; marker`) is handled by existing `tokenise_requires_dist_name`.
- **NOT wired to non-pip readers in this feature** — FR-005 anticipates extension; this feature ships pip-only per spec scope. Adding npm/maven/etc. sibling functions is follow-up work.

## Extension pattern for future readers

To add npm validation in a future milestone:

```rust
// Same file, npm follow-up:
pub(crate) fn is_npm_name(name: &str) -> bool { ... }
pub(crate) fn validate_npm_name(name: &str) -> Result<(), NameValidationError> { ... }
```

The pip integration pattern is a template: reader's `read()` adds a `filter_project_roots_by_name` (or reader-specific equivalent) pre-pass calling the new validator, drops rejected manifests, logs one WARN per drop, extends the reader-complete log with `names_rejected=<N>`.

Cross-reader reuse of the filter pass itself is not required by FR-004 — each reader can copy the ~15-line filter loop against its own validator. Cross-reader reuse of the VALIDATION MODULE (this module) is the FR-004 obligation.
