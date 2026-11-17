# Research: CMake fetches in nested projects, standalone-build fallbacks marked

Two kinds of evidence back these decisions:
- `measurements/census.md`: TheRock in CI, run 44005855500;
- `measurements/cmake-semantics/`: probes run locally with CMake 4.4.5.

No number below is quoted from documentation alone.

## R1. Which CMake files are read

**Decision.** Fetch declarations are read from **every** `CMakeLists.txt` and `*.cmake` file under the scan root that the existing walk reaches. The walker's skip set and `--exclude-path` still apply. `find_package`, `pkg_check_modules` and `pkg_search_module` are still read only from today's set (FR-002).

**Evidence.** On TheRock, literal `add_subdirectory()` / `include()` reachability from the root reaches 51 of 6,172 files and **0 of 191** fetch calls. 229 `add_subdirectory()` calls name a variable, and subprojects are reached through TheRock's own functions (#1539). Of the 191 calls, 1 is in today's file set.

**Alternatives considered:**
- **Follow `add_subdirectory()` / `include()` from the root.** Rejected: it reaches 0 fetches on the target tree. Resolving variables would mean evaluating CMake, which this reader does not do.
- **Walk everything, including `find_package`.** Rejected by clarification 1, with 2,030 nested `find_package` calls measured (SC-008).

## R2. How a conditional fetch is carried

**Decision.** A conditional fetch is **`LifecycleScope::Optional`**, with **C122 `waybill:optional-derivation`** naming the shape. That renders natively and identically to every other optional dependency:
- CycloneDX `scope: excluded`;
- SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`;
- SPDX 3, through the C122 annotation (the Principle V keep-both carve-out C122 already documents).

**The new C122 values** (an open enum, so no new row):

| value | when |
|---|---|
| `cmake-find-package-args` | `FIND_PACKAGE_ARGS` on the declaration, with no `NEVER` mode in force |
| `cmake-not-target` | an enclosing condition contains `NOT TARGET` |
| `cmake-find-package-fallback` | an enclosing condition contains `NOT <X>_FOUND` |
| `cmake-try-find-package-always` | `FETCHCONTENT_TRY_FIND_PACKAGE_MODE ALWAYS` is set in the declaring directory or an enclosing one |
| `cmake-dependency-provider` | the tree defines a provider whose `SUPPORTED_METHODS` include `FETCHCONTENT_MAKEAVAILABLE_SERIAL` |
| `cmake-function-body` | the declaration is inside a `function()` or `macro()` body, so it runs only if that is called |
| `cmake-conditional` | any other enclosing `if()` / `elseif()` / `else()` |

When several shapes apply, the first in that order wins, and C232 keeps every condition.

**Precedent.** C122 already carries `conan-conditional-requires` (#1521): a Conan `self.requires()` nested under `if`/`for` is optional for exactly this reason.

**Alternatives considered:**
- **C57 `waybill:build-reference`.** Rejected: it is per submodule, and its two values describe whether a submodule is referenced, not whether a fetch is conditional.
- **C60 `waybill:build-inclusion`.** Rejected: its values are Go's `go mod why` verdicts, and `unknown` deliberately sets no native scope.
- **A new marker row.** Rejected under Principle V: Optional + C122 already express it, natively in two formats.

## R3. What CMake actually does with each shape

These were measured with CMake 4.4.5 (`measurements/cmake-semantics/probe.sh`, results in `results.txt`):

| probe | result | rule this feature applies |
|---|---|---|
| P1: the root declares X, and a subproject redeclares X, unguarded | X uses the **root's** source | first-declaration-wins holds across `add_subdirectory()`; it is FetchContent only (P6) |
| P2a: `FIND_PACKAGE_ARGS`, default mode, package findable | found by `find_package`, **not fetched** | conditional |
| P2b: no `FIND_PACKAGE_ARGS` | fetched | unconditional |
| P3: `TRY_FIND_PACKAGE_MODE ALWAYS`, plain declaration | **not fetched** | every FetchContent declaration in or below that directory is conditional |
| P4: `TRY_FIND_PACKAGE_MODE NEVER` + `FIND_PACKAGE_ARGS` | **fetched** | `NEVER` in force cancels `cmake-find-package-args` |
| P5: a dependency provider | sees `FetchContent_MakeAvailable`, **never `ExternalProject_Add`**; may only be set through `CMAKE_PROJECT_TOP_LEVEL_INCLUDES` (P5b: an ordinary `include()` is an error) | a provider affects FetchContent only, if it lists `FETCHCONTENT_MAKEAVAILABLE_SERIAL`; it is a configure-time choice, so it marks a possibility |
| P6: duplicate `ExternalProject_Add` name | configure **error** | first-declaration-wins never applies to ExternalProject |

**Two corrections to the spec follow from this, applied to FR-003:**
- `NOT <X>_FOUND` is a recognised shape. It is TheRock's commonest fallback: 32 + 14 + 5 + 6 conditions in the census.
- A provider counts only for FetchContent, and only with `FETCHCONTENT_MAKEAVAILABLE_SERIAL`.

TheRock's one provider lists `FIND_PACKAGE` only. So on TheRock it marks nothing beyond the `FIND_PACKAGE_ARGS` declarations, which are already conditional.

## R4. Where the condition text goes

**Decision.** A new per-component row, **C232 `waybill:cmake-fetch-conditions`**:
- a JSON array of the enclosing conditions, outermost first, whitespace-normalised;
- an `else()` branch recorded as `else: <condition>`;
- a function or macro body recorded as `function: <name>`.

It is present if and only if the fetch is conditional. One row is emitted in all three formats.

**Principle V audit.** No format has a field for "the build-system condition under which this was declared". CycloneDX `evidence` describes how a component was *identified*, not the conditions it is *built* under.

**Merged declarations.** A component can come from several declarations: same PURL, through PURL dedup. It is Optional only if **every** declaration is conditional. This is the strongest-scope rule from feature 1269: one unconditional declaration means some build fetches it. C232 then holds the union of conditions, sorted.

## R5. First declaration wins

**Decision.** In the CMake reader, a `FetchContent_Declare` whose name, compared case-insensitively as CMake does, is also declared in a file of an **ancestor directory** is not emitted.
- The ancestor's component records it, adding the declaring file to C18 `waybill:source-files`. The superseded declaration's version or tag is recorded in **C233 `waybill:superseded-declarations`**, a JSON array of `{file, version}` sorted by file.
- The C231 count is `merged.first-declaration-wins`.
- `ExternalProject_Add` is never treated this way (P6).

**On TheRock this rule applies 0 times:** its 14 repeated names are all in sibling subprojects. It exists because it is real CMake behaviour (P1), and it is tested with fixtures.

**Alternative considered: C56 alone.** Rejected. C56 records the losing *readers'* mechanisms, and here the loser has the same mechanism and usually a different version, so C56 cannot hold what was superseded.

## R6. Who depends on a nested fetch

**Decision.** Spec 1537's `OwnerIndex` (`scan_fs/declared_attach.rs`), generalised so the CMake pass can use it:
- the owner is the nearest enclosing main module or git submodule, up to and including the scan root;
- the edge is `owner DependsOn fetch`, typed by `apply_lifecycle_scope_to_edges`, so Optional and Test become each format's optional or test spelling;
- with no owner, the fetch is a declared root anchor.

**On TheRock** the owners are submodules: `rocm-libraries`, `rocm-systems` and so on. 111 of the 191 calls are under `rocm-libraries/projects`.

**Alternative considered: leave the existing graph-top fallback to attach them.** Rejected. It attaches to the root, not to the submodule that declares the fetch, and it switches off when the root has another edge (spec 1537 R1).

## R7. Test scope

**Decision.** `LifecycleScope::Test` applies in two cases:
- the declaring file is under a `test`, `tests` or `unittests` directory below the scan root (C62 derivation `source-test-directory`, a new value);
- an enclosing condition contains `BUILD_TESTING`, but not inside an `OR` (C62 derivation `cmake-build-testing`, a new value).

Test outranks Optional: the component's scope is single-valued. C232 still records the condition, which is how FR-008's "keeps the conditional mark" is met. **The spec's FR-008 wording is adjusted to say so.**

**Census.** 28 fetches are under test directories, and 0 use `BUILD_TESTING`.

## R8. Accounting (FR-009, SC-002)

**Decision.** A new document-scope row, **C231 `waybill:cmake-fetch-coverage`**, in the style of C222 and C229: a JSON object with sorted keys.

```
{"conditional":N,"emitted":N,"merged":{"first-declaration-wins":N,"existing-component":N},
 "read":N,"test":N,"unresolved":{"identity":N},"unparseable-files":N}
