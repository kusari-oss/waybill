# Implementation Plan: npm-ecosystem optional-axis classification

**Branch**: `1272-npm-optional-scope` | **Date**: 2026-11-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1272-npm-optional-scope/spec.md`

## Summary

A dependency declared only under `optionalDependencies` is classified
optional when a project is scanned through `package-lock.json`, and left
unclassified by the other four npm-ecosystem readers. Four of the five
lockfiles record the axis; each records it somewhere different, and only
npm's happens to be where waybill looks. Consumers therefore cannot
distinguish "required at runtime" from "may be absent and the build
still works" in four of five dialects — which is the entire purpose of
the axis.

Technical approach, derived from measurement rather than from the
formats' documentation: **four arms, four different fixes**, shipped
cheapest first.

1. **pnpm v9** — one wrong source expression. The classifier is already
   complete; `is_optional` reads the `packages:` entry, and v9 moved the
   flag to `snapshots:`. Fix the source and the arm is done, transitive
   case included, because pnpm publishes the closure already computed.
2. **bun** — seed the optional axis from `workspaces.<p>.optionalDependencies`
   (which the existing tracker structurally cannot see) and replace the
   depth-1 `any_hard` guard with a transitive closure.
3. **yarn Berry** — add the lockfile's per-entry `dependenciesMeta` as a
   source alongside the manifest, correct the comment that asserts the
   lockfile does not carry it, and close.
4. **yarn v1** — take the root declaration from `package.json`, which
   measurement shows is its only possible source, and close over the
   per-entry sub-blocks the reader already collects.

The spec's single unmeasured assumption — whether a non-optional path
cancels optionality — was measured in Phase 0 before any design was
built on it. It holds; see research R1.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml` (1.98.1). No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde_yaml` (pnpm + Berry
lockfiles), `serde_json` (bun lockfile, yarn manifests), `regex` (yarn
v1 line format), `tracing` (FR-011a/FR-012 diagnostics), `anyhow`.
**Zero new Cargo dependencies** (FR-016).
**Storage**: N/A — all state in-process per scan, as with every reader
since milestone 002. The seeds and closures live inside one parse call
and die with it.
**Testing**: `cargo test --workspace --no-fail-fast` +
`cargo clippy --workspace --all-targets`, both via
`./scripts/pre-pr.sh`, with **no `+toolchain` argument** (#1189).
Fixtures are manager-produced (FR-013) with synthetic package names
(FR-015).
**Target Platform**: All host platforms; this is user-space reader work
with no platform-conditional code.
**Project Type**: Single Rust workspace — CLI + library.
**Performance Goals**: No regression outside the m669 harness's existing
25% noise budget. The added work is one extra graph closure per affected
reader per scan, over graphs already held in memory; for pnpm there is
no added traversal at all.
**Constraints**: The development axis must be byte-identical
(FR-010/SC-004) — the binding constraint, because three arms need the
runtime seed split out of a set the development-axis helper
deliberately merges.
**Scale/Scope**: 3 reader files, 4 independently shippable arms, ~5
manager-produced fixtures, 1 wrong code comment and 1 self-contradictory
fixture header to correct.

## Constitution Check

*GATE: evaluated before Phase 0, re-evaluated after Phase 1. Constitution v4.0.2.*

| Principle | Verdict | Basis |
|---|---|---|
| **I. Pure Rust, Statically Linked** | PASS | Zero new dependencies (FR-016). No C, no new transitive crates, no subprocess, no network. |
| **IV. Type-Driven Correctness** | PASS | Classification flows through the existing `LifecycleScope` enum. No new stringly-typed state. `clippy::unwrap_used` is denied in `waybill-cli` including `#[cfg(test)]`; test modules need the existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard. |
| **V. Specification Compliance** | PASS | Nothing new is emitted. The axis already uses standards-native constructs — CDX `scope: "excluded"`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` — with the `waybill:` annotation carrying only derivation provenance, which has no native slot. This is the ordering `feedback_native_fields_first` requires. |
| **VI. Three-Crate Architecture** | PASS | Changes confined to `waybill-cli/src/scan_fs/package_db/npm/`. `waybill-common` and `waybill-ebpf` untouched. |
| **VII. Test Isolation** | PASS | Pure-logic reader tests. No eBPF, no privileges, no network. |
| **VIII. Completeness** | PASS — and this is the principle the feature serves | The axis is currently a false *classification* rather than a false negative: the components are present but four of five dialects assert "required" where the honest answer is "optional" or "unknown". |
| **IX. Accuracy** | PASS | FR-012 drops a declaration naming an unresolved package with a diagnostic rather than fabricating a component. FR-006's cancellation, now measured, is what prevents over-claiming optionality. |
| **X. Transparency** | PASS — load-bearing | FR-011a is this principle applied to the exact failure being fixed. pnpm moved the flag and the reader answered "not optional" where it could only honestly answer "unknown", with nothing in the output distinguishing the two. The diagnostic makes an incomplete axis visible instead of silent. |
| **XII. External Data Source Enrichment** | PASS | Scan mode. The lockfile is the scanned evidence resolving what the manifest declares — constraint 1's permitted case. Constraint 2's provenance requirement is FR-008's derivation annotation. |

**Violations requiring justification**: none. Complexity Tracking is
omitted accordingly.

**Process gate — "measure external behaviour before designing around
it"** (CLAUDE.md): satisfied before Phase 1 rather than asserted. Every
claim about where a manager writes the axis is measured on five
installed managers; the cancellation rule was the one inference left in
the spec, and Phase 0 measured it rather than reasoning further about
it. Both probes are committed beside the spec so the findings are
reproducible when the formats move.

## Project Structure

### Documentation (this feature)

```text
specs/1272-npm-optional-scope/
├── spec.md                      # 3 clarifications integrated, 0 placeholders
├── plan.md                      # This file
├── research.md                  # Phase 0 — R1 settles the cancellation rule
├── data-model.md                # Phase 1 — in-process seeds and closures
├── quickstart.md                # Phase 1 — code map, traps, gates
├── contracts/
│   └── reader-optional-axis.md  # Phase 1 — C1..C12, per-dialect behaviour
├── checklists/
│   └── requirements.md          # From /speckit.specify
├── measurements/
│   ├── findings.md              # Probes A, B, C
│   ├── optional-axis-probe.sh   # where the axis lives
│   └── cancellation-probe.sh    # what it means (added in Phase 0)
└── tasks.md                     # Phase 2 — NOT created by /speckit.plan
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/npm/
├── pnpm_lock.rs      # US1 — build_snapshots_lookup:141, is_optional:384
├── bun_lock.rs       # US2 — workspace seed + transitive closure
├── yarn_lock.rs      # US3 Berry source union + FR-014 comment
│                     # US4 v1 root declaration + closure
├── peer_optional.rs  # unchanged — is_peer_optional must keep winning
└── mod.rs            # unchanged

