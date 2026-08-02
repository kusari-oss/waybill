# Contract: Pants shell BUILD-file DSL parse + `pants.toml` `[shellcheck/shfmt/shunit2]` shape + `PackageDbEntry` emission

**Consumer surface**:
`waybill-cli/src/scan_fs/package_db/pants_shell/mod.rs::read(scan_root: &Path) -> Vec<PackageDbEntry>`

**Called from**:
`waybill-cli/src/scan_fs/package_db/mod.rs::read_all` dispatcher
(new call site alongside `pants::read` and `pants_jvm::read`).

Documents the exact wire-format expectations for both files
consumed (BUILD files + `pants.toml`), the shape of every
`PackageDbEntry` emitted, and the fail-open behavior boundaries.

---

## Input contract A: Pants BUILD file target declarations

**Path discovery**: every file with literal name `BUILD` (no
extension) under `<scan_root>` reachable via `safe_walk` (respects
symlink-cycle guard, `--exclude-path`, and depth limits per m054 +
m113).

**Recognized target functions** (per R1):

```python
shell_source(
    name="deploy",              # REQUIRED — target address suffix
    source="deploy.sh",         # REQUIRED — path relative to BUILD file's dir
    # ... other kwargs ignored (dependencies=, tags=, etc.)
)

shell_sources(
    name="utils",               # OPTIONAL — defaults to dir basename per Pants
    sources=["*.sh", "b.sh"],   # OPTIONAL — defaults to Pants convention per docs
)

shunit2_test(
    name="deploy-test",         # REQUIRED
    source="deploy_test.sh",    # REQUIRED
)

shunit2_tests(
    name="unit",                # OPTIONAL
    sources=["*_test.sh"],      # OPTIONAL
)
```

**Extraction rules** (per R2):
- Regex-scoped; multi-line kwargs handled via `[^)]*?` non-greedy
  match up to the closing `)`.
- `name` / `source` / `sources` values must be **string literals**
  (single- or double-quoted). Variable references, string
  concatenation, and f-strings trigger a WARN + skip that target
  (fail-open at target grain).
- `sources=[...]` list items must all be string literals; the same
  rules apply per-item.
- Additional kwargs (`dependencies=[...]`, `tags=[...]`,
  `timeout=...`, etc.) are ignored.

**Target address resolution**:
- BUILD file at `<scan_root>/<subdir>/BUILD` produces addresses
  prefixed with `<subdir>:<name>`.
- Root-level BUILD file (`<scan_root>/BUILD`) produces addresses
  `<name>` (no prefix — Pants convention).
- When `name=` is absent (allowed for `shell_sources` /
  `shunit2_tests`), waybill uses the parent directory's basename
  as the target-name suffix (matches Pants's default target
  behavior).

**File resolution**:
- `source="X.sh"` resolves to `<build_dir>/X.sh`.
- `source="subdir/X.sh"` resolves to `<build_dir>/subdir/X.sh`
  (any relative path allowed).
- `sources=["*.sh"]` glob expands within `<build_dir>` (non-
  recursive).
- `sources=["**/*.sh"]` recursive glob expands under `<build_dir>`.
- Files that don't exist on disk are dropped with a WARN naming
  the target address + the missing path.

## Input contract B: `pants.toml` shell-subsystem sections (optional)

**Path discovery**: `<scan_root>/pants.toml` ONLY (nested
`pants.toml` files under the scan root are ignored per spec
Assumption).

**Recognized subsystem sections** (per R4):

```toml
[shellcheck]
version = "v0.9.0"

[shfmt]
version = "v3.7.0"

[shunit2]
version = "2.1.8"
```

Every other section (including `[GLOBAL]`, `[python]`, `[jvm]`,
plugin sections, etc.) is ignored by this reader.

