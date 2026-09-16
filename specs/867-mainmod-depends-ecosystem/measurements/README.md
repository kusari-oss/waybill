# Measurements

Every figure in `../spec.md` and `../plan.md` comes from here. Commands are
recorded so each can be re-run when behaviour changes.

## Environment

`bitwarden/android` @ `d817f6b4bf7c17172a74fabca1e09e738c7ec6c9`, scanned the
way the m770 quality harness scans it — `--offline`, CycloneDX, and
**`--root-name`**, which is load-bearing (see Traps in `../quickstart.md`).

Pre-change reference: `target/release/waybill-baseline`, built from the
merge-base `d9c4f21e` before any code change on this branch (T001).

## T002 / T013 — the defect and the fix

| | baseline | after gem adopts |
|---|---:|---:|
| main-module outgoing edges | **0** | **9** |
| total edges | 141 | 150 |
| gem components with no incoming edge | 2 | **0** |
| components reachable from root | **1** of 347 | **104** of 347 |
| max depth from root | 1 | **6** |
| reports itself flat | YES | **no** |

The 9 are exactly the `Gemfile.lock` `DEPENDENCIES` block: `abbrev`, `csv`,
`fastlane`, `fastlane-plugin-firebase_app_distribution`, `logger`, `mutex_m`,
`nkf`, `ostruct`, `time`. SC-001 and SC-002 met.

The document still reports `partial`, correctly: its 233 maven components come
from a `gradle.lockfile`, whose format (`group:artifact:version=configuration`)
carries no parent-child topology at all — flat by construction, like `go.sum`.
That is a separate matter and not something this feature claims to fix.

## T014 — FR-008 / D-6: the inference flag moves nothing

Same scan with `--experimental-cross-ecosystem-edges`: **dependency graph
identical**. The default path now resolves before the fallback is reached, so
the opt-in capability has nothing left to do here — which is the intended
outcome, asserted rather than assumed.

## T015 — performance

Research R1 predicted no material cost on structural grounds: the change
substitutes a value into the existing lookup key, so it is still exactly one
`HashMap::get` per declared dependency.

A first attempt measured 618/616/623 ms against a 600 ms baseline recorded
earlier in the session — a ~3% gap, larger than either run's spread, so it
could not honestly be waved away as noise. Re-measured **interleaved on
identical machine state**:

| round | baseline | after |
|---|---:|---:|
| 1 | 659 ms | 596 ms |
| 2 | 598 ms | 598 ms |
| 3 | 598 ms | 593 ms |

Indistinguishable, and if anything marginally faster. The earlier gap was
machine-state drift between two separately-taken measurements, not a
regression.

**Method note worth keeping**: comparing a fresh measurement against a
baseline taken at a different time attributes drift to the change. Interleave
the two binaries in one loop.

## Teeth-checks

Every check here was observed failing before it was trusted (SC-007).

| Check | Observed failure |
|---|---|
| `m867_application_main_module_records_the_gem_ecosystem` (T011) | `left: None, right: Some("gem")` against the unpatched reader |
| `m867_absent_recording_falls_back_to_requirer_ecosystem` (D-2) | see below |
| `m867_normalisation_follows_the_lookup_ecosystem` (D-3) | see below |

The D-2 and D-3 unit tests are pure-function assertions on
`dep_lookup_ecosystem` / `normalize_dep_name` and cannot fail against a build
that lacks the helper — it does not compile. Their teeth come from the
Phase-2 output-neutrality gate (T008) instead: the whole workspace golden
suite is the assertion that `None` behaves exactly as before, and it passed
with **zero churn** across 298 binaries.

## A cost the plan did not anticipate

`PackageDbEntry` has **141 literal construction sites**, none using
struct-update syntax, and no `Default` impl. Adding one field therefore
touched all of them. This is mechanically safe — a missed site is a compile
error, not a silent bug — but it makes the diff wide, and it is the same
shape of problem already tracked for a 31-parameter constructor elsewhere in
this repo.

## T031 — the quality-corpus bound, re-authored from CI

Measured on CI (run `35032804554`), not locally:

```
gradle-bitwarden-android  pkgs=338 files=9 edges=150 depth=6 flat=false sbomqs=6.60
```

| bound | before | after | moved? |
|---|---|---|---|
| `edges` | 346..424 | **135..165** | yes — see below |
| `max_depth` | 4..9 | 4..9 | no: 6 is back inside it on its own |
| `flat` | false | false | no: now genuinely false |
| `pkgs` / `files` / `sbomqs` | — | — | no: never left range |

Only the violating bound moved. A passing bound moved without cause is the
drift this file exists to catch.

**Why 150 and not 385.** The old figure counted the CycloneDX
primary-dependency fallback attaching the root to all 233 maven components,
because the gem main module had no outgoing edges and therefore neither did
the root. 150 is the real graph: 140 gem→gem transitive edges + 9
main-module→gem declared edges recovered here + 1 root→main-module anchor.

The 233 maven components still carry no edges, **correctly** — they come from
a `gradle.lockfile`, whose format (`group:artifact:version=configuration`)
carries no parent-child topology at all. Flat by construction, like `go.sum`.
A separate gap, not one this feature claims to close.

## T033 — walker audit

Zero `fn walk[_(]` lines added or removed under `waybill-cli/src/scan_fs/` on
this branch, and `walk.audit-allowlist.txt` is untouched. The audit set is
byte-identical to `main`, so the gate cannot trip on this change.

Worth recording: a hand-rolled local replication of that CI grep reported 35
live entries against a 12-line allowlist and looked like catastrophic drift.
It was wrong — the CI check strips leading indentation via `sed` and the
replication did not. Diffing the branch against `main` for `fn walk` lines is
the reliable instrument; re-implementing the gate is not.
