# Phase 1 Data Model — Universal Semantic-Invariant Layer

All types are test-only, inside
`waybill-cli/tests/corpus_harness_195/invariants/`. Nothing here is
serialized into an SBOM; the only persisted shapes are the committed
declarations file and the per-run report artifact.

---

## Invariant

A property expected of any emitted SBOM. One registry entry per
catalogued row.

| Field | Meaning | Rules |
|---|---|---|
| `number` | Durable identity, `I<n>` | Append-only. Never reassigned (FR-001a). A meaning change retires it and allocates a new one (FR-001b). Retired numbers never reused (FR-001c). |
| `statement` | One line: the property | Must be precise enough that a given defect belongs to exactly one invariant (FR-019). |
| `precondition` | Prose description of when the invariant is meaningful | Evaluated inside the checker, not separately (research R3). |
| `kinds` | The violation kinds this invariant can report | Non-empty. A checker that cannot classify may not be catalogued (FR-012a). |
| `diagnostic` | What to do when it fails | Carries forward the existing `suggested_action` convention (FR-005). |
| `checker` | `fn(&EmittedSboms) -> CellOutcome` | One walk, one answer. |

**Relationships**: each `Invariant` is referenced by zero or more
`Declaration`s and produces exactly one `Cell` per corpus target.

---

## CellOutcome

What a checker returns. An enum, not a bool — the distinction between
*pass* and *not-applicable* is the feature's main defence against
vacuity (FR-003, FR-006, SC-009).

| Variant | Meaning | Carries |
|---|---|---|
| `Pass` | Precondition met, property holds | — |
| `Fail` | Precondition met, property violated | one or more `Violation` |
| `NotApplicable` | Precondition unmet; the invariant had nothing to examine | reason |
| `Error` | The checker could not complete (panic caught, malformed input) | message |

`Pass` and `NotApplicable` MUST NOT be collapsed by any consumer. An
`Error` never satisfies a declaration.

---

## Violation

| Field | Meaning | Rules |
|---|---|---|
| `kind` | Which of the invariant's declared kinds | Must be one of that `Invariant`'s `kinds`. **This is the pinned field** (FR-012). |
| `count` | How many instances | Recorded and reported; **never a gate condition** (FR-010, FR-012). |
| `example` | One offending item, for the report | Illustrative only; not compared. |

Splitting `kind` from `count` is the whole of Q4: the kind is what a
declaration accepts, the count is what drifts when an upstream base
image or a corpus pin moves.

---

## Cell

The evaluation of one invariant against one target. Not stored; computed
per run and written to the report artifact.

| Field | Meaning |
|---|---|
| `invariant` | `I<n>` |
| `target` | corpus target name |
| `outcome` | `CellOutcome` |
| `declared` | the matching `Declaration`, if any |
| `verdict` | the lane-facing result (below) |

---

## Cell verdict — the state machine

How an outcome plus a declaration becomes a lane result. This table is
the gate's complete behaviour.

| Outcome | Declaration present? | Verdict | Lane |
|---|---|---|---|
| `Pass` | no | **ok** | passes |
| `Pass` | yes | **stale declaration** | **fails** — FR-011; a fixed defect must not keep its exemption |
| `NotApplicable` | no | **ok (not applicable)** | passes |
| `NotApplicable` | yes | **stale declaration** | **fails** — the cell no longer fails, so the declaration is obsolete |
| `Fail`, kinds == declared kind | yes | **accepted known failure** | passes, regardless of count drift |
| `Fail`, any kind ∉ declared | yes | **undeclared violation kind** | **fails** — FR-012 |
| `Fail` | no | **undeclared failure** | **fails** — FR-009 |
| `Error` | either | **error** | **fails** — FR-007; other cells still evaluate |

Two further lane-level rules that are not per-cell:

- **FR-006a**: an invariant whose every cell is `NotApplicable` fails the lane — it is unfalsifiable against this corpus.
- **FR-001c**: a declaration naming an unknown or retired invariant, or a target not in the manifest, fails the lane rather than being skipped.

---

## Declaration

One committed entry accepting a failing cell. The declarations are the
only committed state and the only thing the gate compares against
(FR-014b).

| Field | Required | Meaning |
|---|---|---|
| `invariant` | yes | `I<n>`; must exist and not be retired |
| `target` | yes | must exist in the manifest |
| `kind` | yes | the pinned violation kind |
| `count_at_declaration` | yes | context only; drift does not fail |
| `reason` | yes | written justification (FR-010) |
| `issue` | no | tracking issue |

