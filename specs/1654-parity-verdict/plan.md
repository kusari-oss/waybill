# Implementation Plan: A parity verdict per row, a test helper that can fail, and the closure parity gaps

**Branch**: `1654-parity-verdict` | **Date**: 2026-11-24 | **Spec**: [spec.md](spec.md)
**Input**: `specs/1654-parity-verdict/spec.md`. Refs #1695, #1688.

## Summary

**`parity-check --format json` gains a per-row `verdict`** (`pass`, `fail` or `not-checked`). It comes from the same rule that counts `summary.parity_gaps` (R1).

**Eleven test files share one helper** that fails on any `fail` verdict. Each file is proven able to fail by a mutation (R2).

**The gaps those tests and the closure goldens show are fixed at their causes**, measured before any design (R3):
- **The closure dependency edges (R4):** CycloneDX's extractor classifies an edge by its target, and SPDX types edges into artifact-absent closure members as build dependencies. Measured: the extractor change creates no new failure on any golden.
- **The completeness claim (R6, revised at T010):** the SPDX annotation now withholds the root claim exactly as CycloneDX does when the closure read has a gap. The original plan, redefining the extractors, rested on a wrong diagnosis.
- **The root's properties (R7):** CycloneDX's `metadata.component` keeps them.
- **The binary tests' SPDX 2.3 root edge (R8):** traced first, then fixed.
- **Closure A7:** traced per shape first, then fixed.

**G1 is a golden-masking artefact, not a defect (R3).** The non-closure B1/B2 causes (R5) and A8 are out of scope and filed.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde`/`serde_json`. **Zero new Cargo dependencies.**
**Storage**: N/A.
**Testing**:
- unit tests in `cli/parity_cmd.rs` (the verdict), `parity/extractors` (E1 and the B1/B2 rule) and `scan_fs` (the third typing pass);
- the shared helper in eleven integration files;
- eleven helper mutations, plus mutations of each fix;
- corpus regeneration through CI dispatch, every golden diff traced;
- before and after tables from `measurements/golden_parity.sh`.

**Target Platform**: every host.
**Project Type**: CLI (`waybill sbom parity-check`, `waybill sbom scan`).
**Performance Goals**: none affected.
**Constraints**:
- FR-002: no existing JSON key or table output changes.
- FR-007: documents change only through the FR-005 fixes, traced.

**Scale/Scope**:
- `cli/parity_cmd.rs`;
- `parity/extractors/{cdx,spdx2,spdx3}.rs` (B1/B2 rule, E1);
- `scan_fs/mod.rs` (`apply_lifecycle_scope_to_edges` pass 3);
- the CycloneDX metadata builder (R7);
- the SPDX 2.3 relationship emitter (R8);
- the licence emitters or extractors (closure A7, after its trace);
- `tests/common/parity.rs` (new), and eleven test files;
- `measurements/golden_parity.sh`;
- docs: `sbom-format-mapping.md` notes for B1/B2/E1, and the `parity-check` docs page for the field;
- CHANGELOG;
- the affected goldens, through CI.

## Constitution Check

| Principle | Assessment |
|---|---|
| III. Fail closed | Restored. Parity assertions that could not fail become ones that can (11 mutations). |
| IV. Type-driven correctness | The verdict is a closed three-value string from one code path. The edge rule is one function per format. |
| V. Specification compliance | SPDX `BUILD_DEPENDENCY_OF` is the native carrier for an edge into a build-only member. CycloneDX keeps its native `scope: excluded`. No new row. |
| VII. Test isolation | The helper is test-only, and runs the binary as every test already does. |
| IX. Accuracy | Each fix follows from a measured cause. G1 is not "fixed" in the product, because it isn't a product defect. |
| X. Transparency | Out-of-scope gaps are filed with their measurements, not allowlisted. |

**Post-design re-check:** unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1654-parity-verdict/
├── spec.md
├── plan.md
├── research.md          # R1–R10
├── data-model.md
├── quickstart.md
├── contracts/parity-check-json.md
├── measurements/        # golden_parity.sh, gap tables before (and after)
└── checklists/requirements.md
```

### Source Code (repository root)

```text
waybill-cli/src/cli/parity_cmd.rs                 # verdict
waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs   # B1/B2 target rule; E1
waybill-cli/src/scan_fs/mod.rs                    # apply_lifecycle_scope_to_edges pass 3
waybill-cli/src/generate/cyclonedx/{metadata,builder}.rs # R7 (exact site found by T-task)
waybill-cli/src/generate/spdx/relationships.rs    # R8 (after trace)
waybill-cli/tests/common/parity.rs                # new shared helper
waybill-cli/tests/{11 files}.rs                   # adopt it
```

## Complexity Tracking

No violations.

**Two decisions are left to measure-first tasks rather than this plan:**
- the closure A7 shapes;
- the R8 edge.

Each task names the cause before any change, as FR-001 requires.
