# T001/T002 — pre-change baseline

Captured 2026-09-13 on `779c1362` (branch 860-multi-main-module-override).
Scans use the harness invocation: `--offline ... --root-name <target> --root-version <pin>`.

## Affected targets (N>1 main modules)

| target | main modules | components (no override) | components (golden) | dangling refs (cdx) |
|--------|-------------:|-------------------------:|--------------------:|--------------------:|
| maven-guice | 16 | 61 | 45 | 5 |
| rust-ripgrep | 10 | 68 | 58 | 9 |
| python-flask | 4 | 109 | 105 | 0 |

## Unaffected targets (zero main modules) — SC-005 byte-identity starting point

| target | components (golden) | dangling refs |
|--------|--------------------:|--------------:|
| go-cobra | 7 | 0 |
| npm-express | 44 | 0 |
| pants-example-django | 46 | 0 |
| pants-example-golang | 5 | 0 |
| pants-example-javascript | 302 | 0 |
| pants-example-jvm | 28 | 0 |
| pants-example-python | 11 | 0 |
| image-postgres16 | 315 | 0 |
