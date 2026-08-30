# waybill perf numbers

Generated from `docs/perf/baseline.json` captured at:

- **waybill commit**: `6bfbc70829982f253f8de3d088315fe5666b02c0`
- **fixtures pin**: `891f63429480554cd2fedd48de8e5c0bdd6ba943`
- **runner**: `Darwin Michaels-MacBook-Pro.local 25.5.0 arm64` (noise class: `Noisy`)
- **duration**: 858s
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
| `default` | 52 | 608 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 55 | 656 | 47058 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 55 | 656 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 55 | 608 | 268735 | 33 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `cargo-workspace-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 2589 | 19808 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 1940 | 19776 | 83019 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 1663 | 20400 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 1715 | 22048 | 410473 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `cmake-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 110 | 608 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 107 | 656 | 101070 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 107 | 624 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 110 | 608 | 581533 | 75 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `conan-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 54 | 624 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 55 | 656 | 53499 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 55 | 656 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 55 | 688 | 306588 | 38 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `debian-slim`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1835 | 156144 | 1262187 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 1635 | 159008 | 1288046 | 922 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 1844 | 161632 | 4410305 | 358 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `gem-bundler-small`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 107 | 576 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 55 | 640 | 57317 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 109 | 512 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 106 | 640 | 296589 | 35 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `go-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 13149 | 21696 | 152160 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 13013 | 21632 | 152160 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 12795 | 21776 | 753606 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 12791 | 21248 | 753606 | 71 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `gradle-multi-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 2994 | 22368 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 2625 | 22128 | 127097 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 2188 | 22096 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 2309 | 24720 | 641262 | 47 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `linux-binaries-50`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 54 | 656 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 54 | 656 | 139695 | 61 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `maven-multi-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 3338 | 20560 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 2797 | 20288 | 124355 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 2448 | 20736 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 2492 | 20768 | 612754 | 53 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `npm-monorepo-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 2690 | 19168 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 1842 | 19200 | 92904 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 1663 | 19216 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 1616 | 19232 | 458816 | 45 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `nuget-solution-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 2097 | 19120 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 1983 | 19120 | 73100 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 1505 | 19152 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 1609 | 19552 | 392486 | 41 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `pip-poetry-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 1926 | 20000 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 1722 | 19552 | 82818 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 1437 | 19616 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 1645 | 20032 | 407146 | 36 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

## `vcpkg-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 576 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash` | 55 | 656 | 105399 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `no-deep-hash-plus-triple-format` | 55 | 608 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |
| `triple-format` | 53 | 544 | 599466 | 79 | success | `891f63429480554cd2fedd48de8e5c0bdd6ba943` | `6bfbc70829982f253f8de3d088315fe5666b02c0` |

---

_Baseline captured at 2026-08-30T17:25:10.455758+00:00 (858s). Regenerate this page after
each `docs/perf/baseline.json` refresh via
`cargo run -p xtask -- bench-docs`._
