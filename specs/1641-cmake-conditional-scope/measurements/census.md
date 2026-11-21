# T007: census at the log-only commit (SC-001)

## Runs

**Census:** run 44950692611 of `census-conditions.yml` in the TheRock SDK fixture repository (commit 3a39913), dispatched with `-f waybill_ref=5ae18211d93020686e825d27416408f6bbcc9f10`. It has four jobs, all successful. Each job's `refs.txt` names waybill 5ae18211:

| target | tree |
|---|---|
| cmake-nlohmann-json | 55f93686c01528224f448c19128836e7df245f72 |
| cpp-mongo | d100bf19961251f273e1d0bd8ce66fc60634f53c |
| python-pytorch | 2b3ec34829036a65cd9d1398ea72a0167dc37470 |
| therock | b1cae390cc745283dce25581415353f9bc906162 |

**Public corpus:** run 44950695294, `regen_goldens=true`, scanned 5ae18211. Jobs: `Run public corpus` succeeded; `Report corpus failure` was skipped (scheduled runs only).
- All 67 regenerated goldens are byte-identical to the committed ones.
- They contain 0 occurrences of `waybill:cmake-fetch-conditions`, `waybill:cmake-find-conditions` or `cmake-conditional`. The public corpus has no catch-all conditional, so its census count is **0 by measurement**.

The positive control (T006, `census_one_event_per_category`) passes: one event per category, with the expected evaluation.

## Results

"Change scope" counts events whose chain is not *does-not-hold*. Each of them is Optional (`cmake-conditional`) at `main` and comes into scope under FR-001/FR-004. For searches, the "possibly change" figure counts names. It is an upper bound: an unconditional search emits no event, and already makes its name runtime today.
### cmake-nlohmann-json

- events: 2 (fetches 0, searches 2)
- fetches that change scope: 0 (0 versioned; 0 through unknown)
- search names: 2; possibly change: 1

| kind | category | eval | count |
|---|---|---|---|
| search | option-off | does-not-hold | 1 |
| search | unknown | unknown | 1 |

### cpp-mongo

- events: 9 (fetches 3, searches 6)
- fetches that change scope: 3 (3 versioned; 3 through unknown)
- search names: 6; possibly change: 6

| kind | category | eval | count |
|---|---|---|---|
| fetch | unknown | unknown | 3 |
| search | unknown | unknown | 6 |

### python-pytorch

- events: 67 (fetches 4, searches 63)
- fetches that change scope: 1 (0 versioned; 1 through unknown)
- search names: 46; possibly change: 29

| kind | category | eval | count |
|---|---|---|---|
| fetch | compound | does-not-hold | 3 |
| fetch | compound | unknown | 1 |
| search | compound | does-not-hold | 16 |
| search | compound | holds | 2 |
| search | compound | unknown | 14 |
| search | literal | does-not-hold | 5 |
| search | literal | unknown | 1 |
| search | option-off | does-not-hold | 3 |
| search | option-on | does-not-hold | 1 |
| search | option-on | holds | 3 |
| search | platform | holds | 1 |
| search | unknown | unknown | 17 |

### therock

- events: 1081 (fetches 69, searches 1012)
- fetches that change scope: 57 (41 versioned; 44 through unknown)
- search names: 179; possibly change: 156

| kind | category | eval | count |
|---|---|---|---|
| fetch | compound | does-not-hold | 6 |
| fetch | compound | holds | 3 |
| fetch | compound | unknown | 26 |
| fetch | literal | unknown | 1 |
| fetch | option-off | does-not-hold | 6 |
| fetch | option-on | holds | 8 |
| fetch | platform | holds | 1 |
| fetch | superbuild | holds | 1 |
| fetch | unknown | unknown | 17 |
| search | compound | does-not-hold | 150 |
| search | compound | holds | 25 |
| search | compound | unknown | 231 |
| search | literal | does-not-hold | 4 |
| search | literal | holds | 23 |
| search | literal | unknown | 17 |
| search | option-off | does-not-hold | 60 |
| search | option-off | holds | 15 |
| search | option-on | does-not-hold | 4 |
| search | option-on | holds | 69 |
| search | option-on | unknown | 1 |
| search | platform | holds | 26 |
| search | unknown | holds | 48 |
| search | unknown | unknown | 339 |


## What the numbers say

- **TheRock fetches:** 57 of 69 catch-all conditional fetches change scope, 41 of them versioned.
  - 13 change because the rule reads the condition as holding: a platform check, the super-build guard, a default-ON option, a literal, or a compound of these.
  - 44 change because the condition is unknown (FR-004).
  - 12 stay Optional: 6 under a default-OFF option, 6 compound chains that do not hold.
- **The unknown conditions are the libraries the issue is about.** The most common unknown fetch chains are TheRock's bundled system libraries:
  - `CMAKE_SYSTEM_NAME STREQUAL Linux`, `THEROCK_BUNDLE_SYSDEPS`, `THEROCK_ENABLE_SYSDEPS[_*]`, and the super-build guard: 9, 4 and 1 occurrences of three such chains;
  - `THEROCK_ENABLE_ELFIO`, `_SPDLOG`, `_RDC`, `_HOST_BLAS` and `_HOST_SUITE_SPARSE`.

  No visible `option()` gives the `THEROCK_*` names a default, so they are unknown. That they are declared through a feature macro is inference from the names, not traced. **So FR-004's choice (unknown stays in scope) is what brings these libraries back.** Under the alternative (unknown stays Optional), only 13 fetches would change and they would stay excluded.
- **Fallbacks among the unknowns:** for example `NOT DEFINED ROCPRIM_ROOT` (3), `else: GTest_FOUND`, `else: Catch2_FOUND`. They come into scope too. Where an enclosing project supplies the same repository, spec 1538 R9 still merges them (FR-006, case R-1). T021's TheRock trace checks how many remain as separate components.
- **Searches:** TheRock has 1,012 catch-all conditional search events over 179 names, of which at most 156 names change. PyTorch has 63 events over 46 names, at most 29 changing. Unknown dominates here too.
- **Quality corpus fetches:** cpp-mongo 3 change (all unknown, versioned); python-pytorch 1 of 4; nlohmann-json 0.

**The checkpoint stands.** The counts support the spec's two choices: unknown stays in scope, and searches follow the rule.

**Not measured:** how many options are defined only after their use (tasks T007, planning amendment). The census reads events, not definition positions. The amendment's rationale rests on CMake's cache semantics, not on a count.
