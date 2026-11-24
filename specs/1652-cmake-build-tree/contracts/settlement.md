# Contract: settling a fetch declaration from build evidence

The testable core of spec 1652 (FR-002 to FR-004, FR-007). Every rule
comes from research R1–R3, which were measured on 24 build trees. Each row needs
a fixture, and removing any row must fail a test (SC-007).

## Matching a declaration to a build directory

1. **Build directories.** Every directory beneath a named build tree that holds
   a regular file `CMakeCache.txt`. Discovery does not stop at a match, because
   TheRock-style super-builds configure each subproject separately (research
   R7). A cache whose `CMAKE_HOME_DIRECTORY` lies inside a named build tree is a
   **sub-build** of downloaded content, not a build of the scanned source:
   FetchContent's `_deps/<lc>-subbuild/` and ExternalProject's
   `<name>-prefix/src/<name>-build/` both have one (observed in every probe
   tree, T003). A sub-build owns no declarations and adds no configured
   values. Its stamps and populations still count for the build directory that
   contains it.
2. **Source directory.** A build directory's source directory is the
   `CMAKE_HOME_DIRECTORY` cache entry.
3. **Ownership.** A declaration belongs to the build directories whose source
   directory is the deepest ancestor of the declaring file. The comparison is
   on canonical paths. A declaring file outside every build's source directory
   has *no evidence*.
4. **Name keys.** For FetchContent, the key is the declared name lower-cased
   (`<lc>`) for directories and `<lc>_SOURCE_DIR`, and upper-cased (`<UC>`) for
   `FETCHCONTENT_SOURCE_DIR_<UC>`. For ExternalProject, the key is the declared
   name as written. Both rules were measured (research R1, T004).

## FetchContent states (per build directory)

| State | Rule (all from research R1, R2) |
|---|---|
| **taken** | The cache has `<lc>_SOURCE_DIR`, **or** `FETCHCONTENT_SOURCE_DIR_<UC>` is present, **or** `_deps/<lc>-src/` exists. In the probe all three always agreed. If they disagree, *taken* wins and the disagreement is counted in C245. |
| **taken, overridden** | *Taken*, and `FETCHCONTENT_SOURCE_DIR_<UC>` is non-empty: the build used a local copy. C244's basis says `source-dir-override`, and the path is never emitted. |
| **not taken** | No *taken* signal, **and** the cache has `<lc>_DIR` (or `<name>_DIR`) whose value does not end in `CMakeFiles/pkgRedirects`. |
| **no evidence** | None of the above. This covers a declaration never made available, one that is gated off, and one that is not in this build. |

## ExternalProject states (per build directory)

| State | Rule (research R3) |
|---|---|
| **taken** | A file named `<name>-download` exists anywhere beneath the build directory, outside any nested build directory. It is found by name because `PREFIX` and `STAMP_DIR` move it, for example in TheRock's `therock_subproject_fetch`. |
| **incomplete** | No `<name>-download`, but a sibling stamp named `<name>-mkdir`, or a directory named `<name>-stamp`, exists. Configure creates the stamp directory, so on its own it is never *taken*. |
| **no evidence** | Neither of the above. |

## Combining build directories (FR-002)

Across every build directory that owns the declaration, the precedence is
*taken* > *not taken* > *incomplete* > *no evidence*. Disagreement between
build directories is counted in C245 as `conflicts`.

## Effect on the component (FR-004)

| State | Scope | C122 | C244 |
|---|---|---|---|
| taken | In scope. Optional is cleared, whatever the fallback shape or condition. **Test is kept** (spec 1538 R7). | removed | `{"state":"taken",…}` |
| not taken | Unchanged from source | unchanged | `{"state":"not-taken",…}` |
| incomplete | Unchanged from source | unchanged | `{"state":"incomplete",…}` |
| no evidence | Unchanged from source | unchanged | `{"state":"no-evidence"}` |

- **Without `--cmake-build-tree`:** C244 is never emitted, and the output is
  byte-identical to `main`.
- **Merging into one component per PURL:** spec 1538's merge
  (`one_per_purl`, `merge_into_existing`) runs **after** settlement. A
  component is *taken* if any declaration merged into it is.

## Build-only populations (FR-007)

These are a `_deps/<lc>-src/` population, or an ExternalProject `<name>-download`
stamp, that no declaration in the owning source directory matches by key. Each
gives one component:
- **PURL:** the same form the source reader uses for a fetch with no resolvable
  URL (`cmake.rs:1192`, the unresolved case), named `<name>` as the build
  wrote it.
- **Scope:** in scope.
- **C244:** `{"state":"taken","build-only":true,…}`.
- **Source mechanism:** `cmake-fetchcontent` or `cmake-externalproject`.

Info files (`*-urlinfo.txt`, `*-gitinfo.txt`) are **not** read (research R4),
so a build-only component carries no URL or hash.

## Configured values (FR-005)

- **Overlay.** For each build directory, the cache entries of type `BOOL`,
  `STRING` or `UNINITIALIZED`, plus untyped entries, form an overlay. The
  overlay applies to every declaring file whose deepest-owning build directory
  it is.
  - Values from the overlay replace source defaults of the same name.
  - Several build directories owning the same file with different values make
    that name *unknown*, the same rule as conflicting source defaults.
- **Comparisons.** A comparison whose operands are now both known is
  evaluated. This covers `STREQUAL`, `EQUAL`, `MATCHES` (CMake regex),
  `VERSION_LESS`, `VERSION_GREATER`, `VERSION_EQUAL`,
  `VERSION_LESS_EQUAL` and `VERSION_GREATER_EQUAL`. Anything else stays
  *unknown*, and is kept in scope (spec 1641 FR-004).
- **What is never an input.** `PATH`, `FILEPATH`, `INTERNAL` and `STATIC`
  entries are never condition inputs, and are never emitted.
