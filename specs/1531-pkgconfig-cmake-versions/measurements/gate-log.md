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

## T038: full gate on the branch

- **`1b40d613`…`b6a47259`: `GATE_RESULT=fail`.** Clippy `type_complexity` in
  `installed_lib/resolve.rs`, fixed with a named type.
- **Re-run: `GATE_RESULT=fail`, 397 `test result: ok.` lines, 2 failures.**
  Both were real and both came from this branch:
  - `installed_lib_cmake_reader_inert`. T005's filter matched mechanisms
    starting with `cmake-`, and this reader's own `cmake-package-version`
    now matches that prefix. The filter now excludes it.
  - `public_corpus::…::ecosystem_id_and_the_coverage_matrix_agree`. A row
    added to `docs/ecosystems.md`'s coverage matrix had no `EcosystemId`.
    No C/C++ reader has a row there, so the row became a section instead.
- **At `ab6f2a0a`: `GATE_EXIT=0`, `GATE_RESULT=pass`, 399 `test result: ok.`
  lines, 0 `---- … stdout ----` failure blocks.** The log contains
  `>>> all pre-PR checks passed.`
