# Data model: installed library versions

Everything is in process for one scan. Nothing persists beyond the emitted document.

## Record (phase 1 output, one per file the walker matched)

| Field | Type | Notes |
|---|---|---|
| `kind` | enum `PkgConfig \| CmakeVersion` | from the matched pattern |
| `path` | `PathBuf` | as the walker saw it, under the scan root |
| `name` | `String` | `.pc`: the file stem. CMake: the file name minus `ConfigVersion.cmake` / `-config-version.cmake`. The declared case is kept here and lowercased only for identity. |
| `location` | enum `Recognised { prefix: PathBuf } \| Unrecognised` | R7. `prefix` is the directory above `lib*`, `share`, or `lib/<triplet>` |
| `version` | `Result<String, SkipReason>` | `.pc`: `Version` after variable expansion. CMake: literal `PACKAGE_VERSION` |
| `requires` | `Vec<String>` | module names only, in declared order. `.pc` only |
| `requires_private` | `Vec<String>` | module names only. `.pc` only |

## SkipReason (closed enum, serialised in one place)

`unresolved-version` · `no-version` · `computed-version` · `malformed` · `owned-by-os-package` · `unrecognised-location`

The first four come from parsing, and the rest from phase 2. Each skipped record produces one log warning naming its path.

## OsOwnership (phase 2 input)

| Source | Shape |
|---|---|
| `os_claimed` | `HashSet<PathBuf>` + `HashSet<(u64, u64)>` inodes, filled by dpkg, apk, alpm, opkg and rpm only (R3). Looked up with `binary::is_path_claimed`. |
| Homebrew keg | The record's canonical path is under `<prefix>/Cellar/<name>/<version>/` and that directory holds `INSTALL_RECEIPT.json` |

## Installed library entry (phase 2 output → `PackageDbEntry`)

| Field | Value |
|---|---|
| `purl` | `pkg:generic/<encode(lowercase(name))>@<encode(version)>` |
| `name` | `lowercase(name)` |
| `version` | the resolved version |
| `sbom_tier` | `deployed` |
| `source_path` | the record's path |
| `extra_source_paths` | the matched shared-library files in the same prefix (US3), sorted |
| `depends` | the exact target PURLs from `requires ∪ requires_private`, sorted and deduplicated |
| annotations | C55 `waybill:source-mechanism` = `pkg-config` or `cmake-package-version`. `waybill:pkg-config-private-requires` = a sorted JSON array of the PURLs in `requires_private \ requires`, emitted only when non-empty. `waybill:cmake-package-name` = the declared case, only when it differs (R5) |

**Order in `db_entries`:** after every other package-db entry, ahead of the binary entries. `.pc` entries come before CMake ones (R4).

## Coverage record (document scope, emitted only when phase 1 matched at least one file)

```json
{
  "pkg-config": {"read": N, "emitted": N},
  "cmake-version": {"read": N, "emitted": N},
  "skipped": {"computed-version": N, "malformed": N, "no-version": N,
              "owned-by-os-package": N, "unrecognised-location": N, "unresolved-version": N},
  "unresolved-requires": N,
  "version-disagreements": N
}
```

- **`read`**: counts files the walker matched.
- **`emitted`**: counts entries before the identical-identity merge.
- **`skipped`**: every reason is listed, with zero where it applies.
- **`version-disagreements`**: names whose `.pc` and CMake versions differ within one prefix.

## Resolution of `Requires` (phase 2)

For a module name `m` required by a record in prefix `P`:
1. Use the emitted record named `m` in `P`, if exactly one exists.
2. Otherwise, use the emitted record named `m` anywhere in the scan, if exactly one exists.
3. Otherwise `m` is unresolved: there is no edge, and `unresolved-requires` is incremented.

An OS-owned target is not emitted, so it is never an edge target either. The requirement stays unresolved, because the OS package's own identity is a different name.
