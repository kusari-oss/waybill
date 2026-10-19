# Phase 0 Research — must-pass registry (feature 1217, issue #1227)

Everything here was read or run against the tree at `98f57f7f`. Where a
number appears, the command that produced it appears with it.

## M1 — How far the seed's trap actually reaches (and a spec correction)

**Question**: the spec asserted that the four seeded targets "have no other
versionless components", so that anchors leaving would flip all four cells to
`NotApplicable`. That claim decides what the seed is worth, so it was
measured rather than carried.

**Probe**: `specs/1217-must-pass-registry/measurements/versionless-populations.py`,
which mirrors `is_explained` in `checks/versionless_explained.rs` over the
committed goldens.

| target | components | versionless | anchors (C197) | survivors if anchors leave | I8 then |
|---|---|---|---|---|---|
| `pants-clojure-polyglot` | 55 | 4 | 4 (`java17`, `java21`, `pants-2.30`, `pants-2.31`) | 0 | `NotApplicable` |
| `pants-example-django` | 47 | 14 | 1 (`python-default`) | 13, all explained | `Pass` |
| `pants-example-jvm` | 29 | 2 | 1 (`jvm-default`) | 1, explained | `Pass` |
| `pants-example-python` | 12 | 2 | 1 (`python-default`) | 1, explained | `Pass` |

**Finding**: the claim was true of one target in four. I8 applies when any
component is versionless and passes when every such component is explained,
so a cell only goes `NotApplicable` when the *whole* versionless population
leaves. On django that population is thirteen more components (twelve
design-tier `pkg:pypi/*` carrying C151, one file-tier); on jvm and python it
is one file-tier component each.

**Consequences, all of which the plan is built on:**

1. The registry's `Pass → NotApplicable` trap can fire on one seeded cell
   today. The spec's "Why this exists" section now carries this table and
   FR-013a requires each reason to state its own target's reach, so the
   committed file cannot imply a protection it does not have.
2. **The stopgap being deleted is blind in exactly the same way.** It asserts
   `CellOutcome::Pass`, and `Pass` is what three of the four would still
   report with their anchor gone. Clarification 3's removal therefore gives up
   nothing — a conclusion that got *stronger* with measurement, not weaker.
