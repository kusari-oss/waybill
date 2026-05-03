# Feature Specification: pip source-tree main-module component for PEP 621 pyproject.toml roots

**Feature Branch**: `068-pip-main-module`
**Created**: 2026-05-03
**Status**: Draft
**Input**: User description: "let's move onto t hte next ecosystems" — interpreted as: pick the next #104 ecosystem; pip is the highest-leverage remaining (Python is the second-most-used after JS/npm). Maven and gem follow as separate milestones.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Python project SBOMs identify the project itself (Priority: P1)

A developer or CI pipeline runs `mikebom sbom scan --path <python-project>` against a Python project. The resulting SBOM contains a component identifying the project itself — `pkg:pypi/<name>@<version>` — alongside its dependencies. Today, scanning a Python project emits dependency components from `requirements.txt` / `Pipfile.lock` / `poetry.lock` / installed venv but no component representing the project-being-scanned, so the SBOM cannot answer "what is this an SBOM for?" without falling back to filesystem path heuristics.

**Why this priority**: This is the dominant value of issue #104 for pip. Every Python project's SBOM today is missing its own row, so vuln-intersection tools, dependency-graph visualizers, and `documentDescribes`-following consumers all see a placeholder root instead of the project. Python is the second-most-used package ecosystem after npm; closing this gap completes the high-volume-ecosystem trio (Go ✅ alpha.10, cargo ✅ + npm ✅ alpha.12, pip in this milestone). The pattern is well-trodden by milestones 053+064+066+#127 — the C40-tag-driven generator hooks already work for any new ecosystem; this milestone is mostly reader-side wire-up.

**Independent Test**: Clone any Python project with PEP 621 `[project]` declared in `pyproject.toml` (e.g., `git clone https://github.com/pypa/pip`), run `mikebom sbom scan --path <project> --format spdx-2.3-json --output sbom.json --no-deep-hash`, and verify the output contains exactly one package whose PURL is `pkg:pypi/<name>@<version>` derived from the manifest with PEP 503 name normalization applied (lowercase, hyphens).

**Acceptance Scenarios**:

