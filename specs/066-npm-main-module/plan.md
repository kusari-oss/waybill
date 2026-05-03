# Implementation Plan: npm source-tree main-module component for package.json roots + workspace members

**Branch**: `066-npm-main-module` | **Date**: 2026-05-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/066-npm-main-module/spec.md`

## Summary

Extend the milestone 064 (cargo) + 053 (Go) main-module pattern to npm: emit one synthetic main-module per `package.json` containing `name` (skipping `private: true` + no-version per #104), placed in standards-native "BOM subject" slots (CDX `metadata.component` for single-package scans, sibling `components[]` under super-root for workspaces; SPDX `documentDescribes[]` plural; SPDX 3 `rootElement[]` plural). Carries `mikebom:component-role: main-module` (C40) supplementary signal. Multi-main-module super-root infrastructure from #127 reused at zero marginal cost. Same-PURL dedup with `tracing::warn!`. No version-resolution ladder beyond literal-or-`0.0.0-unknown` (npm has no workspace-inheritance feature). Closes the npm slice of issue #104.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–065; no nightly required).
**Primary Dependencies**: Existing only — `serde_json` (already used by `npm/walk.rs`), `tracing`, `anyhow`. **No new crates.** No subprocess calls.
**Storage**: N/A — all state in-process per scan.
**Testing**: `cargo +stable test --workspace`; existing golden infrastructure regen for npm-bearing fixtures; new `tests/fixtures/npm-workspace/` fixture exercising `workspaces: ["packages/*"]` glob expansion + workspace-member path-deps.
**Target Platform**: Linux (x86_64 + aarch64) + macOS (aarch64) — matches existing CI lanes.
**Project Type**: CLI (workspace-rooted; reuses `mikebom-cli/src/scan_fs/package_db/npm/walk.rs` + the milestone-053/064 generator-side machinery, which is already C40-tag-driven so npm inherits it transparently).
**Performance Goals**: One additional `package.json` re-parse per discovered manifest with `name`. The existing reader (`walk.rs:179` `read_root_package_json` → `parse_root_package_json`) already parses the same files for milestone-051 design-tier dep emission; the main-module pass piggybacks. Sub-millisecond impact.
**Constraints**: Cross-host byte-identity for goldens — manifest-derived `name` + `version` strings are committed and identical across hosts (no `git describe`-style host-state dependency). Same-PURL dedup deterministic on the existing alphabetical walker order.
**Scale/Scope**: One main-module per `package.json` with `name` discovered. Single-package projects emit 1; npm 7+ workspaces with N members emit N (typical: 5–50 for monorepos). Polyglot scans combine with cargo + Go main-modules under the existing super-root mechanism.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Per `.specify/memory/constitution.md` v1.4.0 — same gate-pass posture as milestones 053 + 064 since this milestone reuses their machinery wholesale:

| Principle | Status | Notes |
|-----------|--------|-------|
| **I. Pure Rust, Zero C** | ✅ Pass | All changes in pure Rust. No subprocess calls. |
| **II. eBPF-Only Observation** | ✅ Pass | Main-module represents the workspace root that's the implicit scan target; not new dependency-discovery surface. |
| **III. Fail Closed** | ✅ Pass | When `version` is missing OR `private: true` AND no version, behavior is deterministic (placeholder or skip per FR-001). Same-PURL collisions degrade gracefully via dedup with `tracing::warn!`. |
| **IV. Type-Driven Correctness** | ✅ Pass | Reuses existing `PackageDbEntry`, `Purl`, `serde_json::Value` types. `.unwrap()` only inside `#[cfg(test)]` per existing convention. |
| **V. Specification Compliance** | ✅ Pass — **AUDIT PERFORMED** | Per FR-001a: primary signal is native CDX `metadata.component` (`type: "application"`), SPDX `primaryPackagePurpose: "APPLICATION"` + `documentDescribes`, SPDX 3 `software_primaryPurpose: "application"`. C40 supplementary. Scope-encoded PURL conforms to PURL spec for `@scope/name` (`pkg:npm/%40<scope>/<name>@<version>`). |
| **VI. Three-Crate Architecture** | ✅ Pass | All changes within `mikebom-cli/`. No new crates. |
| **VII. Test Isolation** | ✅ Pass | New tests are unit-level (in `npm/walk.rs::tests`) + integration-level (golden regen + new workspace fixture). No eBPF involvement. |
| **VIII. Completeness** | ✅ Pass | Adds project-self component to npm SBOMs that today have none. |
| **IX. Accuracy** | ✅ Pass | Manifest-derived versions are authoritative. Placeholder is transparent. Same-PURL dedup deterministic. |
| **X. Transparency** | ✅ Pass | Same-PURL dedup emits `tracing::warn!` listing dropped paths. |
| **XI. Enrichment** | ✅ Pass | LICENSE detection deferred to issue #103. |
| **XII. External Data Source Enrichment** | ✅ Pass | `package.json` is read for the main-module's identity (PURL). The main-module IS the scan target, not a new component imported from the lockfile. |
| **Strict Boundary #1 (No lockfile-based dependency discovery)** | ✅ Pass | `package.json`'s `dependencies` / `devDependencies` / etc. are used to relocate existing direct edges (FR-007); no new components introduced from manifest data. |

