# Contract — The Declarations File

**Artifact**: `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`
**Consumer**: `corpus_harness_195::invariants::declarations`
**Status**: committed. This is the **only** committed state in the
feature and the only thing the gate compares against (FR-014b).

A declaration is a recorded decision to accept a known failing cell. It
is how a scoped fix becomes a visible row rather than a sentence in a
spec's Edge Cases — the failure mode `specs/194-close-orphan-gaps/spec.md:60`
demonstrates.

---

## Schema

```toml
# Each entry accepts ONE failing (invariant, target) cell.
# Sorted by (invariant, target). Non-passing cells only.

[[declaration]]
invariant = "I1"
target = "image-postgres16"
kind = "file-tier-dependency-edge"
count_at_declaration = 82
reason = """
dpkg maintainer scripts are anchored to the image root by the m194 US1
orphan-anchoring pass, which emits a plain dependsOn. Retyping to
containment is a parity-model change across three formats.
"""
issue = 1206
```

| Field | Required | Validation |
|---|---|---|
| `invariant` | yes | matches an `Active` catalog row; a retired or unknown ID fails the lane (FR-001c) |
| `target` | yes | exists in the corpus manifest; an unknown target fails the lane |
| `kind` | yes | listed in that invariant's `Kinds` |
| `count_at_declaration` | yes | non-negative integer; **context only, never compared** |
| `reason` | yes | non-empty prose (FR-010) |
| `issue` | no | issue number |

Duplicate `(invariant, target)` pairs fail the lane — a cell has at most
one declaration.

---

## Comparison semantics

Settled by clarification Q4. The **kind is pinned; the count drifts.**

| Observed | Declared | Result |
|---|---|---|
| `Fail` with kinds == `{declared}` | present | **accepted**, whatever the count |
| `Fail` with kinds ⊋ `{declared}` | present | **fails** — a second kind appeared |
| `Fail` with a different kind | present | **fails** — not the defect that was accepted |
| `Pass` | present | **fails** — stale; remove it (FR-011) |
| `NotApplicable` | present | **fails** — stale; the cell no longer fails |
| `Error` | present | **fails** — an error is never accepted |
| `Fail` | absent | **fails** — undeclared (FR-009) |

Why the count is excluded: counts move for reasons unrelated to waybill
— an upstream base-image bump taking python-slim's 82 dpkg scripts to
83, or a corpus repin. A gate that reddens on innocent drift is the gate
that gets switched off, which is the history m117 had to undo for
`walk.audit-allowlist.txt`. The count is still recorded, and reported,
so a large move is visible to a human reading the matrix even though it
does not fail the lane.

---

## Line stability

Required by FR-014c, and the reason the m115 shape was reworked in m117.

- Entries sorted by `(invariant, target)`, ascending.
- One `[[declaration]]` table per cell, fields in schema order.
- Adding an entry inserts at its sorted position and moves no other entry's content.
- Adding a corpus target touches only that target's own entries.
- Removing a fixed cell's entry deletes a contiguous block.

A reviewer reading the diff of this file should see exactly which cells
were accepted, released or re-accepted — nothing else.

---

## Lifecycle

| Event | Action |
|---|---|
| A new invariant lands red on some targets | add one declaration per failing cell, each with a reason |
| A fix clears a cell | the lane fails as stale; **remove** the entry in the same change as the fix |
| A fix clears cells on 3 of 4 ecosystems | remove 3 entries; the 4th remaining is the visible record of the gap |
| A new defect kind appears on a declared cell | the lane fails; either fix it or add a second declaration naming the new kind |
| An invariant is retired | remove every declaration naming it, in the retiring change |

The third row is the feature's point: **the diff of this file is the
per-ecosystem reach of a fix**, which is how SC-003 is satisfied without
reading emitted documents.

---

## Deliberately absent

- **No expiry or TTL.** A declaration persists until removed. Spec Assumption 2 adopts the allowlist model, which has no expiry; an expiry that reddens a lane on a date nobody chose is drift noise, not a signal.
- **No severity or priority.** Those live on the tracking issue.
- **No passing cells.** Implicit by FR-014b, which keeps the file proportional to outstanding defects.
- **No tolerance bounds on counts.** Clarification Q4 rejected option C; a pinned kind makes a tolerance unnecessary.
