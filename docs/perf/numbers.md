# waybill perf numbers

Generated from `docs/perf/baseline.json` captured at:

- **waybill commit**: `55039a4a7bb12211a42299161913766927e5fe3b`
- **fixtures pin**: `4de48e97a9771a884cfe1c64279bb428657a4161`
- **runner**: `Darwin Michaels-MacBook-Pro.local 25.5.0 arm64` (noise class: `Noisy`)
- **duration**: 150s
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
| `default` | 56 | 608 | 6320 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 55 | 624 | 6320 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 56 | 576 | 36071 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 54 | 464 | 36071 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `cargo-workspace-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 429 | 18896 | 13729 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 427 | 18944 | 13729 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 381 | 20608 | 70122 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 372 | 19008 | 70122 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `cmake-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 592 | 10123 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 55 | 592 | 10123 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 56 | 576 | 58716 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 55 | 576 | 58716 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `conan-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 56 | 640 | 6301 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 54 | 576 | 6301 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 56 | 576 | 36018 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 56 | 624 | 36018 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `debian-slim`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 110 | 576 | 0 | 0 | non-zero-exit-code | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 108 | 608 | 0 | 0 | non-zero-exit-code | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 106 | 608 | 0 | 0 | non-zero-exit-code | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `gem-bundler-small`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 53 | 528 | 9098 | 4 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 56 | 656 | 9098 | 4 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 55 | 624 | 48935 | 4 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 56 | 560 | 48935 | 4 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `go-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 2105 | 19200 | 17220 | 5 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 1979 | 19760 | 17220 | 5 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 1973 | 19248 | 94715 | 5 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 1947 | 19264 | 94715 | 5 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `gradle-multi-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 377 | 20400 | 14343 | 5 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 327 | 20384 | 14343 | 5 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 271 | 20432 | 75651 | 5 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 317 | 20432 | 75651 | 5 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `linux-binaries-50`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 55 | 608 | 0 | 0 | non-zero-exit-code | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 56 | 608 | 0 | 0 | non-zero-exit-code | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `maven-multi-module-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 545 | 19104 | 20020 | 8 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 433 | 19136 | 20020 | 8 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 427 | 18928 | 96634 | 8 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 476 | 20688 | 96634 | 8 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `npm-monorepo-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 427 | 18832 | 16170 | 9 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 374 | 18384 | 16170 | 9 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 323 | 18416 | 80115 | 9 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 321 | 18352 | 80115 | 9 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `nuget-solution-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 272 | 18496 | 12278 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 267 | 18496 | 12278 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 215 | 18752 | 66437 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 212 | 19792 | 66437 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `pip-poetry-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 274 | 18336 | 9417 | 4 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 261 | 18432 | 9417 | 4 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 219 | 19968 | 51493 | 4 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 269 | 18400 | 51493 | 4 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

## `vcpkg-project-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 56 | 640 | 6139 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `fingerprints-corpus` | 0 | 0 | 0 | 0 | corpus-unreachable | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash` | 55 | 608 | 6139 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `no-deep-hash-plus-triple-format` | 55 | 592 | 35670 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |
| `triple-format` | 56 | 544 | 35670 | 3 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `55039a4a7bb12211a42299161913766927e5fe3b` |

---

_Baseline captured at 2026-08-30T09:14:30.021811+00:00 (150s). Regenerate this page after
each `docs/perf/baseline.json` refresh via
`cargo run -p xtask -- bench-docs`._
