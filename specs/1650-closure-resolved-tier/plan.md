# Implementation Plan: Closure-resolved Haskell components are not labelled `design`

**Branch**: `1650-closure-resolved-tier` | **Date**: 2026-11-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1650-closure-resolved-tier/spec.md`. Refs #1681, #966, #1282.

## Summary

Under `--nix-closure`, two groups of Haskell components keep the manifest reader's `design` tier after the closure has resolved them:
- **the project's own package,** which the closure evaluated;
- **boot libraries of the closure's compiler,** which the project's manifest also declared.

**This is not an unpinned closure.** Every closure records its nixpkgs revision, and about 99% of members are resolved.

**The tier is decided by emission order** (research R1). The closure synthesises a boot library as `build` only if no component already holds its PURL. Milestone 966 deliberately left a declared one `design`. And the closure's scope pass marks the evaluated package without touching its tier.

**Because these few components are the graph's hubs,** a consumer dropping `design` loses every component in four of five closure documents.

**The plan:**
- **One pass after the closure scope pass** re-tiers a `design` component to `build` when it carries C203 `exempt-evaluated-package` (FR-002) or C218 `boot-library-compiler` (FR-003) (research R2–R4). The evidence already on each component is the recorded reason, so no new row is added (R5).
- **A new corpus invariant, I11,** fails CI if either group goes back to `design`, or if the root's direct dependencies are all `design` (R6).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde_json` (I11 reads documents), `tracing` (one INFO line). **Zero new Cargo dependencies.**
**Storage**: N/A. In process, per scan.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`. The tests:
- unit tests in the new pass;
- an integration file, `waybill-cli/tests/closure_resolved_tier.rs`, on the stub-`nix` harness;
- I11's own unit tests, plus its positive control on `main`'s goldens;
- mutations;
- the corpus, regenerated through CI.

**Target Platform**: every host. Closure fixtures are `#![cfg(unix)]`, as the existing closure tests are.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: one linear pass over components; no measurable cost.
**Constraints**:
- byte identity without `--nix-closure` (FR-007);
- tier only: no PURL, version, scope or edge changes (spec Assumptions);
- no new catalog row (R5).

**Scale/Scope**:
- `resolve/closure_resolved_tier.rs` (new, about 100 lines with tests);
- one call in `cli/scan_cmd.rs`;
- a doc-comment update in `nix/haskell_packages/mod.rs::apply_compiler_version`;
- I11 in the corpus harness;
- docs: C203, C218, `sbom-types.md` and `sbom-invariants.md`;
- the CHANGELOG.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate. |
| III. Fail closed | Pass. A component is re-tiered only on evidence already on it: C203 or C218. Without it, the tier is unchanged (FR-004). |
| IV. Type-driven correctness | Pass. The tier is the existing field; the rule reads closed-enum values; no `.unwrap()` in production. |
| V. Specification compliance | Pass. The standards-native audit (R5) finds no per-component lifecycle field in CycloneDX 1.6, SPDX 2.3 or SPDX 3; the tier vocabulary is CISA's, per `sbom-types.md`. |
| VIII. Completeness | The point: a design-dropping consumer keeps 1,134 / 1,362 / 1,235 / 2,441 components instead of 0 (R7). |
| IX. Accuracy | `build` is what the closure evidences: the build graph. The 966 rule produced two tiers for one library. |
| X. Transparency | The reason is the evidence already on the component (C203, C218), now documented as implying `build`. |
| Strict boundaries | Pass. Manifest-only scans are untouched by construction: only the closure writes C203 and C218. |

**Gate: pass.** Re-checked after Phase 1: unchanged. **One recorded reversal:** FR-003 supersedes milestone 966's clarification of 2026-11-12, at #1681's request. Research R2 gives the reasoning.

## Project Structure

### Documentation (this feature)

```text
specs/1650-closure-resolved-tier/
├── plan.md
├── research.md            # R1–R8
├── data-model.md
├── quickstart.md
├── contracts/wire.md      # fixture cases and the I11 contract
├── measurements/          # T001 path census, goldens, mutations, gate log
├── checklists/requirements.md
└── tasks.md               # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/resolve/closure_resolved_tier.rs          # new: retier() and its unit tests
waybill-cli/src/resolve/mod.rs                            # register the module
waybill-cli/src/cli/scan_cmd.rs                           # call retier() after assign_role_scope
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs   # apply_compiler_version doc comment
waybill-cli/tests/closure_resolved_tier.rs                # new: fixture cases
waybill-cli/tests/corpus_harness_195/invariants/checks/closure_resolved_tier.rs  # new: I11
waybill-cli/tests/corpus_harness_195/invariants/{mod.rs,checks/mod.rs}           # register I11
docs/reference/{sbom-format-mapping.md,sbom-types.md,sbom-invariants.md}, CHANGELOG.md
```

**Structure Decision**: the rule lives in one resolve-stage pass (R4), placed after the closure scope pass whose C203 it reads, so both FR-002 and FR-003 are decided in one place, and nothing runs without a closure.

## Complexity Tracking

No violations.
