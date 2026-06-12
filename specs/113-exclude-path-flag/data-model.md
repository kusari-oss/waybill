# Phase 1 Data Model — User-Supplied Directory Exclusion

**Feature**: 113-exclude-path-flag
**Date**: 2026-06-12

In-process types only. Nothing persists beyond the scan's lifetime.

---

## `ExclusionEntry`

```text
pub enum ExclusionEntry {
    /// No metacharacters in the source string. Matched literally
    /// (after platform-separator normalization) against the
    /// candidate directory's path relative to the scan root.
    Literal(PathBuf),

    /// Source string contained at least one of `*`, `?`, `[`.
    /// Compiled at parse time; matched against the candidate
    /// directory's normalized relative path.
    Pattern(globset::Glob),
}
```

**Construction**: `ExclusionEntry::parse(s: &str) -> Result<Self, ExcludePathError>`. The parser inspects `s` for `*`, `?`, `[`; if present, compiles to `Glob`; otherwise stores as `PathBuf`. Empty strings are rejected (`ExcludePathError::EmptyEntry`).

**Invariants**:
- `Literal` paths are always relative (parser strips any leading `/`; absolute paths are an operator typo we catch by normalization, not by validation — the resulting relative path matches what they probably meant).
- `Pattern` globs are always pre-validated (a `Glob` cannot exist with a parse error).
- Both variants are platform-separator-agnostic at match time (R7).

---

## `ExclusionSet`

```text
pub struct ExclusionSet {
    /// All entries kept in source order for the transparency
    /// annotation's deterministic emission.
    entries: Vec<ExclusionEntry>,

    /// Pre-compiled GlobSet over every Pattern entry. None if
    /// no Pattern entries were supplied (saves an empty-set match
    /// per descent decision).
    pattern_set: Option<globset::GlobSet>,

    /// Literal entries projected to forward-slash form for fast
    /// equality checks at match time.
    literal_paths: Vec<String>,
}
```

**Construction**: `ExclusionSet::from_iter<I: IntoIterator<Item = &str>>(I) -> Result<Self, ExcludePathError>`. Parses each input, collects entries, builds the `GlobSet` once, deduplicates literal paths.

**Operations**:
- `is_empty(&self) -> bool` — fast path for "no exclusions configured."
- `matches(&self, candidate_rel_path: &str) -> bool` — first checks `literal_paths` for exact match, then `pattern_set.is_match`. Returns `true` if either layer hits.
- `entries(&self) -> &[ExclusionEntry]` — accessor for the transparency-annotation emitter.
- `as_normalized_strings(&self) -> Vec<String>` — emits the annotation payload (literal paths with forward slashes, patterns verbatim).

**Invariants**:
- Empty (`ExclusionSet { entries: vec![], pattern_set: None, literal_paths: vec![] }`) is a valid value and the default.
- Order-preserving across construction inputs.
- Idempotent on duplicate inputs (same path supplied twice still produces one literal entry).

---

## `ExcludePathError`

```text
pub enum ExcludePathError {
    EmptyEntry,
    MalformedPattern { entry: String, source: globset::Error },
}
```

`thiserror`-derived. Displayed at the CLI parse boundary; never propagated past `main.rs`.

---

## Threading through the scan pipeline

| Function | Existing signature | New signature |
|---|---|---|
| `scan_path` (`scan_fs/mod.rs:118`) | `…, scan_target_name: Option<&str>` | `…, scan_target_name: Option<&str>, exclude_set: &ExclusionSet` |
| `read_all` (`scan_fs/package_db/mod.rs:1299`) | `…, include_dev: bool` | `…, include_dev: bool, exclude_set: &ExclusionSet` |
| Per-ecosystem `read` (cargo, maven, gem, golang, go_binary, …) | `(rootfs: &Path, include_dev: bool)` | `(rootfs: &Path, include_dev: bool, exclude_set: &ExclusionSet)` |
| `should_skip_default_descent` (`project_roots.rs:115`) | `(name: &str) -> bool` | `(candidate: &Path, rootfs: &Path, exclude_set: &ExclusionSet) -> bool` |
| Each per-walker `should_skip_descent` | `(name: &str) -> bool` or `(path: &Path) -> bool` | `(candidate: &Path, rootfs: &Path, exclude_set: &ExclusionSet) -> bool` |

The helper-signature change from "name-only" to "candidate-path + rootfs" is required so the helper can compute the candidate's path relative to the scan root for both literal and pattern matching. Built-in skips that key on `name` continue to read `candidate.file_name()` internally.

---

## Transparency annotation payload

When `exclude_set.is_empty() == false`, the SBOM-emission code adds:

| Format | Location | Property/Annotation |
|---|---|---|
| CDX 1.6 | `metadata.properties[]` | `{name: "mikebom:exclude-path", value: <comma-joined as_normalized_strings>}` |
| SPDX 2.3 | `creationInfo.annotations[]` | `{annotationType: "OTHER", annotator: "Tool: mikebom-…", annotationDate: <emission ts>, comment: <comma-joined>}` |
| SPDX 3 | document-level `Annotation` element | `statement` field = `<comma-joined>` |

Payload string format: each entry on its own segment, comma-joined, no quoting (FR-007 already forbids commas in patterns through the globset parse step rejecting unbalanced brackets; literal paths cannot contain commas in any path mikebom would scan).
