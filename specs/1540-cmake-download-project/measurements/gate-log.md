# Pre-PR gate (T017)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, run alone with no other cargo process, at `0c3f1257`. No `+toolchain`.

- **Exit:** `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line found once.
- **Tests:** 410 `test result: ok.` lines, 8,497 tests passed, 0 `FAILED` lines.
- **CMake suites:** `cmake_fetch_wrappers` 48 passed, 0 failed; `cmake_nested_fetches` 22 passed, 0 failed.
- **Dependencies (FR-011):** `git diff --quiet main -- Cargo.toml Cargo.lock '*/Cargo.toml'` shows no change.
