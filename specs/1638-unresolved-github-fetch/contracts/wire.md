# Wire contract: Name a CMake git fetch with an unresolved reference after its repository

## Component shapes

**GitHub repository, unresolved reference** (FR-001), where only the reference is unresolved:

```text
FetchContent_Declare(waybill-fixture-target
  GIT_REPOSITORY https://github.com/Waybill-Org/Waybill-Fixture-Dep.git
  GIT_TAG ${WAYBILL_FIXTURE_UNSET})
```

- purl: `pkg:github/waybill-org/waybill-fixture-dep` (no `@`)
- name: `waybill-fixture-target`
- `waybill:sbom-tier`: `design`
- C151 `waybill:unresolved-reason`: `URL or GIT_TAG uses a CMake variable with no literal set() in the same file`
- C55 `waybill:source-mechanism`: `cmake-fetchcontent-git` (or `cmake-externalproject`)
- `waybill:download-url`: `https://github.com/Waybill-Org/Waybill-Fixture-Dep.git`
- scope and C122 / C232 / C236 / C234: as the declaration gives them, unchanged rules

**Other repository, unresolved reference** (FR-004): as today, plus `waybill:download-url`.

**Everything else** (FR-005, FR-006, FR-010): byte-identical to `main`.

## Rows touched

| Row | Change |
|---|---|
| C55, C122, C151, C232, C236 | Values unchanged. They now appear on `pkg:github` components where they appeared on `pkg:generic` ones. |
| C231 | Behaviour as documented. The row's `unresolved.identity` sentence gains: "unless a literal GitHub `GIT_REPOSITORY` names the component" (FR-008). |
| C233 | Row adds: "`version` is `""` for a declaration whose reference is unresolved, on both paths" (FR-013), and names a fallback merged into a repository a versionless component stands for. |
| C234 | The row's "whose built-in call has no identity of its own" gains: "a GitHub repository whose reference is unresolved does not give a call whose name is a forwarded parameter an identity of its own" (R2). |

No new row. Every changed row keeps its extractor; parity-check covers C55, C122, C151, C232, C233, C234 and C236 (FR-009).

## Cases (`waybill-cli/tests/cmake_unresolved_github_fetch.rs`)

Every case runs in CycloneDX, SPDX 2.3 and SPDX 3, and passes `waybill sbom parity-check`; no `bom-ref` repeats.

| Case | Tree | Expected |
|---|---|---|
| U-1 | direct `FetchContent_Declare`, mixed-case GitHub owner/repo, `GIT_TAG ${UNSET}` | `pkg:github/<lower>/<lower>`, no version, C151, design, C55 `cmake-fetchcontent-git`, download-url |
| U-2 | direct `ExternalProject_Add`, same | same identity, C55 `cmake-externalproject` |
| U-3 | wrapper `wf_dep(noimage project)`, body `ExternalProject_Add(${project} GIT_REPOSITORY <github> GIT_TAG ${V})` with `V` set twice; call `wf_dep(OFF waybill-fixture-target)` (the aotriton shape) | `pkg:github/...`, name `waybill-fixture-target`, C234 `[["wf_dep"]]`; C231 `wrappers.definitions` 1, `calls` 1 |
| U-4 | `download_project(PROJ waybill-fixture-target GIT_REPOSITORY <github> GIT_TAG ${UNSET})` with the module's shape (spec 1540 fixture) | `pkg:github/...`, C234 `[["download_project"]]` |
| U-5 | top-level `FetchContent_Declare(${UNSET_NAME} GIT_REPOSITORY <github> GIT_TAG ${UNSET})` | emitted, name = repository; C231 `emitted` includes it, `unresolved.identity` does not; equation holds |
| U-6 | wrapper `wf_tag(name tag)` over `FetchContent_Declare(${name} GIT_REPOSITORY <github> GIT_TAG ${tag})`; calls `wf_tag(a v1.2)` and `wf_tag(b ${UNSET})` | `pkg:github/...@v1.2` byte-identical to `main`; plus versionless `pkg:github/...`; both C234 `[["wf_tag"]]` |
| U-7 | direct git fetch, `https://example.invalid/waybill-fixture-other.git`, `GIT_TAG ${UNSET}` | `pkg:generic/<declared name>`, C151, download-url |
| U-8 | required resolved `FetchContent_Declare` of the U-7 URL at the root; conditional unresolved fetch of it in `sub/` | one component (the resolved one); C233 `[{"file":"sub/CMakeLists.txt","version":""}]`; C231 `merged.existing-component` 1 |
| U-9 | `GIT_REPOSITORY https://github.com/${OWNER}/waybill-fixture-dep.git`, `GIT_TAG ${UNSET}` | `pkg:generic/<declared name>`, no download-url; byte-identical to `main` |
| U-10 | `URL https://example.invalid/waybill-fixture-u-${V}.tar.gz` | unchanged from `main` |
| U-11 | git submodule `https://github.com/waybill-org/waybill-fixture-dep.git` and, in the same directory, `if(NOT TARGET …)` fetch of the same URL with `GIT_TAG ${UNSET}` | one component (the submodule's); C233 lists the fetch with `"version": ""`; C231 `merged.existing-component` 1 |
| U-12 | two unresolved fetches of one GitHub repository, names `a` and `b`, in `x/` and `y/` | one component; C18 lists both files; C231 `emitted` 2, `components` 1 |
| U-13a | required resolved (`GIT_TAG v1`) in `x/` and required unresolved in `y/`, one GitHub repository | two components: `@v1` and versionless (FR-014) |
| U-13b | same two declarations, both named `waybill-fixture-dep`, both in the root `CMakeLists.txt` | two components (FR-014; research R5) |
| U-14 | required unresolved at the root; conditional resolved (`GIT_TAG v2`) of the same repository in `sub/` | one versionless component; C233 `[{"file":"sub/CMakeLists.txt","version":"v2"}]` |
| U-15 | conditional unresolved fetch, no supplier anywhere | emitted on its own, Optional, C122, C232 |
| U-16 | required resolved non-GitHub `ExternalProject_Add` at the root; conditional unresolved `ExternalProject_Add` of the same URL in `sub/` | two components, as with two resolved ones (research R3) |
| U-17 | resolved `FetchContent_Declare(waybill-fixture-dep GIT_REPOSITORY <github> GIT_TAG v1)` at the root; unresolved `FetchContent_Declare(waybill-fixture-dep GIT_REPOSITORY <github> GIT_TAG ${UNSET})` in `sub/` (R5 by name) | one component `@v1`; C233 `[{"file":"sub/CMakeLists.txt","version":""}]`; C231 `merged.first-declaration-wins` 1 (FR-013) |
| U-ctl | resolved GitHub and generic fetches, U-6's literal-tag call alone, U-9, U-10 | byte-identical to the golden from `main`'s binary |
| U-rep | U-1..U-17 trees, scanned twice | identical output |
| U-xhost | one tree combining U-1, U-3, U-7, U-11's fetch side and U-12 | masked output byte-identical to `FX/changed-golden/` on every CI lane (Linux, macOS, Windows), for SC-005's cross-runner identity |
