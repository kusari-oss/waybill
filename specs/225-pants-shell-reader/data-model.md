# Phase 1: Data Model — Pants shell reader

**Feature**: 225-pants-shell-reader
**Date**: 2026-08-02

The feature adds 4 new module-private types (BUILD-DSL parse
outputs) + 1 typed error enum + 2 helper structs (config + emit
context). Reuses `PackageDbEntry`, `Purl`, `LifecycleScope`,
`ContentHash` verbatim. All new types satisfy Constitution
Principle IV.

---

## New types (all module-private to `scan_fs::package_db::pants_shell`)

### `ShellTargetKind` — closed-enum discriminator

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellTargetKind {
    /// `shell_source(name=..., source="a.sh")` — single-file, runtime scope.
    ShellSource,
    /// `shell_sources(name=..., sources=["*.sh"])` — glob, runtime scope.
    ShellSources,
    /// `shunit2_test(name=..., source="a_test.sh")` — single-file, dev scope.
    Shunit2Test,
    /// `shunit2_tests(name=..., sources=["*_test.sh"])` — glob, dev scope.
    Shunit2Tests,
}

impl ShellTargetKind {
    /// FR-008 lifecycle-scope classification: shunit2 variants tag
    /// Development; shell_source variants tag Runtime (or absent).
    pub(crate) fn lifecycle_scope(self) -> LifecycleScope {
        match self {
            Self::Shunit2Test | Self::Shunit2Tests => LifecycleScope::Development,
            Self::ShellSource | Self::ShellSources => LifecycleScope::Runtime,
        }
    }

    /// Function-call name matches the target function's string form.
    pub(crate) fn as_dsl_name(self) -> &'static str {
        match self {
            Self::ShellSource => "shell_source",
            Self::ShellSources => "shell_sources",
            Self::Shunit2Test => "shunit2_test",
            Self::Shunit2Tests => "shunit2_tests",
        }
    }
}
```

### `TargetDeclaration` — one parsed target from a BUILD file

```rust
#[derive(Debug, Clone)]
pub(crate) struct TargetDeclaration {
    /// Which of the 4 built-in shell target types this declaration
    /// invokes. Drives lifecycle-scope classification.
    pub(crate) kind: ShellTargetKind,
    /// The `name=` kwarg value. May be absent for `shell_sources`
    /// / `shunit2_tests` (Pants defaults to the directory name);
    /// resolved to a concrete address by the emit layer.
    pub(crate) name: Option<String>,
    /// The source expression — either a single `source="..."` string
    /// literal or a `sources=[...]` list of glob patterns.
    pub(crate) source: TargetSource,
    /// 1-based line number where the target declaration begins in
    /// the source BUILD file. Used for WARN diagnostics on parse
    /// errors + per-target dedup.
    pub(crate) start_line: u32,
}

