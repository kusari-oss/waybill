# Feature Specification: Vendored source-tree components

**Feature Branch**: `1002-vendored-source-reader`
**Created**: 2026-09-26
**Status**: Draft
**Input**: Issue #989 (re-scoped 2026-09-26). Not to be confused with issue
#1002, which is unrelated — the milestone number and that issue number
collide by coincidence.

## Context

A project that vendors a dependency by copying its source into the tree
declares nothing about it. waybill's C/C++ readers work by reading
declarations — `find_package`, `FetchContent`, `bazel_dep` — so a
vendored copy has nothing for them to match, and the library is absent
from the SBOM entirely.

Measured on `mongodb/mongo` at `41a5752480dd`, already a corpus target:
**51 vendored directories, and 21 of the 25 that use a `dist/` layout
appear nowhere in the emitted document.** The four that do appear are
coincidental cross-ecosystem name collisions — `protobuf` resolves to
`pkg:pypi/protobuf`, `grpc` to `pkg:cocoapods/gRPC-C++` — not the
vendored source.

This is a Principle VIII false negative on the dependency set of a major
C++ project: an operator scanning it sees the Python and npm tooling and
none of the C/C++ libraries actually compiled into the product.

### What this feature is not

Issue #989 originally proposed suppressing the shared default-descent
skip set beneath vendoring roots. That was prototyped and measured:
it recovers **zero** of the missing libraries and adds 40 unwanted
components — the vendored libraries' own dev and test dependencies (19
pypi from grpc's `requirements.bazel.txt`, 16 cargo from protobuf's
`Cargo.lock`) plus one component named after a directory. The skip set
is not the defect and must not be opened globally; see #989 for the
measurement.

## Clarifications

### Session 2026-09-26

- Q: Given mongo publishes a correct SBOM that waybill currently rejects,
  what should this milestone be? → A: Fix the supplement validator defect
  first as its own change, then re-decide this feature's scope.

**Status: PARKED pending that fix.** Do not plan or implement from this
spec as written.

Investigation during clarification established three things that
invalidate parts of the draft below:

1. **`--supplement-cdx` already targets this use case**, and its module
   documentation names it explicitly ("vendored libraries shipped without
   a recognizable manifest"). mongo publishes a complete CycloneDX at
   `/sbom.json` — 50 components with hand-maintained, correct identity
   such as `pkg:github/c-ares/c-ares@cares-1_27_0`, which is better than
   anything this reader could derive.

2. **waybill cannot currently ingest it.** The scan fails closed with
   `dependencies[] references unknown bom-ref or PURL
   pkg:github/mongodb/mongo@v8.3`. That ref is the supplement's
   `metadata.component` — the document subject — and a subject-rooted
   dependency graph is the normal CycloneDX shape. m119 FR-014
   deliberately ignores the supplement's `metadata.component` (there is a
   test asserting it), which is sound on its own, but the dependency
   validator then treats any reference to it as dangling. Two individually
   reasonable rules that jointly reject valid documents.

3. **The identity this reader could derive is weaker than the draft
   assumed**, and the draft's version measurement was wrong:

   | route | identity | coverage |
   |---|---|---|
   | project's published SBOM | `pkg:github/<org>/<repo>@<ref>` | only projects publishing one |
   | this reader | name always; version via `scripts/import.sh`; **org never derivable** | any project |
   | OSV lookup to recover org | correct for **12 of 44** measured | requires network |

   `scripts/import.sh` — which mongo's vendoring policy mandates —
   provides `VERSION=` for **27 of 51** directories, not the 8 stated in
   SC-002. The draft never looked at it. Its GitHub URL names the
   *fork* (`mongodb-forks/c-ares`), never the upstream org, so
   `pkg:github/<org>/<repo>` is not derivable offline at all.

   OSV keys C/C++ as `pkg:generic/<name>` with the canonical repo in a
   GIT range, not in the PURL. `pkg:generic/c-ares` resolves to 9 vulns
   and a commit query to 11, so a generic PURL is matchable — but mongo's
   own convention and OSV's differ, and neither is derivable from the
   vendored tree alone.

When the validator fix lands, re-run clarification against the remaining
question: whether a reader is still warranted for projects that publish
no SBOM, and if so what identity it should assert.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A vendored library appears in the SBOM at all (Priority: P1)

An operator scans a project that vendors its C/C++ dependencies as
source. Every vendored library is present in the document as its own
component, identified at least by name, so the operator can see what the
product actually contains.

**Why this priority**: This is the whole gap. Presence with a weak
identity is strictly more useful than absence — an auditor can act on a
named component and cannot act on one that isn't there. Everything else
in this feature refines an identity that only exists once this story
ships.

**Independent Test**: Scan the `cpp-mongo` corpus target and assert the
document contains a component for each vendored directory. Delivers
value alone: the 21 missing libraries become visible.

**Acceptance Scenarios**:

