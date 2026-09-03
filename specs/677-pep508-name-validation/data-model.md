# Data Model — PEP 508 name validation for pip reader

## Entity 1 — `NameValidationError` enum (new type)

**File**: `waybill-cli/src/scan_fs/package_db/name_validation.rs` (new)

**Type**: enum with two variants surfacing the failure reason.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameValidationError {
    /// Name is empty or contains only whitespace.
    Empty,
    /// Name contains characters or shape outside the ecosystem's regex.
    /// The `reason` field carries a human-readable message for the WARN log.
    Malformed { reason: String },
}

impl std::fmt::Display for NameValidationError { ... }
```

**Design notes**:

- `String` (not `&'static str`) on `Malformed.reason` — the reason may include the offending name substring or position, which is dynamic.
- No `#[derive(thiserror::Error)]` — the reader-error convention in `waybill-cli` reserves `thiserror` for propagated errors; this type is captured at the call site and never propagated as a `?`-return.

## Entity 2 — `is_pep508_name` predicate (new function)

**File**: `waybill-cli/src/scan_fs/package_db/name_validation.rs`

**Signature**:

```rust
pub(crate) fn is_pep508_name(name: &str) -> bool;
```

**Implementation** (Phase 1 sketch):

```rust
use std::sync::OnceLock;
use regex::Regex;

static PEP508_NAME_RE: OnceLock<Regex> = OnceLock::new();

pub(crate) fn is_pep508_name(name: &str) -> bool {
    let re = PEP508_NAME_RE.get_or_init(|| {
        Regex::new(r"^[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?$")
            .expect("valid PEP 508 name regex")
    });
    re.is_match(name)
}
```

**Design notes**:

- `OnceLock` compile-once — matches the pattern used pervasively in the pip reader for regex helpers.
- Regex includes `A-Z` explicitly to match case-insensitively without regex flags — clearer intent, one compile-time invariant.
- Anchored (`^...$`) so partial matches (e.g., `{{package-name}} something`) fail.

## Entity 3 — `validate_pep508_name` structured validator (new function)

**File**: `waybill-cli/src/scan_fs/package_db/name_validation.rs`

**Signature**:

```rust
pub(crate) fn validate_pep508_name(name: &str) -> Result<(), NameValidationError>;
```

**Behavior**:

- Empty or whitespace-only → `Err(NameValidationError::Empty)`
- Regex match → `Ok(())`
- Regex miss → `Err(NameValidationError::Malformed { reason: ... })`

Where `reason` is a short human-readable string:

- Names starting with non-alphanumeric: `"must start with alphanumeric character"`
- Names ending with non-alphanumeric: `"must end with alphanumeric character"`
- Names containing invalid characters: `"contains invalid character(s); allowed: A-Z a-z 0-9 . - _"`

**Design notes**:

- The three reason strings are ordered from most-specific to least-specific — the first matching diagnostic wins.
- The `reason` is what shows up in the WARN log's `reason=` field per FR-002.

## Entity 4 — `filter_project_roots_by_name` filter pass (new function)

**File**: `waybill-cli/src/scan_fs/package_db/pip/mod.rs`

**Signature**:

```rust
/// Feature 677: pre-filter project_roots — drops any project_root whose
/// `pyproject.toml` has a `[project].name` (or `[tool.poetry].name`
/// fallback per m670 T005) that fails PEP 508 validation. Logs one WARN
/// per rejected manifest with structured fields. Returns:
///   - Vec of valid roots (used by downstream loops)
///   - Count of rejected manifests (for reader-complete log's names_rejected)
fn filter_project_roots_by_name(project_roots: &[PathBuf]) -> (Vec<PathBuf>, usize);
```

**Behavior**:

