# Feature Specification: Resolve Haskell dependency versions through the pinned nixpkgs

**Feature Branch**: `926-nixpkgs-haskell-versions`
**Created**: 2026-09-24
**Status**: Draft
**Input**: User description: "947"

A Nix-built Haskell project emits every dependency at design tier with no
version, because its `.cabal` files declare ranges and it ships no
`cabal.project.freeze`. Those versions are not unknown: they are determined by
the nixpkgs revision that `flake.lock` already pins, and waybill now reads that
lockfile (#946) without ever consulting what it points at.

All nixpkgs behaviour cited below is observed, not estimated. See
`measurements/` for the probe and `measurements/README.md` for the numbers.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A Nix-built Haskell project gets real versions (Priority: P1)

An operator scans a Haskell repository that builds through Nix and ships no
Haskell lockfile. Today every dependency appears versionless at design tier,
so nothing downstream can match an advisory against it. After this feature the
dependencies that nixpkgs actually pins appear at source tier with their exact
version and source hash, and the ones that cannot be pinned that way stay
versionless with a recorded reason.

**Why this priority**: This is the entire value of the feature. A versionless
component is invisible to vulnerability matching, which is the main thing
consumers do with an SBOM. Measurement M1 confirms the pinned revision yields
both version and source hash.

**Independent Test**: Scan a fixture repository whose `flake.lock` pins a
nixpkgs revision and whose `.cabal` declares ranges only. Assert that
dependencies present in that revision's package set carry an exact version,
and that each such component records the revision it was resolved through.

**Acceptance Scenarios**:

1. **Given** a repository with a `flake.lock` pinning nixpkgs and a `.cabal`
   declaring `vector >= 0.12 && < 0.14` and no freeze file, **When** the
   operator scans it with nixpkgs resolution active, **Then** the `vector`
   component carries the exact version that revision pins and is no longer
   design tier.
2. **Given** the same scan, **When** the operator inspects that component,
   **Then** it carries the source hash nixpkgs records for it.
3. **Given** a repository with no `flake.lock`, **When** the operator scans it,
   **Then** output is byte-identical to a scan before this feature.

---

### User Story 2 - A boot library is reported honestly, never invented (Priority: P1)

Some declared dependencies ship with the compiler rather than being built from
Hackage. nixpkgs marks these by binding them to `null` in the per-compiler
configuration, and they genuinely have no version in the package set. The
operator must be able to tell "this has no version because it comes from the
compiler" apart from "waybill did not look".

**Why this priority**: Equal to US1 under Constitution Principle IX. The
feature is only trustworthy if the packages it cannot resolve are visibly
distinguished from the ones it did not try to resolve. Inventing a version for
a boot library would be worse than the current versionless state, because it
would look authoritative.

**Independent Test**: Scan a fixture declaring both an ordinary Hackage
dependency and a boot library. Assert the first carries a version, the second
does not, and the second carries a machine-readable reason naming the boot
mechanism.

**Acceptance Scenarios**:

1. **Given** a declared dependency the pinned revision binds to `null` for the
   relevant compiler, **When** the operator scans, **Then** the component
   remains versionless and carries a reason identifying it as a compiler-
   supplied boot library.
2. **Given** that same component, **When** the operator inspects it, **Then**
   no version, no source hash and no PURL version qualifier has been
   synthesised for it.

---

### User Story 3 - The operator can tell where a version came from (Priority: P2)

A version resolved through nixpkgs is evidence of what the *Nix build* would
use. That is not identical to what a `cabal build` would use, and a consumer
comparing two waybill documents must be able to see the difference rather than
find two bare version strings of unequal provenance.

**Why this priority**: Constitution Principle X. It does not block the value in
US1/US2, but without it the document overstates what it knows.

**Independent Test**: Scan two fixtures — one resolved from a
`cabal.project.freeze`, one resolved through nixpkgs — and assert their
components carry different, machine-readable resolution provenance.

**Acceptance Scenarios**:

1. **Given** a component whose version came from the pinned nixpkgs, **When**
   the operator inspects it, **Then** it records that the version is
   nixpkgs-resolved and names the revision it came from.
2. **Given** a component whose version came from a Haskell lockfile, **When**
   the operator inspects it, **Then** its provenance is distinguishable from
   the nixpkgs-resolved case.

---

### Edge Cases

- **The pinned revision is unreachable.** Network failure, a deleted or
  rewritten revision, or a rate limit. The scan MUST complete and fall back to
  today's versionless design-tier behaviour rather than fail or hang.
- **The declaring project selects more than one compiler.** The #947 target's
  flake offers three GHC package sets. Which one was built is not recorded in
  `flake.lock`. See FR-005 and the clarification below.
- **The boot-library set differs by compiler.** Measured: 40 nulled attributes
  at GHC 9.4.x and 9.6.x, 41 at 9.10.x, with a five-package symmetric
  difference (M2). A fixed list would be wrong.
- **The nixpkgs layout changes.** `hackage-packages.nix` is a generated file at
  a path that is not a stable public interface. If it is absent or unparseable
  at the pinned revision, behave as unreachable rather than emit partial
  results.
- **A declared dependency appears in neither the package set nor the nulled
  set.** It stays versionless with a reason distinct from the boot-library
  reason.
- **The flake pins nixpkgs by a moving reference.** If the lock records no
  exact revision, there is nothing reproducible to resolve against; behave as
  unreachable. (#946 already annotates this case as `default-branch` /
  `branch-or-tag`.)
- **Repeated scans.** Re-scanning the same revision must not re-fetch, and must
  produce identical output.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST resolve a declared Haskell dependency to the
  exact version recorded for it by the nixpkgs revision pinned in the
  project's `flake.lock`, when that revision's package set contains it.
- **FR-002**: The system MUST record the source hash that revision carries for
  a resolved dependency, alongside the version.
- **FR-003**: The system MUST promote a dependency resolved this way out of
  design tier, and MUST leave unresolved dependencies at their current tier.
- **FR-004**: The system MUST determine the set of compiler-supplied boot
  libraries by reading the pinned revision's per-compiler configuration, and
  MUST NOT rely on a fixed built-in list of boot-library names. *(Measurement
  M2: the set differs across GHC series at a single revision.)*
- **FR-005**: The system MUST NOT synthesise a version, source hash or
  versioned identifier for a dependency it could not resolve, including every
  boot library. *(Constitution Principle IX.)*
- **FR-006**: The system MUST record, for every declared dependency it did not
  resolve, a machine-readable reason distinguishing at minimum: supplied by the
  compiler; absent from the pinned package set; and the pinned revision could
  not be consulted.
- **FR-007**: The system MUST record, for every dependency it did resolve, that
  the version is nixpkgs-resolved and which revision it came from — in a form
  distinguishable from a version taken from a Haskell lockfile.
- **FR-008**: The system MUST complete the scan and degrade to current
  behaviour when the pinned revision cannot be consulted, and MUST record that
  degradation at document scope rather than failing.
- **FR-009**: The system MUST NOT consult the network when the operator has
  requested offline operation.
- **FR-010**: The system MUST reuse a previously retrieved revision rather than
  retrieving it again, keyed by the exact pinned revision.
- **FR-011**: Two scans of the same repository at the same pinned revision MUST
  produce identical output.
- **FR-012**: A repository with no `flake.lock`, or whose lock pins no exact
  revision, MUST produce output byte-identical to a scan before this feature.
- **FR-013**: A Haskell version already established by a project-local
  lockfile or freeze file MUST take precedence over a nixpkgs-resolved version,
  and the system MUST record when the two disagree rather than silently
  preferring one.
- **FR-014**: When the project selects more than one compiler and no single
  one can be established, the system MUST [NEEDS CLARIFICATION: see Q1 —
  emit boot libraries versionless with the candidate compilers recorded, emit
  one component per compiler, or require an operator-supplied selection].
- **FR-015**: Resolution through the pinned revision MUST be
  [NEEDS CLARIFICATION: see Q2 — active by default, or opt-in behind an
  operator flag].

### Key Entities

- **Pinned revision**: The exact nixpkgs commit recorded in the project's
  `flake.lock`. The unit of caching and the provenance recorded on every
  resolved version.
- **Package set**: The mapping from package name to version and source hash
  carried by a pinned revision. Measured at 19,058 entries (M1).
- **Compiler configuration**: The per-GHC-series record of which packages are
  supplied by the compiler rather than built from Hackage. Measured at 40–41
  entries, differing by series (M2).
- **Resolution outcome**: Per declared dependency — either a version plus
  source hash plus the revision it came from, or a reason it has none.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a Haskell repository whose `flake.lock` pins an exact nixpkgs
  revision and which ships no Haskell lockfile, every declared dependency that
  the revision's package set contains and that the relevant compiler
  configuration does not null out carries an exact version and a source hash.
  Measured against the baseline for that repository today: **zero** such
  versions.
- **SC-002**: Every declared dependency without a version carries a reason
  code, and the count of versionless-without-a-reason dependencies is zero.
- **SC-003**: No component carries a version, source hash, or versioned
  identifier that does not appear in the pinned revision or a project-local
  lockfile — verified by comparing every emitted Haskell version against those
  two sources.
- **SC-004**: Two consecutive scans of the same repository at the same pinned
  revision produce byte-identical documents.
- **SC-005**: A second scan of a revision already retrieved performs no
  retrieval of that revision.
- **SC-006**: A scan whose pinned revision cannot be consulted completes with
  the same component and relationship counts as a scan before this feature, and
  records the degradation at document scope.
- **SC-007**: A repository with no `flake.lock` produces a document
  byte-identical to one produced before this feature.

Deliberately **not** a success criterion: any fixed "N of 19 dependencies
resolved" figure. #947 reports 12 of 19; the probe measures 10 of 19 against a
reconstructed dependency list and establishes that the difference (`deepseq`,
`transformers`) is on the boot-library side. Neither source enumerates the
target's dependencies, so the split is unverified in both directions and cannot
be a criterion. See `measurements/README.md` §M3.

## Assumptions

- **The pinned revision is retrieved, not evaluated.** Reading the generated
  package set at an exact revision keeps the tool pure Rust with no host
  dependency (Constitution Principle I). Evaluating the package set with the
  host's `nix` would be exact but adds a host-tool dependency and a second
  behaviour that only some users can reproduce. #947 raises both; this spec
  assumes the former.
- **Caching follows the existing per-revision pattern.** The m090 fixture
  cache, m108 fingerprint cache and m195 corpus cache already key a local cache
  by a pinned SHA; FR-010 assumes that shape rather than a new mechanism.
- **The retrieved artifact is large.** Measured at 16,634,427 bytes for one
  revision (M1). FR-010 exists because of this, and it informs Q2.
- **`hackage-packages.nix` is a generated file, not a stable interface.** Its
  path and shape can change without notice, which is why the probe is committed
  and why FR-008 treats an unparseable file as unreachable.
- **Boot-library versions are a property of the compiler**, not of the package
  set, and are therefore not obtainable from the package set at all. This is
  the same set `cabal v2-freeze` declines to pin (#938). Two independent tools
  declining for the same reason is treated as a real constraint, not a gap to
  paper over.
- **The measurement target is referred to by its pinned revision, not by name**,
  per the project's external-name policy. The nixpkgs half of every measurement
  is reproducible from the revision alone.

## Dependencies

- **#946 (merged)** — reading `flake.lock`, including the exact-revision vs
  moving-reference distinction FR-012 relies on.
- **#938 (merged)** — established that GHC boot libraries are exactly what
  `cabal v2-freeze` declines to pin.
- Out of scope: **#952** (whether a Nix-shaped PURL identity exists). This
  feature emits Haskell package identities, which are already well-formed; it
  does not depend on that research.

## Out of Scope

- Any change to how non-Haskell nixpkgs packages are identified or emitted.
- Resolving Haskell versions for projects that do not build through Nix.
- Evaluating the flake, building anything, or invoking a host `nix`.
- Inferring which compiler was used from build outputs or the filesystem.
