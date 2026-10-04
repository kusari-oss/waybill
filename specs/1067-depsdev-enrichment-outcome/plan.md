# Implementation Plan: Say why deps.dev did not enrich a component

**Branch**: `1067-depsdev-enrichment-outcome` | **Date**: 2026-10-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1067-depsdev-enrichment-outcome/spec.md`

## Summary

The deps.dev fetch path stops collapsing outcomes into `Option<VersionInfo>`:
- **Three-way result:** found, absent and failed are kept apart (R2).
- **Two batch-path defects fixed first:** unanswered slots and duplicate coordinates were recorded and cached as absent (FR-010).
- **Declined:** a record whose licences all fail SPDX canonicalisation is `declined` (R3).
- **No placeholder requests:** placeholder versions are not sent and become `not-queried:incomplete-coordinate` (R6).
- **Emission:** each component in deps.dev's six ecosystems that it did not enrich carries C191 `waybill:deps-dev-outcome`, and the document carries C192 `waybill:deps-dev-outcomes` counts, including unsupported ecosystems. Both appear only on online scans.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only: `reqwest`, `serde`/`serde_json`, `spdx` (via `SpdxExpression::try_canonical`), `tracing`; dev: existing mock HTTP server used by `depsdev_source.rs` tests. **Zero new Cargo dependencies.**
**Storage**: deps.dev disk cache, format unchanged (R4).
**Testing**: In-crate tests against a local mock deps.dev (R8); generate-layer emission tests in all three formats; parity extractors; `./scripts/pre-pr.sh`; a read-only corpus run (byte-identical); live checks via the committed probes.
**Target Platform**: All.
**Project Type**: CLI (`waybill-cli`).
**Performance Goals**: Fewer requests: placeholder versions are no longer sent (−28 on opentelemetry-go). No other change.
**Constraints**: FR-009 byte-identity for offline, disabled and fully matched scans. No deps.dev content in the document (Q2). No new flag or environment variable.
**Scale/Scope**: `E/depsdev_source.rs` (fetch result, batch slots, classification, annotation), `E/request_key.rs` (placeholder skip), a shared placeholder predicate (moved from `scan_fs/mod.rs`), generate (C192 in three formats), parity (C191, C192), the catalogue, CHANGELOG.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust | ✅ No new crates. |
| IV. Type-driven | ✅ The outcome is an enum; the fetch result becomes a three-way type in place of `Option`. |
| VII. Test isolation | ✅ CI tests use a local mock; live deps.dev only in the measurement probes. |
| IX. Accuracy | ✅ FR-010 removes two paths that recorded present components as absent. Placeholder requests stop. |
| X / XI / XII.3 Transparency | ✅ The feature: per-component and document-level disclosure of why enrichment is missing. |
| Security | ✅ Only closed reason codes are emitted; upstream strings never reach the document (Q2). |
| Measurement rule | ✅ Batch and per-key outcome shapes measured (`measurements/batch_outcomes.txt`); per-repo counts measured. |

No violations.

## Project Structure

```text
specs/1067-depsdev-enrichment-outcome/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/deps-dev-outcome.md
├── measurements/  (probe_outcomes.sh, probe_batch_outcomes.py, counts.txt, batch_outcomes.txt, README.md)
└── checklists/requirements.md
```

```text
waybill-cli/src/enrich/depsdev_source.rs     # LookupResult; batch slot tracking (FR-010); outcome classification; C191 annotation
waybill-cli/src/enrich/request_key.rs        # placeholder → no key, outcome NotQueried(IncompleteCoordinate)
waybill-cli/src/scan_fs/mod.rs               # placeholder predicate moved out (shared), 0.0.0-unknown added
waybill-cli/src/generate/{cyclonedx,spdx}/   # C192 document-level emission (C158 path)
waybill-cli/src/parity/extractors/           # C191 (component scope), C192 (document scope)
docs/reference/sbom-format-mapping.md        # C191, C192 rows (one line each)
CHANGELOG.md
```

## Delivery

One PR. The FR-010 fixes go first within it, because without them the new signal would report false absences.

## Complexity Tracking

None.
