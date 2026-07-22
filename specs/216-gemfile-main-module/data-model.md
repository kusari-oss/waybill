# Data Model: Gemfile-only Ruby application main-module

**Feature**: 216-gemfile-main-module
**Date**: 2026-07-22

Only ONE new entity — the emitted `PackageDbEntry` for an application main-module. Everything else reuses existing types.

## E1 — Application main-module `PackageDbEntry`

**Location**: constructed in `waybill-cli/src/scan_fs/package_db/gem.rs::build_gem_application_main_module_entry`.

**Type**: `waybill_cli::scan_fs::package_db::PackageDbEntry` (existing type, unchanged).

**Field values**:

| Field | Value | Notes |
|---|---|---|
| `purl` | `Purl::new(&format!("pkg:generic/{name}@{version}"))?` | Per FR-002; validated at construction time. |
| `name` | `<derived>` (see below) | Per FR-003. |
| `version` | `<git-describe>` OR `"0.0.0-unknown"` | Per FR-004 + R3 ladder. |
| `depends` | Same as pre-feature Gemfile.lock-derived `depends` at that directory (empty if no `Gemfile.lock`) | Pre-feature behavior preserved; nothing new here. |
| `evidence.source_file_paths` | `[<Gemfile path relative to scan_root>]` | Provenance for the m127 root-selector + m215 split source_dir derivation. |
| `parent_purl` | `None` | Application main-modules are top-level; no containing coord. |
| `extra_annotations` | `{ "waybill:component-role": "main-module", "waybill:package-shape": "application" }` | Per FR-001 + FR-008 + R4. Order: BTreeMap-serialized (deterministic). |
| `sbom_tier` | `Some("source".to_string())` | Per m069 gem main-module convention — application main-modules are source-tier, not deployed/installed. |
| Every other `PackageDbEntry` field | Same defaults as the m069 gemspec-derived main-module builder | Field-for-field parity with `build_gem_main_module_entry` at gem.rs:1202. |

### Name derivation (per FR-003 + R2)

Input: the directory containing the `Gemfile` (as a `PathBuf`).

```
raw = manifest_dir.file_name().and_then(|n| n.to_str())?  // "common-infra"
slug = apply_m215_slug_rules(raw)                         // lowercase + strip unsafe chars
if slug.is_empty() { return None; }                       // skip pathological case
return slug;
```

The m215 slug rules live in `waybill-cli/src/generate/split.rs::subject_slug`. Reuse via `pub(crate)` re-export if not already public.

### Version derivation (per FR-004 + R3)

Input: the manifest directory + the scan-root directory.

```
1. Try `git describe --tags --always` in manifest_dir (2s timeout) → non-empty stdout wins
2. Try `git describe --tags --always` in scan_root      (2s timeout) → non-empty stdout wins
3. Fall back to literal "0.0.0-unknown"
```

Implementation: reuse the m053 `git describe` helper if it's pub(crate)-visible. If it's private to the golang reader, extract to a shared `scan_fs::git_describe` module accessible to both gem.rs and golang.rs. Design decision deferred to implementation phase.

### Invariants

- `purl.ecosystem() == "generic"` for every application main-module (validates FR-002 at test time).
- `extra_annotations["waybill:component-role"] == "main-module"` (validates FR-001 + FR-008).
- `extra_annotations["waybill:package-shape"] == "application"` (validates FR-008 + R4).
- `name` non-empty (skipped emission otherwise per R2).
- Directory-basename → `name` mapping is deterministic across scans (SC-005 validation).

## E2 — Walker output (no new type)

**Function signature**: `fn find_top_level_gemfiles(rootfs: &Path) -> Vec<PathBuf>`.

**Type**: `Vec<PathBuf>` (existing stdlib types).

**Contents**: absolute or scan-relative paths to `Gemfile` files that pass the R1 predicate:
- Filename is exactly `Gemfile` (case-sensitive)
- Parent directory contains NO `*.gemspec` file
- Parent directory is not under an excluded install-state path (`vendor/`, `gems/`, `specifications/`, `.bundle/`)

**Ordering**: sorted lex by path (via `.sort()` after walk), matching `find_top_level_gemspecs`.

## Cross-entity flow (per-scan lifecycle)

```
[Scan starts → gem::read(rootfs) invoked]
             │
             │  Existing gemspec-loop (m069, unchanged):
             │  find_top_level_gemspecs(rootfs) → gemspec paths
             │  → build_gem_main_module_entry each
             │  → augment-or-emit into `out`
             ▼
[out: Vec<PackageDbEntry> populated with gemspec-derived main-modules
      + Gemfile.lock-derived transitive components]
             │
             │  NEW: application-loop
             │  find_top_level_gemfiles(rootfs) → Gemfile paths
             │  (FR-007 guarantee: NO overlap with gemspec paths above)
             │  → build_gem_application_main_module_entry each
             │  → push into `out` (no augment-existing — no PURL overlap possible)
             ▼
[out augmented with application main-modules]
             │
             │  Downstream unchanged: dedup, cross-reader merging,
             │  m127 root selection, m215 split enumeration all operate
             │  on the augmented `out` transparently.
             ▼
[Return to caller]
```

## What we're NOT modeling

- **Gemfile DSL AST**: no Ruby-runtime parsing (Constitution Principle I).
- **Bundler-group scopes on the main-module itself**: the main-module IS the top of the tree; its own scope doesn't need modeling. Transitive deps' scopes are already handled by the pre-existing `parse_gemspec_groups` path.
- **`waybill:framework = "rails"` inference**: explicitly out-of-scope per spec.
- **VCS URL inference from git remote**: explicitly out-of-scope per spec.

## Field additions to existing types

**None.** The `PackageDbEntry` type is used as-is. The `extra_annotations` map already supports arbitrary `waybill:*` keys. The `waybill:package-shape` key is a NEW key in the vocabulary but requires no schema change to the type system.
