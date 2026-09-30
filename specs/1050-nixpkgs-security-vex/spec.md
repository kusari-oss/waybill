# Feature Specification: nixpkgs security declarations as VEX

**Feature Branch**: `1050-nixpkgs-security-vex`
**Created**: 2026-09-30
**Status**: Draft
**Input**: Connect the security metadata nixpkgs itself declares into waybill VEX
output, keyed on the measurements in `specs/1050-nixpkgs-security-vex/measurements/`.

## Context

A consumer scanning a Nix-built artifact today gets two wrong answers and no
way to tell them apart.

Ask a matcher about `unzip 6.0` and it will borrow a neighbouring distro's
advisories — Alpine files `ALPINE-CVE-2014-8139`, `-8140` and `-8141` against
it. Milestone 1035 measured Nix building that same upstream version with
patches named for those same three CVEs. The matcher reports three findings
the build does not have. Ask it about `pkg:generic/unzip@6.0`, which is what
waybill actually emits for a closure member, and it reports nothing at all,
because no advisory database keys on that identity.

Meanwhile nixpkgs has been carrying its own answer the whole time.
`meta.knownVulnerabilities` is a first-party declaration by the package set
the project pins, and Nix refuses to evaluate a package carrying one unless
the build explicitly permits it. Measured: 35 of 46 sampled attributes
declare entries, 72% naming a CVE.

This feature connects that declaration to the VEX output milestone 1035
already produces, so a consumer can distinguish "nixpkgs says this is
insecure" from "this build patched it" from "nobody has said anything".

**Vulnerability match results do not belong in the SBOM.** An SBOM is a
composition snapshot; a match is a query result against a feed that moves
daily, and baking it in produces a document that is wrong within days with no
way to tell a stale claim from a current one. This feature emits VEX.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer learns what the package set itself declares (Priority: P1)

An operator scans a Nix-built project and receives VEX statements recording
every CVE that nixpkgs declares against a component in the build, attributed
to nixpkgs as the asserting party rather than to a third-party feed.

**Why this priority**: This is the whole feature. Without it the declaration
nixpkgs already ships is invisible to every consumer of a waybill SBOM, and
the only alternative — borrowing another distro's advisories — is measurably
wrong for Nix.

**Independent Test**: Scan a project whose closure contains a component
nixpkgs marks with a CVE-bearing `knownVulnerabilities` entry; assert a VEX
statement naming that CVE, with nixpkgs identified as the source.

**Acceptance Scenarios**:

1. **Given** a closure containing a component nixpkgs declares insecure with a
   CVE-bearing entry, **When** the operator scans it, **Then** a VEX statement
   names that CVE against that component.
2. **Given** a closure whose components carry no declarations, **When** the
   operator scans it, **Then** no declaration-derived statements appear and
   output is unchanged from before this feature.
3. **Given** a declaration whose text names several CVEs, **When** the
   operator scans it, **Then** each CVE produces its own statement rather than
   one statement carrying concatenated identifiers.

---

### User Story 2 - Two sources speaking about one CVE do not contradict each other (Priority: P1)

A component carries both a nixpkgs declaration naming CVE-X and a milestone-1035
patch named for CVE-X. The consumer receives a coherent answer rather than one
statement saying the build is affected and another saying it is not, with
nothing to choose between them.

**Why this priority**: Ships with US1 or not at all. A consumer that receives
a bare `affected` and a bare `not_affected` for the same subject and CVE has
been given less than nothing — they must now decide which to trust, without
the evidence that would let them.

**Independent Test**: Construct a scan where both sources speak about one CVE
on one component; assert the emitted statements are reconciled under a stated
rule and that neither source is silently discarded.

**Acceptance Scenarios**:

1. **Given** a component with a nixpkgs declaration for CVE-X and a patch named
   for CVE-X, **When** the operator scans it, **Then** the output states both
   what nixpkgs declared and what the build applied, and a consumer can tell
   which claim rests on which evidence.
2. **Given** a component with a declaration for CVE-X and a patch for CVE-Y,
   **When** the operator scans it, **Then** both are emitted independently and
   neither is treated as bearing on the other.

