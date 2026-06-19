# Implementation Plan: Close milestone-131 SC misses with grounded targets

**Branch**: `132-sc-closeout` | **Date**: 2026-06-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/132-sc-closeout/spec.md`

## Summary

Closeout milestone for the unmet milestone-131 success criteria. Four user stories: **US1**
populates CDX `supplier.name` (and SPDX 2.3 `Package.originator` / SPDX 3 `software:supplier`)
from a static PURL-ecosystem → registry-name table; **US2** emits a companion
`mikebom:assembly-version-informational-stripped` annotation alongside the existing
Informational version so syft-parity comparators can match on the stripped form; **US3**
extends license coverage on the audit image from 37.8 % (2★) to ≥60 % (3★) per the actual
`sbom-comparison` `coverageStarsPct` banding formula (extracted from
`pkg/compare/packages.go:140`); **US4** retrospectively edits
`specs/131-quality-metadata-backfill/spec.md` so the spec record matches what actually
shipped. Zero new Cargo dependencies; every path modifies existing milestone-001 / -012 /
-130 / -131 source files.

The US3 path-choice question (3 candidate paths) was resolved by the §License Path
Analysis in `research.md`: only **Path C — deps.dev online enrichment for `pkg:cargo` and
`pkg:nuget`** lifts coverage above the 60 % band on the audit image. Path A (extended
PE/CLR fingerprinting) ships as a complementary fallback for offline-mode runs. Path B
(rootfs-local cargo cache) is rejected — production container images do not ship the
`.crate` cache files, so this path is effectively dead-letter and would chew planning
budget for ~0 measurable lift.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–131;
no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `serde`/`serde_json` (CDX/SPDX JSON I/O),
`reqwest = "0.12"` (workspace, `rustls-tls` + `blocking` features; reused for Path C
deps.dev calls — same pattern as milestone 012), `tokio` (existing workspace dep; reused
for Path C concurrent fetches with a semaphore identical to milestone 055's
`go-mod-graph` runner), `mikebom_common::types::purl::Purl` (typed ecosystem dispatch
per Constitution Principle IV), `tracing`, `anyhow`, `thiserror`. The
`mikebom-cli/src/enrich/depsdev_source.rs` scaffolding from milestone 012 is the
US3 substrate. **No new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan. The deps.dev enrichment client uses
the existing milestone-012 in-memory response cache (keyed by `(ecosystem, name, version)`)
that lives only for the duration of a single scan invocation. No persistent cache, no
filesystem state, mirrors every milestone since 002.
**Testing**: `cargo +stable test --workspace` + the new integration test at
`mikebom-cli/tests/sc_closeout_supplier_attribution.rs` (US1 verification) + the new
integration test at `mikebom-cli/tests/sc_closeout_version_mismatch_strip.rs` (US2
verification) + the sbom-comparison harness against the pinned audit-image digest for
SC-001 / SC-002 / SC-003 / SC-004 verification. Existing milestone-094 perf-test
infrastructure for SC-006.
**Target Platform**: Linux primary (audit baseline is a `linux/amd64` container image);
macOS dev; Windows experimental per milestone 100 / 101.
**Project Type**: cli (mikebom workspace, `mikebom-cli` crate).
**Performance Goals**: Scan-time growth on the audit image MUST be <30 % relative to
milestone 131 (SC-006). Path C deps.dev calls run concurrently with `tokio::Semaphore`
bounded to 16 in-flight requests (matches milestone 012's existing limit); the steady
state is `O(unique cargo + nuget PURLs)` HTTP round-trips, NOT `O(components)`.
**Constraints**: Pre-PR gate per `CLAUDE.md` § Pre-PR verification — both
`cargo +stable clippy --workspace --all-targets` (zero errors) AND
`cargo +stable test --workspace` (every suite reports `N passed; 0 failed`) MUST pass
locally before any PR opens. Standards-native fields take precedence over `mikebom:*`
properties (Constitution Principle V); the audit citation appears in
`Functional Requirements §FR-002` and is enumerated below in this plan's Constitution
Check. No new C dependencies (Constitution Principle I). Production code MUST NOT call
`.unwrap()` (Constitution Principle IV).
**Scale/Scope**: 3 source files modified (`mikebom-cli/src/scan_fs/mod.rs` for US1
supplier-table extension and US3 deps.dev integration point;
`mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs` for US2 stripped-Informational
emission and Path A fingerprint-table extension; `mikebom-cli/src/enrich/depsdev_source.rs`
for US3 Path C cargo-ecosystem support — the existing scaffolding handles nuget already),
1 documentation file edited (`docs/reference/sbom-format-mapping.md` gains one C-row for
the new `mikebom:assembly-version-informational-stripped` annotation), 2 spec edits
(milestone-131 retrospective per US4), 2 new integration test files,
1 quickstart re-measurement script (`specs/132-sc-closeout/quickstart.md`).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | ✅ | No C anywhere; all crates already pure-Rust. |
| II. eBPF-Only Observation | N/A | This work modifies the `scan_fs` package-DB path, not the eBPF trace path. |
| III. Fail Closed | ✅ | Path C deps.dev unavailability degrades to a transparency annotation (`mikebom:license-source = "depsdev-unavailable"`) per the milestone-012 pattern; the SBOM still emits. |
| IV. Type-Driven Correctness | ✅ | US1 dispatches on `Purl::ecosystem()` (typed); US2 reuses `is_plausible_version_string` (the milestone-131 US3 Phase A sanity filter); US3 reuses the milestone-012 `DepsDevLicense` newtype. No new `String` boundaries. Tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the existing `mikebom-cli/src/trace/` convention. |
| V. Specification Compliance | ✅ | **Standards-native audit (per Principle V v1.4.0)**: US1 uses CDX `metadata.supplier.name` + per-component `supplier.name` (native); SPDX 2.3 `Package.originator` (native); SPDX 3 `software:supplier` (native). NO `mikebom:*` introduced. US3 license expressions use CDX `licenses[].license.id` (native); SPDX 2.3 `licenseDeclared`/`licenseConcluded` (native); SPDX 3 `software:declaredLicense` (native). NO `mikebom:*` introduced beyond the existing milestone-012 provenance annotation `mikebom:license-source` (parity-bridging — no native CDX/SPDX construct for "where this license came from"; already documented in sbom-format-mapping.md). US2 IS a parity-bridging `mikebom:*` — `mikebom:assembly-version-informational-stripped` — JUSTIFIED because no CDX/SPDX construct exists for "alternate canonical version representation"; the C-row addition to `docs/reference/sbom-format-mapping.md` is part of US2's deliverable per FR-008. |
| VI. Three-Crate Architecture | ✅ | Only `mikebom-cli` touched. No new crates. |
| VII. Test Isolation | ✅ | sbom-comparison harness + integration tests run unprivileged. No eBPF involvement. |
| VIII. Completeness | ✅ | US1/US2 are pure metadata additions to already-discovered components; US3 enriches existing components, does NOT introduce new ones (Constitution XII constraint 1). |
| IX. Accuracy | ✅ | US2's stripped form re-runs the milestone-131 `is_plausible_version_string` sanity filter per FR-010; US3 Path C responses are annotated with `mikebom:license-source` provenance per milestone-012's existing convention (Principle IX requirement). |
| X. Transparency | ✅ | All milestone-132-introduced fields are either standards-native or carry an existing milestone-012 provenance annotation. |
| XI. Enrichment | ✅ | Path C IS the canonical enrichment pattern this principle describes. |
| XII. External Data Source Enrichment | ✅ | deps.dev is an enrichment source per Constitution XII; (1) no new components introduced — only existing-PURL components gain license data; (2) `mikebom:license-source = "depsdev"` annotates provenance; (3) deps.dev unavailability degrades gracefully per Principle III; (4) eBPF trace authority unaffected because this is the scan_fs (filesystem-scan) path, not the trace path. |

**Gate: PASS** for Phase 0. Re-checked post Phase 1 (no design changes that introduce new
`mikebom:*` fields or new `String` boundaries; gate remains PASS).

## Project Structure

### Documentation (this feature)

```text
specs/132-sc-closeout/
├── plan.md              # This file (/speckit-plan command output)
├── spec.md              # Feature spec (already exists; written by /speckit-specify)
├── research.md          # Phase 0 output — pinned digest + scoring formula + path analysis
├── data-model.md        # Phase 1 output — supplier table + stripped-version + license-enrichment entities
├── quickstart.md        # Phase 1 output — full re-measurement protocol against pinned digest
├── contracts/
│   └── sbom-format-mapping-row.md  # C-row for `mikebom:assembly-version-informational-stripped`
├── checklists/
│   └── requirements.md  # Already exists from /speckit-specify
└── tasks.md             # Phase 2 output (/speckit-tasks command; NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
mikebom-cli/src/
├── scan_fs/
│   ├── mod.rs                              # US1: extend supplier_from_purl PURL→registry-name table
│   │                                       # US3: post-resolution deps.dev enrichment integration point
│   └── package_db/
│       └── nuget/
│           └── pe_clr.rs                   # US2: emit stripped-Informational annotation
│                                           # US3 Path A: extend SPDX fingerprint table
└── enrich/
    └── depsdev_source.rs                   # US3 Path C: cargo ecosystem support (existing scaffolding)

