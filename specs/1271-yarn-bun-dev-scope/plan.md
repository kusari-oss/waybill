# Implementation Plan: yarn and bun development scope

**Branch**: `1271-yarn-bun-dev-scope` | **Date**: 2026-11-01 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1271-yarn-bun-dev-scope/spec.md`

## Summary

Two of the four npm-ecosystem lockfile readers assign no development
lifecycle scope, so `--exclude-scope dev` removes nothing from a yarn or bun
project and milestone 1269's transitive propagation has no seed to walk from.
bun is served from the lockfile, whose `workspaces` map carries the axis per
workspace path; yarn is served from `package.json` on disk, because neither
the v1 nor the Berry dialect records the axis at all. Both apply the same
rule milestone 1270 proved for pnpm v9: collect development declarations,
collect runtime and optional declarations, transitively close the runtime
set, subtract.

Format shapes are measured, not inferred — `measurements/findings.md` and the
probe beside it, nine cells, reproduced across two rounds.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly. No `+toolchain` argument when running the gate (#1189).
**Primary Dependencies**: Existing only — `serde_json` (bun lockfile, yarn manifests), `serde_yaml` (Berry lockfile), `regex` (yarn v1 line format), `globset` (workspace-pattern expansion, already a direct dep since milestone 113), `tracing`, `anyhow`. **Zero new Cargo dependencies** (FR-008).
**Storage**: N/A — all state in-process per scan, as with every reader since milestone 002.
**Testing**: `cargo test --workspace --no-fail-fast` plus `cargo clippy --workspace --all-targets`, via `./scripts/pre-pr.sh`. Fixtures under `waybill-cli/tests/fixtures/`, synthetic `waybill-fixture-*` coordinates.
**Target Platform**: Linux, macOS, Windows — the three default CI lanes. `waybill-ebpf` untouched; this is user-space only.
**Project Type**: CLI + library (`waybill-cli` readers, `waybill-common` types).
**Performance Goals**: No regression. The one new cost is workspace-pattern expansion for yarn, bounded by FR-011 to the traversal guarantees the scanner already provides.
**Constraints**: No new output field, annotation or dependency (FR-008). Existing optional and peer classification unchanged (FR-003).
**Scale/Scope**: Two readers, two lockfile dialects for one of them, plus workspace members. Three source files expected to change — the two readers and the dispatcher — plus fixtures and tests. No new module.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Basis |
|---|---|---|
| I. Pure Rust, Statically Linked | PASS | Zero new dependencies (FR-008). No C, no FFI, no `*-sys`. |
| II. eBPF-Only Observation | N/A | Scan mode. Principle II governs trace-mode discovery; lockfile reading is scan mode's designed input. |
| III. Fail Closed | PASS | Trace-mode principle; in scan mode Principle X governs, and FR-006 / FR-010 / FR-011 report unavailable evidence rather than filling it in. No heuristic gap-filling: FR-005 forbids inferring development status from a lockfile that does not record it. |
| IV. Type-Driven Correctness | PASS, with obligation | Classification uses the existing `LifecycleScope` enum, not strings. No `.unwrap()` in production code; test modules need the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard the crate convention requires. |
| V. Specification Compliance | PASS | FR-008 reuses the existing spec-native scope carriers in all three formats. No new extension. |
| VI. Three-Crate Architecture | PASS | Changes confined to `waybill-cli` readers; `waybill-common` types reused unchanged; `waybill-ebpf` untouched. |
| VII. Test Isolation | PASS, with obligation | Fixtures are per-test directories. Any test that mutates an environment variable — plausible for the exclusion path — must route through `crate::testing::EnvGuard::acquire()` per the existing convention, or it will race. |
| VIII. Completeness | PASS | No component is added or removed by this feature. It assigns scope to components already emitted. |
| IX. Accuracy | PASS, and strengthened | FR-002's runtime-wins rule exists precisely to avoid the false positive of marking a runtime dependency development-only. FR-012 refuses to synthesise an identity for a declaration the lockfile does not lock. |
| X. Transparency | PASS — see note | Resolved in research R4 rather than assumed. |
| XI / XII. Enrichment | N/A | No external data source; no network access. |

**Note on Principle X.** FR-012 accepts that a `patch:`-wrapped development
dependency stays unclassified, and FR-008 forbids a new annotation to say so.
That looks like a Transparency gap and was checked rather than waved through.
Principle X triggers when completeness (VIII) or accuracy (IX) cannot be
guaranteed; an absent optional scope assertion is neither a missing component
nor a wrongly included one, and absence is the pre-existing state for every
yarn and bun component today. So X is not triggered, and no violation needs
justifying in Complexity Tracking. R4 nonetheless recommends the zero-cost
half-measure: the diagnostic FR-006 already emits should carry a count of
declarations skipped for this reason, so the operator sees a number rather
than silence. That is a log line inside an existing surface, not a new output
field, so it does not contradict FR-008.

No gate fails. Complexity Tracking is empty by design.

## Project Structure

### Documentation (this feature)

```text
specs/1271-yarn-bun-dev-scope/
├── plan.md                 # This file
├── spec.md                 # Feature specification (3 clarifications integrated)
├── research.md             # Phase 0 output
├── data-model.md           # Phase 1 output
├── quickstart.md           # Phase 1 output
├── contracts/              # Phase 1 output
│   ├── classification.md   # The rule both readers implement
│   └── diagnostics.md      # What the operator is told, and when
├── checklists/
│   └── requirements.md     # Spec quality checklist, 16/16
├── measurements/
│   ├── findings.md         # Where the dev axis lives, per lockfile
│   └── lockfile-dev-axis-probe.sh
└── tasks.md                # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── npm/
│   ├── mod.rs              # reader dispatch; threads exclude_set into yarn (new)
│   ├── bun_lock.rs         # workspace dev declarations + closure (US1)
│   ├── yarn_lock.rs        # manifest dev declarations + member discovery (US2, US3)
│   └── pnpm_lock.rs        # unchanged; v9_importer_scopes is the reference rule
└── exclude_path.rs         # unchanged; ExclusionSet::matches is reused

