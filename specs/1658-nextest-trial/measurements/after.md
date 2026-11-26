# After: spec 1658

## Local (Apple M5 Max, 18 cores, same tree, 3 runs each; `runs.tsv`, `wall.tsv`)

| | min / median / max |
|---|---|
| `cargo test --workspace --no-fail-fast` | 334.1 / 351.6 / 353.7 s |
| `cargo nextest run --workspace --no-fail-fast --color never` | 123.0 / 124.4 / 126.0 s |
| `pre-pr.sh` before (clippy + `cargo test`) | 369.7 / 382.7 / 399.1 s |
| `pre-pr.sh` after (clippy + nextest + doctests) | 142.6 / 143.0 / 148.8 s |

The test run is 2.83× faster, and `pre-pr.sh` is 2.68× faster (medians).

## CI, `Tests` step (`ci.tsv`)

| lane | before (45695514065, 45713592591) | after (45728714707, 45731418569) |
|---|---|---|
| Linux | 683, 842 s | 551, 676 s |
| macOS | 1,135, 1,059 s | 976, 942 s |
| Windows (non-blocking) | 1,146, 876 s | 887, 1,068 s |

- **Linux and macOS:** both after runs are below both before runs. The means fall 20% (Linux, 762 → 614 s) and 12% (macOS, 1,097 → 959 s).
- **Windows:** the runs overlap, so there is no measurable change.
- **The CI gain is far smaller than the local one.** Compile time is inside the step (about 3 min on Linux, about 7 on macOS, from the first after run's `Finished` lines). The runners have 3 to 4 cores, so the parallelism has little room.
- **The new `Doctests` step** takes 1 to 3 s.

## Counts and isolation

- **Locally:** identical counts under both runners (9,294 passed, 56 ignored), and the same pass count per binary.
- **Failures under only one runner:** none under nextest in 3 local runs or on any CI lane. On Windows, the failing set is identical to `main`'s (`isolation.md`).
- **Shared-state binaries** pay more under nextest (median ratio 1.66 against 1.20 for the control), but none sets the suite's wall time (`lazy-state.md`).
- **Side findings:** one intermittent failure under `cargo test`, filed as #1723. The Linux floor is `sbom_edit_filter::i1454` at 115 s (`decision.md`).

## The gate

- **Tests:** 8 tests in `release_gates.rs`, each shown to fail under its mutation (`gate-mutations.md`, M1 to M4).
- **M4** came from CI's first run. The first version of the pattern rejected nextest's `(1 slow)` annotation, which would have failed a fully green local run whenever a test passed 60 s.
- **Final local gate:** `GATE_RESULT=pass`, `9302 tests run: 9302 passed, 56 skipped`, and doctests 0 passed with 1 ignored.

## #1716

Its tests aren't the floor on any lane, so it stays filed (`decision.md`).