**Gate result**: Pass. No constitution violations.

## Project Structure

### Documentation (this feature)

```text
specs/066-npm-main-module/
├── plan.md              # This file
├── spec.md              # Feature specification (Q1 clarification recorded)
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── npm-main-module-component.md   # Phase 1 output
├── checklists/
│   └── requirements.md  # Spec-quality checklist (all items pass)
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       └── npm/
│   │           ├── walk.rs                # ⬅️ MAIN CHANGE — new
│   │           │                           #     build_npm_main_module_entry()
│   │           │                           #     + new find_package_json_manifests()
│   │           │                           #     + new dedup_npm_main_modules_by_purl()
│   │           │                           #     (parallels cargo.rs's milestone-064
│   │           │                           #     helpers; npm doesn't need a
│   │           │                           #     WorkspaceContext since there's no
│   │           │                           #     version-inheritance feature)
│   │           ├── mod.rs                  # ⬅️ WIRE-UP — extend `read()` to call
│   │           │                           #     find_package_json_manifests +
│   │           │                           #     build_npm_main_module_entry per
│   │           │                           #     manifest, then augment-existing or
│   │           │                           #     emit-new (mirroring cargo.rs's
│   │           │                           #     Phase A pattern from milestone 064)
│   │           ├── package_lock.rs         # (no change — lockfile-driven dep
│   │           │                           #  emission unchanged)
│   │           └── pnpm_lock.rs            # (no change)
│   ├── generate/
│   │   ├── cyclonedx/
│   │   │   ├── metadata.rs                 # (no change — already C40-tag-driven
│   │   │   │                                #  per milestone 064; npm main-modules
│   │   │   │                                #  flow through naturally)
│   │   │   └── builder.rs                  # (no change — same)
│   │   └── spdx/
│   │       ├── packages.rs                 # (no change — primaryPackagePurpose
│   │       │                                #  predicate is C40-driven)
│   │       ├── document.rs                 # (no change — multi-DESCRIBES from
│   │       │                                #  #127 already handles N>1
│   │       │                                #  main-modules across any ecosystem)
│   │       └── v3_document.rs              # (no change — pick_root_iri uses
│   │                                        #  C40 tag-driven detection per #127)
│   └── parity/
│       └── extractors/
│           └── (no change — C40 catalog row already wired)
└── tests/
    ├── fixtures/
    │   ├── npm-workspace/                  # ⬅️ NEW FIXTURE — npm 7+ workspace
    │   │   ├── package.json                #     workspaces: ["packages/*"],
    │   │   │                               #     private: true (no version on root)
    │   │   ├── packages/
    │   │   │   ├── a/package.json          #     name: "a", version: "0.5.0"
    │   │   │   └── b/package.json          #     name: "b", version: "0.5.0",
    │   │   │                               #     dependencies: { "a": "*" }
    │   │   ├── package-lock.json           #     committed for deterministic scan
    │   │   └── README.md
    │   └── npm-scoped-package/             # ⬅️ NEW FIXTURE — scoped name encoding
    │       └── package.json                #     name: "@kusari/foo"
    └── scan_npm.rs                         # ⬅️ NEW TESTS — 3-4 integration tests
                                             #     covering US1 AS#1 + AS#2 (scoped)
                                             #     + AS#3 (workspace) + FR-002

docs/
├── reference/
│   └── sbom-format-mapping.md              # ⬅️ DOC UPDATE — extend C40 row to
│                                            #     mention npm coverage; per-ecosystem
│                                            #     matrix: Go ✅, cargo ✅, npm ✅;
│                                            #     pip/maven/gem still in #104
└── design-notes.md                         # ⬅️ DOC UPDATE — update the per-ecosystem
                                             #     asymmetry section: Go + cargo + npm
                                             #     done; pip/maven/gem pending

CHANGELOG.md                                # ⬅️ DOC UPDATE — `[Unreleased]` →
                                             #     `### Changed (BREAKING — SBOM
                                             #     output shape, milestone 066)`
