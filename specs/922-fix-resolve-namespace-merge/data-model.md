# Phase 1 — Data Model

**Feature**: `922-fix-resolve-namespace-merge` (#919)

## Entity: Pants language namespace

The `pants.toml` section that declares a resolve. Already exists as a closed
set (`python | jvm`) from milestone 912; this milestone gives it a
per-component home.

| | |
|---|---|
| **Shape** | closed set, not a string (Principle IV) |
| **Values** | `python` (Pex and uv lockfiles), `jvm` (coursier lockfiles) |
| **Cardinality per component** | **exactly one**, measured (research R1) |
| **Source** | the reader that produced the component — never inferred from PURL ecosystem (R2) |
| **Wire** | new per-component annotation, catalogue row **C164** |
| **Absence** | means the component belongs to no Pants resolve — never "a resolve whose namespace we could not determine" |

### Why scalar, and what guards it

Cross-namespace membership cannot arise today: Python readers emit
`pkg:pypi/*` and `pkg:generic/*`, the coursier reader emits `pkg:maven/*`, and
dedup only unions components sharing a PURL. That is a property of today's
readers, not an invariant. So the accessor **detects** the impossible case and
refuses to answer, rather than modelling it as plural or assuming it away.

## Entity: Qualified resolve

The identity of a resolve: its namespace **and** its name. The defect is that
the name alone was treated as the identity.

| | |
|---|---|
| **Components** | namespace + bare name |
| **Rendered** | `python:default` — the spelling milestone 912 already uses for the document identity (C163) |
| **Used as** | the split's grouping key, replacing the bare `String` |
| **Equality** | both parts. `python:default` ≠ `jvm:default` |

**Constructed only from a namespace plus a name.** It must not be
constructible from a bare string, because the defect is precisely that a bare
string was accepted where an identity was required.

## Entity: Resolve membership *(unchanged)*

Which resolves pin a component. Keeps its v0.9.0 key and array-of-bare-names
shape (FR-006a) — the namespace rides alongside rather than being folded in.

Bare names remain unambiguous **in combination** with the component's scalar
namespace: every resolve a component belongs to is in that component's
namespace, which is what makes the pairing work without qualifying the values.

## Relationships

```
component ──has──> namespace            (exactly one, new)
component ──has──> membership[]         (bare names, unchanged)
(namespace, membership[i]) ──is──> qualified resolve
qualified resolve ──groups──> one split document
```

The middle line is the whole feature: neither field alone identifies a
resolve, and together they do.

## What this does not change

- Which components, edges or resolves are discovered.
- The membership annotation's key or value shape.
- `waybill:resolve-ownership` (C161) — out of scope, tracked as #924.
- The document identity (C163). It states the resolves a document represents;
  once grouping is qualified, that is always exactly one, and its plural case
  simply stops arising.
