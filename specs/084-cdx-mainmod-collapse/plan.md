# Implementation Plan: CDX 1.6 main-module super-root collapse

**Branch**: `084-cdx-mainmod-collapse` | **Date**: 2026-05-08 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/084-cdx-mainmod-collapse/spec.md`

## Summary

Fix the CDX 1.6 emission path so that the project-root identifier is coherent across `metadata.component.bom-ref`, `dependencies[]`, and `compositions[]`. The two layers (`metadata.rs:391-409` milestone-053-aware vs `builder.rs:297-300` legacy-only) collapse onto a single identifier: when `main_module.is_some()` AND no `--root-component-name` override is active, `target_ref` becomes the main-module PURL (matching what `metadata.component.bom-ref` already carries). The legacy `<short-name>@0.0.0` short-form is preserved for the no-manifest fallback path (FR-004). The orphan `dependencies[]` bridge entry disappears (no longer synthesized by the `dependencies.rs:78-91` fallback because `target_ref` now has real outgoing edges from `relationships[]`); the two `compositions[]` entries that previously referenced the orphan retarget onto the PURL.

Net code change: ~10 lines in `cyclonedx/builder.rs:297-300` (compute `target_ref` from main-module-aware logic), plus a new closure-invariant regression test (~80 LOC), plus golden regeneration across post-053 ecosystem fixtures.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–083; no nightly).
**Primary Dependencies**: existing only — `serde`/`serde_json` (CDX JSON construction), `tracing`, `anyhow`, `thiserror`. **No new Cargo dependencies.** The `cyclonedx-bom` workspace dep is already in use; this milestone does not change which crates participate in CDX construction.
**Storage**: N/A — purely emission-time identifier-string transformation. No caches, no persistence.
**Testing**: `cargo +stable test --workspace` (existing harness) + new closure-invariant regression test at `mikebom-cli/tests/cdx_ref_closure_invariant.rs` (per-ecosystem table-driven). Goldens regenerated via existing `MIKEBOM_UPDATE_CDX_GOLDENS=1` env-var protocol.
**Target Platform**: Linux + macOS — same as the rest of mikebom (CDX emission is platform-agnostic; the fix touches only user-space `mikebom-cli/src/generate/cyclonedx/`).
**Project Type**: CLI — extends the existing `mikebom-cli` crate. No structure change.
**Performance Goals**: Emission stays within milestone-082 wall-time baseline. Removing one `dependencies[]` entry + retargeting two `compositions[]` entries has no measurable effect on emission speed; clippy + workspace test suite must complete within current CI budgets (~5 min lane per CLAUDE.md timing notes).
**Constraints**: Affected byte-identity goldens regenerate cleanly with diffs containing **only** (a) one fewer `dependencies[]` entry (the bridge entry is gone), (b) two retargeted `compositions[]` entries — no other field changes. SPDX 2.3 + SPDX 3 goldens stay byte-identical pre/post merge (FR-007).
**Scale/Scope**: ~10 LOC in `cyclonedx/builder.rs`; ~80 LOC new test file; ~6-8 byte-identity goldens regenerate (one per post-053 ecosystem fixture: Go / cargo / npm / pip-poetry / pip-pipfile / pip-plain / gem / maven, less those whose fixtures don't exercise the main-module path).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Compliance | Note |
|---|---|---|
| **I. Pure Rust, Zero C** | ✅ pass | Pure Rust diff in `mikebom-cli/src/generate/cyclonedx/builder.rs`. No new C, no FFI, no toolchain change. |
| **II. eBPF-Only Observation** | N/A | This milestone is SBOM emission, not dep discovery. eBPF surface untouched. |
| **III. Fail Closed** | N/A | Emission-time fix; no trace path involvement. |
| **IV. Type-Driven Correctness** | ✅ pass | The fix continues to pass `target_ref` as `String`, matching the existing convention; no new untyped boundaries introduced. The "rename / refactor `target_ref` to a `BomRef` newtype" cleanup is explicitly out of scope per spec (Out of Scope §3). New test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]` pattern per CLAUDE.md guidance. |
| **V. Specification Compliance** | ✅✅ pass — *this is what the milestone IS* | The fix brings CDX 1.6 emission into alignment with the `refLinkType` schema contract ("Descriptor for an element identified by the attribute 'bom-ref' in the same BOM document") and the field-level descriptions of `dependencies[].ref`, `dependsOn[]`, and `compositions[].assemblies[]`. **Standards-native-precedence audit**: no `mikebom:*` property is introduced or modified. The fix uses the standards-native `metadata.component.bom-ref` field (CDX 1.6) as the single coherent identifier — exactly the pattern Principle V's fifth bullet requires. |
| **VI. Three-Crate Architecture** | ✅ pass | No new crates. Change is within `mikebom-cli/`. |
| **VII. Test Isolation** | ✅ pass | New tests are pure-logic (string-set closure check on emitted JSON); no privileges required, runs under `cargo test --workspace` without root. |
| **VIII. Completeness** | N/A | Identifier hygiene, not dep discovery. |
| **IX. Accuracy** | ✅ pass — *improves accuracy* | Removes a phantom node (`hosted-guac-mgmt@0.0.0`) from emitted SBOMs. Reverse-impact analysis terminates correctly at the real root post-fix (US2). |
| **X. Transparency** | N/A | Fix removes confusion (the orphan); does not add transparency metadata. |
| **XI. Enrichment** | N/A | No enrichment surface touched. |
| **XII. External Data Source Enrichment** | N/A | No external sources touched. |

