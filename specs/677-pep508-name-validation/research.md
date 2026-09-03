# Research — PEP 508 name validation for pip reader (issue #768)

## R1 — Pip reader emission-point survey

**Question**: Where does the pip reader emit `pkg:pypi/*` main-module components today, and how many emission points does the fix need to gate?

**Investigation**: `grep -rnE 'pkg:pypi/|build_pypi_purl' waybill-cli/src/scan_fs/package_db/pip/` + code-read of `pip/mod.rs::read`, `pip/mod.rs::build_pip_main_module_entry`, and `pip/mod.rs::pyproject_declared_deps`.

**Findings**:

The pip reader has **two functions** that source components from `pyproject.toml`:

1. **`build_pip_main_module_entry`** (`pip/mod.rs:622`) — reads `[project].name` (or `[tool.poetry].name` fallback per m670 T005), builds a `pkg:pypi/*` main-module `PackageDbEntry`. Returns `(Option<PackageDbEntry>, bool)` where `None` = "skip this manifest".

2. **`pyproject_declared_deps`** (`pip/mod.rs:909`) — reads `[project.dependencies]` and `[project.optional-dependencies].*` from the same `pyproject.toml`; returns `Vec<PackageDbEntry>` — one per declared dep name (using `tokenise_requires_dist_name` to extract the head-of-PEP-508-string as the dep name).

Both are called in `pip/mod.rs::read()` from independent loops over `project_roots: Vec<PathBuf>`:

- Line 343-350: `pyproject_declared_deps` loop
- Line 388-434: `build_pip_main_module_entry` loop

**Decision**: **Pre-filter `project_roots`** at the top of `read()` — one filter pass before either loop. Structure:

```rust
let (valid_roots, names_rejected) = filter_project_roots_by_name(&project_roots);
```

Then both existing loops iterate `&valid_roots` instead of `&project_roots`. Whole-manifest reject is achieved with zero refactoring of the two consumer functions.

**Rationale**:

- Both loops read the same manifest; filtering upstream is DRY.
- Exactly one WARN per rejected manifest (FR-002) is naturally guaranteed by the single filter loop.
- `names_rejected` counter is available at the completion-log site (FR-003).
- Non-regression (FR-006): projects whose names pass validation flow into the existing code paths byte-identically — no changes to the emission code itself.

**Alternatives considered**:

- **Inside-function guard**: add a name-validation guard clause at the top of `build_pip_main_module_entry` AND `pyproject_declared_deps`. Rejected — two log sites, two guard clauses, DRY violation. Filter-upstream is strictly cleaner.
- **Skip only main-module**: gate `build_pip_main_module_entry` but let `pyproject_declared_deps` run. Rejected by Session 2026-09-03 Q1 clarification (whole-manifest reject).

## R2 — Helper module location

**Question**: Where should the reusable name-validation helper live? Spec Assumptions parked two options: `waybill-common/` or `waybill-cli/src/scan_fs/`.

**Investigation**: Reviewed existing per-ecosystem helpers in `waybill-cli/src/scan_fs/package_db/pip/mod.rs` (`normalize_pypi_name_for_purl`, `build_pypi_purl_str`, `tokenise_requires_dist_name`) — flat functions with clear naming, all crate-private.

**Findings**: The audience for the helper is `waybill-cli` readers (pip today, npm/maven/gem etc. tomorrow). All readers live in `waybill-cli/src/scan_fs/package_db/`. `waybill-common` is designed to hold **cross-crate types** (ring buffer events, PURL/hash newtypes, resolution structs) — name validation is neither cross-crate nor a shared type; it's a shared REGEX utility.

**Decision**: Helper at **`waybill-cli/src/scan_fs/package_db/name_validation.rs`** — sibling of the per-ecosystem reader directories.

**Rationale**:

- No `waybill-common` touch — Constitution Principle VI keeps `waybill-common` scoped to cross-crate concerns.
- Convenient import path from every reader: `use super::name_validation::is_pep508_name;` (or `use crate::scan_fs::package_db::name_validation::*;`).
- Follows the existing pattern for reader-shared utilities (though most such utilities are currently inline in `pip/mod.rs`; this feature promotes the pattern into a first-class module).

**Alternatives considered**:

- **`waybill-common/src/validation/name.rs`**: rejected — adds a new module to `waybill-common` with no cross-crate consumer. Principle VI premature-modularization guardrail applies.
- **Inline in `pip/mod.rs`**: rejected — violates FR-004 (reusable helper) explicitly. Every future reader adding npm/maven/etc. validation would either duplicate the code or need to import from a pip module, which is worse abstraction than a shared `name_validation` module.

