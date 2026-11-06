# Feature Specification: components for the closure members waybill does not name

**Feature Branch**: `1282-closure-nameless-components`
**Created**: 2026-11-06
**Status**: Draft
**Input**: Gap A — the closure derivations that carry no `pname`

## Context

The nix closure tier reads a derivation's `pname` to decide whether it becomes
a component. Measured on the pinned `nix-closure-moat` closure: **1,145 of
1,749 derivations carry no `pname`, and waybill emits nothing for any of
them** — 65% of the closure is invisible in the document. Milestone 1280 gave
the tier its dependency graph and milestone 1281 gave it licenses; both
operate on the 604 members that happen to be named.

What is missing is not uniform. The 1,145 fall into four classes
(`measurements/README.md`):

| Class | Count | What they are |
| --- | --- | --- |
| `source` | 549 | fixed-output fetches — the tarballs and patches the build consumed |
| `package` | 83 | version-bearing bootstrap packages (`mes-libc-mini-0.27.1`) |
| `scaffolding` | 267 | `writeText` output — `itoa-builder`, `setup-hook.sh` |
| `fragment` | 246 | live-bootstrap C functions — `strncpy`, `memcpy`, `malloc` |

The 549 sources are the clearest loss: these are the actual inputs to the
build, and a consumer asking "what source did this come from" gets silence.

## Clarifications

### Session 2026-11-06

- Q: Which of the 1,145 should become components? → A: **All of them**, each
  carrying a role annotation so a consumer can filter. The operator's stated
  preference is more information by default, filtered down when irrelevant,
  provided each component says plainly what it is. The risk accepted with this
  choice is that `strncpy` appears in a dependency list; the role annotation is
  what makes that legible rather than misleading.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer can see the sources a build consumed (Priority: P1)

Someone receives an SBOM produced with `--nix-closure` and asks what source
archives the build actually fetched. Today the answer is nothing: 549
fixed-output derivations — every tarball, every patch — are absent, while the
document confidently lists the 604 packages built from them.

**Why this priority**: it is the largest and most defensible part of the gap.
A fixed-output derivation is a source artifact with a checksum nix itself
recorded; omitting it discards the best-evidenced thing in the closure.

**Independent Test**: scan a fixture closure containing fixed-output
derivations and assert each becomes a component whose identity carries its
content hash.

**Acceptance Scenarios**:

1. **Given** a closure member that is a fixed-output fetch, **When** the
   document is emitted, **Then** a component exists for it whose identity
   includes the content hash nix recorded.
2. **Given** two derivations fetching **identical content**, **When** the
   document is emitted, **Then** they produce **one** component, not two.
3. **Given** two derivations whose `name` is the same but whose content
   differs, **When** the document is emitted, **Then** they produce **two**
   distinct components.

---

### User Story 2 - A consumer can tell a source from a build script (Priority: P1)

Emitting all 1,145 means a document in which `strncpy` and
`docutils-0.22.4.tar.gz` are both components. Without a way to tell them
apart, a reviewer reading the dependency list sees a C library function
presented exactly as a third-party dependency.

**Why this priority**: it is the condition on which User Story 1's scope was
accepted. The additional 596 non-source components are only defensible if each
states what it is, so a consumer can filter them out.

**Independent Test**: scan a fixture containing all four classes and assert
every emitted component carries a role, and that the roles partition the set.

**Acceptance Scenarios**:

1. **Given** a closure member that is a `writeText` build script, **When** the
   document is emitted, **Then** its component is marked as scaffolding rather
   than presented as a dependency.
2. **Given** any component this feature adds, **When** the document is
   emitted, **Then** it carries exactly one role value.
3. **Given** a consumer filtering to sources and packages only, **When** they
   apply that filter, **Then** no build scaffolding or bootstrap fragment
   remains.

---

### User Story 3 - The document stays usable at triple the component count (Priority: P2)

The feature adds roughly 1,098 components to a closure that emits about 388
today. Counts that describe the closure — the role census, graph completeness,
orphan accounting — are read by people and by the invariant harness, and all
of them move sharply for reasons that have nothing to do with the scanned
project.

**Why this priority**: a figure that silently changes meaning is worse than one
that is absent. These records exist to be compared across scans.

**Independent Test**: scan a fixture before and after, and assert each
document-scope count either excludes the new population or states that it
includes it.

**Acceptance Scenarios**:

1. **Given** a scan that emits the new components, **When** a reader inspects
   the document-scope closure record, **Then** it states how many components
   each role contributed.
2. **Given** a scan without `--nix-closure`, **When** the document is emitted,
   **Then** output is byte-identical to today.

---

### Edge Cases

- **A derivation whose `name` is empty or absent.** Identity cannot fall back
  to a name that is not there.
- **The same content reached through two different build recipes.** Content
  identity merges them; recipe identity does not. The two paths must not
  disagree about how many components exist.
- **A member that is both pname-less and already emitted by another reader**,
  reached through a path this feature does not own.
- **A closure of a system configuration** (milestone 1066), whose pname-less
  population has not been measured and may be differently shaped.
