# Feature Specification: npm-ecosystem optional-axis classification

**Feature Branch**: `1272-npm-optional-scope`
**Created**: 2026-11-02
**Status**: Draft
**Input**: User description: "npm ecosystem: classify a root-manifest optionalDependencies declaration in the pnpm v9, yarn v1, yarn Berry and bun readers (#1324)"

## Context

A dependency a project declares only under `optionalDependencies` is
classified optional when the project is scanned through
`package-lock.json`, and left unclassified by the other four
npm-ecosystem readers. Four of the five lockfiles record the axis; each
records it in a different place, and only npm's happens to be where
waybill looks.

Consumers reading the emitted SBOM cannot tell these two situations
apart: "this package is required at runtime" and "this package may be
absent and the build still works". That distinction is the whole point
of the optional axis — it is what lets a consumer filter a component
that is not in production. Today four of five npm dialects answer
"required" for both.

This is #1297 for the optional axis rather than the development one, and
it covers the reader #1297's table omitted.

Evidence base: `measurements/findings.md` in this directory, with its
three re-runnable probes — `optional-axis-probe.sh` (where each manager
writes the axis), `cancellation-probe.sh` (what the axis means) and
`peer-context-probe.sh` (how v9 expresses peer-optionality). Every claim
below about where a manager writes the axis is measured on five
installed managers, not read from documentation.

## Clarifications

### Session 2026-11-02

- Q: Is transitive optional propagation in scope, or is this root-manifest declarations only? → A: In scope for all four dialects (option A) — the runtime closure FR-006 requires is mandatory either way, so root-only saves no traversal, and pnpm's root and transitive answers are the same field.
- Q: What should a reader do with a lockfile version newer than any measured here? → A: Read opportunistically and warn (option A) — attempt the known locations, and emit a diagnostic naming the unrecognized version so an incomplete axis is visible rather than silent.
- Q: Does the Berry reader keep `package.json` as a source, or move to the lockfile only? → A: Union both sources (option A) — hand-writing `dependenciesMeta` is the documented way to mark a Berry dependency optional, so the manifest is a legitimate source; Berry mirrors it into the lockfile on install, and the two diverge only when the lockfile is stale, where the manifest is the more current statement of intent.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - pnpm v9 projects report their optional dependencies (Priority: P1)

A consumer scans a pnpm v9 project whose manifest declares a dependency
optional. The emitted SBOM marks that component optional, and marks
every package reachable only through optional edges optional too.

**Why this priority**: Highest value for the least work and the least
risk. The lockfile already carries the finished answer in a field the
reader does not read, so this is one source change rather than a new
traversal — the same shape of defect, one field over, that #1314 fixed
for the development axis in this same reader. It also delivers the
transitive answer for free, which no other arm does.

**Independent Test**: Scan a pnpm v9 fixture whose manifest declares one
dependency under `optionalDependencies` and nothing else differently.
The declared package reports the optional scope; the runtime package
does not. Delivers the optional axis for one of the four dialects with
no change to any other reader.

**Acceptance Scenarios**:

1. **Given** a pnpm v9 lockfile whose `snapshots` entry for a package
   carries `optional: true`, **When** the project is scanned, **Then**
   that component is classified optional and carries a derivation
   annotation naming the pnpm mechanism.
2. **Given** the same lockfile, **When** the project is scanned,
   **Then** a package declared under `dependencies` is NOT classified
   optional.
3. **Given** a pnpm v9 lockfile where a runtime package declares an
   optional dependency of its own, **When** the project is scanned,
   **Then** the optional child is classified optional and the runtime
   parent is not.
4. **Given** a pnpm v6/v7 lockfile, **When** the project is scanned,
   **Then** classification is unchanged from before this feature.

---

### User Story 2 - bun projects report their optional dependencies (Priority: P2)

A consumer scans a bun project whose manifest declares a dependency
optional. The emitted SBOM marks that component optional.

**Why this priority**: Second-cheapest. The declaration lives in the
`workspaces` map, and the reader was taught to read the development
sections of that exact map in the previous milestone, so the access path
exists and is proven. Unlike pnpm, bun records the edge rather than a
closed answer, so this arm needs a traversal.

**Independent Test**: Scan a bun fixture whose root manifest declares
one dependency optional; that component reports the optional scope and
the runtime sibling does not.

**Acceptance Scenarios**:

1. **Given** a bun lockfile whose `workspaces` entry declares a package
   under `optionalDependencies`, **When** the project is scanned,
   **Then** that component is classified optional.
