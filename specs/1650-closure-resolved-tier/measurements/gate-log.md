# T018: the pre-PR gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` was run at `da562955c27f40c1831056591cf0702e54fd6d26`, with default features and no `+toolchain`. Beforehand, `cargo clean` was run, since `target/debug/deps` held 13,337 entries.

| Run | Result | Totals |
|---|---|---|
| 1 | **pass** | 433 `test result` lines, all `ok`; **11,224 passed, 0 failed**; 0 failure blocks |

The log ends with:

```
GATE_NAME=prepr
GATE_EXIT=0
GATE_RESULT=pass
```

and contains `>>> all pre-PR checks passed.` once.

`git diff 18691d8 -- Cargo.lock '*Cargo.toml'` is empty (0 lines): no new dependencies.
