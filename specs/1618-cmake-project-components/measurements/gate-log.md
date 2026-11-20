# T025 gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` at `cc84f823`:

- `GATE_EXIT=0`, `GATE_RESULT=pass`
- 420 `test result: ok` lines, 0 non-ok, 0 failure blocks
- 9,970 passed, 0 failed
- `git diff --stat main -- '*Cargo.toml' Cargo.lock`: empty

Earlier runs on this branch failed and were fixed before this one:
1. Clippy: identical `if` blocks in `record_scopes`, merged into one condition (M9 re-run), and a needless `to_string` in a test.
2. `cdx_regression`, `spdx_regression`, `spdx3_regression` (`cmake` ecosystem): the CMake-only fixture's root became its project; goldens regenerated and traced in commit `cc84f823`.
