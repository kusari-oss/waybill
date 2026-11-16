# Pre-PR gate (T018)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, at `c690b8d8` (the working tree differed only in `specs/`).

- `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line `>>> all pre-PR checks passed.` found once.
- 407 `test result: ok.` lines, 0 `test result: FAILED`, 0 result lines with a non-zero failure count. Counted over the whole log, `target/gate/prepr.log`.
- `tests/python_owned_binaries.rs`: `ok. 22 passed; 0 failed`.