- Iterates `project_roots`. For each root:
  - Read `pyproject.toml` (silent skip if missing / unparseable — existing behavior; not this fix's concern).
  - Extract name using the same logic as `build_pip_main_module_entry` (line 644-651 pattern): `[project].name` first, `[tool.poetry].name` fallback if `[project]` absent, otherwise skip validation entirely (no name to validate → let existing downstream logic handle it).
  - Call `validate_pep508_name(name)`.
  - On `Ok`: retain the root.
  - On `Err`: log WARN with `manifest`, `name`, `reason` fields; increment counter; drop.
- Return `(retained_roots, rejected_count)`.

**Placement in `read()`**: Immediately before the `pyproject_declared_deps` loop at line ~344. Wires as:

```rust
let (project_roots, names_rejected) = filter_project_roots_by_name(&project_roots);
```

Rebinding `project_roots` means both existing downstream loops (line 344 and line 388) iterate the filtered set with zero further modification.

**Design notes**:

- Function is `fn`, not `pub(crate) fn` — private to `pip/mod.rs`.
- Reads the pyproject.toml text a second time (once in the filter, once in each downstream function). Acceptable per SC-007's 200-line-diff scope — refactoring to share the parsed toml across all three sites would triple the diff.
- Consumed by exactly one call site (the `read()` function).

## Entity 5 — `pip reader complete` log addition (modified log)

**File**: `waybill-cli/src/scan_fs/package_db/pip/mod.rs`

**Before**:

```rust
tracing::info!(
    main_modules_emitted,
    poetry_skips,
    ...
    "pip reader complete"
);
```

**After**:

```rust
tracing::info!(
    main_modules_emitted,
    poetry_skips,
    names_rejected,
    ...
    "pip reader complete"
);
```

**Non-regression**: The log line's shape is EXTENDED (new field added), not restructured. Existing consumers reading `main_modules_emitted=<N>` continue to parse it correctly.

## Entity 6 — Test fixture (new)

**Path**: `waybill-cli/tests/fixtures/pip/malformed_name_placeholder/pyproject.toml`

**Content**:

```toml
[project]
name = "{{package-name}}"
version = "0.0.0"
dependencies = [
    "waybill-fixture-real-dep-1",
    "waybill-fixture-real-dep-2",
]
```

## Entity 7 — Integration test (new)

**File**: `waybill-cli/tests/scan_python_m677.rs`

Single test asserting:

1. Scan of the malformed-name fixture emits **zero** components (SC-001 + Session 2026-09-03 Q1 whole-manifest reject).
2. Scan stderr contains **exactly one** WARN line naming the offending `pyproject.toml` path and the malformed name (SC-002).
3. Scan's reader-complete log reports `names_rejected=1` (SC-003).
4. **Sanity check**: no `pkg:pypi/waybill-fixture-real-dep-*` components emit either — confirms whole-manifest reject drops declared deps too.

Test uses the same subprocess-invocation pattern as `scan_python_m670.rs` and neighboring pip integration tests.

## Entity 8 — Unit tests inside `name_validation.rs` (new)

Per FR-007's clauses (a)–(i):

- (a) `is_pep508_name("django")` → true
- (b) `is_pep508_name("{{package-name}}")` → false
- (c) `is_pep508_name("")` → false (also: `validate_pep508_name("")` → `Err(Empty)`)
- (d) `is_pep508_name("   ")` → false (also: `validate_pep508_name("   ")` → `Err(Empty)`)
- (e) `is_pep508_name(".pkg")` → false
- (f) `is_pep508_name("pkg-")` → false
- (g) `is_pep508_name("Django")` → true; `is_pep508_name("PyYAML")` → true
- (h) `is_pep508_name("my.pkg")` + `is_pep508_name("my-pkg")` + `is_pep508_name("my_pkg")` → all true
- (i) *[integration test level]* whole-manifest reject verified in `scan_python_m677.rs`

## No state transitions

The helper module is stateless (module-level `OnceLock` for regex compile-once is inert state). The filter function operates on a slice input and returns owned output.
