# Contract: annotations C245 and C246, and the Principle V audit

## Principle V native-field audit

| Fact | CycloneDX 1.6 | SPDX 2.3 | SPDX 3 | Used |
|---|---|---|---|---|
| Whether the component is in the product | `component.scope` | relationship type (`DEPENDS_ON` vs `OPTIONAL_DEPENDENCY_OF`) | `LifecycleScopedRelationship.scope` | **Native**, exactly as specs 1538 and 1641 use it |
| The project compiled against the component | `dependencies[].dependsOn` | `DEPENDS_ON` | `dependsOn` relationship | **Native** |
| This fetch's outcome in a specific build, and its basis | none. `formulation` describes how a build was performed (workflows, tasks), not whether a declared dependency was used, and `evidence.identity` is about identity, not use | none | none (`Build` profile records build runs, not per-dependency outcomes) | **C245** |
| What the build-tree reader read and counted | none | none | none | **C246** |

## C245 `waybill:cmake-build-settlement` (per component)

- **Shape:** a JSON object, encoded as a string with sorted keys.
- **When emitted:** only when `--cmake-build-tree` is given, and only on
  CMake fetch components and build-only components.

```json
{"basis":["cache-source-dir","deps-src-dir"],"build-only":false,"builds":["build:."],"state":"taken"}
```

| Key | Type | Values |
|---|---|---|
| `state` | string | `taken`, `not-taken`, `incomplete`, `no-evidence` |
| `basis` | sorted array of strings | `cache-source-dir`, `fetchcontent-source-dir-entry`, `deps-src-dir`, `source-dir-override`, `cache-package-dir` (not taken), `download-stamp`, `stamp-dir-only` (incomplete), `compile-include`. Empty for `no-evidence`. |
| `build-only` | bool | `true` when no source declaration matched (FR-007) |
| `builds` | sorted array of strings | the build directories whose evidence gave this `state`, as `build:<path relative to the named tree>`. With more than one named tree, `build<N>:<rel>`, where `N` is the tree's position on the command line. Omitted for `no-evidence`. |

**Parity:** `SymmetricEqual`, `order_sensitive: false`, with extractors in
CycloneDX, SPDX 2.3 and SPDX 3.

## C246 `waybill:cmake-build-tree-coverage` (document scope)

- **Shape:** a JSON object, encoded as a string with sorted keys.
- **When emitted:** only when `--cmake-build-tree` is given.

```json
{"build-dirs":3,"build-only":0,"build-trees":1,"compile-edges":2,"conflicts":0,
 "configured-values-applied":2,"declarations":{"incomplete":0,"no-evidence":1,"not-taken":1,"taken":4},
 "file-api-replies":1,"include-dirs-unresolved":0,"signal-disagreements":0,
 "source-mismatch":0,"unreadable-files":0}
```

| Key | Meaning |
|---|---|
| `build-trees` | `--cmake-build-tree` paths given |
| `build-dirs` | Build directories found (settlement.md, matching rule 1) |
| `declarations.*` | Settled CMake fetch declarations, by state, counted **before** the per-PURL merge |
| `build-only` | Components emitted under FR-007 |
| `configured-values-applied` | Conditions whose evaluation changed because of a configured value |
| `compile-edges` | Edges in the graph that `compile_commands.json` or a file-API reply supports: distinct (owner, component) pairs after the owner rule, excluding a component compiling itself. An edge a declaration already gives counts, but isn't added twice (#1717 review) |
| `include-dirs-unresolved` | Distinct include directories that resolved to no component |
| `file-api-replies` | Build directories with a readable reply |
| `conflicts` | Declarations whose build directories gave different states |
| `signal-disagreements` | FetchContent cases where the R1 signals disagreed |
| `source-mismatch` | Build directories whose `CMAKE_HOME_DIRECTORY` is not inside the scan root |
| `unreadable-files` | Cache, compile-database or reply files that could not be read or parsed |

**Invariants**, asserted on every test scan:
- `sum(declarations.*)` = number of C245 values with `build-only: false`.
- `build-only` = number of C245 values with `build-only: true`.

## Paths (FR-006)

Every path in C245, C246 and the logs is a `RedactedPath`, which takes one of
four forms:
- `build:<rel>` or `build<N>:<rel>`: inside a named tree;
- `<rel>`: inside the scan root;
- `<prefix>/<rel>`: under a `<name>_DIR` install prefix;
- `<external>`: anything else.

No raw cache value of type `PATH`, `FILEPATH`, `INTERNAL` or `STATIC` is ever
emitted.

## Docs rows

`docs/reference/sbom-format-mapping.md` gets C245 and C246, next to C231–C243,
each citing this audit. The consumer guide gets entries next to C232.