2. **Given** a bun lockfile where the same package is ALSO declared
   under `dependencies` by any workspace, **When** the project is
   scanned, **Then** it is NOT classified optional — a runtime path
   cancels optionality.
3. **Given** a bun project, **When** it is scanned, **Then** the
   development-axis classification is byte-identical to before this
   feature.

---

### User Story 3 - yarn Berry projects report optional dependencies from the lockfile (Priority: P3)

A consumer scans an ordinary yarn Berry project — one that never
hand-wrote `dependenciesMeta` into its manifest — and the emitted SBOM
marks its optional dependencies optional.

**Why this priority**: This arm is currently not merely incomplete but
**misleading about its own coverage**: the reader has passing tests for
optional classification that assert a manifest shape real Berry projects
do not have. Measured, Berry derives `dependenciesMeta.<name>.optional`
into its own lockfile, and an ordinary project's manifest has no
`dependenciesMeta` at all. Reading the lockfile also gains the
transitive case, which the manifest can never carry. Ranked below bun
only because it requires correcting a wrong code comment and replacing
fixtures, not just adding a source.

**Independent Test**: Scan a Berry fixture whose manifest declares a
dependency optional and whose manifest contains NO `dependenciesMeta`
key — the realistic shape. The component reports the optional scope.

**Acceptance Scenarios**:

1. **Given** a Berry lockfile whose root workspace entry carries
   `dependenciesMeta.<name>.optional: true`, and a manifest with no
   `dependenciesMeta` key, **When** the project is scanned, **Then**
   that component is classified optional.
2. **Given** a Berry lockfile where a non-root entry carries
   `dependenciesMeta.<name>.optional: true`, **When** the project is
   scanned, **Then** the named dependency is classified optional.
3. **Given** a project that DOES hand-write `dependenciesMeta` in its
   manifest, **When** it is scanned, **Then** classification is
   unchanged from before this feature — the manifest source is
   retained alongside the new lockfile source, not replaced.
4. **Given** a manifest that marks a dependency optional and a lockfile
   that predates that edit and so does not carry it, **When** the
   project is scanned, **Then** that dependency is still classified
   optional.
5. **Given** a name that also satisfies the peer-optional predicate,
   **When** the project is scanned, **Then** the peer classification
   continues to win.

---

### User Story 4 - yarn v1 projects report optional dependencies (Priority: P4)

A consumer scans a yarn v1 project whose manifest declares a dependency
optional, and the emitted SBOM marks it optional.

**Why this priority**: Lowest, because it is the only arm whose root
answer cannot come from the lockfile at all. Measured, a v1 lockfile has
no root or workspace entry, so a root declaration has nowhere to go; the
manifest is the only source. It does record transitive optional edges as
a per-entry sub-block. Ranked last because it is the one arm where the
lockfile is not authoritative, which makes it the least consistent with
how every other reader works.

**Independent Test**: Scan a yarn v1 fixture whose manifest declares one
dependency optional; that component reports the optional scope.

**Acceptance Scenarios**:

1. **Given** a yarn v1 project whose manifest declares a package under
   `optionalDependencies`, **When** it is scanned, **Then** that
   component is classified optional.
2. **Given** a yarn v1 lockfile where an entry carries an
   `optionalDependencies` sub-block, **When** the project is scanned,
   **Then** the named dependency is classified optional unless some
   non-optional path also reaches it.
3. **Given** a yarn v1 project with no optional declarations, **When**
   it is scanned, **Then** output is byte-identical to before this
   feature.

---

### Edge Cases

- A package is declared optional by one workspace and runtime by
  another. A runtime path cancels optionality, matching how the
  development axis already resolves the same conflict.
- A package is reachable by both an optional and a non-optional edge.
  It is not optional. This is the cancellation rule, measured in Probe C
  of `measurements/findings.md` against npm and pnpm.
- A package is declared both optional and development. The existing
  precedence between those two axes is preserved, not redefined.
- A package is peer-optional. The existing peer classification wins, as
  it does today in the npm and pnpm readers.
- An optional declaration names a workspace member, or a
  `link:`/`file:`/`workspace:` target. No registry identity is invented,
  matching the existing development-axis behaviour.
- An optional declaration names a package absent from the lockfile —
  declared but never resolved. The declaration cannot be attached to a
  component that does not exist, and is dropped with a diagnostic.
