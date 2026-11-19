# T033–T034: corpora

## Public corpus (SC-007)

| Run | Ref | Result |
|---|---|---|
| 44463551125 | branch `93fc2cb0` | `regen_goldens=true`: every regenerated golden byte-identical to the committed one ("None — every regenerated golden is byte-identical") |
| 44477496645 | branch `99e6e2f5` | `repeat=2`: 21 `corpus_*` target tests ok |

**No golden changed.** None of the 21 targets has a CMake package search, so C237 never appears in them.

The repository's own CMake ecosystem goldens (`waybill-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/cmake.*`) gained C237: 4, 6 and 8 added lines, nothing else changed (`gate-log.md`).

## Quality corpus (SC-008)

Branch `99e6e2f5` (run 44477506378), with the `main` control at `cd2af654` (run 44463570632). 15 of 18 targets are identical. The three that differ were traced component by component with the harness's exact invocation, `main`'s build against the branch's:

| Target | pkgs | edges | Why |
|---|---|---|---|
| cmake-nlohmann-json | 21 → 19 | 22 → 20 | `find_package(Git)` and `find_package(Python3 COMPONENTS Interpreter)` (`cmake/ci.cmake:9`): tools, FR-006 |
| cpp-mongo | 411 → 418 | 479 → 486 | wiredtiger's `cmake/third_party/*.cmake`, too deep for the old narrow set: five cloud SDKs (Optional under `IMPORT_*_PACKAGE`), `SQLite3` (Optional under `USE_SYSTEM_SQLITE3`), `Python3 COMPONENTS Interpreter Development` (a library, required). Owned by `mdb-python-deps`, the root Python project (spec 1537's rule). |
| python-pytorch | 261 → 257 | 211 → 256 | below |

**python-pytorch:**
- **−7 components:**
  - `git`, `pkgconfig`, `threads`: tools;
  - `gloo`, `protobuf`, `pybind11`: paired with their git submodules;
  - `sycl-dpcpp-`: a module name the old regex truncated at `${` (`pkg_check_modules(… sycl-dpcpp-${SYCL_COMPILER_VERSION_MAJOR})`), now unresolved.
- **+3 components:** `acl`, `backtrace`, `python3`.
- **+45 edges, none removed:** 44 from `torch` and 1 from `torch-openreg`, to search components that had no parent on `main` and to the 3 paired submodules.
- **37 components required on `main` are Optional** under their real conditions (`cuda` under `USE_CUDA`, `mpi` under `USE_MPI`, …).
- **`python`** is now a library search (`Interpreter OPTIONAL_COMPONENTS ${PYTHON_COMPONENTS}`, which is `Development.Module`), Optional under `BUILD_PYTHON`. Round 1's tool misclassification of this search is fixed.

pytorch's `edges` bound is re-centred at ±10% of 256 (230–282). The re-run at `5a6fb4c1` (run 44480610178) passes, with pytorch at 257 components and 256 edges.
