# Isolation findings (T005, T006)

Three full runs under each runner on the same tree (`runs.tsv`, logs `cargo-test-{1,2,3}.log`, `nextest-{1,2,3}.log`, gitignored).

## Counts (FR-011)

- **Totals:** `cargo test` runs 2 and 3, and all three nextest runs: 9,294 passed, 56 ignored or skipped. Doctests: 0 passed, 1 ignored (`doctests-1.log`); nextest does not run them.
- **Per binary:** joined with `count.py --per-binary`, every binary's passed count is identical. The only difference: 7 binaries with 0 tests appear in `cargo test` and not in nextest, which doesn't list a binary with nothing to run: `bin:xtask`, `compiler_pipeline_stdin`, `exclude_path_perf`, `go_transitive_edges`, `scan_image_podman_source`, `scan_windows_smoke`, `triple_format_perf`.

## Tests that failed under one runner only

**Nextest-only failures: none** in 3 runs. No test group is needed (SC-005).

**`cargo test`-only: one, and not caused by nextest.** `gradle_ladder::sc005_subprocess_timeout_degrades_gracefully` failed in `cargo test` run 1 of 3:

> scan MUST complete within 10s of --gradle-timeout-secs 3 … took 15.1041355s

15.1 s is the mock subprocess's full 15 s sleep, so in that run the `--gradle-timeout-secs` timeout did not end the subprocess. It passed in the other 5 runs (2 `cargo test`, 3 nextest). It's an intermittent defect in the old runner's run, out of scope here; filed as #1723.

## CI, first run on PR #1724 (run 45728714707)

**Linux and macOS:** every test passed. Linux ran 9,304 tests (local 9,301, plus 3 Linux-only tests); macOS ran 9,301.

**The summary line on Linux** read `9304 tests run: 9304 passed (1 slow), 56 skipped`. Nextest marks a test that runs past its 60 s slow threshold (`sbom_edit_filter::i1454…`, which takes about 50 s locally) and appends `(N slow)` after `passed`. The first version of the gate pattern didn't allow that, so a fully green local run would have failed the gate whenever one test was slow. Fixed: the pattern accepts `( \([0-9]+ slow\))?`, and `a_slow_test_still_passes_the_summary_gate` covers it (mutation M4 in `gate-mutations.md`). CI itself doesn't match the pattern, so CI was unaffected.

**Windows (non-blocking, #210):** 8,471 passed, 139 failed, 51 skipped. Compared with `main`'s `cargo test` on the same base (run 45713592591), by test name:
- The failing sets are identical, except `scan_image_docker_daemon::docker_daemon_source_scans_locally_cached_image_end_to_end`, which failed on `main` and passed here.
- The run counts match: `main`'s 8,612 includes the two `scan_windows_smoke` tests twice, because the job's earlier blocking `Smoke` step runs that binary on its own. Excluding it, both runs ran 8,610.
- No test failed under nextest that passed under `cargo test`.
