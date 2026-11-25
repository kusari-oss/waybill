# Implementation Plan: Run test binaries in parallel (nextest trial)

**Branch**: `1658-nextest-trial` | **Date**: 2026-11-25 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1658-nextest-trial/spec.md`

## Summary

Replace `cargo test --workspace --no-fail-fast` with `cargo nextest run --workspace --no-fail-fast`, plus a separate `cargo test --workspace --doc` step, in three places:
- `scripts/pre-pr.sh`;
- the Linux, macOS and Windows `Tests` steps in `ci.yml`;
- the constitution's Pre-PR table, amended first.

Before adopting, measure both runners on the same machine (3 runs each), and resolve every failure that only nextest shows. Extend `release_gates.rs` with mutation tests proving that the new positive signal fails on a failing test and on a non-compiling build.

## Technical Context

**Toolchain:**
- **Language:** Rust stable, pinned 1.98.1 by `rust-toolchain.toml`. No `+toolchain`.
- **Dependencies:** none new. cargo-nextest 0.9.152 is a tool, installed locally by the developer and on CI through `taiki-e/install-action`, SHA-pinned (R3).
- **Storage:** N/A.

**Testing and platforms:**
- **Testing:** this feature is the test runner. The gate's own tests are in `waybill-cli/tests/release_gates.rs`.
- **Target platforms:** macOS (local), and the CI lanes ubuntu-latest, macos-latest and windows-latest (non-blocking).
- **Performance goal:** report-only (SC-001 to SC-003). Adopt only if nextest's local median is lower.

**Constraints and scale:**
- **Constraints:** a positive signal on both the tests and the doctests; no-fail-fast; no silent fallback; nothing serialised without a reproduced failure.
- **Scale:** about 437 test binaries and 11,585 tests (#1718).

## Constitution Check

| Principle / rule | Status |
|---|---|
| I. Pure Rust, statically linked | Pass. No shipped-binary change. |
| III. Fail Closed | Pass. A missing nextest fails `pre-pr.sh`, and the gate tests fail rather than skip when it's missing (R6). |
| VII. Test Isolation | Pass. Unprivileged tests stay unprivileged. Nextest runs one process per test, which is more isolation, not less. |
| Development Workflow: Pre-PR Verification table | **Changes.** It names `cargo test --workspace --no-fail-fast`. Amend to 4.1.0 in this PR, before the scripts change (R4). Justified: the table is what the scripts must match, so it changes first. |
| CLAUDE.md "Pre-PR verification" | Update in the same commit as the constitution. |
| Zero new Cargo dependencies | Pass. |
| Actions security defaults | Pass. SHA-pinned action, tool version pinned. |

Re-checked after design: no new violations.

## Project Structure

### Documentation (this feature)

```text
specs/1658-nextest-trial/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/gate.md
├── checklists/requirements.md
└── measurements/          # baseline, comparison, CI runs, isolation findings
```

### Source Code (repository root)

```text
.specify/memory/constitution.md   # Pre-PR table, 4.0.1 → 4.1.0
CLAUDE.md                         # Pre-PR verification section
.config/nextest.toml              # new: default profile; test groups only if a failure demands one
scripts/pre-pr.sh                 # test step → nextest + doctest step; missing-nextest check
.github/workflows/ci.yml          # install step + Tests step on linux/macOS/windows; install only on the eBPF lane
waybill-cli/tests/release_gates.rs   # nextest gate mutation tests
waybill-cli/tests/**              # only tests whose nextest-only failure needs a fix
```

**Structure Decision**: no new crates. The single new config file is `.config/nextest.toml`.

## Phases

1. **Baseline:** `cargo test --workspace --no-fail-fast` on `main`'s tree, 3 runs, idle. Record wall time, the per-binary summed time, passed/ignored counts per package, and doctest counts. Also time `pre-pr.sh`, 3 runs. Collect the CI baseline: 2 runs per lane, the existing run 45695514065 plus one dispatch.
2. **Compare:** `cargo nextest run --workspace --no-fail-fast`, 3 runs. Diff the counts (FR-011). Write up every nextest-only failure with a reproduction, then fix it or put it in a group. Run the per-binary comparison for the 16 lazy-static files (FR-007).
3. **Adopt:** amend the constitution and CLAUDE.md, then change `pre-pr.sh`, `ci.yml` and `.config/nextest.toml`.
4. **Gate tests:** add mutation tests to `release_gates.rs` (contract C3).
5. **Measure after:** 3 local runs of `pre-pr.sh`, and at least 2 CI runs per lane. Check the #1716 condition (spec edge case).

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|---|---|---|
| Constitution amendment | The Pre-PR table names the runner | Leaving it would make the table and the scripts disagree, which is exactly what the table exists to prevent |
