# Research: Maven + Gradle optional-dependency classification (m184)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Decisions

### Decision 1 — Two distinct derivation-annotation values (not one merged)

**Decision**: Emit `mikebom:optional-derivation = "maven-optional-element"` for Maven and `"gradle-compile-only"` for Gradle. Do NOT merge them into a single "java-optional-declared" or similar shared value.

**Rationale**:
- The underlying mechanisms are semantically distinct:
  - **Maven `<optional>` element** is a transitive-exposure control (per POM 4.0.0 spec): a declared dep is used at compile time by the enclosing artifact, but consumers of that artifact are NOT required to pull it. It's a metadata assertion attached to a specific `<dependency>` block.
  - **Gradle `compileOnly` configuration** is a classpath-composition rule: a declared dep goes on the compile classpath but NOT the runtime classpath. It's inferred from the LOCKFILE's configs list (compile-only shape).
- Consumers of the SBOM's C122 annotation can distinguish which mechanism produced the classification — useful for tools that treat these differently (e.g., a build-reproducibility auditor may want to know whether a build system's exposure rule or its classpath rule was the source).
- The C122 catalog docstring at `parity/extractors/cdx.rs:866` has already reserved both values as placeholders since m179. m184 activates them without touching the docstring.

**Alternatives considered**:
- **Single shared value `"java-optional"` or `"jvm-optional-declared"`** — rejected. Merges semantically distinct signals into one, hiding provenance from consumers. Also contradicts the pre-committed C122 docstring.
- **Per-package-manager values (`"maven-<optional-element"`, `"gradle-compile-only"`, `"sbt-provided"`, `"mill-...")`** — this IS the chosen approach for m184; each value tracks a mechanism-specific concept. Follow-up milestones (sbt, Mill, Ant-Ivy) will add their own values without changing the m184 semantic.

**Follow-up**: The value-set grows from 3 (post-m183: `cargo-optional-true`, `npm-optional-dependencies`, `pip-optional-dependencies`) to 5 (post-m184: + `maven-optional-element`, `gradle-compile-only`). Documented in `contracts/derivation-value-set.md`.

---

### Decision 2 — Scope-wins-over-optional (Maven + Gradle)

**Decision**: When a Maven `<dependency>` has BOTH `<optional>true</optional>` AND an explicit `<scope>test</scope>` / `<scope>provided</scope>` (per `lifecycle_scope_from_maven` at `maven.rs:36`), the scope-derived classification wins (Test / Build respectively). The derivation annotation MUST NOT be emitted on those components. Same rule for Gradle: `buildscript-gradle.lockfile` entries with compile-only shape stay classified as `LifecycleScope::Build`; the optional-derivation annotation is not layered.

**Rationale**:
- Matches the m183 Decision 2 pattern (dev-wins-over-optional): a more-specific classification wins over the general "extras-gated" concept.
- Preserves the "one-derivation-per-component" invariant m180 established.
- `--include-dev=false` already filters Test/Build/Optional via `is_non_runtime()`, so the downstream filter behavior is unchanged; only the SBOM's edge-type verb differs.
- Prevents visual noise: a test-scope dep with a redundant optional annotation would confuse SBOM consumers looking for "why is this component filtered?"

**Implementation guidance**: In `pom_dep_to_entry`, compute `lifecycle_scope_from_maven(dep.scope.as_deref())` FIRST. If it returns `Some(Test | Build)`, keep that scope and skip the m184 optional-classification path entirely. Only when the mapping returns `Some(Runtime)` (the default) AND `dep.optional == true` does the m184 path override to `Some(Optional)` + emit the annotation. Analogous logic for Gradle in `read_gradle_lockfile`.

**Alternatives considered**:
- **Optional-wins-over-scope** — would flip the axis but produce the same downstream filter behavior (both are `is_non_runtime()`). Rejected because it contradicts Maven's install-gate semantics: a `<scope>test</scope>` dep only lives in the test classpath regardless of `<optional>`.
- **Both classifications emitted (multiple annotations)** — violates the one-derivation-per-component invariant and doesn't map to a single SPDX 2.3 relationship type.

---

### Decision 3 — Gradle compile-only shape: suffix-based detection, not exact match

**Decision**: Detect the "compile-only shape" by suffix-checking the configs list for any occurrence of `compileClasspath` AND absence of any `runtimeClasspath`. This covers:
- `compileClasspath` (main source set)
- `testCompileClasspath` (test source set)
- `<sourceSetName>_compileClasspath` (custom source sets in multi-source-set projects, Android's `debug`/`release`, Kotlin's `main`/`test`, etc.)

**Rationale**:
- Gradle multi-project + multi-source-set builds emit lockfiles with per-source-set classpath names. Requiring exact-match against `"compileClasspath"` would miss Android and Kotlin projects — the vast majority of modern Gradle deployments.
- Suffix matching keeps the classifier simple (`configs.split(',').any(|c| c.trim().ends_with("compileClasspath"))`) without introducing regex.
- False positives from unrelated `*compileClasspath` names are unlikely in practice — Gradle's classpath naming convention is stable.

**Implementation guidance**:
```rust
fn is_compile_only_shape(configs: &str) -> bool {
    let items: Vec<&str> = configs.split(',').map(|s| s.trim()).collect();
    let has_compile = items.iter().any(|c| c.ends_with("compileClasspath"));
    let has_runtime = items.iter().any(|c| c.ends_with("runtimeClasspath"));
    has_compile && !has_runtime
}
```

**Alternatives considered**:
- **Exact match on `"compileClasspath"`** — rejected. Would miss `testCompileClasspath`, `<sourceSet>_compileClasspath`, etc.
- **Regex match** — rejected. Adds `regex` dep footprint for a simple suffix check.
- **Full DSL parsing** — rejected per spec Deferred section. Groovy/Kotlin parsers violate Constitution Principle I.

---

### Decision 4 — Per-format independence, no cross-format precedence rule

**Decision**: Maven `pom.xml` classifier and Gradle `gradle.lockfile` classifier run independently. There is NO cross-format precedence rule (unlike m183 Decision 3 lockfile-precedes-manifest for pip).

**Rationale**:
- Maven and Gradle read from disjoint file types. In a project root that has BOTH a `pom.xml` (perhaps for interop or IDE-support fallback) AND a `gradle.lockfile` (the actual build system), the two readers emit distinct `PackageDbEntry` sets that flow through the standard `merge_without_override` deduplication path already in use for Maven's own transitive expansion.
- No collision is possible on the `mikebom:optional-derivation` annotation because a single component can only be classified by ONE reader per emission path (each reader owns its own `PackageDbEntry` construction).
- Contrast with m183 Decision 3: pip's poetry.lock and pyproject.toml describe the SAME source (poetry.lock is the resolved view; pyproject.toml is the input). Maven pom.xml and gradle.lockfile describe DIFFERENT sources (each is authoritative for its own build system).

**Alternatives considered**:
- **Lockfile-precedes-manifest (mimic m183)** — rejected. There's no analogous "manifest" for Java the way pyproject.toml is a manifest for pip. Both files are lockfile-shaped or manifest-shaped depending on how you count.
- **Merged classifier across both readers** — rejected. Would require introducing a shared post-pass helper, which contradicts the "classify at construction time" pattern m183 US1 established.

---

## Bug Discovery: none

No bugs surfaced during m184 research. The two derivation values were pre-committed as placeholders in the C122 docstring at `cdx.rs:866` since m179; m184 is the first milestone to activate them without any docstring adjustment (unlike m183 which had to fix `pip-extras-require` → `pip-optional-dependencies`).
