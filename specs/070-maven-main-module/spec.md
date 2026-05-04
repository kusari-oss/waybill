# Feature Specification: maven source-tree main-module component for top-level pom.xml roots + multi-module reactor builds

**Feature Branch**: `070-maven-main-module`
**Created**: 2026-05-03
**Status**: Draft
**Input**: User description: "let's move onto maven" — finish #104 with the maven slice, the most complex of the six ecosystems (XML POM parsing + parent inheritance + multi-module reactor builds).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Maven project SBOMs identify the project itself (Priority: P1)

A developer or CI pipeline runs `mikebom sbom scan --path <maven-project>` against a Java/Maven project. The resulting SBOM contains a component identifying the project itself — `pkg:maven/<groupId>/<artifactId>@<version>` — alongside its dependencies. Today, scanning a Maven project emits dependency components from JAR walks and `pom.xml` direct deps but no component representing the project-being-scanned, so the SBOM cannot answer "what is this an SBOM for?" without filesystem path heuristics.

**Why this priority**: Closes the LAST gap in issue #104. Maven is widely used in enterprise Java/JVM environments — closing it completes the per-ecosystem main-module suite. The pattern is well-trodden by 053+064+066+068+069+#127. Maven adds three ecosystem-specific complexities: XML parsing (mikebom already has `parse_pom_xml`), parent POM inheritance (groupId/version inherited from `<parent>` block), and multi-module reactor builds (`<modules>` lists submodules; each emits its own main-module).

**Independent Test**: Clone any Maven project with a top-level `pom.xml` declaring `<groupId>` + `<artifactId>` + `<version>` (e.g., `git clone https://github.com/google/guava`), run `mikebom sbom scan --path <project> --format spdx-2.3-json --output sbom.json --no-deep-hash`, and verify the output contains exactly one package whose PURL is `pkg:maven/<groupId>/<artifactId>@<version>` derived from the POM's GAV coordinates.

**Acceptance Scenarios**:

1. **Given** a single-module Maven project with `pom.xml` declaring `<groupId>com.example</groupId>` + `<artifactId>my-app</artifactId>` + `<version>1.2.3</version>`, **When** `mikebom sbom scan --path <project>` runs, **Then** the resulting SBOM contains exactly one component with PURL `pkg:maven/com.example/my-app@1.2.3` placed in each format's standards-native "BOM subject" slot (CycloneDX `metadata.component`, SPDX 2.3 `documentDescribes` target, SPDX 3 `DESCRIBES` target).
2. **Given** a multi-module reactor project with a parent `pom.xml` declaring `<groupId>com.example</groupId>`, `<artifactId>parent</artifactId>`, `<version>1.0.0</version>`, `<packaging>pom</packaging>`, and `<modules>`: `module-a`, `module-b`; AND each submodule's `pom.xml` declaring its own `<artifactId>` (`module-a`, `module-b`) + a `<parent>` block referencing the root, **When** mikebom scans, **Then** the SBOM contains exactly three main-module components: parent (`pkg:maven/com.example/parent@1.0.0`) and both submodules (`pkg:maven/com.example/module-a@1.0.0`, `pkg:maven/com.example/module-b@1.0.0`). Each submodule inherits `<groupId>` and `<version>` from the parent's `<parent>` block. The document's `DESCRIBES` relationship targets all three in deterministic name-sorted order via the milestone-064-#127 multi-DESCRIBES infrastructure.
3. **Given** a `pom.xml` with `<groupId>` ABSENT but a `<parent>` block declaring `<parent><groupId>org.springframework.boot</groupId>...</parent>` (Spring Boot starter pattern), **When** mikebom scans, **Then** the main-module's PURL uses the parent's `<groupId>` (`org.springframework.boot`) — POM inheritance per Maven's specification. Same for missing `<version>` inheriting from `<parent>`.
4. **Given** a `pom.xml` with `<version>${revision}</version>` (Maven flatten plugin pattern, common in modern projects), AND `<properties><revision>2.0.0</revision></properties>`, **When** mikebom scans, **Then** the main-module's PURL resolves the property to `2.0.0`. Common Maven property substitution patterns (`${project.version}`, `${parent.version}`, `${revision}`, custom `<properties>` keys) MUST be supported via the existing `parse_pom_properties` helper.