- A lockfile is malformed in the region carrying the axis. Parsing
  continues and the axis is left unset rather than guessed.
- A pnpm v6/v7 lockfile, which carries the marker in the location the
  reader reads today. Unchanged.
- A lockfile whose declared format version is newer than anything
  measured here. The known locations are still read, and a diagnostic
  names the version — so a format move surfaces as a logged warning
  rather than as a confident "not optional".

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The pnpm reader MUST derive the optional axis for v9
  lockfiles from the section where pnpm v9 actually records it, as
  identified in `measurements/findings.md`.
- **FR-002**: The pnpm reader MUST continue to derive the axis for
  v6/v7 lockfiles from the location it reads today, with classification
  unchanged for those dialects.
- **FR-003**: The bun reader MUST treat a workspace-level
  `optionalDependencies` declaration as a seed for the optional axis.
- **FR-004**: The yarn Berry reader MUST derive the optional axis from
  the lockfile, and MUST NOT require the manifest to carry
  `dependenciesMeta` in order to classify. This removes the manifest as
  a *requirement*, not as a source — see FR-004a, which is binding on
  how the two sources combine. FR-004 read alone would license deleting
  the manifest path, which FR-004a forbids.
- **FR-004a**: The yarn Berry reader MUST treat the lockfile's
  per-entry `dependenciesMeta` and the manifest's `dependenciesMeta` as
  a union: a dependency marked optional in either source is classified
  optional. The manifest path that exists today MUST be retained, not
  replaced. Berry mirrors a hand-written manifest declaration into the
  lockfile on install, so the two agree in the common case; they diverge
  only when the lockfile is stale relative to an edited manifest, and
  there the manifest is the more current statement of intent.
- **FR-005**: The yarn v1 reader MUST derive the root optional axis from
  the manifest, which measurement shows is its only available source.
- **FR-006**: A package reachable by any non-optional path MUST NOT be
  classified optional, in every reader. Measured in Probe C of
  `measurements/findings.md`: both reference implementations decline to
  mark such a package optional even when the root declared it so.
  Where several lockfile entries collapse to one emitted component —
  pnpm v9's peer contexts do, per Probe D — the per-entry markers MUST
  be combined so that any non-optional entry cancels. A
  last-write-wins combination would make the classification depend on
  key order within the lockfile.
- **FR-007**: Optionality MUST attach to the reachable package rather
  than to the declaring parent, matching the two reference
  implementations measured in `measurements/findings.md`.
- **FR-007a**: Each reader MUST propagate optionality transitively: a
  package reachable from an optional declaration only along optional
  edges MUST be classified optional, subject to FR-006's cancellation
  rule. This applies to all four dialects.
- **FR-008**: Every newly optional-classified component MUST carry a
  derivation annotation naming which reader mechanism classified it,
  consistent with the existing vocabulary for this annotation.
- **FR-009**: Existing peer-optional precedence MUST be preserved in
  every reader this feature touches. For pnpm v9 this requires giving
  the guard a source it did not have: measured in Probe D, a real v9
  lockfile contains no `peer:` key anywhere, so the guard's existing
  input could never fire. v9 names optional peers from the declaring
  parent's `peerDependenciesMeta`, and the guard MUST read that.
  "Preserve" is therefore not a no-op for this dialect — the precedence
  existed in the code and not in the behaviour.
- **FR-010**: The development-axis classification of every reader this
  feature touches MUST be unchanged. A declaration being optional MUST
  NOT make it development, and MUST continue to disqualify it from
  being development.
- **FR-011**: A malformed or absent axis-bearing region MUST leave the
  axis unset and MUST NOT abort the parse.
- **FR-011a**: When a lockfile declares a format version newer than any
  version this feature was measured against, each reader MUST attempt
  the locations it knows and MUST emit a diagnostic naming the
  unrecognized version and the possibility that the optional axis is
  incomplete. It MUST NOT fail the scan, and MUST NOT report an absent
  axis as a confirmed absence. This is the failure mode that produced the
  defect being fixed: pnpm moved the marker between v7 and v9, and the
  reader answered "not optional" where it could only honestly answer
  "unknown".
- **FR-012**: An optional declaration naming a package with no entry in
  the lockfile MUST be dropped with a diagnostic rather than fabricating
  a component.
- **FR-013**: Fixtures proving the Berry and yarn v1 arms MUST use the
  manifest and lockfile shapes the managers actually produce, as
  captured by the probe. A fixture asserting a hand-written shape MUST
  NOT be the only evidence for an arm.
