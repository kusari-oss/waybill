# TheRock `download_project` census (SC-001)

**Run:** `census-fetches` 44137827351 in `kusari-sandbox/waybill-fixture-therock-sdk`, fixture commit `fecd9b4`.
- **Tree:** TheRock at `b1cae390`, each top-level submodule at its recorded commit (`census-refs.txt`). The checkouts are sparse and blob-less, holding only `*.cmake`, `CMakeLists.txt` and `.gitmodules`. TheRock was not cloned locally.
- **Script:** `census/download_project.py`, copied here as `census-download-project.py`. It reads files and evaluates nothing, and was validated first on a synthetic tree: a shaped definition, a copy with an added keyword, a same-named function without the shape, and calls with a branch, a commit, a release tag, a variable repository and no identity.
- **Output:** `census-download-project.json`.
- **Baseline:** the same run scanned the tree with waybill `main` at `f0133adc` (spec 1539 landed).

## Definitions

| | count |
|---|---|
| definitions of `download_project` | **9**, in 5 distinct file versions |
| … with DownloadProject's shape (prefix `DL_ARGS`, `PROJ` among the resolved one-value keywords, `configure_file()` of `DownloadProject.CMakeLists.cmake.in`) | **9** |
| … with the upstream keyword list (`QUIET`; `PROJ PREFIX DOWNLOAD_DIR SOURCE_DIR BINARY_DIR BUILD_PROJECT`) | 4: `math-libs/libhipcxx`, and rocprim's three copies |
| … adding `STATUS_MSG` | 3: rocthrust's three copies |
| … without `BUILD_PROJECT` (an older DownloadProject) | 2: `rocm-libraries/projects/hiprand`, `rocm-systems/projects/rccl` |

All keyword lists are held in variables (`set(oneValueArgs …)`) and resolve from one unconditional `set()` in the body.

**Template presence is not measured.** `templates_beside_definition` is 0 only because the sparse checkout holds no `.cmake.in` files. The template was read through the GitHub API at rocm-libraries `98e132f8` (spec Background), and the design does not read it (FR-001).

## Calls

| site | `PROJ` | repository | `GIT_TAG` | tag (heuristic) |
|---|---|---|---|---|
| `rocm-libraries/projects/hipccl/hipccl2/rocprim/cmake/Dependencies.cmake:211` | rocrand | `ROCmSoftwarePlatform/rocRAND` | `develop` | branch-like |
| `rocm-libraries/projects/hipccl/hipccl2/rocthrust/extra/CMakeLists.txt:32` | rocprim | `ROCmSoftwarePlatform/rocPRIM` | `master` | branch-like |
| `rocm-libraries/projects/hipccl/hipccl3/rocprim/cmake/Dependencies.cmake:211` | rocrand | `ROCmSoftwarePlatform/rocRAND` | `develop` | branch-like |
| `rocm-libraries/projects/hipccl/hipccl3/rocthrust/extra/CMakeLists.txt:32` | rocprim | `ROCm/rocm-libraries` | `rocm-7.2.4` | release-like |
| `rocm-libraries/projects/hiprand/cmake/Dependencies.cmake:323` | googletest | `google/googletest` | `release-1.11.0` | release-like |
| `rocm-libraries/projects/rocprim/cmake/Dependencies.cmake:178` | rocrand | `ROCmSoftwarePlatform/rocRAND` | `develop` | branch-like |
| `rocm-libraries/projects/rocthrust/extra/CMakeLists.txt:32` | rocprim | `ROCmSoftwarePlatform/rocPRIM` | `master` | branch-like |
| `rocm-systems/projects/rccl/cmake/Dependencies.cmake:51` | googletest | `google/googletest` | `release-1.12.0` | release-like |

- **8 calls,** all with a literal GitHub `GIT_REPOSITORY` and a literal tag, and none with a URL.
- **All 8 are under an `if()`.** None is in a loop or a function body.
- **5 distinct identities.**
- **5 calls name a branch-like tag.** "Branch-like" means not a commit and not release-shaped, which is a heuristic. FR-009 asks whether to mark these; see research R6 and #1598.

## waybill `main` on the same tree

- C231: `{"components":99,"conditional":61,"emitted":150,"merged":{"existing-component":37,"first-declaration-wins":0},"read":253,"test":9,"unparseable-files":0,"unresolved":{"identity":66},"wrappers":{"ambiguous-calls":0,"calls":67,"definitions":9}}`.
- None of the 8 calls is read.
- **Already components** on `main`, from other declarations:
  - `pkg:github/rocm/rocprim@develop` (a direct FetchContent, so a branch name is already emitted as a version today);
  - `pkg:github/google/googletest@release-1.12.0`;
  - `pkg:github/rocm/rocm-libraries@98e132f8…` (the submodule);
  - several googletest submodules.
- Scan time of the sparse tree: 6.533 s, 6.597 s and 6.566 s.

## Prediction for the branch

*Inference from the counts above and the design; checked by a task at implementation.*
- `read` +8 and `wrappers.calls` +8; `wrappers.definitions` +9.
- No call is unresolved: all 8 bind, and every definition's keywords resolve.
- **Components:** `rocRAND@develop` (three calls, one PURL) and `rocPRIM@master` (two calls, one PURL) are new, which is 2 new PURLs.
- **Possible merges.** The `rocm-libraries@rocm-7.2.4` call is a conditional fetch, inside the tree of the `rocm-libraries` submodule, of that same repository, so spec 1538's R9 would merge it into the submodule. The two googletest calls may merge into an enclosing googletest supplier, or into the identical existing `release-1.12.0` component.
