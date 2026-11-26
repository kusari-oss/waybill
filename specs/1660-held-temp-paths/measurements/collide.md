# Do concurrent processes get the same temporary-file name? (US1, FR-001)

**Host:** Apple M5 Max, 18 cores, macOS. **Versions:** `tempfile` 3.27.0, `fastrand` 2.4.1, the same as the workspace lockfile. The crate is in `collide/`, outside the workspace.

**Each process** creates a `NamedTempFile`, prints its path and drops it, which is the tests' shape. **The control (`--fixed-seed`)** builds the name the way `tempfile` does, from a constant seed, so every process must get the same name.

## Variant 1: `collide.sh`, processes started by bash `&` (3 runs; `collide-results.tsv`)

| mode | N | batches per run | names per run | duplicate names, runs 1 / 2 / 3 |
|---|---|---|---|---|
| real | 18 | 50 | 900 | 0 / 0 / 0 |
| real | 36 | 50 | 1,800 | 0 / 0 / 0 |
| real | 72 | 50 | 3,600 | 0 / 0 / 0 |
| control | 18, 36, 72 | 50 | as above | 50 / 50 / 50, one duplicated name per batch: every batch collides |

## Variant 2: `collide_threads.py`, N threads released by one barrier, each starting a process (`collide-threads.tsv`)

This is closer to nextest, which starts test processes from several threads at once.

| mode | N | batches | names | duplicate entries |
|---|---|---|---|---|
| real | 18 | 200 | 3,600 | 0 |
| real | 72 | 200 | 14,400 | 0 |
| control | 18 | 20 | 360 | 340 (all 18 identical in every batch) |

## Result

**0 duplicates among 36,900 real names.** The control collides every time, so the counting works.

**The inferred mechanism did not reproduce on this host.** The seed includes `Instant::now()`. On this host, processes started together still read the clock at different ticks, presumably because process start-up varies by microseconds and the clock ticks far faster. So they got different seeds.

**Scope (analysis U1):** this tests `tempfile`'s naming under concurrent process starts, not nextest's own launcher or a CI runner. A 0 here doesn't show that nextest can't collide, and it doesn't explain the one CI observation.

**What remains true:**
- The 23 sites drop their guard, so nothing reserves the output path. That is a code fact (research R1).
- The CI failure did read another test's SBOM (`editable-pkg@1.0.0`), and an unreserved output path is the only resource the two tests' scans shared.

But **how two processes got the same name is unconfirmed.** The fix is justified on the code fact. It is not claimed to be the proven cause of that failure.
