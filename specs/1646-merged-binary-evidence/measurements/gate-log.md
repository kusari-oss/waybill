# T017: gate on the final commit

- **Commit**: `037dba7c`. The commits after it (the gate log, the T018 notes and the `census.md` wording) change only `specs/`.
- **Command**: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with default features and no `+toolchain`.
- **Result**: `GATE_EXIT=0` and `GATE_RESULT=pass`. The expected line `>>> all pre-PR checks passed.` is present once.
- **Totals**: 429 `test result: ok.` lines and 0 `test result: FAILED` lines. 11,046 passed, 0 failed, 47 ignored, and no `---- … stdout ----` failure block.
- **Ignored**: 47, the baseline before this spec. The 5 cases planned red at the log-only commit are all un-ignored and pass.
- **FR-009**: `git diff e1148818 -- Cargo.lock '*Cargo.toml'` is empty, so there are no new dependencies.
