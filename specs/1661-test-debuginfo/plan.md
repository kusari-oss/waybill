# Implementation Plan: Debug info in test builds

**Branch**: `1661-test-debuginfo` | **Date**: 2026-11-26 | **Spec**: [spec.md](spec.md)

## Summary

1. **macOS:** add `.cargo/config.toml` with `[target.'cfg(target_os = "macos")'] rustflags = ["-C", "split-debuginfo=packed"]`. This stops the leftover object files, keeps full debug info, and leaves Linux, Windows and eBPF untouched (research R1). Re-measure it through this route (R2).
2. **A permanent test** checks that backtraces resolve file and line (R3).
3. **CI:** measure full debug info, line-tables-only and none, 2 runs each, through temporary workflow-`env` commits. Adopt a setting only by rule (R4).
4. **Guidance:** update CLAUDE.md and the two memory notes (R5).

## Technical Context

**Toolchain:**
- **Language:** Rust stable 1.98.1, pinned. No `+toolchain`.
- **Dependencies:** none new.

**Changes:**
- **Build configuration:** one new file, `.cargo/config.toml`.
- **Tests:** one new test file.
- **Workflow:** possibly one `env` addition to the CI workflow, if a CI setting is adopted.
- **Production code and release builds:** unchanged.

## Constitution Check

| Rule | Status |
|---|---|
| I. Pure Rust, statically linked | Pass. Release linkage is unchanged; R1 is checked in T004. |
| Pre-PR gate (4.1.0) | Unchanged. The gate commands are the same; only the macOS debug-info packaging differs. |
| "No `+toolchain`" (#1189) | Pass. `.cargo/config.toml` sets rustflags, not a toolchain. |
| Zero new Cargo dependencies | Pass. |

## Project Structure

```text
.cargo/config.toml                              # new (macOS-only rustflags)
waybill-cli/tests/backtrace_resolves_source.rs  # new (file:line positive control)
.github/workflows/ci.yml                        # temporary measurement env; final env only if adopted
CLAUDE.md                                       # Pre-PR note
specs/1661-test-debuginfo/measurements/{local.md, local.tsv, ci.md, ci.tsv}
```

The memory notes (`keep-target-dir-clean.md`, `project_prepr_slow_investigation.md`) are updated outside the repository.

## Phases

1. Check that profiles can't be scoped by target (R1). Write `.cargo/config.toml` and the backtrace test.
2. **Local:** take 3 rebuilds in a fresh target directory (entry count and time), run the backtrace test, check that Linux is unaffected (simulated by printing the effective rustflags with `--target x86_64-unknown-linux-gnu`), and check the release build.
3. Run the gate, then open the PR.
4. **CI:** run 3 settings × 2 runs, decide, revert the measurement commits, and keep the final state.
5. Update the guidance, then re-check the gate and CI on the final head.

## Complexity Tracking

None.
