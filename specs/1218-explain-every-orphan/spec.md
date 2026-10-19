# Feature Specification: Every unreachable component says why it has no edge

**Feature Branch**: `1218-explain-every-orphan`
**Created**: 2026-10-19
**Status**: Draft — ready for `/speckit.plan`
**Input**: User description: "Extend the I7 orphan-explanation vocabulary beyond Go and npm so every unreachable component in every ecosystem says why it has no edge (#1214)"
**Issue**: #1214 (Refs #1154, #1212). Splits out #1234 and #1235.

## Why this exists

Invariant **I7** says *a component not reachable from the subject by any
dependency edge carries a marker explaining why*. Thirty-four components
across five of the seventeen corpus targets fail it, and they fail because
for anything outside Go and npm there is no code to assign:

```rust
// waybill-cli/src/generate/orphan_reason.rs:161 and :176
if ecosystem != "npm" && ecosystem != "golang" {
    continue;
}
```

All five codes in milestone 167's vocabulary are Go- or npm-specific in
meaning. For a `pkg:maven/*`, `pkg:pypi/*`, `pkg:bazel/*`, `pkg:github/*`
or `pkg:generic/*` component the loop never looks, and if it did there is
nothing to say.

**What a consumer cannot do today.** Given an edgeless component, a reader
cannot tell whether it is part of the artifact and waybill lost the edge,
or an entry a reader should not have attached to this subject at all. Those
readings have opposite consequences for anyone acting on the document.

### The populations were measured, and the measurement halved the feature

#1214's population table was drawn per target. Re-measured on `main` at
`1311224e` the **counts are identical** — 1, 2, 3, 5, 23 = 34. The counts
were never the problem. Grouping offenders by *mechanism* instead produces
six populations, and **only eighteen of the thirty-four are a vocabulary
gap**:

| # | Mechanism | n | Target | Disposition |
|---|---|---|---|---|
| **B** | Alternate resolution in the subject's own lockfile — a real entry the chosen resolution does not use | 10 | python-flask | **US1** |
| **C** | A foreign build system's root manifest, co-located with the subject, in another ecosystem | 3 | maven-guice (2), go-opentelemetry (1) | **US2** |
| **D** | A lockfile node nothing else references | 3 | haskell-security-advisories | **US2** |
| **E** | A toolchain pin — build tooling, not a dependency | 2 | haskell-language-server | **US3** |
| **A1** | `requirements.txt`-sourced components are emitted with **zero out-edges**, so a declaring module's transitive closure dangles | 13 | python-flask | **deferred — #1234** |
| **A2** | The maven dual-key insert emits **both** the declared and the workspace-resolved coordinate and wires only the resolved one | 3 | maven-guice | **deferred — #1235** |

10 + 3 + 3 + 2 + 13 + 3 = 34. This feature explains **15**; nineteen remain
reported, in three targets, by design.

