# Implementation Plan: `--no-binary-scan=<MODE>` flag

**Branch**: `665-no-binary-scan-flag` | **Date**: 2026-08-23 | **Spec**: [spec.md](./spec.md)
**Input**: `specs/665-no-binary-scan-flag/spec.md`

## Summary

Add a parameterized opt-in CLI flag `--no-binary-scan=<MODE>` to `waybill sbom scan` that gates registration of specific binary-content-scanning readers in the m664 shared-walker pilot. v1 recognizes mode `go` (skips the `go_binary` reader; unblocks the mongo residual perf gap identified in `specs/664-single-pass-walker/perf-comparison.md`). Future modes (`all`, `elf`, `symbols`) extend the enum without CLI-surface churn.

**Technical approach**: single-branch registration-skip in `run_shared_walker_pilot` (`waybill-cli/src/scan_fs/package_db/mod.rs`) predicated on the flag's enum value. Zero changes to the reader itself (`go_binary::registration` stays as-is; it just doesn't get registered when the flag is set). Emit a document-scope annotation `waybill:binary-scan-suppressed=<mode>` in every SBOM format (CDX / SPDX 2.3 / SPDX 3) via the existing document-scope annotation channel. Env-var equivalent `WAYBILL_NO_BINARY_SCAN=<mode>` mirrors the `WAYBILL_INCLUDE_VENDORED` precedent.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–664; no nightly required)
**Primary Dependencies**: Existing only — `clap` (workspace; `ValueEnum` derive for the mode enum), `serde`/`serde_json` (annotation values), `tracing` (FR-005 diagnostic log). **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; no persistence.
**Testing**: `cargo +stable test --workspace` (5183/0 baseline from m664 merge). New integration tests in `waybill-cli/tests/` for FR-001..FR-009 acceptance scenarios. SC-005 uses a pre-built Go binary fixture in the `kusari-oss/waybill-test-fixtures` sibling repo per m090 pattern.
**Target Platform**: macOS + Linux + Windows (waybill is cross-platform; the flag has no OS-specific behavior).
**Project Type**: CLI (waybill is a single binary in a Rust workspace).
**Performance Goals**: SC-001 mongo ≤ 700ms (down from 3.04s; ≥ 4× improvement). SC-002 pytorch ≤ 400ms (down from 1.12s). SC-003 ansible ≤ 300ms (down from 777ms).
**Constraints**: SC-004 requires byte-identity on the DEFAULT (flag-absent) path — workspace test 5183/0 must remain unchanged.
**Scale/Scope**: Single new CLI flag + one enum + one branch in the pilot registration + one annotation emission point per format (CDX / SPDX 2.3 / SPDX 3). ~150 LOC of production code + ~200 LOC of tests estimated.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Gate | Assessment |
|---|---|---|
| I. Pure Rust, Zero C | No new C dependencies allowed. | ✓ PASS — zero new dependencies; uses only workspace `clap` / `serde` / `tracing`. |
| II. eBPF-Only Observation | Runtime observation must be eBPF-only. | ✓ N/A — user-space CLI feature. |
| III. Fail Closed | Errors on ambiguous input, no silent degradation. | ✓ PASS — FR-009: unrecognized mode fails fast with non-zero exit. |
| IV. Type-Driven Correctness | Rust enums / newtypes over stringly-typed. | ✓ PASS — the mode is a `clap::ValueEnum`-derived enum, not a `String`. |
| V. Specification Compliance | Standards-native emission for SBOM fields. | ✓ PASS — the suppression annotation lands in the document-scope annotation channel already used by 100+ waybill annotations across CDX/SPDX 2.3/SPDX 3 (m071 parity extractor infrastructure). |
| VI. Three-Crate Architecture | Only `waybill-cli` / `waybill-common` / `waybill-ebpf`. | ✓ PASS — all changes in `waybill-cli`; touches `scan_fs/package_db/mod.rs` + CLI arg struct in `cli/scan_cmd.rs`. |
| VII. Test Isolation | Env-var mutations in tests must serialize via `EnvGuard`. | ⚠ ATTENTION — SC-006's two-fixture parity test (one run with `WAYBILL_NO_BINARY_SCAN=go`, one without) MUST use `crate::testing::EnvGuard::acquire()` per project memory `reference_podman_test_flake`. Recorded as a Phase 1 design constraint. |
| VIII. Completeness | Emit signal on gaps; no silent completeness loss. | ✓ PASS — FR-004: `waybill:binary-scan-suppressed=<mode>` annotation is the completeness-gap signal. |
| IX. Accuracy | No fabricated components. | ✓ PASS — suppression eliminates emissions; never fabricates. |
| X. Transparency | Diagnostic logs at every skip/degrade. | ✓ PASS — FR-005 requires the FR-009 shared-walker log to reflect the skipped reader; an INFO-level `binary-scan suppressed for mode X` log line fires at scan start. |
| XI. Enrichment | Reader emissions are always augmentative. | ✓ N/A — feature disables an emitter; doesn't enrich. |
| XII. External Data Source Enrichment | No new network-dep sources introduced. | ✓ N/A. |

