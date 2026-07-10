# Data Model: Unified Optional-Dependency Classification (m179)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

## 1. Extended Types

### 1.1 `LifecycleScope` enum (`mikebom-common/src/resolution.rs:370`)

**Before**:
```rust
pub enum LifecycleScope {
    Runtime,
    Development,
    Build,
    Test,
}
```

**After**:
```rust
pub enum LifecycleScope {
    Runtime,
    Development,
    Build,
    Test,
    Optional,   // NEW (m179)
}
```

**`as_str()` update** (line 382-389):
```rust
pub fn as_str(&self) -> &'static str {
    match self {
        LifecycleScope::Runtime => "runtime",
        LifecycleScope::Development => "development",
        LifecycleScope::Build => "build",
        LifecycleScope::Test => "test",
        LifecycleScope::Optional => "optional",   // NEW
    }
}
```

**`is_non_runtime()` behavior** (line 394-396): NO CHANGE. The implementation `!matches!(self, LifecycleScope::Runtime)` inherits the correct behavior automatically — `Optional` returns `true`, driving CDX `scope: "excluded"` per FR-006.

**`lifecycle_scope_is_legacy_dev()` helper** (line 477): NO CHANGE. This function returns `true` only for `Development | Build | Test` — not for `Optional`. Rationale in plan.md's Data Model section.

**serde behavior**: `#[serde(rename_all = "snake_case")]` on the enum inherits automatically — `Optional` serializes to `"optional"`. Unit test:
```rust
#[test]
fn lifecycle_scope_optional_serde() {
    let scope = LifecycleScope::Optional;
    assert_eq!(serde_json::to_string(&scope).unwrap(), r#""optional""#);
    assert_eq!(scope.as_str(), "optional");
    assert!(scope.is_non_runtime());
}
```

### 1.2 `RelationshipType` enum (`mikebom-common/src/resolution.rs:487`)

**Before**:
```rust
pub enum RelationshipType {
    DependsOn,
    DevDependsOn,
    BuildDependsOn,
    TestDependsOn,
}
```

**After**:
```rust
pub enum RelationshipType {
    DependsOn,
    DevDependsOn,
    BuildDependsOn,
    TestDependsOn,
    OptionalDependsOn,   // NEW (m179)
}
```

**serde behavior**: `#[serde(rename_all = "snake_case")]` → `OptionalDependsOn` serializes to `"optional_depends_on"`.

### 1.3 `SpdxRelationshipType` enum (`mikebom-cli/src/generate/spdx/relationships.rs:34`)

**Before**:
```rust
pub enum SpdxRelationshipType {
    // ... existing variants ...
    Describes,
    Contains,
    DependsOn,
    DevDependencyOf,
    BuildDependencyOf,
    TestDependencyOf,
    ProvidedDependencyOf,   // m178
}
```

**After**:
```rust
pub enum SpdxRelationshipType {
    // ... existing variants ...
    Describes,
    Contains,
    DependsOn,
    DevDependencyOf,
    BuildDependencyOf,
    TestDependencyOf,
    ProvidedDependencyOf,
    OptionalDependencyOf,   // NEW (m179)
}
```

**Wire-format value**: `"OPTIONAL_DEPENDENCY_OF"` (SPDX 2.3 §11.1 uppercase snake_case convention).

**Display impl update**: the existing `Display for SpdxRelationshipType` (or `as_str()` method) MUST return `"OPTIONAL_DEPENDENCY_OF"` for the new variant.

## 2. Classifier Dispatch Table

`apply_lifecycle_scope_to_edges` at `mikebom-cli/src/scan_fs/mod.rs:1261` MUST implement this exact precedence-ordered dispatch. The pass runs after all reader passes complete, so `component.lifecycle_scope` and `component.build_inclusion` are final by the time this pass reads them.

For each edge `rel` where `rel.relationship_type == RelationshipType::DependsOn`:

