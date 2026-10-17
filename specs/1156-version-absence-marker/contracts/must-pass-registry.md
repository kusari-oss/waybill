# Contract — `invariant-must-pass.toml`

Location: `waybill-cli/tests/corpus_harness_195/invariant-must-pass.toml`.
Companion and mirror image of `invariant-declarations.toml`: declarations
record cells known to be **broken**, must-pass entries record cells known
to be **meaningful**.

## Why the name is not `invariant-expectations.toml`

The harness reserves "expectation" for a target-specific check and
"invariant" for a universal property, and enforces the split with a
source-level terminology gate (feature 1155 FR-020/FR-020a) so the two can
be told apart by name alone. A registry of required outcomes is neither; it
must not borrow the name of a category of checks.

## Entry shape

```toml
[[must_pass]]
invariant = "I8"
target = "pants-example-django"
reason = """
A NotApplicable here would mean the resolve anchor is no longer in the
document at all -- either the Pants reader stopped emitting it or a golden
refresh dropped it. The cell would go green and the version-absence marker
would become unread code. I8 stays falsifiable corpus-wide via
image-postgres16, so `unfalsifiable_invariants` cannot see this.
"""
issue = 1216
```

`reason` answers **"what would a `NotApplicable` here mean?"**. "It passes
today" is rejected by review: that sentence records no judgement and
reproduces the vacuity this file exists to prevent.

## Semantics

| Cell outcome | Listed here | Result |
|---|---|---|
| `Pass` | yes | satisfied |
| `NotApplicable` | yes | **lane fails** — the registry's whole purpose |
| `Fail` | yes | lane fails (also caught as `UndeclaredFailure`) |
| `Error` | yes | lane fails (already `Verdict::Error`) |
| anything | no | unchanged from today |

The failure message must name the invariant, the target, the observed
outcome and the entry's reason — a bare "must-pass cell failed" sends the
reader back to this file to learn what they broke.

## Validation, all before any comparison

1. `invariant` names a live, non-retired invariant in `REGISTRY`.
2. `target` is present in the corpus manifest.
3. `reason` is non-empty after trimming.
4. No duplicate `(invariant, target)`.
5. Entries sorted ascending by `(invariant, target)`.
6. No `(invariant, target)` appears in both this file and
   `invariant-declarations.toml`.

An invalid file fails the lane rather than being partly honoured, matching
`declarations::validate` — a skipped entry is an invisible exemption.

## Attachment point

A separate corpus-level pass, not a third argument to `verdict()`.
`verdict(outcome, declared)` stays the pure two-input eight-row state
machine that feature 1155's data model documents and `mod.rs`'s tests pin.
The precedent is `unfalsifiable_invariants(cells)` in the same file:
a corpus-level pass contributing lane failures of its own. See research R2.

## Seed

The four Pants I8 cells, each with a reason of the shape above. No other
cell. Sweeping in whatever is green today would record no judgement.

## Its own mutation test

A registry that cannot fail is the same vacuity one level up, so the
feature must show it failing: force one registered cell to report
`NotApplicable` and assert the lane fails and names the cell. This is
SC-011 and it is not optional — it is the only evidence the registry works.
