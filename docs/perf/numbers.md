# waybill perf numbers

Generated from `docs/perf/baseline.json` captured at:

- **waybill commit**: `6e5148e967cc9c7c1cbba93fafcb77d3e8a95690`
- **fixtures pin**: `4de48e97a9771a884cfe1c64279bb428657a4161`
- **runner**: `Darwin Michaels-MacBook-Pro.local 25.5.0 arm64` (noise class: `Noisy`)
- **duration**: 11s
- **schema version**: 1

## Reference architecture

Numbers below reflect the Linux x86_64 GitHub-hosted-runner
class per waybill spec 669 Assumption 1. Cross-host projections
are deferred to a future milestone; use these numbers as an
upper-bound reference on quieter hardware and expect drift on
macOS runners (m094 noise-class = `Noisy`).

## `cargo-workspace-medium`

| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |
|---|---:|---:|---:|---:|---|---|---|
| `default` | 486 | 18912 | 13729 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `6e5148e967cc9c7c1cbba93fafcb77d3e8a95690` |
| `no-deep-hash` | 379 | 18976 | 13729 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `6e5148e967cc9c7c1cbba93fafcb77d3e8a95690` |
| `no-deep-hash-plus-triple-format` | 378 | 18960 | 70122 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `6e5148e967cc9c7c1cbba93fafcb77d3e8a95690` |
| `triple-format` | 375 | 19040 | 70122 | 6 | success | `4de48e97a9771a884cfe1c64279bb428657a4161` | `6e5148e967cc9c7c1cbba93fafcb77d3e8a95690` |

---

_Baseline captured at 2026-08-29T19:29:27.785487+00:00 (11s). Regenerate this page after
each `docs/perf/baseline.json` refresh via
`cargo run -p xtask -- bench-docs`._
