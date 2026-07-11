# Feature Specification: Maven + Gradle optional-dependency classification

**Feature Branch**: `184-maven-gradle-optional`
**Created**: 2026-07-11
**Status**: Draft
**Input**: User description: "m184 — extends the m179 unified optional-dependency classification to the Java ecosystem. Two readers plumb the missing `LifecycleScope::Optional` classification: (a) `maven.rs` currently ignores the `<optional>true</optional>` child element on `<dependency>` blocks in `pom.xml` — the `PomDependency` struct at maven.rs:578 has no `optional: bool` field, so Maven-declared optional deps emit as regular Runtime edges; (b) the Gradle lockfile reader at `gradle/lockfile.rs` currently records the raw configs list as a `mikebom:gradle-configurations` annotation but does NOT classify deps that appear ONLY on the `compileClasspath` (absent from `runtimeClasspath`) — the canonical wire signature of `compileOnly` deps — as `LifecycleScope::Optional`. Both mechanisms surface distinct semantic concepts (Maven's `<optional>` = transitive-exposure control per POM spec; Gradle's `compileOnly` = compile-classpath-only configuration) and thus emit distinct `mikebom:optional-derivation` values: `\"maven-optional-element\"` and `\"gradle-compile-only\"` respectively. These are the exact placeholder values already documented in the C122 catalog docstring at `parity/extractors/cdx.rs:866` since m179."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Maven `<optional>true</optional>` maps to `LifecycleScope::Optional` (Priority: P1)

The Maven POM spec defines `<optional>true</optional>` as a `<dependency>` child element that marks a dep as "not transitively exposed to downstream consumers": consumers of the enclosing artifact are NOT required to pull the marked dep. It's semantically equivalent to Cargo's `optional = true` feature-gated dep — the enclosing artifact uses it at compile time, but a consumer must explicitly re-declare it if they want the same integration.

Today mikebom's `parse_pom_xml` (at `maven.rs:689`) skips the `<optional>` element entirely — `PomDependency` at line 578 has no `optional` field. Downstream classifier code treats such deps as regular Runtime edges, producing false-positive Runtime relationships that mislead SBOM consumers running pico-style filter analyses. This gap is analogous to the m183 US1 poetry.lock bug where `optional = true` packages were silently classified as Runtime.

**Why this priority**: Maven is the historically dominant Java build system, and `<optional>true</optional>` is a well-established POM convention (Spring Boot, Hibernate, and countless enterprise Java projects use it to declare feature-gated deps). Same pico filter-parity gap m179/m180/m181/m183 closed for their ecosystems — extended to the Java ecosystem's biggest installed base.

**Independent Test**: Scan a Maven project whose `pom.xml` declares at least one `<dependency>` with `<optional>true</optional>`. Verify (a) the target component gets `LifecycleScope::Optional` + the `mikebom:optional-derivation = "maven-optional-element"` annotation, (b) CDX 1.6 emits `scope: "excluded"` on the target, (c) SPDX 2.3 emits `<target> OPTIONAL_DEPENDENCY_OF <parent>` under `--spdx2-relationship-compat=full`.

**Acceptance Scenarios**:

1. **Given** a `pom.xml` with `<dependency><groupId>org.example</groupId><artifactId>optional-dep</artifactId><version>1.0</version><optional>true</optional></dependency>`, **When** mikebom emits SPDX 2.3 under `--spdx2-relationship-compat=full`, **Then** `optional-dep` MUST appear as source-side of an `OPTIONAL_DEPENDENCY_OF` edge — NOT as a plain `DEPENDS_ON` target.
2. **Given** the same scan, **When** mikebom emits CDX 1.6, **Then** `optional-dep` MUST carry `scope: "excluded"` + `mikebom:optional-derivation = "maven-optional-element"`.
3. **Given** a `pom.xml` with `<dependency>` element WITHOUT the `<optional>` child (or with `<optional>false</optional>` explicit), **When** mikebom emits SPDX 2.3, **Then** the target MUST continue to emit its existing scope-derived classification (Test/Build/Runtime per `<scope>`) — the fix does NOT change other classification paths.
4. **Given** a `pom.xml` where the same `<dependency>` has BOTH `<optional>true</optional>` AND `<scope>test</scope>`, **When** mikebom classifies, **Then** the Test scope classification wins (analogous to m183 Decision 2 dev-wins-over-optional) — the target emits as `TEST_DEPENDENCY_OF`, NOT `OPTIONAL_DEPENDENCY_OF`, and the derivation annotation MUST NOT appear on it. Rationale: `<scope>test</scope>` is already gate-filtered by `--include-dev=false` via `is_non_runtime()`; layering an additional Optional annotation would be visual noise and violate the "one derivation per component" invariant m180 established.

