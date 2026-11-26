# Local measurements: spec 1661

Host: Apple M5 Max, 18 cores, macOS. Toolchain 1.98.1.

## T002: how to scope the setting to macOS (research R1)

**(a) A profile setting under a target table is silently ignored.** Both spellings were tried in `.cargo/config.toml`:
- `[target.'cfg(target_os = "macos")'] profile.dev.split-debuginfo = "packed"`
- `[target.'cfg(target_os = "macos")'.profile.dev] split-debuginfo = "packed"`

Cargo accepted both with no warning, and `rustc` still received `-C split-debuginfo=unpacked`, read from `cargo build -p waybill-common -v`.

**(b) `[target.'cfg(target_os = "macos")'] rustflags = ["-C", "split-debuginfo=packed"]` reaches rustc.** 35 `rustc` lines carried the flag in a `waybill-common` build. Each of them carries both flags, the profile's first and then ours: `-C debuginfo=2 -C split-debuginfo=unpacked -C split-debuginfo=packed`. Whether the later one takes effect is shown by T005's entry counts, not assumed.

**(c) It does not reach another target.** `cargo check -p waybill-common -v --target x86_64-unknown-linux-gnu`, after touching the crate: the one `rustc` line for that target has 0 occurrences of `split-debuginfo=packed`. The `x86_64-unknown-linux-gnu` standard library was already installed, so nothing was added. Host artifacts in that cross build, such as build scripts, also stayed `unpacked`: target rustflags don't apply to host artifacts when `--target` is given.

## T004: release builds are unaffected

`cargo build --release -p waybill -v` with the config: the `waybill` `rustc` lines carry `-C opt-level=3` and `-C split-debuginfo=packed`, and no `-C debuginfo` flag. Release has no debug info, so there is nothing to split. The release binary is 29,686,480 bytes both with and without `.cargo/config.toml`.

## T005: rebuilds with the config (fresh target dir, workspace-only rebuild after touching `lib.rs`/`main.rs`)

| series | rebuild 1 | rebuild 2 | rebuild 3 | `target/debug/deps` entries | `*.rcgu.o` |
|---|---|---|---|---|---|
| 1 | 44.2 s | 43.6 s | 50.5 s | 2,461 every time | 0 |
| 2 | 40.1 s | 39.8 s | 43.1 s | 2,461 every time | 0 |
| default, earlier (#1719) | 29.3 s | 33.1 s | 42.2 s | 65,923 → 97,032 → 128,141 | about 31,100 more per rebuild |

- **SC-001 met:** the entry count is identical across rebuilds, with no `.rcgu.o` left behind. The later `-C split-debuginfo=packed` takes effect over the profile's `unpacked`.
- **SC-002 not met as written:** in both series the 3rd rebuild is slower than the 1st (+14%, +7.5%), beyond the spread between rebuilds 1 and 2 (0.6 s, 0.3 s). It is not accumulation, since the entry count is flat. The cause is unknown; the build saturates the machine (load average 23–33 on 18 cores).
- **Cost:** packed rebuilds take 40–44 s against the default's 29.3 s first rebuild after a clean, because `dsymutil` bundles debug info for each of the ~430 test binaries. The default overtakes packed at about the 3rd rebuild after `cargo clean` and keeps climbing; packed doesn't. That is a trade, not a clear speed win: a higher fixed cost per rebuild, in exchange for no growth and no need to `cargo clean`. Earlier estimates said "within 2 rebuilds", based on 35.5 s; these two series say about 3.

## T006: the backtrace test's controls

**The first version could not fail.** With `CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0` it still passed. Its pattern matched the panic message, `panicked at waybill-cli/tests/backtrace_resolves_source.rs:20:9`, which comes from the panic location and is present without debug info.
- **Without debug info:** the frames carry only function names.
- **With debug info:** each frame has `at ./waybill-cli/tests/backtrace_resolves_source.rs:20:9`.

**Fixed** to match a frame's `at <file>:<line>` line. Controls after the fix:

| build | result |
|---|---|
| default, with the config | PASS |
| no debug info | **FAIL**, "the backtrace does not name this file and a line" |
| line-tables-only | PASS |
