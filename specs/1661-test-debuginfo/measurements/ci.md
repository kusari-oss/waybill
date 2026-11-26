# CI measurements: spec 1661 (T007–T010)

**Branches:**
- `1661-test-debuginfo`: full debug info, today's setting; the PR branch.
- `1661-ci-lt`: `CARGO_PROFILE_{DEV,TEST}_DEBUG=line-tables-only` in the workflow env.
- `1661-ci-none`: the same with `0`.

The two measurement branches were for measuring only, and were deleted afterwards. Each round ran the three settings in parallel. The raw data is in `ci.tsv`; the failing tests per run are in `ci-failures.md`.

**Test build:** cargo's `Finished \`test\` profile … in` time, waybill's own crates only, since dependencies come from the cache.

| lane | full (rounds 1, 2) | line-tables-only | none |
|---|---|---|---|
| Linux | 151 s, 208 s | 221 s, 129 s | 123 s, 193 s |
| macOS | 633 s, 953 s | 542 s, 469 s | 434 s, 387 s |
| Windows (non-blocking, recorded only) | 305 s, 366 s | 219 s, 280 s | 137 s, 53 s |

**The whole `Tests` step:**

| lane | full | line-tables-only | none |
|---|---|---|---|
| Linux | 414 s, 620 s | 603 s, 432 s | 357 s, 550 s |
| macOS | 1,221 s, 1,514 s | 1,088 s, 945 s | 965 s, 983 s |
| Windows | 827 s, 947 s | 697 s, 917 s | 548 s, 638 s |

**The backtrace test** (`backtrace_resolves_source`):

| setting | result |
|---|---|
| full | PASS on every lane, both rounds |
| line-tables-only | PASS on every lane, both rounds |
| none | **FAIL** on every lane, both rounds: no file and line, as intended |

**Other failures:**
- **Linux and macOS:** none, under any setting. With no debug info, the backtrace test was the only failure.
- **Windows:** 140 failures under every setting. This is the existing #210 set, which also fails on `main`. With no debug info it was 141 in round 1, the extra one being the backtrace test, and 140 in round 2.

## Decision (FR-005, research R4): CI unchanged

| setting | adopted? | reason |
|---|---|---|
| none | no | backtraces lose file and line on every lane |
| line-tables-only | no | it beats both full builds on macOS (542 and 469 s against 633 and 953 s), but not on Linux: 221 s is slower than both full runs |

**The real finding is the noise.** The *same* setting varied 38% on Linux (151 → 208 s) and 51% on macOS (633 → 953 s) between two runs, which is more than any difference between the settings. Two runs can't resolve an effect of this size on hosted runners. Line-tables-only may well help on macOS, but this measurement doesn't show it reliably, so it isn't adopted on a guess.
