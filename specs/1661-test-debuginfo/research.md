# Research: spec 1661

## R1. Scoping a debug-info setting to macOS (FR-002)

**Observed in the repository:**
- there is no `.cargo/config.toml`;
- nothing sets `RUSTFLAGS` or `CARGO_PROFILE_*`, in any workflow or script;
- the only profile table is `waybill-ebpf/Cargo.toml`'s `[profile.release]`.

**Cargo profiles can't be scoped by target.** `[profile.dev] split-debuginfo = …` in `Cargo.toml`, or in `.cargo/config.toml`, applies on every host. There's no `[target.…]` key for profile settings. This is from Cargo's documented profile and config schema, and is checked in T002 rather than assumed.

**Decision:** a new `.cargo/config.toml` containing:

```toml
[target.'cfg(target_os = "macos")']
rustflags = ["-C", "split-debuginfo=packed"]
```

This hands rustc the same flag the profile setting would, but only when the target is macOS. Linux and Windows builds, and the eBPF build, are unaffected.

**Behaviour to verify (T002–T004), not assume:**
- **`--target`:** when `--target` is given, `target.<cfg>` rustflags apply to that target only; without it they apply to every artifact. Either way, a non-macOS target is untouched. The eBPF build uses `--target bpfel-unknown-none`, so it doesn't match.
- **Precedence:** cargo's documented rustflags order is `CARGO_ENCODED_RUSTFLAGS`, then `RUSTFLAGS`, then `target.<triple>` or `target.<cfg>`, then `build.rustflags`. A contributor who sets `RUSTFLAGS` overrides this. The guidance says so.
- **Release builds:** they set no debug info by default, so `split-debuginfo` has nothing to split there and the shipped binary is unchanged. T004 checks that a release build's flags and artifacts are unaffected.

**Alternatives considered:**
- **`[profile.dev] split-debuginfo = "packed"` in `Cargo.toml`:** applies on Linux and Windows too, which FR-002 forbids.
- **`CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=packed` exported by `pre-pr.sh` on Darwin:** only fixes `pre-pr.sh`. Plain `cargo test` and editor builds would keep accumulating.

## R2. Does the rustflags route behave like the measured profile setting? (FR-003)

The earlier measurement used the `CARGO_PROFILE_*_SPLIT_DEBUGINFO=packed` environment variables. The rustflags route reaches rustc differently. Inference, not verified: rustc runs `dsymutil` itself when `split-debuginfo=packed` is set on macOS, so the result should be the same.

**Plan:** re-measure through `.cargo/config.toml` in a fresh target directory, 3 rebuilds:
- entry count;
- min/median/max time;
- a panicking test showing file and line (T003).

## R3. The backtrace positive control (FR-003, FR-004)

**Decision:** a permanent test, `waybill-cli/tests/backtrace_resolves_source.rs`.
- **Parent mode:** it re-runs its own executable (`std::env::current_exe()`) with `RUST_BACKTRACE=1` and an environment flag, filtered to itself.
- **Child mode:** with the flag set, it panics.
- **The assertion:** the child's stderr contains this test file's name followed by a line number (`backtrace_resolves_source.rs:<n>`).

So any build configuration that loses file and line in backtraces fails the suite. That makes it both the macOS positive control and the CI backtrace column. In a CI run with no debug info it's *expected* to fail, and that failure is the measurement.

**Unix and Windows:** backtrace symbolication works on all three. The test asserts only the file name and a line number, which is format-independent.

## R4. CI measurement method (FR-004)

**Mechanism:** each setting is a temporary commit on the PR branch, setting it in the CI workflow's top-level `env`:
- **Full:** today's setting, nothing added.
- **Line tables:** `CARGO_PROFILE_DEV_DEBUG: line-tables-only` and `CARGO_PROFILE_TEST_DEBUG: line-tables-only`.
- **None:** both set to `0`.

Each commit gets 2 runs: its own run plus one re-run.

**Measurement:** for Linux, macOS and Windows, using `specs/1658-nextest-trial/measurements/ci_steps.py` and the job logs:
- the `Tests` step's seconds;
- the test build's `Finished \`test\` profile … in` time;
- R3's test result, and the backtrace text it captured.

**Decision rule (FR-005):** adopt a setting only if both of these hold:
- its 2 test-build times are both below both of full's, on Linux and macOS;
- R3 passes, with file and line present.

**After measuring:** the measurement commits are reverted, leaving only the adopted setting, if any. The numbers go in `measurements/ci.md` either way.

**The eBPF lane:** if a setting is adopted, it inherits the workflow-level `env`. In that case the eBPF lane is checked in the same run, so that it's verified rather than assumed (spec edge case).

## R5. Guidance (FR-006)

- **CLAUDE.md:** no line currently prescribes `cargo clean` for speed. The prescription lives in the memory notes `keep-target-dir-clean.md` and `project_prepr_slow_investigation.md`. Update both to name the cause (unpacked debug info leaving `*.rcgu.o` files) and the fix (`.cargo/config.toml`). Keep `cargo clean` only for the one-time cleanup of an already-grown directory.
- **CLAUDE.md** gets a short note under the Pre-PR section explaining what `.cargo/config.toml` does and that `RUSTFLAGS` overrides it.
