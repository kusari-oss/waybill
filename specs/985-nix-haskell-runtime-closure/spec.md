# Feature Specification: Transitive runtime closure for Nix-built Haskell projects

**Feature Branch**: `985-nix-haskell-runtime-closure`
**Created**: 2026-09-24
**Status**: Draft
**Input**: Resolve the transitive runtime closure for Nix-built Haskell projects from the pinned nixpkgs (issue #962), scoped to `libraryHaskellDepends` + `executableHaskellDepends` only.

> **Numbering note**: this is milestone **985**, which collides numerically with
> GitHub issue **#985** (the deferred test/benchmark research). They are unrelated
> — milestone and issue numbers are separate namespaces here, as with milestone
> 926 ↔ issue #947. This milestone implements issue **#962**.

## Context

Milestone 926 (#947) substitutes a pinned nixpkgs revision for a Haskell
lockfile, resolving the versions of a project's **declared** dependencies. But
the thing it substitutes for — a `cabal.project.freeze` — contains the
transitive closure. So a project with a freeze file gets its full resolved set
today, while a Nix-built project gets only its direct dependencies. This closes
that asymmetry.

The measurement issue #962 requires was completed before this spec was written;
its numbers appear throughout and in Success Criteria.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer sees what the project actually depends on (Priority: P1)

Someone reading an SBOM for a Nix-built Haskell project wants the set of
Haskell packages the built artifact contains, not just the handful the author
happened to name in a `.cabal` file. Today they get the declared set and a
document-scope note saying transitive edges were unresolvable; they have no way
to learn the rest without building the project themselves.

**Why this priority**: it is the entire point of the feature, and it is what
Constitution Principle VIII (Completeness) asks for. A vulnerability in a
package reached only transitively is invisible in today's document.

**Independent Test**: scan a Nix-built Haskell project with a `flake.lock`
pinning nixpkgs; the emitted document contains packages the project never
declares, each carrying a version and a source hash, and each reachable from
the root through dependency edges.

**Acceptance Scenarios**:

1. **Given** a project declaring N Haskell dependencies and a `flake.lock`
   pinning an exact nixpkgs revision, **When** it is scanned, **Then** the
   document contains the transitive runtime closure of those dependencies, and
   the count exceeds N.
2. **Given** a package reached only transitively, **When** the document is
   read, **Then** that package carries a version and a native source hash from
   the pinned revision, exactly as a declared package does.
3. **Given** the same project and revision scanned twice, **When** the two
   documents are compared, **Then** they are byte-identical.

---

### User Story 2 - A consumer can tell declared from transitive (Priority: P1)

A reader needs to distinguish "the author asked for this" from "this came along
with it". The two support different decisions: the first is a thing the project
can change directly, the second is a consequence of that choice.

**Why this priority**: co-equal with US1. Emitting the closure without marking
provenance would replace one incomplete document with a misleading one, and
Principle X requires a document to say where its facts came from.

**Independent Test**: scan a project and confirm every component the closure
added is distinguishable from a declared one by a value in the document, without
consulting the project's source.

**Acceptance Scenarios**:

1. **Given** a resolved closure, **When** a component was declared by the
   project, **Then** it is marked as declared.
2. **Given** a resolved closure, **When** a component was reached only
   transitively, **Then** it is marked as transitive.
3. **Given** a component reachable both directly and transitively, **When** the
   document is read, **Then** it is marked declared — the stronger claim wins.

---

### User Story 3 - The dependency graph stays connected (Priority: P1)

Every package the closure adds must be reachable from the root through
dependency edges. A component that appears in the document with no path to the
root cannot be reasoned about: a reader cannot tell why it is there.

**Why this priority**: co-equal. Milestone 980 established what happens when
this is neglected — 66% of one project's SPDX relationships were dropped and
185 CycloneDX edges pointed at nothing, while the document stayed schema-valid.
The invariant (I2) is now enforced per-PR and nightly, so this story is partly
about not regressing it under a much larger component count.

**Independent Test**: scan a project and confirm no edge endpoint names a
component absent from the document, and that every transitively-added component
has at least one inbound edge.

**Acceptance Scenarios**:

1. **Given** a resolved closure, **When** the document is checked, **Then** no
   dependency edge names a component the document does not contain.
2. **Given** a package reached through another package, **When** the document is
   read, **Then** an edge exists from the intermediate package to it, not merely
   from the root.
3. **Given** a package set containing a dependency cycle, **When** the closure is
   resolved, **Then** the scan terminates and the document is emitted.

---

### User Story 4 - An operator can see what the closure did (Priority: P2)

An operator comparing two scans, or debugging why a document grew, needs the
closure's own record: how many components it added, how many names it could not
resolve, and why.

**Why this priority**: valuable but not load-bearing for correctness. Deferred
below the three P1 stories because a correct closure with no summary is still
correct, whereas a summary over a wrong closure is worse than nothing.

**Independent Test**: scan a project and read the closure's counts at document
scope without inspecting individual components.

**Acceptance Scenarios**:

1. **Given** a scan that resolved a closure, **When** the document is read,
   **Then** it records how many components were declared, how many were added
   transitively, and how many names went unresolved.
2. **Given** a scan where the closure did not run, **When** the document is
   read, **Then** it is unchanged from today's output.

---

### Edge Cases

- **A name in a dependency list that resolves to nothing.** Recorded with a
  reason, never dropped silently and never guessed (Principle IX). This includes
  names that exist only through a compiler-configuration alias, which issue #984
  addresses; if that work is not present, such names surface here as unresolved
  rather than as silent holes.
- **A cycle among packages.** The walk must terminate. Hackage package sets
  contain mutually recursive dependencies.
- **A boot library inside the closure.** It carries no version and must not be
  traversed — its dependencies are a property of the compiler, not of the
  package set. The existing union-across-candidate-compilers rule (FR-014a of
  milestone 926) continues to decide what counts as boot.
- **A project with no `flake.lock`, or one pinning a moving reference.** The
  closure does not run; today's behaviour is unchanged.
- **A very large closure.** The largest measured is 394 components from 162
  declared. Document size is bounded by the package set, which is finite and
  fixed by the pinned revision, so the closure cannot grow without bound for a
  given revision. The operator-visible cost is covered by SC-008 and the opt-out
  by FR-016.
- **A package appearing in the closure under two different names** (e.g. via an
  alias). It must appear once.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST resolve the transitive closure of a project's
  declared Haskell dependencies from the pinned nixpkgs revision.
- **FR-002**: The closure MUST be the **runtime** closure: the dependency
  relations a built artifact carries. Test and benchmark relations are OUT of
  scope (see Out of Scope).
- **FR-003**: Resolving the closure MUST NOT require any retrieval beyond what
  resolving the declared dependencies already requires. A project whose declared
  dependencies resolve offline MUST also have its closure resolve offline.
- **FR-004**: Every component added by the closure MUST carry the same version
  and source-hash treatment as a declared component resolved from the same
  revision.
- **FR-005**: The system MUST NOT assert a version for any package it cannot
  resolve. An unresolvable name is recorded with a reason.
- **FR-006**: Every component MUST be distinguishable as declared or
  transitively reached.
- **FR-007**: A component reachable both directly and transitively MUST be
  recorded as declared.
- **FR-008**: Every component the closure adds MUST be reachable from the
  document root through dependency edges, and no edge endpoint may name a
  component absent from the document (invariant I2).
- **FR-009**: Edges MUST reflect the actual relation — a package reached through
  an intermediate MUST have an edge from that intermediate, not only from the
  root.
- **FR-010**: The closure walk MUST terminate on cyclic package sets.
- **FR-011**: A boot library MUST NOT be traversed, and MUST continue to be
  classified by the existing union-across-candidate-compilers rule.
- **FR-012**: Each package MUST appear at most once in the emitted document.
- **FR-013**: Two scans of one project at one revision MUST produce
  byte-identical documents.
- **FR-014**: The document MUST record, at document scope, how many components
  were declared, how many were added transitively, and how many names went
  unresolved.
- **FR-015**: A scan where the closure does not run — no lockfile, a moving
  reference, the feature disabled, or no Haskell dependencies — MUST produce
  output unchanged from before this feature.
- **FR-016**: The operator MUST be able to disable the closure independently of
  the milestone-926 declared-dependency resolution, so the prior behaviour
  remains reachable.

### Key Entities

- **Declared dependency**: a Haskell package the project names in its own
  manifest. The existing input to milestone 926's resolution.
- **Closure member**: a Haskell package reachable from a declared dependency
  through runtime dependency relations in the pinned package set. Carries the
  same version and hash treatment as a declared dependency, plus a marker
  distinguishing it.
- **Dependency relation**: a directed link from one package to another within
  the pinned package set, used both to compute the closure and to emit edges.
- **Closure summary**: the document-scope record of what the pass did — declared
  count, added count, unresolved count.

## Success Criteria *(mandatory)*

### Measurable Outcomes

Baselines are measured, not estimated; the method and per-project figures are
recorded in issue #962 and reproduced by the probe committed with this feature.

- **SC-001**: For a Nix-built Haskell project, the number of Haskell packages in
  the document increases by at least **1.5×** relative to the declared set.
  (Measured runtime-closure multipliers on three real projects: 1.5×, 3.8×,
  2.4×.)
- **SC-002**: For every project measured, the resolved runtime closure matches
  the closure an independent Nix evaluation reports, to within the number of
  names that are unresolvable for a recorded reason. (Measured before
  implementation: exact agreement at 167 components on one project; one name
  apart on another, attributable to issue #984.)
- **SC-003**: 100% of components added by the closure carry a version and a
  source hash, or are absent from the document. No component carries one without
  the other.
- **SC-004**: 100% of components added by the closure are reachable from the
  document root, and 0 dependency edges name an absent component.
- **SC-005**: 100% of names that could not be resolved carry a reason.
- **SC-006**: Two consecutive scans of one project at one revision produce
  byte-identical documents.
- **SC-007**: A project with no Haskell dependencies, no lockfile, or a moving
  reference produces a document byte-identical to the pre-feature output.
- **SC-008**: For the largest measured project (162 declared, 394 in closure),
  scanning with the closure enabled takes no more than **1.5×** the wall clock
  of scanning the same project with it disabled, on the same machine with the
  package set already local. Stated as a ratio against a baseline the same
  scan establishes, because absolute timings are machine-specific.

## Out of Scope

- **Test and benchmark dependencies.** Deliberately excluded. Measured
  multipliers including them reach 7.3× and 9.8×, against 1.5–3.8× for the
  runtime closure alone, and — decisively — the runtime closure can be checked
  against an external oracle (a Nix evaluation of the runtime inputs) while a
  test closure cannot. Committing to a 7.3× change in document size on a set
  nothing can verify is not supportable. Tracked as GitHub issue **#985**.
- **Resolving names that exist only via a compiler-configuration alias.**
  Tracked separately as issue **#984**; if unresolved there, such names appear
  here as unresolved-with-reason rather than as silent omissions.
- **Changing which GHC package set is chosen.** Milestone 926's candidate-series
  selection and union rule are reused unchanged.
- **Non-Haskell ecosystems.** Nothing here generalises to other readers.

## Assumptions

- The pinned package set is the correct authority for what a Nix-built project
  resolves to. This is milestone 926's premise and is unchanged.
- The closure's shape does not vary by GHC series. Measured across four series
  on two projects: the closure was identical each time; only boot classification
  varied, by 1–2 components, which the existing union rule already governs.
- Cycles exist in real package sets and must be handled, but are not frequent
  enough to need an optimised representation.
- Document growth of 1.5–3.8× is acceptable to consumers. This is a substantial
  change in document size and is the main reason the feature is opt-outable
  (FR-016).
- Milestone 926's retrieval, caching, offline behaviour, and boot-library rules
  are reused as-is. This feature extends that resolver rather than standing
  alone.
- The declared-dependency path's existing behaviour is a regression baseline:
  where the closure does not run, output must not move.
