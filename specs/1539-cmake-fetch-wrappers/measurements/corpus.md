# Corpora (T022, SC-006)

## Public corpus: unchanged

| run | waybill | result |
|---|---|---|
| 44114156544 | branch `1b6d7723` | compare mode, **158 passed, 0 failed**, in 17.5 minutes. The goldens are byte-identical. |
| 44114162431 | `main` `b8ba7e4d` (control, dispatched at the same time) | 158 passed, 0 failed, in 23 minutes |
| 44105380279 | branch `bbbe0b34` | cancelled by the 45-minute job timeout. 157 targets and the 3 other tests passed; `corpus_nix_closure_slacklinker` had not finished. |

**The cancelled run.** `corpus_nix_closure_slacklinker` is a Nix closure target, and it was still running when the job hit its time limit. It hit the same limit during spec 1538 (run 44011633657).

The rerun at the branch and the control at `main`, dispatched together, both passed with it. The code under test did not change between `bbbe0b34` and `1b6d7723`, which added only tests and documents. I do not know what made the first run slow, and this record does not attribute it.

No public-corpus target changes. No golden was regenerated.

## Quality corpus: one target moves, traced

| run | waybill | violations |
|---|---|---|
| 44105386117 | branch `bbbe0b34` | none |
| 44107875421 | `main` `b8ba7e4d` (control) | none |

Of 18 targets, only **python-pytorch** changes: packages 158 → 160, edges 108 → 110, and sbomqs CycloneDX 5.728 → 5.724.

**Traced locally.** This used a sparse, blob-less checkout of pytorch's `*.cmake`, `CMakeLists.txt` and `.gitmodules` at `2b3ec34829036a65cd9d1398ea72a0167dc37470` (v2.14.0, the corpus pin), scanned with the branch binary and censused with `census/wrappers.py`.
- **The wrappers.** The reader recognises three, all in `cmake/External/aotriton.cmake`: `aotriton_build_from_source`, `aotriton_download_runtime` and `aotriton_download_image`. `aotriton_build_windows_dependencies` names literal GitHub repositories, so it is not a wrapper (R2), and spec 1538's C-13b components from it (dlfcn-win32, xz) are unchanged.
- **The two new components**, both `cmake-conditional`, versionless, with C151:
  - `pkg:generic/aotriton_external`, from `aotriton_build_from_source(OFF aotriton_external)`. The body's `GIT_TAG ${__AOTRITON_CI_COMMIT}` is set twice in the file, an environment override and a literal, so it is unresolved.
  - `pkg:generic/aotriton_runtime`, from `aotriton_build_from_source(ON aotriton_runtime)` and `aotriton_download_runtime(… aotriton_runtime)`. It carries two chains in C234, and its URL is built with `string(CONCAT …)`.
- **The two new edges** are their owner edges.
- **The one call left unresolved**, `aotriton_download_image(${image} …)`, is inside a loop.

A direct call with the same bound arguments gives the same components (FR-003): spec 1538's rule for unresolved variables (#1517). That the first is named after a CMake target although its repository is literal GitHub is filed as #1595. The 0.004 sbomqs drop is attributed to the two versionless components: an *inference* from the score's inputs, not a measurement.