## R3 — Helper API shape

**Question**: Function-taking-predicate vs per-ecosystem functions vs enum-dispatched? Spec FR-005 says "helper MUST accept an ecosystem-specific name predicate (or regex) rather than hard-coding PEP 508".

**Investigation**: Considered three shapes:

1. `validate_name(name: &str, predicate: fn(&str) -> bool) -> Result<(), NameValidationError>` — takes predicate as function pointer
2. `is_pep508_name(name: &str) -> bool` + future sibling `is_npm_name(...)` etc. — flat per-ecosystem functions
3. `validate(name: &str, ecosystem: Ecosystem) -> Result<(), NameValidationError>` — enum dispatch inside the helper

**Findings**:

Shape 2 matches the existing pip helpers' style (`normalize_pypi_name_for_purl`, `tokenise_requires_dist_name` — flat functions with ecosystem-implicit-in-name). Shape 1 is more "reusable helper takes predicate" per FR-005's literal wording. Shape 3 requires importing an `Ecosystem` enum into every call site.

Trade-offs:

| Shape | Reader call site | Adding a new ecosystem | Testability |
|---|---|---|---|
| 1 (predicate function) | `validate_name(name, is_pep508)` | Define a new predicate function, pass at call site | Test each predicate in isolation |
| 2 (per-ecosystem fns) | `is_pep508_name(name)` | Add `is_<ecosystem>_name` sibling function | Test each function directly |
| 3 (enum dispatch) | `validate(name, Ecosystem::Pypi)` | Add enum variant + match arm | Enum-var-based test parameterization |

**Decision**: **Shape 2** — per-ecosystem functions. The module IS the reusable helper (FR-004); each function is the ecosystem-specific instantiation (FR-005).

**Rationale**:

- Matches the existing pip-reader helper style — minimizes cognitive load for readers of the pip reader.
- Each function is trivially testable in isolation without setup.
- Adding a new ecosystem later is a pure-addition (new sibling function) — no touching the existing pip integration or the shared helper's shape.
- Struct-of-fn-pointers or enum-dispatch adds indirection with no observable benefit for the current 1-ecosystem case.

**API surface**:

