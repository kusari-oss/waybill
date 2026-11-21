# T001 census: fetches that take the unresolved branch

**Run.** `kusari-sandbox/waybill-fixture-therock-sdk` `census-unresolved` run 44858994115. Fixture commit `7bae975`, script `census/unresolved_fetches.py`, all 4 jobs `success`. Scanned with waybill `7f1a568a` (`main`); the SHA was read from each job's `refs.txt`.

**Trees, as read from the logs:**
- **TheRock** `b1cae390`, with its top-level submodules (measure-source's depth).
- **The quality-corpus targets**, at their pins from `xtask/corpus/quality-corpus.toml`, without submodules:
  - `cmake-nlohmann-json`: the pin `65ee6845` is the annotated tag `v3.12.0` (checked with `gh api …/git/ref/tags/v3.12.0`); checked out at commit `55f93686`;
  - `python-pytorch` `2b3ec348`;
  - `cpp-mongo`: the pin `41a57524` is the annotated tag `r8.3.8`; checked out at commit `d100bf19`.

The census is a measuring script, not a model of waybill's reader. Run before any source change.

## The split (SC-001)

| Tree | Declarations on the unresolved branch | literal GitHub repo | literal other repo | unresolved repo | URL | unexpanded name | wrapper templates (not declarations) |
|---|---|---|---|---|---|---|---|
| TheRock | 36 | **15** | 0 | 11 | 10 | 0 | 0 |
| python-pytorch | 1 | **1** | 0 | 0 | 0 | 0 | 3 (1 GitHub, 2 URL) |
| cmake-nlohmann-json | 2 | 0 | 0 | 2 | 0 | 0 | 0 |
| cpp-mongo | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

**Literal-GitHub cases**, with file and line in the run's artifacts:
- **TheRock (15):**
  - `rocm-cmake` ×6 (`get-rocm-cmake.cmake` in hipblas, hipsolver, rocblas, rocsolver; rocm-docs; rccl-tests);
  - `spdlog` ×2, `googletest` ×2;
  - `flatbuffers`, `spirv-headers`, `googlebenchmark`, `fmt`, `nlohmann_json`.
- **pytorch (1 declaration, 1 template):**
  - `cmake/PreBuildSteps.cmake:139` `nccl` (`https://github.com/NVIDIA/nccl`, `${_nccl_tag}`), a direct declaration;
  - the template `cmake/External/aotriton.cmake:174` (`https://github.com/ROCm/aotriton.git`, `${__AOTRITON_CI_COMMIT}`), the body of the wrapper whose call is `aotriton_external`.

**No tree has a literal non-GitHub repository with an unresolved tag**, so FR-004 changes nothing on these trees. Its effect is shown only by U-7 and U-8.

**No tree has an FR-008 case** (an unexpanded name with a literal GitHub repository) outside a wrapper template.

## Reconciliation against `main` (research R7)

Each declaration was classified against the same run's `main` scan:

| Tree | found | accounted | unexplained | components with the CMake C151 reason |
|---|---|---|---|---|
| TheRock | 36 | 0 | **0** | 17 |
| python-pytorch | 1 | 0 | **0** | 3 |
| cmake-nlohmann-json | 2 | 0 | **0** | 1 |
| cpp-mongo | 0 | 0 | **0** | 0 |

**Every counted declaration is in a C151 component's C18 under its own name.** Declarations merge (R4), which is why 36 declarations are 17 components on TheRock.

**The inverse direction is reported, not reconciled.** It lists (component name, C18 file) keys of C151 components that no counted declaration matches:
- **pytorch, 3 keys.** `aotriton_external` and `aotriton_runtime` are wrapper call sites (C234), which the census does not count as declarations. `nccl` matches on `cmake/External/nccl.cmake`, which is in its C18 but is not a counted declaration.
- **TheRock, 15 keys:** `lapack` ×7, `nlohmann_json` ×4, `rocm-cmake` ×1, `spdlog` ×3. None of those components carries C234. The keys are other C18 files of those components; for example, `pkg:generic/rocm-cmake` also lists `base/rocm-cmake/CMakeLists.txt`.

  This is consistent with spec 1614's search evidence (R7.1), which adds a package search's file to a fetch's C18. It was not traced key by key. The script's label "expected: wrapper call sites" is wrong for TheRock: these are C18 files that are not declarations.

## What this predicts (checked in T022 and T023)

- **TheRock (SC-003).** Some of the 15 literal-GitHub declarations' components change identity. Seven distinct repositories are involved, with rocm-cmake, spdlog and googletest declared more than once. The exact count depends on R4 and R9 merges and is measured, not derived.
- **python-pytorch (SC-002):**
  - `pkg:generic/aotriton_external` → `pkg:github/rocm/aotriton`;
  - `pkg:generic/nccl` → `pkg:github/nvidia/nccl`, which the census found and #1595 did not name;
  - `aotriton_runtime` unchanged (a URL template).
- **nlohmann-json, mongo, and the public corpus (SC-004):** nlohmann-json's two unresolved fetches have unresolved repositories, and mongo has none, so both should be byte-identical.
