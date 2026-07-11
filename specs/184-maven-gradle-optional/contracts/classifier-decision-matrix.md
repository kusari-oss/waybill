# Contract: Classifier Decision Matrix (m184)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Data model**: [../data-model.md](../data-model.md)

## Scope

Canonical per-user-story classification tables. This is the single-source-of-truth for what each Java-ecosystem reader emits for every combination of input fields. Task implementers MUST reference these tables when writing unit tests.

## US1 — Maven `<dependency>` block classification

Extends `pom_dep_to_entry` at `mikebom-cli/src/scan_fs/package_db/maven.rs:2347`. The classifier consults BOTH the existing `lifecycle_scope_from_maven(dep.scope.as_deref())` AND the new `dep.optional: bool` field.

### Full input × output table

| `<scope>` in pom.xml | Resulting `lifecycle_scope_from_maven` | `<optional>` in pom.xml | Emitted `lifecycle_scope` | Emitted `mikebom:optional-derivation` | Pre-m184 | Change? |
|---|---|---|---|---|---|---|
| `test` | `Some(Test)` | `true` | `Test` | (absent) | `Test` (absent) | NONE |
| `test` | `Some(Test)` | `false` or absent | `Test` | (absent) | `Test` (absent) | NONE |
| `provided` | `Some(Build)` | `true` | `Build` | (absent) | `Build` (absent) | NONE |
| `provided` | `Some(Build)` | `false` or absent | `Build` | (absent) | `Build` (absent) | NONE |
| `compile` (or absent) | `Some(Runtime)` | **`true`** | **`Optional`** ✱ | **`"maven-optional-element"`** ✱ | `Runtime` (absent) | ✱ CHANGED |
| `compile` (or absent) | `Some(Runtime)` | `false` or absent | `Runtime` | (absent) | `Runtime` (absent) | NONE |
| `runtime` | `Some(Runtime)` | **`true`** | **`Optional`** ✱ | **`"maven-optional-element"`** ✱ | `Runtime` (absent) | ✱ CHANGED |
| `runtime` | `Some(Runtime)` | `false` or absent | `Runtime` | (absent) | `Runtime` (absent) | NONE |
| unrecognized (e.g. `system`, `import`) | `Some(Runtime)` (per m052) | `true` | **`Optional`** ✱ | **`"maven-optional-element"`** ✱ | `Runtime` (absent) | ✱ CHANGED |
| completely unknown | `None` | `true` | **`Optional`** ✱ | **`"maven-optional-element"`** ✱ | `None` (absent) | ✱ CHANGED |
| completely unknown | `None` | `false` or absent | `None` | (absent) | `None` (absent) | NONE |

**Legend**:
- ✱ = new m184 behavior. Four rows change (all `optional = true` cases that land in Runtime or None).
- All other rows preserve pre-m184 byte-identity per FR-011 / SC-004.
- Scope-wins-over-optional (Decision 2) is enforced by the classifier: the `Test` and `Build` branches never emit the derivation annotation, regardless of `<optional>`.

### `<dependencyManagement>` handling

`<optional>` inside a `<dependencyManagement>` block is IGNORED for classification purposes (Edge Cases: m184 does NOT classify management entries as Optional). `<dependencyManagement>` declares default versions but does NOT introduce a dep edge — it's version-pinning metadata. When a real `<dependencies>` block references the coord, THAT reference's own `<optional>` element (or absence) determines the classification.

Implementation note: the existing `inside_dep_mgmt` guard at `maven.rs:791-796` routes `PomDependency` entries into `doc.dependency_management` vs `doc.dependencies`. The m184 classifier only fires when converting entries from `doc.dependencies` (not from `doc.dependency_management`).

## US2 — Gradle lockfile entry classification

Extends `read_gradle_lockfile` at `mikebom-cli/src/scan_fs/package_db/gradle/lockfile.rs:38`. The classifier consults the existing filename-based `is_buildscript` flag AND the new `is_compile_only_shape(configs)` helper.