```rust
// waybill-cli/src/scan_fs/package_db/name_validation.rs

/// Structured failure reason for name validation. Attached to the
/// operator-facing WARN log when a manifest's name is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameValidationError {
    /// Name contains characters outside the ecosystem's regex.
    /// The `reason` field names the specific violation (e.g., "contains `{`").
    Malformed { reason: &'static str },
    /// Name is empty or whitespace-only.
    Empty,
}

/// PEP 508 name predicate. Returns true iff `name` matches
/// `^[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?$` (case-insensitive).
///
/// Reference: PEP 508 (https://peps.python.org/pep-0508/), section
/// "Names". PyPI names must start and end with alphanumeric; interior
/// separators may be `.`, `-`, or `_`.
pub(crate) fn is_pep508_name(name: &str) -> bool;

/// Structured validator variant returning the failure reason.
/// Used by callers that want to log a specific WARN reason string.
pub(crate) fn validate_pep508_name(name: &str) -> Result<(), NameValidationError>;
```

Two functions instead of one — `is_pep508_name` for the fast "yes/no" boolean check, `validate_pep508_name` for the structured error surface. The pip integration will use `validate_pep508_name` (needs the error type for the WARN log's structured field).

**Alternatives considered**:

- **Single function returning `Result<(), NameValidationError>`**: rejected — the `is_*` boolean form has a legitimate use in future non-emitting call sites (e.g., "does this pass validation? if so, do X" without wanting to build the error struct). Two-function shape is trivial code cost.
- **Return just the offending character**: rejected — the WARN log wants a human-readable reason (`"contains '{' at position 0"` is more useful than `Malformed('{')`).

## R4 — PEP 508 regex authority

**Question**: Which regex is canonical for a PyPI package name?

**Investigation**: PEP 508 defines names as:

> The name that follows this specification, in its ASCII form, MUST be non-empty. Names MUST start and end with an ASCII letter or digit. Characters in-between MUST be letters, digits, underscores (`_`), hyphens (`-`), or periods (`.`).

Regex: `^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$` (case-insensitive per PEP 508's "MUST be compared case-insensitively" clause).

PEP 426 has similar language. The `pypa/packaging` Python library's `packaging.utils.canonicalize_name` uses the same regex.

**Decision**: Use the regex `^[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?$` compiled with case-sensitive matching (the character class covers both cases explicitly). Wrap in `OnceLock` following the reader's existing pattern (`waybill-cli/src/scan_fs/package_db/pip/mod.rs` uses `OnceLock` for regex compile-once via `regex::Regex::new`).

**Rationale**:

- PEP 508 is the authoritative source per PyPA governance.
- The regex is trivially correct for the intended reject cases:
  - `{{package-name}}` → fails (starts with `{`)
  - `""` → fails (empty)
  - `"   "` → fails (starts with whitespace, whitespace not in char class)
  - `.pkg`, `pkg-` → fail (must start/end alphanumeric)
  - `Django`, `PyYAML`, `MarkupSafe`, `zope.interface`, `typing_extensions`, `my-pkg` → all pass

**Alternatives considered**:

- **Character-class-only check without regex**: rejected — regex is more expressive for the anchor + interior + boundary structure. `regex` is already a workspace dep.
- **Import `pypa/packaging`-style logic**: N/A — no Rust port of `packaging.utils` in the dependency tree; the regex captures the same semantics in one line.

## R5 — Test fixture design

**Question**: What synthetic fixture reproduces the bug for the integration test (SC-001, SC-002, SC-003)?

**Decision**: Fixture at `waybill-cli/tests/fixtures/pip/malformed_name_placeholder/pyproject.toml`:

```toml
[project]
name = "{{package-name}}"
version = "0.0.0"
dependencies = [
    "waybill-fixture-real-dep-1",
    "waybill-fixture-real-dep-2",
]
```

The `dependencies` list contains valid PEP 508 names (per fixture-synthetic-package-names convention). This is deliberate: the fixture proves the whole-manifest reject semantic — those valid dep names are STILL dropped because the manifest's `name` is malformed.

**Rationale**:

- Matches the real reproduction from issue #768 (Cookiecutter `{{package-name}}` placeholder).
- Valid deps in the deps-list ensure SC-001 (zero components) tests the whole-manifest reject, not just the malformed-name skip.
- Follows the `waybill-fixture-*` synthetic-name convention per memory `feedback_fixture_synthetic_package_names`.

## R6 — Log line format

**Question**: What exact WARN log format does FR-002 require, and how does FR-003's `names_rejected=<N>` fit into the existing reader-complete log?

**Investigation**: Existing pip reader completion log at `pip/mod.rs::read`:

```
INFO pip reader complete
  main_modules_emitted=<N> poetry_skips=<N> ...
```

**Decision**:

WARN log format:

```
WARN pip: pyproject.toml [project].name failed PEP 508 validation; skipping whole manifest
  manifest=<path>
  name=<offending-name-string>
  reason=<reason-str>
```

Structured completion log addition:

```
INFO pip reader complete
  main_modules_emitted=<N>
  poetry_skips=<N>
  names_rejected=<N>    # NEW — count of manifests dropped due to malformed name
  ...
```

**Rationale**:

- WARN line is one per rejected manifest per FR-002.
- Structured fields (`manifest`, `name`, `reason`) are `tracing` key-value pairs — machine-parseable per Constitution Principle X.
- `names_rejected` slot integrates with the existing reader-complete log without shape churn.

## Summary of decisions ready for Phase 1

| Decision | Value |
|---|---|
| Emission-point survey | Two consumer functions (`build_pip_main_module_entry` + `pyproject_declared_deps`), both loop-consumed in `read()` |
| Fix pattern | Pre-filter `project_roots` at top of `read()` — one filter pass drops rejected manifests from BOTH downstream loops |
| Helper module location | `waybill-cli/src/scan_fs/package_db/name_validation.rs` |
| Helper API shape | Per-ecosystem functions: `is_pep508_name` + `validate_pep508_name` returning `Result<(), NameValidationError>` |
| PEP 508 regex | `^[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?$` (compile-once via `OnceLock`) |
| Fixture | `waybill-cli/tests/fixtures/pip/malformed_name_placeholder/pyproject.toml` — malformed name + valid dep list |
| WARN log format | Structured `tracing::warn!` with `manifest`, `name`, `reason` fields; one line per rejected manifest |
| Completion log addition | `names_rejected=<N>` field alongside existing counts |
| Zero new Cargo deps | Confirmed — `regex`, `tracing`, `toml` all pre-existing |
| Zero waybill-common changes | Confirmed |
| Estimated production diff | ~70 lines (comfortably under SC-007's 200-line ceiling) |