#[derive(Debug, Clone)]
pub(crate) enum TargetSource {
    /// From `shell_source(source="a.sh")` or `shunit2_test(source="a_test.sh")`.
    Single(String),
    /// From `shell_sources(sources=["*.sh", "b.sh"])` or `shunit2_tests(sources=[...])`.
    /// Empty vec means "operator omitted `sources=`, use Pants default".
    Globs(Vec<String>),
}
```

### `ResolvedTarget` — target address + resolved on-disk files

```rust
#[derive(Debug, Clone)]
pub(crate) struct ResolvedTarget {
    /// Canonical Pants target address (e.g., `"scripts:deploy"`).
    /// Derived from the BUILD file's parent directory relative to
    /// scan root + the `name=` kwarg (or dir basename if absent).
    pub(crate) address: String,
    /// The declaration this resolved from. Drives lifecycle_scope.
    pub(crate) kind: ShellTargetKind,
    /// Zero or more `.sh` files on disk. Empty for globs that match
    /// zero files (INFO diagnostic emitted, no components).
    pub(crate) files: Vec<PathBuf>,
    /// Absolute path to the source BUILD file (for provenance +
    /// WARN diagnostics).
    pub(crate) build_file: PathBuf,
}
```

### `TargetParseError` — closed-enum failure mode

```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum TargetParseError {
    #[error("target has no name= or source= kwarg (line {line})")]
    MissingRequiredKwarg { line: u32 },
    #[error("target has non-string-literal source expression at line {line}: {snippet}")]
    NonStringLiteralSource { line: u32, snippet: String },
    #[error("unbalanced parens starting at line {line}")]
    UnbalancedParens { line: u32 },
}
```

### `ShellSetupConfig` — parsed `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` shape

```rust
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ShellSetupConfig {
    #[serde(default)]
    pub(crate) shellcheck: Option<ExternalToolSection>,
    #[serde(default)]
    pub(crate) shfmt: Option<ExternalToolSection>,
    #[serde(default)]
    pub(crate) shunit2: Option<ExternalToolSection>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExternalToolSection {
    /// Operator-pinned version string; emitted verbatim (leading
    /// `v` prefix preserved). Absent means "use Pants default" and
    /// waybill emits no component for this tool.
    pub(crate) version: Option<String>,
    // `known_versions` field IS present in real pants.toml files but
    // waybill intentionally does NOT parse it per research §R4.
}
```

### `EmitCounters` — FR-010 log-summary accumulator

```rust
#[derive(Debug, Default)]
struct EmitCounters {
    build_files_discovered: usize,
    build_files_parsed_ok: usize,
    build_files_skipped_corrupt: usize,
    shell_targets_found: usize,
    script_components_emitted: usize,
    tool_components_emitted: usize,
}
```

---

## Existing types being reused (no changes)

### `waybill_common::types::purl::Purl`

Reused verbatim. Constructed via `Purl::new(&purl_str)` after inline
PURL string construction per R3:
`pkg:generic/<basename>@<sha256[:12]>` for scripts,
`pkg:generic/<tool>@<version>` for tool pins.

### `waybill_common::resolution::LifecycleScope`

Reused verbatim. Variants: `Runtime` (default + `shell_source` /
`shell_sources` targets), `Development` (`shunit2_test` /
`shunit2_tests` targets).

### `waybill_common::types::hash::{ContentHash, HashAlgorithm}`

Reused verbatim. Each script file's on-disk bytes hashed via
`ContentHash::sha256(hex)` (streaming SHA-256 over the file contents;
see `waybill-cli/src/scan_fs/file_tier/walker.rs` for the same
pattern the m133 walker uses).

### `PackageDbEntry` (defined in `scan_fs::package_db::mod.rs`)

Reused verbatim. Field mapping from one `ResolvedTarget` file:

| PackageDbEntry field | Value | Notes |
|----------------------|-------|-------|
| `purl` | `pkg:generic/<basename>@<sha256[:12]>` | R3 shape |
| `name` | `<basename>` (e.g., `"waybill-fixture-deploy.sh"`) | Human-readable |
| `version` | `<sha256[:12]>` | Content-addressed pin |
| `source_path` | Absolute path to the `.sh` file | Enables m133 walker dedup |
| `depends` | `Vec::new()` | Shell scripts have no dep-graph edges |
| `lifecycle_scope` | `Some(kind.lifecycle_scope())` | Runtime for shell_source, Development for shunit2_* |
| `sbom_tier` | `Some("source".to_string())` | BUILD-file-derived |
| `evidence_kind` | `None` | Matches m223 / m224 posture |
| `hashes` | 1 `ContentHash::sha256` from streaming file hash | Full 64-char hex |
| `licenses` | `Vec::new()` | Shell scripts don't self-declare licenses |
| `requirement_ranges` | `Vec::new()` | N/A |
| `extra_annotations` | See below | 1–3 keys per component |
| All other fields | `None` / default | Match m224 posture |

Field mapping from one pinned tool (US2):

| PackageDbEntry field | Value | Notes |
|----------------------|-------|-------|
| `purl` | `pkg:generic/<tool>@<version>` | R4 shape, verbatim version |
| `name` | `<tool>` (`"shellcheck"` / `"shfmt"` / `"shunit2"`) | |
| `version` | Operator-pinned version string | e.g., `"v0.9.0"` |
| `source_path` | Absolute path to `pants.toml` | |
| `depends` | `Vec::new()` | Tools have no dep-graph edges here |
| `lifecycle_scope` | `Some(LifecycleScope::Development)` | Lint/test tools are dev-only |
| `sbom_tier` | `Some("design".to_string())` | Manifest-declared, not built |
| `hashes` | `Vec::new()` | pants.toml doesn't carry tool hashes |
| `licenses` | `Vec::new()` | |
| `extra_annotations` | `waybill:source-file = pants.toml` | Reuses m080 C-row |

### `extra_annotations` mapping (per script component)

**Always present**:

| Key | Value | Catalog row |
|-----|-------|-------------|
| `waybill:pants-target` | Comma-separated target addresses (lexically sorted). Example: `"scripts:deploy"` OR `"scripts:glob,scripts:single"` for dupe-owner case. | **NEW C145** |
| `waybill:source-files` | JSON-array-in-string, single-element for non-dupe case: `["scripts/waybill-fixture-deploy.sh"]`. Multi-element when multiple BUILD files own the same file (rare). Reuses m080 row. | C7 |

**Present iff data available**:

| Key | Value | Condition |
|-----|-------|-----------|
| _(none in v1)_ | — | Follow-up: `waybill:pants-shell-shape` (whether the target used `source=` vs `sources=[...]`) is diagnostic-only; not emitted in v1 per data-model.md §"Diagnostic-only fields not emitted". |

### `extra_annotations` mapping (per tool component)

**Always present**:

| Key | Value | Catalog row |
|-----|-------|-------------|
| `waybill:source-file` | `"pants.toml"` (relative to scan root) | m080 |

---

## Discovery + orchestration data flow

```text
┌──────────────────────────────┐
│  scan_fs::package_db::       │
│    mod.rs::read_all          │
└───────────────┬──────────────┘
                │ calls
                ▼
┌──────────────────────────────┐
│  pants_shell::read(root)     │
└───────────────┬──────────────┘
                │
                ├─── 1. Walk BUILD files under scan_root via safe_walk
                │    (extension-agnostic filter matches literal filename `BUILD`)
                │    ↓
                │    Vec<PathBuf> of BUILD files
                │
                ├─── 2. For each BUILD file:
                │      a) Read bytes; skip on I/O error (WARN)
                │      b) Regex-extract target declarations via
                │         build_dsl::extract_targets → Vec<TargetDeclaration>
                │      c) Increment counters
                │
                ├─── 3. For each TargetDeclaration:
                │      a) Resolve target address (BUILD file's dir + name kwarg
                │         OR dir basename if name absent)
                │      b) Resolve source= / sources=[...] to Vec<PathBuf>
                │         relative to the BUILD file's own directory
                │      c) Emit one PackageDbEntry per .sh file found
                │         (WARN + skip files that don't exist on disk)
                │
                ├─── 4. Cross-file dedup pass:
                │      Group emitted entries by canonical path;
                │      when 2+ groups share a path, merge their
                │      `waybill:pants-target` values into one comma-sep
                │      lexically-sorted string (SC-006)
                │
                ├─── 5. Read pants.toml at scan_root (if present):
                │      a) TOML-parse via config::parse
                │      b) For each of [shellcheck]/[shfmt]/[shunit2]
                │         with `version` set: emit one design-tier
                │         PackageDbEntry
                │
                ├─── 6. Emit FR-010 INFO log (unless zero BUILD files
                │      discovered AND no pants.toml — byte-identity
                │      guarantee)
                │
                └─── 7. Return combined Vec<PackageDbEntry> to read_all
