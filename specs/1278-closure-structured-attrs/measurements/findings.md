# Findings — 1278-closure-structured-attrs

Things this milestone corrected about its own reasoning. Recorded because an
abandoned or mistaken step that leaves no trace gets repeated.

## F1 — a count that was right about the wrong population

The first pass at "how many components wrongly claim to be absent from the
artifact" said **16**. It selected golden components whose *corrected* role was
`artifact-input` and never checked their role *before*. That swept in `hspec`,
`HUnit`, `hspec-discover`, `call-stack`, `quickcheck-io` and `haskell-lexer` —
whose roles do not change at all. Their `excluded` comes from dev-scope
lifecycle propagation, which is independent of this defect and remains
correct.

Tracing one of them is what exposed it: `hspec` is referenced via
`buildInputs` **from `env`**, a carrier the reader already read. Nothing about
it could have changed.

The defensible count is **9**, isolated by filtering on
`waybill:scope-classification-source = "closure-role"` — the exclusions
resting on the role and nothing else.

**The rule**: when a fix changes a classification, a finding about it must
compare before against after for the *same* item. Selecting on the after-state
alone measures the population that ends up in a state, not the population the
fix moves. This is the recorded "a right count can carry a wrong cause"
failure with the direction reversed.

## F2 — the control that was available all along

The spec was written with a control of 347-of-348 agreement between the
probe's env-only arm and a per-component view of the committed golden. The
golden carries the derivation-level census directly, in its own C184
annotation, and it matches the probe **exactly**: 1,749 / 256 / 101 / 39 /
1,353.

The weaker control was constructed by re-deriving per-component roles when the
artifact already stated the figure being checked.

**The rule**: before building a comparison, read what the artifact already
asserts. A derived view agreeing 99% is weaker evidence than a stated figure
agreeing exactly, and it costs more to produce.

## F3 — two formats deciding one claim twice

Milestone 1069 routed the per-ecosystem dependency-completeness claims through
one helper so CycloneDX and SPDX 3 could not drift. The **root** claim was
left behind: `build_compositions` recomputed the predicate inline while
`dependency_claims` computed it for SPDX 3.

Found by demoting the claim and watching only SPDX 3 change. The m1069
cross-format test did not catch it, because it exercises the per-ecosystem
claims and not the root one.

**The rule**: "the two formats read the same answer" is a property of each
claim, not of a module. A helper that exists to prevent drift prevents it only
for the callers that actually call it.

## F4 — two test defects with the shape of the bug under repair

Both self-inflicted, both fixed, and both worth recording because they are the
*same* failure mode this milestone exists to fix: a lookup that finds nothing
is indistinguishable from a thing that is not there.

1. The annotation helper searched for a `values` map. The wire envelope is one
   `{schema, field, value}` per annotation. The test reported "absent from
   SPDX 2.3" for an annotation that was emitted correctly.
2. The US3 test asserted that a typed-only identity in the `unreferenced` role
   would be emitted. Unreferenced members that apply no patch are dropped by
   design, so it failed for a reason unrelated to the name it was testing. It
   now uses a *referenced* member whose derivation `name` deliberately differs
   from its typed `pname`, so the assertion distinguishes reading the pname
   from falling back to the name.

**The rule**: a test asserting absence must first prove the mechanism can
observe presence. Both of these would have been caught immediately by a
positive control, which the final versions carry.

## F5 — adding a required field has a blast radius

`ClassifiedClosure` and `RawDerivation` are constructed at eleven sites across
six files, nearly all of them tests in other modules. Adding one required
field to each meant eleven mechanical edits before anything compiled.

Not a defect, and not worth a refactor here. Recorded because it is the cost
of the seam being shared, which is the same property that made the fix itself
a four-accessor change rather than a four-consumer change. The cost landed in
the direction worth paying.
