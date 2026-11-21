# T027: pre-PR gate

Command: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`, so it resolved through `rust-toolchain.toml`.

## First run, at `b57f0c36`: fail

- `GATE_RESULT=fail`, `GATE_EXIT=101`, the expected line missing.
- 10,281 passed, 1 failed: `installed_record_scope::i_ctl_is_byte_identical_to_main`, spec 1617's control golden.
- The regenerated golden was diffed against the old one with `jq -S`. In all three formats the only difference is `waybill:also-detected-via` `["cmake-find-package"]` on the installed CMake record that absorbs the tree's `find_package` (S9).
- That is FR-001 and FR-004 working as specified, so the golden was regenerated and committed with that reason (`b3229cb1`).

## Second run, at `b3229cb1`: pass

Read from `target/gate/prepr.log`:

| | |
|---|---|
| `GATE_RESULT` | `pass` (`GATE_EXIT=0`) |
| `>>> all pre-PR checks passed.` | present |
| `test result: ok` lines | 422 |
| `test result` lines with a non-zero failed count | 0 |
| tests passed / failed (summed over every `test result:` line) | 10,282 / 0 |
| `---- … stdout ----` failure blocks | 0 |