---

### User Story 2 - Main-module component is identifiable and excludable (Priority: P2)

Same use case as milestones 064/066/068/069 US2: downstream tools (sbomqs, vuln scanners, license-compliance tooling) can distinguish the synthetic main-module from real third-party deps via the C40 supplementary `mikebom:component-role: main-module` annotation.

**Why this priority**: Inherits the existing C40 wiring; same posture as Go + cargo + npm + pip + gem.

**Independent Test**: Run a Maven scan that produces the new main-module component; assert the C40 annotation is present in CDX `metadata.component.properties`, SPDX 2.3 annotations, and SPDX 3 native field.

**Acceptance Scenarios**:

1. **Given** a Maven scan producing a main-module, **When** rendered as CycloneDX 1.6, **Then** the main-module is in `metadata.component` with `type: "application"` AND carries `properties[].name = "mikebom:component-role"` with `value = "main-module"`.
2. **Given** the same scan, **When** rendered as SPDX 2.3, **Then** the main-module has `primaryPackagePurpose: "APPLICATION"` AND a C40 annotation envelope.
3. **Given** the same scan, **When** rendered as SPDX 3.0.1, **Then** the main-module has `software_primaryPurpose: "application"` AND the C40-mapped native field.
4. **Given** the new main-module, **When** sbomqs runs, **Then** licensing-coverage doesn't degrade by more than 1pp vs. pre-070 baseline.

---

### User Story 3 - Document root points at maven main-module(s) (Priority: P3)

Inherits the multi-DESCRIBES super-root behavior from milestone 064 + #127. Single-module project scans get length-1 `documentDescribes`; multi-module reactor and polyglot scans extend the existing super-root mechanism.

**Why this priority**: Cosmetic / tool-friendliness. Particularly important for multi-module Maven monorepos where consumers want to walk from `documentDescribes` through every module.

**Independent Test**: Single-module scan → `documentDescribes` length 1; multi-module reactor scan → length-N (parent + submodules); polyglot scan (maven + cargo + Go + npm + pip + gem) → all main-modules listed alphabetically.

**Acceptance Scenarios**:

1. **Given** a single-module Maven project, **When** rendered as SPDX 2.3, **Then** `documentDescribes` is exactly `[<maven-main-module-spdxid>]` (length 1) and the corresponding DESCRIBES relationship exists.
2. **Given** a 3-module reactor (parent + 2 submodules), **When** mikebom scans, **Then** `documentDescribes` lists all three SPDXIDs in deterministic alphabetical order; three DESCRIBES relationships emitted.
3. **Given** a polyglot project (maven + cargo + Go), **When** mikebom scans, **Then** the SPDX `documentDescribes` extends to include the maven main-module(s) alongside cargo and Go main-modules, deterministically sorted.

---

### Edge Cases

