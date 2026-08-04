# Contract: Pants Go BUILD-file DSL parse + `pants.toml` `[golang]` shape + enrichment output

**Consumer surfaces**:
- `waybill-cli/src/scan_fs/package_db/pants_go/mod.rs::read(scan_root: &Path, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry>`
  (toolchain-pin emission via `read_all`)
- `waybill-cli/src/scan_fs/package_db/pants_go/mod.rs::enrich(scan_root: &Path, exclude_set: &ExclusionSet, components: &mut Vec<ResolvedComponent>)`
  (post-`read_all` enrichment)

**Called from**:
- `read` from `waybill-cli/src/scan_fs/package_db/mod.rs::read_all`
  dispatcher (new call site alongside `pants::read`,
  `pants_jvm::read`, `pants_shell::read`).
- `enrich` from `waybill-cli/src/scan_fs/mod.rs` at line ~1001,
  immediately after `crate::resolve::reconciler::reconcile_design_source_tiers`.

Documents the exact wire-format expectations for both files
consumed (BUILD files + `pants.toml`), the shape of any
`PackageDbEntry` emitted by `read`, the mutations `enrich`
performs on `ResolvedComponent` entries, and the fail-open
behavior boundaries.

---

## Input contract A: Pants Go BUILD file target declarations

**Path discovery**: every file with literal name `BUILD` (no
extension) under `<scan_root>` reachable via `safe_walk`
(respects symlink-cycle guard, `--exclude-path`, and depth
limits per m054 + m113). Same discovery path as m225
pants_shell.

**Recognized target functions** (per R1):

```python
go_mod(
    name="mod",                       # OPTIONAL — defaults to "mod"
)

go_third_party_package(
    name="cobra",                     # REQUIRED
    import_path="github.com/spf13/cobra",  # REQUIRED
)

go_binary(
    name="frontend",                  # REQUIRED
    main=".",                         # REQUIRED — path relative to BUILD dir
    # e.g., main="./cmd/foo" for a subdirectory main package
)

go_package(
    name="pkg",                       # OPTIONAL — defaults to dir basename
    # sources=[...] is parsed by Pants; waybill ignores it
)
```

**Extraction rules** (per R2, reusing m225 patterns):
- Regex-scoped; multi-line kwargs handled via char-scan
  `find_matching_close_paren` (with string-literal awareness).
- `name` / `import_path` / `main` values must be **string
  literals** (single- or double-quoted). Variable references,
  string concatenation, and f-strings trigger a WARN + skip
  that target.
- Additional kwargs (`dependencies=[...]`, `tags=[...]`, etc.)
  are ignored.

**Target address resolution** (per R3 + R4):
- BUILD file at `<scan_root>/<subdir>/BUILD` produces addresses
  prefixed with `<subdir>:<name>`.
- Root-level BUILD file (`<scan_root>/BUILD`) produces addresses
  `<name>` (bare, no prefix — Pants convention).
- When `name=` is absent (allowed for `go_mod` and `go_package`),
  waybill uses the Pants default: `"mod"` for `go_mod`, dir
  basename for `go_package`.

**`main=` path resolution for `go_binary`** (per R4):
- `main="."` → BUILD file's directory itself is the main package
- `main="./cmd/foo"` → `<BUILD dir>/cmd/foo` is the main package
- `main="cmd/foo"` (no leading `./`) → also
  `<BUILD dir>/cmd/foo`; Pants normalizes.
- Absolute paths (`main="/etc/foo"`) → warn + skip that target
  (not a legal Pants shape).

## Input contract B: `pants.toml` `[golang]` section (optional)

**Path discovery**: `<scan_root>/pants.toml` ONLY (nested
`pants.toml` files under the scan root are ignored per spec
Assumption).

**Recognized subsystem section** (per R4 of m226 spec):

```toml
[golang]
expected_version = "1.21"
# min_dot_version = "1.21"  # PARSED but NOT emitted per Out-of-Scope
```

