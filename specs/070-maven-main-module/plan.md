# Implementation Plan: maven source-tree main-module component for top-level pom.xml roots + multi-module reactor builds

**Branch**: `070-maven-main-module` | **Date**: 2026-05-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/070-maven-main-module/spec.md`

## Summary

Extend the milestone 053+064+066+068+069+#127 main-module pattern to maven, the largest and last #104 ecosystem. Emit one main-module per top-level `pom.xml` AND per submodule referenced from a parent's `<modules>` block. POM inheritance resolves missing `<groupId>` / `<version>` from the `<parent>` block; property substitution (`${revision}`, `${project.version}`, custom keys) resolved via the existing `parse_pom_properties` helper. Reuses existing `PomXmlDocument` parser + `build_maven_purl` helper; one parser extension needed (add `modules: Vec<String>` field to `PomXmlDocument`).

## Technical Context

**Language/Version**: Rust stable; no nightly.
**Primary Dependencies**: Existing only — `quick-xml` (already used by `parse_pom_xml`), `serde`, `tracing`, `anyhow`. No new crates.
**Storage**: N/A — in-process per scan.
**Testing**: `cargo +stable test --workspace`; new `maven-multi-module-reactor` fixture; integration tests for the 4 acceptance scenarios.
**Target Platform**: Linux (x86_64 + aarch64) + macOS (aarch64).
**Performance Goals**: One additional `pom.xml` parse per top-level project root + per submodule. Sub-millisecond per POM.
**Constraints**: Cross-host byte-identity goldens. Pure-Rust XML parsing only. No Maven runtime invocation, no `<profiles>` activation, no `~/.m2/settings.xml` reading.
**Scale/Scope**: One main-module per resolvable POM. Single-module projects emit 1; reactor builds emit (1 parent + N submodules).

## Constitution Check

| Principle | Status | Notes |
|-----------|--------|-------|
| **I. Pure Rust, Zero C** | ✅ Pass | XML parsing via `quick-xml`; no Maven runtime. |
| **II. eBPF-Only Observation** | ✅ Pass | Main-module represents the implicit scan target. |
| **III. Fail Closed** | ✅ Pass | Unresolvable GAV → skip emission silently; unresolvable property in version → emit verbatim placeholder + warn (transparent). |
| **IV. Type-Driven Correctness** | ✅ Pass | Reuses `PackageDbEntry`, `Purl`, `PomXmlDocument`. |
| **V. Specification Compliance** | ✅ Pass — **AUDIT PERFORMED** | Native CDX `metadata.component`, SPDX `primaryPackagePurpose: APPLICATION`, SPDX 3 `software_primaryPurpose: application`. PURL conforms to spec (`pkg:maven/<g>/<a>@<v>` with `/` separators per existing `build_maven_purl`); the `:` form from #104's free text was correctly disambiguated to `/` per A11. |
| **VI. Three-Crate Architecture** | ✅ Pass | All changes within `mikebom-cli/`. |
| **VII. Test Isolation** | ✅ Pass | Unit + integration tests; no eBPF, no privileges. |
| **VIII. Completeness** | ✅ Pass | Adds project-self component to maven SBOMs; multi-module reactor builds get all-N submodules represented. |
| **IX. Accuracy** | ✅ Pass | POM inheritance resolved per Maven specification (single-level); property substitution via `parse_pom_properties` is a known-good helper. |
| **X. Transparency** | ✅ Pass | `tracing::warn!` for unresolved properties + same-PURL dedup. |
| **XI. Enrichment** | ✅ Pass | LICENSE detection deferred to issue #103. |
| **XII. External Data Source Enrichment** | ✅ Pass | `pom.xml` is read for the main-module's identity. |
| **Strict Boundary #1 (No lockfile-based dep discovery)** | ✅ Pass | Direct edges relocate from synthetic placeholder to main-module; no new components from POM data. |

**Gate result**: Pass.

## Project Structure

### Documentation (this feature)

```text
specs/070-maven-main-module/
├── plan.md              # This file
├── spec.md              # Feature specification
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── maven-main-module-component.md
├── checklists/
│   └── requirements.md  # All-green
└── tasks.md             # Phase 2 output
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           └── maven.rs               # ⬅️ MAIN CHANGE — new
│                                       #     find_top_level_poms() walker,
│                                       #     resolve_pom_gav() (POM inheritance
│                                       #     + property substitution),
│                                       #     build_maven_main_module_entry(),
│                                       #     dedup_maven_main_modules_by_purl(),
│                                       #     Phase A wire-up in `read()`/
│                                       #     `read_with_claims()`. Also extends
│                                       #     PomXmlDocument with `modules: Vec<String>`
│                                       #     for reactor traversal.
└── tests/
    ├── fixtures/
    │   └── maven-multi-module-reactor/
    │       ├── pom.xml                  # parent: groupId, artifactId,
    │       │                             # version, packaging=pom, modules=[a, b]
    │       ├── module-a/pom.xml         # inherits groupId+version from parent
    │       └── module-b/pom.xml         # explicit GAV; ${project.version}
    │                                    # property substitution
    └── scan_maven.rs                    # ⬅️ NEW or extend — 5 integration tests:
                                          # AS#1 single-module, AS#2 reactor,
                                          # AS#3 parent inheritance, AS#4 property
                                          # substitution, FR-003 install-state skip

