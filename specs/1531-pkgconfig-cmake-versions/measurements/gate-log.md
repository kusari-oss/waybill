# Gate log (spec 1531)

## T008: no-op control at `88f87c47` (OS-only claim set, before any reader code)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`
was run in a clean worktree at `88f87c47`.

**Result: `GATE_RESULT=fail`, `GATE_EXIT=101`, 0 `test result: ok.` lines.**

Clippy stopped at `-D dead-code`: `os_claimed_paths` and `os_claimed_inodes`
are never read. That is expected. The fields have no reader until the
post-pass (T019) consumes them, so a green run of this control cannot
exist before then.

The control is therefore folded into T038. The full gate on the finished
branch must show no change to any golden or fixture that holds no file this
reader matches. The OS-only set's behaviour is covered by T007's unit test,
which passed in isolation:
`os_claimed_paths_hold_os_package_claims_only ... ok`.
