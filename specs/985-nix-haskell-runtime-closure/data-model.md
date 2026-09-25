# Phase 1 Data Model — milestone 985 (issue #962)

Entities, their fields, and the rules that govern them. Rust shapes are
indicative; the binding constraints are the validation rules, which map to FRs.

---

## E1. `DependencyRelations`

Extracted per attribute while parsing `hackage-packages.nix`, alongside the
existing `version` and `sha256`.

```rust
pub(crate) struct DependencyRelations {
    /// Names from `libraryHaskellDepends`.
    library: Vec<String>,
    /// Names from `executableHaskellDepends`.
    executable: Vec<String>,
}
```

**Rules**

| # | Rule | Source |
|---|---|---|
| E1.1 | Keyed on the **attribute** name, never `pname` — 371 attributes share a pname at one measured revision | R2, #970 |
| E1.2 | Test and benchmark relation fields are **not** extracted | FR-002, issue #985 |
| E1.3 | A missing field is an empty list, not an error — most attributes have no executable relations | R1 |
| E1.4 | Names are stored verbatim; case is identity for Hackage (`Diff` ≠ `diff`) | m926 / #943 |

---

## E2. `ClosureMember`

A package the walk reached. Produced by the walk, consumed by emission.

```rust
pub(crate) struct ClosureMember {
    name: String,
    origin: ComponentOrigin,
    /// Attributes whose relations named this one. Drives edge emission.
    reached_from: BTreeSet<String>,
}
```

**Rules**

| # | Rule | Source |
|---|---|---|
| E2.1 | Each name appears at most once in the emitted document | FR-012 |
| E2.2 | `reached_from` is a set, so one name reached by several parents yields several edges and one component | FR-009, FR-012 |
| E2.3 | `reached_from` is empty exactly for declared members | E3 |
| E2.4 | Ordering is deterministic (`BTreeSet`), because output must be byte-identical across runs | FR-013 |

---

## E3. `ComponentOrigin`

The explicit provenance marker. A **typed enum**, not a bare string (Principle
IV), serialized at the emission boundary.

```rust
pub(crate) enum ComponentOrigin {
    /// Named by the project's own manifest.
    Declared,
    /// Reached only through another package's runtime relations.
    Transitive,
}
```

**Rules**

| # | Rule | Source |
|---|---|---|
| E3.1 | Present on **every** Haskell component the resolver touched, declared ones included | FR-006a |
| E3.2 | A name reachable both ways is `Declared` — the stronger claim wins | FR-007 |
| E3.3 | Never inferred from graph position | FR-006, R3 |
| E3.4 | Emitted via a `waybill:` annotation; no format has a native carrier | R3 (Principle V audit) |

---

## E4. `ClosureSummary`

The document-scope record of what the pass did.

```rust
pub(crate) struct ClosureSummary {
    declared: usize,
    transitive: usize,
    unresolved: BTreeMap<String, usize>,   // reason -> count
    relations_walked: usize,
}
```

**Rules**

| # | Rule | Source |
|---|---|---|
| E4.1 | Emitted whenever the closure ran; absent when it did not | FR-014, FR-015 |
| E4.2 | `unresolved` is keyed by reason, reusing milestone 926's reason vocabulary | FR-005 |
| E4.3 | Reason keys are a closed set; a new reason is a deliberate vocabulary change | m926 precedent |
| E4.4 | Ordering deterministic (`BTreeMap`) | FR-013 |

> **Why a summary at all**: milestone 973 shipped because milestone 926 computed
> exactly this kind of record, logged it, and dropped it — leaving a document
> that could not distinguish "the pass ran and resolved 97" from "the pass never
> ran". `ClosureSummary` must reach the document, not a `tracing::info!`.

---

## E5. `ClosureEdge`

A relation to emit into the dependency graph.

```rust
pub(crate) struct ClosureEdge {
    from: String,   // attribute name
    to: String,     // attribute name
}
```

**Rules**

| # | Rule | Source |
|---|---|---|
| E5.1 | Both endpoints MUST name a component present in the document — invariant I2 | FR-008 |
| E5.2 | Emitted from the **actual parent**, never synthesised from the root | FR-009, R7 |
| E5.3 | Endpoints are attribute names at construction and MUST be rewritten to final PURLs if identities change afterwards, via the `apply_renames` helper from #981 | R7, #980 |
| E5.4 | An edge to an unresolvable name still points at a real (versionless) component, because such names are emitted | FR-005a |

> **Why E5.3 is called out**: milestone 980 was precisely this failure —
> resolution rewrote component PURLs after edges were built, disconnecting every
> component it resolved (66% of one project's SPDX relationships). The closure
> multiplies both counts, so the same mistake would be larger.

---

## State transitions

A name moves through exactly one path per scan:

```
declared name ──────────────► Declared, resolved
                └───────────► Declared, unresolved(reason)

name in a parent's relations ─► already seen ──► no change (E2.2 adds an edge)
                              ├─► boot ────────► emitted versionless,
                              │                   NOT traversed (R5)
                              ├─► resolvable ──► Transitive, resolved, enqueued
                              └─► unresolvable ► Transitive, unresolved(reason),
                                                  NOT enqueued
```

**Termination**: `seen` is checked before enqueue, so a cycle revisits nothing
(FR-010, R4).

**Precedence**: a name first seen as transitive and later found declared is
promoted to `Declared` (E3.2). The reverse never demotes.
