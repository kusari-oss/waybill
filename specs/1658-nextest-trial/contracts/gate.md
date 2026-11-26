# Contract: the pre-PR gate under nextest

## C1. `scripts/pre-pr.sh` steps, in order

1. `python3 scripts/check-homoglyphs.py`
2. `bash scripts/check-walker-audit.sh`
3. `cargo clippy --workspace --all-targets [features] -- -D warnings` (unchanged)
4. `cargo nextest run --workspace --no-fail-fast --color never [features]`. `--color never` is required because CI sets `CARGO_TERM_COLOR=always`, which would put ANSI codes into the `Summary` line and stop C2's anchored pattern from matching.
5. `cargo test --workspace --doc [features]`
6. On success, print `>>> all pre-PR checks passed.`

**Before step 3:** if `cargo nextest --version` fails, print `cargo-nextest is required: brew install cargo-nextest, or cargo install cargo-nextest --locked`, then exit 1. There is no fallback.

**Unchanged rules:** no `+toolchain` anywhere, and `set -euo pipefail` stays.

## C2. Gate invocation (CLAUDE.md)

```sh
./scripts/gate.sh \
  --expect '>>> all pre-PR checks passed\.' \
  --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed( \([0-9]+ slow\))?(, [0-9]+ skipped)?$' \
  prepr -- ./scripts/pre-pr.sh
```

**Pass:** the exit status is 0, and both expectations appear in the log.

## C3. Mutation tests (`release_gates.rs`)

These tests live under `release_gates.rs`'s existing file-level `#![cfg(unix)]`; no code outside it. Each test builds a scratch crate in a temp directory, runs `gate.sh` with the C2 summary expectation around `cargo nextest run --no-fail-fast --color never`, and reads `GATE_RESULT` from the log.

| Case | Crate | Expected |
|---|---|---|
| pass | 2 passing tests | `GATE_RESULT=pass` |
| failing test | 1 pass, 1 panic | `fail`, and the summary shows both tests ran (no-fail-fast) |
| non-compiling | an undefined identifier | `fail`, and no `Summary` line appears |
| zero tests | a filter that matches nothing | `fail` |
| colour forced | 2 passing tests, `CARGO_TERM_COLOR=always` | `GATE_RESULT=pass` (proves `--color never` wins) |
| nextest absent | `PATH` is only a temporary directory of links to `cargo`, `rustc` and `bash` | `pre-pr`-style check exits 1, and the message names cargo-nextest |

**Proving the tests have teeth:** delete the `--expect` from a case and its assertion must fail. Recorded in `measurements/`.

## C4. CI

**Linux, macOS and Windows:**
- the install step from research R3;
- `Tests` becomes the C1 step 4 command;
- a new `Doctests` step with the C1 step 5 command;
- Windows keeps `continue-on-error: true` on both test steps.

**The eBPF lane:** gets the install step only. Its test command is unchanged (FR-010).

## C5. `.config/nextest.toml`

- **Contents:** a default profile with no retries, and `fail-fast = false`.
- **Test groups:** only for tests with a reproduced nextest-only failure. Each group carries a comment naming the failure and the measurement file.
