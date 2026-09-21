# Teeth-check (T022)

Which tests are **proofs** (they fail against the pre-change behaviour, so
they demonstrate the change) and which are **guards** (they pass on both
sides, so they protect against regression but prove nothing about the flip).

Done by mutating the production wiring in place rather than running against
the preserved pre-change binary. Several of these tests cannot compile on
`main` at all — `--no-enrich-batch`, `BATCH_TRIP_SINK` and the circuit breaker
do not exist there — so "fails on main" would have been indistinguishable from
"does not build on main". Mutation isolates the behaviour under test.

## Mutations applied

| # | mutation | models |
|---|---|---|
| M1 | `.with_batch(!args.no_enrich_batch)` → `.with_batch(args.enrich_batch)` | the pre-change default (opt-in only) |
| M2 | `.with_batch(...)` → `.with_batch(false)` | wiring severed entirely |
| M3 | the `if !batch_circuit_open { continue }` skip → `if false` | breaker never wired in |

## Results

| test | M1 | M2 | M3 | verdict |
|---|---|---|---|---|
| `enrichment_is_batched_by_default_m923` | **FAIL** | **FAIL** | pass | **proof** of the flip |
| `legacy_enrich_batch_flag_is_still_accepted_m923` | pass | **FAIL** | pass | **proof** the legacy flag lands on the batched path |
| `no_enrich_batch_selects_the_per_component_path_m923` | pass | pass | pass | **guard** |
| `a_persistent_batch_failure_is_attempted_exactly_once` (T013) | pass | pass | **FAIL** (3 vs 1) | **proof** of the breaker |
| `the_attempt_bound_does_not_grow_with_repository_size` (SC-005c) | pass | pass | **FAIL** (3 vs 1) | **proof** the bound is size-independent |
| `a_trip_says_so_in_the_log` (T016) | pass | pass | **FAIL** (3 vs 1) | **proof** the trip is announced once |
| `a_trip_still_reports_the_degradation` | pass | pass | pass | **guard** |
| `both_paths_produce_the_same_content_when_batch_succeeds` (T003) | pass | pass | pass | **guard** — and the right kind: it is the equivalence the flip rests on |
| `offline_output_is_unaffected_by_the_enrichment_path_m923` (T009) | pass | pass | pass | **guard** — by construction; offline runs no enrichment |

`no_enrich_batch_selects_the_per_component_path_m923` is a guard because it
asserts a *negative* — every mutation above also makes the path
non-batched, so it cannot distinguish them. It still earns its place: the
opt-out is the fallback the default's whole safety argument rests on, and a
change that broke it would be caught here.

## Two defects this check found

### T008/T010 were testing clap, not the flip

As first written, all three m923 CLI tests asserted on the **parsed flag**
(`!args.no_enrich_batch`) and never on the wiring. They passed under M1 — the
pre-change default — and under M2, with the source hard-coded to the
per-component path. The headline behaviour of this entire feature had no test.

The test's own comment asserted the missing link — *"`with_batch()` receives
`!no_enrich_batch`, so this IS the selection"* — which is a claim in prose,
not in code. A comment cannot fail.

Fixed by extracting `build_deps_dev_source(client, offline, args)` from the
call site and asserting on the constructed source via a `#[cfg(test)]`
`is_batched()`. Both mutations now fail, as recorded above.

### T009 was marked complete and did not exist

`waybill-cli/tests/enrich_default.rs` was checked off in `tasks.md` but was
never committed; SC-004 had been verified once by hand and left with no
standing test. Written now.

Its premise also needed correcting. The two offline documents from the manual
check are **not** byte-identical — same length, different SHA-256. Diffed
structurally, exactly two leaves differ:

```
/serialNumber        urn:uuid:b7cff753-...  vs  urn:uuid:37544949-...
/metadata/timestamp  2026-09-20T23:54:03Z   vs  2026-09-20T23:54:05Z
```

Both are per-run by design, out of a 4.2 MB document. So SC-004 holds, but as
"identical after masking the two nondeterministic fields", not as
"byte-identical" — an unmasked comparison of two runs fails always and the
criterion as literally worded is unsatisfiable.

The committed test therefore compares the two *flag settings* of the current
binary rather than current-against-preserved: the preserved binary is a local
artifact that does not exist in CI, and a test written against it would skip
there — reporting success having compared nothing, which is the #918 failure
mode this project has already been bitten by once.

The comparison is demonstrably sensitive: the first version of the test used a
fresh tempdir per run, the scan-root path appears throughout the document, and
it failed. That was the test's bug, not the product's, and it is also proof
that the mask is not swallowing real differences.

## T026 — corpus goldens

**No movement attributable to this change.** The harness hard-codes
`--offline` (`corpus_harness_195/harness.rs:184`), so enrichment never runs
there and the default flip cannot reach it.

Verified by comparison rather than by assertion: the suite was run on this
branch and on an `origin/main` worktree at `a7ecd7c6`, and produced the
**identical** result both times — 22 passed, 8 failed, the same eight targets
(`go_cobra`, `image_postgres16`, `maven_guice`, `npm_express`,
`pants_example_golang`, `pants_example_javascript`, `python_flask`,
`rust_ripgrep`).

Those eight are the known local-vs-CI divergence, not a regression: they fail
on the `graph-completeness` invariant with `observed: complete, expected:
partial`, which is what a warm local module cache produces — edges resolve
locally that a clean CI runner cannot resolve. The lane is green in CI on
`main` (run of 2026-09-20 06:37). This is why goldens are CI-generated and
never local.

The authoritative check is the lane run against this branch, dispatched with
`--ref 923-enrich-batch-default` (omitting `--ref` takes the workflow file
from `main` against a branch checkout, which produced a false green in #918).
