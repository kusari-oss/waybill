# Canary run history (captured 2026-09-17)

Command:

```bash
gh run list --workflow=ebpf-canary.yml --limit 60 \
  --json conclusion,createdAt,databaseId,event
```

**Total runs: 36. Successes: 0.**

The canary has never produced a green run. This is not a 35-night streak after
a green baseline — there is no green baseline anywhere in its history, so
nothing has ever established that the pinned version builds under the canary's
own environment. That is why FR-002 exists.

| # | Date (UTC) | Run ID | Event | Conclusion |
|---|---|---|---|---|
| 1 | 2026-08-13 | `31674387553` | schedule | failure |
| 2 | 2026-08-14 | `31776715099` | schedule | failure |
| 3 | 2026-08-15 | `31868754084` | schedule | failure |
| 4 | 2026-08-16 | `31930772378` | schedule | failure |
| 5 | 2026-08-17 | `32000904046` | schedule | failure |
| 6 | 2026-08-18 | `32105910821` | schedule | failure |
| 7 | 2026-08-19 | `32222457240` | schedule | failure |
| 8 | 2026-08-20 | `32338649124` | schedule | failure |
| 9 | 2026-08-21 | `32453513787` | schedule | failure |
| 10 | 2026-08-22 | `32556251418` | schedule | failure |
| 11 | 2026-08-23 | `32622273017` | schedule | failure |
| 12 | 2026-08-24 | `32696758995` | schedule | failure |
| 13 | 2026-08-25 | `32815991607` | schedule | failure |
| 14 | 2026-08-26 | `32937273174` | schedule | failure |
| 15 | 2026-08-27 | `33057429006` | schedule | failure |
| 16 | 2026-08-28 | `33158626099` | schedule | failure |
| 17 | 2026-08-29 | `33238241613` | schedule | failure |
| 18 | 2026-08-30 | `33296717252` | schedule | failure |
| 19 | 2026-08-31 | `33364490699` | schedule | failure |
| 20 | 2026-09-01 | `33477531700` | schedule | failure |
| 21 | 2026-09-02 | `33599027738` | schedule | failure |
| 22 | 2026-09-03 | `33723042744` | schedule | failure |
| 23 | 2026-09-04 | `33844166487` | schedule | failure |
| 24 | 2026-09-05 | `33949578771` | schedule | failure |
| 25 | 2026-09-06 | `34016270412` | schedule | failure |
| 26 | 2026-09-07 | `34091202033` | schedule | failure |
| 27 | 2026-09-08 | `34194469688` | schedule | failure |
| 28 | 2026-09-09 | `34318922531` | schedule | failure |
| 29 | 2026-09-10 | `34445040422` | schedule | failure |
| 30 | 2026-09-11 | `34569711677` | schedule | failure |
| 31 | 2026-09-12 | `34677903736` | schedule | failure |
| 32 | 2026-09-13 | `34742718469` | schedule | failure |
| 33 | 2026-09-14 | `34813829390` | schedule | failure |
| 34 | 2026-09-15 | `34936856073` | schedule | failure |
| 35 | 2026-09-16 | `35063568788` | schedule | failure |
| 36 | 2026-09-17 | `35189734662` | schedule | failure |

First run: 2026-08-13T06:35:17Z (`31674387553`)
Latest run: 2026-09-17T06:25:09Z (`35189734662`)
Span: 34 days

Open report #685 was created 2026-08-13T06:36:09Z — 52 seconds after the first
failing run. That is why the issue's `created_at` is a sound streak-start
timestamp (research R7): it tracks the streak start to within one run's duration.
