# Research: TheRock's last two unexplained components

The evidence for every decision is in `measurements/trace.md`.

## R1. Workspace roots are anchors, wherever they are not the subject (FR-002, FR-003)

**Decision.** The scan pass collects every synthesized workspace root that no relationship targets, and adds it to `declared_root_anchors`. `root_edges::synthesized_root_targets` then gives each format's root an edge to it:
- after the fallback gate is decided, so the gate is unchanged;
- never when the workspace root *is* the root (`t != root_ref`, already enforced there).

**Rationale.**
- The anchor channel is the existing "the root depends on this, whichever root a format picks" mechanism. Root choice is per format at emit time, so this mechanism is required: the scan cannot name the root.
- It covers all seven stranded cells at once, the `--root-name` cells and the subdirectory cells alike. The at-root plain cells are covered by R2.
- Members are untouched (FR-003). Their milestone-860 anchors stay, and the workspace root's own edges to them stay. A member is then reached twice, never contradicted.

**Alternatives rejected.**
- *Treat the workspace root as a retained main module in `anchor_retained_mainmod_edges`.* That covers only `--root-name`. The trace shows the gate closes for other reasons too, such as a Cargo main module at the root with dependencies of its own.
- *Attach it to its owner through `OwnerIndex`, as fetches are.* The nearest enclosing project of a workspace root is usually one of its own members, in the same directory (`tensile@5.0.0` beside `pkg:generic/tensile`). Excluding members first still puts the root at a different level from its members, which are anchored to the document root under `--root-name`. The maintainer chose the document root.

## R2. At the scan root, the workspace root is the subject (clarification 1)

**Decision.** `root_selector` gains a rung ahead of the main-module ladder, beside `evaluated_nix_package` (`root_selector.rs:407`). Without an operator override, if *exactly one* component has role `workspace-root` and its first `waybill:source-files` entry contains no `/`, it is the subject. The files are scan-root-relative: measured `pyproject.toml`, `package.json`, `settings.gradle.kts` at the root and `proj/settings.gradle.kts` below it. So the rung is a pure function of the components, and no caller needs the scan root. Its C69 heuristic is a new registered value, `workspace-root` (confidence 0.95, matching `repo-root-main-module`), and the members become `losers`. None of C69's seven existing names describes this subject. With two or more, none is promoted; R1 anchors them all, and the existing ladder picks the subject as today.

**Rationale.**
- Measured, uv and bun currently pick a *member* as the subject, so the workspace is described as one of its parts.
- The workspace root is what was scanned. Kotlin's placeholder `atroot@0.0.0` becomes the named root `pkg:generic/<rootProject.name>@0.0.0`.
- No corpus golden holds a workspace root (trace §1). The quality corpus is traced against `main` (SC-005).

**Alternatives rejected.**
- *Pick by ecosystem priority among several.* Their PURLs are all `pkg:generic`, so there is no ecosystem to rank by. Not promoting keeps the existing behaviour for a shape no fixture or corpus target has.

## R3. Follow `include()` for link declarations (FR-004, US2 scenario 5)

**Decision.** While reading a walked CMake file for link declarations, an `include(<arg>)` is followed when all of these hold:
- the argument, after substituting `${CMAKE_CURRENT_SOURCE_DIR}`, `${CMAKE_CURRENT_LIST_DIR}`, `${PROJECT_SOURCE_DIR}` and `${CMAKE_SOURCE_DIR}`, has no `${` left;
- it names an existing file inside the scan root;
- it contains a path separator or a `.` extension, so `include(FetchContent)` and other module names are not file paths.

`CMAKE_CURRENT_SOURCE_DIR` stays the *including* `CMakeLists.txt`'s directory. `CMAKE_CURRENT_LIST_DIR` is the included file's own directory, which is CMake's semantics. The included file is read with the include site's open blocks already on the stack. A visited set and a depth bound of 8 stop cycles.

**Scope.** This applies to link declarations only. Reading fetches, `find_package` and downloads through `include()` would also cure the condition gap noted in trace §2, but it would move fetch output on corpus targets. That is filed as #1605, with this mechanism as the starting point.

**Alternatives rejected.**
- *Widen the walker to `CMakeLists*.txt`.* That reads the WSL file, but without the `if (WIN_SDK)` that makes it conditional, so SC-003 would be false. It also reads files nothing includes.

