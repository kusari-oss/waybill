# Contract — `invariant-must-pass.toml` (v2)

Location: `waybill-cli/tests/corpus_harness_195/invariant-must-pass.toml`.

**Supersedes** `specs/1156-version-absence-marker/contracts/must-pass-registry.md`,
which this feature's three clarifications amend in four places: a seventh
validation rule, the grid mark, missing-versus-empty file semantics, and the
lane pass returning records rather than strings. That file stays where it is
as the record of what feature 1156 specified; this one is what gets built.

Companion and mirror image of `invariant-declarations.toml`: declarations
record cells known to be **broken**, must-pass entries record cells known to be
**meaningful**.

## Why the name is not `invariant-expectations.toml`

The harness reserves "expectation" for a target-specific check and "invariant"
for a universal property, and enforces the split with a source-level
terminology gate (feature 1155 FR-020/FR-020a) so the two can be told apart by
name alone. A registry of required outcomes is neither; it must not borrow the
name of a category of checks.

## Entry shape

```toml
[[must_pass]]
invariant = "I8"
target = "pants-clojure-polyglot"
reason = """
Four Pants resolve anchors -- two jvm (java17, java21) and two python
(pants-2.30, pants-2.31) -- and no other versionless component in the
document. A NotApplicable here would mean all four anchors have left, dropped
by the Pants readers or by a golden refresh; the cell would read green and the
version-absence marker would become unread code. This is the one seeded cell
the anchors alone can trip.
"""
issue = 1216
```

`reason` answers **"what would a `NotApplicable` here mean?"** — for *this*
target, not in general (FR-013a). "It passes today" is rejected by review:
that sentence records no judgement and reproduces the vacuity this file exists
to prevent. So is a reason that implies the anchors alone would trip a cell
whose document carries other versionless components; three of the four seeded
targets are in that position, measured at
`specs/1217-must-pass-registry/measurements/versionless-populations.py`.

## Semantics

| Cell outcome | Listed here | Result |
|---|---|---|
| `Pass` | yes | satisfied; the cell is marked in the grid |
| `NotApplicable` | yes | **lane fails** — the registry's whole purpose |
| `Fail` | yes | lane fails (also caught as `UndeclaredFailure`) |
| `Error` | yes | lane fails (already `Verdict::Error`) |
| target not evaluated this run | yes | **skipped**, not unmet (FR-009) |
| anything | no | unchanged from today |

The failure message must name the invariant, the target, the observed outcome
and the entry's reason — a bare "must-pass cell failed" sends the reader back
to this file to learn what they broke.

## Validation, all before any comparison

1. `invariant` names a live, non-retired invariant in `REGISTRY`.
2. `target` is present in the corpus manifest.
3. **`target` is evaluable by the lane that always runs** — a target in
   `goldens::GOLDENS_ARE_FILTERED` is rejected. An entry guarded only by the
   network-gated lane is a gate that may not run for months, and "some lane
   checks it" would otherwise be true by construction. *(New in v2.)*
4. `reason` is non-empty after trimming.
5. No duplicate `(invariant, target)`.
6. Entries sorted ascending by `(invariant, target)`.
7. No `(invariant, target)` appears in both this file and
   `invariant-declarations.toml`.

An invalid file fails the lane rather than being partly honoured, matching
`declarations::validate` — a skipped entry is an invisible exemption.

**A missing or unreadable file fails the lane. An empty file is valid and
green.** A deleted gate must not read as a passing one; an empty one is the
honest state of the world before any cell has been judged load-bearing.
*(New in v2.)*

## Attachment point

A separate corpus-level pass, not a third argument to `verdict()`.
`verdict(outcome, declared)` stays the pure two-input eight-row state machine
that feature 1155's data model documents and `mod.rs`'s tests pin. The
precedent is `unfalsifiable_invariants(cells)` in the same file: a
corpus-level pass contributing lane failures of its own.

```rust
pub fn unmet(cells: &[Cell], file: &MustPassFile) -> Vec<Unmet>
```

Records, not strings, because the grid needs to know which cells are
registered and not merely how to print a failure. *(Changed in v2.)*

**Unlike the applicability floor, this pass runs under `Scope::SingleTarget`
too.** The floor is a corpus-wide property that false-alarmed per target; this
pass skips entries for targets a run did not evaluate, so a single-target run
checks exactly that target's entries. Gating it by scope would mean the corpus
lane never enforces the registry.

This is a requirement, not a note — FR-008a — and SC-017 requires a test of it
that needs no network, because the lane that would notice the loss is the
network-gated one. *(New in v2.)*

## Report rendering

Must-pass failures appear in `lane_failures()` alongside the other classes,
sorted with them. The grid additionally marks **every** registered cell,
whether met or not, as `  *<glyph>` — two spaces, the mark, the cell's own
outcome glyph. Four columns, so alignment is unchanged; the outcome stays
legible, so a registered cell that fails still reads as a failure. The legend
line gains `* must-pass`. *(New in v2.)*

Marking the commitment rather than only the breach is what lets a reader tell
a load-bearing `.` from an incidental one — the distinction the feature
exists to make.

## Seed

The four Pants I8 cells, each with a reason of the shape above. No other cell.
Sweeping in whatever is green today would record no judgement.

## Coverage

Every committed entry must be evaluated by the always-run lane, asserted over
that lane's evaluated cells by a test — not by inspection, and not by the
static rule 3 alone, which cannot see a target whose goldens are missing from
disk. *(New in v2.)*

## Its own mutation test

A registry that cannot fail is the same vacuity one level up, so the feature
must show it failing: force one registered cell to report `NotApplicable` and
assert the lane fails and names the cell. This is SC-002 and it is not
optional — it is the only evidence the registry works.