**Extraction rules**:
- `version` key must be a string literal.
- Value is preserved verbatim, including any leading `v` prefix.
- Missing `version` key → no component emitted for that tool.
- Missing entire subsystem section → no component; not an error.
- Non-existent `pants.toml` → gracefully skipped; not an error.
- Malformed TOML → WARN naming the file + parse-error message;
  scan continues.

---

## Fail-open behavior boundaries (FR-006 / FR-009 / SC-005)

Per-file AND per-target diagnostics never abort the whole scan:

| Condition | Diagnostic level | Reader behavior |
|-----------|------------------|-----------------|
| BUILD file cannot be read (I/O error) | WARN | Skip this file; increment `build_files_skipped_corrupt`; continue |
| BUILD file contains no recognized shell targets | INFO (debug-level) | Skip; no components emitted; do NOT increment corrupt |
| BUILD file contains 3 valid + 1 broken target | WARN naming broken target + line | Emit 3 valid; skip 1 broken; file counts as parsed_ok |
| Target has `name=` kwarg with non-string-literal value | WARN | Skip this target; continue with others in the file |
| Target has `source=` with variable reference / concat | WARN | Skip this target; continue |
| Target's `source=` file doesn't exist on disk | WARN naming target + path | Skip this file; if target had other files (glob), they still emit |
| Target's `sources=[...]` glob matches zero files | INFO | No components for this target; not a WARN — empty globs are legal |
| `pants.toml` cannot be read | WARN | Skip; no tool components; script components still emit |
| `pants.toml` has non-string `version` value | WARN | Skip that tool; other tools' `version` still checked |
| Zero BUILD files discovered AND no `pants.toml` | (silent) | Return `Vec::new()` early; emit NO summary log (byte-identity guarantee per FR-011 / SC-003) |

The reader ONLY returns `Vec<PackageDbEntry>`. It has no `Result`
return — errors are logged and swallowed per the fail-open contract.

---

## Output contract: `PackageDbEntry` emission

### Per script file (US1 / US3 output)

