# waybill perf numbers

Generated from `docs/perf/baseline.json` captured at:

- **waybill commit**: `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8`
- **fixtures pin**: `891f63429480554cd2fedd48de8e5c0bdd6ba943`
- **runner**: `Darwin Michaels-MBP.localdomain 25.5.0 arm64` (noise class: `Noisy`)
- **duration**: 73s
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
| `default` | 53 | 608 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 51 | 656 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 54 | 640 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 54 | 624 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `cargo-workspace-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 109 | 20160 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 108 | 640 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 108 | 624 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 107 | 20896 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `cmake-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 106 | 544 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 104 | 528 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 106 | 512 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 106 | 624 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `conan-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 608 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 52 | 608 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 52 | 672 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 55 | 656 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `debian-slim`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1494 | 157312 | 1262187 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 1403 | 140032 | 1288046 | 922 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 1456 | 164784 | 4410305 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `gem-bundler-small`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 109 | 608 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 55 | 608 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 108 | 464 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 107 | 608 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `go-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 107 | 656 | 147289 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 106 | 608 | 147289 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 107 | 528 | 720758 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 107 | 656 | 720758 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `gradle-multi-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 108 | 656 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 108 | 496 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 105 | 640 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 110 | 19360 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `linux-binaries-50`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 608 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 53 | 656 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `maven-multi-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 106 | 656 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 107 | 608 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 106 | 608 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 108 | 608 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `npm-monorepo-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 53 | 576 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 55 | 656 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 54 | 656 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 52 | 608 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `nuget-solution-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 53 | 496 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 53 | 608 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 55 | 640 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 55 | 608 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `pip-poetry-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 656 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 55 | 560 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 53 | 656 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 55 | 656 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

## `vcpkg-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 51 | 608 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash` | 55 | 608 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `no-deep-hash-plus-triple-format` | 53 | 576 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |
| `triple-format` | 51 | 656 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `eea2fab7d342b6e6dedd4b210e8efdc6838c50e8` |

---

_Baseline captured at 2026-08-31T21:04:27.139151+00:00 (73s). Regenerate this page after
each `docs/perf/baseline.json` refresh via
`cargo run -p xtask -- bench-docs`._
