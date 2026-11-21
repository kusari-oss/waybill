# T022: TheRock measure-source (SC-003, SC-006)

**Run.** `kusari-sandbox/waybill-fixture-therock-sdk` `measure-source` run 44859951815, dispatched with `--ref main -f waybill_ref=c7b1bfe8… -f base_ref=7f1a568a…`. Every job's conclusion: `measure` `success` (the only job). The scanned SHAs were read from `refs.txt`: base `7f1a568a`, branch `c7b1bfe8`, TheRock `b1cae390`. Tracing below uses the `sub` scans (with submodules).

## Changed components: every one traced (SC-003)

Both sides have 2,948 components. Seven PURLs leave and seven arrive:

| `main` | branch | Explained by |
|---|---|---|
| `pkg:generic/flatbuffers` | `pkg:github/google/flatbuffers` | FR-001 |
| `pkg:generic/fmt` | `pkg:github/fmtlib/fmt` | FR-001 |
| `pkg:generic/googlebenchmark` | `pkg:github/google/benchmark` | FR-001 |
| `pkg:generic/nlohmann_json` | `pkg:github/nlohmann/json` | FR-001 |
| `pkg:generic/spdlog` | `pkg:github/gabime/spdlog` | FR-001 |
| `pkg:generic/spirv-headers` | `pkg:github/khronosgroup/spirv-headers` | FR-001 |
| `pkg:generic/rocm-cmake` (design, Optional, 7 files) | merged into `pkg:github/rocm/rocm-cmake@10155d72…` (git submodule; C18 27 → 33 files) | FR-007, FR-013 |
| `pkg:generic/googletest` (design, 8 files), still present with 6 files | `pkg:github/google/googletest` (design, Optional, 2 files) | FR-001, then R4 |

**The six renames.** Each keeps scope (`excluded`), tier (`design`), its C151 reason and its C18 files exactly; only the PURL changes. nlohmann_json and spdlog list 5 files each on both sides. The extra files are search evidence (spec 1614 R7.1), keyed by declared name, so they follow the fetch's new PURL.

**rocm-cmake.** The six conditional `rocm-cmake` fallbacks the census found have a GitHub identity now, so R9 merges them into the submodule that supplies the repository, as it already did for resolved fallbacks of the same repository. Their files are the submodule's six new C233 entries with `"version": ""`:
- `rocm-docs/cmake/Modules/Dependencies.cmake`;
- `get-rocm-cmake.cmake` in hipblas, hipsolver, rocblas and rocsolver;
- `rocm-systems/projects/rccl-tests/cmake/Dependencies.cmake`.

**googletest.** On `main`, the two literal-GitHub googletest declarations shared `pkg:generic/googletest` with six other unresolved googletest declarations (URL or unresolved repository). Under FR-001 the two are `pkg:github/google/googletest` and the six stay generic. They do not merge into a googletest submodule, because no submodule of that repository is in an ancestor directory of theirs (R9's rule).

**Accounting.** Seven PURLs out, seven in, and rocm-cmake −1 with googletest +1: 2,948 = 2,948.

## Orphans (SC-003)

| Side | Components with no incoming edge | Of which with no `waybill:orphan-reason` |
|---|---|---|
| `main` | 406 | 4 |
| branch | 406 | 4 |

The four without a reason are the same on both sides (`diff` of sorted PURLs empty): `pkg:generic/TheRock@10.3.0`, `pkg:pypi/nanobind@2.15.0`, `pkg:pypi/rocm-bootstrap@unresolved`, `pkg:pypi/swig@4.5.0`. **Unexplained orphans do not increase.**

**Document scope.** `waybill:graph-completeness-reason`'s `transitive-edges-unresolvable` list goes from `generic, pypi` to `generic, github, pypi`. `classify_transitive_edges_unresolvable` lists the ecosystem of any design-tier component with no source-tier component of the same versionless identity. `pkg:github/fmtlib/fmt` and the others have none, so `github` is listed by the existing rule, a consequence of FR-001. The report's PyPI columns are identical on both sides.

## Scan time (SC-006)

Three timed runs per side after a warm-up, with submodules:

| Side | run 0 | run 1 | run 2 | median |
|---|---|---|---|---|
| `main` | 35.245 s | 34.479 s | 33.995 s | 34.479 s |
| branch | 35.048 s | 35.115 s | 34.878 s | 35.048 s |

**Ratio of medians: 1.016**, within SC-006's 1.05. The runs are sequential per side, not interleaved; `main`'s spread (33.995–35.245 s) contains the branch's median.
