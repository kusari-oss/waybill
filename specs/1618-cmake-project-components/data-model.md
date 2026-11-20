# Data model

## ProjectDeclaration (in process, CMake reader)

One per literal `project(<name> …)` call read in the existing single pass (research R1).

| Field | Type | Source |
|---|---|---|
| `name` | lowercased `String` | first argument; a declaration whose name contains `${` is skipped, as today |
| `version` | `DeclaredVersion` | the `VERSION` argument |
| `file` | path relative to the scan root | the declaring listfile |
| `reached` | `bool` | whether the scan-root `CMakeLists.txt` reaches the file (R9); `true` when there is no scan-root listfile |

`DeclaredVersion` is one of:
- `Literal(String)`: `VERSION 1.2.3`;
- `Resolved(String)`: `VERSION ${X}` with a same-file literal `set(X …)`;
- `Expression(String)`: `VERSION ${X}`, unresolved, kept verbatim;
- `NotDeclared`: no `VERSION`.

## ProjectComponent (emitted `PackageDbEntry` → `ResolvedComponent`)

One per distinct (name, effective version). Two declarations merge when their names match and their versions are equal. A `NotDeclared` or `Expression` declaration merges into a versioned one of the same name (R5).

| Field | Value |
|---|---|
| PURL | `pkg:generic/<name>@<version>`, or `pkg:generic/<name>` when there is no version |
| `sbom_tier` | `source` |
| `waybill:component-role` | `main-module` |
| `waybill:source-mechanism` (C55) | `cmake-project` (new value) |
| `evidence.source_file_paths` | every declaring `CMakeLists.txt`, sorted |
| `waybill:version-absence` (C197) | `not-declared` iff every declaration is `NotDeclared` |
| `waybill:declared-version-expression` (C238) | the expression, iff the effective version came from an `Expression` |
| `lifecycle_scope` / C62 | `Test` / `source-test-directory` under `TEST_DIRS`; `Development` / `source-example-directory` under an example or benchmark directory (R8); otherwise none, unless R4's test-only-search rule applies |
| `waybill:cmake-build-reach` (C239) | `not-reached` iff no declaration is reached |
| `waybill:vendored-signals` (C241) | sorted JSON array of the signals that hold, if any (R10) |
| `waybill:vendored` (C240) | never set (spec Clarifications, 2026-11-20) |
| C236 | the union of the conditions of the conditional searches paired with it |

## Pairing (extended, `cmake_search_pair`)

Adds `project: bool`. The order is unchanged for earlier kinds: installed record, then submodule, then project. A fetch is paired earlier, in `cmake.rs`.

## OwnerIndex (extended, `declared_attach`)

Ranks, best first:
1. `RANK_PYPI_MAIN_MODULE`;
2. `RANK_OTHER_MAIN_MODULE`;
3. **`RANK_CMAKE_PROJECT`** (new);
4. `RANK_SUBMODULE`.

Adds `enclosing(dir, own)`: the best other owner of the project's own directory (another ecosystem's main module there, or the submodule whose root it is), else the nearest owner of an ancestor; never the project itself. Used for a project's enclosing edge (R4). It replaces the planned `owner_above`, which would have skipped a submodule sharing the project's directory.

## FindCoverage / C237 (extended)

`paired` gains `project`, the count of searches paired with a project component. It is **serialised only when non-zero**, so C237 is byte-identical for scans with no project components (FR-012). `in-tree` lists only names with no project component (R6).

## State transitions

None. Everything is in process for one scan.
