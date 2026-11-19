# Trace

## T001: premises at `main` (`1647ee01`)

- **(a) Main modules carry `waybill:component-role` = `main-module` when propagation runs.**
  - Milestone 149's `apply_main_module_drop_or_demote` is called only from `generate/` (`generate/mod.rs`, `spdx/document.rs`, `spdx/v3_document.rs`, `graph_completeness/document.rs`), which runs after `scan_cmd`'s propagation call.
  - The fixture scans show the role on every main module in the emitted documents without `--root-name`.
  - P-10 asserts the `--root-name` case after the change: if the premise were wrong there, P-10 would fail.
- **(b) In the `two` fixture, `two@0.0.0`, a synthetic subject, is the primary root.** It isn't a main module, so `component_root_seeds` adds one pip main module by `pick_ecosystem_top`: no workspace root, then the lowest PURL, which is `pkg:pypi/waybill-fixture-a@1.0.0`. Evidence from the emitted document at `main`'s behaviour:
  - `waybill-fixture-a-runtime`, declared by `a`, is required: `a` is a seed;
  - `waybill-fixture-shipped`, declared at runtime by `b`, is `excluded` with C62 `transitive`: `b` is not a seed.
- **(c) `runtime_reachable` puts every seed in the immune set before walking** (`scope_propagation.rs`, the `for seed in seeds` loop). A non-runtime seed would therefore be immune itself, and would immunise its runtime children. FR-002 exists for this.

## Fixtures at `main`'s behaviour (T002)

| Fixture | Component | `main` | Expected after |
|---|---|---|---|
| `two` | `shipped`, `shipped-dep` | excluded, `transitive` | required (P-1, P-2) |
| `two` | `a-runtime` | required | required (P-1 guard, FR-005) |
| `two` | `devonly` | excluded, `transitive` | unchanged (P-3) |
| `nonruntime` | `c` (main module, a dev-group member of `a`) | excluded, no C62 | unchanged |
| `nonruntime` | `cdep` | excluded, `transitive` | unchanged (P-4) |
| `dotdir` | `action` (main module under `.github/actions/x/`) | excluded, `ci-directory` | unchanged |
| `dotdir` | `actdep` | excluded, `transitive` | unchanged (P-5) |

**P-4's first fixture did not work.** In it, `c` was its own project with a lockfile, and also a dev-group member of `a`. Its runtime declaration won the merge, so it was required, not non-runtime. The working shape is TheRock's `rocisa`: an editable workspace member that `a`'s lockfile lists in a dev group, with no lockfile of its own.

## T006: P-9 is a unit test

Building P-9 as a tree needs a reader that emits a declared dependency edge from a component of one ecosystem to a component of another. The only cross-ecosystem edges I know of are milestone 218's inferred bridges, which spec 1269 excludes by design, and I did not build a tree that produces a declared one. P-9 is therefore asserted in `scope_propagation.rs` (`s1616_cross_ecosystem_added_seed_protects`), on an in-memory graph with a declared `dependency-graph` edge from a pip dev tool to an npm package. Before the change the package takes the dev scope, and after it the non-top npm project protects it.

## T015: P-8 reads the binary's log

The propagation INFO line is on the scan's stderr under `RUST_LOG=info`, with ANSI colour codes, which the test strips. On the `two` fixture it reports `immune_by_added_seeds=2` (`shipped` and `shipped-dep`). The contract's 1 was an arithmetic slip and is corrected.
