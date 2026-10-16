# Feature Specification: Universal Semantic-Invariant Layer for the Public Corpus

**Feature Branch**: `1155-invariant-major-corpus`
**Created**: 2026-10-15
**Status**: Draft
**Input**: Issue #1210 — "Invert the corpus invariant layer from target-major to invariant-major: 17 per-target functions check zero properties across targets"

## Context

The public corpus harness has four conceptual layers. Three exist; the one that catches cross-ecosystem defects does not.

| layer | scope | where | state |
|---|---|---|---|
| 0 — structural integrity | every target | `layer1_assertions.rs:1569` `layer0_document_integrity` | **exists** (dangling `dependsOn` refs, etc.) |
| **semantic invariants** | **every target** | — | **missing — this feature** |
| 1 — target expectations | one target each | `layer1_assertions.rs`, 17 functions | exists |
| 2 — goldens | one target each | `layer2_golden.rs` | exists |

`manifest.rs:17` wires exactly one checker per target:

```rust
pub layer1: fn(&EmittedSboms) -> Result<(), AssertionFailure>,
```

and `public_corpus.rs:79` calls `(target.layer1)(&sboms)`.

So a property that should hold for *every* SBOM gets asserted only on the target where someone noticed it was violated. Layer 0 already proves the universal-checker shape works; it is limited to structural integrity — whether the document is internally consistent — and never asks whether the graph it describes is meaningful.

The consequence is a documented, repeating failure mode. `specs/194-close-orphan-gaps/spec.md:60`:

