# T020: gate on the final commit

- **Commit**: `e160526f`. The commits after it (this log, T021's notes and the corpus-count corrections) change only `specs/` and one CHANGELOG line of spec 1647's entry.
- **Command**: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with default features and no `+toolchain`.
- **Result**: `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line `>>> all pre-PR checks passed.` present once.
- **Totals**: 431 `test result: ok.` lines and 0 `test result: FAILED` lines. 11,072 passed, 0 failed, 49 ignored, and no `---- … stdout ----` failure block.
- **Ignored**: 49, the baseline of 48 plus `control_byte_identical_to_main`, which runs only when given `main`'s binary. K1, K2 and K6, ignored on the log-only commit, run and pass.
- **No new dependencies**: `git diff 33c445f6 -- Cargo.lock '*Cargo.toml'` is empty.
