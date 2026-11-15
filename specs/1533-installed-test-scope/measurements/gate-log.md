# Pre-PR gate (T020)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` at `0c06a355`. The uncommitted changes at the time were spec documents only.

- `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line was found.
- 403 `test result: ok.` lines; 0 `test result: FAILED`; 0 `---- … stdout ----` failure blocks.
- `tests/installed_test_scope.rs`: `ok. 7 passed; 0 failed`.