1. **Given** a single-project Python directory with `pyproject.toml` containing `[project]` table declaring `name = "foo"` and `version = "1.2.3"`, **When** `mikebom sbom scan --path <project>` runs, **Then** the resulting SBOM contains exactly one component with PURL `pkg:pypi/foo@1.2.3` placed in each format's standards-native "BOM subject" slot (CycloneDX `metadata.component`, SPDX 2.3 `documentDescribes` target, SPDX 3 `DESCRIBES` target).
2. **Given** a `pyproject.toml` with `[project]` declaring a name like `Some_Package.Name` (mixed case + underscore + dot), **When** mikebom scans, **Then** the main-module's PURL applies PEP 503 normalization: lowercase + hyphenate dots/underscores → `pkg:pypi/some-package-name@<version>`.
3. **Given** a `pyproject.toml` with `[project]` declaring `dynamic = ["version"]` (no literal version — version comes from setuptools-scm or `__version__`), **When** mikebom scans, **Then** the main-module emits with the literal `0.0.0-unknown` placeholder per the cross-host determinism convention from milestones 053/064/066.
4. **Given** a `pyproject.toml` with `[tool.poetry]` instead of `[project]` (Poetry's pre-PEP-621 schema), **When** mikebom scans, **Then** the main-module is skipped per FR-002 and a `tracing::info!` notes the skip with a pointer to a future Poetry-extension follow-up issue. Existing Poetry lockfile-driven dep emission is unaffected.

---

### User Story 2 - Main-module component is identifiable and excludable (Priority: P2)

Same use case as milestones 064 (cargo) US2 and 066 (npm) US2: downstream tools (sbomqs, vuln scanners, license-compliance tooling) can distinguish the synthetic main-module from real third-party deps via the C40 supplementary `mikebom:component-role: main-module` annotation alongside each format's standards-native "BOM subject" slot.

**Why this priority**: Same posture as Go + cargo + npm. The C40 tag is reused as-is; no new annotation infrastructure required.

**Independent Test**: Run a pip scan that produces the new main-module component; assert the C40 annotation is present in CDX `metadata.component.properties`, SPDX 2.3 annotations, and SPDX 3 native field, parallel to milestones 053/064/066.

**Acceptance Scenarios**:

1. **Given** a pip scan producing a main-module, **When** rendered as CycloneDX 1.6, **Then** the main-module is in `metadata.component` with `type: "application"` AND carries `properties[].name = "mikebom:component-role"` with `value = "main-module"`.
2. **Given** the same scan, **When** rendered as SPDX 2.3, **Then** the main-module has `primaryPackagePurpose: "APPLICATION"` AND a C40 annotation envelope.
3. **Given** the same scan, **When** rendered as SPDX 3.0.1, **Then** the main-module has `software_primaryPurpose: "application"` AND the C40-mapped native field.
4. **Given** the new main-module, **When** sbomqs runs, **Then** licensing-coverage doesn't degrade by more than 1pp vs. pre-068 baseline.

---

### User Story 3 - Document root points at pip main-module (Priority: P3)

Inherits the multi-DESCRIBES super-root behavior from milestone 064 + #127. Single-project scans get length-1 `documentDescribes`; polyglot scans (pip + cargo + Go + npm) extend the existing super-root mechanism.

**Why this priority**: Cosmetic / tool-friendliness on top of US1. SPDX-tree-walking tools (sbomqs root scoring, GitHub dep visualizations, GUAC ingest) get a more accurate root.

**Independent Test**: Run a single-project pip scan; assert `documentDescribes` contains exactly the pip main-module's SPDXID. Run a polyglot scan with pip + npm + cargo; assert all three main-modules appear in `documentDescribes`, alphabetically sorted by SPDXID.

**Acceptance Scenarios**:

1. **Given** a single Python project, **When** rendered as SPDX 2.3, **Then** `documentDescribes` is exactly `[<pip-main-module-spdxid>]` (length 1) and the corresponding DESCRIBES relationship exists.
2. **Given** a polyglot project (pip + cargo + Go), **When** mikebom scans, **Then** the SPDX `documentDescribes` extends to include the pip main-module alongside cargo and Go main-modules, deterministically sorted.

---

### Edge Cases

- **Poetry `[tool.poetry]` schema (no `[project]`)**: skip main-module emission per FR-002 + #104 explicit guidance ("Skip for poetry projects using `[tool.poetry]` with a different schema (or extend to cover them)"). A future follow-up issue will add Poetry coverage if user demand surfaces. Existing Poetry lockfile-driven dep emission (`pip/poetry.rs`) is unaffected.
- **`pyproject.toml` with both `[project]` AND `[tool.poetry]`**: the Poetry-shim case (Poetry 1.5+ supports PEP 621 alongside `[tool.poetry]`). Emit a main-module from `[project]` (the standards-native source) and ignore `[tool.poetry]` for main-module purposes. Operators using Poetry 1.5+ get full coverage.
- **`dynamic = ["version"]`**: literal `0.0.0-unknown` placeholder per FR-001 step 3, matching the cross-host determinism convention from milestones 053 (Go's `git describe` ladder step 3), 064 (cargo's resolver fallback), and 066 (npm's missing-version fallback).
- **`name` with PEP 503 normalization edge cases**: `Foo_Bar.Baz` → `pkg:pypi/foo-bar-baz`. `XYZ___123` → `pkg:pypi/xyz-123` (consecutive underscores collapse). Apply existing `normalize_pypi_name_for_purl` helper at `mikebom-cli/src/scan_fs/package_db/pip/mod.rs`.
- **Pre-release versions (`1.0.0a1`, `2.0.0rc1`, `1.0.0.dev0`)**: PEP 440 / Python's specific pre-release form. Used verbatim in the PURL (PEP 440 strings are URL-segment-safe).
- **Local version segments (`1.0.0+local.build1`)**: PEP 440's `+` separator. Encode `+` to `%2B` per PURL spec; the existing `build_pypi_purl_str` helper at `mod.rs:79` already does this via `encode_purl_segment`.
- **Editable installs (`pip install -e .`) of the project**: the venv may have a `.dist-info` for the project itself. The Tier-1 venv reader currently emits this as a deployed-tier component. Post-068, the Phase-A main-module emission has the same PURL — augment-existing-or-emit-new merges them, preserving the venv's accurate version + deployed-tier evidence on the main-module identity.
- **`name` missing in `[project]`**: skip main-module emission (no identity). Common for `pyproject.toml` configs that only declare `[build-system]` / `[tool.*]` but no `[project]` (yet — pre-PEP-621 layout).
- **`version` missing in `[project]` (and not `dynamic`)**: per PEP 621, `[project].version` is required UNLESS `version` is in `[project].dynamic`. Treat missing-version-without-dynamic as a malformed manifest: emit with `0.0.0-unknown` placeholder + `tracing::warn!` noting the malformation (lenient parse, never block the scan).
- **Same-PURL collisions across discovered `pyproject.toml` files**: rare for pip given the existing walker excludes `__pycache__/`, `.venv/`, etc. via `should_skip_descent`. When they happen, exactly one main-module emits (deterministic first-discovered-wins) and a `tracing::warn!` lists dropped duplicate paths. Same convention as cargo (064) / npm (066) per spec Clarifications Q1 of those milestones.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every `pyproject.toml` discovered during a source-tree scan that contains a `[project]` table with a declared `name`, mikebom MUST emit a single component representing that project, with PURL `pkg:pypi/<name>@<version>`. Name MUST be PEP 503-normalized (lowercase, all `_` and `.` collapsed to `-`) before encoding into the PURL — reuse the existing `normalize_pypi_name_for_purl` helper. Version resolution: (1) if `[project].version` is a literal string, use it verbatim; (2) if `[project].dynamic` array contains `"version"`, emit with the literal `0.0.0-unknown` placeholder; (3) if `[project].version` is missing AND not in `dynamic`, emit with `0.0.0-unknown` + `tracing::warn!` (malformed manifest, lenient).

- **FR-001a (placement)**: The pip main-module component MUST be emitted via each format's standards-native "BOM subject" construct, identical to cargo (064) and npm (066) wiring. Inherits the C40-tag-driven hooks established in milestones 053+064+#127:
  - **CycloneDX 1.6**: `metadata.component` (single) or `components[]` siblings under super-root (multi-project polyglot), `type: "application"`.
  - **SPDX 2.3**: `packages[]` entry with `primaryPackagePurpose: "APPLICATION"` + `documentDescribes[]` targeting.
  - **SPDX 3.0.1**: `software_primaryPurpose: "application"` + `DESCRIBES` Relationship.

- **FR-002**: For `pyproject.toml` files declaring ONLY `[tool.poetry]` (no `[project]`), mikebom MUST NOT emit a main-module. The Poetry pre-PEP-621 schema is explicitly out of scope for milestone 068 per #104's guidance. A `tracing::info!` notes the skip with a pointer to a future Poetry-extension follow-up issue (filed by this milestone if demand surfaces). Existing Poetry lockfile-driven dep emission is unaffected.

- **FR-003**: For `pyproject.toml` declaring BOTH `[project]` AND `[tool.poetry]` (Poetry 1.5+ shim), mikebom MUST emit a main-module from `[project]` and ignore `[tool.poetry]` for main-module purposes. The standards-native PEP 621 source wins.

- **FR-004**: The pip main-module component MUST also carry `mikebom:component-role: main-module` (catalog row C40) as a supplementary signal across all three formats. Inherits the existing C40 wiring from milestone 053; no new annotation infrastructure required.

- **FR-005**: The pip main-module component MUST emit with an empty `licenses` field. License detection (PEP 621's `license` field, classifier strings, LICENSE-file content matching) is out of scope for milestone 068 and tracked as a follow-up to issue #103.

- **FR-006**: The pip main-module component MUST carry `mikebom:sbom-tier: source` per the existing tier-classification convention.

- **FR-007**: Direct-dep edges from the pip main-module to its dependencies MUST originate from the main-module's PURL. The lockfile-driven dep-emission (poetry.lock, Pipfile.lock, requirements.txt) integrates with the milestone-064-style augment-existing-entry logic so the dep edges those readers emit attach to the main-module rather than the synthetic `DocumentRoot-*` placeholder. Inherits the same `name_to_purl` resolution + dangling-target-drop convention as cargo + npm.

- **FR-008**: The SPDX 2.3 `documentDescribes[]` array (and SPDX 3 `rootElement[]`, CycloneDX `metadata.component`) MUST point at the pip main-module component(s). Polyglot scans extend the existing milestone-064-#127 multi-DESCRIBES super-root mechanism.

- **FR-009**: The pip main-module emission MUST NOT alter the existing component count from `pyproject.toml`'s `[project.dependencies]` or `[project.optional-dependencies]` tables (other than removing the synthetic root placeholder when one would have been emitted). Total dependency-graph edge count from the project's own package MUST be byte-equivalent to pre-068 modulo the placeholder→main-module identifier swap.

- **FR-010**: The new pip main-module component MUST be excluded from `mikebom:not-linked` annotation eligibility (milestone 050) — inherits the existing C40-tag-driven guard from milestone 064.

- **FR-011**: Editable installs (`pip install -e .`) of the project under scan: when the Tier-1 venv reader emits a `.dist-info` entry with the same `pkg:pypi/<name>@<version>` PURL as the milestone-068 main-module, the augment-existing-entry logic merges the venv's evidence (deployed-tier classification, hashes, etc.) onto the main-module identity. The C40 tag wins from Phase A; the venv's `sbom_tier` and `evidence_kind` fields take precedence over the main-module defaults to preserve the upstream signal that this is an installed, not just a manifested, artifact.

### Key Entities

- **pip main-module component**: A synthetic SBOM component representing a single Python project at a `pyproject.toml` discovered during scan. Identified by `pkg:pypi/<name>@<version>` (PEP 503-normalized name). Carries `primaryPackagePurpose: APPLICATION`, the C40 supplementary role tag, and `mikebom:sbom-tier: source`. Source of all `[project.dependencies]` / `[project.optional-dependencies]` direct edges (when those exist; many Python projects defer dep declarations to `requirements.txt` or lockfiles).

- **PEP 621 project root**: A `pyproject.toml` containing a `[project]` table. Emits one main-module per FR-001.

- **Poetry-only project root**: A `pyproject.toml` containing `[tool.poetry]` but no `[project]`. Skipped for main-module emission per FR-002; existing Poetry lockfile-driven dep emission unchanged.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of Python project scans containing a `pyproject.toml` with `[project].name` declared (and not skipped per FR-002's Poetry-only rule) emit at least one pip main-module component in the resulting SBOM. Verified by integration tests scanning a single-project fixture, an editable-install fixture (FR-011), and at least one realistic OSS Python project (e.g., `pipx`, `httpx`, or `ruff`).

- **SC-002**: PEP 503 name normalization is applied correctly. Manifest names with mixed case + underscores + dots → PURL with lowercase + hyphens. Verified by an integration test using a fixture with a deliberately denormalized `name`.

- **SC-003**: sbomqs licensing-coverage score for the pip fixture does not regress by more than 1 percentage point vs. the pre-068 baseline. The C40 role tag (FR-004) excludes the new main-module from the denominator.

- **SC-004**: Byte-identity goldens hold across hosts. The pip fixture's CycloneDX, SPDX 2.3, and SPDX 3 goldens regenerate identically on Linux x86_64, Linux aarch64, and macOS aarch64 runners per the cross-host playbook.

- **SC-005**: SPDX 2.3 `documentDescribes` array for any pip-only single-project scan contains the pip main-module's SPDXID directly. The synthetic `DocumentRoot-*` placeholder no longer appears in pip-only outputs (when `[project]` is present).

## Assumptions

- **A1 (manifest authoritative)**: `pyproject.toml`'s `[project].name` and `[project].version` are the canonical source for the main-module's PURL. PEP 621 is the standards-native Python project metadata location; mikebom defers to it.

- **A2 (PEP 503 normalization)**: Name normalization follows PEP 503 + the existing `normalize_pypi_name_for_purl` helper. This is identical to how mikebom already builds PURLs for pip-derived dep components, ensuring consistency.

- **A3 (Poetry deferred)**: `[tool.poetry]`-only manifests are explicitly skipped per #104. A follow-up issue will be filed if demand for Poetry coverage materializes — Poetry's schema is similar enough that adding it later is a small extension to the FR-001 reader. The existing `pip/poetry.rs` lockfile-driven dep emission is unaffected.

- **A4 (license deferred)**: License detection for the pip main-module is out of scope. The C40 carve-out from milestone 053 already protects sbomqs scoring. PEP 621's `license` field, classifier strings, and LICENSE-file detection tracked as follow-up to issue #103.

- **A5 (`__pycache__/`, `.venv/` excluded)**: The existing pip walker honors `should_skip_descent` for these patterns. Manifests inside excluded dirs are not discovered for main-module emission. This matches the npm `node_modules/` exclusion (066 FR-003) — these are caches/installations, not project-internal artifacts.

- **A6 (no binary path interaction)**: Python doesn't have a binary-discovery path that emits main-modules (no equivalent to Go's BuildInfo). The pip main-module is unconditionally source-tree-derived. Mirrors cargo (064 A6) + npm (066 A6).

- **A7 (existing scope filtering preserved)**: All existing milestone-052/part-3 `--exclude-scope` filtering continues unchanged; FR-007 only relocates the edge origin.

- **A8 (other ecosystems unchanged)**: This milestone is pip-specific. cargo, Go, npm, maven, gem behaviors are untouched.

- **A9 (super-root reuse)**: Multi-main-module super-root + plural-DESCRIBES from milestone 064+#127 ships unchanged. pip main-modules slot in as additional describable elements.

- **A10 (lockfile / requirements format-agnostic)**: Main-module emission reads only `pyproject.toml`'s `[project]` table. Whether the project uses pip (`requirements.txt`), pipenv (`Pipfile.lock`), or poetry (`poetry.lock`) is irrelevant — the existing per-format lockfile readers continue to drive transitive component emission.

- **A11 (editable install merge)**: When a venv `.dist-info` entry shares the same PURL as a Phase-A main-module (FR-011), evidence from the venv (`sbom_tier: deployed`, hashes) takes precedence over Phase-A defaults. Rationale: the venv signal is stronger evidence than the manifest alone — the project IS installed, not just declared.
