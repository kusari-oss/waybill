# Phase 1: Data Model — Pants Go reader

**Feature**: 226-pants-go-reader
**Date**: 2026-08-03

The feature adds 4 new module-private types (parse outputs +
ownership index) + 1 typed error enum + 1 config helper.
Reuses `PackageDbEntry`, `ResolvedComponent`, `Purl`,
`LifecycleScope`, `ContentHash` verbatim. All new types
satisfy Constitution Principle IV.

---

## New types (all module-private to `scan_fs::package_db::pants_go`)

### `GoTargetKind` — closed-enum discriminator

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoTargetKind {
    /// `go_mod(name="mod")` — implicit owner of every go.sum entry in the dir.
    GoMod,
    /// `go_third_party_package(name="X", import_path="example.com/foo")` — one dep.
    GoThirdPartyPackage,
    /// `go_binary(name="X", main="./cmd/foo")` — a buildable Go binary.
    GoBinary,
    /// `go_package(name="X")` — a Go package source directory.
    GoPackage,
}
```

### `GoTargetDeclaration` — one parsed target from a BUILD file

```rust
#[derive(Debug, Clone)]
pub(crate) struct GoTargetDeclaration {
    pub(crate) kind: GoTargetKind,
    /// The `name=` kwarg. `None` when omitted (Pants default:
    /// `"mod"` for go_mod, dir basename otherwise).
    pub(crate) name: Option<String>,
    /// `import_path=` kwarg from `go_third_party_package`. Only
    /// populated for that kind.
    pub(crate) import_path: Option<String>,
    /// `main=` kwarg from `go_binary`. Path relative to BUILD dir.
    /// Only populated for that kind.
    pub(crate) main: Option<String>,
    /// 1-based line number of the target's opening `(` for diagnostics.
    pub(crate) start_line: u32,
}
```

### `TargetAddress` — resolved Pants target address

Newtype for clarity + `impl Display` for annotation emission.

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TargetAddress(pub(crate) String);
```

### `GoOwnershipIndex` — the enrichment lookup structure

Built from all parsed BUILD-file target declarations. Consulted
by the enrichment pass to attribute each `pkg:golang/*` component.

```rust
#[derive(Debug, Default)]
pub(crate) struct GoOwnershipIndex {
    /// `go_mod` root directory → target address. Longest-prefix
    /// match against a component's `source_path` wins per R3.
    /// Sorted-key iteration order preserves determinism.
    pub(crate) go_mod_roots: BTreeMap<PathBuf, TargetAddress>,
    /// import_path → list of `go_third_party_package` addresses.
    /// Multiple targets can claim the same import path (rare).
    pub(crate) import_path_to_addresses: HashMap<String, Vec<TargetAddress>>,
    /// (main_package_absolute_dir, address) for every `go_binary(main=...)`.
    /// Matched against main-module component's `source_path.parent()`.
    pub(crate) main_targets: Vec<(PathBuf, TargetAddress)>,
    /// (package_absolute_dir, address) for every `go_package`.
    /// Matched against main-module component's `source_path.parent()`
    /// via `starts_with` (package dir contains the source file).
    pub(crate) package_targets: Vec<(PathBuf, TargetAddress)>,
}
```

### `GoTargetParseError` — closed-enum failure mode

```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum GoTargetParseError {
    #[error("target has no name= or required kwarg (line {line})")]
    MissingRequiredKwarg { line: u32 },
    #[error("target has non-string-literal expression at line {line}: {snippet}")]
    NonStringLiteralValue { line: u32, snippet: String },
    #[error("unbalanced parens starting at line {line}")]
    UnbalancedParens { line: u32 },
}
```

### `GoSetupConfig` — parsed `pants.toml` `[golang]` shape

```rust
#[derive(Debug, Default, Deserialize)]
pub(crate) struct GoSetupConfig {
    #[serde(default)]
    pub(crate) golang: Option<GolangSection>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GolangSection {
    /// Operator-pinned minimum Go version. When present + non-empty,
    /// waybill emits a design-tier `pkg:generic/go@<version>`
    /// component with waybill:source-file=pants.toml.
    #[serde(default)]
    pub(crate) expected_version: Option<String>,
    // `min_dot_version` is deliberately NOT parsed per spec Out-of-Scope.
}
```

---

## Existing types being reused (no changes)

### `waybill_common::resolution::ResolvedComponent`

Reused verbatim. The `enrich()` entry point takes
`&mut Vec<ResolvedComponent>` and mutates each matching entry's
`extra_annotations` in place. The `sbom_tier`, `purl`,
`hashes`, `source_path`, `licenses`, and every other field are
left untouched.

### `PackageDbEntry` (for the toolchain-pin component only)

Only used by `pants_go::read()` for the design-tier
`pkg:generic/go@<version>` emission (US2). Field mapping
identical to m225's tool-pin path — see m225's data-model.md
§"Per pinned tool" for the exact mapping. Adapted values:

| Field | Value |
|-------|-------|
| `purl` | `pkg:generic/go@<version>` (via `Purl::new`) |
| `name` | `"go"` |
| `version` | Operator-pinned `expected_version` verbatim |
| `source_path` | Absolute path to `pants.toml` |
| `depends` | `Vec::new()` |
| `lifecycle_scope` | `Some(LifecycleScope::Development)` |
| `sbom_tier` | `Some("design".to_string())` |
| `hashes` | `Vec::new()` |
| `licenses` | `Vec::new()` |
| `extra_annotations` | `waybill:source-file = "pants.toml"` (m080 row) |

### `waybill_common::types::purl::Purl`

Reused verbatim. Only constructed for the toolchain-pin PURL
via `Purl::new("pkg:generic/go@<encoded-version>")`. NEVER
constructed by the enrichment pass — that pass only reads
existing PURLs off `ResolvedComponent` entries.