Non-passing cells only; passing is implicit, so the file stays
proportional to outstanding defects rather than to the 17 × N cell count
(FR-014b). Sorted by `(invariant, target)` for line stability
(FR-014c).

---

## MatrixReport

The per-run artifact at `target/invariants/run-<sha>.json`, summarised
to the console. Uncommitted and regenerated each run (FR-014a).

| Field | Meaning |
|---|---|
| `waybill_sha` | the build under test |
| `generated_at` | run timestamp |
| `cells` | every `Cell`: state, kinds, counts, declaration status |
| `applicability` | per invariant, the count of targets whose precondition was met (FR-015a) |
| `ecosystems_covered` | exercised by ≥1 target (FR-015) |
| `ecosystems_uncovered` | have a reader, no target (FR-015) |
| `lane_result` | pass, or the failing verdicts |

Ordering within `cells` must be deterministic — sorted by
`(invariant, target)` — so two runs diff cleanly, which is what SC-003
relies on.

---

## Changes to existing types

| Type | Change | Driver |
|---|---|---|
| `AssertionFailure.invariant_name` | → `check_name` | FR-020; the field is shared by layer 0 (where "invariant" is right) and the 17 per-target functions (where it is a misnomer), so a neutral name is accurate for both |
| `CorpusTarget.layer1` | → `expectations` | FR-020; signature unchanged (research R1) |
| `CorpusTarget` | **+ `reader_ecosystems: &[EcosystemId]`** | FR-015; which reader ecosystems this target actually exercises. A *different axis* from the existing `ecosystem: Ecosystem` (`manifest.rs:15`), which is a coarse target classifier whose variants include `PolyglotImage` — not an ecosystem — and which `cross_ecosystem_coverage_check` (`manifest.rs:606`) already asserts against a hand-maintained required set. Both fields stay; neither supersedes the other. Named `reader_ecosystems` rather than `ecosystems` so it is not one letter from `ecosystem`, and not `exercises` — `CorpusTarget.exercises` already exists (`manifest.rs:16`) as free-text milestone prose. |
| **new** `EcosystemId` | **+ enum, one variant per row of `docs/ecosystems.md`'s coverage matrix** | FR-015 / Principle IV. A raw `&str` here fails silently in the one direction that matters: a typo makes a covered ecosystem read as *uncovered*, which is a false claim of the exact kind this feature exists to prevent. Gated bidirectionally against the doc table, same shape as the catalog gate. |
| `layer1_assertions.rs` | → `expectations.rs` | FR-020 |

`layer0_document_integrity` keeps its name and location (spec
Assumption 6); only its failure construction is touched by the field
rename.

---

## The seven initial invariants

Numbers are provisional until the catalog lands (FR-019). Preconditions
are the load-bearing column: each is what stops the cell reporting a
vacuous pass.

| # | Statement | Precondition | Candidate kinds |
|---|---|---|---|
| I1 | No dependency edge targets a file-tier component | document contains ≥1 file-tier component | `file-tier-dependency-edge` |
| I2 | `(name, version, type)` identifies at most one component | ≥2 components | `duplicate-file-identity`, `duplicate-library-identity`, `duplicate-application-identity` |
| I3 | A component reachable only via non-runtime paths carries a non-runtime marker | ≥1 component carries a non-runtime marker | `unmarked-non-runtime-transitive` |
| I4 | CDX and SPDX agree on the root's direct dependency set | both formats emitted, each with a resolvable root | `root-direct-only-in-cdx`, `root-direct-only-in-spdx` |
| I5 | A `complete` verdict implies nothing was excluded from the judgement | completeness verdict is `complete` | `complete-with-unreachable` |
| I6 | A package does not appear under several unknown-version spellings | ≥1 component carries an unknown-version sentinel | `mixed-unknown-version-spelling` |
| I7 | Every component is reachable from the subject, or carries a marker explaining why not | subject resolvable and ≥1 component | `unreachable-and-unexplained` |

Two notes carried from research:

- **I3 and I7 overlap as written** and must be restated (FR-019). The probe's I3 seeds only from already-marked components, so a population that was never marked at all falls through to I7 — which is how the gem and Go instances of one defect class landed in the wrong row.
- **I2's kinds are the least settled** (research R4). Keying on component type matches the measured instances, but whether that is the right axis needs review against FR-012a before the catalog is fixed.
