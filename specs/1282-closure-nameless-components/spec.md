# Feature Specification: components for the closure members waybill does not name

**Feature Branch**: `1282-closure-nameless-components`
**Created**: 2026-11-06
**Status**: Draft
**Input**: Gap A — the closure derivations that carry no `pname`

## Context

Measured on the pinned `nix-closure-moat` closure: **1,145 of 1,749
derivations carry no `pname`, and waybill emits nothing for any of them** —
65% of the closure is invisible in the document.

**What suppresses them is not a `pname` test.** `RawDerivation::pname()` falls
back to the derivation's own `name`, and every one of the 1,145 names is
non-empty, so that accessor returns `Some` for all of them. The gate is
`emit::emits_as_component`, which admits a member on its milestone-1277 C182
role — `artifact-input`, `build-tooling` or `both`, plus the `unreferenced`
members that apply a patch. All 1,145 are `unreferenced` non-patch-appliers.
Read from the golden: 1,315 `unreferenced` derivations yield **11**
components. Any implementation that edits a `pname` test will change nothing. Milestone 1280 gave
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

Baselines, read from the committed golden for the pinned target rather than
recalled: **381 components emitted**, **3,459 dependency edges**, and a role
census of `{artifact-input: 268, both: 45, build-tooling: 121, unreferenced:
1315}` totalling 1,749 — the census counts derivations, not components.

## Clarifications

### Session 2026-11-06

- Q: Where should the distinguishing hash live in the emitted identifier? →
  A: **In the name segment.** PURL equality is how downstream tooling matches
  components, and some tools compare without qualifiers. A hash carried only
  as a qualifier would let 56 distinct `source` artifacts compare equal in
  such a tool — re-creating precisely the collision FR-002 exists to prevent.
  The human-readable name is preserved in the component's own name field, so
  nothing is lost to a reader.
- Q: How should the existing document-scope counts treat the 1,145 new
  components? → A: **Leave them alone; add a separate record.** The question
  was put on a false premise — that the milestone-1277 census describes only
  `pname`-bearing members. It does not: it counts **derivations**, totals
  1,749 on the pinned closure, and already covers the whole pname-less
  population (1,315 of them `unreferenced`). So emitting them as components
  cannot change it, and the answer costs no work: the census stays untouched
  and the four-role component record is additive beside it.
- Q: Do the newly emitted scaffolding and fragment components participate in
  dependency edges? → A: **`source` and `package` do; `scaffolding` and
  `fragment` do not.** The projection gains `source` and `package` as stopping
  points, so the emitted graph shows which source a package was built from.
  513 components — scaffolding and fragments — are then orphans by
  construction and need a reason that says so.

  **The graph changes, and that is accepted.** An earlier wording of this
  answer also claimed the measured shape was preserved; measurement shows the
  two cannot both hold. Stopping earlier replaces wide transitive fan-outs
  with direct edges: a derivation-level proxy over the pinned closure moves
  from 12,013 edges to 7,755, −35%. The milestone-1280 component-level figure
  will move too, in the same direction, and must be re-measured rather than
  predicted from the proxy.
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

The feature adds roughly 1,098 components to a closure that emits 381
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
- **A scaffolding member that another member genuinely depends on.** FR-013
  keeps it out of the graph, so the dependency is expressed by the projection
  walking through it — the same treatment a non-package derivation receives
  today. The member is still emitted as a component; it is simply never an
  endpoint.
- **A source reached only through scaffolding.** The walk passes through the
  scaffolding and stops at the source, so the edge names the source. This is
  the case that makes the graph change shape rather than merely grow.
- **A derivation whose name is absent or unusable in an identifier.** Measured
  as zero occurrences, so FR-017 is defensive; it must still not drop the
  member, because the FR-009 counts would then fail to reconcile.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every closure derivation that carries no **declared** `pname`
  MUST be emitted as a component, regardless of its milestone-1277 C182 role.
  The gate this relaxes is the C182 role filter, not a `pname` test, and it
  must be relaxed at **both** sites that consult it — the component builder
  and the edge-endpoint key set — because they are two callers of one
  predicate.
- **FR-002**: Each such component's identity MUST be unique within the
  document, and MUST remain unique under a comparison that ignores identifier
  qualifiers. A distinguishing value carried only as a qualifier does not
  satisfy this: tooling that compares without qualifiers would see one
  component where the closure holds many. Measured: `name` alone collides for
  188 of 1,145, and `source` alone covers 56 distinct contents, so a
  name-keyed identity is not permissible.
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
  packages and obtain a set containing no scaffolding and no fragments. The
  filter is over the components **this feature adds**: a component carrying no
  member role is one the closure tier already emitted and is not what this
  requirement excludes. A filter expressed as "role is `source` or `package`"
  alone would discard all 381 named packages, which is the opposite of what a
  consumer asking for sources and packages wants.
- **FR-008**: Where a derivation records a source URL, that URL MUST be
  carried on its component. Measured: 138 of 1,145 carry one.
- **FR-009**: The document MUST record how many components each of the four
  roles contributed, in a record distinct from the milestone-1277 census. A
  reader MUST be able to reconcile the tier's component count as **the count
  of components carrying no member role, plus the four role counts**. The
  C182 census is *not* one of those addends: it counts derivations (FR-012),
  so summing it with a component record reconciles nothing.