waybill-cli/tests/            # manager-produced fixtures per arm
specs/1272-npm-optional-scope/measurements/   # the two probes
```

**Structure Decision**: No new modules. Each arm is a change inside the
reader that owns its dialect, which keeps the four arms independently
shippable and independently revertible. Research R7 explicitly defers a
shared closure helper: the three readers' edge maps are keyed
differently (`name@version`, key-path, bare name), so extracting one now
would require a normalisation layer that does not exist and that nothing
else needs — a speculative abstraction ahead of its third call site.

## Phase sequencing

Phase 0 and Phase 1 are complete. Phase 2 (`/speckit.tasks`) should
order the work as:

1. **Foundational** — per-dialect format-version reading for FR-011a,
   since all four arms need it and pnpm already has it.
2. **US1 pnpm v9** (P1) — the one-field source fix. Shippable alone.
3. **US2 bun** (P2) — workspace seed plus the first real closure.
4. **US3 yarn Berry** (P3) — source union, the FR-014 corrections, and
   the fixture whose manifest has no `dependenciesMeta`.
5. **US4 yarn v1** (P4) — manifest root plus closure over existing
   sub-blocks.
6. **Polish** — the five-dialect agreement test (SC-002/SC-003, the one
   contract no single-reader test can assert), and the SC-006
   load-bearing check on each arm.

The cancellation probe is **not** a Phase 2 task. The spec made it the
first task of the plan phase, and the plan phase ran it: research R1 and
Probe C of `measurements/findings.md` carry the result, and the spec's
Assumptions section no longer claims any unmeasured rule.

## Post-Design Constitution Re-Check

Re-evaluated against the Phase 1 artifacts. No verdict changed, and no
new dependency, crate boundary, emitted field or privileged test was
introduced by the design. The data model adds no persisted type; the
contract adds no output format. One risk is worth carrying forward in
writing rather than discovering later:

> `v9_importer_scopes` merges `dependencies` and `optionalDependencies`
> into one `protected` set. Correct for the development axis, fatal for
> the optional one — the cancelling set would contain the
> classification subjects. Any arm reusing those seeds must split them,
> and FR-010 requires the development result stay byte-identical while
> doing so.

That is the single place where this feature can silently break
something that works today, and C5 of the contract is the assertion
that catches it.
