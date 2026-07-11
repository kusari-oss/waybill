# Data Model: Maven + Gradle optional-dependency classification (m184)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

## 1. New Type (US1 only)

### 1.1 `PomDependency` — new `optional: bool` field

Extends the existing struct at `mikebom-cli/src/scan_fs/package_db/maven.rs:578`:

```rust
pub(crate) struct PomDependency {
    pub group_id: String,
    pub artifact_id: String,
    pub version: Option<String>,
    pub scope: Option<String>,
    pub dep_type: Option<String>,
    // Milestone 184 US1 — extracted from the `<optional>` child element of
    // `<dependency>` blocks. Defaults to `false` when the element is absent
    // or its text isn't `"true"` (case-insensitive per POM 4.0.0 spec).
    pub optional: bool,
}
```

Every existing `PomDependency` construction site MUST be updated to initialize the new field. Grep pattern for the audit: `PomDependency\s*{`. Expected sites:
- `maven.rs:798` (inside `parse_pom_xml` at line ~800) — populate from `dep_optional.take().map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false)`
- Any test-only fixture construction — initialize `optional: false` unless the test specifically exercises the m184 path.

## 2. No New Type for US2

Gradle US2 is a shape-inference extension — no new struct fields are needed. The `PackageDbEntry` type is unchanged; classification happens at construction time inside `read_gradle_lockfile`.

## 3. Classifier Decision Matrices

### US1 — Maven `pom_dep_to_entry` at `maven.rs:2347`

The classifier consults BOTH the existing `lifecycle_scope_from_maven(dep.scope.as_deref())` AND the new `dep.optional` field:

| `dep.scope` (via `lifecycle_scope_from_maven`) | `dep.optional` | Emitted `lifecycle_scope` | Emitted `mikebom:optional-derivation` | Change? |
|---|---|---|---|---|
| `Some(Test)` (from `<scope>test</scope>`) | `true` | `Test` | (not emitted — Decision 2 scope-wins) | NONE |
| `Some(Test)` | `false` | `Test` | (not emitted) | NONE |
| `Some(Build)` (from `<scope>provided</scope>`) | `true` | `Build` | (not emitted — Decision 2 scope-wins) | NONE |
| `Some(Build)` | `false` | `Build` | (not emitted) | NONE |
| `Some(Runtime)` (from `<scope>compile/runtime/system/import</scope>` OR absent) | **`true`** | **`Optional`** ✱ | **`"maven-optional-element"`** ✱ | ✱ CHANGED |
| `Some(Runtime)` | `false` | `Runtime` | (not emitted) | NONE |
| `None` (unrecognized `<scope>`) | `true` | **`Optional`** ✱ | **`"maven-optional-element"`** ✱ | ✱ CHANGED |
| `None` | `false` | `None` | (not emitted) | NONE |

✱ = new m184 behavior. Two rows change (Runtime + None both with `optional = true`). All other rows preserve pre-m184 behavior byte-identically.

**Read helper** (already in maven.rs, unchanged):
```rust
fn lifecycle_scope_from_maven(scope: Option<&str>) -> Option<LifecycleScope> {
    match scope {
        Some("test") => Some(LifecycleScope::Test),
        Some("provided") => Some(LifecycleScope::Build),
        Some("compile") | Some("runtime") | Some("system") | Some("import") | None => {
            Some(LifecycleScope::Runtime)
        }
        Some(_) => None,
    }
}
```

**Classifier extension** at `pom_dep_to_entry` (data-model reference — implementation at Task-phase):
```rust
let base_scope = lifecycle_scope_from_maven(dep.scope.as_deref());
let (lifecycle_scope, is_m184_optional) = match (base_scope, dep.optional) {
    // Scope-wins (Decision 2): Test / Build classifications win; no annotation.
    (Some(LifecycleScope::Test), _) => (Some(LifecycleScope::Test), false),
    (Some(LifecycleScope::Build), _) => (Some(LifecycleScope::Build), false),
    // Runtime + optional=true → Optional classification.
    (Some(LifecycleScope::Runtime), true) => (Some(LifecycleScope::Optional), true),
    // Runtime + optional=false → Runtime (unchanged).
    (Some(LifecycleScope::Runtime), false) => (Some(LifecycleScope::Runtime), false),
    // Unrecognized scope + optional=true → Optional classification.
    (None, true) => (Some(LifecycleScope::Optional), true),
    // Unrecognized scope + optional=false → None (unchanged).
    (None, false) => (None, false),
    // Other LifecycleScope variants — Runtime/Test/Build cover the m052
    // mapping; a future variant added to `lifecycle_scope_from_maven`
    // would need explicit handling here. `_` guard keeps the compiler
    // check honest.
    (other, _) => (other, false),
};
```

The annotation is inserted into `extra_annotations` only when `is_m184_optional` is true. Analogous to the m183 US1 pattern in `poetry.rs`.

### US2 — Gradle `read_gradle_lockfile` at `gradle/lockfile.rs:38`

The classifier consults BOTH the existing filename-based `is_buildscript` flag AND the new `is_compile_only_shape(configs)` helper:

| `is_buildscript` | `is_compile_only_shape(configs)` | Emitted `lifecycle_scope` | Emitted `mikebom:optional-derivation` | Change? |
|---|---|---|---|---|
| `true` | `true` | `Build` | (not emitted — Decision 2 buildscript-wins) | NONE (per US2 acceptance 5) |
| `true` | `false` | `Build` | (not emitted) | NONE |
| `false` | **`true`** | **`Optional`** ✱ | **`"gradle-compile-only"`** ✱ | ✱ CHANGED |
| `false` | `false` | `None` | (not emitted) | NONE |

