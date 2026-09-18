# Phase 1 Data Model: Per-resolve SBOMs

**Feature**: `911-per-resolve-sboms` | **Date**: 2026-09-17

No persistent store. Every entity is either an annotation on an emitted
document or in-process state for the duration of one scan.

---

## Entity: Resolve membership

The set of Pants resolves that pin a given package.

| Aspect | Before | After |
|---|---|---|
| Carrier | `waybill:pants-resolve` (C143) | unchanged — the row widens, it is not replaced |
| Value | bare string, `"app"` | lex-sorted JSON array, `["app","tools"]` |
| Cardinality on the wire | one | one *or more*, always as an array |
| Written by | 3 readers (R3) | same 3 readers |
| Survives dedup | no — first wins | yes — union |

**Validation**

- Lexically sorted, always. Order is not incidental: FR-003 requires two scans
  of one repository to agree, and read order is the thing that varies.
- Never empty. A component carrying the annotation names at least one resolve;
  a component with no resolve does not carry the key at all.
- No duplicates within the array.
- A component belonging to one resolve uses `["app"]`, not `"app"`
  (FR-006a). The encoding does not vary with cardinality.

**Lifetime**: set by the reader, unioned at dedup, emitted. Never read back.

---

## Entity: Resolve provenance

Whether a resolve was declared by the repository or discovered by convention.

| Aspect | Today | After |
|---|---|---|
| Carrier | `waybill:resolve-ownership` (C161), document scope | unchanged row |
| Value | `weak-classification=N;unanchored-lockfiles=N` | extended to *name* the resolves per category |
| Answers "how many?" | yes | yes |
| Answers "which?" | **no** | yes (FR-007) |

**Validation**

- The declared and discovered lists together account for every resolve named
  on any component (SC-006). A resolve appearing in membership but in neither
  list is a defect, not a third category.
- A resolve appears in exactly one of the two lists.
- Discovered resolves remain unanchored (FR-009). Naming them is information;
  it is not ownership.

**Constraint inherited from R5**: the existing value is a semicolon-delimited
`key=value` string, not JSON. Extending it changes a documented grammar, and
the catalogue row plus all three extractors move in the same change or the
parity gate fails.

---

## Entity: Resolve anchor

The component a declared resolve's packages hang from.

Unchanged by this feature, and deliberately so. Emitted for declared resolves
only. Its role here is narrower than it first appears: after R1, the anchor is
**not** what the per-resolve split partitions on, so a repository without
anchors is still partitionable.

---

## Entity: Per-resolve projection

The components and relationships that become one sub-SBOM.

| Field | Source |
|---|---|
| resolve name | membership value |
| components | every component whose membership contains this resolve |
| relationships | every relationship whose endpoints are both in that set |
| root component | the resolve's anchor if declared; synthesised if discovered (R2) |

**Selection rule**: membership filter, **not** BFS from a seed (R1). This is
the one place this feature departs from how `--split` has worked since m215,
and the reason is that a discovered resolve has no seed to start from.

**Validation**

- A package in several resolves appears in several projections, carrying its
  **full** membership in each (FR-011a) — not narrowed to the projection's own
  resolve.
- A projection's relationship set never references a component outside it.
- A projection's components may name resolves absent from that projection.
  Expected under FR-011a; any validation over split output must accept it.

**Lifetime**: built at emit time, consumed by the serializer, dropped.

---

## Entity: Dedup merge policy

Not a runtime entity — a rule about which annotations union.

| Key class | Policy | Why |
|---|---|---|
| plural membership | **union** | the relation is many-to-many; first-wins drops real claims |
| everything else | first-wins, unchanged | m109's rule is correct for single-valued evidence: unioning `sbom-tier` across a merge yields a value true of neither side |

Explicitly an allowlist of plural keys rather than a rule inferred from the
value's shape (R4) — `waybill:source-files` and `waybill:file-paths` are
already arrays and already unioned by a dedicated pass, and a generic rule
would double-handle them.

---

## What this feature does not change

- Which packages are discovered.
- Which resolves get anchors.
- The dedup grouping key.
- The annotation key `waybill:pants-resolve` — the row widens rather than
  being retired, so nothing needs a migration map.
