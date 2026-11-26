# After: spec 1660

## Mechanism (US1; `collide.md`)

0 duplicate names among 36,900 real ones; the control collided in every batch. The inferred same-seed collision did not reproduce on this host. The fix rests on the code fact that the output paths were unreserved.

## Fix (US2; `sites.md`)

- **23 sites in 21 test files**, all of the dropped shape, all fixed by holding a `TempDir` until the output is read.
- **`scan_npm`** keeps its directory in a thread-local beside the path it already stored.
- **Tests:** the 21 changed binaries run 98 tests, all passing, with the same 98 listed before and after.

## Check (US3; `check-controls.md`)

- **`scripts/check-held-temp-paths.py`:** the self-test passes, 7 must-flag cases and 7 must-pass cases.
- **Positive control:** on the unfixed tree it listed exactly the 23 sites; after the fix it is clean.
- **Where it runs:** `pre-pr.sh` and CI's Linux `lint-and-test` job.

## Suite under nextest, 3 runs (FR-007)

| run | summary |
|---|---|
| 1 | `9299 tests run: 9299 passed, 56 skipped` (95.3 s) |
| 2 | `9299 tests run: 9299 passed, 56 skipped` (94.6 s) |
| 3 | `9299 tests run: 9299 passed, 56 skipped` (94.8 s) |

## Gate

`GATE_RESULT=pass`; the held-temp-path step ran; `9299 tests run: 9299 passed, 56 skipped`, the same count as before.