- **Scaffolding that is also a dependency edge target.** Milestone 1280's
  graph projects edges between emitted components; adding 596 nodes changes
  what the projection stops at.
- **A name that is a valid PURL segment by accident** — `libtcc1.a`, `crt` —
  where the emitted identifier must still be unambiguous.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every closure derivation that carries no `pname` MUST be emitted
  as a component.
- **FR-002**: Each such component's identity MUST be unique within the
  document. Measured: `name` alone collides for 188 of 1,145, and `source`
  alone covers 56 distinct contents, so a name-keyed identity is not
  permissible.
- **FR-003**: Where a derivation records a content hash, that hash MUST
  participate in its identity, and two derivations recording the **same**
  content hash under the same name MUST produce **one** component. Measured:
  this merges 549 derivations into 502 components, and the merge is correct —
  the same content fetched twice is one artifact.
- **FR-004**: Where a derivation records no content hash, its identity MUST
  still be unique. Measured: the derivation's own store-path hash achieves
  this for all 1,145 with zero collisions, and is available for every node.
- **FR-005**: Every component this feature adds MUST carry a role drawn from a
  closed set — source, package, scaffolding, fragment — and MUST carry exactly
  one.
- **FR-006**: The role MUST be derivable from the closure alone, without
  network access and without a maintained list of names.
- **FR-007**: A consumer MUST be able to filter the document to sources and
  packages and obtain a set containing no scaffolding and no fragments.
- **FR-008**: Where a derivation records a source URL, that URL MUST be
  carried on its component. Measured: 138 of 1,145 carry one.
- **FR-009**: The document MUST record how many components each role
  contributed, so a reader can account for the component count rather than
  inferring it.
- **FR-010**: A scan without `--nix-closure` MUST produce byte-identical
  output to today.
- **FR-011**: Components this feature adds MUST NOT displace, rename or
  deduplicate against components another reader already emitted.
- **FR-012**: The existing role census for named members (milestone 1277) MUST
  remain comparable with scans taken before this feature — either by excluding
  the new population or by stating that it includes it.
- **FR-013**: Dependency-edge projection MUST remain well-defined when the
  newly emitted members participate, and the document MUST NOT contain an edge
  naming a component that is not present.
- **FR-014**: A derivation carrying no usable name MUST still be emitted, or
  be omitted with its omission counted — silently dropping it would make the
  FR-009 counts fail to add up.

### Key Entities

- **Pname-less derivation**: a closure member that the tier reads today and
  emits nothing for. 1,145 of 1,749 on the measured closure.
- **Role**: which of the four classes a member belongs to. Derived, not
  declared — nix records no such field.
- **Identity basis**: the content hash where one exists, otherwise the
  derivation's own store-path hash. The only two values measured to be
  collision-free over this population.

## Success Criteria *(mandatory)*

All figures are the `nix-closure-moat` target at its pin, **x86_64-linux**.
The darwin closure is a different closure and its figures must be measured
rather than scaled — the error milestone 1280 made three times.

- **SC-001**: Closure components rise by approximately **1,098** on the
  measured closure: 502 from the content-addressed path and 596 from the
  recipe-addressed one.
- **SC-002**: Every added component carries exactly one role, and the four
  role counts sum to the number of components added.
- **SC-003**: No two added components share an identity.
- **SC-004**: Two scans of one closure emit identical component sets.
- **SC-005**: Filtering to sources and packages yields 585 components on the
  measured closure (502 + 83) and no scaffolding or fragments.
- **SC-006**: Every corpus target that does not scan a closure produces
  byte-identical goldens.
- **SC-007**: No component another reader emits is displaced or renamed.
- **SC-008**: Disabling the new emission makes a test fail. A population no
  test is sensitive to is not evidence that it is emitted.
- **SC-009**: The document contains no dependency edge naming a component that
  is absent from it.
- **SC-010**: The 549 fixed-output derivations yield 502 components, and a
  fixture in which two derivations fetch identical content yields one.

## Assumptions

- **The closure is the only input.** Roles and identities are derived from the
  derivation records already fetched; this feature adds no network access, no
  second evaluation and no maintained name list.
- **`pname`-bearing members are untouched.** Their identity, licensing and
  role handling are milestones 1275, 1277 and 1281's concern.
- **Four roles are enough for the measured population.** The classifier
  assigns every one of the 1,145 to exactly one of them, with no residue.
- **The derivation store-path hash is a stable identity within a document.**
  It is nix's content-address of the build recipe, so it is stable for a fixed
  closure, and it is not claimed to be stable across nixpkgs revisions.
- **Role is advisory, not a security claim.** It describes what a derivation
  looks like in the closure, not what it is trusted to do.

## Out of scope

- Licenses for the newly emitted members — milestone 1281 covers the named
  population, and nixpkgs declares nothing for most of these.
- Changing how `pname`-bearing members are identified or classified.
- Emitting the closure of a system configuration differently from a package
  closure.
- Any claim that a role value is reproducible across nixpkgs revisions.