- **POM with `<packaging>pom</packaging>` (parent POM with no own JAR)**: Still emits a main-module — the parent POM IS publishable to Maven Central as its own artifact (BOMs and parent POMs are common). FR-001 fires whenever GAV is resolvable.
- **Property substitution edge cases**: `${project.version}` resolves to the project's own `<version>` (or the parent's if inherited); `${parent.version}` resolves to the `<parent><version>` value; `${revision}` is a Maven flatten convention typically defined in `<properties>`. Unresolved properties (e.g., `${some.undefined.prop}`) → emit with the literal placeholder string verbatim plus a `tracing::warn!` (the property is left as-is in the PURL because resolution failed). Operators see the unresolved property string as a signal to investigate.
- **POM with no `<parent>` AND missing `<groupId>` or `<version>`**: invalid Maven POM. Skip emission silently (no fallback identity available — Maven coordinates require GAV to be complete).
- **Multi-module submodule with NO `<parent>` block (free-standing submodule)**: rare; treat as a standalone single-module project. Each emits its own main-module per FR-001 from its own GAV. The reactor parent's `<modules>` list is discovered for traversal purposes; the parent-child relationship doesn't affect emission semantics.
- **Reactor parent with NO own GAV (extremely rare; just a `<modules>` aggregator)**: Skip emission for the parent; emit per-submodule main-modules. Treat the same as application-style projects in gem (FR-002 of milestone 069) — no GAV means no project-self identity.
- **Property defined in `<profiles>` activation**: Out of scope. Active-profile resolution is Maven-runtime-state; mikebom only reads the static POM. Properties under non-default profiles are NOT resolved.
- **POM imported as a BOM (`<dependencyManagement>` with `<scope>import</scope>`)**: Out of scope. The BOM itself emits as a main-module if scanned at the top level (it has GAV); the `<dependencyManagement>` import semantics affect dep-emission but not main-module emission.
- **Same-PURL collision** (e.g., a multi-module reactor where two submodules accidentally declare the same artifactId — usually a misconfiguration): dedup with `tracing::warn!` per the established Q1 convention from cargo (064) / npm (066) / pip (068) / gem (069).
- **`<version>` declared but ALSO `<version>${some.prop}</version>` form** (rare; ambiguous POM): use the literal-substitution result. If the property resolves, use the resolved value; if not, use the literal `${some.prop}` string.
- **Large multi-module reactors (10+ modules)**: emit each as a separate main-module. The `documentDescribes` array can be plural with arbitrary length per the milestone-064-#127 plural-DESCRIBES mechanism.
- **Module path glob support (`<modules><module>foo*</module></modules>`)**: NOT supported — Maven itself doesn't support globs in `<modules>`. Each `<module>` element points at a single subdirectory containing a `pom.xml`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every `pom.xml` discovered AT THE TOP LEVEL of a project root during a source-tree scan, AND for every `pom.xml` reached via parent POM `<modules>` declarations, mikebom MUST emit a single main-module component representing that artifact, with PURL `pkg:maven/<groupId>/<artifactId>@<version>`. GAV resolution: (1) literal `<groupId>`, `<artifactId>`, `<version>` in the POM → use verbatim; (2) missing `<groupId>` or `<version>` → resolve from `<parent>` block per Maven's POM inheritance specification; (3) property substitution (`${revision}`, `${project.version}`, custom `<properties>` keys) → resolve via existing `parse_pom_properties` helper; (4) unresolved properties → use the literal placeholder string verbatim + `tracing::warn!`; (5) `<artifactId>` missing OR fully unresolvable GAV → skip emission silently.

- **FR-001a (placement)**: The maven main-module component MUST be emitted via each format's standards-native "BOM subject" construct, identical to cargo (064) / npm (066) / pip (068) / gem (069) wiring. Inherits the C40-tag-driven hooks from milestones 053+064+#127:
  - **CycloneDX 1.6**: `metadata.component` (single) or `components[]` siblings under super-root (multi-module reactor or polyglot), `type: "application"`.
  - **SPDX 2.3**: `packages[]` entry with `primaryPackagePurpose: "APPLICATION"` + `documentDescribes[]` targeting.
  - **SPDX 3.0.1**: `software_primaryPurpose: "application"` + `DESCRIBES` Relationship.

- **FR-002**: For Maven multi-module reactor builds (parent `pom.xml` with `<modules>` listing submodules), mikebom MUST emit one main-module per resolvable GAV — both the parent (if it has its own GAV) AND each submodule. Each submodule's GAV is resolved via Maven's POM inheritance: explicit `<groupId>`/`<version>` wins, else inherit from `<parent>` block.