1. **Given** a project with a vendoring root containing library
   subdirectories, **When** the operator scans it, **Then** the document
   contains one component per vendored library directory.
2. **Given** a vendored library whose version cannot be determined,
   **When** it is emitted, **Then** it carries an explicit
   machine-readable reason for the missing version rather than a guessed
   or empty one.
3. **Given** a scan of a project with no vendoring root, **When** the
   operator scans it, **Then** the emitted document is byte-identical to
   the document produced before this feature.

---

### User Story 2 - The component carries a real version where one exists (Priority: P2)

Where the vendored tree states its own version, the component carries
it, so the operator can match the library against advisories rather than
only naming it.

**Why this priority**: Version is what makes a component actionable for
vulnerability matching. It is P2 rather than P1 because measurement
shows it is available for a minority of directories, so gating presence
on it would keep most of the gap open.

**Independent Test**: Scan `cpp-mongo` and assert that the libraries
whose trees state a version carry that exact version, and that no
component carries a version the tree does not state.

**Acceptance Scenarios**:

1. **Given** a vendored library whose tree states a version, **When** it
   is emitted, **Then** the component's version is that stated value.
2. **Given** a vendored library whose tree states no version, **When** it
   is emitted, **Then** the component has no version and records why.
3. **Given** a vendored library whose stated version is a placeholder
   (for example `0.0.0` or `head`), **When** it is emitted, **Then** it
   is treated as no version rather than emitted as a real one.

---

### User Story 3 - The component carries its declared licence (Priority: P2)

Where the vendored tree ships a licence file, the component carries the
licence, so a licence-compliance review of the product covers the
vendored code.

**Why this priority**: Equal in value to version for a compliance
audience, and measurement shows it is available for more directories
than version is, so it recovers more of the gap per unit of work.

**Independent Test**: Scan `cpp-mongo` and assert the components whose
trees ship a licence file carry a licence, and that the count matches the
number of such directories.

**Acceptance Scenarios**:

1. **Given** a vendored library whose tree contains a licence file,
   **When** it is emitted, **Then** the component carries a licence
   derived from that file.
2. **Given** a licence file whose contents cannot be resolved to a known
   licence identifier, **When** it is emitted, **Then** the component
   records that a licence file exists but was not identified, rather than
   silently omitting it.

---

### User Story 4 - The component uses the library's real name (Priority: P3)

Where the vendored tree states a name that differs from its directory
name, the component uses the stated name, so it matches the upstream
project rather than a local directory convention.

**Why this priority**: Affects a small minority of directories, but when
it is wrong the component is unmatchable against any advisory source —
a wrong name is worse than a generic one.

**Independent Test**: Scan `cpp-mongo` and assert the two directories
whose stated name differs from their directory name emit under the
stated name.

**Acceptance Scenarios**:

1. **Given** a vendored library whose tree states a name differing from
   its directory name, **When** it is emitted, **Then** the component
   uses the stated name and records the directory name as evidence.

---

### Edge Cases

- A vendoring root containing a file rather than a directory, or an
  empty directory: emits no component.
- A vendored tree that is empty because the project fetches submodules
  non-recursively. Not exercised by the `cpp-mongo` fixture, where all 51
  directories are populated, so it needs a synthetic case.
- Nested vendoring: a vendored library that itself contains a vendoring
  root. The feature must not recurse without bound.
- A vendored library that is ALSO declared through a manifest the
  existing readers see, producing two components for one dependency.
  The scan must not double-count.
- Two vendored directories that resolve to the same name and version.
- A licence file present but empty or unreadable.
- A stated version that is not a version — a build-system expression, a
  placeholder, or a parse artifact.

## Requirements *(mandatory)*

### Functional Requirements

**Discovery**

- **FR-001**: The system MUST recognise a vendoring root by directory
  name and treat each immediate subdirectory as one candidate vendored
  library.
- **FR-002**: The system MUST recognise, at minimum, the vendoring-root
  names `third_party`, `thirdparty`, `3rdparty`, and `external`.
- **FR-003**: The system MUST NOT descend into a nested vendoring root
  found inside a vendored library.
- **FR-004**: The system MUST emit no component for a candidate whose
  directory contains no files.

**Identity**

- **FR-005**: The system MUST emit one component per recognised vendored
  library.
- **FR-006**: The system MUST name the component from the library's own
  stated name when the tree states one, and from the directory name
  otherwise.
- **FR-007**: When the stated name differs from the directory name, the
  system MUST record the directory name as evidence on the component.
- **FR-008**: The system MUST assign a version only from a value stated
  within the vendored tree, and MUST NOT infer, guess, or carry over a
  version from any other component.
- **FR-009**: The system MUST treat a stated version that is a known
  placeholder as absent rather than real.
- **FR-010**: When no version can be determined, the system MUST emit
  the component without a version and record a machine-readable reason,
  consistent with how waybill already reports under-determined identity.
