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

## Re-runs after R9–R11

| Commit | GATE_RESULT | ok lines | passed / failed | failure blocks | walker audit |
|---|---|---|---|---|---|
| `38c4babb` (R9) | pass | 414 | 9,151 / 0 | 0 | OK |
| `bbeb4c5c` (R10) | pass | 414 | 9,153 / 0 | 0 | OK |
| `b8b840fe` (R11) | **fail** — clippy `if_same_then_else`, no tests ran | 0 | — | — | — |
| `d87f5d05` (fold) | pass | 414 | 9,154 / 0 | 0 | OK |
