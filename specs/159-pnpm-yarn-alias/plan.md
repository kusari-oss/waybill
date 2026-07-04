# Implementation Plan: pnpm/yarn npm-alias syntax support

**Branch**: `159-pnpm-yarn-alias` | **Date**: 2026-07-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/159-pnpm-yarn-alias/spec.md`

## Summary

Fix issue #493: both pnpm-lock.yaml (v9 snapshots) and yarn.lock (v1) support a lockfile syntax where a local dep name aliases to a different real package. mikebom currently emits components + edges under the LOCAL name, so consumers looking up the aliased-canonical PURL find nothing and vulnerability scans silently miss the true package. Milestone 159 detects the alias syntax in both parsers, emits components under the ALIASED canonical PURL, rewrites edges to point at the aliased identity, and adds `mikebom:pnpm-alias` / `mikebom:yarn-alias` component-scope annotations (raw string, local-name only per Q1 clarification) so the audit trail survives PURL canonicalization.

Empirical target: 10 known dropped alias-edges (6 pnpm on test-podman-desktop + 1 yarn on test-guac-visualizer + 3 yarn on test-rails) all correctly resolved to their aliased-canonical PURLs.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–158; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `serde_yaml` (pnpm-lock + yarn Berry parsing; already used), the milestone-113 `regex` crate (for yarn v1 line-by-line descriptor extraction; already used), `serde` / `serde_json` (annotation emission), `tracing` (info/warn logs per FR-010/FR-011), `anyhow` / `thiserror` (error propagation). **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan. The alias-mapping tables (per-lockfile `HashMap<LocalName, AliasedIdentity>`) live in local scope inside `parse_pnpm_lock` / `parse_yarn_lock` and die at function return. No caches, no persistence (matches every milestone since 002).

**Testing**: `cargo +stable test --workspace` — 12+ new unit tests inline in `pnpm_lock.rs` + `yarn_lock.rs` (SC-007 floor ≥12) plus a new SC-008 integration test at `mikebom-cli/tests/npm_alias_resolution.rs` synthesizing a mixed pnpm+yarn testbed.

**Target Platform**: Same as milestone 158 — Linux + macOS + Windows via existing CI matrix. No platform-specific behavior.

**Project Type**: Rust workspace with three crates (`mikebom-cli`, `mikebom-common`, `mikebom-ebpf`); milestone 159 touches ONLY `mikebom-cli` (user-space).

**Performance Goals**: Alias-detection is per-lockfile-entry O(1) overhead. No perf constraint beyond the milestone-158 O(V+E) BFS pass (which is unchanged — the milestone-159 change is upstream in the parser layer, not in emission).

**Constraints**:

- Standards-native precedence (Constitution Principle V): the two annotations use `mikebom:*` because no CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native "package alias" property exists. FR-013 codifies the migration path if either standard adopts one.
- SC-003 byte-identity guard on all 11 milestone-090 goldens: no alias syntax in fixtures per empirical verification 2026-07-04. Zero diff bytes expected.
- No `.unwrap()` in production paths (Constitution Principle IV) — the test-only guard convention applies.
- The aliased-name-based PURL is authoritative (per spec assumption). No dual-emission of both local + aliased identities.

**Scale/Scope**: Milestone-159 touches ~200–300 LOC across `pnpm_lock.rs` + `yarn_lock.rs` + a new `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` submodule for the shared alias-detection logic. Plus ~150 LOC of unit tests, ~100 LOC of integration test, 2 parity-catalog rows.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)**: PASS. Zero new Cargo deps. No C source, no FFI.
- **Principle IV (No `.unwrap()` in production)**: PASS. All production paths use `?` propagation. Test-only `.unwrap()` uses `#[cfg_attr(test, allow(clippy::unwrap_used))]` per convention.
- **Principle V (Specification Compliance — standards-native precedence)**: PASS with acknowledged deviation. The two annotations use `mikebom:*` because no CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native "package alias" property exists. FR-013 codifies the migration path.
- **Principle VIII (Completeness)**: **Directly served**. The 10 previously-dropped alias-edges move from "silent miss" to "correctly resolved," measurably closing the false-negative surface on the 3 test repos.
- **Principle IX (Accuracy)**: **Directly served**. Emitting components at the aliased-canonical PURL instead of the LOCAL name means consumers doing vulnerability lookup key against the correct npm-registry identity — reduces false positives (LOCAL-name PURLs would never match any CVE) AND surfaces true positives (aliased-canonical PURLs match real CVE feeds).
- **Principle X (Transparency)**: **Directly served**. The `mikebom:pnpm-alias` / `mikebom:yarn-alias` annotations expose the local-name-to-aliased-name relationship as machine-inspectable metadata; consumers can audit the resolution decision.
- **Strict Boundary #4 (No `.unwrap()`)**: PASS.
- **Strict Boundary #5 (No file-tier duplicates in default mode)**: N/A — milestone 159 does not emit file-tier components.

