# Data model: spec 1658

No runtime data. The measurement records live in `measurements/`.

## Runner comparison record

One row per run, in `measurements/runs.tsv`.

**Fields:** runner (`cargo-test` or `nextest`), host, commit, run number, wall time in seconds, summed test time in seconds, passed, failed, ignored or skipped, doctests passed.

**Validation:**
- Each runner has 3 rows per host.
- For the same commit, passed plus ignored is equal across runners. Any difference is explained in `isolation.md` (FR-011).

## Isolation finding

One per test that fails under only one runner, in `measurements/isolation.md`.

**Fields:** test path, the runner it fails under, the reproduction command, the cause, and the resolution: a code fix, or a test group with its name.

**Rule:** a group entry must reference a reproduced failure (SC-005).

## CI timing record

One row per CI run per lane, in `measurements/ci.tsv`.

**Fields:** run id, lane, before or after, `Tests` step seconds, `Doctests` step seconds (after only).
