# Pre-PR gate (T015)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` at `f9c75061`.

- `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line was found.
- 405 `test result: ok.` lines; 0 `test result: FAILED`; 0 failure blocks.
- `tests/installed_lib_embedded_paths.rs`: `ok. 4 passed; 0 failed`.