3. The residual gap — an anchor disappearing while the cell stays green — is
   not closable by any registry keyed on invariant outcomes. It needs an
   assertion about document *content*, which is M3 below. It is **in scope**
   (FR-019, issue #1230), because M3 found it needs no new machinery.
4. Nothing about the mechanism changes. The registry is general machinery for
   all 136 cells; the seed is where it is first exercised.

**Decision**: proceed as specified, seed unchanged at four.
**Rationale**: the feature's value is the general trap, and FR-010's coverage
test plus FR-014's mutation prove the trap works regardless of how many
seeded cells can trip it today.
**Alternatives considered**: (a) seed only `pants-clojure-polyglot` — rejected,
the other three cells' `Pass` is load-bearing whether or not today's document
shape lets the trap fire, and trimming would make the file a record of what is
convenient to catch; (b) widen I8 so the anchors alone trip it — rejected, that
changes a cell outcome, which the spec puts out of scope, and it would make I8
fail on documents that are correct.

## M3 — Is anchor presence asserted anywhere? (issue #1230)

**Question**: M1's residual gap needs an assertion that the anchor component
is present. Does one already exist? "This target emits this anchor" is a claim
about one target, so the per-target expectation layer is where to look — not
the invariant layer, which holds universal properties.

**Probe**: the second half of `measurements/versionless-populations.py`,
matching each anchor's PURL against `expectations.rs`.

| anchor | asserted |
|---|---|
| `pkg:generic/pants-2.31?pants-namespace=python` | yes — `pants_clojure_polyglot_expectations` (`expectations.rs:852`) |
| `pkg:generic/jvm-default?pants-namespace=jvm` | yes — `pants_example_jvm_expectations` (`expectations.rs:988`) |
| `pkg:generic/java17?pants-namespace=jvm` | **no** |
| `pkg:generic/java21?pants-namespace=jvm` | **no** |
| `pkg:generic/pants-2.30?pants-namespace=python` | **no** |
| `pkg:generic/python-default?pants-namespace=python` (django) | **no** |
| `pkg:generic/python-default?pants-namespace=python` (python) | **no** |

**Finding**: two of seven. The pattern exists and was applied to two targets.

**The near-miss that makes this worth stating.**
`pants-resolve-annotation-present` asserts that *some* component carries
`waybill:pants-resolve`, which reads like anchor coverage. It is not: 34
components satisfy it on django, 28 on jvm, 11 on `pants-example-python`. The
anchor can leave with that expectation green — the same shape as
`feedback_a_passing_test_may_exercise_nothing`.

**Decision**: fold the five missing assertions into this feature (FR-019,
FR-019a), rather than deferring to #1230.
**Rationale**: it is five assertions of an existing shape in a file this
feature already edits, and the hole was measured while planning the feature
that would otherwise record a commitment it cannot keep. The earlier
out-of-scope argument — "a different mechanism" — was about a *general*
marker-presence guard and did not survive contact with the finding that two
such assertions already exist by hand.
**Alternatives considered**: (a) defer to #1230 — rejected, a measured
five-line hole behind an issue number is how it stays open, and this project
has that history: the stopgap this feature deletes was itself a deferral; (b)
derive the assertions automatically for every C197-bearing component — a
larger idea, kept out of scope and recorded there.
**Scope note**: all four of `pants-clojure-polyglot`'s anchors are covered,
not just the three missing ones' convenience — its registry reason cites the
four-anchor case as what the cell exercises, so pinning one anchor would guard
a quarter of the claim.

## M2 — Grid column arithmetic for the commitment mark

**Question**: clarification 2 requires every registered cell to render
distinguishably *and* keep its own outcome glyph. Does a two-character cell
fit without shifting the grid?

**Read**: `report.rs` `console()` — the header writes `format!(" {:>3}", inv.id)`
(4 columns per invariant) and each cell writes `format!("   {glyph}")`
(3 spaces + 1 char = 4 columns).

**Decision**: render a registered cell as `"  *{glyph}"` — 2 spaces, a mark,
the glyph. Four columns, alignment unchanged, outcome preserved. The legend
line gains `* must-pass`.
**Rationale**: it is the only shape that satisfies both halves of the
clarification without touching the column width every other row depends on.
**Alternatives considered**: replacing the glyph as the `d` annotation does —
rejected, it violates US3 scenario 3 (the outcome must stay legible) and
collides with `d` on a cell that is both declared and registered, a state the
overlap rule forbids but which the renderer should not depend on.

## R1 — Attachment point and plumbing

**Decision**: a new `must_pass` module beside `declarations`, exposing
`unmet(cells, file) -> Vec<Unmet>`, and `report::build_scoped` /
`build` / `build_for_target` take the registry as an **explicit parameter**
alongside the declarations file.

**Rationale**: the report already takes the declarations file because it needs
it to compute verdicts and the `d` glyph; a registry that renders in the grid
is the same kind of input. The alternative — a `with_must_pass()` setter
keeping today's signatures — was rejected for a specific reason: a caller who
forgets the setter gets a green report, which is this feature's own failure
mode reproduced in its API. An explicit parameter makes the compiler the thing
that stops it. `verdict(outcome, declared)` is untouched, per FR-008.

**Alternatives considered**: computing unmet entries in each lane and
appending to a local failure list — rejected, FR-012 requires them inside the
report, and two lanes would each need their own copy of the logic.

## R2 — Does the pass run under `Scope::SingleTarget`?

**Decision**: yes. Unlike `unfalsifiable_invariants`, which `build_scoped`
deliberately skips under `SingleTarget`.

**Rationale**: the applicability floor is a corpus-wide property and was a
false alarm per target — it printed failures for targets that had none, which
is the history the `Scope` enum exists to record. The must-pass pass has no
such problem, because FR-009 skips entries whose target this run did not
evaluate: on a single-target run, exactly the entries for that target are
checked and the rest are silent. Gating it by scope would instead mean the
corpus lane never enforces the registry, which FR-006 requires it to.

This asymmetry is the single most likely thing for a later reader to
"correct", so the module and the contract both state it.

## R3 — Missing versus empty, made demonstrable

**Read**: `declarations::load()` panics via `unwrap_or_else` on a read or
parse error. SC-006 requires a test that removing the file fails the lane,
and a test cannot delete a committed file.

**Decision**: `load_at(path) -> Result<MustPassFile, String>` as the real
entry point; `load()` resolves `must_pass_path()` and panics with the error.
A missing path yields `Err` naming the path; an empty file parses to an empty
`Vec` through `#[serde(default)]` and is valid.

**Rationale**: it makes FR-011 testable without a destructive fixture — point
`load_at` at a path inside a `tempfile::tempdir()` for the missing case, and at
an empty temp file for the empty case. `tempfile` is already a dev-dependency.
**Alternatives considered**: renaming the committed file during the test —
rejected, a test that mutates the repository can leave the tree broken when it
fails, which is the opposite of what a gate should do.

## R4 — Making FR-010 enforceable (clarification 1)

**Decision**: two mechanisms, not one.

1. **Static, in validation**: reject an entry whose target is in
   `goldens::GOLDENS_ARE_FILTERED`. No I/O, runs with the other six rules, and
   catches the case at authoring time with a message saying what the author is
   asking for.
2. **Observed, in a test**: over the hermetic matrix's evaluated cells, assert
   every committed entry's `(invariant, target)` is present among them.

**Rationale**: the two fail differently. The static rule catches a *filtered*
target; the observed test catches a target whose goldens are simply missing
from disk, which validation cannot see without reading the filesystem. Either
alone leaves a way for an entry to be checked by nothing.
**Alternatives considered**: calling `goldens::load(target).is_some()` inside
validation — rejected, it puts a filesystem read inside a pure validator and
would make the rule's failure depend on cache state.

## R5 — Validation signature and the overlap rule's direction

**Decision**: `validate(&MustPassFile, &DeclarationsFile) -> Vec<String>`,
with the overlap rule checked from the registry's side only.

**Rationale**: the rule is a property of the *pair*, not of either file, and
both lanes load both files and run both validators before any comparison.
Checking it twice would double the message on one mistake.

## R6 — Line stability

**Decision**: mirror `line_stability.rs` — a `render(&MustPassFile) -> String`
helper over the parsed model, an insert at a middle sort position, an assertion
that every other entry's rendered block survives verbatim, and a removal that
restores the rendering exactly.

**Rationale**: testing the parsed model rather than the file's bytes is a
deliberate choice the predecessor already made; the property is about entry
shape, and a bytes-level test would also fail on a comment edit.
**Note for the probe entry**: the seed is four entries that share the
invariant `I8`, so an insert must sort into the middle by *target*
(`pants-example-golang` lands between django and jvm). An entry appended at
either end would pass the test vacuously, which is the trap the predecessor's
comment calls out.

## R7 — Reconciling the 1156 stopgap (clarification 3)

**Read**: `PANTS_I8_TARGETS` at `mutation_tests.rs:424` feeds three loops —
`i8_applies_and_passes_on_each_named_pants_target` (:454, the stopgap),
`removing_the_marker_from_one_anchor_fails_i8` (:476) and a third at :493.

**Decision**: delete only the stopgap. The const and the other two tests stay,
because they mutate the document and assert the cell goes red — they test the
checker, not the registry, and nothing here replaces them. Update the I8
header comment in `invariant-declarations.toml`, which currently names the
stopgap as what holds those four cells.

**Rationale**: M1 finding 2 — the stopgap asserts the same `Pass` the registry
asserts, with the same blind spot, so it is a pure duplicate. A comment
pointing at a deleted test is worse than no comment.

## R8 — The draft preserved in #1227

The issue inlines a 555-line `must_pass.rs` and the seeded TOML. It compiles
against the harness as of feature 1156's merge and was never run. Treat it as a
reference to re-read, per spec Assumption. **What the clarifications changed
about it**, so a task does not reintroduce the old shape:

| draft | now |
|---|---|
| six validation rules | **seven** — the always-run-lane rule (R4) |
| `unsatisfied(cells, file) -> Vec<String>` | `unmet(cells, file) -> Vec<Unmet>`, so the report can render as well as print |
| `load()` panicking | `load_at(path) -> Result<_, String>` + panicking `load()` (R3) |
| report prints the failure | report prints **and** marks the grid (M2) |
| contract under `specs/1156-.../contracts/` | superseded by this feature's contract |

The draft's four reasons were written against the measured corpus and are
substantially right — notably django's, which already says a `NotApplicable`
there "would mean the pip reader stopped emitting design-tier components too",
i.e. the very reach M1 measured. They are the starting text for FR-013a, not a
blank page.

## R9 — Gates the feature must not trip

- **`no_new_io.rs`**: the `FORBIDDEN` token list needs no new entry —
  `std::fs::read` is already permitted for repository files. The doc comment at
  `:28` enumerates them ("the catalog, the declarations file, the ecosystems doc
  and the committed goldens") and must name the registry (FR-016, 1156's T042).
- **`terminology.rs`**: checked. It gates `expectations.rs` naming, the `I<n>`
  namespace and invariant statements. `invariant-must-pass.toml` and
  `must_pass.rs` trip none of it, and the name satisfies FR-002 — it borrows
  neither "expectation" nor "invariant" alone.
- **`clippy::unwrap_used`** is denied in `waybill-cli` including `#[cfg(test)]`,
  so the tests module needs the established
  `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard.
- **The `rust-toolchain.toml` pin**: run the gate with no `+toolchain`
  argument.

## R10 — Feature 1156's eleven deferred tasks

This feature's input, mapped so none is silently dropped. All eleven are
carried; three are amended by the clarifications.

| 1156 task | lands as | amended |
|---|---|---|
| T026 entry + file types, `load()` | Phase 2 | yes — `load_at` Result (R3) |
| T027 `validate` with six rules | Phase 2 | yes — seven rules (R4) |
| T028 the lane pass | Phase 2 | yes — `unmet` returning records (R1) |
| T029 `mod must_pass;` + invocation as a sibling | Phase 2 | no |
| T030 surface in the report | Phase 4 | yes — grid mark too (M2) |
| T031 seed the TOML | Phase 3 | yes — FR-013a per-target reach |
| T035 the `NotApplicable` mutation (SC-011) | Phase 2 | no |
| T036 per-rule validation tests | Phase 3 | seven, not six |
| T037 the line-stability pair | Phase 3 | no |
| T038 committed-file-is-valid test | Phase 3 | no |
| T042 the `no_new_io` comment | Phase 6 | no |
