# T019: gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, default features, no `+toolchain`.

| Commit | GATE_RESULT | ok lines | non-ok lines | passed / failed | failure blocks |
|---|---|---|---|---|---|
| `3c27229e`, first run | **fail** | 417 | 1 | — | 1: `gradle_ladder::sc005_subprocess_timeout_degrades_gracefully` |
| `3c27229e`, re-run | pass | 418 | 0 | 9,781 / 0 | 0 |

**The first run's failure is #1492.** The test took 15.09 s against its 10 s bound, which is the mock Gradle's full 15 s sleep. That is #1492's open, documented symptom under full-suite load. Run alone 5 times on the same commit, it passed every time, at 3.19–3.21 s. This change touches only `scan_fs/cmake_search_pair.rs` and one call in `scan_fs/mod.rs`, neither on the Gradle subprocess path. The re-run passed without any change.

- **FR-008:** `git diff --stat main -- '*Cargo.toml' Cargo.lock` is empty.
- **Goldens:** no repository golden changed.
