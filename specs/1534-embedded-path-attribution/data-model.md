# Data model: embedded-path attribution

Everything is in process for one scan.

## VersionedNames (before the binary scan)

The set of lowercased names of the #1531 records that will become versioned components. A record qualifies when it has:
- a recognised prefix;
- a parsed version;
- no OS owner.

Names on the stoplist (`amd`) are removed.

## EmbeddedPathFinding (per binary, from the binary scan)

| Field | Meaning |
|---|---|
| `packages` | the sorted set of names from `VersionedNames` that are the **outermost** matching directory of at least one compiled-source path |
| `header_only` | `true` when only header paths name a versioned package |
| `bundled` | the sorted set of bundled-tree names (`third_party/`, `third-party/`, `thirdparty/`, `3rdparty/`, `external/`, `extern/`, `vendor/` `<name>`, or `_deps/<name>-src`), minus the exclusion list (`sysdeps`) |
| `has_paths` | at least one absolute source path was found |
| `toolchain_only` | every path found was a toolchain path (`/usr/`, `/builddir/`, `/opt/`) |

**Path rules, applied in order:**
1. **Only absolute paths with a source extension are considered.** Compiled sources are `.c .cc .cpp .cxx .c++ .hip .cu`. Headers are `.h .hh .hpp .hxx .inl .inc .cuh .ipp .tcc`.
2. **A toolchain path is skipped.**
3. **Walk the directory components from the root.**
   - The first vendoring marker makes the path bundled. It never attributes.
   - Otherwise, the first component equal to a `VersionedNames` entry is the match. A compiled-source match is recorded in `packages`; a header-only match is recorded in `header_only`.

**Source region:** the scanner's existing read-only string region (≤ 16 MiB), split on NUL.

## Attribution (phase 2, `installed_lib::resolve`)

For each binary file component not already in a #1531/#1563 evidence list:
- **`packages` = {p}:** attach the file to `p`'s live record (the CMake version record first, then a same-name `.pc`) through `extra_source_paths`. Record it in C227 as `{"path": rel}`.
- **`packages` = {p, q, …}:** attach the file to each, and record it in C227 as `{"path": rel, "shared-with": [the other names]}`.
- **Otherwise:** it is counted only.

## Bundled component (binary scan)

| Field | Value |
|---|---|
| `purl` | `pkg:generic/<name>` (no version) |
| `name` | `<name>` |
| `version` | absent (the existing version-absence handling) |
| `sbom_tier` | `analyzed` |
| `evidence_kind` | `embedded-source-path` |
| `confidence` | `heuristic` |
| `parent_purl` | the binary file component's PURL |
| `source_path` | the binary's path |

There is one per (binary, name) pair. The same name in two binaries gives two components, each under its own binary.

## Coverage

**C225, `embedded-paths` object.** Present when phase 2 ran and the binary scan reported any finding:

```json
{"attributed":N,"bundled":N,"header-only":N,"no-versioned-match":N,"read":N,"shared":N,"toolchain-only":N,"with-paths":N}
```

**C227 `waybill:embedded-path-binaries`.** On an installed-library component: a sorted JSON array of `{"path": "<root-relative>"}` or `{"path": …, "shared-with": [...]}`.