**Extraction rules**:
- `expected_version` must be a string literal.
- Value preserved verbatim (e.g., `"1.21"` vs `"1.21.5"` vs
  `"go1.21"` — Pants accepts multiple shapes; waybill emits
  whatever the operator wrote).
- Missing `expected_version` → no toolchain component emitted.
- Missing `[golang]` section → no toolchain component; not an
  error.
- Non-existent `pants.toml` → gracefully skipped.
- Malformed TOML → WARN naming the file + parse-error message;
  scan continues.

---

## Fail-open behavior boundaries (FR-006 / FR-009 / SC-005)

Per-file AND per-target diagnostics never abort the whole scan:

| Condition | Diagnostic level | Reader behavior |
|-----------|------------------|-----------------|
| BUILD file cannot be read (I/O error) | WARN | Skip this file; increment `build_files_skipped_corrupt`; continue |
| BUILD file contains no recognized Go targets | (debug only) | Skip; not an error |
| BUILD file contains 3 valid + 1 broken target | WARN naming broken target + line | Add 3 valid to index; skip 1 broken; file counts as parsed_ok |
| Target has `name=` / `import_path=` / `main=` with non-string-literal value | WARN | Skip this target; continue with others in the file |
| `go_third_party_package(import_path=X)` names an import path with no matching `pkg:golang/*` component in the reconciled set | INFO | No annotation attached (nothing to attach to); scan does not abort |
| `go_binary(main=X)` names a path that doesn't resolve to any main-module component | INFO | No annotation attached; scan does not abort |
| `pants.toml` cannot be read | WARN | Skip; no toolchain component; enrichment still runs |
| `pants.toml` has non-string `expected_version` value | WARN | Skip toolchain component; enrichment still runs |
| Zero BUILD files discovered AND no `pants.toml` `[golang]` | (silent) | Return `Vec::new()` from `read`; `enrich` early-returns; emit NO summary log |

The `read` entry point ONLY returns `Vec<PackageDbEntry>`. The
`enrich` entry point returns `()`. Neither has a `Result` — all
errors are logged and swallowed per the fail-open contract.

---

## Output contract A: `PackageDbEntry` from `read()` (toolchain pin)

Emitted iff `pants.toml` `[golang] expected_version` is set to
a non-empty string. Zero or one entry.

| Field | Value / source | Type |
|-------|----------------|------|
| `purl` | `pkg:generic/go@<version>` (via `Purl::new`) | `Purl` |
| `name` | `"go"` | `String` |
| `version` | Operator-pinned `expected_version` verbatim | `String` |
| `source_path` | Absolute path to `pants.toml` | `String` |
| `depends` | `Vec::new()` | `Vec<String>` |
| `lifecycle_scope` | `Some(LifecycleScope::Development)` | `Option<LifecycleScope>` |
| `sbom_tier` | `Some("design".to_string())` | `Option<String>` |
| `hashes` | `Vec::new()` | `Vec<ContentHash>` |
| `licenses` | `Vec::new()` | `Vec<SpdxExpression>` |
| `requirement_ranges` | `Vec::new()` | `Vec<String>` |
| `extra_annotations` | `waybill:source-file = "pants.toml"` (m080 row) | `BTreeMap<String, Value>` |
| All other fields | `None` / default | Match m225 posture |

---

## Output contract B: `enrich()` mutations on `ResolvedComponent`

`enrich()` iterates `&mut Vec<ResolvedComponent>` and mutates
`extra_annotations` in place. It MUST NOT push new components
into the vector, remove any, or reorder them (FR-012 / Principle
IX no-fabrication invariant).

**Per matched component** (any `pkg:golang/*` PURL whose
`source_path` or `main-module` role matches at least one target):