```

**Structure Decision**: Single-crate (`mikebom-cli`) feature. The npm reader (`scan_fs/package_db/npm/walk.rs`) gains the new main-module helpers; the generator-side machinery is unchanged (already C40-tag-driven from milestones 053 + 064 + #127). New fixture directory exercises the workspace + scoped-name + path-dep cases. The existing single-package npm fixture (`tests/fixtures/npm/`) gains a main-module on regen; goldens update.

## Phase 0: Outline & Research — COMPLETE (in-spec)

Phase 0 captured directly in spec Clarifications + Assumptions. Key decisions:

- **Decision**: Skip emission only for `private: true` + no `version`. Other manifests with `name` + missing `version` emit with `0.0.0-unknown` placeholder. **Rationale**: per spec Q1, matches cargo's permissive ladder behavior. **Alternatives considered**: strict skip-on-missing-version (rejected — diverges from cargo); skip if `private: true` regardless of version (rejected — `private` is a publish guard, not an SBOM-presence signal).
- **Decision**: `node_modules/` excluded from manifest discovery (existing `should_skip_descent` list). **Rationale**: `node_modules/` contains upstream deps, not project-internal artifacts. **Alternatives**: emit for excluded `node_modules` paths (rejected — would balloon SBOMs with thousands of upstream-package main-modules — pure FP).
- **Decision**: Reuse milestone 053 + 064 + #127 generator-side machinery unchanged. **Rationale**: The C40-tag-driven hooks (CDX `metadata.component` selector, SPDX `primaryPackagePurpose` predicate, multi-DESCRIBES wiring) are ecosystem-agnostic by design; npm main-modules tagged with the same C40 annotation flow through automatically.
- **Decision**: Scoped packages encoded as `pkg:npm/%40<scope>/<name>@<version>` per PURL spec. **Rationale**: existing `build_npm_purl` helper at `walk.rs` already handles scope encoding for non-main-module components.

## Phase 1: Design & Contracts

### 1. Data model

`data-model.md` (next run) — captures:

- **NpmMainModuleEntry**: `PackageDbEntry` with constrained shape:
  - `purl`: `pkg:npm/<name>@<version>` or `pkg:npm/%40<scope>/<name>@<version>`
  - `name`: `package.json#name` verbatim
  - `version`: literal `package.json#version` or `"0.0.0-unknown"` placeholder
  - `source`: `Some("path+file://<absolute-package-json-dir>")`
  - `parent_purl`: `None`
  - `sbom_tier`: `Some("source")` per FR-006
  - `extra_annotations`: contains `mikebom:component-role: "main-module"` (C40)
  - `depends`: from `package.json#{dependencies, devDependencies, peerDependencies, optionalDependencies}` (post-scope filter)
  - `licenses`: `vec![]` per FR-005
- **DroppedDuplicate** struct: same as cargo (`purl`, `kept_path`, `dropped_path`)
- **No NpmWorkspaceContext**: npm has no version-inheritance feature, so the single-step resolver doesn't need a workspace map (unlike cargo's `WorkspaceContext`)

### 2. Contracts

`contracts/npm-main-module-component.md` — captures per-format placement contract. Identical structure to cargo's contract; the only differences are PURL prefix (`pkg:npm/...`) and scoped-name encoding. The CDX / SPDX / SPDX 3 invariants are all unchanged from milestone 064 + #127.

### 3. Quickstart

`quickstart.md` — three recipes: single-package express-style, scoped package (`@types/node`), npm 7+ workspace.

### 4. Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` post-plan-commit. No new technologies to register.

### 5. Re-evaluate Constitution Check

Re-checked above table after Phase 1 design — no new violations. The reuse of milestones 053 + 064 + #127 machinery is the design strength: zero marginal generator-side code; one new reader hook + one fixture + a few tests.

**Phase 1 outputs**: this section + `data-model.md` + `contracts/npm-main-module-component.md` + `quickstart.md` (next run).

## Complexity Tracking

*No constitution violations to justify. Section intentionally empty.*
