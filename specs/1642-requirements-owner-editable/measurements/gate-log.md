# T017: pre-PR gate

Command: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`.

## Runs

1. **At `1474a600`: fail.** One target failed: `propagation_seeds` (spec 1616), 141 passed and 1 failed. P-7 is a byte-identity control against `main`'s golden. Its single main module is merged with its `uv.lock` entry, so FR-001 records the manifest directory in C18: `["uv.lock"]` becomes `[".","uv.lock"]`. The golden was regenerated, and its diff is that one value, in each of the three formats (`9685499f`). FR-007 as amended covers this: byte-identity holds only where no main module merges with a lockfile entry.
2. **At `9685499f`: pass.**

## Run at `9685499f`

Read from `target/gate/prepr.log`:

| | |
|---|---|
| `GATE_RESULT` | `pass` (`GATE_EXIT=0`) |
| `>>> all pre-PR checks passed.` | present |
| `test result: ok` lines | 425 |
| `test result` lines with a non-zero failed count | 0 |
| tests passed / failed / ignored (summed over every `test result:` line) | 10,805 / 0 / 47 |
| `---- … stdout ----` failure blocks | 0 |
| `requirements_owner` | 155 passed, 0 failed |
| `propagation_seeds` | 142 passed, 0 failed |
| `declared_requirements` (B-2) | 18 passed, 0 failed |

Clippy (`--workspace --all-targets`) finished clean.
