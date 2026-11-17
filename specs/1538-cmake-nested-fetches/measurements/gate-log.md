# Pre-PR gate (T028)

**Commit:** `505153ae`.
**Command:** `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Default features, no `+toolchain`.
**Log:** `target/gate/prepr.log`. It was run alone: an earlier attempt overlapped another cargo build, and the machine went down.

| | value |
|---|---|
| `GATE_EXIT` / `GATE_RESULT` | `0` / `pass` |
| expected line `>>> all pre-PR checks passed.` | found, once |
| `test result: ok. N passed; 0 failed` lines | 409 |
| `test result:` lines that are not ok | 0 |
| tests passed, summed | 8,426 |
| `tests/cmake_nested_fetches.rs` | 22 passed, 0 failed, including C-12c to C-12f and C-13b |

**Dependencies:** `git diff main -- Cargo.toml '*/Cargo.toml' Cargo.lock` is empty. There are zero new Cargo dependencies (FR-011).
