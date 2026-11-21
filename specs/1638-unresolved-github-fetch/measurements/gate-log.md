# T021: pre-PR gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`, at `81b833ca`.

`target/gate/prepr.log`:
- `>>> all pre-PR checks passed.`, `GATE_EXIT=0`, `GATE_RESULT=pass`.
- Steps: homoglyph check, walker audit, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`.
- 421 `test result: ok` lines, summing to 10,131 passed and 0 failed; no `test result: FAILED`.
- Two `^error:` lines (log lines 15370 and 15373) are stderr from `--root-purl` validation tests, which assert those messages.

## PR #1648 CI (SC-005, T014's cross-lane check)

CI run 44863277491. Every job's conclusion:
- `Lint + test` on linux-x86_64, macos-latest and windows-latest: `success`;
- keyless-signing guard: `success`;
- `ci-ok`: `success`;
- ebpf lane and `main-red`: `skipped`;
- all 17 Realistic-projects scans: `success`;
- Kusari Inspector: `success`;
- pin consistency: `success`.

`u_xhost_changed_shapes_match_the_golden_on_every_host` and `u_ctl_unchanged_shapes_are_byte_identical_to_main` print `... ok` in the Linux, macOS and Windows job logs. The changed shapes are byte-identical to one golden on all three hosts.

The `review-gate` status is `failure`: "not approved: gittuf's log has no entry for pr/1648". That is the maintainer's approval step, not a defect.
