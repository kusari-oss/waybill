# Feature Specification: npm source-tree main-module component for package.json roots + workspace members

**Feature Branch**: `066-npm-main-module`
**Created**: 2026-05-03
**Status**: Draft
**Input**: User description: "Add npm main-module component for source-tree projects with package.json. Parallels milestones 064 (cargo) + 053 (Go), closes the npm slice of issue #104."

## Clarifications

### Session 2026-05-03

- Q: How should `package.json` with `name` but no `version` (and not `private`) be handled? → A: **Match cargo's permissive behavior** — emit a main-module with the literal `0.0.0-unknown` placeholder version. Same fallback semantics as cargo's `resolve_cargo_main_module_version` step 3c (when no resolution path produces a real version). Preserves ecosystem-consistent behavior across cargo, Go, and npm; consumers that filter out placeholder versions can do so uniformly. Mirrors milestone 053 (Go) FR-001 step 3 and milestone 064 (cargo) FR-001's behavior.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - npm project SBOMs identify the project itself (Priority: P1)

A developer or CI pipeline runs `mikebom sbom scan --path <npm-project>` against a Node.js project. The resulting SBOM contains a component identifying the project itself — `pkg:npm/<name>@<version>` (or `pkg:npm/%40<scope>/<name>@<version>` for scoped packages) — alongside its dependencies. Today, scanning a Node.js project emits dependency components from `package-lock.json` / `pnpm-lock.yaml` but no component representing the project-being-scanned, so the SBOM cannot answer "what is this an SBOM for?" without falling back to filesystem path heuristics.

**Why this priority**: This is the dominant value of issue #104 for npm. Every Node.js project's SBOM today is missing its own row, so vuln-intersection tools, dependency-graph visualizers, and `documentDescribes`-following consumers all see a placeholder root instead of the project. npm is the highest-volume package ecosystem in the world; closing this gap is the single highest-leverage move from #104. Pattern-matches what shipped for Go (milestone 053) and cargo (milestone 064).

**Independent Test**: Clone any Node.js project with `name` and `version` declared in `package.json` (e.g., `git clone https://github.com/expressjs/express`), run `mikebom sbom scan --path <project> --format spdx-2.3-json --output sbom.json --no-deep-hash`, and verify the output contains exactly one package whose PURL is `pkg:npm/<name>@<version>` derived verbatim from `package.json`.

**Acceptance Scenarios**:

