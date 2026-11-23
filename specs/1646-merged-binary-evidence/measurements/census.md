# T009: census at the log-only commit (FR-001, SC-002)

## Runs

- **Census**: run 45116219040 of `census-versions.yml` in the TheRock SDK fixture repository, at fixture commit `3d1faed`. It was dispatched with `--ref main -f waybill_ref=db322f4906f1da78a8215726a96b81533b15930c`, the log-only commit.
  - **Jobs**: 20 of 20 succeeded.
  - **Commit check**: all 20 `refs.txt` name waybill `db322f49`.
- **Public corpus**: run 45116221817 with `-f branch=1646-merged-binary-evidence`. Its summary reads `Scanned commit: db322f49…`, and it passed `166 passed; 0 failed`. The log-only commit therefore changes no public-corpus output.

## Positive controls

From the `control` job's log:
- `control ok: [('bzip2', 'versioned', '1.0.8'), ('gmp', 'versioned', '6.3.0'), ('liblzma', 'versioned', '5.8.1')]` (spec 1644);
- `identity control ok: [('ncurses', 'merge')] [] []` (spec 1645, in its wired state);
- `spec 1646 control ok: log-only ['pkg:generic/llvm@23.0.0git', 'pkg:generic/ncurses@6.6.20251230'] ['pkg:generic/llvm@23.0.0git', 'pkg:generic/ncurses@6.6.20251230', 'pkg:generic/zlib']`.

Every event kind fires on its synthetic case, so a zero in a corpus job means the event is absent, not that the probe is broken.

## Per target

Only targets with an event are listed. The other 14 quality-corpus targets have none.

| Target | `technique_change` | `binary_merge` | `binary_derived_kind` |
|---|---|---|---|
| therock-sdk | 25 events, 8 PURLs: `embedded-version-string` 20 events / 7 PURLs, `symbol-fingerprint` 5 / 1 | 23 events, all at S1: 18 absorbing `binary-analysis` into a record, and 5 binary into binary (absorbing nothing) | `embedded-source-path` 645 events / 15 PURLs; `dynamic-linkage` 49 / 49 |
| pants-backend-ai | 4 events, 2 PURLs (`embedded-version-string`) | 2 events at S1, binary into binary (absorbing nothing) | `dynamic-linkage` 8 / 8 |
| go-kubernetes | 0 | 0 | `go-buildinfo` 38 / 38 |
| cpp-mongo | 0 | 0 | `dynamic-linkage` 3 / 3 |
| go-cobra | 0 | 0 | `go-buildinfo` 1 / 1 |

### TheRock

- **Merges with a record (FR-002): exactly 6 PURLs**: `expat@2.7.5`, `gmp@6.3.0`, `liblzma@5.8.1`, `llvm@23.0.0git`, `ncurses@6.6.20251230` and `zlib@1.3.2`. There are 18 such events, one per absorbed binary entry, since each library has several files.
- **Binary into binary**: 5 events, for `bzip2@1.0.8` and `pkg:generic/sqlite`. The same library was found in several files; the technique is the same on both sides, so nothing is absorbed.
- **Technique changes (FR-003): 8 PURLs**. The 7 curated libraries, plus `pkg:generic/sqlite`, an unversioned symbol-fingerprint match.
- **Merge sites (FR-006)**: all 23 events are at S1. S2 (`fold_declared_not_cached`) absorbed no binary match on any target.

### Quality corpus

`pants-backend-ai` has banner matches for `openssl@3.5.4` and `zlib@1.3.1` (spec 1643's four pattern events), each merged binary into binary. Their technique changes to `binary-analysis`, and no record absorbs them. Every other quality-corpus target has no banner or fingerprint match.

## Checkpoint

| Requirement | Expected (tasks T009) | Measured |
|---|---|---|
| FR-002, merges with a record on TheRock | exactly 6 PURLs (expat, gmp, liblzma, llvm, ncurses, zlib) | as expected |
| FR-003, technique changes on TheRock | 7 libraries | **8 PURLs**. The 8th is `pkg:generic/sqlite`, a symbol-fingerprint match. FR-003 names `symbol-fingerprint`, but the expectation counted only curated banner matches. **The expectation was incomplete; the spec is not contradicted.** |
| FR-008, scope outside TheRock | — | `pants-backend-ai`: 2 components change technique (openssl, zlib); the public corpus is unchanged at the log-only commit |
| FR-006, merge sites | S1 and S2 observed | S1 only. S2 never received a binary match in these corpora. Its call site runs the same `absorb_methods` and `apply_absorbed` as S1, and those helpers are unit-tested; the S2 call site itself is not exercised by any test |

No count contradicts FR-002 to FR-008, so the spec is not amended.

The other binary-derived kinds counted here (`embedded-source-path`, `dynamic-linkage`, `go-buildinfo`) are the subject of the follow-up issue #1668, which gives these counts.
