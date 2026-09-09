# waybill perf numbers

Generated from `docs/perf/baseline.json` captured at:

- **waybill commit**: `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7`
- **fixtures pin**: `891f63429480554cd2fedd48de8e5c0bdd6ba943`
- **runner**: `Linux runnervmejwal 6.17.0-1022-azure x86_64` (noise class: `Reference`)
- **duration**: 64s
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
| `default` | 50 | 2416 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 50 | 2484 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 50 | 18540 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 50 | 4884 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `cargo-workspace-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 50 | 4360 | 88802 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 50 | 5020 | 88802 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 100 | 2368 | 428836 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 100 | 3140 | 428836 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `cmake-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 100 | 4616 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 100 | 2572 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 100 | 2608 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 100 | 2872 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `conan-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 50 | 2492 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 50 | 2472 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 50 | 2344 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 50 | 2416 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `debian-slim`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1503 | 121336 | 1557801 | 585 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 1503 | 121388 | 1284854 | 922 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 1604 | 121656 | 5742067 | 585 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `gem-bundler-small`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 100 | 2480 | 57312 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 100 | 2524 | 57312 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 100 | 2836 | 296574 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 100 | 2492 | 296574 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `go-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 100 | 2512 | 147289 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 100 | 4624 | 147289 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 100 | 2536 | 720758 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 100 | 7100 | 720758 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `gradle-multi-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 150 | 17068 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 150 | 17104 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 150 | 17144 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 150 | 17252 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `linux-binaries-50`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 50 | 2984 | 138418 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 50 | 2572 | 138418 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `maven-multi-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 150 | 17248 | 124315 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 150 | 17236 | 124315 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 150 | 17300 | 612644 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 150 | 17204 | 612644 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `npm-monorepo-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 50 | 2480 | 101206 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 50 | 2472 | 101206 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 50 | 2564 | 481732 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 50 | 2532 | 481732 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `nuget-solution-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 50 | 2592 | 79961 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 50 | 2576 | 79961 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 100 | 2924 | 414031 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 100 | 2528 | 414031 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `pip-poetry-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 50 | 2460 | 83655 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 50 | 2856 | 83655 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 50 | 2588 | 396623 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 50 | 2440 | 396623 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

## `vcpkg-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 100 | 7772 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash` | 100 | 2452 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `no-deep-hash-plus-triple-format` | 100 | 2508 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |
| `triple-format` | 100 | 2584 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `39eaf9b3be932d5c44019f68a393da2a4b9d3ee7` |

---

_Baseline captured at 2026-09-08T21:33:28.458735939+00:00 (64s) on `Linux runnervmejwal 6.17.0-1022-azure x86_64` (Reference). Regenerate this
page after each `docs/perf/baseline.json` refresh via
`cargo run -p xtask -- bench-docs`. To refresh the baseline
itself see [refreshing-the-baseline.md](refreshing-the-baseline.md)
— it must come from a reference-class CI runner, not a local
machine._