- **FR-003**: POM files inside install-state paths (`target/`, `.m2/repository/` if vendored, `lib/`, fat JAR contents extracted by mikebom's existing JAR walker) MUST NOT be discovered for main-module emission. Those are install-state paths handled by the existing dep-emission walker. Excluded directories: `target/`, `.m2/`, `node_modules/`, plus the standard skip set.

- **FR-004**: The maven main-module component MUST also carry `mikebom:component-role: main-module` (catalog row C40) as a supplementary signal across all three formats. Inherits existing C40 wiring; no new annotation infrastructure.

- **FR-005**: The maven main-module component MUST emit with an empty `licenses` field. License detection (POM's `<licenses>` block, LICENSE-file content matching) is out of scope and tracked as a follow-up to issue #103.

- **FR-006**: The maven main-module component MUST carry `mikebom:sbom-tier: source` per the existing tier-classification convention.

- **FR-007**: Direct-dep edges from the maven main-module to its dependencies — POM's `<dependencies>` block — MUST originate from the main-module's PURL. Reuses the existing maven dep-extraction machinery; the augment-existing-entry pattern from milestones 064/066/068/069 merges with same-PURL dep-tier entries when collisions occur.

- **FR-008**: The SPDX 2.3 `documentDescribes[]` array (and SPDX 3 `rootElement[]`, CycloneDX `metadata.component`) MUST point at the maven main-module component(s). Multi-module reactor and polyglot scans extend the existing milestone-064-#127 multi-DESCRIBES super-root mechanism.

- **FR-009**: The maven main-module emission MUST NOT alter the existing dep-emission component count from POM `<dependencies>` blocks or JAR walks. Total dependency-graph edge count from the project's own artifact MUST be byte-equivalent to pre-070 modulo the placeholder→main-module identifier swap.

- **FR-010**: The new maven main-module component MUST be excluded from `mikebom:not-linked` annotation eligibility (milestone 050) — inherits the existing C40-tag-driven guard from milestone 064.

- **FR-011**: Same-PURL collisions across discovered POMs (rare given install-state path exclusion, but possible in misconfigured reactor builds) dedup with `tracing::warn!` per the established cargo/npm/pip/gem Q1 convention.

- **FR-012**: Property substitution resolution operates on the static POM only — NO Maven runtime state, NO active-profile resolution, NO settings.xml reading. The set of resolvable properties is: `${project.groupId}`, `${project.artifactId}`, `${project.version}`, `${parent.groupId}`, `${parent.version}`, `${revision}` (flatten plugin convention), and any custom keys declared in the POM's `<properties>` block. Unresolvable properties pass through verbatim with a warn-level log per FR-001 step 4.

### Key Entities

- **maven main-module component**: A synthetic SBOM component representing a single Maven artifact at a `pom.xml` discovered during scan. Identified by `pkg:maven/<groupId>/<artifactId>@<version>` (with `/` separators per PURL spec, NOT the `:` from issue #104's free-text). Carries `primaryPackagePurpose: APPLICATION`, the C40 supplementary role tag, and `mikebom:sbom-tier: source`. Source of all `<dependencies>` direct edges.

- **Top-level project POM**: A `pom.xml` file at a project root (NOT inside `target/`, `.m2/`, etc.). Emits one main-module per FR-001.

- **Multi-module reactor parent POM**: A `pom.xml` with `<packaging>pom</packaging>` and a `<modules>` block. Emits a parent main-module (if its own GAV is complete) AND one main-module per resolved submodule per FR-002.

- **POM inheritance context**: For each POM with a `<parent>` block, the parent's GAV provides defaults for missing `<groupId>` / `<version>` per Maven specification. Resolution is single-level (we don't follow parent chains beyond what's directly declared in the POM); if a top-level scan includes both parent and child, both are read and the child's inheritance is resolved using the parent's POM data.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of Maven project scans containing a top-level `pom.xml` with resolvable GAV emit at least one maven main-module component in the resulting SBOM. Verified by integration tests scanning a single-module fixture, a multi-module reactor fixture, and at least one realistic OSS Maven project.

- **SC-002**: Multi-module reactor scans correctly emit per-submodule main-modules — parent + N submodules, with submodule GAVs resolved via POM inheritance. Verified by integration test against a 3-module reactor fixture exercising the inheritance pattern.

- **SC-003**: sbomqs licensing-coverage score for the maven fixture does not regress by more than 1 percentage point vs. the pre-070 baseline.

- **SC-004**: Byte-identity goldens hold across hosts. The maven fixture's CycloneDX, SPDX 2.3, and SPDX 3 goldens regenerate identically on Linux x86_64, Linux aarch64, and macOS aarch64 runners.

- **SC-005**: SPDX 2.3 `documentDescribes` array for any Maven-only single-module scan contains the maven main-module's SPDXID directly. Multi-module reactor scans contain all module SPDXIDs alphabetically sorted.

## Assumptions

- **A1 (manifest authoritative)**: `pom.xml`'s `<groupId>` + `<artifactId>` + `<version>` are the canonical source for the main-module's PURL. POM inheritance via `<parent>` block resolves missing GAV components. The existing `parse_pom_xml` helper at `mikebom-cli/src/scan_fs/package_db/maven.rs:570` handles XML parsing.

- **A2 (existing helpers reused)**: `parse_pom_xml` (XML parser), `parse_pom_properties` (property substitution helper), `build_maven_purl` (PURL builder with `/` separators per PURL spec) are all reused from the existing maven dep-emission machinery. No new XML parser added.

- **A3 (license deferred)**: License detection for the maven main-module is out of scope. The C40 carve-out from milestone 053 already protects sbomqs scoring. POM `<licenses>` field reading + LICENSE-file detection tracked as follow-up to issue #103.

- **A4 (install-state paths excluded)**: `target/`, `.m2/`, `node_modules/`, plus the standard skip set, are NOT discovered for main-module emission. Mirrors cargo / npm / pip / gem ecosystem-specific exclusions.

- **A5 (no binary path interaction)**: Maven binary scanning (JAR `META-INF/maven/<g>/<a>/pom.properties` extraction by `parse_pom_properties` helper at `maven.rs:1062`) is for installed-artifact identification, not project-self emission. The maven main-module is unconditionally source-tree-derived from a top-level `pom.xml`.

- **A6 (existing scope filtering preserved)**: `--exclude-scope dev,build,test` continues to filter dep edges identically; FR-007 only relocates the edge origin.

- **A7 (other ecosystems unchanged)**: This milestone is maven-specific. Go, cargo, npm, pip, gem behaviors are untouched.

- **A8 (super-root reuse)**: Multi-main-module super-root + plural-DESCRIBES from milestone 064 + #127 ships unchanged. Maven main-modules slot in as additional describable elements; multi-module reactor builds get a length-N `documentDescribes` array via this infrastructure.

- **A9 (POM inheritance is single-level)**: When a child POM has a `<parent>` block, mikebom resolves missing GAV components from the parent's POM if both are present in the same scan tree. We do NOT walk parent chains beyond one level (BOMs of BOMs of BOMs is rare and out of scope). Free-standing submodules without parents in the scan tree fall back to the `<parent>` block's literal values (which are themselves GAV strings, just used as-is for inheritance purposes).

- **A10 (no Maven runtime, no profiles)**: Mikebom does NOT execute Maven, read `~/.m2/settings.xml`, or apply active-profile filters. Static POM XML is the sole source. Properties under `<profiles>` are NOT resolved per FR-012 / Edge Cases.

- **A11 (PURL separator)**: PURL uses `pkg:maven/<groupId>/<artifactId>@<version>` with `/` separators per [PURL spec](https://github.com/package-url/purl-spec) and the existing `build_maven_purl` helper. Issue #104's free-text used `:` as a separator informally; the spec itself uses `/`. This is consistent with milestones 064/066/068/069 which all defer to PURL spec for their respective ecosystems.
