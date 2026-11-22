# T021: gate on the final commit

- **Commit**: `2f990c0c1f94b51b955e06a0723435d39aefb07b`, with a clean tree. It includes the regenerated Nix closure goldens, which tests that are not ignored also read.
- **Command**: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with default features and no `+toolchain`.
- **Result**: `GATE_EXIT=0` and `GATE_RESULT=pass`. The expected line `>>> all pre-PR checks passed.` is present once.
- **Totals**: 427 `test result: ok.` lines and 0 `test result: FAILED` lines. 11,009 passed, 0 failed, 47 ignored, and no `---- … stdout ----` failure block.
- **Ignored**: 47. That is the log-only commit's 55 less the 8 cases planned red there (A-1 to A-5, A-CPE, P-1 and P-2), which are all un-ignored now. It equals spec 1643's last gate.
