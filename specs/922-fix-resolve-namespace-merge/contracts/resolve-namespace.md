# Contract: per-component Pants language namespace

**Feature**: `922-fix-resolve-namespace-merge` (#919)
**Consumers**: anything reading resolve membership, or consuming `--split=resolve` output.

Additive on the wire. Behavioural change to the split, where the current
behaviour is wrong.

---

## C-1. Every component with resolve membership carries a namespace

Catalogue row **C164**, per component, in all three formats, decoding to the
same scalar value (`python` or `jvm`).

Emitted **unconditionally** — not only where names collide (FR-006d). Presence
must not depend on whether another resolve happened to share a name, so
absence means exactly one thing: this component is in no Pants resolve.

## C-2. The namespace comes from the producing reader

Never inferred from the component's PURL ecosystem. Measured: a **Python**
resolve routinely contains `pkg:generic/*` components, so ecosystem is not a
weak signal for namespace — for that class it is no signal at all.

## C-3. The namespace is scalar, and plurality is detected rather than modelled

Exactly one per component. If a component ever accumulates two, the accessor
returns "cannot answer" and warns; it does not pick one.

Guessing would file a component into the wrong resolve's document — the exact
defect this milestone fixes, reintroduced one layer down.

## C-4. Membership keeps its shape

`waybill:pants-resolve` retains its v0.9.0 key and array-of-bare-names value.
A consumer reading it today keeps working unchanged. Bare names are
unambiguous in combination with the component's namespace.

## C-5. Two resolves sharing a name produce two documents

Grouping is on the qualified resolve. Each document contains only its own
resolve's components, and states exactly one identity (C163).

Where a collision exists, the documents are distinguishable by **filename**,
manifest **`subproject_id`** and manifest **`root_purl`**, each
namespace-qualified. Where no collision exists, all three are unchanged — the
fix is invisible where the defect was absent.

**The existing filename-collision fallback does not cover this.** It appends
`sha8_hex(source_dir)`, and every resolve projection's synthetic root has the
same empty source directory, so both colliding resolves hash identically and
collide again.

## C-6. A repository whose only resolves collide still splits

The group count is taken **after** regrouping. Counting before it leaves a
colliding pair looking like one group, and the not-partitionable fallback
emits a single unsplit SBOM — the simplest real-world case, swallowed
silently, behind a warning that reads like correct behaviour.

A genuinely single-resolve repository still falls back. Only the miscount is
fixed.

## C-7. Nothing else moves

Non-colliding repositories produce byte-identical split output. Discovery is
unchanged. C161 is untouched (#924).

---

## Verification

| Contract | How verified |
|---|---|
| C-1 | every component with membership also has a namespace, across all Pants fixtures and corpus targets |
| C-1 (absence) | a non-Pants component carries neither |
| C-2 | a Python resolve's `pkg:generic/*` members carry `python`, not an ecosystem-derived value |
| C-3 | unit: a component with two namespaces yields "cannot answer" and warns |
| C-4 | membership key and value shape byte-identical to v0.9.0 on a non-colliding fixture |
| C-5 | the collision fixture yields two documents, disjoint components, distinct filenames and manifest ids |
| C-5 (no collision) | every existing Pants fixture's split output byte-identical to before |
| C-6 | a fixture with **only** the colliding pair splits; a genuine single-resolve fixture still falls back |
| C-7 | corpus goldens move only by the added annotation |