---

### User Story 2 — Gradle `compileOnly` deps map to `LifecycleScope::Optional` (Priority: P1)

Gradle's `compileOnly` configuration declares deps that are on the compile classpath but NOT the runtime classpath — semantically equivalent to Maven's `<optional>true</optional>` (deps required at compile time, not transitively exposed to runtime). In Gradle lockfiles, `compileOnly` deps appear on the `compileClasspath` configuration but are ABSENT from `runtimeClasspath`.

Today mikebom's `read_gradle_lockfile` (at `gradle/lockfile.rs:38`) preserves the raw configs list as a `mikebom:gradle-configurations` annotation (a valuable transparency signal) but does NOT set `lifecycle_scope` for entries that appear only on compile-side configurations. Gradle projects with `compileOnly` deps thus emit them as Runtime edges by default, same as they would emit `implementation` deps — producing the same false-positive Runtime signal Maven's US1 case does.

**Why this priority**: Gradle is the dominant Java build system for greenfield projects (Android by default, Kotlin by default, most modern JVM projects). Same pico filter-parity gap as US1 for the Java-Gradle user base.

**Independent Test**: Scan a Gradle project whose `gradle.lockfile` has at least one entry appearing on `compileClasspath` but NOT on `runtimeClasspath`. Verify (a) the target gets `LifecycleScope::Optional` + `mikebom:optional-derivation = "gradle-compile-only"`, (b) CDX 1.6 emits `scope: "excluded"`, (c) SPDX 2.3 emits `OPTIONAL_DEPENDENCY_OF` under Full mode.

**Acceptance Scenarios**:

1. **Given** a `gradle.lockfile` entry `org.example:compile-only-dep:1.0=compileClasspath,testCompileClasspath`, **When** mikebom emits SPDX 2.3 under Full mode, **Then** `compile-only-dep` MUST emit as source-side of `OPTIONAL_DEPENDENCY_OF` — NOT as plain `DEPENDS_ON`.
2. **Given** the same scan, **When** mikebom emits CDX 1.6, **Then** `compile-only-dep` MUST carry `scope: "excluded"` + `mikebom:optional-derivation = "gradle-compile-only"`.
3. **Given** a `gradle.lockfile` entry `org.example:runtime-dep:1.0=compileClasspath,runtimeClasspath`, **When** mikebom classifies, **Then** the target MUST continue to emit as Runtime (unchanged from pre-m184) — presence on BOTH compile AND runtime classpaths means the dep is transitive, not compile-only.
4. **Given** a `gradle.lockfile` entry `org.example:runtime-only-dep:1.0=runtimeClasspath`, **When** mikebom classifies, **Then** the target MUST NOT be classified as Optional (it appears only on runtime, which is the Gradle `runtimeOnly` shape — semantically Runtime, not Optional; no derivation annotation).
5. **Given** a `buildscript-gradle.lockfile` entry with the same compile-only shape, **When** mikebom classifies, **Then** the existing `LifecycleScope::Build` classification wins (buildscript deps are already build-time-only; Optional would double-classify — analogous to US1 acceptance 4 dev-wins-over-optional but for Build).

---

### Edge Cases

