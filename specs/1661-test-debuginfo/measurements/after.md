# After: spec 1661

| | result |
|---|---|
| **US1, macOS packing** | `.cargo/config.toml` (macOS-only rustflags). `target/debug/deps` holds at 2,461 entries across rebuilds, with 0 `*.rcgu.o` (SC-001 met). Rebuilds take 40–44 s against the default's 29 s first rebuild after a clean, and the default overtakes packed at about the 3rd rebuild. In both series the 3rd rebuild was 7–14% above the 1st, unexplained and not accumulation, so SC-002 is **not met as written**. Kept by decision. The release binary is unchanged. (`local.md`) |
| **Backtrace control** | `backtrace_resolves_source` passes with full or line-tables-only debug info and fails with none, locally and on all three CI lanes (SC-003). The first version could not fail, because it matched the panic message; fixed. |
| **US2, CI** | Measured full, line-tables-only and none, 2 runs each. **Nothing adopted**, CI unchanged: no debug info loses file and line, and line-tables-only wasn't faster on Linux in both runs. Runner noise of 38–51% for a single setting exceeded any setting's effect (SC-004). (`ci.md`) |
| **US3, guidance** | CLAUDE.md has a section on the config file. The two memory notes now name the cause and say `cargo clean` is needed less often, not never: version bumps still leave stale binary generations. |