**Strict Boundaries**:

| # | Boundary | Status |
|---|---|---|
| 1 | No lockfile-based dependency discovery | ✅ unaffected |
| 2 | No MITM proxy | ✅ unaffected |
| 3 | No C code | ✅ unaffected |
| 4 | No `.unwrap()` in production | ✅ existing pattern preserved; new code uses `?` propagation; new test code uses the documented `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard per CLAUDE.md convention |

**Pre-PR gate** (CLAUDE.md mandatory): the milestone MUST land with both `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero errors AND zero warnings) AND `cargo +stable test --workspace` (`0 failed` per suite) green. SC-004 captures this; the implementation phase MUST run `./scripts/pre-pr.sh` before opening the PR.

**Gate decision**: PASS. No principle violations, no boundary breaches. Proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/084-cdx-mainmod-collapse/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── target-ref-derivation.md   # Phase 1 output — the single contract
├── checklists/
│   └── requirements.md  # spec quality checklist (already created)
└── tasks.md             # Phase 2 output (created by /speckit.tasks)
```

### Source Code (repository root)

The change is constrained to one module + one new test file:

```text
mikebom-cli/
├── src/
│   └── generate/
│       └── cyclonedx/
│           ├── builder.rs               # MODIFY — target_ref derivation (~10 LOC change at line 297-300)
│           ├── compositions.rs          # NO CHANGE — already correctly accepts target_ref
│           ├── dependencies.rs          # NO CHANGE — already correctly accepts target_ref; the orphan entry disappears as a natural consequence (the line-58 init now hits the same key as the line-65 relationships-populate, merging them; the line-78 fallback no longer fires because target_ref already has real outgoing edges)
│           └── metadata.rs              # NO CHANGE — milestone-053 logic at line 391-409 stays as-is; this is the source of truth that builder.rs converges to
├── tests/
│   └── cdx_ref_closure_invariant.rs     # NEW — closure-invariant regression test (FR-011)
└── tests/
    └── fixtures/
        └── transitive_parity/           # REUSE — milestone-083 fixtures already cover post-053 ecosystems (when 083 lands first); else use existing per-ecosystem fixtures from milestones 053/064/066/068/069/070
```

**Structure Decision**: Single-crate change in `mikebom-cli`. No workspace-level structure changes. The fix is the smallest possible surface area: one identifier-derivation conditional in `cyclonedx/builder.rs` plus one new test file. All surrounding infrastructure (compositions builder, dependencies builder, metadata builder, fixture goldens) stays.

## Complexity Tracking

> Filled only if Constitution Check has violations that must be justified.

No violations. This section intentionally empty.