docs/
└── reference/
    └── sbom-format-mapping.md          # ⬅️ DOC UPDATE — extend C40 row's
                                         #   per-ecosystem matrix to ALL 6:
                                         #   Go ✅, cargo ✅, npm ✅, pip ✅,
                                         #   gem ✅, maven ✅. #104 fully
                                         #   closed after this milestone.

CHANGELOG.md                            # ⬅️ DOC UPDATE — `[Unreleased]` →
                                         #   `### Changed (BREAKING — SBOM
                                         #   output shape, milestone 070)`.
                                         #   Note this closes #104 entirely.
```

**Structure Decision**: Single-crate (`mikebom-cli`) feature. The maven reader (`scan_fs/package_db/maven.rs`) gains the new helpers; generator-side machinery is unchanged. New fixture exercises the multi-module reactor + parent inheritance + property substitution paths.

## Phase 0: Outline & Research — COMPLETE (in-spec)

Phase 0 captured in spec Assumptions A1–A11. Key decisions:

- **Decision**: Reuse existing `PomXmlDocument` + `parse_pom_xml` + `parse_pom_properties` + `build_maven_purl`. **Rationale**: maven.rs already has comprehensive XML parsing for the dep-emission path; no point duplicating. **Alternatives**: standalone parser for main-module emission (rejected — divergence risk, more code).
- **Decision**: Extend `PomXmlDocument` with `modules: Vec<String>` field to capture `<modules>/<module>` elements. **Rationale**: needed for FR-002 multi-module reactor traversal; minimal addition (~10 LOC in the event-driven parser). **Alternatives**: parse modules in a separate pass (rejected — duplicates XML parsing).
- **Decision**: POM inheritance is single-level (A9). **Rationale**: BOM-of-BOM-of-BOM chains are rare and require effective-pom resolution which would mean walking transitive parents (out of repo for in-tree main-module emission). **Alternatives**: full transitive parent chain walk (rejected — complex; mostly unnecessary for typical projects).
- **Decision**: Unresolved property → emit verbatim placeholder + warn. **Rationale**: cross-host determinism + visibility. Operator sees `${unresolved.prop}` in the PURL as a signal to investigate. **Alternatives**: skip emission (rejected — loses the project-self signal); use `0.0.0-unknown` (rejected — would mask the misconfiguration; verbatim shows what's wrong).
- **Decision**: Reactor parent without own GAV → skip emission for parent, emit per-submodule (Edge Cases). **Rationale**: same posture as gem (069) FR-002 application-style projects. A pure aggregator parent has no project-self identity.

## Phase 1: Design & Contracts

### 1. Data model

`data-model.md` — captures:

- **MavenMainModuleEntry**: `PackageDbEntry` constrained to top-level `pom.xml` emission. PURL `pkg:maven/<g>/<a>@<v>`; `parent_purl: None`; `sbom_tier: Some("source")`; C40 + `mikebom:component-role: "main-module"`; depends from POM `<dependencies>` block via existing extractor.
- **PomXmlDocument** extension: add `pub modules: Vec<String>` field for `<modules>/<module>` elements.
- **MavenInheritanceContext** (new private helper): given a child POM's `<parent>` block AND a map of (groupId, artifactId, version) → `PomXmlDocument` parsed from the same scan tree, resolve missing GAV components for the child. For free-standing children whose parent is outside the scan tree, fall back to the child's `<parent>` block's literal values (which carry the parent's GAV).
- **Property substitution helper**: `resolve_pom_property(value: &str, doc: &PomXmlDocument, parent_doc: Option<&PomXmlDocument>) -> Option<String>` — handles `${project.groupId}`, `${project.artifactId}`, `${project.version}`, `${parent.groupId}`, `${parent.version}`, `${revision}`, and custom `<properties>` keys per FR-012.
- **MavenDroppedDuplicate**: same shape as cargo/npm/pip/gem.

### 2. Contracts

`contracts/maven-main-module-component.md` — per-format placement contract identical to cargo/npm/pip/gem with PURL prefix `pkg:maven/<g>/<a>@<v>`. Multi-module reactor case = multi-main-module super-root via #127 infrastructure.

### 3. Quickstart

`quickstart.md` — four recipes: single-module, multi-module reactor, parent inheritance, property substitution.

### 4. Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` post-commit. No new technologies.

### 5. Re-evaluate Constitution Check

Re-checked above table — no new violations.

**Phase 1 outputs**: this section + `data-model.md` + `contracts/maven-main-module-component.md` + `quickstart.md` (next run).

## Complexity Tracking

*No constitution violations to justify. Section intentionally empty.*