```
target = scope_by_purl.get(rel.to.as_str())          // Option<LifecycleScope>
inclusion = inclusion_by_purl.get(rel.to.as_str())    // Option<BuildInclusion>

// Precedence 1-4: manifest-declared lifecycle scope wins
match target {
    Some(LifecycleScope::Test)        => rel.relationship_type = TestDependsOn;    continue;
    Some(LifecycleScope::Development) => rel.relationship_type = DevDependsOn;     continue;
    Some(LifecycleScope::Build)       => rel.relationship_type = BuildDependsOn;   continue;
    Some(LifecycleScope::Optional)    => rel.relationship_type = OptionalDependsOn; continue;  // NEW (m179)
    Some(LifecycleScope::Runtime)     => continue;  // Runtime: no rewrite (unchanged m052)
    None                              => {}         // fall through to Precedence 5
}

// Precedence 5: m112 build-inclusion (only when no lifecycle_scope was set)  // NEW (m179)
match inclusion {
    Some(BuildInclusion::NotNeeded) => rel.relationship_type = TestDependsOn; continue;
    _                               => {}
}

// Precedence 6: default (unchanged)
// rel.relationship_type stays DependsOn
```

**Existing implementation at `scan_fs/mod.rs:1266-1287` uses a single `HashMap<&str, LifecycleScope>` lookup — m179 adds a parallel `HashMap<&str, BuildInclusion>` lookup for the same targets.** The two lookups run at O(1) per edge; the overall pass stays O(n_edges).

## 3. Classifier Emission (SPDX 2.3)

The SPDX 2.3 classifier at `mikebom-cli/src/generate/spdx/relationships.rs:241-279` currently has these match arms (post-m178):

```
match (compat, rel.relationship_type) {
    (Full, DependsOn) if peer_edges.contains(&(from, to)) =>
        (to_id, from_id, ProvidedDependencyOf),  // m178
    (_, DependsOn) => (from_id, to_id, DependsOn),
    (Basic, _) => (from_id, to_id, DependsOn),
    (Full, DevDependsOn) => (to_id, from_id, DevDependencyOf),   // m052
    (Full, BuildDependsOn) => (to_id, from_id, BuildDependencyOf), // m052
    (Full, TestDependsOn) => (to_id, from_id, TestDependencyOf),  // m052
}
```

**m179 adds one new arm** (order matters: this arm sits ABOVE the catch-all Basic arm and BELOW the peer-guard arm):

```
    (Full, OptionalDependsOn) => (to_id, from_id, OptionalDependencyOf),   // NEW (m179)
```

**Note on completeness**: after m179, Rust's exhaustive match forces the compiler to catch every `RelationshipType` variant. The existing arms cover every variant; the new `OptionalDependsOn` gets its own arm. `Basic` mode's catch-all `(Basic, _) => DependsOn` continues to swallow every typed variant per FR-003.

## 4. Classifier Emission (SPDX 3.0.1)

The SPDX 3 classifier at `mikebom-cli/src/generate/spdx/v3_relationships.rs:96-105` currently maps typed variants to `lifecycleScope` parameter values:

```
match rel_type {
    RelationshipType::DevDependsOn => Some("development"),
    RelationshipType::BuildDependsOn => Some("build"),
    RelationshipType::TestDependsOn => Some("test"),
    _ => None,
}
```

**m179 MUST NOT** add `RelationshipType::OptionalDependsOn => Some("optional")` — SPDX 3.0.1's `LifecycleScopeType` enum has no `"optional"` value at spec version 3.0.1 (verified against the JSON schema pinned in mikebom's SPDX 3 conformance harness at `mikebom-cli/src/generate/spdx/v3_*`). Adding a non-standard value would fail the m078 `spdx3-validate` conformance gate.

**m179 approach**: `OptionalDependsOn` emits SPDX 3 relationships with NO `lifecycleScope` parameter (falls through to the `_ => None` arm). Classification information rides on the `mikebom:optional-derivation` annotation instead, which is a Principle V KEEP-BOTH carve-out for the SPDX 3 side (native SPDX 3 lacks the construct; annotation carries it).

If a future SPDX 3 minor spec (3.1, 3.2, ...) adds an `optional` value to `LifecycleScopeType`, a follow-up milestone can flip that switch on.

## 5. Ecosystem Reader Extensions

Each ecosystem reader that discovers an "optional-declared" dep MUST:

1. Set `component.lifecycle_scope = Some(LifecycleScope::Optional)` on the resolved component.
2. Insert into `component.extra_annotations`: `"mikebom:optional-derivation" → serde_json::Value::String("<ecosystem-specific-value>".to_string())`.

**Ecosystem → derivation value mapping** (per FR-008 through FR-013):

