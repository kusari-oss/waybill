# T032: TheRock source tree

**Final measurement:** `kusari-sandbox/waybill-fixture-therock-sdk` measure-source, two runs at branch `99e6e2f5` against `main` `cd2af654` (spec 1613 merged). TheRock `b1cae390` with submodules, CI only:
- 44477517009;
- 44477532788.

`5a6fb4c1` (the PR head) adds only quality-corpus bounds; no code changed.

Two earlier rounds found the defects recorded in `trace.md` and research R1, R7.1 and R12:
- **Round 1** (`93fc2cb0`, runs 44463579565 and 44463591426): scan time 1.114× and 1.082×; a library Python search dropped as a tool.
- **Round 2** (`4d2038d8`, runs 44471788355 and 44471802132): search evidence moved fetch declaration sites; module-name includes were not followed.

## Scan time (SC-009)

| Round | Ratio to `main` (median of 3 interleaved, after a warm-up) |
|---|---|
| 1, `93fc2cb0` | 1.114, 1.082: **over budget** |
| 2, `4d2038d8` | 1.008, 1.010 |
| 3, `99e6e2f5` | **1.014, 1.013** |

Budget ≤ 1.05× (plan R12).

## Components (SC-001 to SC-004)

| | `main` | branch |
|---|---|---|
| non-file components | 2,299 | 2,391 |
| edges | 3,542 | 3,692 |
| package-search components | 4 | 97 |
| unreachable, no orphan reason | 0 | 0 |
| duplicate `bom-ref`s | 0 | 0 |

**C237:**
```json
{"components":97,"conditional":57,"in-tree":81 names,"in-tree-declarations":1091,
 "paired":{"fetch":200,"installed":0,"submodule":24},"read":2090,"test":13,"tools":429,"unresolved":32}
```
`read` = 32 + 429 + 200 + 0 + 24 + 1,091 + 314, the searches the 97 emitted components stand for.

**SC-001.** 97 package-search components, against a census prediction of 110 (research R13). Every difference was traced, name by name, to the prediction's method, not to the reader:
- **pkg-config prefixes.** The census took `pkg_check_modules(<PREFIX> …)`'s first argument as the name (`_ffmpeg_avcodec`, `DW`, `ELF`, `DRM`, `pc_libxml`, …). waybill emits the module names (`libavcodec`, `libdw`, `libdrm`, `libxml-2.0`, …).
- **Nested git submodules.** The prediction keyed submodules on `.gitmodules` paths. `dyninst`, `elfio`, `ffts`, `glog`, `gotcha`, `kokkos` and `pybind11` are nested submodules, and pair with them (R7.3); `yaml-cpp` pairs with its fetch.
- `spirv_tools` is emitted under its declared spelling, `spirv-tools`.

**SC-002.**
- **57 conditional and 13 test components.**
- **The six fetch components the spec names are now Optional:** hotswap_intercept, hotswap_rocr, json-c, libvfio-user, otf2-source, and both nanobind pins (`json-c` and `libvfio-user` after round 2's module-path fix).
- **13 components shared with `main` moved from required to Optional.**

**SC-003.** Every searched name maps to one identity: 200 search declarations pair with fetches and 24 with git submodules, and there are no duplicate `bom-ref`s.

Gone, and why:
- **`pkg:generic/libiberty`:** the tree has a `project(libiberty)`, so FR-007 lists it as in-tree.
- **`googletest@f8d7d77`:** composablekernel adds its tests only under `if(BUILD_TESTING)`, so its `cmake/gtest.cmake` fetch is now conditional. Spec 1538's R9 records a conditional fetch of a repository an enclosing project already supplies on that supplier: rocm-libraries' googletest submodule (`e2239ee`), with `gtest.cmake` in its C233 and C18. On `main`, without conditions, it stayed separate.

**SC-004.** 0 unreachable components without an orphan reason, on both sides.

## Formats and repeats (SC-005, SC-006)

- **`waybill sbom parity-check`:** the same three count-row gaps as `main` (A12, B1, B2); C42, C62, C232, C236 and C237 agree.
- **Two runners at one commit:** CycloneDX, SPDX 2.3 and SPDX 3 identical once serial, timestamps and document namespace are removed.