- **FR-011**: The system MUST record, per component, which source within
  the tree supplied the name, the version and the licence, so an operator
  can audit any single value back to a file.

**Licence**

- **FR-012**: The system MUST derive the component's licence from a
  licence file within the vendored tree when one is present.
- **FR-013**: When a licence file is present but its contents cannot be
  resolved to a known licence identifier, the system MUST record that
  fact rather than omitting the licence silently.

**Boundaries**

- **FR-014**: Reading inside a vendored tree MUST be scoped to this
  feature's own identity sources. The existing dependency-manifest
  readers MUST NOT be enabled inside a vendored tree, so a vendored
  library's own development and test dependencies do not enter the
  document.
- **FR-015**: When a vendored library is also discovered by an existing
  reader through a declaration, the scan MUST emit a single component for
  it rather than two.
- **FR-016**: A scan of a project with no recognised vendoring root MUST
  produce output byte-identical to the pre-feature output.
- **FR-017**: The system MUST NOT change the shared default-descent skip
  set for any reader other than this one.

**Transparency**

- **FR-018**: The system MUST report, once per scan, how many vendored
  libraries were discovered and how many received a version and a
  licence, so the operator can see the completeness of the result without
  diffing documents.

### Key Entities

- **Vendoring root**: A directory whose immediate subdirectories are
  third-party libraries copied into the project. Recognised by name.
- **Vendored library**: One immediate subdirectory of a vendoring root,
  representing a single upstream project copied into the tree. Carries a
  name, optionally a version, optionally a licence, and the evidence for
  each.
- **Identity source**: The specific file within a vendored library from
  which a name, version or licence was read. Recorded so any emitted
  value is auditable back to its origin.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the `cpp-mongo` corpus target, the number of components
  representing vendored C/C++ libraries rises from 0 to at least 45 of
  the 51 vendored directories.
- **SC-002**: At least 8 of those components carry a version, matching
  the count of directories measured to state one.
- **SC-003**: At least 30 of those components carry a licence, against 32
  directories measured to ship a licence file.
- **SC-004**: No component carries a version that is not stated verbatim
  somewhere in its vendored tree, verified by searching the tree for each
  emitted version.
- **SC-005**: Scanning every other corpus target produces output
  byte-identical to the pre-feature output.
- **SC-006**: No component is emitted whose name is a generic directory
  term such as `dist`, `src`, or `build`.
- **SC-007**: The count of components attributable to a vendored
  library's own development or test dependencies is zero.
- **SC-008**: A scan of `cpp-mongo` completes within 1.5x the wall clock
  of the same scan before this feature.

## Assumptions

- Vendoring-root recognition is by directory name. A project that
  vendors into a differently-named directory is out of scope for this
  iteration; FR-002's list can be extended later without reshaping the
  feature.
- The measurements quoted throughout come from a single project,
  `mongodb/mongo` at the pinned corpus revision, measured 2026-09-26.
  They establish that the gap is real and size it for that project; they
  are not claimed to generalise to every vendoring convention. A second
  project should be measured during planning before the success criteria
  are treated as targets rather than observations.
- Version and licence coverage will be low. Measurement found a stated
  version for 11 of 51 directories, of which 3 are unusable placeholders,
  leaving 8 real; and a licence file for 32 of 51. The feature is
  therefore specified to deliver presence first and identity where
  available, not full identity.
- Emitting a component with a name but no version is an improvement over
  emitting nothing, consistent with how waybill already reports
  under-determined identity elsewhere.
- All 51 vendored directories in the `cpp-mongo` fixture are populated
  (measured 2026-09-26). An earlier draft of this spec asserted some
  were empty from a non-recursive fetch; that is true of the
  `python-pytorch` target, not this one. FR-004 still requires tolerating
  an empty candidate, because other projects and other fetch modes will
  produce them — but it is not exercised by this fixture, so a test for
  it needs a synthetic case.
- SC-001's headroom (45 of 51 rather than 51 of 51) covers subdirectories
  under the vendoring root that are not libraries at all, not emptiness.

## Dependencies

- The `cpp-mongo` corpus target and its pinned revision, used as the
  measurement and acceptance fixture.
- The existing mechanism for reporting under-determined component
  identity, reused by FR-010 rather than reinvented.
- The existing per-component evidence channel, reused by FR-007 and
  FR-011.

## Out of Scope

- Changing the shared default-descent skip set for any other reader
  (FR-017). The prototype measurement on #989 is the argument against it.
- Matching a vendored library against an upstream registry to recover a
  version the tree does not state.
- Detecting that a vendored copy has been patched relative to upstream.
- Vendoring conventions in ecosystems other than C/C++ that already have
  a reader covering them.

## Open Questions

- **What identifier should a vendored component carry?** There is no
  registry coordinate for "the copy of c-ares vendored inside mongo".
  This is the standing question in #952 and this feature is a concrete
  instance of it; it should be resolved during clarification rather than
  decided implicitly at implementation time.
