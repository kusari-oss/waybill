# waybill perf numbers

Generated from `docs/perf/baseline.json` captured at:

- **waybill commit**: `d5bffeb6e5613df9e69933c8c1506ed513f5d11c`
- **fixtures pin**: `891f63429480554cd2fedd48de8e5c0bdd6ba943`
- **runner**: `Darwin Michaels-MacBook-Pro.local 25.5.0 arm64` (noise class: `Noisy`)
- **duration**: 341s
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
| `default` | 54 | 640 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 55 | 672 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 55 | 656 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 55 | 672 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `cargo-workspace-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 108 | 656 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 107 | 576 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 110 | 608 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 108 | 656 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `cmake-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 110 | 656 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 108 | 640 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 109 | 640 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 107 | 656 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `conan-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 640 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 54 | 656 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 54 | 640 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 55 | 640 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `debian-slim`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1993 | 160736 | 1262187 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 1866 | 152928 | 1288046 | 922 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 1884 | 164752 | 4410305 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `gem-bundler-small`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 105 | 608 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 103 | 640 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 108 | 656 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 104 | 576 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `go-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 10142 | 22384 | 152160 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 10711 | 21136 | 152160 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 11024 | 21216 | 753606 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 10905 | 21152 | 753606 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `gradle-multi-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 105 | 736 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 106 | 656 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 108 | 640 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 105 | 19920 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `linux-binaries-50`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 52 | 608 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 55 | 656 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `maven-multi-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 105 | 608 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 106 | 656 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 107 | 464 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 106 | 464 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `npm-monorepo-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 52 | 608 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 54 | 656 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 53 | 560 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 52 | 640 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `nuget-solution-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 656 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 55 | 640 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 54 | 656 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 55 | 624 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `pip-poetry-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 54 | 688 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 53 | 608 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 54 | 656 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 54 | 656 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

## `vcpkg-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 624 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash` | 55 | 640 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `no-deep-hash-plus-triple-format` | 53 | 656 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |
| `triple-format` | 55 | 640 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `d5bffeb6e5613df9e69933c8c1506ed513f5d11c` |

---

_Baseline captured at 2026-08-31T00:52:38.125712+00:00 (341s). Regenerate this page after
each `docs/perf/baseline.json` refresh via
`cargo run -p xtask -- bench-docs`._