### C145 `waybill:pants-target` catalog row + 3 extractors

Reused verbatim. No changes to extractor code. Doc-only
description broadening in `docs/reference/sbom-format-mapping.md`
per contracts/c145-broadening.md.

---

## `extra_annotations` mapping (enrichment pass)

The enrichment pass injects at most ONE annotation per matched
component:

| Key | Value | Condition | Catalog row |
|-----|-------|-----------|-------------|
| `waybill:pants-target` | Comma-separated, lexically-sorted list of owning Pants target addresses (e.g., `"3rdparty/go:mod"` or `"cmd/frontend:frontend,cmd/frontend:pkg"`) | Present iff at least one BUILD-file Go target owns the component | C145 (broadened by m226) |

**Merge behavior with existing annotations**: if the component
already has a `waybill:pants-target` annotation (impossible in
practice — no other reader emits it on `pkg:golang/*`), the
enrichment REPLACES it with the merged value. In v1 this is a
no-op safeguard; documented for future readers.

**Zero-fabrication invariant** (FR-012 / Principle IX): if no
target matches a given component, no annotation is added.
`pkg:golang/*` components with no Pants attribution look exactly
like their pre-m226 form. Byte-identity preserved for
non-Pants Go repos.

---

## Discovery + enrichment data flow

```text
┌──────────────────────────────────┐
│  scan_fs::package_db::            │
│    mod.rs::read_all               │
└──────────────┬────────────────────┘
               │ calls
               ▼
┌──────────────────────────────────┐
│  pants_go::read(root, exclude)    │  ← runs INSIDE read_all
└──────────────┬────────────────────┘
               │
               ├─── 1. Read pants.toml at scan_root (if present)
               │      a) TOML-parse via config::parse
               │      b) If [golang].expected_version set → emit
               │         PackageDbEntry(pkg:generic/go@<version>,
               │         design tier)
               │
               └─── returns Vec<PackageDbEntry>
                    (0 or 1 element)

... existing Go reader emits pkg:golang/* components ...
... m191 reconciler runs ...

┌──────────────────────────────────┐
│  scan_fs::mod.rs at ~line 1001    │
│  (after reconcile_design_source_  │
│  tiers, before m148 canonicaliz)  │
└──────────────┬────────────────────┘
               │ calls
               ▼
┌──────────────────────────────────┐
│  pants_go::enrich(root, exclude,  │
│    &mut components)               │
└──────────────┬────────────────────┘
               │
               ├─── 1. Discover BUILD files via safe_walk
               │    (same as m225 pants_shell)
               │
               ├─── 2. For each BUILD file:
               │      a) Extract Go target declarations via
               │         build_dsl::extract_targets
               │      b) Add each parsed target to the
               │         GoOwnershipIndex (per R3/R4 rules)
               │
               ├─── 3. Byte-identity early-return: if the ownership
               │    index is empty AND no components were touched
               │    by any previous pass, return without logging
               │
               ├─── 4. For each pkg:golang/* component in &mut components:
               │      Collect owning target addresses per R3/R4:
               │        - go_mod root longest-prefix match
               │        - import_path direct match
               │        - main-module + main= path resolution
               │        - main-module + go_package dir match
               │      If any addresses collected:
               │        - Sort + dedup lexically
               │        - Inject waybill:pants-target = comma-joined
               │
               ├─── 5. Log FR-012 INFO diagnostic for any
               │    go_third_party_package(import_path=X) with no
               │    matching pkg:golang/* component
               │
               └─── 6. Emit FR-010 INFO log with 6 structured fields
                    (silent when no BUILD files + no pants.toml)
```

---

## Storage / persistence

None. All state (parsed target declarations + ownership index +
`GoSetupConfig`) lives on the stack for the duration of a
single scan. Matches m225 + every language-reader milestone
since 002.

---

## Compatibility

- **Existing Go reader (m053+m055+m160+m161)**: no interaction
  changes. The pants_go enrichment reads
  `PackageDbEntry.source_path`, `PackageDbEntry.extra_annotations["waybill:component-role"]`,
  and `ResolvedComponent.purl.ecosystem() == "golang"` — all
  already stable.
- **m191 reconciler**: no interaction. Enrichment runs
  AFTER `reconcile_design_source_tiers` on the reconciled
  component set. Any `pkg:golang/*` deduplication by m191 is
  transparent to the enrichment pass.
- **m225 pants_shell reader**: independent module (`pants_go/`
  vs `pants_shell/`). Both may activate on the same scan (repos
  with Go + shell scripts both use `pants.toml` for different
  subsystem sections).
- **Existing goldens**: unchanged when no Pants BUILD files
  declaring Go targets AND no `pants.toml` `[golang]` (FR-011 +
  SC-003 enforce this).
- **CI**: adds no new jobs. Existing lint + test lanes cover
  the new test binary.
- **Parity catalog**: **zero new rows or extractor entries**.
  C145 description is broadened via a doc-only edit; extractor
  triple (cdx/spdx2/spdx3) unchanged.

---

## Diagnostic-only fields NOT emitted in v1

- **`GoTargetDeclaration.start_line`** — used for WARN
  diagnostics at parse time; not carried into any component.
- **`min_dot_version`** from `pants.toml` `[golang]` — parsed
  by TOML reader (falls under the same section) but NOT
  extracted or emitted per spec Out-of-Scope.
- **`go_binary.output_path`, `go_package.sources`, `go_third_party_package.checksum`**
  — additional kwargs waybill's regex extractor ignores. If
  operators demand them later, a follow-up spec extends the
  extractor + adds targeted annotations.
