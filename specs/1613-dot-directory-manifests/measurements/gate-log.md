# T020 gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` at `bc13f385`, default features, no `+toolchain`. Results from `target/gate/prepr.log`:

| | |
|---|---|
| `GATE_EXIT` | 0 |
| `GATE_RESULT` | pass |
| `test result: ok.` lines | 414 |
| non-ok `test result:` lines | 0 |
| passed / failed | 9,148 / 0 |
| `---- … stdout ----` failure blocks | 0 |
| walker audit | `Walker-audit allow-list check: OK` (once) |
