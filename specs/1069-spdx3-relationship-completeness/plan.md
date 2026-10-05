# Implementation Plan: SPDX 3 native dependency completeness

**Branch**: `1069-spdx3-relationship-completeness` | **Date**: 2026-10-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1069-spdx3-relationship-completeness/spec.md`

## Summary

SPDX 3 dependency relationships gain native `completeness`, decided by the same predicate CycloneDX `compositions[]` uses. The two formats agree by construction.

The work:
- **Shared predicate.** It is extracted from `build_compositions` into a shared `dependency_claims` (research R4), keeping CycloneDX byte-identical.
- **Ordering.** The completeness result is computed before SPDX 3 relationships are built (R5).
- **Grouping pass.** One post-pass over the final relationship list groups `dependsOn` per `(from, type, scope)`, sets `completeness`, and adds a `dependsOn → NoAssertionElement` relationship for unknown leaves (R6).

The pinned validator accepts every shape involved (measured, R1).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only: `serde_json`, `tracing`; the existing `hash_prefix` helper for deterministic IRIs. CI/test: the existing `spdx3-validate` 0.0.5 (milestone 078). **Zero new Cargo dependencies.**
**Storage**: N/A, in-process per scan.
**Testing**:
- in-crate tests for `dependency_claims` and the grouping pass, covering all six cases in the contract table;
- a cross-format agreement test (CycloneDX `compositions[]` against SPDX 3 `completeness`);
- the SPDX 3 conformance gate;
- golden regeneration (in-repo SPDX 3, plus corpus SPDX 3 via two regen runs) with a reviewed diff;
- `./scripts/pre-pr.sh`.

**Target Platform**: All.
**Project Type**: CLI (`waybill-cli`).
**Performance Goals**: Negligible: one pass over relationships, grouping by a hash map.
**Constraints**:
- CycloneDX and SPDX 2.3 byte-identical (FR-005);
- the completeness annotations unchanged (FR-004);
- no new flag;
- no new `waybill:` field.

**Scale/Scope**:
- `G/cyclonedx/compositions.rs` (+ `builder.rs`): extract `dependency_claims` and the degraded-ecosystem derivation;
- `G/spdx/v3_document.rs`: compute completeness earlier, and run the grouping pass;
- `G/spdx/v3_relationships.rs`: the grouping pass;
- SPDX 3 goldens (in-repo and corpus);
- `docs/reference/sbom-format-mapping.md`;
- `CHANGELOG.md`.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust | ✅ No new crates. |
| IV. Type-driven | ✅ Claims are a typed struct consumed by both emitters; completeness values come from one `match`. |
| V. Native fields first | ✅ The feature. SPDX 3's native `completeness` and `NoAssertionElement` carry what only `waybill:` annotations carried. The annotations stay, for SPDX 2.3 (no native construct) and cross-format parity. |
| VII. Test isolation | ✅ Fixture-built artifacts. The validator runs at CI/test time only. |
| IX. Accuracy | ✅ `complete` is set only on a relationship holding the whole dependency set (grouping), and only where CycloneDX also claims it. The unknown case uses SPDX's own "cannot determine" individual. |
| X / XII.3 Transparency | ✅ Completeness gaps become visible to standards-only readers. |
| Measurement rule | ✅ Validator behaviour, JSON-LD term expansion, the model text and the corpus claim distribution were all measured (`measurements/`). |

No violations.

## Project Structure

```text
specs/1069-spdx3-relationship-completeness/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/spdx3-completeness.md
├── measurements/  (relationship_shape.sh/.txt, probe_validator.py, validator.txt, cdx_claims.txt, README.md)
└── checklists/requirements.md
```

```text
waybill-cli/src/generate/cyclonedx/compositions.rs   # dependency_claims (shared), build_compositions uses it
waybill-cli/src/generate/cyclonedx/builder.rs        # degraded-ecosystem derivation moves beside it
waybill-cli/src/generate/spdx/v3_document.rs         # completeness computed before relationships; grouping pass invoked
waybill-cli/src/generate/spdx/v3_relationships.rs    # group_dependency_relationships
waybill-cli/tests/fixtures/golden/spdx-3/*           # regenerated
waybill-cli/tests/fixtures/public_corpus/*/spdx-3.json  # regenerated (two regen runs, reviewed)
docs/reference/sbom-format-mapping.md                # dependency-edge row, SPDX 3 column (FR-007)
CHANGELOG.md
```

## Delivery

One PR. The golden regeneration is the bulk of the diff, and its review is the main verification.

## Complexity Tracking

None.