| Ecosystem | Reader | Derivation value |
|-----------|--------|------------------|
| Cargo | `cargo.rs` | `"cargo-optional-true"` |
| npm | `npm/mod.rs` + `npm/package_lock.rs` | `"npm-optional-dependencies"` |
| yarn v1 + Berry | `npm/yarn_lock.rs` | `"npm-optional-dependencies"` (same value; Yarn wraps npm) |
| pnpm | `npm/pnpm_lock.rs` | `"npm-optional-dependencies"` (same value) |
| pip PEP 621 | `pip/pyproject.rs` | `"pip-extras-require"` |
| pip setup.py | `pip/setup_py.rs` | `"pip-extras-require"` |
| pip setup.cfg | `pip/setup_cfg.rs` | `"pip-extras-require"` |
| uv | `pip/uv_lock.rs` | `"pip-extras-require"` |
| Poetry | (pip readers) | `"pip-extras-require"` |
| Maven | `maven.rs` | `"maven-optional-element"` |
| Gradle | `gradle/` | `"gradle-compile-only"` |
| Erlang | `erlang.rs` | `"erlang-optional-applications"` |

**Precedence when a reader detects both dev-scope AND optional**: manifest-declared dev/build/test WINS (per FR-015). Practical example: a Cargo `[dev-dependencies]` entry that ALSO has `optional = true` (rare but legal) → the entry is `Development`-scoped, NOT `Optional`-scoped. Implementation: readers check the table (`[dependencies]` vs `[dev-dependencies]` vs `[build-dependencies]`) FIRST, then check `optional = true` only within `[dependencies]`.

## 6. New Annotation Contract

The `mikebom:optional-derivation` annotation is a component-level `extra_annotations` entry that MUST round-trip byte-identically through all three format emitters:

- **CDX 1.6**: `component.properties[]` entry with `name` = `"mikebom:optional-derivation"`, `value` = one of the string values above.
- **SPDX 2.3**: `Package.annotations[]` entry wrapping the `MikebomAnnotationCommentV1` envelope (see contracts/mikebom-optional-derivation.md for the exact JSON shape).
- **SPDX 3.0.1**: `spdx:Annotation` node with `spdx:statement` containing the same envelope.

The parity extractor at `mikebom-cli/src/parity/extractors/` MUST register a new row for this annotation with `Directionality::SymmetricEqual` — value MUST match across the three emitters for the same source component.

## 7. Test Contract

**Unit tests** (`mikebom-common/src/resolution.rs::tests`):
- `LifecycleScope::Optional` serde round-trip + `as_str()` + `is_non_runtime()`.
- `RelationshipType::OptionalDependsOn` serde round-trip.

**Unit tests** (`mikebom-cli/src/generate/spdx/relationships.rs::tests`):
- `optional_depends_on_reverses_to_optional_dependency_of` (Full mode).
- `optional_depends_on_collapses_to_depends_on_in_basic_mode`.
- `precedence_lifecycle_scope_wins_over_build_inclusion`.
- `precedence_optional_wins_over_not_needed`.

**Integration tests** (`mikebom-cli/tests/optional_dep_classification.rs`):
- One test per user story (US1 through US7 as they land per milestone cadence).
- SC-001 flagship: yaml.v3 fixture — 23 components with CDX `scope: "excluded"` MUST match 23 SPDX 2.3 typed-dep-scope-source components.
- SC-002 set-equality: PURL set derived via CDX filter MUST equal PURL set derived via SPDX 2.3 filter.

**Golden fixtures**:
- SC-004 CDX zero-drift gate: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --workspace` regeneration must show ONLY additive changes on fixtures that exercise the new signal (Cargo fixture); untouched fixtures show zero drift.
- SC-005 SPDX 3 zero-drift gate: `MIKEBOM_UPDATE_SPDX3_GOLDENS=1` regeneration must show `mikebom:optional-derivation` annotation additions on affected components; typed relationships remain unchanged (SPDX 3 has no `OPTIONAL_DEPENDENCY_OF`).
- SC-003 SPDX 2.3 no-decrement gate: `MIKEBOM_UPDATE_SPDX_GOLDENS=1` regeneration must show ADDITIVE typed-relationship changes only; no `TEST_DEPENDENCY_OF` / `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` edges are removed.
