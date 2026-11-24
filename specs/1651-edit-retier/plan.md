# Implementation Plan: Relabel a component tier in an edited SBOM (`sbom edit --retier`)

**Branch**: `1651-edit-retier` | **Date**: 2026-11-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1651-edit-retier/spec.md`. Refs #1683, kusaridev/iac#3552.

## Summary

`waybill sbom edit` gains `--retier FROM=TO`, repeatable. It relabels `waybill:sbom-tier` on every component whose tier is `FROM`, and leaves the node, PURL, version, scope and edges untouched. Its purpose is testing consumers that act on the tier, such as the ingest rule in kusaridev/iac#3552, which drops `design` as if it were dev-only.

**What the change touches:**
- a new `EditOp::Retier` variant, applied in `edit::run`'s operation loop through a new `SbomAdapter::set_tier` (R1);
- a run-local set, so a component is relabelled at most once (R2);
- one C194 `operations` entry per mapping, with optional `from` and `to` (R3);
- a new protected per-component marker, **C243 `waybill:retiered-from`** (R4);
- the document type, recomputed in all three formats only when it was derived from the tiers (R5);
- completeness, which is never touched (R6).

**The emitter and the edit compute the type through one shared tier-string aggregator,** so they cannot drift (R5).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde`/`serde_json`, `clap` (the flag), `anyhow`. **Zero new Cargo dependencies.**
**Storage**: N/A. One document in, one document out.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`. The tests:
- unit tests in `edit/` and `cli/edit.rs`;
- a new integration file, `waybill-cli/tests/sbom_edit_retier.rs`;
- the C243 parity extractor's test;
- the extended `sbom_edit_parity.rs`;
- SC-001's measurement on three committed corpus goldens (read only);
- mutations.

**No corpus goldens change:** `sbom scan` is untouched, and the aggregator refactor is output-preserving, which T-tasks verify.

**Target Platform**: every host.
**Project Type**: CLI (`waybill sbom edit`).
**Performance Goals**: a linear pass over components; no measurable cost.
**Constraints**:
- FR-003: nothing but the tier, the marker, C194, the document type and signatures changes;
- byte identity of every existing edit output, because the new `OpReport` fields are optional.

**Scale/Scope**:
- `edit/mod.rs`, `edit/{cdx,spdx23,spdx3}.rs` (`set_tier`, setting the marker and the type);
- `cli/edit.rs` (the flag);
- `generate/lifecycle_phases.rs` (the shared tier-string aggregators);
- `parity/extractors/{mod,cdx,spdx2,spdx3}.rs` (C243);
- docs: `sbom-format-mapping.md` (C243, and C194's note), the `sbom edit` page, `sbom-types.md`;
- CHANGELOG.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate. |
| III. Fail closed | Pass. Malformed or unknown mappings are refused before anything is written (R7). The type is recomputed only when it was demonstrably derived (R5). |
| IV. Type-driven correctness | Pass. `TO` is parsed into the closed CISA vocabulary; mappings are a typed `EditOp`; no `.unwrap()` in production. |
| V. Specification compliance | Pass. The standards-native audit for C243 finds no per-component lifecycle field in CycloneDX 1.6, SPDX 2.3 or SPDX 3: KEEP-NO-NATIVE, as for C5 (R4). |
| IX. Accuracy | A relabel asserts nothing new about resolution. Completeness is untouched (R6), and the document says it was edited. |
| X. Transparency | C194 records each mapping and its counts. C243 says, on every relabelled component, what it was. Both are protected. |
| Strict boundaries | Pass. `sbom scan` is unchanged. |

**Gate: pass.** Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1651-edit-retier/
├── plan.md
├── research.md            # R1–R9
├── data-model.md
├── quickstart.md
├── contracts/cli.md       # the flag, the record, the marker, the cases
├── measurements/          # SC-001 script and results, mutations, gate log
├── checklists/requirements.md
└── tasks.md               # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/edit/mod.rs                      # EditOp::Retier, OpReport {from,to}, run loop, PROTECTED_ANNOTATIONS += C243
waybill-cli/src/edit/{cdx,spdx23,spdx3}.rs       # set_tier (tier + marker), recompute_document_type
waybill-cli/src/cli/edit.rs                      # --retier FROM=TO parser and validation
waybill-cli/src/generate/lifecycle_phases.rs     # *_from_tiers aggregators shared with the emitter
waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs   # C243
waybill-cli/tests/sbom_edit_retier.rs            # new
waybill-cli/tests/sbom_edit_parity.rs            # extended
docs/reference/sbom-format-mapping.md, docs/user-guide/<sbom edit page>, docs/reference/sbom-types.md, CHANGELOG.md
```

**Structure Decision**: the relabel is one more edit operation, inside the module that already owns the derivation record and the format adapters. The type computation is shared with the emitter rather than copied (R5).

## Complexity Tracking

No violations. One new catalog row (C243), confirmed in clarification.
