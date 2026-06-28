# Implementation Plan: npm peerDependencies — emit as edges + annotate peer-kind

**Branch**: `147-npm-peer-edges` | **Date**: 2026-06-28 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/147-npm-peer-edges/spec.md`

## Summary

Close the orphan gap surfaced by the Trivy / Syft / mikebom comparison on `looker-frontend` package-lock.json (5 mikebom orphans → 0, matching Trivy). One-line section-list extension at `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs:177-181` (add `"peerDependencies"`) + a small per-entry annotation stamp (`mikebom:peer-edge-targets`) that preserves the install-vs-functional distinction for downstream consumers that want to filter on edge kind.

Phase 0 research §A confirmed three load-bearing assumptions:
1. **Sort-order precedent**: milestone 145's `mikebom:file-paths` uses `paths_str.sort()` (alphabetical) before storing — same pattern applies here for the `mikebom:peer-edge-targets` PURL array.
2. **Unmet-peer handling is automatic**: the existing `resolve_dep_via_node_modules_walk` helper at line 354 explicitly returns `None` when the dep isn't installed at any level (doc comment lines 350-353: "*Returns None if dep_name isn't installed at any level... can happen when a dep is declared but not actually resolved*"). Spec FR-002 (no phantom edges for unmet peers) gets satisfied for free.
3. **Existing doc-comment is already half-correct**: lines 149-160 say "*Walks ALL four standard npm dep sections — dependencies, devDependencies, peerDependencies, optionalDependencies*" but lines 177-181 only walk three of them. The implementer's INTENT (per the upper comment) was always to walk all four; the lower skip-comment (168-176) is the deliberate exception we're now reverting.

Total code surface estimate: ~15 LOC reader change (section list + per-entry annotation stamp + comment rewrite) + ~80 LOC of tests + golden refresh (3 npm fixtures expected to gain new edges + annotations). Zero new Cargo dependencies. One coordinated change in one crate.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–146; no nightly required).
**Primary Dependencies**: Existing only — `serde_json` (already pervasive in the npm reader), `BTreeMap` (already used at `package_lock.rs:166` for the existing `depends_set`), `BTreeSet` (for tracking peer-edge target uniqueness). **No new Cargo dependencies.**
**Storage**: N/A — per-scan in-process state; no persistence.
**Testing**: `cargo +stable test --workspace`. New tests are in-file `#[cfg(test)] mod tests` in `package_lock.rs` (existing test block). The existing `peer_dependencies_are_skipped_declarative_not_install` test at line 680-711 gets REPLACED with the new "peer-edges-are-emitted-with-annotation" test (FR-007).
**Target Platform**: Linux x86_64 + macOS arm64 + Windows (CI lanes). Pure-Rust pure-data-transform; no platform-specific behavior.
**Project Type**: Library/reader — touches `mikebom-cli`'s npm package-lock reader. Downstream consumers (CDX builder, SPDX 2.3 emitter, SPDX 3 emitter) consume the `PackageDbEntry.depends` Vec + `extra_annotations` map transparently with **zero call-site changes**.
**Performance Goals**: Microsecond-cost section-list iteration extension per scan (one more dictionary lookup + iteration per `PackageDbEntry`). Negligible.
**Constraints**:
- **Constitution V (standards-native > `mikebom:*`)**: The new `mikebom:peer-edge-targets` annotation IS a new `mikebom:*` property. Per Principle V's parity-bridging clause, this is permitted ONLY because CDX 1.6, SPDX 2.3, and SPDX 3 all lack a native carrier for per-edge "peer-kind" metadata (verified in spec FR-009 audit). The annotation MUST be documented in `docs/reference/sbom-format-mapping.md` per Principle V's documentation requirement.
- **Constitution IV (Type-Driven Correctness)**: PURL strings in the annotation array are not wrapped in a `Purl` newtype because the annotation is a wire-output value (string array in JSON), not a parsed domain value. The PURL strings ARE produced by the reader's existing `Purl::new()` validation pipeline, so format correctness is guaranteed upstream.
- **No subprocess calls**: pure-Rust BTreeMap / BTreeSet operations.
- **Pre-PR gate**: `./scripts/pre-pr.sh` (clippy `-D warnings` + `cargo test --workspace`) MUST exit 0 before PR open. The pre-existing local `sbomqs_parity` env-only failure documented in milestone 144 T001 still applies; CI on a clean runner validates.
**Scale/Scope**: ~5 orphans → 0 on the audit corpus (looker-frontend, ~671 components). Across the full mikebom test fixture set, the 3 affected npm-bearing goldens will gain ~0-2 peer-edges each (most npm fixtures don't exercise peer-installations — peer-deps are predominantly a React/Vue/framework-plugin pattern).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ PASS | No new Cargo dependencies. |
| II | eBPF-Only Observation | ✅ N/A | Pure value-extraction from package-lock.json; eBPF discovery untouched. |
| III | Fail Closed | ✅ PASS | No change to scan-failure semantics. |
| IV | Type-Driven Correctness | ✅ PASS | PURL strings produced by existing `Purl::new()` pipeline; new `extra_annotations` value uses existing `serde_json::Value::Array` type. No new `.unwrap()` in production paths. |
| V | Specification Compliance | ⚠️ **Parity-bridging carve-out** | New `mikebom:peer-edge-targets` annotation is permitted per Principle V's "parity-bridging" clause because no standards-native carrier exists for per-edge peer-kind metadata in CDX 1.6 / SPDX 2.3 / SPDX 3. Audit recorded in spec FR-009 + this milestone MUST update `docs/reference/sbom-format-mapping.md` (Phase 1 T—TBD in tasks). |
| VI | Three-Crate Architecture | ✅ PASS | All change in `mikebom-cli`. |
| VII | Test Isolation | ✅ PASS | All new tests are pure-logic unit tests; no eBPF privilege requirements. |
| VIII | Completeness | ✅ IMPROVES | Closes 5 orphans on the audit corpus → 0. Improves reachability-based vulnerability-scanner accuracy. |
| IX | Accuracy | ✅ IMPROVES | The functional dependency expressed by `peerDependencies` IS real (peer is installed per lockfile, executes at runtime). Emitting the edge is more accurate than omitting it. |
| X | Transparency | ✅ PASS | The peer-kind distinction is preserved via the new annotation — consumers that want to filter on edge kind have a queryable signal. |
| XI | Enrichment | ✅ N/A | No enrichment-source changes. |
| XII | External Data Source Enrichment | ✅ N/A | Lockfile is in-scope filesystem content, not external. |
| SB-1 | No lockfile-based discovery | ✅ N/A | Lockfile reads are for ENRICHMENT (dep-graph edges), not for component discovery; FR-002 explicitly requires the peer to be installed (present in lockfile's `packages` map) before emitting an edge. |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ PASS | Existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention preserved on new tests. |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | |

**All gates pass.** Principle V check requires the documentation update at `docs/reference/sbom-format-mapping.md`; that's a tasks.md polish-phase item (not a violation).

## Project Structure

### Documentation (this feature)

```text
specs/147-npm-peer-edges/
├── plan.md              # This file
├── research.md          # Phase 0 output (sort precedent + unmet-peer behavior + golden audit)
├── data-model.md        # Phase 1 output (mikebom:peer-edge-targets annotation contract)
├── quickstart.md        # Phase 1 output (operator-facing verification)
├── contracts/
│   └── peer-edge-targets.md   # Pure-function contract for the new annotation
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify
```

### Source Code (repository root)

Touched files (one reader, narrow scope):

```text
mikebom-cli/
├── (no Cargo.toml change)
└── src/
    └── scan_fs/
        └── package_db/
            └── npm/
                └── package_lock.rs                         # PRIMARY change — section-list extension + annotation stamp + test replacement + comment rewrite
docs/
└── reference/
    └── sbom-format-mapping.md                              # NEW row: mikebom:peer-edge-targets — Principle V parity-bridging documentation
mikebom-cli/
└── tests/
    └── fixtures/
        └── golden/                                         # 3 npm-bearing fixtures may need refresh (cyclonedx/npm.cdx.json + spdx-2.3/npm.spdx.json + spdx-3/npm.spdx3.json)
```

**Structure Decision**: Single-file change in `npm/package_lock.rs` (the reader). Zero new modules, zero signature changes to public APIs, zero call-site changes elsewhere. The annotation is stamped via the existing `PackageDbEntry.extra_annotations` channel which all three SBOM emitters already iterate (via the existing milestone-127 `is_internal_emission_key` filter pattern). Documentation update is mechanical.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

Not a violation, but worth recording the Principle V parity-bridging audit decision:

| Carve-out | Why Needed | Standards-Native Alternative Rejected Because |
|---|---|---|
| `mikebom:peer-edge-targets` annotation | Preserves the install-vs-functional distinction for downstream consumers that want to filter peer-driven edges separately from regular dependencies | CDX 1.6 `dependencies[].dependsOn[]` is `Array<bom-ref-string>` with no per-element metadata slot. SPDX 2.3 typed relationships ({DEPENDS_ON, DEV_DEPENDENCY_OF, BUILD_DEPENDENCY_OF, TEST_DEPENDENCY_OF, RUNTIME_DEPENDENCY_OF}) have NO `PEER_DEPENDENCY_OF` variant. SPDX 3 `LifecycleScopedRelationship.scope` enum is {development, build, test, runtime} with NO `peer` value. No standards-native edge-typing carrier exists for the peer-kind signal. |
