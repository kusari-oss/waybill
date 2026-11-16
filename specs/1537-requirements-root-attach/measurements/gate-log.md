# Gate (T024)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, at `c2694a08`, with default features and no `+toolchain`.

- `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line was found.
- 408 `test result: ok. N passed; 0 failed` lines. 0 `FAILED` results, and 0 `---- … stdout ----` blocks.
- 8,379 tests passed across those targets.
- `tests/declared_requirements.rs`: 18 passed, 0 failed.
- `git diff --quiet main -- Cargo.lock Cargo.toml '*/Cargo.toml'`: no dependency change.

**The first run at `ae34c492` failed one test,** `pants_targets_agree_across_formats`. It pins the root's out-edge counts in the pants goldens, and the regenerated goldens moved them:
- `pants-example-django`: 14 → 15;
- `pants-example-python`: 2 → 4.

All three formats still agree, which is what the test guards. The counts were updated in `c2694a08`.
