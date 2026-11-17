# TheRock at the branch (T012: SC-002 to SC-004, SC-008, I-1)

**Run:** `census-fetches` 44149681033 in `kusari-sandbox/waybill-fixture-therock-sdk`.
- **Refs:** `waybill_ref` = branch at `cc85a5e9`; `base_ref` = `main` at `f0133adc` (spec 1539 landed).
- **Tree:** TheRock at `b1cae390`, each top-level submodule at its recorded commit. The checkout is sparse (`*.cmake`, `CMakeLists.txt`, `.gitmodules`), and TheRock was not cloned locally.
- **Scans:** both refs, the same tree, 3 times each, interleaved.

## C231

| | `main` | branch | prediction (`census.md`, inference) |
|---|---|---|---|
| `read` | 253 | **261** | +8 |
| `emitted` | 150 | **156** | |
| `components` | 99 | **101** | 2 new PURLs |
| `merged.existing-component` | 37 | **39** | "possible merges" |
| `unresolved.identity` | 66 | **66** | no new unresolved |
| `conditional` / `test` | 61 / 9 | 63 / 9 | |
| `wrappers` | `{"ambiguous-calls":0,"calls":67,"definitions":9}` | `{"ambiguous-calls":0,"calls":75,"definitions":18}` | calls +8, definitions +9 |

V1 holds: 261 = 156 + 0 + 39 + 66. V2 holds: 63 + 9 ≤ 101. Every count is as predicted.

## SC-002: each of the 8 calls

| call site | identity | became |
|---|---|---|
| `rocm-libraries/projects/rocprim/cmake/Dependencies.cmake` | `rocRAND@develop` | **new** `pkg:github/rocmsoftwareplatform/rocrand@develop` |
| `…/hipccl/hipccl2/rocprim/cmake/Dependencies.cmake` | `rocRAND@develop` | the same component |
| `…/hipccl/hipccl3/rocprim/cmake/Dependencies.cmake` | `rocRAND@develop` | the same component |
| `rocm-libraries/projects/rocthrust/extra/CMakeLists.txt` | `rocPRIM@master` | **new** `pkg:github/rocmsoftwareplatform/rocprim@master` |
| `…/hipccl/hipccl2/rocthrust/extra/CMakeLists.txt` | `rocPRIM@master` | the same component |
| `…/hipccl/hipccl3/rocthrust/extra/CMakeLists.txt` | `rocm-libraries@rocm-7.2.4` | **merged** (R9) into the `rocm-libraries` submodule `pkg:github/rocm/rocm-libraries@98e132f8…`, recorded in its C233 |
| `rocm-libraries/projects/hiprand/cmake/Dependencies.cmake` | `googletest@release-1.11.0` | **merged** (R9) into rocm-libraries' googletest submodule `pkg:github/google/googletest@e2239ee6…`, recorded in its C233 |
| `rocm-systems/projects/rccl/cmake/Dependencies.cmake` | `googletest@release-1.12.0` | the existing `pkg:github/google/googletest@release-1.12.0`, one component per PURL, which now carries the chain |

So **6 emitted** (as 2 new components and 1 existing one) and **2 merged**: all 8 are accounted for.

**The new components.**
- `rocrand@develop`: `cmake-find-package-fallback`, because the calls are under `if(WITH_ROCRAND AND NOT rocrand_FOUND)`. C18 lists the three call sites.
- `rocprim@master`: `cmake-conditional`. C18 lists two call sites.

Both are owned by the `rocm-libraries` submodule, not the root.

**Branch names.** Both new components are versioned by a branch (`develop`, `master`), as a direct call is today; filed as #1598.

## SC-003: nothing false

No component's PURL contains `${`, `%24`, `DL_ARGS` or `-download`.

## SC-008 and I-1

- **C234:** a chain naming `download_project` is on 3 components in CycloneDX, 3 in SPDX 2.3 and 3 in SPDX 3: the two new ones, and `googletest@release-1.12.0`.
- **I-1:** the root's direct targets agree in the three formats, with 118 each at both refs (`specs/1537-requirements-root-attach/measurements/root_targets.py`). The new components hang off the submodule.

## Scan time of the sparse tree (same job, interleaved)

| | run 1 | run 2 | run 3 | median |
|---|---|---|---|---|
| `main` | 6.744 s | 6.723 s | 6.711 s | 6.723 s |
| branch | 6.755 s | 6.751 s | 6.744 s | 6.751 s |

## SC-005: full tree with submodules (T013)

`measure-source` 44149685219 in the fixture repository scanned the full 17 GB tree with all submodules at `main` (`f0133adc`) and at the branch (`cc85a5e9`). Each side had one untimed warm-up, then three alternating timed rounds.

| | warm-up | run 1 | run 2 | run 3 | median |
|---|---|---|---|---|---|
| `main` | 140.8 s | 112.9 s | 111.2 s | 109.6 s | 111.2 s |
| branch | 112.4 s | 113.1 s | 109.4 s | 107.8 s | 109.4 s |

**Ratio of medians: 0.984**, within SC-005's 1.5× and within each side's own spread.

The same documents:
- **Components:** 4,346 → 4,348, and unique PURLs 2,670 → 2,672: the two new components.
- **C231** is identical to the sparse census above, at both refs.