---

### User Story 3 - Declarations that name no CVE still reach the consumer (Priority: P2)

nixpkgs records that a component bundles a vulnerable OpenSSL, vendors an
end-of-life Electron, or has been abandoned upstream. The operator sees these,
even though no CVE identifier exists to key them on.

**Why this priority**: Measured at 28% of entries, and they describe something
no other source can. `googleearth-pro` declares "Includes vulnerable versions
of bundled libraries: openssl, ffmpeg, gdal, and proj" — an SBOM of that
package lists the package, not the OpenSSL inside it, so no version matcher
can ever reach it. Four separate packages declare a vendored end-of-life
Electron. No CVE feed carries "upstream abandoned this". Dropping these for
lacking an identifier would discard the half that is not available anywhere
else, which is the same mistake as discarding the ~90% of nixpkgs patches
that name no CVE.

**Independent Test**: Scan a project containing a component whose declaration
is prose; assert the text reaches the output and is attributed to nixpkgs.

**Acceptance Scenarios**:

1. **Given** a component whose declaration names no CVE, **When** the operator
   scans it, **Then** the declaration text is emitted and attributed.
2. **Given** a component with both CVE-bearing and prose declarations, **When**
   the operator scans it, **Then** both are emitted and neither displaces the
   other.

---

### User Story 4 - The build's acceptance of a known-insecure package is visible (Priority: P2)

An operator can see that the build contains a package nixpkgs marks insecure,
and that the build therefore accepted it.

**Why this priority**: This is a property of the build rather than of a feed,
so it does not go stale. It is also the half a CVE scanner structurally cannot
produce: it describes a decision, not a match. Lower than US1 because it is
derivable by a consumer who has US1 plus the component list.

**Independent Test**: Scan a project whose closure contains a
`knownVulnerabilities`-carrying package; assert the acceptance is recorded and
that the wording claims only what is supportable.

**Acceptance Scenarios**:

1. **Given** a closure containing a package nixpkgs marks insecure, **When**
   the operator scans it, **Then** the output records that this build accepted
   a package nixpkgs marks insecure.
2. **Given** that same scan, **When** a consumer reads the record, **Then** it
   does not claim the operator named that specific package, because a blanket
   permission is indistinguishable from a targeted one.

---

### User Story 5 - The feature degrades without hiding that it did (Priority: P3)

An operator scanning without a usable `nix`, or offline, receives a scan that
succeeds and says plainly that declarations were not consulted.

**Why this priority**: Consistent with milestones 1034 and 1035, which both
degrade with a named reason rather than failing or silently emitting less.
Silence here is actively harmful: absence of a declaration would otherwise be
indistinguishable from absence of a check.

**Independent Test**: Scan with the tier unavailable; assert the scan succeeds,
emits no declaration-derived statements, and records why.

**Acceptance Scenarios**:

1. **Given** no usable `nix`, **When** the operator scans, **Then** the scan
   succeeds and records that declarations were not consulted, with a reason.
2. **Given** the feature is not requested, **When** the operator scans, **Then**
   output is byte-identical to a scan from before this feature existed.

---

### Edge Cases

- A declaration names a CVE that a patch on the *same* component also names.
  Covered by US2 — the case the reconciliation rule exists for.
- A declaration names a CVE that a patch on a *different* component names.
  Different subjects; both stand independently.
- A declaration's text names a CVE inside prose, e.g. "CVE-2019-9501: heap
  buffer overflow, potentially allowing remote code execution". The identifier
  must be extracted without discarding the surrounding description.
- A component in an OSV-covered language ecosystem also carries a nixpkgs
  declaration. Measured: OSV already has these with version ranges, so waybill
  adds nothing on the CVE axis and must not imply it discovered something new.
- A closure contains the same package at two versions, one declared insecure
  and one not.
- `NIXPKGS_ALLOW_INSECURE=1` was set, so permission was blanket rather than
  targeted.
- The declaration is empty (`[]`), which is the healthy case for the
  overwhelming majority of packages and must produce nothing.

## Requirements *(mandatory)*

### Functional Requirements

**Reading the declaration**

