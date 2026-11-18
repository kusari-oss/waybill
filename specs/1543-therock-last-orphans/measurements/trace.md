# FR-001 trace

Measured on `main` at `26271222` (the binary is `target/debug/waybill`, built from that commit), 2026-11-18. TheRock facts were read through the GitHub API at `rocm-systems` `c1ab6df5` and TheRock `b1cae390`. Nothing was cloned or scanned locally.

## 1. Workspace roots (#1601)

**Readers that synthesize one.** These three call `workspace::synthesize_workspace_root`:
- `pip/uv_lock.rs:869`;
- `npm/bun_lock.rs:1162`;
- `kotlin_dsl/mod.rs:282`.

No other reader emits `waybill:component-role` = `workspace-root`.

**Corpus exposure.** None of the 21 public-corpus goldens holds a component with that role (checked with `jq` over every `cdx.json`). So a change confined to workspace roots cannot move a golden.

**Where the root ends up.** `workspace-roots.sh` builds each reader's fixture in two shapes:
- `atroot`: the workspace is the scan root;
- `sub`: the workspace sits in a subdirectory, under a Cargo main module.

Each shape is scanned twice: `plain`, and `op` (with `--root-name`). Output in `workspace-roots.out`:

| reader | at root, plain | at root, `--root-name` | subdir, plain | subdir, `--root-name` |
|---|---|---|---|---|
| uv | **subject is the member `rocisa`**; root stranded, `foreign-build-system` | stranded, `foreign-build-system` | attached (fallback) | stranded, `foreign-build-system` |
| bun | **subject is the member `a`**; stranded, `foreign-build-system` | stranded, `foreign-build-system` | attached (fallback) | stranded, `foreign-build-system` |
| Kotlin DSL | attached to placeholder `atroot@0.0.0` | attached (fallback) | attached (fallback) | stranded, **no reason** |

7 of 12 shapes strand the root.
- **Kotlin, subdir, `--root-name`** is TheRock's shape: no I7 marker.
- **The uv and bun cells** carry `foreign-build-system`, which is false: a workspace root is not a foreign build system.

**Mechanism.** Every attached cell is attached by the graph-top fallback (`generate/root_edges.rs`). The fallback is open only while no relationship leaves the root. Under `--root-name`, the retained main modules are anchored to the root (`anchor_retained_mainmod_edges`, milestone 860), and that closes the gate. A synthesized workspace root is not a main module, so nothing anchors it, and the fallback no longer reaches it. At the scan root without `--root-name`, `root_selector` considers only main modules, so a member wins.

**The out-of-band path exists.** `root_edges` already has *anchors*: root edges added after the gate is decided, which never change it. They are fed today by:
- top-level git submodules (spec 1530);
- requirements and CMake fetches with no owner (specs 1537, 1538), via `ScanResult.declared_root_anchors` (`scan_fs/mod.rs:1662`).

## 2. The vendored `wkmi` (#1602)

**At `rocm-systems` `c1ab6df5`:**
- **The record.** `shared/amdgpu-windows-interop/wkmi/lnx/lib/cmake/wkmi/wkmiConfigVersion.cmake` sets `PACKAGE_VERSION "1.0.0"`. `installed_lib::location::prefix_of` (`CmakeVersion`) gives the prefix `…/wkmi/lnx`.
- **The library.** `lnx/lib/` holds `libwkmi.a.dvc` (md5 `3c217a97…`, size 333990) and `.gitignore` = `/libwkmi.a`. The library itself is not in git.
- **The include.** `projects/rocr-runtime/libhsakmt/CMakeLists.txt:28` reads `if (WIN_SDK) include(${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists_wsl.txt)`.
- **The link.** `CMakeLists_wsl.txt` has:
  - `target_link_directories(${ROCDXG_TARGET} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../../../shared/amdgpu-windows-interop/wkmi/lnx/lib)`;
  - `target_link_libraries(${ROCDXG_TARGET} PRIVATE wkmi)`.
- **No `find_package(wkmi)`** anywhere.
- **The Windows branch** (the `else()` of `if (UNIX)`) links `win/lib/wkmi.lib` by path. That directory has no package files.

**In the reader (code):**
- **`CMakeLists_wsl.txt` is not read.** `cmake.rs::is_cmake_file` accepts only `CMakeLists.txt` and `*.cmake`. The shared walker's CMake set uses the same patterns (`**/CMakeLists.txt`, `**/*.cmake`), and no code follows `include()`.
- **Conditions are recorded only for fetches.** `cmake/blocks.rs::BlockStack` tracks the enclosing `if`/`else`/`function` blocks per file. `cmake/fetches.rs` turns them into C122 + C232 (`waybill:cmake-fetch-conditions`), whose contract names fetch declarations.
- **No condition crosses an `include()`.** A file read separately starts with an empty stack. This is also true today of a `*.cmake` file included under `if()`. That is a pre-existing gap for fetches, out of scope here.
- **Fetch edges come from an owner.** `scan_fs/cmake_fetch_attach.rs` uses spec 1537's `OwnerIndex`: the nearest enclosing main module or git submodule, else a root anchor. On TheRock the owners are the submodules (`rocm-systems`, `rocm-libraries`, …).
- **Nothing ever attaches the record.** An installed-library record reaches the graph only through `find_package` correlation (specs 1531, 1532). With no `find_package(wkmi)`, nothing does.