### Full input × output table

| Filename | `is_buildscript` | `configs` includes `*compileClasspath` | `configs` includes `*runtimeClasspath` | Emitted `lifecycle_scope` | Emitted `mikebom:optional-derivation` | Pre-m184 | Change? |
|---|---|---|---|---|---|---|---|
| `gradle.lockfile` | `false` | Yes | Yes | `None` (pre-m184) | (absent) | `None` (absent) | NONE |
| `gradle.lockfile` | `false` | Yes | No | **`Optional`** ✱ | **`"gradle-compile-only"`** ✱ | `None` (absent) | ✱ CHANGED |
| `gradle.lockfile` | `false` | No | Yes | `None` (pre-m184) | (absent) | `None` (absent) | NONE |
| `gradle.lockfile` | `false` | No | No | `None` (pre-m184) | (absent) | `None` (absent) | NONE |
| `buildscript-gradle.lockfile` | `true` | Yes | Yes | `Build` (per m106) | (absent) | `Build` (absent) | NONE |
| `buildscript-gradle.lockfile` | `true` | Yes | No | `Build` (Decision 2 buildscript-wins) | (absent) | `Build` (absent) | NONE |
| `buildscript-gradle.lockfile` | `true` | No | Yes | `Build` | (absent) | `Build` (absent) | NONE |
| `buildscript-gradle.lockfile` | `true` | No | No | `Build` | (absent) | `Build` (absent) | NONE |

**Legend**:
- ✱ = new m184 behavior. One row changes (non-buildscript + compile-only shape).
- Buildscript-wins (Decision 2 US2 acceptance 5) is enforced by the classifier: `is_buildscript = true` always produces `Build`, regardless of shape.
- Suffix matching per Decision 3 covers `compileClasspath`, `testCompileClasspath`, `<sourceSet>_compileClasspath`, etc.

### Shape-detection precise semantics

- `has_compile = any config ends with "compileClasspath"` — this includes `compileClasspath`, `testCompileClasspath`, `debugCompileClasspath` (Android), `main_compileClasspath` (multi-source-set), etc.
- `has_runtime = any config ends with "runtimeClasspath"` — same suffix rule.
- `compile-only shape = has_compile AND NOT has_runtime`.

This means:
- `compileClasspath,testCompileClasspath` → compile-only (both are compile suffixes; no runtime) → Optional ✱
- `compileClasspath,runtimeClasspath` → NOT compile-only (both suffixes present) → None (unchanged)
- `runtimeClasspath,testRuntimeClasspath` → NOT compile-only (no compile suffix) → None (unchanged)
- `annotationProcessor,compileClasspath` → compile-only (compile suffix present, no runtime) → Optional ✱ (spec Edge Cases: annotation-processor + compileClasspath is treated as compile-only per m184 initial delivery)

## Shared invariants (both user stories)

1. **Distinct derivation values per source**: `"maven-optional-element"` from Maven; `"gradle-compile-only"` from Gradle. NEVER emit one value from the other's code path.
2. **One-derivation-per-component**: a component classified as Test / Build (either format) CANNOT ALSO carry `mikebom:optional-derivation`. Enforced by Decision 2.
3. **Independent per-format classification**: each reader classifies at construction time. No cross-format precedence rule (Decision 4).
4. **C122 parity byte-identity**: the annotation MUST appear byte-identically in CDX 1.6, SPDX 2.3, and SPDX 3.0.1 for every fixture that exercises the m184 classifications. Enforced by the existing C122 `Directionality::SymmetricEqual` extractor.
5. **`--include-dev=false` filtering**: `LifecycleScope::Optional` targets are filtered via `is_non_runtime()`, same as `Dev/Build/Test`. Enforced by m179's existing `is_non_runtime()` extension.
6. **`--spdx2-relationship-compat=basic`**: all new `OPTIONAL_DEPENDENCY_OF` emissions collapse to natural-direction `DEPENDS_ON`. Enforced by m228's basic-mode contract.
