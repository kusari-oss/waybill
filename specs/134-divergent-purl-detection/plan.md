# Implementation Plan: Divergent-PURL collision detection in main-module dedup

**Branch**: `134-divergent-purl-detection` | **Date**: 2026-06-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/134-divergent-purl-detection/spec.md`

## Summary

When mikebom's main-module dedup (introduced in milestone 064) finds two-or-more `Cargo.toml` files claiming the same `pkg:cargo/<name>@<version>` PURL identity AND those manifests have divergent declared direct-dep sets (or divergent deep-hashes when `--deep-hash` is set), emit:

- **Per-component property** `mikebom:duplicate-purl-divergent` on the deduped root component, carrying a structured value `{ reason, paths[], dep_sets_by_path, hashes_by_path? }`.
- **Document-scope summary annotation** `mikebom:purl-collisions-detected` listing every collision detected in the scan.

Detection piggybacks on the existing milestone-064 dedup hash-set — zero overhead when no collisions are present. The existing `tracing::warn!` from milestone 064 continues to fire alongside the new annotations (FR-008). Soft-by-default this milestone (FR-007); hard-fail mode deferred to a follow-up. Cargo-only this milestone (FR-010); detection logic is structured at the data-model layer for ecosystem-agnostic reuse by future npm / maven / pip / gem / go-binary follow-ups.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–133; no nightly required for this user-space-only feature).

**Primary Dependencies**: Existing only — `serde`/`serde_json` (annotation value construction), `toml = "0.8"` (Cargo.toml parsing — already used by the cargo reader at `mikebom-cli/src/scan_fs/package_db/cargo.rs`), `sha2` (deep-hash comparison; reuses milestone-038 infrastructure), `tracing` (preserve milestone-064 `warn!`), `anyhow`, `thiserror`. **Zero new Cargo dependencies.**

**Storage**: N/A — collision-detection state is in-process per scan, emitted into the SBOM as annotations. No caches, no persistence (matches every milestone since 002).

**Testing**: `cargo +stable test --workspace`. Synthetic-fixture pattern via `tempfile::tempdir()`. Two new integration test files at `mikebom-cli/tests/divergent_purl_deps_differ.rs` and `mikebom-cli/tests/divergent_purl_hashes_differ.rs`. SC-002 byte-identity regression guarded against the existing 11-ecosystem golden suite at `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/cargo.{cdx,spdx,spdx3}.json` (must NOT change for no-collision scans).

**Target Platform**: Cross-platform — Linux + macOS + Windows. Detection runs in user-space only. The cargo reader's existing platform support carries through unchanged.

**Project Type**: CLI tool — extends `mikebom sbom scan` emission code path.

**Performance Goals**: <2% scan wall-clock increase across CI realistic-project fixtures (SC-004). The no-collision path is O(1) — at most one hash-set lookup per emitted root component, which the dedup phase already performs.

**Constraints**: 
- Byte-identical SBOM goldens when no divergence detected (SC-002).
- `tracing::warn!` from milestone 064 must continue to fire (FR-008).
- Detection ecosystem-agnostic at the data-model layer (FR-010).
- Principle V audit: no CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native field expresses "same identity, divergent content" semantics — `mikebom:*` annotation is justified (FR-011, audit narrative goes in `docs/reference/sbom-format-mapping.md` per Principle V).

**Scale/Scope**: Cargo-only this milestone. Typical realistic-project scan: 0 collisions (workspace member dedup is harmless). Pathological synthetic fixture: 3–10 collisions for stress testing. No expected upper bound — the data model scales linearly with collision count.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Justification |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | All new code is user-space Rust; no FFI, no C toolchain. |
| II. eBPF-Only Observation | N/A | This milestone enriches metadata for components ALREADY discovered by the existing filesystem walker. No discovery change. |
| III. Fail Closed | ✓ | Soft-only this milestone (FR-007). When divergence detected, annotation emitted; scan continues. Hard-fail mode is explicit follow-up scope. |
| IV. Type-Driven Correctness | ✓ | Introduces typed `DivergenceRecord` struct with enum reason field (`deps-differ` / `hashes-differ` / `both`). No stringly-typed signals. |
| V. Specification Compliance | ✓ | **Audit completed in Phase 0 research**: no CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native field expresses "same identity across multiple paths with divergent content." Closest construct is SPDX `VARIANT_OF`, which is a between-packages relationship (different PURLs), not a within-PURL divergence signal — doesn't fit. `mikebom:*` annotation justified. Audit narrative ships as new C-row in `docs/reference/sbom-format-mapping.md`. |
| VI. Three-Crate Architecture | ✓ | All new code lives in `mikebom-cli` (the orchestrator + reader crate). Shared types stay in `mikebom-common`. No new workspace crate. |
| VII. Test Isolation | ✓ | Synthetic tempfile fixtures only; no host-state dependency. |
| VIII. Completeness | ✓ | Divergent-PURL is itself a completeness signal — the annotation surfaces a real-world ambiguity that today silently first-wins. Strict Boundary §5 (no-duplicates in default mode) is preserved bit-for-bit: the dedup still emits exactly one component; the annotation describes the collision context without violating the boundary. |
| IX. Determinism | ✓ | Detection is deterministic given a stable filesystem walk order (sorted directory entries). Path lists are emitted in walk order — same order produces identical annotation byte sequences. |
| X. Transparency | ✓ | The annotation IS the transparency. Per-component + document-scope surfaces give consumers full visibility into what mikebom detected. |
| XII. External Data Source Enrichment | N/A | No external data sources consulted for this detection. |

**Verdict: PASS.** No violations, no justifications required.

## Project Structure

### Documentation (this feature)

```text
specs/134-divergent-purl-detection/
├── plan.md              # This file
├── spec.md              # Feature spec (already written)
├── research.md          # Phase 0 output — Principle V audit + design decisions
├── data-model.md        # Phase 1 output — DivergenceRecord + payload shapes
├── quickstart.md        # Phase 1 output — operator-facing walkthrough
├── contracts/           # Phase 1 output — annotation wire-format contracts
│   ├── per-component-property.md
│   └── document-scope-annotation.md
├── checklists/
│   └── requirements.md  # Spec-quality checklist (already written)
└── tasks.md             # Phase 2 output (via /speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/src/
├── scan_fs/
│   └── package_db/
│       └── cargo.rs                       # MODIFY: extend existing milestone-064
│                                          # dedup site to populate the divergence
│                                          # record per FR-001..FR-006
├── generate/
│   ├── divergence_annotation.rs           # NEW: ecosystem-agnostic annotation
│                                          # construction; consumed by all three
│                                          # format emitters
│   ├── cyclonedx/
│   │   ├── component_properties.rs        # MODIFY: emit per-component property
│   │   └── document_properties.rs         # MODIFY: emit document-scope summary
│   ├── spdx/
│   │   └── document.rs                    # MODIFY: emit per-component annotation
│                                          # + document-scope annotation in SPDX 2.3
│   └── spdx/
│       └── v3_document.rs                 # MODIFY: emit per-component property
│                                          # + document-scope property in SPDX 3
└── parity/
    └── extractors/
        ├── divergent_purl_per_component.rs # NEW: parity-catalog extractor for
                                            # per-component property (C-row N)
        └── divergent_purl_document_scope.rs # NEW: parity-catalog extractor for
                                            # document-scope property (C-row N+1)

mikebom-common/src/
└── divergence.rs                          # NEW: typed DivergenceRecord struct +
                                          # DivergenceReason enum; shared across
                                          # the CLI's three format-emission paths

mikebom-cli/tests/
├── divergent_purl_deps_differ.rs          # NEW: SC-001 + acceptance scenarios
                                          # for US1 (declared-dep divergence)
└── divergent_purl_hashes_differ.rs        # NEW: SC-003 + acceptance scenarios
                                          # for US2 (deep-hash divergence under
                                          # --deep-hash)

docs/reference/
└── sbom-format-mapping.md                 # MODIFY: add 2 new C-rows with the
                                          # Principle V audit narrative for the
                                          # two new mikebom:* properties
```

**Structure Decision**: Extends the existing mikebom-cli crate. Shared `DivergenceRecord` type lives in `mikebom-common` (the cross-CLI/library boundary), keeping the cargo reader, the three format emitters, and the parity extractors all consuming the same typed representation. No new workspace crate per Principle VI.

## Complexity Tracking

> No Constitution Check violations — no justifications required.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none)    | n/a        | n/a                                  |