| Mutation | Value | Catalog row |
|----------|-------|-------------|
| Set `extra_annotations["waybill:pants-target"]` | JSON string: comma-separated, lex-sorted list of owning Pants target addresses | C145 (broadened by m226 doc update) |

**Per unmatched component**: no mutation (byte-identity preserved).

**Multi-owner example** (aligned with SC-004):
- `3rdparty/go/BUILD` declares both `go_mod(name="mod")` and
  `go_third_party_package(name="cobra", import_path="github.com/spf13/cobra")`.
- `pkg:golang/github.com/spf13/cobra@v1.6.0` matches BOTH:
  - `go_mod` implicit ownership via `source_path` starts_with
    `3rdparty/go/`
  - `go_third_party_package` explicit import-path match
- Annotation value: `"3rdparty/go:cobra,3rdparty/go:mod"`
  (lex-sorted, comma-sep).

---

## FR-010 scan-end INFO log

Emitted exactly once at the end of `enrich()`:

```text
INFO waybill::scan_fs::package_db::pants_go: pants-go enrichment complete
  build_files_discovered=<N>
  build_files_parsed_ok=<N>
  build_files_skipped_corrupt=<N>
  go_targets_found=<N>
  components_annotated=<N>
  toolchain_component_emitted=<0|1>
```

The `toolchain_component_emitted` field is populated by
`read()`'s return value length (0 or 1); passed to `enrich()`
via a shared counter or re-derived from the components vec by
counting the toolchain PURL.

If ZERO BUILD files discovered AND `pants.toml` `[golang]`
absent: reader returns early without emitting any log (SC-003
byte-identity guarantee).

If ZERO BUILD files but `pants.toml` `[golang]` PRESENT
(toolchain-only case): reader emits the log with all counts at 0
except `toolchain_component_emitted=1`.

---

## Zero-fabrication contract (FR-012 / Principle IX)

**`enrich()` MUST NEVER**:
- push a new `ResolvedComponent` into the vector
- remove an existing `ResolvedComponent`
- change any field other than `extra_annotations`
- change the `waybill:pants-target` annotation on any component
  whose PURL does NOT match a declared target

**`read()` MUST NEVER**:
- emit a `pkg:golang/*` `PackageDbEntry` (even for
  `go_third_party_package(import_path=X)` — no synthetic
  fabrication)
- emit any component other than the single design-tier
  `pkg:generic/go@<version>` when `expected_version` is set

**Enforcement**: an integration test (T031 in tasks.md) will
compare `pkg:golang/*` component counts between a pre-m226
baseline and a post-m226 scan; the counts MUST be identical for
Pants Go fixtures. Only annotations may differ.

---

## Dedup contract (SC-004) — cross-target annotation merge

**Problem**: a single `pkg:golang/*` component may be owned by
multiple Pants targets (e.g., `go_mod` implicit + explicit
`go_third_party_package` + `go_package` main-module).

**Rule**: the enrichment pass collects all owning addresses into
a `Vec<TargetAddress>`, `.sort()` + `.dedup()` in place, then
`.join(",")` for the final annotation value.

**Example** (SC-004 gate):
- Owners: `[3rdparty/go:mod, 3rdparty/go:cobra]`
- After sort + dedup: `[3rdparty/go:cobra, 3rdparty/go:mod]`
- Joined: `"3rdparty/go:cobra,3rdparty/go:mod"`

---

## Non-goals for v1

- **`go_source` / `go_test` file-level targets** — deferred.
- **Plugin-registered custom Go target types** — silently ignored.
- **Nested `pants.toml` files under scan root** — only root-level.
- **`min_dot_version` from `pants.toml` `[golang]`** — parsed but
  not emitted; deferred.
- **`pkg:golang/*` fabrication for missing go.sum entries** — see
  Zero-fabrication contract above.
- **Cross-workspace / cross-repo Pants** — per-scan-root scope.
- **Interaction with `waybill trace`** — enrichment is a pure
  filesystem-read pass; no runtime observation.