| Field | Value / source | Type |
|-------|----------------|------|
| `purl` | `pkg:generic/<basename>@<sha256[:12]>` (per R3) | `Purl` |
| `name` | Script file's basename verbatim (e.g., `"waybill-fixture-deploy.sh"`) | `String` |
| `version` | First 12 hex chars of the file's SHA-256 | `String` |
| `source_path` | Absolute path to the `.sh` file | `String` |
| `depends` | `Vec::new()` | `Vec<String>` |
| `lifecycle_scope` | `ShellTargetKind::lifecycle_scope()` — Runtime for `shell_source`/`shell_sources`, Development for `shunit2_test`/`shunit2_tests` | `Option<LifecycleScope>` |
| `sbom_tier` | `Some("source".to_string())` | `Option<String>` |
| `hashes` | 1 `ContentHash::sha256` (full 64-char hex) | `Vec<ContentHash>` |
| `licenses` | `Vec::new()` (shell scripts don't self-declare licenses) | `Vec<SpdxExpression>` |
| `requirement_ranges` | `Vec::new()` | `Vec<String>` |
| `extra_annotations` | See below | `BTreeMap<String, Value>` |
| All other fields | `None` / default | Match m224 posture |

**Per-script `extra_annotations`**:

| Key | Value | Catalog row |
|-----|-------|-------------|
| `waybill:pants-target` | Target address(es), comma-separated, lexically sorted when multiple targets own the same file | **NEW C145** (this feature) |
| `waybill:source-files` | JSON-array-in-string of scan-root-relative file paths (single-element for the typical case) | C7 |

### Per pinned tool (US2 output)

| Field | Value / source | Type |
|-------|----------------|------|
| `purl` | `pkg:generic/<tool>@<version>` (per R4) | `Purl` |
| `name` | Tool name (`"shellcheck"` / `"shfmt"` / `"shunit2"`) | `String` |
| `version` | Operator-pinned version verbatim | `String` |
| `source_path` | Absolute path to `pants.toml` | `String` |
| `depends` | `Vec::new()` | `Vec<String>` |
| `lifecycle_scope` | `Some(LifecycleScope::Development)` | `Option<LifecycleScope>` |
| `sbom_tier` | `Some("design".to_string())` | `Option<String>` |
| `hashes` | `Vec::new()` | `Vec<ContentHash>` |
| `licenses` | `Vec::new()` | `Vec<SpdxExpression>` |
| `requirement_ranges` | `Vec::new()` | `Vec<String>` |
| `extra_annotations` | `waybill:source-file = pants.toml` | `BTreeMap<String, Value>` |
| All other fields | `None` / default | Match m080-shipped posture |

---

## FR-010 scan-end INFO log

Emitted exactly once at the end of `pants_shell::read()`:

```text
INFO waybill::scan_fs::package_db::pants_shell: pants-shell reader complete
  build_files_discovered=<N>
  build_files_parsed_ok=<N>
  build_files_skipped_corrupt=<N>
  shell_targets_found=<N>
  script_components_emitted=<N>
  tool_components_emitted=<N>
```

If ZERO BUILD files discovered AND `pants.toml` absent: reader
returns early without emitting any log (byte-identity guarantee
per SC-003).

If ZERO BUILD files discovered but `pants.toml` PRESENT (tool-only
scan case): reader emits the log with `build_files_discovered=0`
etc., but `tool_components_emitted` may be > 0. This is not silent
because operator-visible pinned tools represent supply-chain
signal even without script inventory.

---

## Dedup contract (SC-006) — cross-target dedup within the reader

**Problem**: a single `.sh` file may be owned by multiple targets
in the same BUILD file (or across two BUILD files at different
depths that both glob-match it).

**Rule**: emit exactly ONE `PackageDbEntry` per unique canonical
file path. Merge the owning target addresses into the
`waybill:pants-target` annotation as a comma-separated,
lexically-sorted string.

**Example**:
- `scripts/BUILD` has:
  ```python
  shell_source(name="single", source="waybill-fixture-x.sh")
  shell_sources(name="glob", sources=["waybill-fixture-*.sh"])
  ```
- One component emitted with
  `waybill:pants-target = "scripts:glob,scripts:single"`.

**Lifecycle-scope on merged targets**: when a file is owned by
both a runtime `shell_source` AND a dev `shunit2_test`, the
lifecycle_scope defaults to Development (dev scope is the safer
default — operators can spot the dev tag and re-triage if surprised).

---

## Reader-to-reconciler boundary

The pants-shell reader's emitted entries flow through the standard
`read_all` → m191 reconciler → format-emit pipeline.

- **PURL uniqueness**: content-addressed PURLs
  (`pkg:generic/<basename>@<sha256[:12]>`) are unique per unique
  file content. The reconciler treats them as ordinary generic
  entries.
- **Cross-reader collisions**: extremely unlikely — the
  `<basename>@<sha256[:12]>` shape doesn't collide with any other
  reader's PURL scheme (m133 file-tier uses `file-tier` name;
  cargo/npm/etc. use ecosystem-specific PURLs).
- **m133 file-tier walker dedup**: sees the reader's emitted
  `source_path` values in its dedupe index (built AFTER all
  package_db readers), so orphan-file discovery of the same paths
  is automatically suppressed. Zero interaction changes needed
  on the m133 side.

---

## Non-goals for v1

- **`shell_command` targets**: deferred per spec Out-of-Scope.
- **Plugin-registered custom target types**: silently ignored.
- **Nested `pants.toml` files under scan root**: only root-level
  consulted.
- **shunit2 embedded bundle detection**: only operator-pinned
  `[shunit2] version=...` is honored.
- **Per-arch `known_versions` entries**: parsed by pants.toml TOML
  reader (they're in the same section) but NOT emitted; only
  `version` triggers a component.
- **Cross-workspace / cross-repo Pants**: scope is per-scan-root.
