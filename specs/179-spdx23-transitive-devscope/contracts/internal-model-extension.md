# Contract: Internal Model Extension

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## Scope

Extends three enums in the internal type model without breaking any existing serde round-trip or match-arm exhaustiveness contract downstream of the enums.

## Public API changes

### `mikebom_common::resolution::LifecycleScope`

Gains a new variant `Optional`. Backwards compatibility:

- Any existing production code that pattern-matches on `LifecycleScope` MUST be updated to cover the new variant (compiler catches this).
- `LifecycleScope::as_str()` gains value `"optional"`.
- `LifecycleScope::is_non_runtime()` returns `true` for `Optional` (inherited behavior via `!matches!(self, Runtime)`).
- serde JSON serialization: `"optional"` (via `rename_all = "snake_case"`).

### `mikebom_common::resolution::RelationshipType`

Gains a new variant `OptionalDependsOn`. Backwards compatibility:

- Any existing pattern-match MUST be updated.
- serde JSON serialization: `"optional_depends_on"` (via `rename_all = "snake_case"`).

### `mikebom_cli::generate::spdx::relationships::SpdxRelationshipType`

Gains a new variant `OptionalDependencyOf`. Backwards compatibility:

- Wire-format value: `"OPTIONAL_DEPENDENCY_OF"` (SPDX 2.3 §11.1 convention).
- The `Display`/`as_str()` impl MUST return exactly `"OPTIONAL_DEPENDENCY_OF"`.

## Test signatures

```rust
#[test]
fn lifecycle_scope_optional_serde_roundtrip() {
    let scope = LifecycleScope::Optional;
    let json = serde_json::to_string(&scope).unwrap();
    assert_eq!(json, r#""optional""#);
    let back: LifecycleScope = serde_json::from_str(&json).unwrap();
    assert_eq!(back, scope);
}

#[test]
fn lifecycle_scope_optional_is_non_runtime() {
    assert!(LifecycleScope::Optional.is_non_runtime());
}

#[test]
fn lifecycle_scope_optional_as_str() {
    assert_eq!(LifecycleScope::Optional.as_str(), "optional");
}

#[test]
fn lifecycle_scope_legacy_dev_excludes_optional() {
    // FR-015 precedence semantics: `Optional` is NOT dev-legacy.
    assert!(!lifecycle_scope_is_legacy_dev(&Some(LifecycleScope::Optional)));
}

#[test]
fn relationship_type_optional_serde_roundtrip() {
    let rt = RelationshipType::OptionalDependsOn;
    let json = serde_json::to_string(&rt).unwrap();
    assert_eq!(json, r#""optional_depends_on""#);
    let back: RelationshipType = serde_json::from_str(&json).unwrap();
    assert_eq!(back, rt);
}

#[test]
fn spdx_relationship_type_optional_wire_value() {
    let sr = SpdxRelationshipType::OptionalDependencyOf;
    assert_eq!(sr.to_string(), "OPTIONAL_DEPENDENCY_OF");
}
```

All test signatures MUST compile and pass before any classifier code is added — this is a Phase 2 (foundational tests-first) task.
