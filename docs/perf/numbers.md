# waybill perf numbers

Generated from `docs/perf/baseline.json` captured at:

- **waybill commit**: `2da78461e3f5271398ebc759f15a461cf9128718`
- **fixtures pin**: `891f63429480554cd2fedd48de8e5c0bdd6ba943`
- **runner**: `Darwin Michaels-MacBook-Pro.local 25.5.0 arm64` (noise class: `Noisy`)
- **duration**: 679s
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
| `default` | 55 | 608 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 55 | 592 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 55 | 608 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 55 | 656 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `cargo-workspace-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 2306 | 20288 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 1881 | 20736 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 1620 | 20752 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 1705 | 20320 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `cmake-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 105 | 656 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 55 | 656 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 105 | 656 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 108 | 640 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `conan-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 592 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 55 | 528 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 52 | 656 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 55 | 656 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `debian-slim`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1741 | 158432 | 1262187 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 1646 | 160032 | 1288046 | 922 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 1742 | 163888 | 4410305 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `gem-bundler-small`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 640 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 55 | 608 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 55 | 560 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 55 | 656 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `go-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 13462 | 22128 | 152160 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 12851 | 21952 | 152160 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 12989 | 22176 | 753606 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 12470 | 22096 | 753606 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `gradle-multi-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 3034 | 22720 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 2397 | 22880 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 2136 | 22736 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 2198 | 22720 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `linux-binaries-50`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 608 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 55 | 624 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `maven-multi-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 3317 | 20720 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 2823 | 22976 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 2466 | 21168 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 2509 | 23504 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `npm-monorepo-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 2300 | 20032 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 1875 | 19648 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 1663 | 19584 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 1698 | 19632 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `nuget-solution-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1935 | 19824 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 1733 | 19568 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 1397 | 19792 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 1492 | 19840 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `pip-poetry-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1985 | 20144 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 1652 | 20192 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 1395 | 20576 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 1499 | 20592 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

## `vcpkg-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 560 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash` | 55 | 656 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `no-deep-hash-plus-triple-format` | 55 | 608 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |
| `triple-format` | 55 | 608 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `2da78461e3f5271398ebc759f15a461cf9128718` |

---

_Baseline captured at 2026-08-30T22:07:16.277683+00:00 (679s). Regenerate this page after
each `docs/perf/baseline.json` refresh via
`cargo run -p xtask -- bench-docs`._
