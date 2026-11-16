# Pre-PR gate (T012)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` at `e66b3add`.

- `GATE_EXIT=0`, `GATE_RESULT=pass`.
- 406 `test result: ok.` lines; 0 `test result: FAILED`; 0 failure blocks.
- `tests/cmake_declared_datasets.rs`: `ok. 4 passed; 0 failed`.
