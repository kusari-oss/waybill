# T019: pre-PR gate

Command: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`, so it resolves through `rust-toolchain.toml`.

## Runs

1. **At `8d76de26`: incomplete.** The machine crashed mid-run. The log ends during `cargo test` with no `GATE_RESULT`, so it is evidence of nothing.
2. **At `dce5cc36`: fail.** One target failed: `scan_cmake_tree_finds` (spec 1614), 153 passed, 7 failed. It had been failing since `3df578c4`; T014 updated the other CMake suites and missed this one.
   - Six cases (W-5, W-6, W-8, W-11, W-14b, W-18) expected Optional under names with no visible default, which FR-004 now keeps in scope. Each fixture gained a default-OFF `option()` in reach, so the test still checks condition propagation.
   - W-17 found a defect. A search in a `function()` body had come into scope, although spec 1614 gives it spec 1538's sixth shape, which this spec leaves Optional. Fixed in `searches.rs::scopes`; mutation M25 (`mutations.md`).
3. **At `49c12ee8`: pass.**

## Run at `49c12ee8`

Read from `target/gate/prepr.log`:

| | |
|---|---|
| `GATE_RESULT` | `pass` (`GATE_EXIT=0`) |
| `>>> all pre-PR checks passed.` | present |
| `test result: ok` lines | 424 |
| `test result` lines with a non-zero failed count | 0 |
| tests passed / failed / ignored (summed over every `test result:` line) | 10,642 / 0 / 47 |
| `---- … stdout ----` failure blocks | 0 |
| `scan_cmake_tree_finds` | 160 passed, 0 failed |
| `cmake_conditional_scope` | 166 passed, 0 failed |

Clippy (`--workspace --all-targets -D warnings`) finished clean. The log's `error:` lines come from CLI rejection tests asserting their own messages (`--root-purl` validation). They are not build errors.
