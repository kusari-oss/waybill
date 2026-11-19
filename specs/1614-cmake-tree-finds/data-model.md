# Data model: Read find_package from every CMake file, with the conditions that reach it

All state is in process for one scan.

## Reach (per CMake file)

```text
enum Reach {
    Unconditional,                    // some path reaches the file with no condition, or nothing reaches it
    Conditional(BTreeSet<String>),    // every path passes a condition; the union, in BlockStack::conditions() form
}
```

**Edges into a file:**
- `include(<path>)`, resolved by `links.rs::resolve_include` against the includer's source directory, carrying the blocks open at the call;
- `add_subdirectory(<dir>)`, resolved against the current source directory to `<dir>/CMakeLists.txt`, carrying the blocks open at the call.

**Rules:**
- A file is `Unconditional` if any edge into it is unconditional and comes from an `Unconditional` file.
- A file with no edges is `Unconditional`.
- A file is otherwise `Conditional`. Its conditions are the union, over every edge, of the edge's own conditions and the source file's conditions.
- A cycle is `Unconditional`.

**Also produced:** `extra_files`, the files reached by `include()` that the walker did not hand the reader (R2).

## PackageSearch (per declaration)

| Field | Source |
|---|---|
| kind | `find_package` / `pkg_check_modules` / `pkg_search_module` |
| name (declared casing) | first argument; a name holding `${` is skipped and counted as unresolved |
| version range | `find_package_requirement` (#1520) |
| components | `COMPONENTS` list (R6's Python rule) |
| file | the declaring file |
| conditions | `Reach` conditions, then the file's open blocks |
| in test directory | as `fetches.rs` computes it |

## Outcome (per package name, after all declarations)

The first match wins. Steps 1, 2 and 6 are decided in the CMake reader. Steps 3–5 are decided in the post-pass (`cmake_search_pair.rs`), after `installed_lib::resolve`. The reader therefore does **not** drop an in-tree name: it emits the search component tagged in-tree, and the post-pass drops it only if steps 3 and 4 did not claim it. A git submodule is usually a CMake `project()` too, so dropping it early would lose its pairing edge.
1. **Tool** (FR-006): no component, counted.
2. **Paired with a fetch** (R7.1): no component; the fetch gains the search files as evidence.
3. **Paired with an installed-library record** (R7.2): no component; the record gains evidence and the combined scope rule applies.
4. **Paired with a git submodule** (R7.3): no component; an edge from the owner to the submodule.
5. **Built in tree** (FR-007): no component; the name goes in C237 `in-tree`.
6. **Component:** `pkg:generic/<lowercased name>`, as today.
   - **Scope:** the strongest across its declarations (`one_per_purl`), using `mark`'s rule: Test, then Optional, then required.
   - **Annotations:** C236 holds the union of conditions when non-runtime. C122 is `cmake-conditional`, or C62 `source-test-directory` / `cmake-build-testing`.

## FindCoverage, C237 (per scan)

`read`, `unresolved`, `tools`, `paired.{fetch, installed, submodule}`, `in-tree` (sorted names), `in-tree-declarations`, `components`, `conditional`, `test`.

- **Counted by declaration:** `read`, `unresolved`, `tools`, `paired.*`, `in-tree-declarations`.
- **Counted by component:** `components`, `conditional`, `test`, as in C231.
- **Listed by name:** `in-tree`.
- **Invariant:** `read` = `unresolved` + `tools` + `paired.fetch` + `paired.installed` + `paired.submodule` + `in-tree-declarations` + the declarations of emitted components.

## Changed existing state

| What | Change |
|---|---|
| C232 values | a fetch's conditions also include its `Reach` |
| C57 (`git_submodule::build_reference`) | names from the walked declarations, not `discover_cmake_files` |
| CMake registration patterns | `+ **/*Config.cmake.in`, `**/*-config.cmake.in` (names only) |
