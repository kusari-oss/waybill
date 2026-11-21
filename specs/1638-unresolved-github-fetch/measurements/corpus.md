# T023: public corpus and quality corpus (SC-002, SC-004)

## Public corpus (SC-004)

**Run.** `public-corpus.yml` run 44860085872, `branch=1638-unresolved-github-fetch`, `regen_goldens=true`. The run summary says `Scanned commit: c7b1bfe8…`. Every job's conclusion:
- `Run public corpus (1638-unresolved-github-fetch)`: `success`;
- `Report corpus failure`: `skipped`.

**Result.**
- The run summary says "None — every regenerated golden is byte-identical to the committed one."
- Checked independently: the `corpus-goldens-regen` artifact holds 67 files across 21 targets, the same file set as `waybill-cli/tests/fixtures/public_corpus/` (66 JSON plus `pants-example-javascript/README.md`). `cmp` of every file found **0 differing**.
- No golden is committed. As T001 predicted, no public-corpus target was expected to change, and none did.

## Quality corpus (SC-002)

**Runs.** `quality-corpus.yml` on the branch (run 44860087802, artifact `quality-corpus-c7b1bfe8…`) and on `main` (run 44860089990, artifact `quality-corpus-7f1a568a…`). Every job's conclusion: `Measure SBOM quality corpus` `success`, `Report quality-corpus failure` `skipped`, on both runs.

**Metrics.** For all 18 targets, `status`, `pkgs`, `files`, `edges`, `nodes_with_out_edges`, `max_depth`, `graph_completeness` and the sbomqs CycloneDX score are identical (`diff` of the per-target rows is empty); `violations` is `[]`. Renaming changes no count, so this is expected, and it does not show which components changed.

**Component by component.** The quality-corpus run uploads no SBOMs. The three targets were therefore traced with `census-unresolved`, which scans the same pinned trees the same way and uploads the CycloneDX output:
- run 44858994115 at `main` `7f1a568a` (T001);
- run 44862201376 at the branch `c7b1bfe8`.

Every job of both runs succeeded, and each job's `refs.txt` names the scanned waybill SHA.

| Target | Only on `main` | Only on the branch | Rest |
|---|---|---|---|
| cmake-nlohmann-json | none | none | document identical with serial, timestamp and tool masked |
| cpp-mongo | none | none | document identical with serial, timestamp and tool masked |
| python-pytorch | `pkg:generic/aotriton_external`, `pkg:generic/nccl` | `pkg:github/rocm/aotriton`, `pkg:github/nvidia/nccl` | unchanged |

**python-pytorch, traced against `cmake/External/aotriton.cmake` at `2b3ec348`** (fetched with `gh api`):
- **`pkg:github/rocm/aotriton`** comes from both calls of `aotriton_build_from_source`, which wraps `ExternalProject_Add(${project} GIT_REPOSITORY https://github.com/ROCm/aotriton.git … GIT_TAG ${__AOTRITON_CI_COMMIT})`:
  - line 286, `aotriton_build_from_source(OFF aotriton_external)`;
  - line 296, `aotriton_build_from_source(ON aotriton_runtime)`.

  `__AOTRITON_CI_COMMIT` is set at lines 31 and 33 (an environment override, else a literal), so it stays unresolved. The component has no version, C151, the design tier, C55 `cmake-externalproject`, C234 `[["aotriton_build_from_source"]]`, `waybill:download-url` `https://github.com/ROCm/aotriton.git`, and Optional with C122 `cmake-conditional`. Its name is `aotriton_external`. **SC-002's first half holds:** `aotriton_external` is `pkg:github/rocm/aotriton` with C151 and no version (FR-001, FR-002).
- **`pkg:generic/aotriton_runtime` keeps its PURL, but no longer stands for line 296.**
  - On `main`, line 296's from-source build and line 298's `aotriton_download_runtime(… aotriton_runtime)` shared that PURL because they share a target name, so its C234 was `[["aotriton_build_from_source"],["aotriton_download_runtime"]]`.
  - On the branch, line 296 fetches `ROCm/aotriton` and is on `pkg:github/rocm/aotriton` (FR-001, then R4). `aotriton_runtime` stands for the download only, with C234 `[["aotriton_download_runtime"]]`; its scope, tier, C151, C55 and C18 are unchanged.

  This is SC-002's "unchanged" in its PURL and rows, with one chain fewer. The chain left because the declaration it belonged to now names its real source.
- **`pkg:github/nvidia/nccl`** comes from `cmake/PreBuildSteps.cmake:139`, `FetchContent_Declare(nccl GIT_REPOSITORY https://github.com/NVIDIA/nccl GIT_TAG ${_nccl_tag})` (T001). Its name, scope (Optional, `cmake-conditional`), tier, C151 and C18 (`cmake/External/nccl.cmake`, `cmake/PreBuildSteps.cmake`) are unchanged, and it gains `waybill:download-url`. The census found it; #1595 did not name it (FR-001).

**SC-002:** every change on the three targets is explained by FR-001 and R4.
