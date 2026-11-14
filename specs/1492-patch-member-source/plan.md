# Implementation Plan: Locally built patches are sources, not fragments

**Branch**: `1492-patch-member-source` (stacked on `1367-closure-edge-count`, #1509) | **Date**: 2026-11-13 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1492-patch-member-source/spec.md`

## Summary

A closure member that another derivation lists in its `patches` attribute is a patch, and therefore C206 `source`, whatever its name (FR-001). The check moves ahead of `env.text` in the classifier's fixed evidence order (FR-010).

The endpoint helper must then take the closure's patch set, which it ignores today (research R2). Otherwise the classifier, the projection and the emitter would disagree about the 5 members.

The edge change is measured with the #1367 accounting on the captured moat closure and reviewed against the regenerated goldens (FR-005, FR-008).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only. **Zero new Cargo dependencies.**
**Storage**: N/A, in-process per scan.
**Testing**: `cargo test`. Unit tests in `nix/closure/member.rs` and `graph.rs`. The env-gated `s1367` accounting runs on the captured closure, and the public-corpus goldens are regenerated in CI.
**Target Platform**: as today.
**Project Type**: CLI (single binary).
**Performance Goals**: no measurable change. One extra `PatchOutputs::of` pass per closure, at the projection and at endpoint filtering. The classifier already builds the set once per emission.
**Constraints**: the C206 values stay the five frozen values. No new annotation.
**Scale/Scope**: 5 of 1,749 derivations on the pinned moat closure (research R1). 5–6 patch-named fragments per closure golden (R6, name-matched).

## Constitution Check

| Principle | Assessment |
|---|---|
| V. Specification Compliance (standards-native first) | Pass. No new `waybill:` row; an existing value changes. |
| VII. Test Isolation | Pass. Unit tests use constructed closures; the real-closure measurement is env-gated and evaluates nothing. |
| VIII. Completeness | Pass. Each reclassified member gains the edges nix recorded to it, instead of being walked through. |
| IX. Accuracy | Pass, and the point of the feature. "A live-bootstrap C function" stops being said of patches. |
| X. Transparency | Pass. C184 counts are read back from the document, and the measurement is committed. |
| III. Fail Closed / IV. Type-Driven | Pass. The classifier stays total over a closed enum, and the endpoint answer gets one source of truth (R2). |

No violations, so Complexity Tracking is empty. Re-checked after design: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1492-patch-member-source/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/          # created by the tasks: before/after accounting, golden review
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/closure/
├── member.rs     # classify(): patches rung before env.text; is_edge_endpoint(drv, &PatchOutputs)
├── graph.rs      # stops()/nearest_stopping()/project(): thread the closure's PatchOutputs
└── emit.rs       # emitted_drv_keys(): pass PatchOutputs::of(raw)
docs/reference/sbom-format-mapping.md   # C206 row
docs/reference/nix-evaluation.md        # role table, evidence order, the #1502 sentence
CHANGELOG.md
waybill-cli/tests/fixtures/public_corpus/nix-closure-*/   # regenerated in CI
```

**Structure Decision**: Single project. Three files in the closure tier change, plus docs and goldens. No new module.

## Approach

1. **Classifier.** Move the `patches.lists(drv)` check from the version-name rung to directly after the content-hash rung.
2. **One source of truth for endpoints.**
   - `is_edge_endpoint` takes `&PatchOutputs`.
   - `project` builds `PatchOutputs::of(closure)` once and passes it to `stops` through `nearest_stopping`.
   - `emitted_drv_keys` builds it once from `raw`.
   - The existing `edge_endpoint_keys_are_a_subset_of_emitted` invariant stays.
3. **Tests.**
   - Rewrite the three spec 1380 tests that pin edge neutrality (research R4).
   - Add `s1492` tests for the new rung, the generated-patch case, and helper/classifier agreement with the patch set.
   - Run a mutation that drops the new rung.
4. **Measure.**
   - Run the `s1367` accounting on the captured moat closure on the base commit and on the change.
   - Run `debug closure-dump` before and after.
   - Commit both under `measurements/`.
5. **Goldens.**
   - Dispatch the corpus with `regen_goldens=true`.
   - Run a normalized, sorted diff per target, reviewed member by member and edge by edge.
   - Commit the goldens with the review.
6. **Docs.** The C206 row, `nix-evaluation.md` and the CHANGELOG.

## Complexity Tracking

None.
