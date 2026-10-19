# Quickstart — the must-pass registry

## Run the gate

```sh
# The hermetic lane. No network, no container pulls, no corpus cache.
cargo test --test public_corpus golden_matrix_is_evaluable_and_gated -- --nocapture

# The registry's own tests.
cargo test --test public_corpus must_pass

# Everything, the way CI runs it.
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

The grid prints under `--nocapture`. A registered cell shows its mark:

```text
invariant matrix — . pass  X fail  - n/a  E error  (d) declared  * must-pass

target                   I1  I2  I3  I4  I5  I6  I7  I8
pants-clojure-polyglot    .   .   -   .   -   .   .  *.
pants-example-django     (d)  .   .   .   -   .  (d) *.
```

## Record a cell whose `Pass` is load-bearing

You have just fixed a defect, a cell now passes, and you want it to stay
meaningful. Add an entry to
`waybill-cli/tests/corpus_harness_195/invariant-must-pass.toml`, **at its
sorted position** by `(invariant, target)`:

```toml
[[must_pass]]
invariant = "I3"
target = "rust-ripgrep"
reason = """
The cargo reader now propagates dev-dependency scope transitively, and this
is the target where that path is exercised. A NotApplicable here would mean
the document declares no non-runtime edge at all -- the reader stopped
classifying dev-dependencies -- and the propagation code would be unread
while the cell read green.
"""
issue = 1209
```

The `reason` has one job: **what would a `NotApplicable` here mean, on this
target?** Two ways to get it wrong:

- "It passes today." Records no judgement. Rejected by review, not by code.
- A sentence that assumes your change is the only thing keeping the
  precondition met, when the document has other components that also meet it.
  Check before you write it — `measurements/versionless-populations.py` is the
  shape of that check for I8, and three of the four seeded targets are in
  exactly that position.

Then run the gate. Validation will tell you if the invariant is retired, the
target unknown or filtered, the reason empty, the cell duplicated, the file
unsorted, or the cell already declared as a known failure.

## The other half: is the thing it depends on still there?

The registry catches a cell that stops **applying**. It cannot catch the
component the cell depends on quietly leaving a document that stays applicable
for other reasons — on three of the four seeded targets, the anchor can vanish
and I8 still reports `Pass` because other versionless components remain.

That half lives in the per-target expectation layer, where a claim about one
target belongs:

```rust
// in expectations.rs, the shape pants_example_jvm_expectations already uses
if !cdx_has_component_purl(&sboms.cdx, |p| {
    p == "pkg:generic/python-default?pants-namespace=python"
}) { /* AssertionFailure */ }
```

Beware the near-miss: `pants-resolve-annotation-present` asserts that *some*
component carries `waybill:pants-resolve`, which reads like anchor coverage
and is not — 34 components satisfy it on django. If you add an entry for a
cell whose meaning rests on one specific component, pin that component by
PURL as well (#1230).

## Remove an entry

Only as part of a change that makes the cell legitimately stop applying — and
say why in that change's diff. Deleting an entry because the lane went red is
the one move this file exists to make visible.

## What a failure looks like

```text
1 lane failure(s):
  I8 / pants-clojure-polyglot: must-pass cell is not applicable: every
  component carries a version — recorded as load-bearing because: Four Pants
  resolve anchors … A NotApplicable here would mean all four anchors have
  left …
```

The reason travels with the failure on purpose. The reader who broke it should
not have to open the file to find out what they broke.

## Why there are two files

| file | records | a cell may be in |
|---|---|---|
| `invariant-declarations.toml` | cells known to be **broken**, with the kind pinned | one of them |
| `invariant-must-pass.toml` | cells whose **`Pass` is load-bearing** | or the other, never both |

A cell required to pass and accepted as failing is a half-finished change, and
the lane refuses to pick which half to honour.
