# Implementation Plan: Dynamic linkage and embedded source paths are labelled binary analysis

**Branch**: `1648-binary-kind-technique` | **Date**: 2026-11-23 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1648-binary-kind-technique/spec.md`. Refs #1668, #1665; tracked in #1543.

## Summary

Spec 1646 labels two binary evidence kinds `binary-analysis` through a private list, `deduplicator::BINARY_KINDS`. This plan replaces that list with one decision, `binary_kind_technique(kind)`, defined over the existing, drift-tested `BINARY_EVIDENCE_KINDS` (research R1):
- `dynamic-linkage` and `embedded-source-path` become `binary-analysis` (FR-002);
- embedded manifests stay `manifest-analysis` (FR-003);
- the two path-identified collapse kinds are recorded as unchanged, so the decision is total (FR-006).

The deduplicator's absorption gate reads the same decision, so merges follow automatically (FR-004). The Go census label is corrected by counting build info where `go_binary.rs` emits it (R3, FR-005).

The order of work:
1. **Census first.** A log-only commit adds `kind_technique_change`, `kind_merge` and the corrected Go events, with output unchanged (R4).
2. **The census.** TheRock, the quality corpus and the public corpus are counted, with real builds on the runner and synthetic fixtures as positive controls (R5).
3. **Wire.** Both sites read `binary_kind_technique`, and `BINARY_KINDS` and the log-only events are removed.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only. `serde_json` and `tracing` for the census. **Zero new Cargo dependencies.**
**Storage**: N/A.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`.
- `s1648_*` unit tests: the decision's totality, the dedup gate.
- A new integration file `waybill-cli/tests/binary_kind_technique.rs` (contracts/wire.md K1–K7), with a new test-only `DT_NEEDED` builder.
- Mutations M1–M5 (R7).

**Target Platform**: every host. The decision is per evidence kind, not per format.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: SC-006 (1.05×). The change is a string match per converted component.
**Constraints**:
- byte identity for every other kind (FR-008);
- no new field (FR-007);
- census before output (FR-001).

**Scale/Scope**: `scan_fs/binary/mod.rs` (the decision), `scan_fs/mod.rs` and `nix/closure/contents.rs` (conversion), `resolve/deduplicator.rs` (the gate and the census), `package_db/go_binary.rs` (the census), one test helper, one test file, the fixture repository's census and control, docs and CHANGELOG.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate, no subprocess. |
| IV. Type-driven correctness | Pass. The decision returns the existing `ResolutionTechnique`. Its totality over the list is tested, and the list's completeness is drift-tested (R1). No `.unwrap()` in production. |
| V. Specification compliance | Pass. Each label follows CycloneDX 1.6's documented definitions, quoted in the spec. |
| IX. Accuracy | The purpose: evidence stops claiming a package manager listed what a binary's structure revealed. |
| X. Transparency | Pass. The census's Go label is corrected so it counts what it names. |
| Strict boundaries | Unaffected (scan mode). |

**Gate: pass.** Re-checked after Phase 1: unchanged. Replacing two lists with one decision removes a duplication rather than adding a mechanism.

## Project Structure

### Documentation (this feature)

```text
specs/1648-binary-kind-technique/
├── plan.md
├── research.md            # R1–R7
├── data-model.md
├── quickstart.md
├── contracts/wire.md      # K1–K7, census shapes
├── checklists/requirements.md
└── tasks.md               # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/binary/mod.rs          # binary_kind_technique; totality test
waybill-cli/src/scan_fs/mod.rs                 # entry_technique; kind_technique_change census; detected-go label
waybill-cli/src/scan_fs/package_db/nix/closure/contents.rs   # same conversion
waybill-cli/src/resolve/deduplicator.rs        # effective_technique / is_binary_side read the decision; kind_merge census; BINARY_KINDS removed
waybill-cli/src/scan_fs/package_db/go_binary.rs # go-buildinfo census at emission
waybill-cli/tests/binary_kind_technique.rs     # new: K1–K7
waybill-cli/tests/common/installed_lib.rs      # new test-only elf_with_needed
docs/reference/binary-identity.md, docs/reference/sbom-format-mapping.md (D1), CHANGELOG.md

kusari-sandbox/waybill-fixture-therock-sdk:
├── census/versions.py                          # kind_merge, detected-go, go-buildinfo
└── .github/workflows/census-versions.yml       # control: gcc dynamic program, go build
```

**Structure Decision**: the decision lives beside `BINARY_EVIDENCE_KINDS`, the list it is total over, so the drift test and the totality test together cover every binary kind (R1).

## Complexity Tracking

No violations.