> Other ecosystems (pip nested venvs, cargo workspace crates that aren't top-level, gem nested Gemfiles) may have analogous gaps — those are OUT OF SCOPE for m194 and filed separately **if they surface**.

"If they surface" resolves to "if a human reads that target's output". Those gaps had surfaced and gone unobserved for months.

### Evidence

Seven properties, each derived from a defect originally found by reading one document, evaluated against nine emitted SBOM pairs in one run (probe: `measurements/invariants.py`):

```
                                chatwoot jellyfin kube-proxy kubernetes node-alpine pydantic python-slim starship ubi9
I1 no dependsOn->file-tier          PASS     PASS      FAIL       PASS       FAIL      PASS      FAIL      PASS   FAIL
I2 identity unique                  PASS     PASS      FAIL       FAIL       PASS      PASS      PASS      PASS   PASS
I3 non-runtime propagated           PASS     PASS      PASS       PASS       PASS      FAIL      PASS      PASS   PASS
I4 cross-format root parity         PASS     PASS      PASS       PASS       PASS      PASS      PASS      PASS   PASS
I5 complete means complete          PASS     PASS      PASS       PASS       PASS      PASS      PASS      FAIL   PASS
I6 one unknown spelling             PASS     PASS      PASS       PASS       PASS      FAIL      PASS      PASS   PASS
I7 no silent orphans                FAIL     PASS      PASS       FAIL       PASS      FAIL      PASS      PASS   PASS

12 failing cells of 63
```

Two cells were defects **not** found by reading those same documents beforehand:

- `chatwoot` — 14 gem components unreachable and unmarked, with the platform folded into the version string (`pkg:gem/ffi@1.17.2-arm64-darwin`), silently breaking advisory matching. Filed as #1211.
- `kubernetes` — 6 Go `hack/tools` modules in the same unanchored, unmarked state.

With the gem and Go instances visible alongside the uv one, a single defect class — *non-production or non-selected dependencies are neither marked nor anchored* — is live in four ecosystems. Four separate per-target investigations had each identified one of them.

## Clarifications

### Session 2026-10-15

- Q: How is the matrix persisted, and what does the gate compare against? → A: Commit declarations only (non-passing cells) as the gate, plus write the complete matrix as a per-run uncommitted artifact.
- Q: What is an invariant's durable identity, and what happens when one is restated? → A: Stable, append-only numbers; a restatement that changes a row's meaning retires the old number and allocates a new one, and retired numbers are never reused.
- Q: Is there a floor on how many targets an invariant must actually apply to? → A: At least one applicable target, enforced by the catalog gate, with each invariant's applicable-target count published in the report.
- Q: What counts as a material change in a declared cell's observation? → A: A declaration records a violation kind plus a count; the kind must match exactly while the count may drift freely, and a changed or newly appearing kind fails the lane.
- Q: Which concept owns the word "invariant", given the existing per-target usage? → A: "Invariant" is reserved for universal properties (layer 0 and the new catalog); the per-target concept is renamed to "expectation" at those call sites.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - One invariant, every target, one run (Priority: P1)

A maintainer discovers a defect in an emitted SBOM. Instead of adding an assertion to that target's function, they write the property once. The run reports that property's state for every corpus target, so the defect's true blast radius is known before any fix is designed.

**Why this priority**: This alone converts the recurring failure mode into one-time work per defect class, and it is the smallest slice that delivers that. Everything else in this spec makes it durable; this makes it true once.

**Independent Test**: Add one invariant expressing a property known to hold on some targets and fail on others. Run the corpus lane. Assert the output names the state of that invariant for every target, including the failing ones, in a single invocation.

**Acceptance Scenarios**:

1. **Given** an invariant that fails on 4 of 17 targets, **When** the corpus lane runs, **Then** the output reports all 4 failures, not only the first — a short-circuiting result cannot express a matrix.
2. **Given** an invariant that holds on every target, **When** the corpus lane runs, **Then** every cell reports pass and the run does not fail.
3. **Given** a target whose scan did not produce the documents an invariant needs, **When** the invariant is evaluated, **Then** the cell reports *not-applicable* with the reason, distinctly from pass.
4. **Given** an invariant whose checker panics on one target, **When** the corpus lane runs, **Then** that cell reports an error and the remaining cells are still evaluated.

---

### User Story 2 - A carve-out is a visible row, not a sentence (Priority: P1)

A maintainer scopes a fix to two ecosystems and defers the rest. Rather than recording that in a spec's Edge Cases, they declare the deferred cells in the matrix with a written reason. Anyone reading the corpus output sees the deferral; nobody has to rediscover it by inspecting a document.

**Why this priority**: This is the half that prevents recurrence. US1 makes the gaps visible once; US2 keeps them visible and makes silently acquiring a new gap impossible. Shipped alongside US1 so the 12 known-failing cells land declared rather than as a red lane.

**Independent Test**: Declare one failing cell as a known failure with a reason and confirm the lane passes; remove the reason and confirm the lane fails naming that cell.

**Acceptance Scenarios**:

1. **Given** a cell that fails and carries a declared reason, **When** the lane runs, **Then** the lane does not fail and the output lists the cell as a declared known failure.
2. **Given** a cell that fails with no declared reason, **When** the lane runs, **Then** the lane fails and names the invariant, the target, and what was observed.
3. **Given** a cell declared as a known failure that now passes, **When** the lane runs, **Then** the lane fails, telling the maintainer to remove the stale declaration — a fixed defect must not keep its exemption.
4. **Given** a catalogued invariant with no checker, **When** the test suite runs, **Then** it fails, mirroring `every_catalog_row_has_an_extractor` at `parity/extractors/mod.rs:922`.
5. **Given** a new corpus target added to the manifest with no declared state for an invariant, **When** the lane runs, **Then** the cell is evaluated and must pass or be declared — a new target cannot silently inherit an exemption.

---

### User Story 3 - Universal properties leave the per-target functions (Priority: P2)

A maintainer reading `layer1_assertions.rs` can tell which assertions are properties of any SBOM and which are expectations about one target. The former live in the invariant catalog; the latter stay where they are.

**Why this priority**: Delivers the clarity benefit and removes duplication, but US1 and US2 already provide the coverage benefit for newly written invariants. Migration is valuable cleanup rather than the mechanism, so it follows.

**Independent Test**: Pick one assertion currently duplicated across two or more per-target functions, move it to the catalog, and confirm both targets still fail or pass as before while the per-target functions shrink.

**Acceptance Scenarios**:

1. **Given** an assertion that appears in more than one per-target function, **When** it is migrated to the catalog, **Then** every target is evaluated against it and the originating targets' outcomes are unchanged.
2. **Given** a genuinely target-specific expectation (for example `go-cobra`'s offline `partial` verdict, or the nixpkgs moat checks), **When** migration is reviewed, **Then** it remains a per-target expectation and the spec's own guidance explains why.
3. **Given** a migrated assertion, **When** its `suggested_action` text is read, **Then** it is preserved — the diagnostic prose is the most valuable part of the existing layer and must survive the move.
4. **Given** any check in the harness, **When** a reviewer reads its name, **Then** they can tell whether it is an invariant or an expectation without reading its body (FR-020a).

---

### User Story 4 - The matrix states what it cannot see (Priority: P3)

A maintainer reading a green matrix learns which ecosystems it covers and which it does not, so a clean run is not mistaken for broad confidence.

**Why this priority**: Directly addresses how #1211 escaped — the corpus has **no gem target at all**, so no invariant over the existing 17 targets could ever have caught it. Valuable, cheap, and independent of the mechanism, so it ships last.

**Independent Test**: Run the lane and confirm the output names at least one ecosystem waybill supports that no corpus target exercises.

**Acceptance Scenarios**:

1. **Given** the corpus manifest, **When** the lane reports, **Then** it lists the ecosystems exercised by at least one target and those exercised by none.
2. **Given** an ecosystem with a reader but no corpus target, **When** the report is read, **Then** that ecosystem is named as uncovered rather than omitted.

---

### Edge Cases

- **An invariant passes because the behaviour it checks never fired.** The dominant risk, and the one this repo has been bitten by repeatedly (see `feedback_a_passing_test_may_exercise_nothing` and `feedback_zero_results_can_mean_broken`). A propagation invariant evaluated against a target with no dev dependencies passes vacuously. Each invariant must declare the precondition that makes it meaningful, and a cell whose precondition is unmet reports *not-applicable*, never pass.
- **A target's scan failed or emitted nothing.** Seven passes against absent documents is the worst possible output. A target with no emitted documents must report every cell as error or not-applicable and must fail the lane.
- **Two invariants overlap.** In the evidence above the gem and Go instances surfaced under I7 rather than I3, meaning I3 is under-specified. Overlapping invariants double-count a defect and can attribute it to the wrong class, sending a fix to the wrong place. Each catalogued invariant needs a statement precise enough that a given defect belongs to exactly one.
- **An invariant is too strict for a legitimate case.** Some documents are correctly `partial`; offline scans correctly resolve fewer edges. An invariant that cannot distinguish a legitimate limitation from a defect will be exempted everywhere and become decoration.
- **A declared known failure masks a *second*, different failure of the same invariant on the same target.** Resolved by pinning the violation kind (FR-010, FR-012): a second kind appearing under an existing declaration fails the lane, while the count moving does not. The residual risk is an invariant whose kinds are too coarse — if every violation collapses to one kind, the declaration absorbs everything, which is what FR-012a guards against.
- **The matrix grows faster than the corpus.** 17 targets × N invariants is a cell count that will dominate the output. The report must stay readable as N grows — failures and declarations first, passes summarised.

## Requirements *(mandatory)*

### Functional Requirements

**The invariant layer**

- **FR-001**: The harness MUST provide a catalog of semantic invariants, each a property expected to hold of any emitted SBOM, independent of which target produced it.
- **FR-001a**: Each invariant MUST carry a stable number allocated append-only. Numbers MUST NOT be reassigned when rows are added, removed or reordered, mirroring the C-row catalog in `docs/reference/sbom-format-mapping.md`.
- **FR-001b**: A restatement that changes what an invariant means MUST retire its number and allocate a new one. Editing a shipped row's meaning in place is forbidden, because every declaration referencing that number would silently transfer to a different property.
- **FR-001c**: Retired numbers MUST NOT be reused, and a declaration referencing a retired or unknown invariant MUST fail the lane rather than being ignored.
- **FR-002**: Every catalogued invariant MUST be evaluated against every corpus target in a single run.
- **FR-003**: Evaluation MUST report a state for every (invariant, target) cell, drawn from: pass, fail, not-applicable, declared known failure, or error.
- **FR-004**: A run MUST report all failing cells, not terminate at the first. The existing one-failure-per-target result shape cannot express a matrix and MUST be extended or replaced for this layer.
- **FR-005**: Each invariant MUST carry a diagnostic explaining the class of defect it catches and what to do when it fails, preserving the `suggested_action` convention already used by the per-target assertions.
- **FR-006**: Each invariant MUST declare the precondition under which it is meaningful, and a cell whose precondition is unmet MUST report not-applicable rather than pass.
- **FR-006a**: Every catalogued invariant MUST have its precondition met on at least one corpus target. An invariant that is not-applicable on every target MUST fail the lane: it is unfalsifiable against the corpus and its row of not-applicable cells is indistinguishable from a working check.
- **FR-007**: A checker that panics MUST produce an error cell without preventing evaluation of the remaining cells.

**The gate**

- **FR-008**: The test suite MUST fail if a catalogued invariant has no checker, mirroring the existing `every_catalog_row_has_an_extractor` gate.
- **FR-009**: A failing cell MUST fail the corpus lane unless it carries a declared known-failure entry.
- **FR-010**: A declared known-failure entry MUST include a written reason, the **kind** of violation being accepted, and the count observed at declaration time. The kind is the pinned part of the declaration; the count is recorded for context and for the report, not as a gate condition.
- **FR-011**: The lane MUST fail if a declared known-failure cell now passes, so stale declarations cannot accumulate.
- **FR-012**: The lane MUST fail if a declared known-failure cell fails with a violation kind other than the declared one, or with an additional kind beyond it. A change in the count alone MUST NOT fail the lane: counts drift for reasons unrelated to waybill — an upstream base-image bump or a corpus repin — and a gate that reddens on innocent drift is the gate that gets switched off.
- **FR-012a**: Each invariant MUST classify its violations into kinds stable enough to pin, and a checker that can only report a count cannot satisfy FR-012 and MUST NOT be catalogued.
- **FR-013**: Adding a corpus target MUST NOT implicitly exempt it from any invariant; every cell for a new target MUST pass or be explicitly declared.

**Reporting**

- **FR-014**: The run MUST emit the full matrix in a form readable at a glance, with failures and declarations surfaced ahead of passes.
- **FR-014a**: The run MUST write the complete matrix — every cell's state and observation — as a per-run artifact that is not committed to the repository, so a maintainer can read a fix's per-ecosystem reach from one run without inspecting emitted documents.
- **FR-014b**: The committed source of truth MUST be the known-failure declarations alone, recording only non-passing cells. Passing cells MUST remain implicit, so the committed file stays proportional to the number of outstanding defects rather than to the cell count, and adding a corpus target does not rewrite unrelated entries.
- **FR-014c**: Declaration entries MUST be shaped so that an unrelated change does not move them, mirroring the line-stability requirement that milestone 117 applied to `walk.audit-allowlist.txt` after churn made the preceding shape unusable.
- **FR-015**: The report MUST name the ecosystems exercised by at least one corpus target and those exercised by none, where the universe of ecosystems is the coverage matrix in `docs/ecosystems.md`. The universe MUST be derived from that document rather than hand-listed in the harness, so it cannot drift from it silently.
- **FR-015a**: The report MUST publish, per invariant, the number of targets on which its precondition was met, so an invariant that satisfies FR-006a only marginally is visible as such rather than reading as broad coverage.
- **FR-016**: The invariant layer MUST consume documents already emitted by the existing harness and MUST NOT trigger additional scans, network access, or container pulls.

**Scope of the initial catalog**

- **FR-017**: The initial catalog MUST include the seven invariants demonstrated in the evidence above: no dependency edge targets a file-tier component; `(name, version, type)` identifies at most one component; a component reachable only via non-runtime paths carries a non-runtime marker; the formats agree on the root's direct dependency set; a `complete` verdict implies nothing was excluded from the judgement; a package does not appear under several unknown-version spellings; every component is reachable from the subject or carries a marker explaining why not.
- **FR-018**: Every failing cell in the first full-corpus run MUST be either fixed or declared with a written reason referencing the issue that tracks it. The twelve failures in the evidence above were measured against nine Pico pairs, not the seventeen corpus targets; eight targets are unmeasured, so the delivered count is established by that run and is not predicted here.
- **FR-019**: I3 and I7 MUST be restated so that a given defect satisfies the precondition of exactly one of them, resolving the overlap observed in the evidence. Because neither has shipped, the restatement happens at authoring time and the numbers in FR-017 are provisional until the catalog lands; FR-001b's retire-and-reallocate rule governs every change after that.

**Terminology**

- **FR-020**: "Invariant" MUST refer only to a property expected of any emitted SBOM — the universal structural checks and the new catalog. A check that asserts something about one specific target MUST be called an **expectation**, and the per-target call sites MUST be renamed accordingly.
- **FR-020a**: The two terms MUST be distinguishable by name alone, so the US3 sorting decision — property of any SBOM, or expectation about this target — can be made without reading each check's body.

### Key Entities

- **Invariant**: a property of an emitted SBOM, identified by a stable append-only number, with a statement, a precondition for meaningfulness, a diagnostic, and a checker. Its number is its durable identity: refining the statement keeps it, changing its meaning retires it.
- **Invariant Catalog**: the enumerated set of invariants, including retired numbers, which remain listed so they are never reallocated; the source of truth the no-checker gate reads.
- **Corpus Target**: an existing entry in the corpus manifest, unchanged by this feature except that it no longer carries universal assertions.
- **Cell**: the evaluation of one invariant against one target, holding a state and an observation.
- **Known-Failure Declaration**: a recorded acceptance of a failing cell, carrying a reason, the pinned violation kind, the count at declaration time, and optionally the tracking issue. The set of these is the committed source of truth and the only thing the gate compares against; it names non-passing cells only.
- **Violation Kind**: the classification of *how* an invariant was violated, as distinct from how much. It is the part of a declaration the gate pins, chosen to be stable against upstream drift.
- **Matrix Report**: the per-run output, written as an uncommitted artifact and summarised to the console: every cell's state and observation, the failures, the declarations, and the ecosystem coverage statement. It is the artifact a maintainer reads to establish a fix's reach, and it is regenerated per run rather than versioned.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A newly discovered defect class, written once as an invariant, reports its state for all 17 corpus targets in one run. Today establishing the same coverage requires reading 17 documents by hand.
- **SC-002**: Zero (invariant, target) cells have an undeclared state: every cell is pass, not-applicable, declared, or fails the lane.
- **SC-003**: The per-ecosystem reach of a change is readable from a diff of two runs' matrix artifacts, without inspecting individual documents. Demonstrated within this feature by the mutation check (SC-004): the cells that move are exactly the ecosystems the mutation reached. The same diff, run across a real fix, is how the first of #1206-#1211 reports its own blast radius — but that fix is out of scope here (Assumption 1), so the criterion is verified against the mutation, not against it.
- **SC-004**: Deliberately violating an invariant on a target that passes it turns that cell red. Verified for every invariant in the initial catalog — an invariant that cannot be made to fail is not evidence of anything.
- **SC-005**: Every ecosystem in `docs/ecosystems.md`'s coverage matrix that no corpus target exercises is named in the run's output. At the time of writing that set is non-empty and includes `gem`, which is why #1211 was unobservable from this corpus.
- **SC-006**: All seven initial invariants are implemented, and every failing cell in the first full-corpus run is fixed or declared with a written reason — no cell left undeclared.
- **SC-007**: A cell moving from pass to fail without a declaration fails the corpus lane. Verified by introducing such a regression deliberately.
- **SC-008**: The invariant layer adds no scans, no network requests and no container pulls to a corpus run; it reads documents the run already produced.
- **SC-009**: No invariant in the initial catalog reports pass on a target whose precondition for that invariant is unmet — each such cell reports not-applicable instead.
- **SC-010**: Every invariant in the catalog applies to at least one corpus target, and the report states that count for each. Verified by adding an invariant whose precondition no target meets and confirming the lane fails.

## Assumptions

- **Fixes are out of scope.** This feature makes defects visible and keeps them visible. The twelve failing cells are fixed under their own issues (#1206, #1207, #1208, #1209, #1211); here they may land declared. Shipping a visibility mechanism and the fixes together would make it impossible to tell which one caused a golden to move.
- **The declared-known-failure model follows the existing allowlist precedent** in this repo (`waybill-cli/src/scan_fs/walk.audit-allowlist.txt`, milestones 115 and 117), and the gate fires on undeclared change in either direction. No longer an assumption: settled in the 2026-10-15 clarification session and stated as FR-014b and FR-014c.
- **The lane's gating is unchanged.** The corpus harness needs network and containers and runs in its own dispatched lane, not in `scripts/pre-pr.sh`. The invariant layer inherits exactly that gating; it does not become part of the mandatory pre-PR gate, whose two commands stay as `CLAUDE.md` specifies.
- **The 17 existing targets are the corpus.** Adding targets to cover uncovered ecosystems — gem among them — is separate work that this feature's reporting is designed to motivate (FR-015), not perform.
- **The catalog lives alongside the existing parity catalog convention** in `docs/reference/`, so the two gates are discoverable together.
- **Layer 0 stays as it is.** Structural integrity checks already run universally and are not re-homed; the new layer sits beside them and asks semantic rather than structural questions.
- **Nine targets were used for the evidence, seventeen exist.** The matrix above was produced against the nine Pico nightly SBOM pairs because they were already emitted and available. The feature targets the 17-entry public corpus; cell counts in the evidence will not match the delivered matrix, and the seven invariants' states on the eight unseen targets are unmeasured.
- **The coverage statement is as complete as `docs/ecosystems.md` is.** That document's coverage matrix carries 17 rows while roughly 40 reader modules exist under `waybill-cli/src/scan_fs/package_db/`. The report therefore names uncovered ecosystems *among those the matrix lists*, not among all readers. Closing that gap is a docs change in `docs/ecosystems.md`, not harness work, and FR-015's derive-don't-hand-list rule is what makes the report follow it the day it lands.

## Dependencies

- The existing corpus harness (`waybill-cli/tests/corpus_harness_195/`) and its emitted-document plumbing, unchanged.
- The existing `AssertionFailure` / `suggested_action` diagnostic convention in `harness.rs:35`.
- The `every_catalog_row_has_an_extractor` gate at `parity/extractors/mod.rs:922` as the pattern for FR-008.
