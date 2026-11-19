# Implementation Plan: Scope propagation protects every runtime project, not one per ecosystem

**Branch**: `1616-propagation-seeds` | **Date**: 2026-11-19 | **Spec**: [spec.md](spec.md)

## Summary

Spec 1269's immunity search (Phase A) starts from the primary root and one main module per ecosystem. Other projects' declared runtime dependencies are therefore unprotected, and propagation marks them non-runtime when a dev or docs component also reaches them. That is 11 packages on TheRock.

**The fix (R3).** Phase A also starts from every other main module that is runtime when propagation runs (R2). The seed set graph completeness uses, `ecosystems_with_root` and the edge exclusions are unchanged. By construction, scopes can only move toward runtime (R4). A new summary count records how many components the added seeds protected (R5).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- `resolve/scope_propagation.rs`: `propagate_non_runtime_scope`, `runtime_reachable`, `PropagationSummary`;
- `cli/scan_cmd.rs`, the call site;
- `project_roots::listed_ancestor_class`;
- `std::collections`, `tracing`.

**Zero new Cargo dependencies** (FR-009).

**Storage**: N/A. In process per scan.

**Testing**:
- **Unit tests** in `scope_propagation.rs`:
  - an added seed protects its closure;
  - a non-runtime main module is not a seed;
  - an added seed does not cross an excluded edge;
  - `immune_by_added_seeds` counts only what the added seeds alone protect;
  - with no added seeds, the result is identical to today's.
- **Integration:** `waybill-cli/tests/propagation_seeds.rs`, cases P-1..P-8 and P-rep (`contracts/wire.md`), asserted in three formats through the C42 and C62 extractors. P-7 is a byte-identity golden generated with `main`'s binary before any source change.
- **Mutations:**
  - no added seeds;
  - non-runtime main modules seeded;
  - dot-directory main modules seeded;
  - the excluded edges traversed from added seeds;
  - the count includes components the base seeds already protect;
  - `ecosystems_with_root` widened.
- **CI** (R6):
  - TheRock: two interleaved measure-source runs against `main`;
  - the public corpus: golden regeneration;
  - the quality corpus: against a `main` control.

  Every changed component is traced to its protecting seed.

**Target Platform**: the waybill CLI on Linux, macOS and Windows. No platform-gated code.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-005, ≤ 1.05× `main` on TheRock. The added work is one breadth-first search per scan (R5).

**Constraints**:
- FR-008: byte-identical output where each ecosystem has at most one main module;
- FR-004: graph completeness and the rows it feeds are unchanged;
- R4: scopes only move toward runtime.

**Scale/Scope**: TheRock has 64 main modules. 61 are runtime; 3 are non-runtime and excluded by FR-002.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies, no subprocess. |
| III. Fail closed | ✅ Under uncertainty the change keeps a component runtime, never demotes it. That is the direction spec 1269 names safe. |
| IV. Type-driven correctness | ✅ The added seeds are a separate set, so the graph-completeness seed set cannot be widened by accident. The summary count is a typed field. |
| V. Specification compliance | ✅ Native scope fields carry the change; C62 loses values but its value set is unchanged. No new row. |
| VII. Test isolation | ✅ Synthetic `waybill-fixture-*` trees; TheRock only in CI. |
| VIII. Completeness | ✅ No component is added or removed. |
| IX. Accuracy | ✅ Declared runtime dependencies stop being reported as non-runtime. |
| X. Transparency | ✅ The INFO summary reports what the added seeds protected (FR-006). C62 still marks every remaining propagated scope. |

Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1616-propagation-seeds/
├── spec.md
├── plan.md            # this file
├── research.md        # R1–R6
├── data-model.md
├── quickstart.md
├── contracts/wire.md  # P-1..P-8, P-rep
├── checklists/requirements.md
└── measurements/      # later: trace.md, mutations.md, therock.md, corpus.md, gate-log.md
```

### Source Code (repository root)

```text
waybill-cli/src/resolve/scope_propagation.rs   # added seeds in Phase A; immune_by_added_seeds
waybill-cli/src/cli/scan_cmd.rs                # compute the added seeds (R2) and pass them
waybill-cli/tests/propagation_seeds.rs         # P cases
waybill-cli/tests/fixtures/propagation_seeds/  # P-7 control tree and golden
specs/1615-nested-req-scope/spec.md            # remove FR-007/SC-002's exception (FR-007 here)
docs/reference/sbom-format-mapping.md          # C62 `transitive`: which roots protect from propagation
CHANGELOG.md
```

**Structure decision.** The added seeds are computed at the call site and passed as a separate argument. `component_root_seeds` stays shared with graph completeness and unchanged (R3).
