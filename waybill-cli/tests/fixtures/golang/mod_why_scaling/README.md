# mod_why_scaling fixture

Synthetic Go monorepo for milestone-771 integration tests. Shape:

- Root `go.work` referencing three members: `mod-a`, `mod-b`, `mod-c`.
- Sibling `loose/` main-module NOT in the go.work — exercises FR-008
  fallback path (per-workspace preflight for non-workspace roots).
- All module + dependency names use the `waybill-fixture-*` synthetic
  convention (see memory `feedback_fixture_synthetic_package_names`) —
  never real coordinates.

The fixture drives **structural** checks only: the walker discovers 4
`go.mod` files, `parse_go_work` identifies 3 members, and the classifier
partitions them into 1 scope-with-3-members + 1 loose. No real `go`
toolchain invocations happen against this fixture — integration tests
use `WAYBILL_GO_MOD_WHY_BUDGET_MS=1` to short-circuit the actual
subprocess pass while still exercising the orchestration code paths.

Ownership: `specs/771-gomodwhy-subprocess-scale/tasks.md::T003`.