```

**The invariant, tested:** `read = emitted + merged.* + unresolved.identity`. An unresolvable name (`${…}`) with no literal `set()` in the same file counts as `unresolved.identity`. On TheRock, 8 such names are known in advance.

The row is present only when at least one fetch declaration was read, so scans without CMake fetches are unchanged.

## R9. A fallback the scan already holds (FR-005)

**Decision.** A **conditional** fetch whose identity matches a component another reader emitted is merged into that component. Identity is the repository or URL, without the version: for `pkg:github` this is owner/repo, already lowercased (#1515). The other reader is typically a git submodule, or a FetchContent elsewhere that was emitted unconditionally.
- C56 gets `cmake-fetchcontent-*` when the mechanism differs.
- C233 records the fallback's file and version.
- The C231 count is `merged.existing-component`.

Unconditional fetches are never merged across versions. They are separate claims, and PURL dedup already collapses identical pins.

## R10. Cost (SC-006)

**Baseline:** a scan of the sparse TheRock tree at `main` takes **6.32 s**, median of 3, in the census job (cmake-only files, so the reader's work dominates).

**SC-006's ceiling is 1.5× that, measured in the same job:** the census workflow scans `main` and the branch side by side (T-task). Reading 6,172 files with the existing per-file regex passes is the cost to watch. A single pass per file that extracts every construct at once is the planned shape.

## R11. `--cmake-third-party-recursive`

**Decision.** The flag is kept (FR-012). It still widens `find_package` reading under `third_party/`. Fetches are read everywhere, so for them it no longer matters, and its help text says so.

## R12. Corpus prediction

- **Public corpus:** no target has a CMake-mechanism component (grep of all 21 goldens), so the expected result is byte-identical. A change would be unexplained, and would stop the work.
- **Quality corpus:** only `cpp-mongo` is exposed. It has 116 CMake files, 5 of them with fetch calls, so edges and pkgs may move, and are re-authored with their cause.

## R13. Wrappers (#1518 boundary)

**Decision.** A declaration **inside** a `function()` / `macro()` body is read like any other.
- If its name and identity are literal, it is emitted as Optional, `cmake-function-body`, because it runs only when called.
- If they use the function's parameters (`${name}`), it is `unresolved.identity`.

Resolving a wrapper's *call sites* (TheRock's `therock_subproject_fetch`, `rocm_libs_declare_shared_deps`) remains #1518 and #1539. The census finds 17 declarations in function or macro bodies, 8 of them with `${…}` names.
