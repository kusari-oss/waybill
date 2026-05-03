# Implementation Plan: pip source-tree main-module component for PEP 621 pyproject.toml roots

**Branch**: `068-pip-main-module` | **Date**: 2026-05-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/068-pip-main-module/spec.md`

## Summary

Extend the milestone 053+064+066+#127 main-module pattern to pip: emit one synthetic main-module per `pyproject.toml` containing PEP 621 `[project]` table (skipping Poetry-only `[tool.poetry]` schemas per #104), placed in standards-native "BOM subject" slots. Reuses every generator-side hook unchanged — the C40-tag-driven predicates already work for any new ecosystem. Also adds an editable-install merge case (FR-011): when a venv `.dist-info` shares the same PURL as the manifest-derived main-module, venv evidence (`sbom_tier: deployed`, hashes) takes precedence to preserve the upstream signal that the project IS installed, not just declared.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited; no nightly).
**Primary Dependencies**: Existing only — `toml = "0.8"` (already used by `mikebom-cli/src/scan_fs/package_db/cargo.rs:305` and indirectly elsewhere), `serde`/`serde_json`, `tracing`, `anyhow`. **No new crates.** No subprocess calls.
**Storage**: N/A — all state in-process per scan.
**Testing**: `cargo +stable test --workspace`; existing golden infrastructure regen for pip-bearing fixtures; new `tests/fixtures/pip-pyproject-pep621/` fixture exercising basic PEP 621 emission + name normalization.
**Target Platform**: Linux (x86_64 + aarch64) + macOS (aarch64).
**Project Type**: CLI (workspace-rooted; reuses `mikebom-cli/src/scan_fs/package_db/pip/mod.rs` + the milestone-053+064+#127 generator-side machinery).
**Performance Goals**: One additional `pyproject.toml` parse per discovered project root. Sub-millisecond impact.
**Constraints**: Cross-host byte-identity for goldens. PEP 503 normalization is pure-string (no host state).
**Scale/Scope**: One main-module per `pyproject.toml` with `[project]` discovered. Single-project scans emit 1; monorepos with N Python projects emit N.

## Constitution Check

Per `.specify/memory/constitution.md` v1.4.0 — same gate-pass posture as milestones 053+064+066:

| Principle | Status | Notes |
|-----------|--------|-------|
| **I. Pure Rust, Zero C** | ✅ Pass | All changes in pure Rust. No subprocess calls. |
| **II. eBPF-Only Observation** | ✅ Pass | Main-module represents the workspace root that's the implicit scan target. |
| **III. Fail Closed** | ✅ Pass | Lenient fallback to `0.0.0-unknown` when version unresolvable; Poetry-only manifests skip with info-level log. |
| **IV. Type-Driven Correctness** | ✅ Pass | Reuses `PackageDbEntry`, `Purl`, `toml::Value`. |
| **V. Specification Compliance** | ✅ Pass — **AUDIT PERFORMED** | Native CDX `metadata.component`, SPDX `primaryPackagePurpose: APPLICATION`, SPDX 3 `software_primaryPurpose: application`. PEP 503 name normalization conforms to PURL spec for `pkg:pypi/...`. |
| **VI. Three-Crate Architecture** | ✅ Pass | All changes within `mikebom-cli/`. |
| **VII. Test Isolation** | ✅ Pass | Unit-level + integration-level tests; no eBPF, no privileges. |
| **VIII. Completeness** | ✅ Pass | Adds project-self component to pip SBOMs that today have none. |
| **IX. Accuracy** | ✅ Pass | Manifest-derived versions are authoritative. PEP 503 normalization deterministic. |
| **X. Transparency** | ✅ Pass | Same-PURL dedup `tracing::warn!`; Poetry-skip `tracing::info!`; editable-install merge tracked via existing evidence machinery. |
| **XI. Enrichment** | ✅ Pass | LICENSE detection deferred to issue #103. |
| **XII. External Data Source Enrichment** | ✅ Pass | `pyproject.toml`'s `[project]` table is read for the main-module's identity. |
| **Strict Boundary #1 (No lockfile-based dep discovery)** | ✅ Pass | `pyproject.toml`'s `[project.dependencies]` reuse the existing edge-relocation pattern; no new components introduced. |

**Gate result**: Pass.

## Project Structure

### Documentation (this feature)

```text
specs/068-pip-main-module/
├── plan.md              # This file
├── spec.md              # Feature specification (no Clarifications needed — spec was tight)
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── pip-main-module-component.md   # Phase 1 output
├── checklists/
│   └── requirements.md  # All-green
└── tasks.md             # Phase 2 output
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       └── pip/
│   │           ├── mod.rs                # ⬅️ MAIN CHANGE — new
│   │           │                          #     build_pip_main_module_entry()
│   │           │                          #     + dedup_pip_main_modules_by_purl()
│   │           │                          #     wired into `read()` after the
│   │           │                          #     existing per-project-root tier
│   │           │                          #     loop. Augment-existing-entry
│   │           │                          #     pattern from cargo (064) +
│   │           │                          #     npm (066). Editable-install
│   │           │                          #     merge (FR-011) is a special
│   │           │                          #     case of augment: venv-tier
│   │           │                          #     entries from Tier-1 take
│   │           │                          #     precedence on `sbom_tier` and
│   │           │                          #     `evidence_kind`.
│   │           ├── poetry.rs              # (no change — Poetry lockfile
│   │           │                          #  emission unchanged; FR-002 says
│   │           │                          #  Poetry-only manifests are skipped
│   │           │                          #  for main-module purposes only)
│   │           ├── pipfile.rs             # (no change)
│   │           ├── requirements_txt.rs    # (no change)
│   │           └── dist_info.rs           # (no change — venv reader produces
│   │                                       #  same-PURL entries that get merged
│   │                                       #  via FR-011 augment-existing logic)
│   ├── generate/                          # (no change — already C40-tag-driven)
│   └── parity/                            # (no change — C40 already wired)
└── tests/
    ├── fixtures/
    │   ├── pip-pyproject-pep621/          # ⬅️ NEW FIXTURE — basic PEP 621 root
    │   │   ├── pyproject.toml             #   [project] name="my_pkg" version="1.0.0"
    │   │   └── README.md
    │   └── pip-pyproject-poetry-only/     # ⬅️ NEW FIXTURE — Poetry-only schema
    │       ├── pyproject.toml             #   [tool.poetry] only — must skip
    │       │                              #   per FR-002
    │       └── README.md
    └── scan_pip.rs                        # ⬅️ NEW TESTS — integration tests
                                            #   for US1 AS#1-4 + FR-002 skip