- **FR-001**: The system MUST read `meta.knownVulnerabilities` for components
  it emits from a Nix build.
- **FR-002**: The system MUST extract CVE identifiers from declaration text,
  including identifiers embedded in a longer description.
- **FR-003**: The system MUST preserve the full declaration text, not only the
  extracted identifiers, because the text carries the maintainer's reasoning.
- **FR-004**: A declaration naming several CVEs MUST produce one statement per
  identifier.
- **FR-005**: An empty declaration MUST produce no output.

**Emitting**

- **FR-006**: CVE-bearing declarations MUST be emitted as VEX statements, not
  as SBOM vulnerability arrays.
- **FR-007**: Every declaration-derived statement MUST identify nixpkgs as the
  asserting party, distinguishably from a third-party advisory feed.
- **FR-008**: Declaration-derived statements MUST carry an evidence grade, as
  patch-derived statements already do.
- **FR-009**: The evidence grade for a nixpkgs declaration MUST be distinct
  from the patch-filename grade. A maintainer stating "this version is
  vulnerable" is a different and stronger kind of evidence than a filename
  containing a CVE identifier, and a consumer weighing the two must be able to
  tell them apart.
- **FR-010**: Prose declarations MUST be emitted as a per-component SBOM
  annotation, not as VEX.
  *These are composition facts, not match results. "Vendors Electron 2.0" and
  "Includes vulnerable versions of bundled libraries: openssl, ffmpeg, gdal,
  and proj" each say the same thing: there are components inside this one that
  the SBOM does not list. That is nixpkgs reporting the component graph is
  incomplete, which is what an SBOM is for, and it does not breach the rule
  against putting match results in an SBOM because no match was performed. The
  rejected alternative was a VEX statement carrying a synthetic identifier,
  which would mean fabricating a vulnerability ID and emitting invalid
  OpenVEX.*
- **FR-010a**: The annotation MUST carry the declaration text verbatim. The
  value of these entries is the maintainer's own words; paraphrasing or
  reducing them to a flag would discard what makes them worth emitting.
- **FR-011**: The count of declarations that named no CVE MUST be recorded at
  document scope, so a consumer can tell partial coverage from absence — the
  same reason milestone 1035 records the equivalent figure for patches.

**Reconciling with patch evidence**

- **FR-012**: When a nixpkgs declaration and a milestone-1035 patch statement
  concern the same CVE on the same component, the declaration MUST win in VEX:
  the system emits `affected` from the declaration and MUST NOT emit the
  patch-derived `not_affected` for that CVE.
  *A first-party maintainer statement that a version is vulnerable outranks a
  CVE identifier read out of a patch filename. Emitting a `not_affected`
  against an explicit contrary declaration would let a consumer dismiss a real
  finding on the weaker of two pieces of evidence, which is precisely the
  overclaim the graded two-statement model exists to prevent.*
- **FR-013**: Suppressing that statement MUST NOT lose the patch evidence. The
  patch remains recorded in the component's pedigree, which milestone 1035
  emits independently of VEX, so the fact that this build applied a patch
  named for that CVE stays visible to anyone reading the SBOM. What is
  withheld is only the *suppression*, not the *evidence*.
- **FR-013a**: The system MUST record at document scope that a reconciliation
  occurred and how many times, so a consumer can tell "no patch statement was
  produced" from "a patch statement was withheld".
- **FR-014**: Statements concerning different components, or different CVEs,
  MUST NOT be reconciled against each other.

**Acceptance signal**

- **FR-015**: The system MUST record that a build contains a package nixpkgs
  marks insecure.
- **FR-016**: That record MUST NOT assert that the operator named the specific
  package, since a blanket permission cannot be distinguished from a targeted
  one.

**Scope and degradation**

- **FR-017**: The system MUST NOT emit vulnerability arrays into any SBOM
  format.
- **FR-018**: The system MUST NOT query any external advisory database.
- **FR-019**: When declarations cannot be consulted, the system MUST complete
  the scan, emit no declaration-derived statements, and record a named reason.
- **FR-020**: When the feature is not requested, output MUST be byte-identical
  to output from before this feature.

### Key Entities

