# TheRock at the branch (T023: SC-002 to SC-006, I-1)

**Run:** `census-fetches` 44024737390 in `kusari-sandbox/waybill-fixture-therock-sdk`.
- waybill at `505153ae` (branch) against `7e567e0d` (`main`, v0.17.0), side by side in one run.
- TheRock is at `b1cae390cc74`, with each submodule at its recorded commit. The checkout is the same sparse one `census.md` used.
- Nothing was cloned or scanned locally.

Two earlier branch runs found defects, which are now fixed. Their numbers are superseded, kept here for the record:

| run | waybill | what it showed | fix |
|---|---|---|---|
| 44011629471 | `7d6a5943` | 20 of 46 R9 merges went sibling-to-sibling into the first holder found. 16 googletest fallbacks at five versions collapsed into one subproject's `googletest@f8d7d77`. | `a35b1853`: an enclosing supplier only (C-12c/d/e) |
| 44020737966 | `d3e8b0c6` | rocprim's googletest fallback pins `rocm-libraries`' submodule commit, so PURL dedup merged them. The survivor was required but kept `cmake-not-target` and the fetch's conditions (I-3), and lost `git-submodule`. | `505153ae`: an exact-PURL fallback merges explicitly (C-12f) |

## C231 (SC-002, V1, V2)

```json
{"components":65,"conditional":42,"emitted":110,"merged":{"existing-component":37,"first-declaration-wins":0},"read":191,"test":9,"unparseable-files":0,"unresolved":{"identity":44}}
```

- **V1 holds:** 191 = 110 + 37 + 0 + 44.
- **V2 holds:** 42 + 9 ≤ 65.
- **The document agrees with the row:** 65 fetch components, 42 optional, 9 test, 14 required.

## Every declaration accounted for (SC-002)

**All 191 census declarations are read** (`read` = 191 = the census total).

**File by file:**
- 139 census files declare a fetch. 117 of them are named by the SBOM, through C18 on a fetch component or C233 on a merge target.
- The SBOM names no fetch file that the census does not list.
- The other 22 files hold 23 declarations, all counted under `unresolved.identity`. By kind:
  - 11 literal-name `ExternalProject_Add`: in-tree builds with no repository or URL (`compiler-rt`, `bolt_rt`, `DXC`, `rocm-device-libs` ×2, and others);
  - 6 `ExternalProject_Add` whose name is a `${…}` parameter and whose source is not a GitHub repository;
  - 6 `FetchContent_Declare` with `SOURCE_DIR` only (`fmt`, `googletest`, `nlohmann_json` and `spdlog` in rocprofiler-systems, `rocrand` in hiprand, and `composable_kernel` in miopen). The repository comes from a project helper, or from the monorepo tree. Each was read at its pinned commit.

The remaining 21 unresolved declarations sit in files that also hold a resolved one.

## Shapes (SC-003)

| C122 | components |
|---|---|
| `cmake-find-package-fallback` (`NOT <X>_FOUND`) | 17 |
| `cmake-conditional` | 11 |
| `cmake-function-body` | 6 |
| `cmake-not-target` | 5 |
| `cmake-find-package-args` | 3 |
| none (required 14, test 9) | 23 |

Test scope: 9 components, all C62 `source-test-directory`. The census found 0 `BUILD_TESTING` guards.

Census declarations against components:

| census declarations | components |
|---|---|
| 32 `NOT TARGET` | 5 `cmake-not-target`. Most are googletest fallbacks, and 11 of them went into `rocm-libraries`' googletest submodule. |
| 5 `FIND_PACKAGE_ARGS` | 3 |
| 17 in a function | 6 `cmake-function-body`. 8 of the 17 have parameterised names (above). |

Shapes are counted per component after merging, so the counts are smaller than the census's declaration counts.

## One component per package (SC-004)

- **No PURL repeats** in the document.
- **R9 merges, 37, into two suppliers, both required `git-submodule` components with no conditional rows:**
  - TheRock's own `rocm-cmake` submodule (`.gitmodules` at the root) takes 26 fallbacks;
  - `rocm-libraries`' googletest `e2239ee` takes 11. Of these, 3 pin that exact commit (rocprim in three places), and 8 are fallbacks in `rocm-libraries` subprojects at other versions.
- **First declaration wins: 0.** The census also found 0 shadowed names.
- **googletest still appears at 10 versions.** Each comes from a project with no enclosing supplier, so the repeats are siblings, as SC-004 allows.

## Ownership and the graph (FR-007, I-1)

- Every fetch component has an owner: 0 of 65 lack an incoming edge.

| owner | fetch edges |
|---|---|
| `rocm-libraries` | 42 |
| `rocm-systems` | 27 |
| `llvm-project` | 2 |
| `libhipcxx`, `rocm-docs`, `spirv-llvm-translator` | 1 each |

- **I-1 holds:** the three formats agree on the root's direct targets, 79 in each (`specs/1537-requirements-root-attach/measurements/root_targets.py` on the branch's `scan.*`).
- `graph-completeness` is `partial` at both `main` and the branch.
- **Components:** 113 → 178. **Unique PURLs:** 50 → 115. **PURLs lost against `main`:** 0.

## Cost (SC-006)

| | run 1 | run 2 | run 3 | median |
|---|---|---|---|---|
| `main` | 6.795 s | 6.741 s | 6.749 s | 6.749 s |
| branch | 7.258 s | 7.217 s | 7.288 s | 7.258 s |

The ratio is **1.08×**, against a limit of 1.5×. The two earlier branch runs measured 1.06× and 1.06×.
