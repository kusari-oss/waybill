# T023: pre-PR gate

Command: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`, so it resolves through `rust-toolchain.toml`.

## Run at `34f1f1c9` (all reader fixes, before T021–T022): pass

Read from `target/gate/prepr.log`:

| | |
|---|---|
| `GATE_RESULT` | `pass` (`GATE_EXIT=0`) |
| `>>> all pre-PR checks passed.` | present |
| `test result: ok` lines | 423 |
| `test result` lines with a non-zero failed count | 0 |
| tests passed / failed (summed over every `test result:` line) | 10,462 / 0 |
| `---- … stdout ----` failure blocks | 0 |

The log's two `error:` lines come from CLI rejection tests asserting their own messages (`--root-purl` validation). They are not build errors.
