# Research: spec 1658, nextest trial

Probes ran on 2026-11-25, macOS, against cargo-nextest 0.9.152 installed by Homebrew. The scratch crate was `nxprobe`: two tests, one ignored test and one doctest.

## R1. Nextest's positive signal

**Observed:**

| Case | Output |
|---|---|
| All pass | `     Summary [   0.007s] 2 tests run: 2 passed, 1 skipped` |
| One fails | `     Summary [   0.007s] 2 tests run: 1 passed, 1 failed, 1 skipped`, then `error: test run failed` |
| Does not compile | No `Summary` line; `error: could not compile …`; cargo exits with 101 |
| Filter matches nothing | `0 tests run: 0 passed, 3 skipped`, then `error: no tests to run` |

**Decision:** the gate expects a nextest summary that names no failure:

```
^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed(, [0-9]+ skipped)?$
```

- **Anchoring:** the line ends right after `passed` or `skipped`. A run that also failed, timed out, was flaky or leaked writes `, N failed` (or similar) and does not match.
- **`[1-9]`:** zero tests run does not count as a pass.

The existing `>>> all pre-PR checks passed.` expectation stays. The gate is given both with `--expect`, which is repeatable.

**Rationale:** CLAUDE.md requires a positive signal that cannot appear unless the tests ran and passed. A non-compiling build prints no `Summary` line, so the expectation fails even if an exit status were lost.

**Alternatives considered:** reading nextest's JUnit output. It's a heavier mechanism for the same information, and `gate.sh` already works on log lines.

## R2. Doctests

**Observed:** nextest does not run doctests. `cargo test --doc` ran the probe's one doctest: `test result: ok. 1 passed`.

**Decision:** `pre-pr.sh` and each CI lane run `cargo test --workspace --doc` as its own step after nextest. The doctest count before and after is the sum of `test result:` lines from `--doc`, compared against the doctest targets in a `cargo test --workspace` log on `main`.

## R3. Installing nextest on CI

**Observed:** `taiki-e/install-action`'s latest release is `v2.90.10`. Its tag resolves directly to commit `893d6ee8d714b6c7dfe2bcd94e5759e9a7958a7e`. The repository has no other use of the action today.

**Decision:**

```yaml
- uses: taiki-e/install-action@893d6ee8d714b6c7dfe2bcd94e5759e9a7958a7e # v2.90.10
  with:
    tool: cargo-nextest@0.9.152
```

- **Pinning:** the action is pinned by SHA, per the Actions security defaults.
- **Version:** the tool version is pinned to the one probed locally.
- **Supply chain:** the action fetches a prebuilt binary and checks its checksum. Nothing is compiled, and no Cargo dependency is added.

**Alternatives considered:**
- `cargo install cargo-nextest --locked`: compiles for about 2 minutes on every lane, which is slower.
- A hand-written download plus checksum: duplicates what the action already does.

## R4. Constitution impact (finding)

The constitution's Pre-PR Verification table names step 2 as `cargo test --workspace --no-fail-fast`, with the passing condition "Every suite reports `ok. N passed; 0 failed`". Swapping the runner changes that contract. Under Tenet 1 and the workflow section, the contract must change first.

**Decision:** amend the constitution in the same PR, from 4.0.1 to 4.1.0. That's a MINOR bump, because the gate's command and passing condition change while no principle changes.

- **Step 2** becomes `cargo nextest run --workspace --no-fail-fast`, with the R1 summary as its passing condition.
- **A new step 3** is `cargo test --workspace --doc`.
- **CLAUDE.md's "Pre-PR verification" section** is updated to match.
- **The paragraph about not reintroducing `+toolchain`** stays as it is. Nextest invokes `cargo`, which resolves through `rust-toolchain.toml`. To verify: the run's own build lines must name 1.98.1, measured in T-baseline.

## R5. Why no-fail-fast still matters

**Observed:** with `--no-fail-fast`, both tests ran and the failure was reported at the end. Nextest's default is fail-fast, so `--no-fail-fast` is passed explicitly, as `cargo test` is today.

## R6. The eBPF lane and `release_gates.rs`

The new gate mutation tests (FR-005) run nextest on a scratch crate. Any lane that runs the workspace tests therefore needs nextest installed, or those tests fail.

**Decision:**
- **The eBPF lane** gets the same install step. It keeps its `cargo test` invocation (FR-010), and the install keeps the new tests runnable there.
- **When nextest is missing,** the tests fail with a message saying so. They don't skip, because a skipped gate test looks like a passing one.

## R7. Shared-state candidates (measure, don't assume)

- **Lazy statics:** 16 test files use `OnceLock`, `LazyLock` or `lazy_static`.
- **Environment variables:** 35 files across tests and source call `set_var` or use `EnvGuard`.

Under nextest each test runs in its own process. So:

- **(a) Env-var races disappear** between tests that used to share a process. `EnvGuard`'s mutex becomes uncontended.
- **(b) Lazy setup repeats** once per test. That's slower, but not wrong.
- **(c) Cross-binary conflicts can now occur,** because binaries run at the same time. Fixed temp paths, ports and the shared `~/.cache/waybill/*` caches are the candidates. #1718's binary-parallel run saw none in one run.

These are hypotheses for T-baseline and T-compare to settle. No test is serialised until a failure has been reproduced (SC-005).
