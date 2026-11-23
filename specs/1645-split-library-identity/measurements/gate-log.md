# T021: gate on the final commit

- **Commit**: `4953c559470c16c5df63f62fc2d0ba9d590e410f`, with a clean tree. It includes the regenerated `nix-closure-openai` goldens and B-1's golden.
- **Command**: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with default features and no `+toolchain`.
- **Result**: `GATE_EXIT=0` and `GATE_RESULT=pass`. The expected line `>>> all pre-PR checks passed.` is present once.
- **Totals**: 428 `test result: ok.` lines and 0 `test result: FAILED` lines. 11,031 passed, 0 failed, 47 ignored, and no `---- … stdout ----` failure block.
- **Ignored**: 47, spec 1644's final count. The 4 cases planned red at the log-only commit (M-1, M-2, M-3, E-1) are all un-ignored and pass.
- **FR-012**: `git diff bb9d6cc4 -- Cargo.lock '*Cargo.toml'` is empty, so there are no new dependencies.