**Gates pass. One attention item (Principle VII) becomes a design constraint recorded in `data-model.md` and enforced in the integration test scaffolding.**

## Project Structure

### Documentation (this feature)

```text
specs/665-no-binary-scan-flag/
├── plan.md              # This file
├── spec.md              # /speckit.specify output (already exists)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── cli-flag.md      # CLI flag surface contract
├── checklists/
│   └── requirements.md  # Existing spec-quality checklist
└── tasks.md             # /speckit.tasks output (not by this command)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   ├── cli/
│   │   └── scan_cmd.rs        # +1 flag on ScanArgs, +1 ValueEnum
│   ├── scan_fs/
│   │   ├── mod.rs             # thread `binary_scan_mode: Option<BinaryScanMode>` through scan_path()
│   │   └── package_db/
│   │       └── mod.rs         # branch inside run_shared_walker_pilot skip go_binary::registration()
│   └── generate/
│       ├── cyclonedx/         # emit waybill:binary-scan-suppressed as CDX metadata.property
│       ├── spdx/              # emit as SPDX 2.3 documentAnnotation
│       └── spdx3/             # emit as SPDX 3 annotation on SpdxDocument
└── tests/
    ├── no_binary_scan_us1_perf.rs      # SC-001/002/003 (env-gated)
    ├── no_binary_scan_us2_help.rs      # SC-006 --help visibility
    ├── no_binary_scan_us3_annotation.rs # SC-006 annotation present/absent parity
    └── no_binary_scan_scope.rs          # SC-005 fixture check + SC-007 error handling
```

**Structure Decision**: single-project CLI. All changes in `waybill-cli/`. No changes to `waybill-common/` or `waybill-ebpf/`.

## Complexity Tracking

*No constitution violations. No complexity items to justify.*

## Phase 0: Research (see `research.md`)

Six research questions to resolve before writing code:

- **R1**: `clap` value-enum shape — `#[arg(value_enum)] mode: Option<BinaryScanMode>` vs a `Vec<BinaryScanMode>` (repeatable) vs `BinaryScanMode::None` sentinel. Impacts FR-001's ergonomics.
- **R2**: env-var precedence when both `--no-binary-scan=<X>` and `WAYBILL_NO_BINARY_SCAN=<Y>` are set. Precedent from `WAYBILL_INCLUDE_VENDORED`.
- **R3**: annotation emission surface — which of the m071 parity-catalog rows this maps to (new C-row or reuse an existing one). Affects whether a m071 extractor needs adding.
- **R4**: SBOM document-scope annotation shape per format (CDX `metadata.property`, SPDX 2.3 `creationInfo.annotation`, SPDX 3 `Annotation` element). Where each lives in the emitter code.
- **R5**: sibling-repo fixture path convention — SHA-pinning + `fixture_path()` helper usage (m090 precedent).
- **R6**: how to preserve FR-003 byte-identity across the SC-004 workspace-golden suite (5183/0). Verification via the same `WAYBILL_UPDATE_*=0` methodology as SC-004 in m664.

## Phase 1: Design & Contracts (see `data-model.md`, `contracts/cli-flag.md`, `quickstart.md`)

Design deliverables:

1. **`data-model.md`** — the two entities:
   - `BinaryScanMode` enum (v1: `Go`; docstring reserves future variants).
   - `binary_scan_suppression` annotation contract (per-format serialization + value shape).
2. **`contracts/cli-flag.md`** — exact CLI flag surface, env-var contract, exit-code contract for FR-009 error path.
3. **`quickstart.md`** — 5-step operator recipe for adopting the flag (with/without env var, verify annotation, sanity-check perf).
