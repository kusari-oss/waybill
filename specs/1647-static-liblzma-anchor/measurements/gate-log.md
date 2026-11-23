# T019: gate on the final commit

- **Commit**: `a3d60b95`. The commits after it (this log, the T020 notes and two contract notes) change only `specs/`.
- **Command**: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with default features and no `+toolchain`.
- **Result**: `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line `>>> all pre-PR checks passed.` present once.
- **Totals**: 430 `test result: ok.` lines and 0 `test result: FAILED` lines. 11,061 passed, 0 failed, 48 ignored, and no `---- … stdout ----` failure block.
- **Ignored**: 48, the baseline of 47 plus `control_byte_identical_to_main`, which runs only when given `main`'s binary. S1, S2 and S5, ignored on the log-only commit, run and pass.
- **No new dependencies**: `git diff b6ad3c26 -- Cargo.lock '*Cargo.toml'` is empty.
