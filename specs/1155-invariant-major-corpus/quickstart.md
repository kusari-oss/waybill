# Quickstart — Working with the Invariant Layer

Four recipes: add an invariant, declare a known failure, read the
matrix, and prove an invariant can actually fail. The last one is not
optional — it is SC-004, and it is the check this repo has learned to
insist on.

---

## 1. Add an invariant

Six edits. The point of the layer is that this is all it takes to check
every target — and that none of the six is per-target work.

1. **Catalog row** in `docs/reference/sbom-invariants.md` — next number, statement, precondition, kinds, `Active`, plus the `### I<n>` diagnostic prose. The diagnostic is gated: an active row without one fails the lane.
2. **Checker** at `waybill-cli/tests/corpus_harness_195/invariants/checks/<name>.rs`:

```rust
pub fn check(sboms: &EmittedSboms) -> CellOutcome {
    // Precondition FIRST, and it returns NotApplicable — never Pass.
    // A pass here would be a vacuous one, which is the whole thing
    // this layer exists to prevent (FR-006, SC-009).
    let subjects = file_tier_components(sboms);
    if subjects.is_empty() {
        return CellOutcome::NotApplicable {
            reason: "no file-tier components in this document".into(),
        };
    }

    let offenders = /* ... */;
    if offenders.is_empty() {
        return CellOutcome::Pass;
    }
    CellOutcome::Fail(vec![Violation {
        kind: KIND_FILE_TIER_DEPENDENCY_EDGE,
        count: offenders.len(),
        example: Some(offenders[0].clone()),
    }])
}
```

3. **Module declaration** — `pub mod <name>;` in `invariants/checks/mod.rs`. Easy to forget, and the compiler's error points at the registry line rather than here.
4. **Registry line** in `invariants/mod.rs`, naming the number, the statement, the precondition, the kinds and the checker. The statement and precondition must match the catalog row **verbatim** (modulo line wrapping); a gate compares them, so a reworded row without a matching registry edit fails.
5. **Mutation test** in `invariants/mutation_tests.rs` — not optional, see recipe 4.
6. **Run it** — see recipe 3. Every target is now evaluated; expect red cells, and declare them (recipe 2) or fix them.

The gate will reject the invariant if the catalog row and the registry
entry disagree, in either direction, or if the row has no kinds.

### Choosing kinds

The kind is the part a declaration pins (FR-012), so it must be stable
against upstream churn and fine enough that a second defect cannot hide
under an existing declaration.

- **Good**: `duplicate-file-identity` vs `duplicate-library-identity` — a duplicated file and a duplicated library are different defects with different causes.
- **Bad**: `duplicate` — one kind absorbs everything, and FR-012 stops catching anything.
- **Bad**: `duplicate-util.sh` — derived from the data, so an upstream rename reddens the lane for no reason.

---

## 2. Declare a known failure

When a cell fails and the fix belongs elsewhere:

```toml
[[declaration]]
invariant = "I1"
target = "image-postgres16"
kind = "file-tier-dependency-edge"
count_at_declaration = 82
reason = """
Orphan-anchoring emits a plain dependsOn for file-tier components.
Retyping to containment is a parity-model change across three formats.
"""
issue = 1206
```

Keep entries sorted by `(invariant, target)`. The count is recorded for
context and will **not** fail the lane when it drifts; the `kind` will,
if it changes or if a second kind appears.

**When a fix lands, delete the entry in the same change.** The lane
fails on a stale declaration (FR-011) precisely so a fixed defect cannot
keep its exemption.

---

## 3. Read the matrix

The matrix runs in **two** places, and both matter.

```sh
# Everything hermetic: catalog <-> registry, declarations schema, the
# full 7 x 17 matrix over the committed goldens, the mutation check.
# No network, no containers, no corpus clone.
cargo test -p waybill --test public_corpus invariants::

# The grid, printed:
cargo test -p waybill --test public_corpus \
    invariants::tests::golden_matrix -- --nocapture

# Freshly emitted documents, in the dispatched corpus lane, same gating
# as every other corpus check. Writes target/invariants/run-<id>.json.
WAYBILL_RUN_PUBLIC_CORPUS=1 cargo test -p waybill --test public_corpus -- --include-ignored
```

The goldens catch a refresh that imports a defect; the lane catches a
defect before a refresh can hide it. Neither is in
`scripts/pre-pr.sh`'s two mandatory commands in the sense of being new
work there — but the hermetic half does run under
`cargo test --workspace`, which is one of them, so an undeclared cell
fails a normal pre-PR run.

Console output is the grid, failures and declarations first. The JSON
artifact holds every cell, the per-invariant applicability counts
(FR-015a) and the ecosystem coverage statement (FR-015).

### Reading a fix's reach

This is SC-003, and it is a diff, not an investigation:

```sh
# before the fix
cp target/invariants/run-<sha-before>.json /tmp/before.json
# after
diff <(jq -S '.cells' /tmp/before.json) <(jq -S '.cells' target/invariants/run-<sha-after>.json)
```

Cells moving `fail → pass` are the ecosystems the fix reached. Cells
still failing are the ones it did not — which is the question that
previously took a session of reading documents one at a time.

The declarations file diff tells the same story at review time: a fix
that clears three of four ecosystems removes three entries and leaves
the fourth as the visible record of the gap.

---

## 4. Prove the invariant can fail (required)

**A green cell is not evidence until you have seen it go red.** Two
distinct traps, both recorded in this repo's history:

- *A passing check that exercises nothing* — `feedback_a_passing_test_may_exercise_nothing`.
- *Healthy and broken looking identical* — `feedback_zero_results_can_mean_broken`.

For every new invariant, do both:

**Mutation (SC-004)** — on a target whose cell passes, perturb the
emitted document so the property is violated, and confirm that cell goes
red with the expected kind. An invariant that cannot be made to fail
checks nothing.

**Applicability (SC-010, FR-006a)** — read the applicability count in
the report. It must be ≥ 1, and the lane fails otherwise. If it is 1 of
17, the invariant is nearly decoration even though it passes the gate:
that is exactly what FR-015a publishes the count for.

The matrix's 51 passing cells in `measurements/findings.md` have **not**
had this done to them. The 12 failures there are trustworthy; the passes
are not yet evidence of anything.

---

## Terminology

Since the rename (FR-020), the two words mean different things and the
distinction is the one US3 asks reviewers to make:

| Term | Means | Lives in |
|---|---|---|
| **invariant** | a property of *any* emitted SBOM | the catalog + `invariants/`, and layer 0 |
| **expectation** | a claim about *one specific* target | `expectations.rs`, per-target functions |

A check that mentions a particular target's package, version or verdict
is an expectation. A check that would be equally true of a document from
any target is an invariant and belongs in the catalog. If you cannot
tell from the name which one you are reading, that is an FR-020a bug.