- **Maven `<optional>true</optional>` inside a `<dependencyManagement>` block**: `<dependencyManagement>` declares default versions but does NOT introduce a dep edge — it's version-pinning metadata. m184 does NOT classify `<dependencyManagement>` entries as Optional; they remain unclassified until a real `<dependencies>` block references them, at which point the classifier fires for the real reference.
- **Maven `<optional>true</optional>` on a `<scope>provided</scope>` dep**: Provided-scope deps are already classified as `LifecycleScope::Build` at `maven.rs:42`. Per Decision 2 (analogous to US1 acceptance 4), Build wins over Optional — the target emits as Build, no derivation annotation. Preserves the "one derivation per component" invariant.
- **Maven inherited-optional via parent POM**: if a parent POM's `<dependencyManagement>` declares `<optional>true</optional>` for a coord, and a child POM references that coord in `<dependencies>` without overriding, the optional-flag inherits per POM spec. m184's initial delivery scope covers only the CHILD POM's explicit `<optional>` — inherited-optional resolution requires a full parent-POM resolver walk, deferred to a follow-up milestone. Documented as a Known Limitation.
- **Gradle configs list absent (empty=... marker or missing configs)**: existing reader-level skip behavior preserved (line 68 skips `empty=...`). No m184 change to those code paths.
- **Gradle deps on `compileClasspath` + `annotationProcessor` only** (a common shape for annotation-processor libs like Lombok): m184's initial delivery scope treats this as US2 Optional (compile-only shape). `annotationProcessor`-only presence without `compileClasspath` is a separate case (rare); classified as Optional in m184's initial delivery per the same shape rule. A follow-up milestone MAY add a dedicated `gradle-annotation-processor` derivation value if operator demand emerges.
- **Gradle `testCompileOnly` + `testRuntimeOnly`**: these are test-scoped configurations. If a dep appears on `testCompileClasspath` but NOT `testRuntimeClasspath` (or vice versa), the test-scope classification wins — per acceptance 5 pattern extended to Test scope. Not m184's initial delivery focus; documented as covered by the general "test-scope-wins-over-optional" rule.
- **Both formats present in the same project root** (a Gradle project with `pom.xml` for interop): each reader classifies independently. No cross-format precedence rule is needed because Maven and Gradle read from disjoint files.
- **`--spdx2-relationship-compat=basic`**: all typed dep-scope edges collapse to natural-direction `DEPENDS_ON` per m228. Same as m179/m180/m181/m183.
- **`--include-dev=false`**: `LifecycleScope::Optional` targets filter via `is_non_runtime()`, same as m179+ family.
- **Legacy `build.gradle` / `build.gradle.kts` script parsing**: m184 scope is limited to LOCKFILE formats — `gradle.lockfile` for US2. Parsing the DSL scripts themselves (Groovy or Kotlin) requires either a Groovy/Kotlin parser or a Gradle plugin, both violate Constitution Principle I (no C via nested interpreters). Documented as OUT of m184 scope; the m106 US1 gradle DSL reader (if any) is unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `parse_pom_xml` function at `mikebom-cli/src/scan_fs/package_db/maven.rs:689` MUST extract the `<optional>` child element of `<dependency>` blocks. The `PomDependency` struct at line 578 MUST gain an `optional: bool` field defaulting to `false` when the element is absent or set to a non-`"true"` value.
- **FR-002**: Downstream Maven classifier code paths that convert `PomDependency` into `PackageDbEntry` (or into `ResolvedComponent` via `depends_to_entries`-family functions) MUST set `lifecycle_scope = Some(LifecycleScope::Optional)` + insert `mikebom:optional-derivation = "maven-optional-element"` into `extra_annotations` when the source `PomDependency` has `optional = true` AND no more-specific NON-RUNTIME scope (Test via `<scope>test</scope>` or Build via `<scope>provided</scope>`) is already assigned per Decision 2 US1 acceptance 4 precedence. Default-runtime scopes (`compile` / `runtime` / `system` / `import` / absent, all mapping to `LifecycleScope::Runtime` per m052) DO NOT block the Optional override — see FR-005 for the full precedence table.
- **FR-003**: The `read_gradle_lockfile` function at `mikebom-cli/src/scan_fs/package_db/gradle/lockfile.rs:38` MUST detect the "compile-only shape" per entry: presence of `compileClasspath` in the configs list AND absence of `runtimeClasspath`. When this shape is detected AND the entry's current `lifecycle_scope` is `None`, set `lifecycle_scope = Some(LifecycleScope::Optional)` + insert `mikebom:optional-derivation = "gradle-compile-only"` into `extra_annotations`.
- **FR-004**: The Maven and Gradle sources MUST emit DISTINCT derivation-annotation values — `"maven-optional-element"` for Maven, `"gradle-compile-only"` for Gradle — because the underlying mechanisms are semantically distinct (POM `<optional>` element vs Gradle configuration composition). This differs from m180/m181/m183 which shared a single value per ecosystem family; the Java-family split reflects the C122 catalog docstring's existing separation.
- **FR-005**: For Maven, when a `<dependency>` has BOTH `<optional>true</optional>` AND `<scope>test</scope>` OR `<scope>provided</scope>` (a more-specific non-Runtime scope per `lifecycle_scope_from_maven` at `maven.rs:36-48`): the scope-derived classification wins (Test / Build respectively); the derivation annotation MUST NOT be emitted. Analogous to m183 Decision 2 dev-wins-over-optional. **Explicitly**: `<scope>compile</scope>` / `<scope>runtime</scope>` / `<scope>system</scope>` / `<scope>import</scope>` / absent `<scope>` all map to `LifecycleScope::Runtime` per m052's classifier — these are the default-runtime scopes and DO NOT win over `<optional>true</optional>`. When any of them is combined with `<optional>true</optional>`, the classifier emits `LifecycleScope::Optional` + the derivation annotation (per the data-model.md §3 US1 decision matrix). Rationale: Maven's `<optional>` element is orthogonal to `<scope>` per POM 4.0.0 spec — a runtime-scoped optional dep is on the runtime classpath but NOT transitively exposed to consumers, which is the Optional semantic.
- **FR-006**: For Gradle, when a `buildscript-gradle.lockfile` entry has the compile-only shape (per FR-003), the existing `LifecycleScope::Build` classification wins per acceptance 5 (buildscript deps are already build-time-only; layering Optional would double-classify).
- **FR-007**: Under `--spdx2-relationship-compat=basic`, all new `OPTIONAL_DEPENDENCY_OF` emissions collapse to natural-direction `DEPENDS_ON` per m228's contract.
- **FR-008**: The `--include-dev=false` filter MUST filter out `LifecycleScope::Optional` components alongside `Dev/Build/Test` via `is_non_runtime()` (inherited from m179's extension).
- **FR-009**: The CDX 1.6 emission MUST show `scope: "excluded"` for every m184-classified `LifecycleScope::Optional` component — auto-inherited via the m179 `is_non_runtime()` extension.
- **FR-010**: The SPDX 3.0.1 emission MUST NOT include a native `lifecycleScope: optional` value on any relationship (SPDX 3.0.1 has no such enum value per m179 FR-017); the annotation IS the SPDX 3 classification carrier.
- **FR-011**: For scans that do NOT exercise the new Java signals (all non-Maven / non-Gradle fixtures + Maven fixtures without `<optional>` elements + Gradle fixtures without compile-only shape), the emitted CDX 1.6, SPDX 2.3, and SPDX 3.0.1 documents MUST be byte-identical to the pre-m184 baseline (regression guard, same shape as m180 FR-012 / m181 FR-011 / m183 FR-011).
- **FR-012**: The existing maven regression tests (m085 dep-edges, m087 workspace-version fix, m130 US2 nested-JAR recursion) AND the existing Gradle lockfile regression tests (m106 US1 `configurations_recorded_in_annotation`) MUST continue to pass, allowing for additive changes on any test that would exercise `<optional>` or `compileOnly` shapes.
- **FR-013**: The m184 changes MUST NOT introduce any new Cargo dependency. Existing `quick-xml` (Maven pom parsing) and stdlib string operations (Gradle configs list parsing) cover all needs.
- **FR-014**: The C122 catalog docstring at `mikebom-cli/src/parity/extractors/cdx.rs:866` — which already lists `maven-optional-element` and `gradle-compile-only` as placeholder value-set entries — remains UNCHANGED text-wise; m184 lands the code paths that populate those values with real data.

### Key Entities

- **Component (`ResolvedComponent`)**: Reuses m179's `LifecycleScope::Optional` variant and the `mikebom:optional-derivation` annotation. Zero new types.
- **`mikebom:optional-derivation` annotation**: Two new active values — `"maven-optional-element"` (US1) and `"gradle-compile-only"` (US2) — after m184. The C122 catalog value-set grows from 3 (post-m183: cargo, npm, pip) to 5 (post-m184: + maven + gradle). All flow through the same C122 `Directionality::SymmetricEqual` extractor registered in m179.
- **`PomDependency` at `maven.rs:578`**: extended with `optional: bool` field, defaulting to `false`. Zero other field changes.
- **Gradle configs list at `gradle/lockfile.rs:117`**: unchanged wire format (still emits `mikebom:gradle-configurations` annotation). The compile-only shape is inferred from the raw configs string via a new classifier helper.
- **Shared classifier logic**: each reader classifies at construction time (analogous to m183 US1 poetry.rs pattern), not via a downstream post-pass. No shared helper file is introduced.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of a Maven fixture with N `<optional>true</optional>` dep declarations (non-test-scope) MUST have the SET of maven-emitted PURLs marked `scope: "excluded"` in CDX 1.6 equal to the SET of PURLs appearing as source-side of any typed dep-scope relationship (`OPTIONAL_DEPENDENCY_OF` + existing `TEST_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` if the reader marked them) in SPDX 2.3. Same pico filter-parity gate as m179/m180/m181/m183 SC-001, extended to Maven.
- **SC-002**: Same set-equality gate for Gradle fixtures with compile-only shape.
- **SC-003**: Zero mikebom regression fixture SPDX 2.3 golden files experience NET-DECREMENT in `*_DEPENDENCY_OF` edge counts pre-vs-post m184. Net-INCREMENT is expected on any maven fixture that exercises `<optional>true</optional>` (moves from `DEPENDS_ON` to `OPTIONAL_DEPENDENCY_OF`) and any gradle fixture with compile-only-shape entries.
- **SC-004**: Zero drift in any mikebom CDX 1.6 golden file that does not exercise the new signals (regression guard on non-Java fixtures — same shape as m180 SC-003 / m181 SC-004 / m183 SC-005).
- **SC-005**: Zero drift in any mikebom SPDX 3.0.1 golden file (m184 does not touch SPDX 3 emission per FR-010).
- **SC-006**: Under `--spdx2-relationship-compat=basic`, Java goldens show zero new `OPTIONAL_DEPENDENCY_OF` edges — every optional-classified edge is natural-direction `DEPENDS_ON` (m228 escape hatch preserved).
- **SC-007**: Existing maven regression tests (m085/m087/m130-US2 + parse_pom_xml unit tests) AND existing Gradle lockfile regression tests (m106 US1) MUST continue to pass, allowing for additive changes on any test that exercises `<optional>` / compile-only shapes.
- **SC-008**: The `mikebom:optional-derivation` annotation MUST appear byte-identically in CDX 1.6, SPDX 2.3, and SPDX 3.0.1 output — value `"maven-optional-element"` for Maven-classified components, `"gradle-compile-only"` for Gradle-classified components — for every fixture that exercises the m184 classifications. C122 parity extractor (registered in m179) validates this automatically via `SymmetricEqual` polarity.
- **SC-009**: Zero new production Cargo dependencies added to `mikebom-cli/Cargo.toml` — `cargo tree -p mikebom | wc -l` MUST be identical pre- vs post-m184.

## Assumptions

- Maven's `<optional>` element accepts the string values `"true"` and `"false"` per POM 4.0.0 spec. Other values (e.g., whitespace, other capitalization) are treated as `false` — the reader's `.eq_ignore_ascii_case("true")` guard is the canonical parse rule.
- Gradle's compile-only shape is detected purely from the LOCKFILE's configs list — no DSL parsing. Deps appearing on `compileClasspath` (or `<subproject>_compileClasspath`, or `<sourceSet>_compileClasspath` for multi-source-set projects) AND absent from any `runtimeClasspath` variant are classified. The specific classpath-name-suffix pattern (`compileClasspath`, `testCompileClasspath`, `xyz_compileClasspath`) is matched via suffix-check, not exact match, so multi-source-set Gradle projects (Kotlin's `main`/`test`, Android's `debug`/`release`) are covered by construction.
- The Maven and Gradle classifications are independent per project root — a Maven pom.xml and a gradle.lockfile in the same root each classify their own entries with no cross-format precedence rule. This differs from m183's lockfile-precedes-manifest rule (Decision 3) because Maven and Gradle read distinct file types, not competing views of the same source.
- The m184 scope covers ROOT project's pom.xml + gradle.lockfile + buildscript-gradle.lockfile. Multi-module Maven reactors (pom.xml → child module poms via `<modules>`) and Gradle multi-project builds (settings.gradle → subproject build.gradle) each maintain per-module classification independence. m184 does NOT introduce a cross-module inheritance walk; a `<optional>true</optional>` in a parent POM's `<dependencyManagement>` does NOT propagate to child POM references (Known Limitation, documented in Edge Cases).
- The C122 catalog docstring at `parity/extractors/cdx.rs:866` already documents `maven-optional-element` and `gradle-compile-only` as expected values since m179. m184 does NOT change the docstring — the values were pre-committed as placeholders; m184 makes them real.
- Golden fixture regeneration (`MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`) will show additive-only changes on any maven/gradle fixture that exercises the new signals. Existing fixtures may or may not include `<optional>` / compile-only shapes; drift on any pre-m184 golden is expected only when the fixture's underlying pom.xml or gradle.lockfile actually contains the classifiable signal. Non-Java goldens MUST show zero drift.
- Legacy `build.gradle` / `build.gradle.kts` DSL parsing is OUT of m184 scope. Deferred until a broader "Gradle DSL introspection" workstream (if any); m184 covers only the LOCKFILE view. `build.gradle.kts` was covered by an unrelated milestone (m106 US1) for a different purpose (config recording); m184 does not extend that reader.
- Maven's inherited-optional via parent POM `<dependencyManagement>` is OUT of m184 scope (Known Limitation documented in Edge Cases). Deferred until a full parent-POM resolver walk lands as a separate milestone.

## Constitution Alignment

**Principle V** (v1.4.0): Direct continuation of the m179+m180+m181+m183 KEEP-BOTH polarity. Native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` is the primary signal (elevated from the current `DEPENDS_ON`); the `mikebom:optional-derivation` annotation is the finer-grained supplement carrying WHICH Java-ecosystem construct produced the classification (Maven `<optional>` element or Gradle `compileOnly` configuration). Zero new `mikebom:*` invention — flows through the C122 catalog row m179 registered. No new Principle V audit surface.

**Principle IX** (Accuracy): The pico filter-parity gap m179 closed for Go + Cargo, m180 for npm + pnpm, m181 for yarn v1 + Berry, m183 for pip / poetry / uv, now closes for the Java ecosystem — same measurable accuracy gain (SC-001, SC-002). Specifically corrects the silent misclassification path where Maven `<optional>true</optional>` and Gradle `compileOnly` deps emit as Runtime (misleading pico + other downstream consumers today).

**Principle X** (Transparency): SBOM consumers can distinguish "this component was classified optional by a Java-ecosystem source" via the specific derivation value (`"maven-optional-element"` or `"gradle-compile-only"`) + `evidence.source_file_paths` field pointing at the specific `pom.xml` / `gradle.lockfile`. Two distinct values (not one merged) faithfully carry the mechanism the operator can audit.

## Deferred to Future Milestones

- **Inherited-optional resolution**: parent POM `<dependencyManagement>` propagation to child POM references. Requires the full parent-POM resolver walk (repository fetch + version resolution). Deferred until a broader Maven-resolver workstream lands.
- **Legacy `build.gradle` / `build.gradle.kts` DSL parsing**: requires a Groovy or Kotlin parser, both violate Constitution Principle I. Deferred indefinitely.
- **Gradle `annotationProcessor`-only shape**: dedicated derivation value `gradle-annotation-processor` (currently subsumed under `gradle-compile-only` when the entry also appears on `compileClasspath`; standalone `annotationProcessor`-only entries are rare in practice). Deferred until operator demand emerges.
- **Sbt / Mill / Ant-Ivy optional-dep**: the JVM-ecosystem tail beyond Maven and Gradle. Follow-up milestones may cover these (m142 already handles sbt at the reader level; classification extension is a future scope).
- **Multi-module Maven reactor cross-classification**: `<modules>`-based parent → child inheritance walk. m184 delivers per-module independence; cross-module walk is future scope.