## R4. Link-path correlation (FR-004, FR-006)

**Decision.** Per CMake file (with its includes):
- **Collected.**
  - `link_directories(<dirs>)`, which applies to every link later in the file;
  - `target_link_directories(<tgt> [BEFORE] <scope> <dirs>)`;
  - `target_link_libraries(<tgt> <items>)` and `link_libraries(<items>)`.
- **A plain item** contains no `::`, `/`, `\`, `$<` or `${`, does not start with `-`, and is not a keyword (`PUBLIC`, `PRIVATE`, `INTERFACE`, `debug`, `optimized`, `general`).
- **A name N pairs with a directory D** when D is from `link_directories` earlier in the file, or from `target_link_directories` with the textually same target token. Directories are resolved like the existing `collect_cmake_path_references`, then lexically normalised.
- **Correlation.** N links an installed-library record R (cmake-version or pkg-config, spec 1531) when D lies inside R's prefix (`installed_lib::location::prefix_of` on R's source file) and either:
  - N equals R's name, case-insensitively; or
  - D holds `lib<N>.a`, `lib<N>.so`, `lib<N>.so.*`, or a DVC pointer to one of them (R6).
- **Nested prefixes:** when D lies inside several matching records' prefixes, only the record with the longest prefix is linked.
- **Otherwise nothing:** no component, edge or record is created (FR-006).

**Where.** The CMake reader records link declarations (file, directory, name, conditions) in a scan-scoped list, like its fetch coverage. A pass beside `cmake_fetch_attach` correlates them, and runs where fetches run (`scan_fs/mod.rs`):
- edges and component scope in the owner block (≈1550), before `apply_lifecycle_scope_to_edges` (≈1554) and the dedup;
- root anchors at the `declared_root_anchors` computation (≈1665), from the final components, so a PURL the reconciler rewrote is anchored under its final name.

## R5. Owner, scope and conditions (FR-004, FR-005)

**Decision.**
- **The edge** comes from the linking file's owner, through `OwnerIndex::owner_of`, the rule `cmake_fetch_attach` uses. With no owner, the record becomes a declared root anchor. Provenance `data_type` is `cmake-link-declaration`. On TheRock the owner is the `rocm-systems` submodule.
- **Scope** (spec 1538 R4: Optional only if every declaration is):
  - if every link to R is inside at least one block, R gets `LifecycleScope::Optional`, with C122 = `cmake-conditional-link` (new value) and the new row `waybill:cmake-link-conditions` (C235). That row holds the sorted union of every link's conditions, normalised like C232.
  - if any link is unconditional, R's scope is unchanged and it carries no C235.
  - an existing non-runtime scope from another declaration is kept. This uses the spec 1538 merge rule, test outranks optional.
- **Native carriers.** CycloneDX `scope: excluded` and SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` come from the existing scope machinery (C42, C122). The Principle V audit for C235 is C232's: no format records the build-system condition a dependency is declared under.

## R6. DVC pointers get no annotation (FR-007)

**Decision.** A file named `<library>.dvc` in the link directory counts as `<library>` for R4's match. Nothing more is emitted.

**Rationale.**
- The document says nothing about a library's presence for any other record. A presence marker that exists only for DVC would be asymmetric.
- The pointer's content (md5, size) identifies a blob in a remote cache waybill does not fetch.
- No consumer was found that needs it.
- The match only needs the name. DVC's YAML is not parsed, so a malformed pointer is still a name.

**Alternatives rejected.**
- *A row `waybill:library-presence` = `dvc-pointer`.* It would be a new row with one producer and no consumer. If a consumer appears, it is a later, additive row.

## R7. Measuring TheRock

**Decision.** In CI only, through `kusari-sandbox/waybill-fixture-therock-sdk`:
- `measure-source` (interleaved branch/`main`, ≥5 pairs) for SC-002, SC-003, SC-004 and SC-006;
- `measure-full` once, to show the built SDK is unchanged.

Probed with the corpus harness's `probe_i7_on_document` and `row_all_formats`, as in specs 1541 and 1542. The workflow inputs are `--ref main -f waybill_ref=<branch sha> -f base_ref=main`.
