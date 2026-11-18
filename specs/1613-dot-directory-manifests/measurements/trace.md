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
