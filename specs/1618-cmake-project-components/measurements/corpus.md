# T027 corpora

## Public corpus (SC-004)

Run 44773742294 with `regen_goldens=true`, `SCANNED_SHA` `62cfe74e`: 166 corpus tests passed. All 66 committed goldens were regenerated, under the same names.
- **63 are byte-identical.** No public-corpus repository declares a `project()` (research R12).
- **3 differ: `nix-closure-slacklinker`, unrelated to this branch.** A crate embedded in two binaries of `cargo-auditable-0.7.2` cites either `bin/auditable2cdx` or `bin/cargo-auditable` from run to run. The scheduled run on `main` `55141875` (44688132058) failed on the same drift, and the dispatched regeneration at `a82d2f7b` (44756595932) was byte-identical on all 66. Filed as #1637, and the goldens are not changed here.

## Quality corpus (SC-003, SC-008)

Branch run 44773746593 (scanned `62cfe74e`): success under the re-baselined bounds. Control on `main` `786091a8`: run 44756604395. The earlier branch run at `a82d2f7b` (44756600371) failed 5 bounds, all traced here and re-baselined in `xtask/corpus/quality-corpus.toml`.

| Target | sbomqs | Packages | Edges | Depth | Flat |
|---|---|---|---|---|---|
| `cmake-nlohmann-json`, `main` → branch | 6.56 → 6.06 | 19 → 25 | 20 → 30 | 1 → 2 | yes → no |
| `python-pytorch`, `main` → branch | 5.94 → 5.83 | 257 → 276 | 256 → 303 | 2 → 2 | no → no |
| `cpp-mongo`, both | 6.16 | 418 | 486 | 7 | no |

The changes were reproduced locally with the harness's invocation (`--offline`, `--root-name <target>`, `--root-version <pin>`), using `main`'s binary built at `786091a8` and the branch binary, on the cached checkouts.

**`cmake-nlohmann-json`:**
- **+6 projects:**
  - `nlohmann_json@3.12.0`, runtime: the root `CMakeLists.txt`;
  - `json_example`, Development: 8 `docs/…/integration/*` declarations merged into one;
  - `dummyimport`, `dummyimportminver`, `json_benchmarks`, `json_cuda`, Test (`tests/`).
- **Graph.** `nlohmann_json` owns the tree, so the pip docs tooling's edges move from the root to it, and the graph is no longer flat.
- **Bazel.** The two Bazel modules lose the fallback root edge and carry `foreign-build-system`.
- **sbomqs** falls because the five test, example and docs projects declare no version, licence or supplier.
- **Cycle:** `nlohmann_json ⇄ dummyimportminver`, the accepted enclosure-plus-search-back pattern (#1635).

**`python-pytorch`:**
- **+19 projects.**
  - **Runtime:** `c10`, `torch`, `libshm`, `headeronly`, `pytorch_qnnpack`, `clog`, `pytorch_nnapi`, `nccl_ep`, `pytorch_jni`, `pytorch_jni_lite`, `pytorch_vision_jni`.
  - **Test:** 7 projects under `test/`.
  - **Development:** `simple-torch-test`, under `.ci/`, through spec 1613's `ci-directory`.
- **+56 edges, 9 removed.** The fetches and searches that `pytorch_qnnpack` and `c10` declare move from `pkg:pypi/torch` to them, and the override root anchors each project.
- **Defect found and fixed here.** `pytorch_jni` and `pytorch_jni_lite`, both declared in `android/pytorch_android/CMakeLists.txt`, at first enclosed each other. Two projects in one directory are now siblings (P-21, M14). The edge count is unchanged at 303: two mutual edges were replaced by one edge to each from their encloser.

**`cpp-mongo`: unchanged.** Its three projects are under `src/third_party/*/dist`, and `dist` is in the shared walker's built-in skip set (FR-009). The T001 census counted them because it does not apply that skip set.

**The other 15 targets are identical** between the control and the branch in every measured column except wall time: sbomqs, packages, files, edges, depth, flat and waybill's self-report. That is 16 of 18 including `cpp-mongo`, compared row by row from the two runs' logs.

**SC-008.** Every target with another ecosystem's main module keeps its root and its C69 heuristic. The quality harness passes `--root-name`, so its roots are overrides; within this repository P-14 and P-15 test the no-override case.
