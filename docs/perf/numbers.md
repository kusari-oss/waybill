# waybill perf numbers

Generated from `docs/perf/baseline.json` captured at:

- **waybill commit**: `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f`
- **fixtures pin**: `891f63429480554cd2fedd48de8e5c0bdd6ba943`
- **runner**: `Darwin Michaels-MacBook-Pro.local 25.5.0 arm64` (noise class: `Noisy`)
- **duration**: 78s
- **schema version**: 1

## Reference architecture

Numbers below reflect the Linux x86_64 GitHub-hosted-runner
class per waybill spec 669 Assumption 1. Cross-host projections
are deferred to a future milestone; use these numbers as an
upper-bound reference on quieter hardware and expect drift on
macOS runners (m094 noise-class = `Noisy`).

## `bazel-monorepo-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 656 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 55 | 704 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 53 | 656 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 55 | 608 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `cargo-workspace-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 110 | 20192 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 109 | 352 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 106 | 656 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 109 | 640 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `cmake-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 105 | 608 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 110 | 656 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 107 | 656 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 108 | 560 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `conan-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 52 | 656 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 55 | 656 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 52 | 656 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 55 | 640 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `debian-slim`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1963 | 162288 | 1262187 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 1931 | 143264 | 1288046 | 922 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 1949 | 164624 | 4410305 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `gem-bundler-small`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 576 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 55 | 640 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 55 | 656 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 55 | 608 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `go-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 102 | 656 | 147289 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 105 | 608 | 147289 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 108 | 608 | 720758 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 106 | 640 | 720758 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `gradle-multi-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 108 | 656 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 109 | 640 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 105 | 656 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 108 | 608 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `linux-binaries-50`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 53 | 608 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 53 | 608 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `maven-multi-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 107 | 304 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 105 | 640 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 109 | 640 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 108 | 624 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `npm-monorepo-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 464 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 55 | 608 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 55 | 656 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 55 | 608 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `nuget-solution-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 54 | 624 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 55 | 608 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 55 | 640 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 54 | 560 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `pip-poetry-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 640 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 55 | 464 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 55 | 640 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 54 | 464 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

## `vcpkg-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 656 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash` | 55 | 656 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `no-deep-hash-plus-triple-format` | 55 | 656 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |
| `triple-format` | 54 | 640 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d4fd4f5848be79a387bfba0aab9426b4dd2e2a6f` |

---

_Baseline captured at 2026-08-31T01:43:02.446172+00:00 (78s). Regenerate this page after
each `docs/perf/baseline.json` refresh via
`cargo run -p xtask -- bench-docs`._