docs/
├── reference/
│   └── sbom-format-mapping.md             # ⬅️ DOC UPDATE — extend C40 row's
│                                           #   per-ecosystem matrix: Go ✅,
│                                           #   cargo ✅, npm ✅, pip ✅;
│                                           #   maven/gem still in #104.
└── design-notes.md                        # ⬅️ DOC UPDATE — bump per-ecosystem
                                            #   coverage status.

CHANGELOG.md                               # ⬅️ DOC UPDATE — `[Unreleased]` →
                                            #   `### Changed (BREAKING — SBOM
                                            #   output shape, milestone 068)`
```

**Structure Decision**: Single-crate (`mikebom-cli`) feature. The pip reader (`scan_fs/package_db/pip/mod.rs`) gains the new helpers; generator-side machinery is unchanged. New fixtures cover the basic case (PEP 621 root) + the FR-002 skip case (Poetry-only schema).

## Phase 0: Outline & Research — COMPLETE (in-spec)

Phase 0 is captured in spec Assumptions A1–A11. Key decisions:

- **Decision**: Skip emission for `[tool.poetry]`-only manifests; emit when `[project]` is present even if `[tool.poetry]` is also present (Poetry 1.5+ shim case). **Rationale**: per #104 + PEP 621's standards-native authority. **Alternatives**: extend `[tool.poetry]` reading (rejected — Poetry's schema differs subtly + this is well-scoped follow-up territory).
- **Decision**: PEP 503 name normalization via existing `normalize_pypi_name_for_purl` helper. **Rationale**: consistent with mikebom's existing pip dep-component PURL building. **Alternatives**: emit name verbatim (rejected — would diverge from how dep components are emitted, breaking `name_to_purl` lookups for FR-007 edge resolution).
- **Decision**: `dynamic = ["version"]` → `0.0.0-unknown` placeholder. **Rationale**: cross-host determinism per 053/064/066 convention; avoids host-state dependencies (no setuptools-scm shellout). **Alternatives**: shell out to setuptools-scm (rejected — runtime dep on Python toolchain + host git state).
- **Decision**: Editable-install merge precedence — venv evidence wins for `sbom_tier`, `evidence_kind`, `hashes`; Phase A wins for C40 + `parent_purl: None`. **Rationale**: venv signals "actually installed" which is stronger evidence than the manifest alone; C40 + top-level-ness are project-identity signals that the venv reader can't supply.

No further research needed.

## Phase 1: Design & Contracts

### 1. Data model

`data-model.md` — captures:

- **PipMainModuleEntry**: `PackageDbEntry` constrained to PEP 621 emission. PURL `pkg:pypi/<pep503-normalized-name>@<version>`; `parent_purl: None`; `sbom_tier: Some("source")` (overridden to `"deployed"` when venv-merged); C40 + `mikebom:component-role: "main-module"`.
- **DroppedDuplicate** struct: same shape as cargo (064) / npm (066). Returned from `dedup_pip_main_modules_by_purl`.
- **No PipWorkspaceContext**: Python doesn't have workspace-version-inheritance.

### 2. Contracts

`contracts/pip-main-module-component.md` — per-format placement contract identical to cargo/npm with PURL prefix `pkg:pypi/...` and PEP 503 name normalization.

### 3. Quickstart

`quickstart.md` — three recipes: PEP 621 single-project (e.g., httpx), name normalization (denormalized name → normalized PURL), Poetry-only skip.

### 4. Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` post-plan-commit. No new technologies to register.

### 5. Re-evaluate Constitution Check

Re-checked above table — no new violations.

**Phase 1 outputs**: this section + `data-model.md` + `contracts/pip-main-module-component.md` + `quickstart.md` (next run).

## Complexity Tracking

*No constitution violations to justify. Section intentionally empty.*
