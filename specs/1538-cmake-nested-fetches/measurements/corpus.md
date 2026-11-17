# Corpora (T026)

## Public corpus: unchanged

| run | waybill | result |
|---|---|---|
| 44024740653 | `505153ae` (final) | compare mode, **158 passed, 0 failed**. The goldens are byte-identical. |
| 44020740646 | `d3e8b0c6` | 158 passed, 0 failed |
| 44011633657 | `7d6a5943` | cancelled by the 45-minute job timeout |

**The cancelled run.** `corpus_nix_closure_slacklinker` ran 39+ minutes before the job timed out. It is a Nix closure target that this branch does not touch.
- Its usual time is 10–12 minutes (run 43977779899).
- A control at `main` (44019256105), dispatched beside the next branch run, passed in 16 minutes.
- The branch's next two runs took 18 and 23 minutes.

The slow run did not recur at the branch or at `main`.

## Quality corpus: three C++/CMake targets move, each traced

| run | waybill | violations |
|---|---|---|
| 44024743932 | `505153ae` | none, with the re-authored pytorch band |
| 44020744424 | `d3e8b0c6` | pytorch `edges` 109, above the band max of 107 |
| 44011636941 | `7d6a5943` | none |

Against `main` (run 44012060504, `7e567e0d`), only three targets change:

| target | `main` → branch |
|---|---|
| cmake-nlohmann-json | pkgs 19 → 21, edges 20 → 22 |
| cpp-mongo | pkgs 407 → 411, edges 475 → 479 |
| python-pytorch | pkgs 151 → 158, edges 97 → 108 |

**How each was traced.** Each target was scanned locally at its pinned commit with the released v0.17.0 binary (`main`) and with the branch binary, and the sorted, unique PURL sets were diffed (`LC_ALL=C comm`):
- **cpp-mongo** (`41a5752480dd`): +4, -0. The four are the wiredtiger CMake fetches: `aws-sdk-cpp@1.11.17`, `azure-sdk-for-cpp@azure-storage-blobs_12.10.0`, `google-cloud-cpp@v2.36.0` (all `cmake-conditional`), and `lazyfs@b0383127`.
- **cmake-nlohmann-json** (`65ee68451d8e`): +2, -0. Both are fetches in its own `tests/` tree, test-scoped (C62 `source-test-directory`): `pkg:generic/json` and `pkg:github/google/benchmark@origin/main`.
- **python-pytorch** (`2b3ec3482903`): +8, -1.
  - **Added:** qnnpack's seven `Download*.cmake` fetches (`fp16`, `fxdiv`, `psimd`, `pthreadpool`, `cpuinfo`, `googletest@1.10.0`, `googlebenchmark@1.4.1`), and `googletest@release-1.12.1` from `test/inductor`, which is test-scoped.
  - **Removed:** `pkg:generic/${project}`, a parameterised `ExternalProject_Add` name that v0.17.0 emitted as a component.

**A regression caught.** The first pytorch trace also showed `dlfcn-win32@v1.4.2` and `xz@v5.8.1` removed. Both are `ExternalProject_Add(${…} GIT_REPOSITORY https://github.com/…)` inside a function. That was a regression, fixed in `d3e8b0c6` (C-13b, T025b), and the two are emitted again, named `dlfcn-win32` and `xz` rather than after the variable.

**Band change.** Only pytorch's `edges` band violated. It is re-authored in `xtask/corpus/quality-corpus.toml` to ±10% of 109, giving 98–120. The final run observed 108, inside the band.
- The one-edge difference between `d3e8b0c6` (109) and `505153ae` (108) was not traced. *Inference:* it is probably the exact-PURL merge in `505153ae` (C-12f), the only change between the two. The band holds either way.
- The other bounds and targets are unchanged.
