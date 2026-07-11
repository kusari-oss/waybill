# Quickstart: Maven + Gradle optional-dependency classification (m184)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Operator flow

### Scenario 1 — Maven project with `<optional>true</optional>` deps

A `pom.xml` declares a dependency with `<optional>true</optional>` — the Maven convention for "this dep is used at compile time but NOT transitively exposed to consumers." Spring Boot, Hibernate, and countless enterprise Java projects use this pattern for feature-gated deps.

**Before m184**: mikebom's `parse_pom_xml` skipped the `<optional>` element entirely; the target emitted as a regular Runtime dep.

**After m184**:

```bash
mikebom sbom scan --path ./my-maven-project --format cyclonedx-json,spdx-2.3-json
```

The emitted SBOMs classify the optional-declared dep as:

- **CDX 1.6**: `components[].scope = "excluded"` + `properties[]` includes `mikebom:optional-derivation = "maven-optional-element"`
- **SPDX 2.3** (under `--spdx2-relationship-compat=full`, the default): `<target> OPTIONAL_DEPENDENCY_OF <parent>` instead of the pre-m184 `<parent> DEPENDS_ON <target>`
- **SPDX 3.0.1**: annotation-only classification (no native `OPTIONAL_DEPENDENCY_OF` in SPDX 3)

Downstream tools running pico-style filter analyses can now consume the `scope: "excluded"` (CDX) or `OPTIONAL_DEPENDENCY_OF` (SPDX 2.3) signal to exclude optional-declared deps from vulnerability analysis / build reproducibility checks / license aggregation.

### Scenario 2 — Gradle project with `compileOnly` deps

A `build.gradle` declares deps under the `compileOnly` configuration (Lombok, Delombok annotation processors, provided-by-container Servlet APIs, etc.). Gradle's dependency-locking mechanism produces a `gradle.lockfile` where these deps appear on `compileClasspath` but NOT on `runtimeClasspath`.

**Before m184**: mikebom read the raw configs list into a `mikebom:gradle-configurations` annotation but did NOT classify the target as Optional; the target emitted as a regular Runtime dep.

**After m184**: same commands, mikebom now classifies:
- Lockfile entry `com.example:lombok:1.18.30=compileClasspath,testCompileClasspath` → `LifecycleScope::Optional` + `mikebom:optional-derivation = "gradle-compile-only"`
- Lockfile entry `com.example:guava:32.1.3=compileClasspath,runtimeClasspath` → `Runtime` (unchanged; presence on both classpaths means transitive)

The `mikebom:gradle-configurations` annotation is PRESERVED alongside the new classification for transparency (operators auditing an SBOM can see the exact raw configs list that produced the classification).

## Filter parity in action

The pico-style analysis a downstream SBOM consumer can now run for Java projects:

```bash
# Before m184: `lombok` shows up as vulnerable-in-scope
mikebom sbom scan --path ./my-gradle-project | \
    jq '.components[] | select(.scope != "excluded") | .name'
# → guava, hibernate, lombok, spring-boot  ← FALSE POSITIVES

# After m184: compile-only + optional-declared deps correctly filtered
mikebom sbom scan --path ./my-gradle-project | \
    jq '.components[] | select(.scope != "excluded") | .name'
# → guava, hibernate, spring-boot  ← ACCURATE
```

## Precedence rules (operator-visible)

- **Scope wins over Optional (Maven)**: if a `<dependency>` has BOTH `<optional>true</optional>` AND `<scope>test</scope>` (or `<scope>provided</scope>`), the scope classification wins. The target emits as `Test` (or `Build`) — no derivation annotation is added. Rationale: a test-scope dep only lives in the test classpath regardless of `<optional>`; layering an additional Optional annotation would be redundant.
- **Buildscript wins over Optional (Gradle)**: if a `buildscript-gradle.lockfile` entry has the compile-only shape, the existing `LifecycleScope::Build` classification wins. No derivation annotation. Same rationale.
- **`<dependencyManagement>` doesn't classify**: `<optional>true</optional>` inside a `<dependencyManagement>` block is IGNORED. `<dependencyManagement>` declares default versions, not real dep edges. The classification fires when a real `<dependencies>` block references the coord.
- **Independent per-format classification**: a project with BOTH `pom.xml` AND `gradle.lockfile` doesn't need a cross-format precedence rule. Each reader classifies its own emitted `PackageDbEntry` values; the standard `merge_without_override` dedup path handles collisions.

## Developer flow — verifying an m184 classification

To verify a specific component's classification in an emitted SBOM:

```bash
# CDX 1.6 — Maven
jq '.components[] | select(.name == "some-optional-dep") | {scope, properties}' \
    mikebom.cdx.json

# Expected output (m184 US1):
# {
#   "scope": "excluded",
#   "properties": [
#     { "name": "mikebom:optional-derivation", "value": "maven-optional-element" },
#     ...
#   ]
# }

# CDX 1.6 — Gradle
jq '.components[] | select(.name == "lombok") | {scope, properties}' \
    mikebom.cdx.json

# Expected output (m184 US2):
# {
#   "scope": "excluded",
#   "properties": [
#     { "name": "mikebom:optional-derivation", "value": "gradle-compile-only" },
#     { "name": "mikebom:gradle-configurations", "value": "compileClasspath,testCompileClasspath" },
#     ...
#   ]
# }

# SPDX 2.3 — both formats
jq '.relationships[] | select(.relationshipType == "OPTIONAL_DEPENDENCY_OF") | {spdxElementId, relatedSpdxElement}' \
    mikebom.spdx.json
```

## Failure modes

There are none new in m184 — the classifier is purely additive on the read path. Existing failure modes (malformed pom.xml, missing gradle.lockfile, etc.) surface via `tracing::warn!` and skip-and-continue, unchanged from pre-m184.

## When NOT to expect the classification

- **Legacy `build.gradle` / `build.gradle.kts` DSL projects without `gradle.lockfile`**: mikebom's Gradle classifier reads from the LOCKFILE only. Without a lockfile, `compileOnly` deps aren't detectable — no classification is emitted. DSL parsing is deferred indefinitely per Constitution Principle I (no Groovy/Kotlin parser).
- **Maven inherited-`<optional>`**: a parent POM's `<dependencyManagement>` may declare `<optional>true</optional>`, but m184's initial delivery only reads the CHILD POM's explicit `<optional>` element. Inherited-optional resolution requires a full parent-POM resolver walk — deferred to a follow-up milestone.
- **Basic-mode SPDX 2.3** (`--spdx2-relationship-compat=basic`): all typed dep-scope edges collapse to `DEPENDS_ON` per m228. The classifier still runs; only the emission form differs.
- **sbt / Mill / Ant-Ivy projects**: JVM-ecosystem tail beyond Maven and Gradle. sbt is already read at the reader level (m142) but classification extension is future scope; Mill and Ant-Ivy readers don't exist yet.

## Cross-references

- Spec: [spec.md](./spec.md)
- Plan: [plan.md](./plan.md)
- Classifier decision matrix: [contracts/classifier-decision-matrix.md](./contracts/classifier-decision-matrix.md)
- Derivation value set: [contracts/derivation-value-set.md](./contracts/derivation-value-set.md)
- Research decisions: [research.md](./research.md)