- **FR-014**: The incorrect comment in the yarn reader asserting that
  Berry's lockfile does not carry the optional flag MUST be corrected,
  since it is the stated reason for the current source choice. The
  existing Berry optional fixture's header makes the same claim while
  its own body writes `dependenciesMeta` into the lockfile; that
  self-contradiction MUST be corrected too, because a reader who
  believes either text would reintroduce the defect.
- **FR-015**: Fixtures MUST use synthetic package names.
- **FR-016**: No new third-party dependencies.

### Key Entities

- **Optional declaration**: a statement in a root manifest, or in a
  package's own metadata, that a named dependency may be absent. Lives
  in a different place in each of the five dialects.
- **Optional seed**: the set of packages a manifest or lockfile
  declares optional directly, before any propagation. Named
  `OptionalSeed` in `data-model.md`.
- **Runtime seed**: the set declared under a non-optional,
  non-development section. Named `RuntimeSeed` in `data-model.md`, and
  MUST be kept separate from the optional seed — see FR-006, which
  cannot hold if the two are merged.
- **Optional closure**: the set of packages reachable from the optional
  seed along optional edges, minus the runtime closure. Two dialects
  supply this already computed; three supply only the edges. Named
  `OptionalClosure` in `data-model.md`.
- **Runtime closure**: the set reachable from the runtime seed by any
  edge. The cancelling set for the optional axis, and already computed
  for the development axis. Named `RuntimeClosure` in `data-model.md`.
- **Derivation annotation**: the per-component record of which reader
  mechanism produced the classification.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For each of the four dialects, a project declaring one
  dependency optional reports exactly one optional component — the
  declared one — where today it reports zero.
- **SC-002**: All five npm-ecosystem dialects agree on the optional
  classification of the same dependency graph expressed in each. Today
  one of five answers correctly; the target is five of five.
- **SC-003**: For a runtime parent declaring an optional child, all five
  dialects classify the child optional and the parent not.
- **SC-004**: Development-axis classification is byte-identical to the
  pre-feature output for every existing fixture across all five
  dialects.
- **SC-005**: Every arm is proven by at least one fixture whose manifest
  and lockfile were produced by running the real manager, and the Berry
  arm is proven by a manifest containing no `dependenciesMeta` key.
- **SC-006**: Removing any one reader's classification logic makes at
  least one test fail — no arm is asserted only by a test that would
  pass without it.
- **SC-007**: The committed probes reproduce every row of
  `measurements/findings.md` when re-run, and the cancellation probe's
  two cases continue to return different answers — a run in which they
  agree indicates a broken harness, not a changed manager.
- **SC-008**: A lockfile bearing an unrecognized future format version
  produces a diagnostic naming that version, in every one of the four
  dialects, and does not fail the scan.

## Assumptions

- **Cancellation semantics are measured, not inferred** (resolved
  2026-11-02, plan phase). This was recorded here as the feature's one
  unmeasured assumption; `measurements/cancellation-probe.sh` and Probe
  C of `measurements/findings.md` now settle it. With one package
  reachable by both an optional and a non-optional path, npm writes
  `optional: false` and pnpm writes no `optional` key — both decline to
  mark it optional. The same probe's control, where the package is
  reachable only along an optional edge, has both managers mark it
  optional. FR-006 and FR-007a therefore stand as written, and no
  requirement rests on inference.
- Transitive propagation is in scope for all four dialects, not just the
  root declaration (clarified 2026-11-02). This is cheaper than it
  sounds: FR-006's cancellation rule already requires walking the
  runtime closure, so the only additional work is a second pass
  propagating optional along optional edges. For pnpm there is no
  additional work at all, because its root and transitive answers are
  the same field.
- Reading the manifest for the yarn v1 root answer is acceptable, since
  measurement shows no alternative exists and the yarn reader already
  loads the manifest for two existing features.
- The optional and development axes remain separate classifications with
  their existing precedence. This feature does not redefine what happens
  when a dependency is both.
- The emitted representation of the optional axis in each output format
  is already established and unchanged; this feature changes which
  components receive that classification, not how it is expressed.
- Lockfile formats are external behaviour that can change without
  notice. The committed probe is the mechanism for re-establishing these
  findings, and is expected to be re-run rather than trusted
  indefinitely.
- `optionalDependencies` on a transitive package in a pnpm v6/v7
  lockfile is out of scope; those dialects are frozen by FR-002.