✱ = new m184 behavior. One row changes. All other rows preserve pre-m184 behavior byte-identically.

**Helper** (new in m184):
```rust
/// Milestone 184 US2 — Detect the "compile-only shape" per entry.
///
/// A Gradle dep is classified as `LifecycleScope::Optional` iff it
/// appears on any `*compileClasspath` configuration AND is absent from
/// any `*runtimeClasspath` configuration. Suffix-check per Decision 3
/// so multi-source-set + multi-project builds are covered.
fn is_compile_only_shape(configs: &str) -> bool {
    let items: Vec<&str> = configs.split(',').map(|s| s.trim()).collect();
    let has_compile = items.iter().any(|c| c.ends_with("compileClasspath"));
    let has_runtime = items.iter().any(|c| c.ends_with("runtimeClasspath"));
    has_compile && !has_runtime
}
```

**Classifier extension** at `read_gradle_lockfile`:
```rust
let (lifecycle_scope, is_m184_optional) = if is_buildscript {
    // Buildscript wins (Decision 2 US2 acceptance 5); no annotation.
    (Some(LifecycleScope::Build), false)
} else if is_compile_only_shape(configs_value) {
    // Runtime candidate with compile-only shape → Optional.
    (Some(LifecycleScope::Optional), true)
} else {
    // Pre-m184 default: None for non-buildscript, non-compile-only.
    (None, false)
};
```

## 4. C122 Parity Catalog Value-Set Update

The C122 catalog row at `mikebom-cli/src/parity/extractors/mod.rs:545` is UNCHANGED — it's the same `Directionality::SymmetricEqual` extractor. Only the value-set grows:

**Pre-m184 (post-m183)**:
- `cargo-optional-true` (m179)
- `npm-optional-dependencies` (m180, m181)
- `pip-optional-dependencies` (m183)

**Post-m184**:
- `cargo-optional-true` (m179)
- `npm-optional-dependencies` (m180, m181)
- `pip-optional-dependencies` (m183)
- **`maven-optional-element`** ✱ (m184 US1)
- **`gradle-compile-only`** ✱ (m184 US2)

The docstring at `cdx.rs:866` ALREADY lists both new values as placeholders since m179 — m184 makes them real by wiring the code paths that populate them. No docstring edit needed.

## 5. Test Contract

**Unit tests** (colocated with the code they cover):

**maven.rs::tests** (new):
- `parse_pom_xml_extracts_optional_true` (parse — verifies the `<optional>` element flows into `PomDependency.optional`)
- `parse_pom_xml_optional_false_or_absent_stays_false` (parse regression pin)
- `pom_dep_to_entry_optional_true_default_scope_classifies_as_optional` (US1 acceptance 1+2)
- `pom_dep_to_entry_optional_true_scope_test_stays_test` (US1 acceptance 4 + Decision 2 pin)
- `pom_dep_to_entry_optional_true_scope_provided_stays_build` (Decision 2 provided-scope pin)
- `pom_dep_to_entry_optional_false_stays_runtime` (US1 acceptance 3 regression pin)
- `pom_dep_to_entry_optional_absent_stays_runtime` (regression pin: no `<optional>` element at all)

**gradle/lockfile.rs::tests** (new):
- `is_compile_only_shape_detects_compile_only` (helper unit)
- `is_compile_only_shape_rejects_compile_and_runtime` (US2 acceptance 3 pin)
- `is_compile_only_shape_rejects_runtime_only` (US2 acceptance 4 pin)
- `is_compile_only_shape_detects_test_compile_only` (Decision 3 suffix-match pin — testCompileClasspath alone)
- `is_compile_only_shape_detects_source_set_variants` (Decision 3 suffix-match pin — custom source set names)
- `read_gradle_lockfile_compile_only_classifies_as_optional` (US2 acceptance 1+2 end-to-end)
- `read_gradle_lockfile_buildscript_compile_only_stays_build` (US2 acceptance 5 + Decision 2 buildscript-wins pin)
- `read_gradle_lockfile_runtime_stays_none` (regression pin: `compileClasspath,runtimeClasspath` shape stays pre-m184)

**Integration tests** (via existing regression fixtures + potential new fixtures):

- Existing maven regression tests (m085/m087/m130-US2) MUST continue to pass byte-identically OR show only additive changes if the fixture happens to include `<optional>` deps.
- Existing gradle lockfile regression tests (m106 US1 `configurations_recorded_in_annotation`) MUST continue to pass byte-identically OR show only additive changes if the fixture happens to include compile-only shapes.
- Golden regen expected drift: additive `mikebom:optional-derivation` properties + `scope: "excluded"` on maven/gradle goldens only IF the underlying pom.xml or gradle.lockfile fixture contains m184-classifiable signals. Zero drift on non-Java goldens per SC-004.

**FR-013 zero-new-dep verification** (T029-equivalent):
- `cargo tree -p mikebom | wc -l` MUST be identical pre- vs post-m184 (1131 lines expected).

## 6. Backward Compatibility

- **Fixtures with NO `<optional>` elements + NO compile-only-shape gradle entries**: byte-identical output (SC-004 gate).
- **Fixtures with maven deps + explicit `<scope>test</scope>` / `<scope>provided</scope>`**: byte-identical output (Decision 2 preserves the scope-classification path).
- **Existing tests in maven.rs + gradle/lockfile.rs**: unchanged behavior. Any test that happens to construct `PomDependency` values will need `optional: false` added to the struct literal — this is a source-level breaking change on the struct definition BUT NOT a behavior change (the field defaults to false when initialized by the parser).
