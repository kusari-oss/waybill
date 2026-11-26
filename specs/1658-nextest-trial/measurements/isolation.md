# Isolation findings (T005, T006)

Three full runs under each runner on the same tree (`runs.tsv`, logs `cargo-test-{1,2,3}.log`, `nextest-{1,2,3}.log`, gitignored).

## Counts (FR-011)

- **Totals:** `cargo test` runs 2 and 3, and all three nextest runs: 9,294 passed, 56 ignored or skipped. Doctests: 0 passed, 1 ignored (`doctests-1.log`); nextest does not run them.
- **Per binary:** joined with `count.py --per-binary`, every binary's passed count is identical. The only difference: 7 binaries with 0 tests appear in `cargo test` and not in nextest, which doesn't list a binary with nothing to run: `bin:xtask`, `compiler_pipeline_stdin`, `exclude_path_perf`, `go_transitive_edges`, `scan_image_podman_source`, `scan_windows_smoke`, `triple_format_perf`.

## Tests that failed under one runner only

**Nextest-only failures: none** in 3 runs. No test group is needed (SC-005).

**`cargo test`-only: one, and not caused by nextest.** `gradle_ladder::sc005_subprocess_timeout_degrades_gracefully` failed in `cargo test` run 1 of 3:

> scan MUST complete within 10s of --gradle-timeout-secs 3 … took 15.1041355s

15.1 s is the mock subprocess's full 15 s sleep, so in that run the `--gradle-timeout-secs` timeout did not end the subprocess. It passed in the other 5 runs (2 `cargo test`, 3 nextest). It's an intermittent defect in the old runner's run, out of scope here; filed as a side finding.
