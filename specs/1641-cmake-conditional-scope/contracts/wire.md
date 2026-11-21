# Wire contract: CMake conditional scope

## Fields

- **Native scope** (CycloneDX `scope`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`, SPDX 3 via C122): as data-model's scope outcome.
- **C122 `waybill:optional-derivation`:** `cmake-conditional` only when the chain does not hold. Absent on a kept-in-scope fetch.
- **C232 `waybill:cmake-fetch-conditions`:** present on every fetch with an enclosing condition, in scope or not. Strings unchanged from `main`.
- **C236 `waybill:cmake-find-conditions`:** unchanged.
- **C231 `waybill:cmake-fetch-coverage`:** `conditional` still counts Optional components. It falls by the fetches brought into scope.

## Documentation (FR-010)

- **C122 row:** `cmake-conditional` applies only when the enclosing conditions do not hold in the default configuration (spec 1641). A platform check, the super-build guard, a default-ON option or an unreadable condition leaves the fetch in scope.
- **C232 row:** also present on an in-scope fetch, recording what gates it.
- **Consumer guide:** one paragraph.

## Integration cases (`waybill-cli/tests/cmake_conditional_scope.rs`)

Each case scans in three formats and runs parity (scope, C122, C232, C236). It validates CycloneDX, checks unique `bom-ref`s, and repeats 10 scans. "Changed" marks a case that gives Optional on `main` (the control).

| Case | Fixture | Expected | Changed |
|---|---|---|---|
| T-1 | fetch under `if(CMAKE_SYSTEM_NAME STREQUAL "Linux")` | in scope, C232 present, no C122 | yes |
| T-2 | fetch under `if(NOT CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)` | in scope | yes |
| T-2r | the guard with operands reversed | in scope | yes |
| T-3 | `option(X "" ON)` + `if(X)` | in scope | yes |
| T-4 | `option(X "" OFF)` + `if(X)` | Optional, `cmake-conditional` | no |
| T-5 | `if(WIN32 AND X)`, X OFF | Optional | no |
| T-6 | `else()` of `if(WIN32)` | in scope | yes |
| T-7 | `else()` of `if(X)`, X ON | Optional | no |
| T-7e | `elseif(Y)` after `if(X)`, X ON, Y ON | Optional (an earlier branch holds) | no |
| T-8 | nested: `if(UNIX)` around `if(X)`, X ON | in scope | yes |
| T-9 | option declared in the parent directory, fetch in a subdirectory added by `add_subdirectory` | as its default | yes (ON) |
| T-9b | an OFF option declared in the parent directory | Optional (`scope_files` reaches the default) | no |
| T-10 | `if(X)` with no default anywhere | in scope (unknown) | yes |
| T-11 | `if(EXISTS ${CMAKE_SOURCE_DIR}/x)` | in scope (unknown) | yes |
| T-12 | `cmake_dependent_option(X "" ON "UNIX" OFF)` + `if(X)` | in scope | yes |
| T-13 | the same option declared ON and OFF in reach | in scope (unknown) | yes |
| F-1…F-6 | the six fallback shapes (`NOT TARGET`, `NOT X_FOUND`, `FIND_PACKAGE_ARGS`, try-find `ALWAYS`, provider, function body) | unchanged from `main` | no |
| B-1 | `if(BUILD_TESTING)` | Test, unchanged | no |
| M-1 | the same repository declared under `if(NOT TARGET x)` and under `if(UNIX)` (one PURL) | in scope; C232 the union of both declarations' conditions; no C122 | yes |
| M-2 | an unconditional `ExternalProject_Add` and, in a file that sorts first, a `FetchContent_Declare` of the same URL under `if(UNIX)` | in scope; C55 `cmake-externalproject`, C56 `["cmake-fetchcontent-url"]`, as on `main` | no |
| R-1 | conditional fetch under `if(UNIX)` of a repository an enclosing submodule supplies | merged into the submodule (R9), unchanged | no |
| S-1 | `find_package(X)` under `if(UNIX)` only | the searched record in scope, C236 present | yes |
| S-2 | `find_package(X)` under an OFF option only | Optional, as today | no |
| S-3 | an installed record (`…ConfigVersion.cmake`) searched only under `if(UNIX)` | the record in scope, C236 `["UNIX"]` | yes |
| C-ctl | a tree with no catch-all conditional | byte-identical to `main`'s golden | no |
