# Phase 1 Data Model: transitive propagation of non-runtime lifecycle scope

**Feature**: `1269-transitive-dev-scope` | **Date**: 2026-10-25

No new persisted state. One accessor on an existing enum, one new
per-component annotation, and two in-pass sets that live for the
duration of a single emission.

---

## Changed types

### `LifecycleScope` (`waybill-common/src/resolution.rs:432`)

Unchanged variants. One new accessor:

```rust
impl LifecycleScope {
    /// Strength as a claim about production: higher means a stronger
    /// claim that the component ships. The propagation pass takes the
    /// strongest scope across all inbound paths (FR-004), so this is a
    /// total order over the five variants and not a preference list.
    ///
    /// `Optional` outranks `Development` deliberately: optional means
    /// the component MAY ship, development means it does not, and when
    /// two paths disagree the weaker claim about production is the
    /// safer one to publish.
    ///
    /// Exhaustive by construction — a sixth variant fails to compile
    /// here rather than silently sorting last.
    pub fn strength(&self) -> u8 {
        match self {
            LifecycleScope::Runtime => 4,
            LifecycleScope::Optional => 3,
            LifecycleScope::Build => 2,
            LifecycleScope::Test => 1,
            LifecycleScope::Development => 0,
        }
    }
}
```

Validation: `strength()` MUST be injective over the five variants (a
unit test asserts five distinct values), or "strongest wins" would be
ambiguous for a tie it cannot detect.

### `ResolvedComponent.extra_annotations`

Gains at most one entry, on components the pass marked:

| key | value | when |
|---|---|---|
| `waybill:lifecycle-scope-derivation` | `"transitive"` | iff this pass assigned `lifecycle_scope` |

Absent on every other component (FR-008), including every
reader-declared non-runtime component. A non-runtime scope with no
entry reads as declared.

---

## New catalog row

**C199** — `waybill:lifecycle-scope-derivation`

| field | value |
|---|---|
| carrier (CDX) | per-component `properties[]`: `{"name":"waybill:lifecycle-scope-derivation","value":"transitive"}` |
| carrier (SPDX 2.3) | per-Package `annotations[].comment`, `MikebomAnnotationCommentV1` envelope |
| carrier (SPDX 3) | per-Package `Annotation.statement`, same envelope |
| directionality | `SymmetricEqual` |
| vocabulary | closed enum, one value in v1: `transitive` |
| native audit | KEEP-NO-NATIVE in all three formats — see research R6 |

Mutual exclusions and neighbours:

- **Not** emitted on a component whose `lifecycle_scope` is `None` or
  `Runtime` — the marker explains a non-runtime scope, it does not
  assert one.
- Distinct from `waybill:optional-derivation` (C122), whose values name
  a reader's lockfile construct (`pep-735-dependency-groups:all`,
  `npm-optional-dependencies`). C199 names *who decided*, C122 names
  *which declaration a reader read*. A component may carry C122 and
  never C199, since C122 implies a declaration.
- Same posture as C197 `waybill:version-absence`: mark the anomalous
  population only, so every untouched component stays byte-identical.

---

## In-pass state (not persisted)

### Immunity set

```
runtime_reachable: HashSet<&str>   // canonical PURL strings
```

Components reachable from any root seed over **declared, non-bridge**
edges, never traversing into a component that already carries a
non-runtime scope. Built once. Membership is absolute immunity
(FR-002).

### Assignment map

```
assigned: HashMap<&str, LifecycleScope>
```

Written only for components whose `lifecycle_scope` is `None` and which
are not in the immunity set. On a second arrival, the stronger scope
wins (FR-004). Applied to components at the end of the pass, so no
component is read mid-mutation and the result is independent of
component order.

### Traversal edge set

An edge participates in both traversals iff:

- its `relationship_type` is a dependency relation, and
- its `provenance.data_type` is neither `cross-ecosystem-bridge` nor
  `cross-ecosystem-bridge-ambiguous` (FR-002b, research R3).

Anchoring edges need no exclusion rule: none exist yet at this point in
the pipeline (research R2). The pass asserts that, rather than assuming
it.

---

## State transitions

A component's `lifecycle_scope` moves along exactly one edge of this
graph, once per scan:

```
None ──(not runtime-reachable, reached from a declared
         non-runtime seed)──▶ Some(strongest inbound)   + C199 marker
None ──(runtime-reachable, or unreached)──────────────▶ None
Some(_) ─────────────────────────────────────────────▶ Some(_)   unchanged
```

The third row is FR-003 and is what makes FR-013 hold: a document whose
every non-runtime component was declared by a reader has no `None`
component to assign, so nothing changes and no marker appears.
