# Implementation Plan: a marker for a version that was never there to find

**Branch**: `1156-version-absence-marker` | **Date**: 2026-10-16 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1156-version-absence-marker/spec.md`

## Summary

Emit `waybill:version-absence` = `by-construction` on the synthetic Pants
resolve anchors from both the Python and JVM lockfile readers, register it as
catalogue row C197 with its three per-format extractors, teach invariants I7
and I8 to accept it, and delete the four Pants I8 declarations. Add a
committed must-pass registry so that a cell's two green spellings — the
property holds versus the property never applied — stop being
indistinguishable.

Delivered as two independently-green changes (FR-017): emission plus
goldens, then acceptance plus registry plus tests. The split works because
until acceptance widens, the four cells keep failing with exactly the kind
their declarations pin, so the declarations stay accurate rather than stale.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (annotation values, document walking), `toml = "0.8"` (the new registry file; already parses the declarations file). **Zero new Cargo dependencies.**
**Storage**: Two committed TOML files under `waybill-cli/tests/corpus_harness_195/` — the existing `invariant-declarations.toml` (four rows removed) and the new `invariant-must-pass.toml`. Twelve refreshed golden documents. No runtime state.
**Testing**: `cargo test --workspace --no-fail-fast` for the hermetic matrix (reads committed goldens, no network); the `public-corpus.yml` dispatch for the golden refresh and the unfiltered-document lane.
**Target Platform**: Host-agnostic; the matrix is hermetic and the readers are filesystem-only.
**Project Type**: CLI / library — SBOM generator plus its test harness.
**Performance Goals**: N/A. One additional per-component annotation on 7 of ~3000 corpus components; no new traversal, subprocess or I/O.
**Constraints**: No network or filesystem access added to the matrix (FR-016, gated by the existing `no_new_io.rs`). Goldens never generated locally (FR-014). Each change green on its own (FR-018).
**Scale/Scope**: 7 anchor components, 4 of 17 corpus targets, 12 golden files, 1 catalogue row, 3 extractors, 2 checker edits, 1 new registry with its loader and validator.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Basis |
|---|---|---|
| I. Pure Rust, statically linked | **PASS** | No new dependency, no C, no subprocess. |
| III. Fail closed | **PASS** | Every new gate fails the lane rather than warning: a stale declaration, a registry entry that stops passing, a registry/declaration overlap, an unrecognised marker value. |
| IV. Type-driven correctness | **PASS** | The marker's value is a pinned constant, not a free string; the registry is a deserialized struct validated before use. `clippy::unwrap_used` applies to the new test code, so test modules carry the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard the harness already uses. |
| V. Specification compliance — **standards-native precedence** | **PASS, audit performed** | No native carrier in CDX 1.6, SPDX 2.3 or SPDX 3.0.1. Audited against the vendored schemas, not from memory; result in [research.md](./research.md) R1 and cited in spec FR-005a. |
| VI. Three-crate architecture | **PASS** | Changes confined to `waybill-cli` (readers, parity, tests). `waybill-common` and `waybill-ebpf` untouched. |
| VII. Test isolation | **PASS** | The matrix stays hermetic; no new env-var mutation, so no `EnvGuard` participation needed. |
| VIII. Completeness | **PASS** | The feature's entire purpose is to make an existing silence explicit. |
| IX. Accuracy | **PASS** | Nothing is fabricated. The marker asserts the absence of a version, which is observably true of an anchor, and the anchors' `version` field is unchanged. |
| X. Transparency | **PASS** | The marker is itself a transparency signal; the registry makes "this cell is load-bearing" a reviewable committed fact. |

**No violations.** Complexity Tracking is therefore empty and omitted.

One gate needed work rather than a tick: Principle V requires the audit
result to be cited **in the spec's Functional Requirements**, and the first
draft only required the catalogue row to contain it. Spec FR-005a was added
during this phase to carry the result itself.

## Project Structure

### Documentation (this feature)

```text
specs/1156-version-absence-marker/
├── plan.md                        # This file
├── research.md                    # Phase 0 — R1 Principle V audit, R2-R5
├── data-model.md                  # Phase 1 — the registry and the marker
├── quickstart.md                  # Phase 1 — how to run and refresh
├── contracts/
│   ├── c197-version-absence.md    # The wire contract
│   └── must-pass-registry.md      # The registry file contract
├── checklists/requirements.md     # From /speckit.specify
└── tasks.md                       # /speckit.tasks — not created here
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── pants_resolve.rs               # + VERSION_ABSENCE_KEY / BY_CONSTRUCTION consts
├── pants/lockfile.rs              # resolve_component_entry:382 — set the marker
└── pants_jvm/lockfile.rs          # resolve_component_entry:495 — set the marker