- **FR-010**: A scan without `--nix-closure` MUST produce byte-identical
  output to today.
- **FR-011**: Components this feature adds MUST NOT displace, rename or
  deduplicate against components another reader already emitted.
- **FR-012**: The existing milestone-1277 role census MUST be left exactly as
  it is. It counts **derivations**, not components — measured on the pinned
  closure it totals 1,749 and already covers the whole pname-less population,
  most of them under `unreferenced` (1,315). Emitting those derivations as
  components therefore cannot change it, and nothing in this feature may
  reclassify them to make it appear to change. The new four-role record
  (FR-009) counts components and is strictly additive beside it; the two
  answer different questions and neither replaces the other.
- **FR-013**: Only `source` and `package` components MAY be dependency-edge
  endpoints. The projection MUST stop at them, and MUST continue to walk
  *through* `scaffolding` and `fragment` members rather than stopping at them.
  The document MUST NOT contain an edge naming a component that is not
  present. The emitted edge set changes as a result; that is intended, and
  FR-018 governs how the change is reported.
- **FR-014**: A component this feature adds that no edge can reach MUST carry
  a reason saying why. Measured consequence of FR-013: 513 of the added
  components are unreachable by construction, and an unexplained orphan is
  indistinguishable from a defect in the graph — which is the distinction
  milestone 1218 exists to preserve.
- **FR-015**: Graph-completeness and orphan accounting MUST state which
  population they describe, so a reader can tell a closure whose graph is
  incomplete from one that merely gained components this feature excluded from
  edges by design (FR-013).
- **FR-016**: The component's human-readable name MUST remain the derivation's
  own name, unmodified, even where the identifier incorporates a hash. The
  identifier carries uniqueness; the name carries legibility, and a reader
  must not have to read a digest to learn what something is.
- **FR-017**: A derivation whose name is absent, empty, or not usable in an
  identifier MUST still be emitted, identified by its derivation hash alone,
  and MUST NOT be silently dropped — an omission would make the FR-009 counts
  fail to reconcile. Measured: zero such derivations occur in the closure
  under test, so this is a defensive requirement rather than an observed
  case.
- **FR-018**: Where the emitted edge set differs from what the same closure
  produced before this feature, the document MUST carry the new figure rather
  than leaving a reader to infer it from an unexplained change. A graph that
  silently changes size is indistinguishable from one that broke.
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
- **SC-003**: No two added components share an identity, including under a
  comparison that ignores identifier qualifiers.
- **SC-004**: Two scans of one closure emit identical component sets.
- **SC-005**: Filtering the added population to sources and packages yields
  585 components on the measured closure (502 + 83) and no scaffolding or
  fragments. Applied to the whole document the same filter also keeps the 381
  components carrying no member role (FR-007).
- **SC-006**: Every corpus target that does not scan a closure produces
  byte-identical goldens.
- **SC-007**: No component another reader emits is displaced or renamed.
- **SC-008**: Disabling the new emission makes a test fail. A population no
  test is sensitive to is not evidence that it is emitted.
- **SC-009**: The document contains no dependency edge naming a component that
  is absent from it.
- **SC-010**: The 549 fixed-output derivations yield 502 components, and a
  fixture in which two derivations fetch identical content yields one.
- **SC-011**: The emitted dependency-edge count on the measured closure is
  re-measured and recorded after the change, and every edge resolves to a
  component present in the document. No criterion asserts the count is
  unchanged: FR-013 stops the walk at sources and packages, which measurably
  reshapes the graph.
- **SC-012**: Every added component unreachable by an edge carries a reason,
  and that reason is the one this feature adds rather than an inherited
  blanket explanation. The count is **at least** the number of `scaffolding`
  plus `fragment` components emitted — 513 on the pinned closure, which are
  orphans by construction — and any excess is a `source` or `package`
  component nothing consumes. The excess MUST be measured rather than assumed
  to be zero; an equality here would assert something no measurement
  establishes.
- **SC-013**: The milestone-1277 role census is numerically identical before
  and after this feature on the same closure — `{artifact-input: 268, both:
  45, build-tooling: 121, unreferenced: 1315}` on the pinned target, totalling
  1,749. It counts derivations, so this holds by construction; the criterion
  exists to catch a change that reclassifies them as a side effect.
- **SC-014**: The 56 closure members named `source` yield 56 distinct
  components whose identifiers differ without reference to any qualifier.
## Assumptions

- **The closure is the only input.** Roles and identities are derived from the
  derivation records already fetched; this feature adds no network access, no
  second evaluation and no maintained name list.
- **`pname`-bearing members are untouched.** Their identity, licensing and
  role handling are milestones 1275, 1277 and 1281's concern. This is
  deliberately asymmetric and worth stating plainly: after FR-001 an
  `unreferenced` member with no declared `pname` is emitted, while an
  `unreferenced` member that declares one is still suppressed unless it
  applies a patch. Roughly 170 members sit on the suppressed side. Emitting
  them is a different question about a population this feature did not
  measure, and SC-001's figure assumes they stay as they are.
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
