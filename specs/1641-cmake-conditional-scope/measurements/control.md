# T008, T013, T015: the control at the log-only commit

The cases ran at the log-only commit. It has the evaluator, branch history, defaults and census events, but no scope change.

Command: `cargo test -p waybill --test cmake_conditional_scope -- --skip corpus_harness`.

**Result: 15 passed, 13 failed.** The 13 failures are exactly the cases `contracts/wire.md` marks "Changed". Each fails on its scope assertion (`scope == "required"`), because the component is still `excluded`.

| Case | At the log-only commit |
|---|---|
| T-1 platform check | fails: excluded |
| T-2 super-build guard | fails: excluded |
| T-2r guard reversed | fails: excluded |
| T-3 option ON | fails: excluded |
| T-6 else of a platform check | fails: excluded |
| T-8 nested | fails: excluded |
| T-9 option in the parent directory | fails: excluded |
| T-10 no default visible | fails: excluded |
| T-11 unreadable expression | fails: excluded |
| T-12 dependent option | fails: excluded |
| T-13 conflicting defaults | fails: excluded |
| M-1 merged declarations | fails: excluded |
| S-1 package search under a platform check | fails: excluded |
| T-4, T-5, T-7, T-7e (still Optional) | pass |
| S-2 search under an OFF option | passes |
| R-1 R9 merge into a submodule | passes |
| F-1 … F-6 fallback shapes | pass |
| B-1 `BUILD_TESTING` | passes |
| C-ctl (golden from `main`'s binary) | passes |
| census positive control (T006) | passes: one event per category, expected evaluations |

## Unchanged output at the log-only commit

Every CMake-related integration suite passed at this commit, each run on its own: 25 suites, among them `cmake_*`, `installed_*`, `git_submodule_reader`, `also_detected_via` and `cross_reader_dedup_ecosystem_107`. So did 169 CMake unit tests.
