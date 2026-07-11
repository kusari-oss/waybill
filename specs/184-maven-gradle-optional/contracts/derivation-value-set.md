# Contract: C122 Derivation-Value Set (m184 update)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Research**: [../research.md](../research.md)

## Scope

Documents the expected value-set for the `mikebom:optional-derivation` annotation (C122 parity catalog row) after m184. Same contract shape as m183's derivation-value-set.md — this is a documentation contract, not a runtime-validated enum. The catalog docstring at `mikebom-cli/src/parity/extractors/cdx.rs:866` is the sole source of truth for expected values.

## Value Set (after m184)

| Value | Milestone | Emitted by | Semantic |
|---|---|---|---|
| `cargo-optional-true` | m179 | `cargo.rs::build_cargo_component` | Cargo `[dependencies].foo = { optional = true }` |
| `npm-optional-dependencies` | m180 + m181 | `npm/package_lock.rs`, `npm/pnpm_lock.rs`, `npm/yarn_lock.rs` (v1 + Berry) | npm-registry family `optionalDependencies` sub-block / `dependenciesMeta.<name>.optional` |
| `pip-optional-dependencies` | m183 | `pip/poetry.rs`, `pip/mod.rs` (main-module post-pass), `pip/uv_lock.rs` | pip-family: poetry.lock `optional = true`, pyproject `[project.optional-dependencies]`, uv.lock `optional-dependencies` |
| **`maven-optional-element`** ✱ | m184 | `maven.rs::pom_dep_to_entry` | Maven `<dependency><optional>true</optional></dependency>` in `pom.xml` (POM 4.0.0 spec) |
| **`gradle-compile-only`** ✱ | m184 | `gradle/lockfile.rs::read_gradle_lockfile` | Gradle `compileOnly` deps (compile-only shape: `*compileClasspath` present + `*runtimeClasspath` absent in the lockfile configs list) |

✱ = added by m184. Total values after m184: **5**.

## Invariants

1. **Values are stable identifiers** — Once documented in this value-set, a value MUST NOT be renamed. Downstream SBOM consumers (pico, other analyzers) grep for these exact strings.
2. **One value per (milestone, mechanism)** — the value uniquely identifies the ecosystem-specific mechanism. m184's split between `maven-optional-element` and `gradle-compile-only` reflects the fact that Maven's `<optional>` element and Gradle's `compileOnly` configuration are semantically distinct mechanisms, unlike the m180/m181/m183 case where all sub-mechanisms within an ecosystem family shared one underlying registry concept.
3. **No component carries multiple derivations** — Enforced by the "one derivation per component" invariant (Decision 2). If a component is classified by two different sources (in m184's case, this can only happen when a project has BOTH pom.xml AND gradle.lockfile), the standard `merge_without_override` deduplication path already in use for maven's transitive expansion applies.
4. **Docstring lists all values** — The `cdx.rs:866` docstring is the sole source of truth; it MUST list every value the mikebom codebase can emit. m184 does NOT need a docstring edit — both new values are already pre-committed since m179.

## Docstring — NO UPDATE NEEDED

**File**: `mikebom-cli/src/parity/extractors/cdx.rs`
**Line**: ~866 (the C122 docstring block)

Current state (pre-m184, from m183's docstring fix):
```rust
// C122 — `mikebom:optional-derivation` (milestone 179). Records
// which ecosystem-reader mechanism populated the
// `LifecycleScope::Optional` classification (`cargo-optional-true`,
// `npm-optional-dependencies`, `pip-optional-dependencies`,
// `maven-optional-element`, `gradle-compile-only`,
// `erlang-optional-applications`). KEEP-BOTH polarity per m178:
// ...
```

**Post-m184**: identical text. Both `maven-optional-element` and `gradle-compile-only` are already listed as placeholders. m184 makes them real by wiring the emission code paths — zero docstring edit.

## Emitted Format Byte-Identity

The C122 extractor at `mikebom-cli/src/parity/extractors/mod.rs:545` is registered as `Directionality::SymmetricEqual`. Same shape as m179/m180/m181/m183:

- **CDX 1.6**: emitted as `properties[].value` on the component, key `mikebom:optional-derivation`.
- **SPDX 2.3**: emitted as an `annotationText` in a `PackageAnnotation` on the component.
- **SPDX 3.0.1**: emitted as `elements[].extension[]` with the same annotation-key.

For any Maven fixture that exercises US1, the three emitted formats MUST contain the exact string `"maven-optional-element"` byte-identically. For any Gradle fixture that exercises US2, the three formats MUST contain `"gradle-compile-only"` byte-identically. SC-008 validates this automatically per format via the parity extractor infrastructure.

## Growth Trajectory (informational)

The C122 value-set is expected to grow through m185+:
- m185 candidate: `erlang-optional-applications` (already docstring'd)
- Future candidates: `sbt-provided`, `mill-optional`, `ant-ivy-optional`, etc.

Each new value is added by:
1. Wiring the emission code path in the appropriate reader.
2. (If needed) extending the docstring at `cdx.rs:866`.
3. Updating a fresh `contracts/derivation-value-set.md` document in the new milestone's spec.

The C122 extractor itself is unchanged across all these additions.
