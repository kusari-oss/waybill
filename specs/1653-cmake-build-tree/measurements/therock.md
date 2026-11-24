# SC-001(b): a partial TheRock build tree *(measured, 2026-11-24)*

**Run:** `census-buildtree` run 45418947072 in `kusari-sandbox/waybill-fixture-therock-sdk`,
branch `census-buildtree`, workflow `.github/workflows/census-buildtree.yml`.
- **Source:** TheRock `b1cae390cc745283dce25581415353f9bc906162`, from the fork
  `kusari-sandbox/TheRock`, with top-level submodules.
- **Runner:** `ubuntu-24.04`, CMake 3.31.6, Ninja.
- **Configure:** `-DTHEROCK_ENABLE_ALL=OFF -DTHEROCK_ENABLE_CORE=OFF
  -DTHEROCK_AMDGPU_FAMILIES=gfx1150 -DCMAKE_EXPORT_COMPILE_COMMANDS=ON`,
  exit 0.
- **Build:** `ninja -k 0 therock-fmt therock-nlohmann-json therock-yaml-cpp
  therock-googletest`, exit 0, 16 s. These four are third-party subprojects
  that need only the host compiler.
- **Not built:** any first-party ROCm subproject. Those need the
  `amd-llvm` compiler, which is hours of build. The question of whether
  a first-party subproject's FetchContent calls show R1's signals is
  therefore **not measured here**.

## What the tree holds

| | Count |
|---|---|
| Listing entries | 2,225 (39.0 MB of files) |
| `CMakeCache.txt` | 5: the super-build `.` and `third-party/{fmt,googletest,nlohmann-json,yaml-cpp}/build` |
| ExternalProject step stamps (`*-download`, `*-mkdir`) | 8: two per third-party fetch |
| `_deps/*-src` populations | 0: none of the four uses FetchContent |
| `compile_commands*.json` | 9, of which 2 are **empty (0 bytes)**; see below |
| Metadata kept | 216 KB |

## Findings against research

- **R3/R7 confirmed: stamps are moved by `PREFIX`.** `therock_subproject_fetch(therock-fmt-sources …)`
  stamps at `third-party/fmt/therock-fmt-sources_fetch/src/therock-fmt-sources-stamp/therock-fmt-sources-download`.
  The stamp's name is the declared name in `third-party/fmt/CMakeLists.txt`
  (`therock_subproject_fetch(therock-fmt-sources …)`), so matching by file name
  (contracts/settlement.md) settles all four as *taken*. The default
  `<name>-prefix/` would have found none.
- **Matching rule 1 (sub-builds) confirmed, and it matters.** Each third-party
  subproject's cache has `CMAKE_HOME_DIRECTORY = <build>/third-party/<x>/source`.
  That source is the tarball TheRock downloaded **into the build tree**, so
  these are sub-builds under the rule. They own no declarations, because their
  source is not in the scan. Their stamps count for the super-build
  directory, whose home is the TheRock checkout and so owns the
  `therock_subproject_fetch` calls. Without the rule, four build directories
  would report `source-mismatch`.
- **The super-build's cache carries the feature switches:**
  `THEROCK_ENABLE_ALL:BOOL=OFF`, `THEROCK_ENABLE_CORE:BOOL=OFF`,
  `BUILD_TESTING:BOOL=ON`. These are the configured values User Story 2 needs
  for TheRock's `if(THEROCK_ENABLE_…)` gates.
- **Empty compile databases are real (new).**
  `third-party/nlohmann-json/build/compile_commands.json` and its fragment are
  0 bytes: TheRock touches one when CMake writes none
  (`cmake/therock_subproject.cmake:1072`). This is a header-only project.
  **Decision (US3):** a 0-byte compile database is *absent*, not
  *unreadable*. It does not count in `unreadable-files`.
- **R1 (FetchContent) is neither confirmed nor contradicted** on TheRock:
  nothing in the subset populates `_deps/`.

**Gate (T001):** nothing contradicts R1–R3 or R7. Reader work proceeds. A
first-party subset would need the compiler build, which is out of reach on a
hosted runner within this task. The SC-005 before/after census reuses this
workflow, adding a scan step.
