# Feature Specification: the closure's build inputs that `unreferenced` hides

**Feature Branch**: `1372-unreferenced-pname-components`
**Created**: 2026-11-06
**Status**: Draft
**Issue**: #1366 — *Nix closure: 32 names sbomnix reports that waybill does not*

> **Numbering note.** Spec directory numbers and GitHub issue numbers are two
> separate sequences in this repository and do **not** correspond. This is
> spec **1372**; it implements issue **#1366**. Issue **#1372** is an unrelated
> complaint (the `image-postgres16` corpus golden's layer digests). Cite the
> issue, never the bare number.

## Context

A nix closure scan emits no component for `gzip 1.14`, `gawk 5.4.0`,
`gnused 4.9`, `less 692`, `patch 2.8` or `coreutils 9.11`. They are not
unmatchable — they are **absent**. Nothing in the document says they were
part of the build.

An absent component is strictly worse than an unmatchable one. Milestones
1366 and 1367 made 151 components *findable* by a vulnerability feed; these
six cannot be found by anything, because no consumer can ask about a
component that was never emitted.

## What was measured, and why it changes the question

The issue was filed as a coverage gap — "32 names sbomnix reports that we
don't" — and the obvious reading is that `unreferenced` derivations should
start emitting. **The measurement says something else.**

On the pinned `nix-closure-moat` closure (1,275 derivations):

| | count |
| --- | --- |
| derivations | 1,275 |
| emitted as a component | 1,174 |
| **not emitted** | **101** |
| …that declare a `pname` (so: real packages) | **101** |
| …whose role is `unreferenced` | **101** |
| **…that another derivation's `inputs.drvs` names** | **100** |
| …that nothing names at all | **1** — `moat 0.1`, the closure root |

**100 of the 101 are in the build graph.** Nix itself records them as inputs
to other derivations. The one that is not is the root, which correctly has no
consumer.

So `unreferenced` does not mean what a reader takes it to mean. It is computed
from **declared dependency attributes** — the `buildInputs` family — and these
derivations are named somewhere else. For the six above, the naming attributes
are:

| attribute | what it is |
| --- | --- |
| `postInstall`, `installPhase`, `postPatch`, `buildPhase` | shell script bodies that reference the store path |
| `allowedRequisites`, `initialPath` | stdenv's own bookkeeping |

None is a dependency declaration, so the classifier sees no reference — while
`inputs.drvs`, which is the graph nix actually built from, names all six.

**This is a correctness problem in a consumer-visible classification, not only
a coverage gap.** A document that labels `gzip 1.14` `unreferenced` is telling
a reader that nothing in the build depends on it. Three other derivations do.

## Why the comparison tool sees them

sbomnix walks store references rather than declared attributes, so a
derivation reachable only through a script body is as visible to it as any
other. That is the whole of the residual name gap: not a reader waybill lacks,
but a reference waybill declines to count.

## Clarifications

### Session 2026-11-06

- Q: Which of the 100 should emit?
  → A: **All of them.** A derivation named by another derivation's recorded
  build inputs was a build input; nix already decided that, and the declared
  attributes are a lossier restatement of it.

**What this buys.** All 23 names the comparison document lists for the
residual 32-name gap are in this population — **23 of 23, none missing**. The
gap is not chipped at by this feature; it is the feature.

**What it costs, stated plainly.** Roughly 45 distinct components appear that
did not before, including nine `bootstrap-stage*-stdenv-darwin` stages and
the darwin SDK. They are stdenv's scaffolding, they are genuinely build
inputs, and a consumer who does not want them must filter. FR-005's
disclosure is what makes that possible; without it this would be noise.

**Why not a narrower rule.** The obvious alternative — admit only references
from "dependency-shaped" attributes — reintroduces exactly the judgement that
produced this bug: deciding which attributes count. The six tools here are
named in `postInstall` and `initialPath`, which no reasonable list of
dependency attributes would include, and they are still build inputs.

**What it does not establish.** That every admitted component is a dependency
in the sense a consumer means. `bootstrap-stage0-stdenv-darwin` is a build
input and is not something anyone depends on deliberately. The disclosure
records how it was reached; it does not grade how much that matters.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer sees the tools the build actually used (Priority: P1)

Someone scans a nix-built artifact and asks what was in the build. Today
`gzip 1.14` is absent, so the answer omits a tool with a CVE history that
three derivations consumed. After this feature the component is present, and
the document says how it was referenced.

**Why this priority**: it is the feature. An absent component cannot be
matched, filtered, annotated or argued with; every other capability waybill
has is downstream of the component existing.

**Independent Test**: scan the pinned closure and confirm each of the six
named tools is emitted with the name and version the closure records.

**Acceptance Scenarios**:

1. **Given** a closure in which a derivation is named by another derivation's
   build inputs but by no declared dependency attribute, **When** the closure
   is scanned, **Then** a component is emitted for it.
2. **Given** that same closure, **When** the document is queried for
   `gzip`, `gawk`, `gnused`, `less`, `patch` and `coreutils`, **Then** all six
   are found — where before the query returned nothing.

---

### User Story 2 - `unreferenced` stops saying something untrue (Priority: P1)

A component labelled `unreferenced` should mean nothing in the build depends
on it. Today 100 of the 101 so labelled are named by another derivation's
build inputs.

**Why this priority**: equal to US1, because a wrong label is worse than a
missing component in one specific way — the missing component is visibly
missing, while the wrong label is confidently wrong. A consumer filtering
`unreferenced` to find dead weight would discard the build's own toolchain.

**Independent Test**: scan the pinned closure and assert that no component the
build graph references carries the `unreferenced` role.

**Acceptance Scenarios**:

1. **Given** a derivation named by another derivation's build inputs, **When**
   it is classified, **Then** its role is not `unreferenced`.
2. **Given** the closure root, which nothing consumes, **When** it is
   classified, **Then** `unreferenced` remains available and is what it
   carries.

---

### User Story 3 - A reader can tell how a component was reached (Priority: P2)

A component reached only through a script body is weaker evidence than one
declared in `buildInputs`. A reader should be able to tell them apart rather
than having both arrive as the same claim.

**Why this priority**: below the first two because the document is more
correct without it than with neither. It earns its place because this feature
admits a new kind of evidence, and an admission with no disclosure is how
`unreferenced` came to mean two different things in the first place.

**Independent Test**: scan the pinned closure and confirm every newly admitted
component records which signal admitted it.

**Acceptance Scenarios**:

1. **Given** a component admitted by the build graph rather than by a declared
   attribute, **When** the document is read, **Then** it says so.
2. **Given** a component declared in `buildInputs`, **When** the document is
   read, **Then** its disclosure is unchanged from today's.

---

### Edge Cases

- **The closure root.** `moat 0.1` is consumed by nothing, correctly. It must
  stay distinguishable from the 100 that are consumed.
- **Bootstrap stages.** Nine `bootstrap-stage*-stdenv-darwin` derivations are
  in the build graph and are stdenv's own scaffolding. They are real build
  inputs and are also not what most consumers mean by "a dependency".
- **A derivation consumed only by something itself not emitted.** Reachability
  through a chain that ends outside the emitted set.
- **Build variants.** Several of the 45 distinct names appear at more than one
  version (`cctools-binutils-darwin` at `1010.6` and `boot`, `macOS-SDK` at
  `14.4` and `26.4`). Existing merge behaviour must not collapse them.
- **A closure where every derivation is declared properly.** The document must
  be unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST treat a derivation named by another derivation's
  recorded build inputs as referenced, independently of whether a declared
  dependency attribute also names it.
- **FR-002**: The system MUST emit a component for a referenced derivation
  that declares a package name, where today it emits none.
- **FR-003**: The system MUST NOT label a derivation `unreferenced` when the
  build graph names it. Measured: 100 of the 101 so labelled today.
- **FR-004**: The system MUST keep `unreferenced` available and correct for a
  derivation nothing consumes, including the closure root.
- **FR-005**: The system MUST disclose, per newly admitted component, which
  signal admitted it, so that a declared dependency and a script-body
  reference are distinguishable.
- **FR-006**: The system MUST NOT change the identity, version, licence or
  annotations of any component it emits today.
- **FR-007**: The system MUST NOT change the dependency edges it emits today
  except by adding edges for components that did not previously exist.
- **FR-008**: The system MUST record, at document scope, how many components
  were admitted by this signal — so a closure with none is distinguishable
  from a pass that did not run.

### Key Entities

- **Build-graph reference**: the record, made by nix itself, that one
  derivation's build consumed another's output.
- **Admission signal**: what caused a derivation to be emitted — a declared
  dependency attribute, or the build graph alone.
- **Reference disclosure**: the per-component statement of which signal
  admitted it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the pinned closure, the number of emitted components rises
  from **1,174 derivations' worth** by the number of newly admitted
  derivations, and that number is **measured and recorded**, not predicted.
- **SC-002**: `gzip`, `gawk`, `gnused`, `less`, `patch` and `coreutils` each
  appear as a component with the version the closure records.
- **SC-003**: **Zero** components carrying the `unreferenced` role are named
  by another derivation's recorded build inputs.
- **SC-004**: The closure root remains classified `unreferenced`.
- **SC-005**: Every component emitted before this change is byte-identical
  after it, apart from any role correction FR-003 requires.
- **SC-006**: A scan of a project with no nix closure produces a
  byte-identical document.
- **SC-007**: Disabling the feature makes a test fail.
- **SC-008**: The residual name gap against the comparison tool is
  **re-measured** and recorded. It is 32 names today, and all 23 of the names
  that document lists explicitly are in this feature's population — so the
  expected outcome is a gap near zero, and a run that leaves it substantially
  above zero is a finding rather than a pass.

## Out of scope

- **The edge undershoot** (issue #1367). Admitting components will add edges,
  but the 5,131-vs-8,230 difference has its own cause and its own issue.
- **Changing how any already-emitted component is identified.** Milestones
  1366 and 1367 did that work; this feature changes *which* components exist.
- **The `waybill:closure-unclassified-refs` disclosure (C205).** It records
  nixpkgs *dependency attributes* the reader did not consult. The attributes
  implicated here — `postInstall`, `initialPath` — are not dependency
  attributes, so C205 neither covers this nor is wrong about it.
- **Deciding whether stdenv's bootstrap stages are "dependencies" in a
  consumer's sense.** They are build inputs and will be emitted as such; a
  consumer wanting to filter them can, via the role and the disclosure.
  Grading that — a notion of how deliberate a dependency is — is a separate
  axis and a separate feature.
- **A new role value for scaffolding.** Issue #1374 is a live example of what
  a role that says more than its test supports costs; this feature adds a
  disclosure rather than a fifth role.

## Assumptions

- **The build graph is authoritative about what was consumed.** `inputs.drvs`
  is nix's own record of the derivations a build consumed. If it names a
  derivation, that derivation was a build input, whatever the declared
  attributes say.
- **More, clearly labelled, beats less.** The project's stated posture is to
  emit by default and let consumers filter, provided each component says what
  it is. This feature follows that rather than curating a list of what counts
  as a real dependency.
- **The reference signal is already read.** Milestone 1280 parses
  `inputs.drvs` to build the closure's dependency edges, so this feature
  changes which signal classification consults rather than introducing a new
  source of truth.
- **Figures are from one closure and one system.** All counts are
  aarch64-darwin at a pinned nixpkgs revision. The corpus goldens pin
  x86_64-linux, which is a different closure — a figure from one is wrong in
  membership when quoted against the other.

## Evidence

Every count was measured on 2026-11-06 and is reproducible:

- `waybill debug closure-dump --closure-json <file>` — the 101 not emitted,
  their declared names and their roles, from the reader's own functions.
- The same closure capture, cross-referenced against each derivation's
  `inputs.drvs` — 100 of 101 referenced; the six tools' naming attributes.
- `specs/1366-closure-ecosystem-purl/measurements/` — the closure capture and
  the comparison harness.
