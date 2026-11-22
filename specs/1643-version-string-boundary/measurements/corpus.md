# T017: public corpus (SC-005)

Each run is `public-corpus.yml` with `-f branch=1643-version-string-boundary -f regen_goldens=true`, and its job list is `Run public corpus` succeeded and `Report corpus failure` skipped (scheduled runs only). Regenerated goldens were compared with the committed ones using `cmp`.

| run | scanned commit (from the run summary) | what it is | goldens differing |
|---|---|---|---|
| 44990811702 | `83657af2` | **not this branch**: dispatched without `-f branch=`, so it scanned `main` | 0 of 66 (says nothing about the branch) |
| 44992685827 | `9f0366dd` | log-only commit plus a CHANGELOG-only commit | 0 of 66 |
| 44995035262 | `4caee6a1` | the rule wired | 0 of 66 |
| 45000671526 | `1586d74b` | the rule wired, with the SC-006 fix | 0 of 66 |

**No public-corpus target changes.** That is consistent with the census: its only verdict changes were on the TheRock SDK. The public corpus's diff at the final commit is its census (SC-005 as amended), and it is empty.

The quality corpus and TheRock are in `therock.md`.
