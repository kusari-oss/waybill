# Implementation Plan: CMake fetches made through DownloadProject's `download_project()`

**Branch**: `1540-cmake-download-project` | **Date**: 2026-11-17 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1540-cmake-download-project/spec.md`

## Summary

Issue #1594, a follow-on to spec 1539 (#1518).

**The defect.** DownloadProject's `download_project()` fetches at configure time through a `configure_file()`d template, so its body has no fetch call and spec 1539 sees no wrapper. On TheRock the census (CI, SC-001) counts **8 calls, all with a literal GitHub repository, over 5 identities**. There are 9 vendored definitions, all with DownloadProject's shape, using 3 different keyword lists.

**The fix, in three parts:**
1. **Recognition by name and shape** (R1, clarification). A `download_project` definition is DownloadProject when it has a `cmake_parse_arguments(DL_ARGS …)` parsing `PROJ` and a `configure_file()` of `DownloadProject.CMakeLists.cmake.in`. Such a definition gets a synthesized body call, `ExternalProject_Add("${DL_ARGS_PROJ}" ${DL_ARGS_UNPARSED_ARGUMENTS})`. That is what probe D1 measures CMake running, named `<PROJ>` (clarification). Everything after that is spec 1539's binder, accounting (C231 `wrappers`) and evidence (C234), unchanged.
2. **Keywords from each definition** (R2, clarification). Spec 1539's binder learns to expand `cmake_parse_arguments`' keyword-list arguments in the body's scope. An unresolved list is unresolved (`Unbound::UnknownKeywords`), never guessed. This matters on TheRock, where 3 copies add `STATUS_MSG` and 2 lack `BUILD_PROJECT`. Probe D5 shows a keyword the copy does not parse is forwarded, and breaks the fetch.
3. **No marking of branch-name tags** (R6). 5 of 8 calls name `develop` or `master`, but `main` already emits branch names as versions for direct fetches, and no existing row fits. Filed as #1598 for all CMake git fetches.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: Existing only. Spec 1539's `cmake/wrappers.rs` (`Definitions`, `Scope`, `bind`), `regex`, `serde_json`, `tracing`. **Zero new Cargo dependencies** (FR-011).

**Storage**: N/A. Everything is in process for one scan.

**Testing**:
- **Unit** (`cmake/wrappers.rs`):
  - the shape check: shaped; no `DL_ARGS`; `PROJ` missing; another template name; a same-named function;
  - keyword-list expansion: literal; variable; empty; conditional (unresolved); appended (unresolved);
  - the synthesized call's binding for each of TheRock's three keyword lists.
- **Integration:** `waybill-cli/tests/cmake_fetch_wrappers.rs` gains contract cases D-1 to D-18 (`contracts/wire.md`), on a synthetic DownloadProject-shaped module written for the tests, in all three formats. W-1 to W-26 and spec 1538's C-1 to C-15 stay green.
- **Parity:** no new row. C234 and C231's extractors are unchanged.
- **Mutations:** the shape check's three conditions, keyword expansion, the unresolved-list rule, `<PROJ>` naming, and the synthesized call's position, each mutated and caught.
- **CI:**
  - `census-fetches` at the branch against `main`, compared with `measurements/census.md`'s prediction;
  - interleaved `measure-source`;
  - the public corpus in compare mode;
  - the quality corpus against a `main` control.

**Target Platform**: as today.

**Project Type**: CLI.

**Performance Goals**: SC-005, at most 1.5× `main`'s median full-tree scan.

**Constraints**: deterministic; static; no network; synthetic fixtures; TheRock in CI only.

**Scale/Scope**: TheRock has 9 definitions and 8 calls (census).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates, and no subprocess. |
| III. Fail Closed | Pass. An unresolved keyword list, a missing `PROJ`, a missing identity, or disagreeing definitions are unresolved and counted, never guessed. A same-named function without the shape is not bound. |
| IV. Type-Driven Correctness | Pass. `Unbound::UnknownKeywords` is a new variant of a closed enum. The alias is a `Definition` with a synthesized body call, not a parallel code path. |
| V. Specification Compliance | Pass. No new row; C234 and C231 carry it. Branch-name marking is deferred to #1598, because no existing row fits and a new one is out of scope (R6). |
| VII. Test Isolation | Pass. The fixture module is synthetic, not a copy of DownloadProject. The probe is synthetic, with localhost tarballs. TheRock is measured in CI only; single files were read through the GitHub API. |
| VIII. Completeness | Pass. 8 of 8 calls on TheRock are bound (predicted; a task checks it). |
| IX. Accuracy | Pass. Each binding rule is measured (D1–D6). Keywords come from each copy, because D5 shows a fixed list would mis-forward one. |
| X. Transparency | Pass. C234 names `download_project`; C231 counts definitions and calls. The branch-tag gap is published as #1598 with its count. |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1540-cmake-download-project/
├── spec.md
├── plan.md                  # this file
├── research.md              # R1–R8
├── data-model.md
├── quickstart.md
├── contracts/wire.md        # D-1…D-18, I-8
├── measurements/
│   ├── census.md            # SC-001
│   ├── census-download-project.json, census-download-project.py, census-refs.txt
│   └── cmake-semantics/     # probe.sh, results.txt, cmake-version.txt (D1–D6)
├── checklists/requirements.md
└── tasks.md                 # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs   # shape check + synthesized call in collect(); keyword-list expansion in Scope; Unbound::UnknownKeywords
waybill-cli/tests/cmake_fetch_wrappers.rs               # D-1…D-18
waybill-cli/tests/common/cmake_fetch.rs                 # I-8 check
docs/reference/sbom-format-mapping.md                   # C234 row: names download_project
docs/ecosystems.md                                      # CMake wrappers section: DownloadProject
CHANGELOG.md
```

**Structure Decision**: all in spec 1539's `wrappers.rs`. The alias is a synthesized body call on a recognised definition, so `fetches.rs` and the emitters need no change.

## Complexity Tracking

None.
