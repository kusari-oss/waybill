# T040: the cost of reading Python virtual environments

**Fixture.** `proj/.venv/lib/python3.12/site-packages/` with 2,000 package
directories of 25 empty files each (50,000 files), plus a `pyproject.toml`.
There is no `.pc` file. Debug builds of the branch were used.

**Arms.**
- `with`: the reader registers `descend_into: [".venv", "venv"]`, so the
  shared walker visits every file in the environment.
- `probe`: no `descend_into`. `venv_candidates` lists directories up to 3
  levels below each `.venv`/`venv` site-packages directory, and reads only
  `<lib*>/pkgconfig/` and `<lib*>/cmake/`.
- `without`: neither; the environment is not read at all. This is the
  baseline.

## First attempt: invalid, kept for the record

3 runs per arm, run arm by arm (`with` ×3, then `without` ×3), with no
warm-up:

| Arm | Runs (s) |
|---|---|
| with | 3.295, 3.553, 3.440 |
| without | 2.460, 2.167, 2.384 |

That reads as +44%, but the two arms ran under different conditions. A
later run showed a binary's first execution taking 2.44 s against 0.62 s
for the runs after it (macOS first-exec cost). Running the arms one after
the other without a warm-up confounded the binary with warm-up state.

## Measurement

Each binary got one discarded warm-up run. Then came 5 rounds,
interleaved, alternating the arm order each round:

| Arm | Min | Median | Max | vs `without` (median) |
|---|---|---|---|---|
| with | 0.871 | 0.924 | 0.928 | +68% |
| probe | 0.578 | 0.590 | 0.612 | +7.5% |
| without | 0.529 | 0.549 | 0.552 | — |

**Decision.** `with` exceeds the task's 20% threshold, and `probe` does
not. The reader therefore uses the bounded probe (`installed_lib::venv_candidates`),
not `descend_into`. US1's `.venv` wheel fixture still yields its component
with the probe (`installed_lib_us1`, all 5 tests pass).
