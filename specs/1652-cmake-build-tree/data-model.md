# Data model: spec 1652

Everything here is in-process for one scan. Nothing persists beyond the emitted SBOM.

## BuildEvidence

Loaded once per scan from the `--cmake-build-tree` paths
(`cmake/build_tree/mod.rs`).

| Field | Type | Notes |
|---|---|---|
| `trees` | `Vec<BuildTreeRoot>` | in command-line order; the index gives `build<N>:` |
| `dirs` | `Vec<BuildDir>` | sorted by redacted path, for deterministic output |
| `coverage` | `BuildTreeCoverage` | becomes C245 |

## BuildTreeRoot

| Field | Type | Notes |
|---|---|---|
| `index` | `usize` | position on the command line |
| `canonical` | `PathBuf` | never emitted |

## BuildDir

| Field | Type | Notes |
|---|---|---|
| `id` | `BuildDirId` (newtype over `RedactedPath`) | `build:<rel>` |
| `root` | `PathBuf` (canonical) | never emitted |
| `home` | `Option<PathBuf>` | `CMAKE_HOME_DIRECTORY`, canonicalised. `None` gives *no evidence* for every declaration (settlement.md, ownership) |
| `cache` | `CacheView` | see below |
| `populations` | `BTreeSet<String>` | `<lc>` for each `_deps/<lc>-src/` |
| `stamps` | `BTreeMap<String, StampState>` | ExternalProject name → `Downloaded` or `StampDirOnly` |
| `compile_includes` | `Vec<PathBuf>` | from `compile_commands.json` / a reply. Absolute, never emitted |
| `file_api` | `Option<FileApiReply>` | reply index → cache and codemodel |

## CacheView

The parsed `CMakeCache.txt`.

| Field | Type | Notes |
|---|---|---|
| `entries` | `HashMap<String, CacheEntry>` | the name is case-sensitive, as CMake stores it |

`CacheEntry { kind: CacheKind, value: String }`.
`CacheKind` = `Bool | String | Path | FilePath | Internal | Static | Uninitialized | Untyped`.

Validation:
- A malformed line is skipped and counted in `unreadable-files` only when
  the whole file fails to parse.
- Comments `#` and `//` are ignored.
- `NAME:TYPE=VALUE` splits at the first `=` after the first `:`.

## SettlementState / Settlement

```text
enum SettlementState { Taken, NotTaken, Incomplete, NoEvidence }   // precedence Taken > NotTaken > Incomplete > NoEvidence
enum EvidenceKind   { CacheSourceDir, FetchContentSourceDirEntry, DepsSrcDir, SourceDirOverride,
                      CachePackageDir, DownloadStamp, StampDirOnly, CompileInclude }
struct Settlement   { state: SettlementState, basis: BTreeSet<EvidenceKind>,
                      builds: BTreeSet<BuildDirId>, build_only: bool }
```

**State transitions.** A declaration starts at `NoEvidence`. Each owning build
directory proposes a state, and the highest-precedence proposal wins. A
proposal that differs from the current state bumps `conflicts`.
`Taken`/`NotTaken` are the only states that change anything beyond C244, and
only `Taken` changes scope (contracts/settlement.md).

## ConfiguredOverlay

The input to the evaluator (`conditions.rs`).

| Field | Type | Notes |
|---|---|---|
| `values` | `HashMap<String, Configured>` | from `Bool`, `String`, `Uninitialized` and `Untyped` entries only |

`Configured = Known(String) | Conflicting`.
- `Defaults::lookup` consults the overlay first.
- A boolean reading uses CMake's truthiness: `ON`, `YES`, `TRUE`, `Y`, or a
  non-zero number holds; `OFF`, `NO`, `FALSE`, `N`, `0`, empty, `IGNORE`,
  `NOTFOUND` or `*-NOTFOUND` does not.
- `Conflicting` reads as `Unknown`.

## RedactedPath (newtype, `build_tree/redact.rs`)

`RedactedPath(String)` is built only by `Redactor::redact(&Path)`, with roots
(named trees, scan root, known install prefixes). It has four forms:
`build[N]:<rel>`, `<rel>`, `<prefix>/<rel>`, `<external>`. There is no
`From<String>`, so a raw path cannot become one.

## BuildTreeCoverage → C245

Its fields are the keys in contracts/annotations.md. Counters are `u64`, and
it is serialised with sorted keys.

## Relationships to existing types

- **`PackageDbEntry`** (fetch entries from `fetches.rs`) gains, through
  `extra_annotations`, `waybill:cmake-build-settlement` (C244). On `Taken`,
  its `lifecycle_scope` is set to `None` unless it is `Test`, and C122 is
  removed.
- **Spec 1618's CMake project component** (C55 `cmake-project`) for a build
  directory's `home` is the source of compile edges.
- **`ScanDiagnostics.cmake_build_tree_coverage: Option<String>`** is added
  next to `cmake_fetch_coverage` (`package_db/mod.rs:407`), and threaded
  through `ScanResult` → `ScanArtifacts` → the three emitters, like C231.
