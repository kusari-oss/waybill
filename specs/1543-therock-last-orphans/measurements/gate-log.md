# Gate (T028)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` at `90745535`, run alone, default features, no `+toolchain`.

- `GATE_EXIT=0`, `GATE_RESULT=pass`; the expected line was found.
- 413 `test result: ok` lines; 0 result lines with a non-zero failure count; **8,986 passed, 0 failed**; 0 `---- … stdout ----` failure blocks.
- `tests/scan_therock_last_orphans.rs` ran in the gate.