- **Declaration**: a single `meta.knownVulnerabilities` entry on one component.
  Has text; may name zero or more CVE identifiers. Attributed to nixpkgs.
- **Declaration-derived statement**: a VEX statement produced from a
  CVE-bearing declaration, carrying its evidence grade and its source.
- **Acceptance record**: a document-scope statement that this build contains a
  package nixpkgs marks insecure.
- **Reconciliation outcome**: what the system emitted when a declaration and a
  patch statement concerned one CVE on one component.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning a project whose closure contains declared-insecure
  components yields a VEX statement for every CVE those declarations name.
- **SC-002**: Every declaration-derived statement is attributable to nixpkgs
  and distinguishable from a patch-derived one without inspecting the text.
- **SC-003**: Declarations naming no CVE are visible in the output, and their
  count appears at document scope.
- **SC-004**: For the `unzip` case — Alpine files three CVEs that Nix patches —
  a consumer reading waybill output can determine that this build applied
  patches named for those CVEs, without consulting another distro's data.
- **SC-005**: A CVE claimed by both a declaration and a patch yields exactly
  one VEX statement for that CVE on that component — `affected`, from the
  declaration — while the patch remains present in that component's pedigree.
  A test asserts both halves, because asserting only the suppression would
  pass equally well if the patch evidence had been dropped too.
- **SC-005a**: The count of withheld patch statements appears at document
  scope, so silence is distinguishable from suppression.
- **SC-006**: With the feature unrequested, every committed corpus golden is
  unchanged.
- **SC-007**: With `nix` unavailable, the scan completes and names the reason.
- **SC-008**: No emitted SBOM gains a vulnerability array in any format.
- **SC-009**: No scan issues a request to an external advisory database, proven
  the way milestone 1035 proved the offline refusal rather than by inspection.

## Assumptions

- **Declarations require evaluation.** `meta.knownVulnerabilities` is a Nix
  attribute, so reading it means evaluating nixpkgs. This rides on the
  evaluation milestones 1034 and 1035 already established rather than adding a
  new execution path, and inherits their degradation vocabulary. A
  file-parsing route was considered and rejected: the attribute is set by
  arbitrary Nix expressions, and grepping for it would silently miss the
  computed cases while appearing to work on the literal ones.
- **Scope is the closure.** Declarations are read for components the build
  actually contains, not for every package in nixpkgs.
- **The acceptance signal needs no config discovery.** Measured: Nix refuses to
  *evaluate* a package marked insecure, so presence in a closure proves
  permission was granted. waybill never locates `permittedInsecurePackages`.
- **OSV is not a competitor here.** Measured: OSV is complete for language
  ecosystems, where this feature adds nothing, and per-distro for system
  packages, where Nix is not one of the distros. This feature fills the gap
  that absence creates; it does not duplicate OSV.
- **The existing evidence-grade type was built to extend.** Milestone 1035
  deliberately made it an enum with one variant so a second, stronger
  provenance could be distinguished rather than conflated.
- Milestone 1035's patch-derived statements are the substrate; this feature
  adds a second source to them rather than replacing them.
- **Prose declarations belong in the SBOM, not in VEX** (FR-010). Settled
  2026-09-30. They describe what is inside a component rather than asserting
  anything about a vulnerability's applicability, so the SBOM is their home
  and no identifier has to be invented to carry them.
- **A declaration outranks a patch filename** (FR-012). Settled 2026-09-30.
  The suppression is withheld; the evidence is not, because pedigree carries
  the patch regardless of what VEX says.

## Out of Scope

- Reachability or call-graph analysis.
- Vulnerability arrays in any SBOM format.
- Queries to any external advisory database, including OSV.
- The supply-chain trust declarations (`alist` — "acquired by [a company]
  distrusted by the community"). These are a different kind of claim, they
  name a party rather than a defect, and emitting them is a separate decision.
- `meta.insecure` as a signal independent of `knownVulnerabilities`.

## Dependencies

- Milestone 1035 (`--nix-closure`) for the component set and the existing VEX
  statement machinery.
- Milestone 1034 (`--nix-eval`) for the evaluation path and its degradation
  reason codes.