**Mechanism D ships but moves no cell, and that was discovered at implement
time.** Its `unreferenced-lock-node` value is correct and unit-tested, and
it cannot reach its own population: `nix::attach_inputs_to_projects` adds
main-module-to-root-input *relationships*, so the emit-time classifier's BFS
sees those three flake inputs as **reachable** and skips them, while the
emitted `dependencies[]` carries no such edge and invariant I7 — which reads
the document — sees them as **unreachable**. The classifier and the document
disagree, which is a waybill defect rather than a missing word, so the cell
keeps reporting it (**#1237**) and the value is accepted forward-lookingly,
the posture C197 already established in this harness.

**Every offender is `waybill:sbom-tier = source` except mechanism E's two,
which are `design`.** This is load-bearing. C151
`waybill:unresolved-reason` is emitted *"iff and only if the same component
carries `waybill:sbom-tier: design`"*, so it cannot legally attach to
mechanisms B, C or D however the readers are extended — which means FR-015's
extension of C151 to `bazel.rs` and `nix/lockfile.rs` closes **no** I7 cell
and is consistency work, not a fix.

**But `design` tier is not a synonym for mechanism E.** The corpus holds
**six** unreachable `design`-tier components, not two: mechanism E's two
toolchain pins, plus four documentation dependencies in the same target
(`docutils`, `myst-parser`, `sphinx`, `sphinx-rtd-theme`, from
`docs/requirements.txt`, `workspace-member = ["docs"]`) which are already
explained by C151 and are therefore not offenders. So C151 is not merely
"available" for `design`-tier components — it is already in use on four of
them, doing a different job, two components away from the ones this feature
marks. That makes FR-012's refusal to reuse C151 sharper, and FR-012's
three-part predicate mandatory rather than merely tidy.

**A1 and A2 are waybill's own defects, not properties of the artifact.** A
marker saying "no edge, because nothing wired one" describes waybill, and
emitting it would close the I7 cell that is currently reporting a real bug.
The four mechanisms this feature does cover are the opposite case: no source
of truth states an edge for them, so a marker is the truthful answer rather
than the cheap one.

The full evidence is in
[`measurements/i7-populations.md`](./measurements/i7-populations.md),
reproducible offline with the committed probe. Two corrections to #1214
follow from it, neither cosmetic:

1. **`maven-guice` is 2 bazel + 3 maven**, not "five `pkg:bazel/*` read
   from WORKSPACE". A fix written from the issue would have changed the
   bazel reader for three components it never emitted.
2. **`python-flask` is 10 + 13**, not 23 uv.lock forks. Thirteen come from
   `examples/celery/requirements.txt` and are #1234.

A third, minor: `go-opentelemetry`'s `codespell` is sourced from a
`requirements.txt` at the **repository root** (`workspace-member = ["."]`),
not "a tooling directory".

### The second root cause

`waybill:unresolved-reason` (C151, milestone 236) is the other marker that
explains a design-tier component. Its inventory is seventeen reader files
(`specs/236-unresolved-reason/spec.md:185`); `bazel.rs`, `cmake.rs` and
`nix/lockfile.rs` are not among them, nor among the four readers Q2
explicitly trimmed. They were never enumerated — bazel and cmake predate
m236, the nix readers postdate it.

Measured, only **two of the three are exercised by the corpus**. All
seventeen goldens contain exactly two components carrying a
`waybill:source-mechanism`, both `bazel-http-archive`, and nothing sourced
from cmake. Extending the marker to `cmake.rs` is therefore a change no
lane can verify, and this feature does not make it (see Out of Scope).

## Clarifications

### Session 2026-10-19

- Q: Does population A1 (13 components) get the edges the lockfile states,
  or a marker explaining the absence? → A: Neither here. A1 is a
  missing-edge defect and is deferred to **#1234**, which keeps the I7 cell
  reporting it. Its `python-flask` declaration survives with a corrected
  reason.
- Q: Does population A2 (3 components) belong to this feature or to #1216's
  divergent-PURL family? → A: To that family, as **#1235**. Its
  `maven-guice` declaration survives with a corrected reason, following the
  precedent of commit `a3691e56` for I8.
- Q: How does the new vocabulary relate to C45's existing five codes? → A:
  It extends C45's open enum rather than introducing a new annotation.
  Where an existing code's name is already ecosystem-neutral and accurate it
  is generalized rather than duplicated: measurement shows **all ten** of
  mechanism B have a reachable same-name sibling, which is exactly what
  `dead-lockfile-entry` names, so B reuses it. Mechanisms C, D and E get new
  C45 values. Consequence: no new catalog row, no new parity extractor, and
  I7's `is_explained` arm count is **unchanged**, because
  `waybill:orphan-reason` is already an arm (FR-004, FR-005, SC-014).
- Q: How do mechanism E's two toolchain pins get explained, given they are
  the only `design`-tier population and so the only one where C151 is
  available? → A: A new C45 `orphan-reason` value meaning *build toolchain,
  not a dependency*. **Not** C151: that annotation asserts resolution was
  attempted and failed, which is false of a pin like `ghc-9.6.5` that is
  exact rather than unresolved — the distinction C197's row was written to
  protect. **Not** a new tier value either: that would need a sixth
  `is_explained` arm against SC-014, and widening a tier taxonomy belongs in
  its own change (FR-012).
- Q: Must mechanism C's marker itself name the foreign build system? → A:
  No. One C45 code, and the **pip reader emits the existing C55
  `waybill:source-mechanism`** so identification is uniform and
  machine-readable across all three of C's components. Measured, the two
  bazel components already carry `source-mechanism = bazel-http-archive`
  while `codespell` carries only `source-files = ["requirements.txt"]`, a
  filename a consumer would have to parse; the pip reader emits C55
  nowhere. Encoding the build system into the orphan-reason enum instead
  would grow that enum once per foreign build system forever and conflate
  *why there is no edge* with *what tool declared it* (FR-002, FR-002a).
- Q: Does deferring A1 imply waybill should avoid edges to dependencies
  that may not reach the built artifact? → A: No, and the opposite. An edge
  asserts *X declares Y*, which a lockfile states regardless of pruning;
  whether Y ships is a separate fact already modelled on Y by
  `waybill:build-inclusion` (`not-needed` / `unknown`, m112, with a native
  CDX `scope: "excluded"` slot) and `waybill:lifecycle-scope`
  (`development` / `optional` / `test` / `build`). Withholding an edge to
  avoid overclaiming loses the declaration entirely and leaves a consumer
  unable to tell "not declared" from "declared, may not ship" — the same
  two-readings failure this feature exists to fix, one level down. Recorded
  in #1234 as a constraint on its fix; this feature adds no edges at all
  (FR-009).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The unused half of a duplicate pair says which it is (Priority: P1)

A consumer finds the same package name twice in one document at different
versions. The copy the subject's resolution does not use now says so, and
the consumer can act on the one that is actually present.

**Why this priority**: 10 offenders — the largest of the four mechanisms
this feature covers — and the only one where the ambiguity is actively
dangerous rather than merely unexplained. A vulnerability decision taken
against the wrong version of a duplicated name is wrong in a way the
document invited.

**Independent Test**: Run the hermetic invariant lane and read
`python-flask`'s per-offender report: the count falls from 23 to 13, and
every remaining offender is a `examples/celery/requirements.txt` component
(#1234). No cell flips to `Pass` on this story alone, because #1234's
thirteen share the document — the measurable outcome is the residual count
and its composition, not a verdict.

**Acceptance Scenarios**:

1. **Given** `python-flask`'s golden, **When** I7 is evaluated, **Then**
   `pkg:pypi/alabaster@0.7.16` carries a marker meaning *present in the
   lockfile, not in this resolution*, and `alabaster@1.0.0`, which is
   reachable, carries no such marker.
2. **Given** the same document, **When** a consumer groups components by
   name, **Then** for each of the nine names appearing more than once, every
   copy sourced from `uv.lock` is either reachable or marked.
3. **Given** a lockfile with exactly one resolution, **When** it is
   scanned, **Then** no component receives this marker.
4. **Given** `click@8.1.3`, which is declared by `examples/celery` and *is*
   reachable, **Then** it receives no marker and its edges are unchanged.

---

### User Story 2 - Foreign manifests and dangling lockfile nodes say so (Priority: P2)

A consumer finds components that belong to a build system the subject does
not use, or lockfile entries that no other entry references. Each now says
which of the two it is, and the two are distinguishable from one another.

**Why this priority**: 6 offenders, but it is the story that flips cells —
`haskell-security-advisories` and `go-opentelemetry` reach `Pass` on this
story alone, and `maven-guice` falls from 5 to 3. Second rather than first
because the consumer hazard is lower: nobody mistakes a bazel
`http_archive` in a Maven project for a shipped dependency.

**Independent Test**: Delete the `haskell-security-advisories` and
`go-opentelemetry` I7 declarations and run the hermetic lane; both cells
report `Pass`, with `python-flask` unchanged by this story.

**Acceptance Scenarios**:

1. **Given** `maven-guice`'s golden, **Then** the two `pkg:bazel/*`
   components are explained as belonging to a build system the subject does
   not use — distinctly from the three maven components in the same
   document, which this feature does not touch (#1235).
2. **Given** `go-opentelemetry`'s golden, **Then**
   `pkg:pypi/codespell@2.4.3` is explained as a root-level manifest of an
   ecosystem foreign to the subject, and the marker names the manifest.
3. **Given** `haskell-security-advisories`'s golden, **Then** the three
   `pkg:github/*` flake inputs are explained, and the explanation says no
   node references the input rather than that it is foreign.
4. **Given** mechanisms C and D in one document, **Then** their markers
   differ, because "a build system you do not use" and "a lockfile entry
   nothing references" call for different consumer actions.

---

### User Story 3 - A toolchain pin says it is build tooling (Priority: P3)

A consumer finds a compiler or snapshot pin with no edge and can tell it is
build tooling rather than a dependency of the subject.

**Why this priority**: 2 offenders, the smallest population, and it flips
`haskell-language-server` to `Pass`. Last because it is the one mechanism
that may need no new vocabulary at all — an existing build-tooling tier may
already cover it (FR-012), making it the cheapest and most contingent slice.

**Independent Test**: Delete the `haskell-language-server` I7 declaration
and run the hermetic lane; the cell reports `Pass`.

**Acceptance Scenarios**:

1. **Given** `haskell-language-server`'s golden, **Then** both
   `pkg:generic/ghc-9.6.5@unspecified` and
   `pkg:generic/stackage-lts-23.28@unspecified` are explained as build
   tooling rather than as dependencies of the subject.
2. **Given** a component with `waybill:source-files = [""]` and no
   `waybill:workspace-member`, **Then** the absence of those fields is
   treated as its own case and not as `"."`.

---

### Edge Cases

- **A component matching two mechanisms.** A sibling member's lockfile
  could also contain a resolution fork. The marker set must be ordered so
  exactly one applies, and the order stated rather than emergent (FR-007).
- **A component already explained.** One carrying
  `waybill:unresolved-reason`, `waybill:closure-role`,
  `waybill:component-tier = file` or
  `waybill:version-absence = by-construction` must not receive a second,
  possibly contradictory marker (FR-008).
- **A reachable component matching a marker shape.** Measured:
  `click@8.1.3` in `python-flask` is declared by `examples/celery` and is
  reachable, while two `uv.lock` `click` forks are not. Markers describe
  provenance, not reachability, so emitting on a reachable component is not
  wrong — but it must not change that component's edges (FR-009).
- **A deferred population acquiring a marker by accident.** #1234's
  thirteen and #1235's three must remain unexplained. A mechanism predicate
  written loosely enough to catch them would silently close the cells that
  report those defects (FR-011, SC-004).
- **A document with no subject**, or whose only component is the subject.
  I7 already returns `NotApplicable` for both; neither may change (FR-010).
- **A fixed cell keeping its exemption.** The lane fails on a stale
  declaration, so a declaration left behind after its defect is fixed
  breaks the lane — by design (FR-013).
- **An ecosystem outside the corpus.** Seventeen targets do not cover every
  reader; a component in an unexercised ecosystem must still reach a
  defined outcome even if no lane observes it (FR-011).

## Requirements *(mandatory)*

### Functional Requirements

**The vocabulary**

- **FR-001**: The vocabulary MUST cover mechanisms B, C, D and E as values
  of the existing `waybill:orphan-reason` annotation (catalog row C45), not
  as a new annotation name. A consumer MUST be able to tell the four apart
  from the document alone.
- **FR-001a**: Mechanism B MUST reuse the existing `dead-lockfile-entry`
  code, whose name already describes it — a lockfile entry at a version the
  resolution does not use. Measurement supports this without exception: all
  ten of B's components have a reachable same-name sibling, so B is
  uniformly the multi-version case and contains no no-sibling population
  that would need a second code.
- **FR-001b**: C45's row MUST be updated to delete the sentence
  *"Emission scope is restricted to `pkg:golang/*` and `pkg:npm/*`
  ecosystems (FR-001); other ecosystems continue to carry no
  orphan-reason."* That sentence is what this feature falsifies, and a row
  that still asserts it would contradict the emission it documents.
- **FR-002**: Mechanism C MUST use a single C45 code. The code MUST NOT
  encode which build system declared the component; that would grow the
  enum once per build system and conflate *why there is no edge* with
  *what declared it*.
- **FR-002a**: The pip reader MUST emit the existing C55
  `waybill:source-mechanism` on the components it reads from a
  `requirements.txt`, so every mechanism-C component identifies its
  declaring build system machine-readably. Measured, the bazel pair already
  carries `bazel-http-archive` and the pip component carries nothing, so a
  consumer must today parse the string `"requirements.txt"` to learn it is
  pip. C55 already has a catalog row and an extractor, so this adds neither.
- **FR-002b**: FR-002a is a **reader-level** change and MUST stay uniform —
  a reader that annotates some of its own output and not the rest is worse
  than one that annotates none. Its measured radius is therefore wider than
  mechanism C: **41 components across 5 targets** carry a
  `requirements.txt` source today (`python-flask` 21,
  `pants-example-django` 13, `haskell-language-server` 4,
  `pants-example-python` 2, `go-opentelemetry` 1), and mechanism C needs the
  annotation on **one** of them. Three of those targets are outside this
  feature's scope table and two currently report I7 `Pass`. The golden diff
  MUST be expected to contain **two** kinds of added property —
  `waybill:orphan-reason` on 18 components and `waybill:source-mechanism` on
  41 — and a verification step that expects only the first will report a
  false alarm.
- **FR-003**: The ecosystem restriction at `orphan_reason.rs:161` and
  `:176` MUST either be removed, or be replaced by a condition that states
  what it excludes and why. A guard justified only by the vocabulary
  happening to be Go/npm MUST NOT remain.
- **FR-004**: Every component carrying a milestone-167 code today MUST
  still carry the same code after this feature. A code's *domain* may widen
  — `dead-lockfile-entry` gains the pypi multi-version case under FR-001a —
  but no component's assignment may change, and no code may be renamed or
  removed. Widening a domain un-assigns nobody; this is the narrowing of
  the original "keep its meaning" that Option A requires, and it is what
  SC-016 verifies.
- **FR-005**: The new values MUST be emitted in all three formats through
  C45's existing row and parity extractor. No new catalog row and no new
  extractor may be added, because a new row for a value of an existing
  annotation would make `every_catalog_row_has_an_extractor` pass while the
  formats disagree about which row owns the annotation. C45's row text MUST
  be updated to enumerate the new values alongside the existing five, and to
  state the total order of FR-007 where it already states m167's.

**Assignment rules**

- **FR-006**: Mechanism membership MUST be decided from data already in the
  component and the document, with no new filesystem read, subprocess or
  network call.
- **FR-007**: Where a component matches more than one mechanism, the marker
  assigned MUST be determined by a stated total order documented beside the
  vocabulary. Two components with identical provenance MUST never receive
  different markers.
- **FR-008**: A component already carrying one of I7's four other
  explaining markers MUST NOT receive a new one from this feature.
- **FR-009**: Marker assignment MUST NOT create, delete or redirect any
  dependency edge, and MUST NOT change any component's reachability. This
  feature adds no edges; #1234 is where the missing ones are added.
- **FR-010**: I7's two `NotApplicable` verdicts MUST be unchanged.
- **FR-011**: A component that is unreachable and matches no mechanism MUST
  remain unexplained and MUST still fail I7. No catch-all marker may be
  introduced: a marker that explains everything distinguishes nothing and
  would make I7 unfalsifiable. In particular the thirteen components of
  #1234 and the three of #1235 MUST still fail I7 after this feature.
- **FR-012**: Mechanism E MUST be explained by a new C45 value meaning
  *build toolchain, not a dependency of the subject*. Its predicate MUST
  require **all three** of: `design` tier, an empty `source_file_paths`,
  and an absent `waybill:workspace-member`. The tier alone is **not**
  sufficient, and this is not a theoretical tightening —
  `haskell-language-server` carries four further unreachable `design`-tier
  components (`docutils`, `myst-parser`, `sphinx`, `sphinx-rtd-theme`, all
  from `docs/requirements.txt` with `workspace-member = ["docs"]`) which are
  documentation dependencies of a sibling member, not toolchain pins.
  Calling them build tooling would be false, and because they are already
  explained by C151 they are **not** I7 offenders — so the probe's count
  would not move and the error would be invisible to SC-001 through SC-004.
  FR-008 is the only other thing standing in the way, which is one guard too
  few for an assertion this wrong. It MUST NOT reuse
  C151 `waybill:unresolved-reason`, even though E is the only population
  whose `design` tier makes C151 available: C151 asserts resolution was
  attempted and did not find a version, and `pkg:generic/ghc-9.6.5` is
  pinned exactly rather than unresolved. C197's row draws that distinction
  explicitly ("resolution was attempted and did not find a version" versus
  "there was never one to find"), and using C151 here would make it mean
  both. It MUST NOT introduce a new tier or `component-tier` value either,
  which would require a sixth `is_explained` arm against SC-014.

**Observability**

- **FR-012a**: Each new value MUST appear as its own counter in the
  classifier's existing per-scan INFO log. **Corrected at implement time**:
  an earlier draft of this requirement asserted the log did not exist and
  that an operator could not distinguish a scan where nothing matched from
  one where the classifier never ran. That was wrong — `tracing::info!` at
  `orphan_reason.rs:320-327` already fires unconditionally with all five
  counters and the comment *"Zero counters indicate a healthy scan"*, so
  zero-matched is already distinguishable from not-run. The discarded
  `let _m167_orphan_reason_counts` binding at `scan_cmd.rs:4939` is
  genuinely unused and harmless. What remains is real but small: the three
  new counters must be added to that log, or the new codes fire invisibly.
  The module header at `orphan_reason.rs:18` MUST also be corrected — it
  says the log "fires from the CALL SITE (not this module)", which is false
  and is what produced the error.

**The record**

- **FR-013**: The three `I7` entries this feature fully fixes —
  `go-opentelemetry`, `haskell-language-server`,
  `haskell-security-advisories` — MUST be deleted in the same change.
- **FR-014**: The two surviving `I7` entries MUST have their counts
  corrected to the residual (`python-flask` 23 → 13, `maven-guice` 5 → 3)
  and their reasons rewritten to name #1234 and #1235 respectively, instead
  of #1214. This follows commit `a3691e56`, which re-pointed the I8
  declaration at the defect it actually had.
- **FR-015**: `waybill:unresolved-reason` MUST be extended to `bazel.rs`
  and `nix/lockfile.rs`, and milestone 236's reader inventory at
  `specs/236-unresolved-reason/spec.md:185` MUST be updated to include
  them, so the next reader added does not repeat the omission. This closes
  **no** I7 cell: C151 attaches only to `design`-tier components and every
  I7 offender from those two readers is `source`-tier. It MUST NOT be
  counted toward SC-001, and a plan that treats it as part of the fix has
  misread it.
- **FR-016**: The feature MUST record that `cmake.rs` is the third reader
  m236 never enumerated and is deliberately unchanged, with the measured
  reason: the corpus contains no cmake-sourced component, so the change
  would be unverifiable.

**Falsifiability**

- **FR-017**: A positive control MUST exist: a scan whose only unexplained
  orphans are in a non-Go/npm ecosystem, asserted distinguishable from a
  scan with none. A test that only checks for the absence of offenders
  proves nothing, because a document with no components also has none.
- **FR-018**: Each new marker MUST have a mutation test showing the I7 cell
  or residual count it affects moves when that marker is withheld.
- **FR-019**: The committed probe MUST be re-run as the feature's closing
  step and its output recorded, so the spec's counts are reproducible
  rather than quoted.

### Key Entities

- **Orphan explanation marker**: a value of `waybill:orphan-reason`
  (catalog row C45) naming why a component has no edge to the subject.
  C45 is an open enum by declaration; this feature adds values to it and
  reuses `dead-lockfile-entry` for mechanism B.
- **Mechanism**: a predicate over a component's provenance annotations and
  the document's subject, identifying one measured population. Mechanisms
  are totally ordered so assignment is deterministic.
- **I7 declaration**: an entry recording a cell known to fail, with a count
  at declaration time. Five exist; this feature deletes three and corrects
  two.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The corpus-wide count of unreachable-and-unexplained
  components falls from **34** to **19** — thirteen from #1234, three from
  #1235, three from #1237, and nothing else. *(Measured on the corpus run at
  `af246692`. An earlier draft said 16, before #1237 was found: mechanism D's
  three flake inputs cannot be reached by any vocabulary because the
  classifier's BFS and the emitted document disagree about whether they are
  reachable at all.)*
- **SC-002**: I7 reports `Pass` on **14** of 17 targets, up from 12. The
  three that still fail are `python-flask` (#1234), `maven-guice` (#1235) and
  `haskell-security-advisories` (#1237).
- **SC-003**: `go-opentelemetry` and `haskell-language-server` report
  `Pass`; `python-flask` falls 23 → 13 and `maven-guice` 5 → 3.
  `haskell-security-advisories` stays at 3, blocked by #1237.
- **SC-004**: Every one of #1234's thirteen, #1235's three and #1237's three
  still fails I7, verified by name. A feature that accidentally explained any
  of them would show as a better SC-001 number and a worse outcome.
- **SC-005**: In `python-flask`, for each of the **nine** names appearing
  more than once, every copy sourced from `uv.lock` is either reachable or
  marked. Measured today: nine names, ten unexplained `uv.lock` copies.
- **SC-006**: `invariant-declarations.toml` contains exactly **three** `I7`
  entries, citing #1234, #1235 and #1237, with counts 13, 3 and 3.
- **SC-007**: The catalog gains **zero** new rows and the extractor set
  gains **zero** new entries; C45's row text is updated in place, and the
  parity suite plus `every_catalog_row_has_an_extractor` pass. Every new
  value has a mutation test.
- **SC-008**: The positive control of FR-017 fails when the feature is
  reverted and passes with it, demonstrated by running it both ways.
- **SC-009**: Component counts, edge counts and the reachable set are
  **identical** in all seventeen goldens before and after, for every target
  and every format. This feature changes annotations only.
- **SC-010**: No new filesystem read, subprocess or network call appears on
  the emission path, verified by the harness's existing no-new-I/O check.
- **SC-011**: `waybill:unresolved-reason` appears in `bazel.rs` and
  `nix/lockfile.rs`. Measured today: zero occurrences in each.
- **SC-012**: The hermetic lane passes under plain `cargo test` with no
  network and no privileges.
- **SC-013**: The pre-PR gate is green — `scripts/gate.sh` reports
  `GATE_RESULT=pass` with every suite at `0 failed`.
- **SC-014**: I7's `is_explained` arm count is **unchanged at five** — counted inside the function, not file-wide, since the test module calls `cdx_prop` too and an unscoped count reads 9 — and
  no existing arm is removed or altered. Every new value flows through the
  arm that already tests `waybill:orphan-reason`, which is the point of
  extending C45 rather than adding an annotation. An implementation that
  adds a sixth arm has added a new annotation and missed FR-001.
- **SC-015**: Re-running `measurements/i7-populations.py` after the change
  reproduces SC-001 through SC-003, and its output is pasted into the pull
  request.
- **SC-016**: Every component carrying a milestone-167 code before the
  change carries the same code after it, verified by diffing the
  `waybill:orphan-reason` value per component ref across all seventeen
  goldens. Only additions are permitted, never a changed or removed value.
- **SC-017**: Every mechanism-C component carries `waybill:source-mechanism`
  (C55). Measured today: 2 of 3 — the bazel pair has it, `codespell` does
  not, and the pip reader emits it in no file.
- **SC-018**: The four unreachable `design`-tier components in
  `haskell-language-server` sourced from `docs/requirements.txt` carry
  **no** value this feature adds, and keep their existing
  `waybill:unresolved-reason`. Verified by name, because they are not I7
  offenders and so no count in SC-001 through SC-004 would move if this
  went wrong.

## Assumptions

- **The goldens are the measurement surface.** Every count here was taken
  from the seventeen committed CDX goldens at `main` `1311224e`,
  reproducing the harness's own I7 predicate offline. The corpus lane is
  network-gated; the goldens are what it compares against, so a conclusion
  drawn from them is the conclusion the lane would reach.
- **CDX is representative of the three formats.** I7 is evaluated against
  the CDX document. The parity suite is what makes a CDX-measured claim
  true of SPDX 2.3 and SPDX 3, which is why FR-005 requires a row and an
  extractor rather than trusting emission.
- **`waybill:workspace-member` and `waybill:source-files` are
  trustworthy.** All 34 offenders were classified from them alone; the
  feature reads them rather than re-deriving provenance.
- **`waybill:version-absence = by-construction` stays non-load-bearing for
  I7.** Its doc comment says it moves no cell today, and measurement
  confirms no offender carries it. Feature 1217 governs that marker's
  reach; this feature does not touch it.
- **The counts are stable, not a snapshot.** They were identical at
  `dce37304` when #1214 was filed and at `1311224e` ten days and several
  merges later. A golden refresh could move them, which is why FR-019
  requires re-running the probe rather than trusting these numbers.
- **#1234 and #1235 land later or never.** This feature is correct either
  way: its markers do not apply to those populations, and the two surviving
  declarations are the record that they are open.
- **This feature and #1234 both touch the pip reader.** FR-002a adds C55
  emission there; #1234 adds the missing `requirements.txt` edges. They are
  separable — one is an annotation on components already emitted, the other
  is edges between them — but whichever lands second should expect a
  conflict in that file, and #1234's golden churn will be the larger.
  FR-002a is deliberately the smaller of the two so this feature does not
  become a dependency of #1234.
- **No new Cargo dependencies.** The feature reads annotations already in
  memory on the emission path.

## Out of Scope

- **#1234 — the thirteen missing `requirements.txt` edges.** A missing-edge
  defect, not an explanation gap. Deferred so the I7 cell keeps reporting
  it. Its fix must wire the edges and annotate inclusion on the target
  (`waybill:build-inclusion`, `waybill:lifecycle-scope`) rather than
  withhold the edge — an edge asserts *X declares Y*, not *Y ships*.
- **#1235 — the three duplicate maven coordinates.** A divergent-PURL
  defect in the dual-key insert, in #1216's family. Explaining it would
  close the cell that reports it while leaving the duplicate in place.
- **`cmake.rs`'s missing `unresolved-reason`.** No cmake-sourced component
  exists in the corpus, so the change cannot be verified by any lane.
  FR-016 records it instead of making it. A corpus target that builds with
  cmake would move it in scope and is the cheaper prerequisite.
- **Deciding whether a mechanism-C component belongs in the document at
  all.** #1214 asks whether the pip reader should attach a Python
  dependency to a Go subject or merely explain it. This feature explains;
  whether such components should be emitted is a reader-scope question that
  would change component counts, which SC-009 forbids.
- **Collapsing mechanism B's duplicate pairs.** US1 explains which copy of
  a duplicated name is unused. Choosing between them, or removing one, is
  divergent-PURL work.
- **Wiring mechanism D's flake inputs to the subject.** It would assert a
  dependency `flake.lock` does not state. The marker is the truthful answer.
- **The remaining I-invariants.** #1206–#1211 are separate defect classes.
  I7 is the one the first full-corpus run surfaced as a vocabulary gap.
- **Retiring milestone 167's Go/npm codes.** FR-004 keeps them. A unified
  cross-ecosystem vocabulary would be a redesign, and the five existing
  codes are accurate for the populations they name.