**Result**: All gates PASS. No violations to track in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/159-pnpm-yarn-alias/
├── plan.md                                 # This file
├── research.md                             # Phase 0 output
├── data-model.md                           # Phase 1 output
├── quickstart.md                           # Phase 1 output
├── contracts/
│   ├── annotation-schema.md                # Per-format wire shape
│   ├── pnpm-alias-grammar.md               # Pnpm alias value grammar (BNF)
│   └── yarn-alias-grammar.md               # Yarn v1 key-side alias grammar (BNF)
├── checklists/
│   └── requirements.md                     # (from /speckit-specify)
├── spec.md                                 # (from /speckit-specify + /speckit-clarify)
└── tasks.md                                # Phase 2 output (from /speckit-tasks)
```

### Source Code (repository root)

New submodule + parser wire-up (impl PR delta):

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       └── npm/
│   │           ├── alias_mapping.rs        # NEW submodule (milestone 159)
│   │           │                            # - AliasResolution type
│   │           │                            # - PnpmAliasParser (parses value shape)
│   │           │                            # - YarnV1AliasParser (parses key-side shape)
│   │           │                            # - Unit tests inline
│   │           ├── pnpm_lock.rs            # extend collect_pnpm_dep_names + parse_pnpm_lock
│   │           ├── yarn_lock.rs            # extend v1_header_to_name + parse_v1
│   │           └── mod.rs                  # +pub mod alias_mapping
│   ├── generate/
│   │   ├── cyclonedx/
│   │   │   └── components.rs               # +mikebom:pnpm-alias/yarn-alias property emission
│   │   ├── spdx/
│   │   │   └── annotations.rs              # +per-package Annotation emission
│   │   └── spdx3/
│   │       └── v3_annotations.rs           # +per-package Annotation emission
│   └── parity/extractors/
│       ├── mod.rs                          # +2 new catalog rows: C106/C107 (SC-010)
│       ├── cdx.rs                          # +2 extractors (cdx_anno!)
│       ├── spdx2.rs                        # +2 extractors (spdx23_anno!)
│       └── spdx3.rs                        # +2 extractors (spdx3_anno!)
└── tests/
    └── npm_alias_resolution.rs             # NEW SC-008 integration test

docs/
└── reference/
    └── sbom-format-mapping.md              # +2 rows (C106/C107) — catalog-vs-extractor sync
```

**Structure Decision**: A new `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` submodule holds the alias-detection logic — shared between pnpm and yarn parsers, distinct enough from the existing per-parser logic to deserve its own file. The submodule exposes:

- `AliasResolution` — a value type carrying `(local_name, aliased_name, aliased_version, optional_peer_suffix)`.
- `detect_pnpm_alias(local_name, raw_value) -> Option<AliasResolution>` — reused from `collect_pnpm_dep_names` and `build_snapshots_lookup`.
- `detect_yarn_v1_alias(key_line) -> Option<AliasResolution>` — reused from `v1_header_to_name`.

Alias-resolution tables (`HashMap<LocalName, AliasedIdentity>`) live in local scope inside `parse_pnpm_lock` / `parse_yarn_lock` and are consumed by the same functions when rewriting dep edges (FR-005). This keeps parser scope contained per-lockfile — no cross-file alias resolution.

Component emission (US1 P1) happens naturally via the aliased-canonical PURL: `PackageDbEntry.name` gets the aliased-name and `PackageDbEntry.version` gets the aliased-version. The existing `build_npm_purl` at `npm/mod.rs:580` produces the correct `pkg:npm/%40scope/name@version` shape without changes.

Annotation emission (US2 P2) follows the milestone-127/134/158 established pattern for CDX component-scope + SPDX per-package Annotation, registered as C106 + C107 in the parity catalog.

## Complexity Tracking

*Empty — no Constitution violations to justify.*

The design deliberately reuses:

- Milestone-157's `parse_pnpm_key` helper (unchanged): the alias VALUE is itself a `<name>@<version>(peer-suffix)` string parsable by the same function. The insight: what the current code discards as a peer-suffixed "local canonical" is actually the ALIASED canonical identity.
- Milestone-106's yarn `v1_header_to_name` (extended): the current single-descriptor logic is preserved; the `npm:` detection is a lookaside on the FULL comma-joined key.
- The milestone-071 parity catalog (2 new rows, symmetric across 3 formats — same pattern as milestone-158's C104/C105).
- Milestone-158's per-format component-scope annotation emission (CDX `properties[]`, SPDX 2.3 `Annotation`, SPDX 3 `Annotation` element).

The Q1 raw-string annotation shape (locked during /speckit-clarify) means we don't need the milestone-127 envelope-JSON machinery — the value is a bare string, matching milestone-158's `mikebom:graph-completeness` shape.
