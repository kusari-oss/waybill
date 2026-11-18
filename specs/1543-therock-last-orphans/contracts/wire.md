# Wire contract

Every case is asserted in CycloneDX, SPDX 2.3 and SPDX 3, with:
- **I7:** 0 unexplained components;
- **agreement:** C45, C104 and C105 agree across the three formats;
- **repeatability:** 20 scans are identical after masking.

Fixtures use `waybill-fixture-*` names.

## Workspace roots (#1601)

| case | shape | expected |
|---|---|---|
| W-1 | uv workspace in `proj/` under a Cargo main module, `--root-name` | root → `pkg:generic/<ws>`; `<ws>` → both members; members keep their root edges; no `foreign-build-system` |
| W-2 | W-1 with bun | same |
| W-3 | W-1 with Kotlin DSL | same (today: stranded, no reason) |
| W-4 | uv workspace at the scan root, no flags | subject = `pkg:generic/<ws>`; C69 heuristic `workspace-root`; members reached through it |
| W-5 | W-4 with bun | subject = `pkg:generic/<ws>` |
| W-6 | Kotlin at the scan root, no flags | subject = `pkg:generic/<rootProject.name>@0.0.0` (today: placeholder `<dir>@0.0.0`); no C69, which milestone 127 emits only when a main module lost, and Kotlin emits none |
| W-7 | uv and bun workspaces both at the scan root | neither is promoted; both are anchored to the subject the ladder picks |
| W-8 | workspace root already targeted by a relationship | no anchor; output equal to `main` |

## Links (#1602)

| case | shape | expected |
|---|---|---|
| L-1 | `vendor/pkg/lnx/lib/cmake/pkg/pkgConfigVersion.cmake` (1.0.0) plus `lib/libpkg.a`; `if(FLAG)` around `target_link_directories(t PRIVATE …/vendor/pkg/lnx/lib)` and `target_link_libraries(t PRIVATE pkg)` | owner → `pkg:generic/pkg@1.0.0`; Optional; C122 `cmake-conditional-link`; C235 `["FLAG"]` |
| L-2 | L-1 without the `if` | edge; scope unchanged; no C235 |
| L-3 | L-1, library absent | as L-1 |
| L-4 | L-1, only `libpkg.a.dvc`, and the record named differently from the link name | as L-1 (matched by file) |
| L-5 | link directory outside every record's prefix | no edge; output equal to `main` |
| L-6 | name matching neither the record nor a file in the directory | no edge |
| L-7 | the link in `CMakeLists_wsl.txt`, `include()`d from `CMakeLists.txt` inside `if (WIN_SDK)`, the link itself unconditional | C235 `["WIN_SDK"]` |
| L-8 | the linking file inside a git submodule directory / no owner at all | edge from the submodule / root anchor |
| L-9 | one record linked under `if(A)` and under `if(B)`; then a third, unconditional, link | `["A","B"]`, Optional; then required, no C235 |
| L-10 | `include(FetchContent)`; an `include()` cycle | ignored; terminates |
| L-11 | `link_directories(…)` then `target_link_libraries(other PRIVATE pkg)` | edge (directory scope applies to every target) |
| L-12 | a record at `vendor/a` and another at `vendor/a/sub`; link directory under `vendor/a/sub/lib` | edge to the inner record only |

## TheRock (CI)

| check | expected |
|---|---|
| unexplained unreachable components | 0 (was 2) |
| `pkg:generic/tensile` | reachable from `TheRock@10.3.0` |
| `pkg:generic/wkmi@1.0.0` | reachable from the `rocm-systems` submodule; Optional; C235 `["WIN_SDK"]`; all three formats |
| repeated scans | identical after masking |
| median scan time | ≤ 1.1× `main` |
