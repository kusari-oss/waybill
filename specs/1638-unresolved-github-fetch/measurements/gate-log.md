# T021: pre-PR gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`, at `81b833ca`.

`target/gate/prepr.log`:
- `>>> all pre-PR checks passed.`, `GATE_EXIT=0`, `GATE_RESULT=pass`.
- Steps: homoglyph check, walker audit, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`.
- 421 `test result: ok` lines, summing to 10,131 passed and 0 failed; no `test result: FAILED`.
- Two `^error:` lines (log lines 15370 and 15373) are stderr from `--root-purl` validation tests, which assert those messages.
