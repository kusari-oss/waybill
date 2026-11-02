# Feature Specification: Haskell/nixpkgs resolution correctness

**Feature Branch**: `1273-haskell-nixpkgs-truthfulness`
**Created**: 2026-11-02
**Status**: Draft
**Input**: User description: "Haskell/nixpkgs resolution correctness: an hpack-only
project yields nothing and claims complete (#1278); the optional-mark closure is
seeded from what it must confirm (#1281); resolved components carry no license
(#1280); unresolved-reason is not reconciled (#1279)"

## Context

Four open issues against the Haskell reader and the nixpkgs resolution pass.
They are one feature because they are one defect class: **the pass does not tell
the truth about what it did.** Each one produces a document that reads as a
confident answer and is not one.

All four were filed against waybill 0.13.1 (`96e7a545`). All four were
re-measured against 0.14.0 at the head of this branch before this spec was
written, because a spec built on a stale defect report describes a defect that
may no longer exist. The numbers below are from that re-measurement, not from
the issues.

| issue | measured at 0.14.0 |
|---|---|
| #1278 | hpack-only project: **0 components**, `waybill:graph-completeness=complete`, no diagnostic — and the reader WAS dispatched to `package.yaml` (`per_reader_dispatch_counts{"haskell": 1}`) |
| #1281 | `aeson` and `async` marked optional from a `buildable: False` stanza are **restored to runtime** once a `flake.lock` pins nixpkgs; the closure's seed is `declared=5`, which contains them |
| #1280 | **0 of 66** emitted hackage components carry a license |
| #1279 | **2 components are version-resolved AND still carry `waybill:unresolved-reason`**; only 4 of 66 carry the annotation at all |

Reproduction harnesses are committed under `measurements/`.

## Clarifications

### Session 2026-11-02

- Q: In a project with BOTH an hpack `package.yaml` and an hpack-generated
  `*.cabal`, which is the source of truth for that package's dependencies?
  → A: **`package.yaml`**. It is hpack's own source of truth and the more
  current statement of intent; the generated file is output. The consequence
  taken deliberately is that waybill describes the project as its author last
  declared it rather than as a tree where hpack never runs would build it. The
  disagreement does not become invisible — FR-004 keeps it reportable, and
  FR-004a requires the existing warning's advice to be rewritten, since
  "regenerate before scanning" stops being true the moment the manifest is what
  gets read.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — a project the reader cannot read says so (Priority: P1)

An operator scans a Haskell project that declares its dependencies in hpack's
`package.yaml` with no committed `*.cabal` file. Today the scan succeeds, emits
no Haskell components at all, logs nothing about it, and the document asserts
`waybill:graph-completeness=complete`. The operator has an SBOM that says the
project has no dependencies, with nothing anywhere to suggest otherwise.

**Why this priority**: it is the only one of the four that produces a
*confidently wrong* document rather than a degraded one. The other three publish
a real answer with something missing from it; this one publishes nothing and
claims the nothing is complete. A missing component an operator can see is a
different kind of problem from a missing component the document denies exists.

hpack is also not an edge case: `package.yaml` is how most stack-based Haskell
projects declare dependencies, and a project that `.gitignore`s its generated
`*.cabal` is following hpack's own documented workflow.

**Independent Test**: scan a project containing only `package.yaml` and assert
the emitted components match its declared dependencies.

**Acceptance Scenarios**:

1. **Given** a project with `package.yaml` and no `*.cabal`, **When** it is
   scanned, **Then** its declared dependencies are emitted as components.
2. **Given** that same project, **When** it is scanned, **Then** the document
   does NOT assert graph completeness on the strength of having found nothing.
3. **Given** a project with BOTH `package.yaml` and an hpack-generated
   `*.cabal`, **When** it is scanned, **Then** exactly one of the two is used as
   the source of truth and the choice is recorded, so no component is emitted
   twice and no operator has to guess which file the document describes.
4. **Given** a `package.yaml` this feature cannot parse, **When** it is scanned,
   **Then** the failure is announced and completeness is not asserted — an
   unparseable manifest must not degrade into the silence this story exists to
   remove.

---

### User Story 2 — an optional mark is withdrawn only on evidence (Priority: P2)

An operator scans a Haskell project with a default-off build flag, so some
dependencies are declared only by a stanza that is `buildable: False` under the
manifest's own flag defaults. The `.cabal` reader marks those optional. When the
project also has a `flake.lock` pinning nixpkgs, the mark is withdrawn and the
dependencies are published as ordinary runtime dependencies.

**Why this priority**: it silently undoes a shipped fix (#955 / PR #1030)
whenever a `flake.lock` is present, which for a Nix-based Haskell project is
always. The withdrawal is justified in the code as "evidence beats inference",
and the evidence is circular: the runtime closure is seeded from the declared
dependency set, which contains the components the reader has just marked
optional, so every one of them is reached by construction. The closure can only
ever confirm the marks it was handed.

The measured consequence is not confined to the directly-marked components.
Restoring a mark on one component also restores everything reachable below it,
so the error fans out through the closure.

**Independent Test**: scan a project with a default-off flag twice, once with
and once without a `flake.lock`, and assert the optional classification is the
same both times unless a non-seed closure path genuinely reaches the component.

**Acceptance Scenarios**:

1. **Given** a dependency declared only by a `buildable: False` stanza and
   reachable by no other path, **When** the project is scanned with a
   `flake.lock` pinning nixpkgs, **Then** the optional mark survives.
2. **Given** a dependency declared by such a stanza AND genuinely reachable
   through the closure below a dependency that is actually built, **When** the
   project is scanned, **Then** the mark is withdrawn and the withdrawal names
   the path that justified it.
3. **Given** either case, **When** the project is scanned, **Then** the
   diagnostic distinguishes "no mark was withdrawn" from "the reconciliation did
   not run", which today look identical.

---

### User Story 3 — a resolved component carries the license its source declares (Priority: P3)

An operator scans a Nix-based Haskell project and gets components with resolved
versions and no license information, though the `hackage-packages.nix` the
resolution already read and cached declares a license for every package in it.

**Why this priority**: it is a pure gain with a known source — the data is
already on disk in a file this pass opens — and licensing is one of the two
things most consumers read an SBOM for. It is below US1 and US2 because an
absent license field is visibly absent, where their defects are not.

**Independent Test**: scan a project resolved against a pinned nixpkgs and
assert that components resolved from the cached package set carry the license
that set declares.

**Acceptance Scenarios**:

1. **Given** a project resolved against a pinned nixpkgs revision, **When** it
   is scanned, **Then** components resolved from the cached package set carry
   their declared license.
2. **Given** a package whose declared license cannot be canonicalised, **When**
   it is scanned, **Then** the raw declaration is preserved rather than dropped,
   and the fact that it is uncanonicalised is recorded.
3. **Given** a component the resolution did NOT resolve, **When** it is
   scanned, **Then** no license is invented for it.

---

### User Story 4 — the unresolved-reason annotation agrees with the document (Priority: P4)

An operator filters components by `waybill:unresolved-reason` to find what
waybill could not identify. Today two components in the measured fixture are
version-resolved and still carry a reason saying they are unresolved, and 62 of
66 carry no reason at all despite the annotation being specified as universal
across design-tier components.

**Why this priority**: it is a self-contradiction inside one document rather
than a missing fact, so a consumer acting on the annotation is actively misled
rather than merely under-informed. It is last because it is the narrowest, and
because the two contradicting components are the same two US2 is about — so part
of it may resolve as a consequence of US2 and the remainder should be measured
after, not before.

**Independent Test**: scan the resolved fixture and assert no component both
carries a resolved version and an unresolved-reason.

**Acceptance Scenarios**:

1. **Given** a component the nixpkgs pass resolved to a version, **When** the
   document is emitted, **Then** it does not also carry an unresolved-reason.
2. **Given** a component the pass did not resolve, **When** the document is
   emitted, **Then** it carries a reason naming why.
3. **Given** a compiler-supplied transitive dependency, **When** the document is
   emitted, **Then** its reason distinguishes "supplied by the compiler" from
   "could not be determined".

### Edge Cases

- A `package.yaml` in a multi-package repository where some packages have a
  committed `*.cabal` and some do not.
- A `package.yaml` whose `*.cabal` sibling is hpack-generated but STALE relative
  to it — the two disagree and only one can be the document's answer.
- A `package.yaml` using hpack features that have no `.cabal` equivalent this
  feature models.
- A dependency marked optional by a `buildable: False` stanza that is ALSO a
  direct dependency of the library — not optional, and the closure must say so
  for the right reason.
- A project whose `flake.lock` pins a nixpkgs revision not present in the local
  cache, under `--offline`.
- A package in the cached set whose license field is absent, empty, or a Nix
  expression rather than a string.

## Requirements *(mandatory)*

### Functional Requirements

#### Reading hpack manifests (US1)

- **FR-001**: A project whose Haskell dependencies are declared only in
  `package.yaml` MUST have those dependencies emitted as components. This
  reverses milestone 143's Q3 decision, which chose to detect `package.yaml` and
  warn rather than parse it, on the stated grounds of avoiding a second source
  of truth. That decision is reversed deliberately and in the spec rather than
  in an implementation: the cost it avoided is real but smaller than publishing
  a project's dependency set as empty.
- **FR-002**: Where both `package.yaml` and an hpack-generated `*.cabal` are
  present, exactly one MUST be the source of truth for a given package, and the
  document MUST record which. Emitting from both is the second-source-of-truth
  failure milestone 143 was right to fear, and FR-001 does not license it.
  **`package.yaml` is the source of truth** (resolved 2026-11-02): hpack's own
  model makes the manifest the source and the `*.cabal` generated output, so the
  manifest is the more current statement of intent. This matches how the yarn
  Berry reader resolves the same tension in milestone 1272's FR-004a, where a
  manifest declaration overrides a lockfile that predates an edit.
- **FR-003**: A `package.yaml` that is present but cannot be parsed MUST produce
  a diagnostic naming the file and the reason, and MUST NOT leave the scan
  looking like a scan of a project with no dependencies.
- **FR-004**: A disagreement between a `package.yaml` and its generated
  `*.cabal` MUST remain visible to the operator. Milestone 143 shipped a warning
  for this case, and measurement shows it fires ONLY for it — the warning loop
  iterates `*.cabal` manifests, so a `package.yaml` with no sibling can never
  reach it. FR-001's gap and this behaviour are therefore not the same code path
  and fixing one does not fix the other.
- **FR-004a**: That warning's *content* MUST change as a consequence of FR-002.
  It currently reads "run 'hpack' to regenerate before scanning if package.yaml
  has been edited", which was sound advice while the generated file was the only
  thing read: regenerating it was the operator's only way to be described
  accurately. Under FR-002 it is no longer true — waybill reads the manifest, so
  regenerating changes nothing about the document. Leaving the text unchanged
  would instruct an operator to perform a step that cannot affect the output,
  which is worse than silence because it looks like a remedy.

#### Completeness claims (US1)

- **FR-005**: A document MUST NOT assert graph completeness on the strength of a
  component set that is empty because a manifest went unread. An absent reader
  result and a genuinely dependency-free project MUST be distinguishable in the
  emitted document.

#### The optional-mark closure (US2)

- **FR-006**: An optional mark derived from a `buildable: False` stanza MUST be
  withdrawn only when a component OTHER than the marked set reaches it through a
  closure edge. The seed set used to justify a withdrawal MUST NOT contain the
  components whose marks are under review.
- **FR-007**: A withdrawal MUST record what justified it, in enough detail that
  an operator can tell which path was taken.
- **FR-008**: The reconciliation MUST distinguish, in its diagnostic, "ran and
  withdrew nothing" from "did not run". These are the same output today, which
  is the condition under which a feature that cannot fire looks like a feature
  with nothing to do.
- **FR-009**: Where a withdrawal is declined, the component's optional
  classification and its derivation annotation MUST both be preserved —
  restoring one without the other would publish a scope with no account of
  where it came from.

#### Licenses from the cached package set (US3)

- **FR-010**: A component resolved from the cached nixpkgs Haskell package set
  MUST carry the license that set declares for it.
- **FR-011**: A declared license that cannot be canonicalised MUST be preserved
  as declared rather than dropped, and the document MUST record that it is
  uncanonicalised.
- **FR-012**: No license may be attached to a component the resolution did not
  resolve.

#### Reconciling unresolved-reason (US4)

- **FR-013**: A component the nixpkgs pass resolved MUST NOT carry an
  unresolved-reason.
- **FR-014**: A component the pass did not resolve MUST carry a reason, and the
  reason MUST distinguish compiler-supplied from undeterminable.

#### Cross-cutting

- **FR-015**: The development, optional and build axes of every ecosystem this
  feature does not name MUST be unchanged.
- **FR-016**: Where this feature changes what a reader emits, any committed
  fixture asserting the old shape MUST be corrected rather than pinned, and the
  correction MUST be justified against a measurement rather than against this
  spec.
- **FR-017**: Every diagnostic this feature adds MUST be reachable. A diagnostic
  that no input can trigger is the defect US1 and US2 are both instances of, and
  reintroducing it while fixing it would be the milestone's worst outcome.

### Key Entities

- **hpack manifest**: a `package.yaml` declaring a Haskell package's
  dependencies, which hpack renders to a `*.cabal` file.
- **Generated manifest**: a `*.cabal` carrying hpack's generation header, whose
  authority derives from a `package.yaml` that may or may not still agree.
- **Optional mark**: the classification the `.cabal` reader assigns a dependency
  declared only by a stanza excluded under the manifest's declared flag
  defaults.
- **Closure seed**: the set of components a runtime-closure walk starts from.
  Its membership is what FR-006 is about.
- **Non-seed reach**: a closure edge into a component from somewhere other than
  the seed set — the only evidence that can withdraw an optional mark.
- **Cached package set**: the `hackage-packages.nix` of a pinned nixpkgs
  revision, already fetched and cached per revision, carrying a version and a
  declared license per package.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An hpack-only project emits a component for every dependency its
  `package.yaml` declares, where today it emits zero.
- **SC-002**: No scan of a project with an unread Haskell manifest reports
  graph completeness.
- **SC-003**: A default-off-flag project classifies its flag-gated dependencies
  identically with and without a `flake.lock`, unless a non-seed closure path
  reaches them. Measured today: the two disagree on 2 of 2 directly-marked
  components, and on 8 components in total on the public library #955 was filed
  against.
- **SC-004**: Of the components resolved from a cached nixpkgs revision, the
  proportion carrying a declared license goes from the measured 0 of 66 to all
  of those the cached set declares one for.
- **SC-005**: No emitted component both carries a resolved version and an
  unresolved-reason. Measured today: 2.
- **SC-006**: Every requirement above is covered by a test that fails when the
  behaviour it describes is removed. Mutation-tested, not assumed — on this
  milestone's own evidence, two of the four defects are cases where the existing
  tests passed throughout.
- **SC-007**: The development-axis and optional-axis output of every other
  ecosystem is byte-identical.
- **SC-008**: Every probe committed under `measurements/` re-runs and reproduces
  its recorded finding.

## Assumptions

- **The four issues' diagnoses are re-measured, not taken on trust.** All four
  were filed against 0.13.1 and re-measured against 0.14.0 before this spec was
  written; the Context table holds those numbers. Any further claim this spec
  makes about external behaviour is marked as measured or labelled as inference.
- **hpack's rendering rules are an external behaviour and will be measured, not
  read.** How hpack resolves defaults, `source-dirs`, per-component dependency
  inheritance and the `dependencies`/`build-depends` relationship is a property
  of hpack, and the project's standing rule is to probe such behaviour before a
  number or a shape enters a requirement. FR-001's scope depends on it.
- **A nixpkgs revision already in the local cache is used for reproduction**, so
  US2/US3/US4 are reproducible offline. The revision #1281 measured
  (`a799d3e3…`) is present.
- **#1282 and #1283 are out of scope.** Both concern `--nix-closure` output
  quality, and how to fix them depends on open scoping decisions (#971, whether
  nix evaluation becomes primary; #1034, how deep a closure is followed).
  Fixing them before those are settled risks doing the work twice.
- **No new runtime dependency on the `hpack` binary.** The project's constitution
  constrains what the shipped binary links and shells out to; a parser is
  expected to be in-tree. Whether that is affordable for the subset of hpack
  FR-001 needs is a Phase 0 question, and if it is not, FR-001's shape changes
  rather than the constraint.
- **The two contradicting components in US4 are the same two US2 is about**, so
  US4's remaining population must be measured after US2 lands rather than
  predicted now.

## Dependencies

- The nixpkgs per-revision cache and the cached `hackage-packages.nix` it holds
  (milestone 926).
- The `.cabal` reader's flag-default evaluation and its optional-mark derivation
  (milestone 955 / PR #1030).
- The runtime-closure walk and its reconciliation pass (milestone 985).
- The universal unresolved-reason annotation, catalog row C151 (milestone 236).
- The graph-completeness annotation (milestone 158) for FR-005.