waybill-cli/tests/
├── fixtures/bun_lock/dev_scope*/          # new, lockfileVersion 2
├── fixtures/yarn_lock/dev_scope*/         # new, v1 and Berry over one manifest
└── optional_dep_classification.rs         # existing suite the new cases join
```

**Structure Decision**: No new module. Both readers already own their parsing
and both already construct `PackageDbEntry` values with a `lifecycle_scope`
field; this adds a classification pass inside each. The pnpm reader is the
reference implementation and is deliberately left untouched so that a
regression in it cannot be attributed to this work.

## Two structural facts that shape the tasks

Both were read from the code, not assumed, and both would have broken an
implementation that started from the spec alone.

**The dev flag reaches these readers and is discarded.** `read_yarn_lock`
(`npm/yarn_lock.rs:34`) and `read_bun_lock` (`npm/bun_lock.rs:159`) each take
`_include_dev: bool` and ignore it, while `read_package_lock` and
`read_pnpm_lock` use theirs. The plumbing for the operator's intent already
arrives at the door. Separately, neither reader receives the `ExclusionSet`,
which stops at `npm/mod.rs` — so FR-011 requires threading it in, a signature
change on the yarn reader.

**The readers are mutually exclusive.** `npm/mod.rs:252-266` is an
`if/else-if` chain in precedence order package-lock → pnpm → bun → yarn, so
exactly one lockfile reader runs per project root. Two consequences. First,
SC-003's cross-reader comparison cannot be observed within one scan; it needs
one fixture directory per lockfile type over the same manifest, compared
across scans. Second, in a project carrying both `package-lock.json` and
`yarn.lock`, yarn is never read at all, so this feature changes nothing
there — worth stating in the tasks so a test is not written against a
project shape where the code under test cannot run.

## Complexity Tracking

No Constitution Check violations. Section intentionally empty.