mikebom-cli/tests/
├── sc_closeout_supplier_attribution.rs    # US1 integration tests
├── sc_closeout_version_mismatch_strip.rs  # US2 integration tests
└── sc_closeout_license_coverage.rs        # US3 integration tests (offline + online modes)

docs/reference/
└── sbom-format-mapping.md                  # US2: append one C-row for the stripped-version annotation

specs/131-quality-metadata-backfill/
└── spec.md                                 # US4: amend SC-001..SC-004 in place; add "Post-Milestone Outcomes (2026-06-19)" section
```

**Structure Decision**: Minimal — every milestone-132 source change is an extension to an
existing milestone-001 / -012 / -130 / -131 file. No new modules, no new directories
under `mikebom-cli/src/`. New integration tests follow the milestone-083
`transitive_parity_*.rs` naming pattern but live at the test-crate root (not inside a
`fixtures/` subdir) because the only fixture they need is the pinned audit image
(referenced by digest, pulled at test-time via Docker — same pattern as
`tests/image_sbom_*.rs`). The documentation hit is a single-line catalog row appended to
the existing format-mapping doc.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**

No constitution violations. Table omitted.

## Phase 0 status

`research.md` produced. Contains: pinned-digest capture command + `<DIGEST>` placeholder
flagged BLOCKING for the implementer (ECR reauth required before any measurement);
extracted `coverageStarsPct` formula from `sbom-comparison/pkg/compare/packages.go:140`;
extracted CDX→Normalized license-resolution priority from
`sbom-comparison/pkg/sbom/cyclonedx_normalize.go:90`; per-path coverage math against the
cached `/tmp/mb-rp-131-final.cdx.json` baseline with the projected 3★/4★/5★ bands the
implementer will re-verify against the pinned digest. Decision: **Path C** as primary;
Path A as offline-mode complement; Path B rejected.

## Phase 1 status

`data-model.md`, `quickstart.md`, `contracts/sbom-format-mapping-row.md` produced. The
agent-context update script invoked at the end of Phase 1.
