# T016: pre-PR gate

Command: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, default features and no `+toolchain`. The log is `target/gate/prepr.log`.

| commit | run | result |
|---|---|---|
| `4caee6a1` | 1 | interrupted when the session restarted: 45 suites `ok`, none failed, no final line. Not a result. |
| `4caee6a1` | 2 | `GATE_RESULT=pass`. 426 `test result: ok` lines; 10,972 passed, 0 failed, 47 ignored; `>>> all pre-PR checks passed.` |
| `1586d74b` (the SC-006 fix) | 1 | `GATE_RESULT=fail`, `GATE_EXIT=101`. One failure: `go_proxy_fetch_bounds::t010_unreachable_proxy_is_reported_and_loses_nothing`, with 24 Go modules from go.sum where it asserts 40. This is the open flake #1476: the same signature, under full-suite parallel load, in a test this spec does not touch. The test passed 30 of 30 in isolation on this commit. |
| `1586d74b` | 2 | **`GATE_RESULT=pass`.** 426 `test result: ok` lines; 10,972 passed, 0 failed, 47 ignored; `>>> all pre-PR checks passed.` `go_proxy_fetch_bounds`: 6 passed, t010 included. |

The final passing run is on `1586d74b`, and the working tree's source was identical to that commit.
