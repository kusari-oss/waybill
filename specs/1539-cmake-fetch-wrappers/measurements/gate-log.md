# Pre-PR gate (T024)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, run alone with no other cargo process, at `87fe7c51`. No `+toolchain`.

- `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line found once.
- 410 `test result: ok.` lines, 8,473 tests passed, 0 `FAILED` lines.
- `cmake_fetch_wrappers`: 29 passed, 0 failed. `cmake_nested_fetches`: 22 passed, 0 failed.
- `git diff --quiet main -- Cargo.toml Cargo.lock '*/Cargo.toml'`: no change (FR-013).