1. **Given** a single-package Node.js project with `package.json` declaring `name = "foo"` and `version = "1.2.3"`, **When** `mikebom sbom scan --path <project>` runs, **Then** the resulting SBOM contains exactly one component with PURL `pkg:npm/foo@1.2.3` placed in each format's standards-native "BOM subject" slot (CycloneDX `metadata.component`, SPDX 2.3 `documentDescribes` target, SPDX 3 `DESCRIBES` target).
2. **Given** a scoped-package project with `name = "@types/node"` and `version = "20.5.0"`, **When** mikebom scans, **Then** the main-module's PURL is `pkg:npm/%40types/node@20.5.0` per the PURL spec's URL-encoding of the `@` scope sigil.
3. **Given** an npm 7+ workspace whose root `package.json` declares `private: true` (no version) and `workspaces: ["packages/a", "packages/b"]`, with each member having its own `package.json` declaring `name` + `version`, **When** mikebom scans, **Then** the SBOM contains exactly two main-module components (one per workspace member), the workspace root itself emits NO main-module (per its `private: true` + no-version signal), and the document's `DESCRIBES` relationship targets both members in deterministic name-sorted order.
4. **Given** a `package.json` with `private: true` AND a declared `version`, **When** mikebom scans, **Then** the main-module IS emitted (the `version` declaration outweighs the `private` hint — `private` blocks accidental npm publish but doesn't signal "not a real artifact").

---

### User Story 2 - Main-module component is identifiable and excludable (Priority: P2)

Same use case as milestone 064 US2: downstream tools (sbomqs, vuln scanners, license-compliance tooling) can distinguish the synthetic main-module from real third-party deps via the C40 supplementary `mikebom:component-role: main-module` annotation alongside each format's standards-native "BOM subject" slot.

**Why this priority**: Without the C40 signal, sbomqs licensing-coverage scoring penalizes the project-self component (no upstream npm registry license metadata is fetched for it), and vuln scanners waste cycles looking it up. Same posture as Go + cargo.

**Independent Test**: Run an npm scan that produces the new main-module component; assert the C40 annotation is present in CDX `metadata.component.properties`, SPDX 2.3 annotations, and SPDX 3 native field, parallel to cargo (064) and Go (053).

**Acceptance Scenarios**:

1. **Given** an npm scan producing a main-module, **When** rendered as CycloneDX 1.6, **Then** the main-module is in `metadata.component` with `type: "application"` AND carries `properties[].name = "mikebom:component-role"` with `value = "main-module"`.
2. **Given** the same scan, **When** rendered as SPDX 2.3, **Then** the main-module has `primaryPackagePurpose: "APPLICATION"` AND a C40 annotation envelope.
3. **Given** the same scan, **When** rendered as SPDX 3.0.1, **Then** the main-module has `software_primaryPurpose: "application"` AND the C40-mapped native field.
4. **Given** the new main-module, **When** sbomqs runs, **Then** licensing-coverage doesn't degrade by more than 1pp vs. pre-066 baseline.

---

### User Story 3 - Document root points at npm main-module(s) (Priority: P3)

Inherits the multi-DESCRIBES super-root behavior from milestone 064 + #127. Cargo workspace shipped with multi-target `documentDescribes` arrays for both SPDX 2.3 and SPDX 3; npm workspaces extend the same mechanism to a new ecosystem at no marginal cost.

**Why this priority**: Cosmetic / tool-friendliness on top of US1. SPDX-tree-walking tools (sbomqs root scoring, GitHub dep visualizations, GUAC ingest) get a more accurate root.

**Independent Test**: Same recipe as cargo workspace (US3) — single-package npm scan has length-1 `documentDescribes`; workspace scan has length-N (one per member, sorted deterministically); polyglot scan combines with cargo + Go main-modules.

**Acceptance Scenarios**:

1. **Given** a single-package npm project, **When** rendered as SPDX 2.3, **Then** `documentDescribes` is exactly `[<main-module-spdxid>]` (length 1) and the corresponding DESCRIBES relationship exists.
2. **Given** an npm workspace with N members, **When** mikebom scans, **Then** `documentDescribes` lists all N main-module SPDXIDs sorted alphabetically AND there are N `SPDXRef-DOCUMENT DESCRIBES` relationships.
3. **Given** a polyglot project (npm + cargo + Go), **When** mikebom scans, **Then** the SPDX `documentDescribes` extends to include the npm main-module(s) alongside cargo and Go main-modules, deterministically PURL-string-sorted.

---

### Edge Cases

- **`private: true` AND no `version` field**: Skip main-module emission per issue #104's guidance (the author has explicitly signaled "not a publishable artifact"). Workspace members are unaffected — they have their own `package.json`s with `version`s.
- **`private: true` AND a declared `version`**: Emit the main-module. The `private` flag is an npm-publish guard, not an SBOM-presence signal. Common pattern: monorepo roots set `private: true` to prevent `npm publish` accidents while still declaring metadata.
- **`workspaces: ["packages/*"]` glob expansion**: Members are discovered by globbing the pattern (existing reader infrastructure already supports this for the milestone-051 dev-classification path). Each member emits its own main-module per FR-001.
- **Nested workspace members (`packages/<member>/packages/<sub>`)**: each `package.json` with `name` + `version` discovered by the walker emits its own main-module, regardless of nesting depth.
- **Scoped packages (`@scope/name`)**: PURL encoding puts `%40` for the `@` per PURL spec. Existing `build_npm_purl` helper at `mikebom-cli/src/scan_fs/package_db/npm/walk.rs` already handles this for non-main-module components; reuse for main-module emission.
- **Pre-release versions (`1.0.0-beta.1`, `2.0.0-rc.0`)**: Used verbatim in the PURL. SemVer pre-release strings are PURL-segment-safe (`-` and `.` are unreserved).
- **`bin/`, `engines/`, `os/`, `cpu/` fields**: Irrelevant to main-module identity. Skipped (consumers reading `bin` for executable-discovery do that off the package.json directly, not the SBOM).
- **No `package-lock.json` / `pnpm-lock.yaml` present (library packages without committed lockfile)**: Main-module emission is independent of lockfile presence — the manifest provides everything needed for the project-self component. Dependency edges from main-module to direct deps still emit (the `dependencies` / `devDependencies` / `peerDependencies` / `optionalDependencies` keys in the manifest are authoritative for direct edges).
- **pnpm vs npm vs yarn lockfiles**: All three coexist; mikebom's `npm/walk.rs` + `npm/package_lock.rs` + `npm/pnpm_lock.rs` already differentiate. The main-module emission path is lockfile-format-agnostic — it reads only `package.json`.
- **Same-PURL collisions across discovered `package.json` files**: When the walker discovers two-or-more `package.json` files yielding identical `pkg:npm/<name>@<version>` PURLs (common for vendored copies in `node_modules/<name>/package.json` if the walker descended there, though `node_modules/` is already excluded by `should_skip_descent`), exactly one main-module emits (deterministic first-discovered-wins) and a `tracing::warn!` lists dropped duplicate paths. Same dedup convention as cargo (064) per spec Clarifications Q1.
- **`name` with leading dot or special characters**: Per the npm spec, `name` is URL-safe-ish but historically permissive. Use the manifest value verbatim and let PURL segment encoding handle it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every `package.json` discovered during a source-tree scan that contains a `name` field AND either a `version` field OR `private != true`, mikebom MUST emit a single component representing that project, with PURL `pkg:npm/<name>@<version>` for unscoped packages or `pkg:npm/%40<scope>/<name>@<version>` for scoped packages (`@scope/name`). Manifests with `private: true` AND no `version` are skipped per issue #104's guidance. Manifests missing `name` are skipped (FR-001 requires `name`). Manifests with `name` but no `version` AND `private` not set use the literal `0.0.0-unknown` placeholder per the same cross-host determinism convention as cargo (064 FR-001) + Go (053 FR-001 step 3).

- **FR-001a (placement)**: The npm main-module component MUST be emitted via each format's standards-native "BOM subject" construct, not as a sibling of regular dependency components:
  - **CycloneDX 1.6**: emit each npm main-module as `metadata.component` (when there is exactly one) or as the children of a `metadata.component` super-root (when there are multiple — workspace member packages), with `type: "application"`. The npm main-module(s) MUST NOT also appear in the top-level `components[]` array when N=1.
  - **SPDX 2.3**: emit each npm main-module as a regular `packages[]` entry, with `primaryPackagePurpose: "APPLICATION"`, and ensure `documentDescribes[]` (and the corresponding `SPDXRef-DOCUMENT DESCRIBES <main-module>` relationships) targets it/them.
  - **SPDX 3.0.1**: set `software_primaryPurpose: "application"` on the main-module element, and add a `DESCRIBES` (or v3-equivalent) relationship from the SBOM document to the main-module element.
  Inherits the multi-main-module super-root + multi-DESCRIBES infrastructure from milestone 064 + #127 (ships transparently for npm).

- **FR-002**: For `package.json` files declaring `workspaces: [...]` with no own `name`/`version` (or with `private: true` AND no `version`), mikebom MUST NOT emit a main-module for the workspace root. Each glob-expanded member's `package.json` is discovered separately and produces its own main-module per FR-001.

- **FR-003**: The npm walker honors the existing `should_skip_descent` exclusion list (`node_modules/`, `target/`, `vendor/`, etc.). Manifests inside excluded directories are NOT discovered for main-module emission. This intentionally diverges from cargo's FR-003 (which emits for excluded crates) because `node_modules/` contents are upstream deps, not project-internal artifacts. Documented as a deliberate ecosystem-specific divergence.

- **FR-004**: The npm main-module component MUST also carry `mikebom:component-role: main-module` (catalog row C40) as a supplementary signal across all three formats. Inherits the existing C40 wiring established by milestone 053 (Go) + 064 (cargo); no new annotation infrastructure required.

- **FR-005**: The npm main-module component MUST emit with an empty `licenses` field. Coverage parity with sbomqs is achieved via the C40 role tag (FR-004). LICENSE-file detection (npm's `license` field in package.json + SPDX-License-Identifier header scan + askalono content matching) is out of scope and tracked as a follow-up to issue #103.

- **FR-006**: The npm main-module component MUST carry `mikebom:sbom-tier: source` per the existing tier-classification convention (matching milestones 053 + 064).

- **FR-007**: Direct-dep edges from the npm main-module to its dependencies MUST originate from the main-module's PURL. The lockfile-driven dep-emission (existing milestone-051 path through `package_lock.rs` / `pnpm_lock.rs`) integrates with the milestone-064-style augment-existing-entry logic so workspace-root entries' dep declarations (when present in the lockfile) merge onto the main-module. Inherits the same `name_to_purl` resolution + dangling-target-drop convention as cargo.

- **FR-008**: The SPDX 2.3 `documentDescribes[]` array (and the SPDX 3 `rootElement[]`, CycloneDX `metadata.component`) MUST point at the npm main-module(s). For polyglot scans, mikebom MUST extend the existing milestone-064-#127 multi-DESCRIBES super-root mechanism to include npm main-modules alongside cargo and Go main-modules, in deterministic PURL-sorted order.

- **FR-009**: The npm main-module emission MUST NOT alter the existing `dependencies` / `devDependencies` / `peerDependencies` / `optionalDependencies` direct-edge fanout's component count (other than removing the synthetic root placeholder when one would have been emitted). Total dependency-graph edge count from the project's own package MUST be byte-equivalent to pre-066 modulo the placeholder→main-module identifier swap.

- **FR-010**: The new npm main-module component MUST be excluded from `mikebom:not-linked` annotation eligibility (milestone 050) — the project's own package is the linker root, never a non-linked dep. Inherits the existing C40-tag-driven guard from milestone 064.

- **FR-011**: Workspace-member main-modules that depend on other workspace members via npm's workspace-link mechanism (`"<member>": "*"` resolved through the lockfile to the in-tree member) MUST emit `dependsOn` edges to the depended-on member's main-module component. No synthetic / orphan handling required: both endpoints are real components emitted by FR-001 / FR-002.

### Key Entities

- **npm main-module component**: A synthetic SBOM component representing a single npm package at a `package.json` discovered during scan. Identified by `pkg:npm/<name>@<version>` (or scoped equivalent). Carries `primaryPackagePurpose: APPLICATION`, the C40 supplementary role tag, and `mikebom:sbom-tier: source`. Source of all `dependencies`/`devDependencies`/`peerDependencies`/`optionalDependencies` direct edges.

- **npm workspace root**: A `package.json` containing a `workspaces` array. Does not itself emit a main-module unless the same file ALSO declares `name` AND (`version` OR not-`private`). Provides the glob expansion list for member-package discovery.

- **Workspace member package**: A `package.json` whose path matches a glob in the parent workspace root's `workspaces` array. Emits its own main-module per FR-001; depends on other members via the FR-011 workspace-link convention.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of npm project scans containing at least one `package.json` with `name` declared (and not skipped per FR-001's `private`+no-version rule) emit at least one npm main-module component in the resulting SBOM (CDX, SPDX 2.3, and SPDX 3 outputs all consistent). Verified by integration tests scanning a single-package fixture, a workspace fixture, and at least one realistic OSS project (express or similar).

- **SC-002**: Multi-main-module workspace scans correctly surface every member through `documentDescribes` / `rootElement` / CDX super-root `dependencies[]`. The cargo-workspace pattern from milestone 064 + #127 carries over verbatim (no new generator-side code; the C40-tag-driven hooks already work for npm).

- **SC-003**: sbomqs licensing-coverage score for the npm fixture does not regress by more than 1 percentage point vs. the pre-066 baseline. The C40 role tag (FR-004) excludes the new main-module from the denominator.

- **SC-004**: Byte-identity goldens hold across hosts. The npm fixture's CycloneDX, SPDX 2.3, and SPDX 3 goldens regenerate identically on Linux x86_64, Linux aarch64, and macOS aarch64 runners per the cross-host playbook.

- **SC-005**: SPDX 2.3 `documentDescribes` array for any npm-only scan contains the npm main-module's SPDXID(s) directly. The synthetic `DocumentRoot-*` placeholder no longer appears in npm-only outputs.

## Assumptions

- **A1 (manifest authoritative)**: `package.json`'s `name` and `version` are the canonical source for the main-module's PURL. No git introspection or other version ladder is needed (npm doesn't have a workspace-inheritance feature like cargo's `version.workspace = true`).

- **A2 (private + no version skip)**: Per issue #104's explicit guidance, manifests with `private: true` AND no `version` are skipped from main-module emission. This is the author's own opt-out signal. Workspace roots commonly use this pattern.

- **A3 (private + version emit)**: `private: true` alone (with a `version` declared) does NOT block emission. The flag is an npm-publish guard, not an SBOM-presence signal. This is documented as Edge Case to prevent surprise.

- **A4 (license deferred)**: License detection for the npm main-module is out of scope. The C40 carve-out from milestone 053 already protects sbomqs scoring. Real `package.json` `license` field reading + LICENSE-file detection tracked as follow-up to issue #103.

- **A5 (node_modules excluded, deliberately)**: Unlike cargo's FR-003 (which emits for excluded crates), npm's `node_modules/` is excluded from main-module discovery because it contains upstream dependencies, not project-internal artifacts. This is an ecosystem-specific divergence documented in FR-003.

- **A6 (no binary path interaction)**: npm packages don't have a binary-discovery path that emits main-modules (no equivalent to Go's BuildInfo). The npm main-module is unconditionally source-tree-derived. Mirrors cargo (064 A6).

- **A7 (existing scope filtering preserved)**: The milestone-052/part-3 `--exclude-scope` flag continues to filter dev/peer/optional dep edges identically; FR-007 only relocates the edge origin from the placeholder root to the new main-module.

- **A8 (other ecosystems unchanged)**: This milestone is npm-specific. cargo, Go, pip, maven, gem behaviors are untouched. Future #104 milestones extend the pattern to those ecosystems.

- **A9 (super-root reuse)**: The multi-main-module super-root mechanism from milestone 064 + #127 (CDX `metadata.component` super-root + SPDX plural `documentDescribes` + SPDX 3 plural `rootElement`) ships unchanged. npm main-modules slot in as additional describable elements; no new top-level structure introduced.

- **A10 (lockfile-format-agnostic)**: Main-module emission reads only `package.json`. Whether the project uses npm (`package-lock.json` v1/v2/v3), pnpm (`pnpm-lock.yaml`), or yarn classic (`yarn.lock`, currently not parsed by mikebom) is irrelevant to main-module emission. Existing per-format lockfile readers continue to drive transitive component emission.
