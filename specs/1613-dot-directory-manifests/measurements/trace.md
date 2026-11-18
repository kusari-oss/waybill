# T001 trace (on `main`'s code, before any change)

The branch held only spec files at this point, so the debug build is `main`'s code. Fixtures were built under the session scratchpad.

## (a) The quickstart tree is invisible today

The tree:
- `.ci/requirements.txt`, `.ci/metrics/requirements.txt` and `.ci/all_requirements.txt`, each declaring one package;
- `.github/actions/act/package.json`, with one dependency;
- a root `requirements.txt` (`waybill-fixture-root`).

Components emitted: `waybill-fixture-root` only. None of the four CI packages appear. **Confirmed.**

## (b) CMake discovers through the shared walker (R2)

The tree is a root `CMakeLists.txt` plus `.github/workflows/test-suite/waybill-fixture.cmake`, which holds `FetchContent_Declare(waybill-fixture-dep GIT_REPOSITORY …/dep.git GIT_TAG v1.2.3)`.

| Code | Components |
|---|---|
| `main` | none |
| scratch: `descend_into: [".github"]` on the CMake registration | `waybill-fixture-dep@v1.2.3` |

**Confirmed:** opting in through `descend_into` is enough for CMake. The scratch change was reverted with `cp`, and `git status` was clean afterwards.

## (c) The pip merge rule (U1)

The tree is a root `requirements.txt` declaring `waybill-fixture-both==1.0`, beside `requirements-ci.txt` declaring `waybill-fixture-both==1.0` and `waybill-fixture-cionly==1.0`. It has no dot-directory, so it runs on `main`.

| Component | CycloneDX scope | C155 | Source files |
|---|---|---|---|
| `waybill-fixture-both` | `required` | **`ci`** | `requirements-ci.txt`, `requirements.txt` |
| `waybill-fixture-cionly` | `excluded` | `ci` | `requirements-ci.txt` |

**What this shows:**
- **The runtime scope wins**, as FR-006 assumed.
- **The `ci` label survives the merge.** A runtime component carries C155 = `ci`, which reads as CI tooling to any consumer selecting by label.

This is existing `main` behaviour, and the feature makes it more common: a `.github/requirements.txt` often repeats the root's packages.

**Decision** (recorded as research R8; folded into T010): after deduplication, the R5 pass removes C155 from any component whose final scope is runtime (no scope, or `Runtime`). The label then agrees with the scope FR-006 gives. This also changes `main`'s output for the `requirements-<scope>.txt` + `requirements.txt` case; public-corpus goldens that move are traced in T022.

## T005: the US1 tests fail on `main`'s code

`cargo test -p waybill --test scan_dot_directory_manifests` (before T006–T011): 4 failed. Each fails at the "missing" assertion in `present()`:
- `d01_to_d05_…`;
- `d08_…`;
- `d10_…`;
- `d11_…`, where `waybill-fixture-cionly` is absent because `.ci` is not walked.

The other 135 passing tests in the binary are the corpus harness module the file includes.

## T012: D-10's first expectation was wrong

The first D-10 tree had `.github/actions/act/package.json`, which declares `waybill-fixture-npm-a`, and a committed `node_modules/` holding only `waybill-fixture-npm-junk`. It expected `npm-a` present and `npm-junk` absent. Instead it got `npm-junk` and no `npm-a`.

A probe on this build showed why: this is existing npm-reader behaviour, not a defect of the change.
- **A bare `package.json`** (no lockfile, no `node_modules/`) gets design-tier dependencies only at the scan root. Nested project roots emit nothing from a bare `package.json`.
- **With `node_modules/`,** the npm reader reads each project root's `node_modules/` as its installed tree.

GitHub JavaScript actions commit `node_modules/`, and it is what they ship. The spec's edge case conflated two things:
- the *walker's* skip, which still holds: no project root is found inside `node_modules/`;
- the npm reader's installed-tree read.

D-10 now installs the declared dependency in the action's `node_modules/`, and the spec edge case and contract say so. Probe result on this build: `waybill-fixture-npm-a@1.0.0`, `scope: excluded`, C62 `ci-directory`, source file `.github/actions/act/node_modules/waybill-fixture-npm-a/package.json`; the root requirements package stays `required`.