waybill-cli/src/parity/extractors/
├── mod.rs                         # + C197 ParityExtractor row, + 3 import entries
├── cdx.rs                         # + cdx_anno!(c197_cdx, …, component)
├── spdx2.rs                       # + spdx23_anno!(c197_spdx23, …, component)
└── spdx3.rs                       # + spdx3_anno!(c197_spdx3, …, component)

waybill-cli/tests/corpus_harness_195/
├── invariant-declarations.toml    # − 4 Pants I8 rows (image-postgres16 stays)
├── invariant-must-pass.toml       # NEW — seeded with the 4 Pants I8 cells
└── invariants/
    ├── must_pass.rs               # NEW — loader, validator, lane pass
    ├── mod.rs                     # + module decl, + must-pass pass in the lane
    ├── checks/versionless_explained.rs   # I8 accepts the marker; rationale rewritten
    ├── checks/orphan_explained.rs        # I7 accepts the marker
    └── mutation_tests.rs          # + named-target control, + marker mutations

docs/reference/
├── sbom-format-mapping.md         # + C197 row
└── sbom-invariants.md             # I8's note rewritten (prose, not a gated field)

waybill-cli/tests/fixtures/public_corpus/{pants-clojure-polyglot,
  pants-example-django,pants-example-jvm,pants-example-python}/
  {cdx,spdx-2.3,spdx-3}.json       # 12 refreshed goldens
```

**Structure Decision**: No new modules outside the harness. The marker's
constants live beside the other shared Pants annotation keys in
`pants_resolve.rs`, which both readers already import, so neither reader
owns a string the other must match by hand. The must-pass registry gets its
own file (`must_pass.rs`) mirroring `declarations.rs` rather than extending
it, because the two files make opposite claims and the existing
`verdict()` state machine stays a pure two-input function — see research R2.

## Phase sequencing

Change one, **emission** (US1):

1. Constants in `pants_resolve.rs`; set the annotation at both
   `resolve_component_entry` sites.
2. C197 row in `sbom-format-mapping.md` carrying the R1 audit.
3. Three extractor macros, one registry row, three import-list entries.
4. Unit tests: both readers emit it; a non-anchor entry does not.
5. Refresh the 12 goldens by dispatch; review with `xtask corpus-diff`;
   confirm the diff is the marker and nothing else (FR-015).
6. Gate: declarations untouched, all five I8 rows still accurate, lane green.

Change two, **acceptance** (US2, US3):

7. `is_explained` in both checkers accepts the pinned key/value pair;
   rewrite the rationale in both and in `sbom-invariants.md` (FR-008).
8. Delete the four Pants I8 declarations; keep `image-postgres16`.
9. `must_pass.rs` — struct, loader, validator, and the lane pass; seed
   `invariant-must-pass.toml` with the four cells and their reasons.
10. Positive control naming the four targets; mutation tests for a removed
    marker, an unrecognised value, and a registered cell forced to
    `NotApplicable`.
11. Gate: lane green against goldens already on the default branch.

## Complexity Tracking

Not applicable — the Constitution Check records no violations.