```

---

## Storage / persistence

None. All state (parsed BUILD-file target declarations + resolved
`.sh` file paths + intermediate `EmitCounters` + final
`Vec<PackageDbEntry>`) lives on the stack for the duration of a
single scan. Matches m223 + m224 + every language-reader milestone
since 002.

---

## Compatibility

- **Existing m133 file-tier walker**: no conflict. Runs AFTER
  `package_db::read_all` and its dedupe index sees the
  pants-shell-emitted `source_path` values automatically. No
  double-emission risk.
- **Existing `pants` (m223) + `pants_jvm` (m224) readers**:
  independent modules under `package_db/`. All three may activate
  on the same scan (repos with Python + JVM + shell all use
  `pants.toml` for different subsystem sections).
- **Existing goldens**: unchanged when no Pants BUILD files
  present (FR-011 + SC-003 enforce this).
- **CI**: adds no new jobs. Existing lint + test lanes cover the
  new test binary.
- **Parity catalog**: **one new row (C145 `waybill:pants-target`)** +
  3 matching extractor entries. All other annotations reuse
  existing rows.

---

## Diagnostic-only fields NOT emitted in v1

Fields declared in the parse types but intentionally NOT surfaced
in the SBOM per data-model.md's minimum-viable emission surface:

- **`TargetDeclaration.start_line`** — used for WARN diagnostics
  at parse time; not carried into the emitted component.
- **`TargetSource::Globs` shape distinction from
  `TargetSource::Single`** — `waybill:pants-shell-shape`
  annotation would carry this. Follow-up if operators request.
- **`shunit2_test.timeout`, `shell_source.dependencies` kwargs** —
  ignored per FR-002 / FR-003 scoping.

Adding any of these requires a new catalog row + extractor triple
per memory `feedback_sbom_format_mapping_extractor_gate`, so
they're deferred behind demand.
