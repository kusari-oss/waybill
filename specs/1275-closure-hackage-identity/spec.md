# Feature Specification: a closure member that is a known Hackage package is identified as one

**Feature Branch**: `1275-closure-hackage-identity`
**Created**: 2026-11-03
**Status**: Draft
**Input**: #1282 — "`--nix-closure` appends Hackage packages as `pkg:generic` with no edges, license or CPE (65 of 278 on a fixture, 103 of 316 on a real library)"

## Context

`--nix-closure` adds every emitting derivation in a Nix build closure as a
component. For Haskell packages it names them `pkg:generic/<pname>@<version>`
and gives them nothing else — no license, no hash, no identity a consumer can
match against anything.

Meanwhile, in the **same scan, the same process, from the same cached file**,
waybill already resolves Haskell packages properly. Measured at `001fa50` on a
flake depending on `aeson` and `text`:

| | n | license | hashes | CPE | out-edges | in-edges |
|---|---|---|---|---|---|---|
| `pkg:hackage` (resolved) | 65 | 47 | 47 | **0** | 245 | 248 |
| `pkg:generic` (closure-appended) | 276 | **0** | **0** | **0** | **0** | **0** |

So this is not a missing capability. It is a pass that already works, applied
to one set of components and not the other.

The issue names four gaps. Measurement says three of them are already solved
elsewhere and one is not:

- **license and hash** — the resolution pass produces both from the pinned
  revision's package set, which is already fetched, parsed and cached
  in-process. Reaching the closure members costs no new I/O.
- **identity** — `pkg:hackage` is the type the resolved components already
  use.
- **CPE** — **zero** of the already-resolved components carry one either, so
  this is a separate gap and is explicitly out of scope here (see Assumptions).
- **edges** — available, but from a different source than #1282 implies.
  Milestone 1066 chose that closure references drive a component's *role*, not
  its `dependsOn` edges, "because a store reference is not a declared
  dependency". That decision stands. What this feature adds instead is edges
  from the dependencies the package set **declares** — genuine `dependsOn`
  semantics, from the same cached file, by the same mechanism that already
  gives the 65 resolved components their 245 out-edges. See FR-012.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - a Haskell package in the closure is identified as one (Priority: P1)

An operator scans a Nix-built Haskell project with `--nix-closure` and feeds
the result to a vulnerability scanner and a license-policy check. Today 276
components are opaque `pkg:generic` entries that match nothing in either tool.
After this story, every closure member that the pinned revision's package set
identifies exactly carries `pkg:hackage/<name>@<version>` with the license and
hash that revision declares.

**Why this priority**: it is the feature. 60 components on a small fixture, 103
on a real library, become matchable instead of opaque — and the data to do it
is already in memory.

**Independent Test**: scan the fixture flake with `--nix-closure` and assert
that the 60 closure members whose `(name, version)` is an exact pair in the
pinned package set carry `pkg:hackage` PURLs, with licenses present at the
same rate as the already-resolved components.

**Acceptance Scenarios**:

1. **Given** a closure member `aeson 2.2.4.1` whose `(pname, version)` is an
   exact pair in the pinned `hackage-packages.nix`, **When** the scan emits,
   **Then** the component carries `pkg:hackage/aeson@2.2.4.1`, the license
   that revision declares, and the SHA-256 it records.
2. **Given** the same member, **When** the scan emits, **Then** the document
   contains exactly one component for that identity — not a `pkg:generic`
   twin alongside the `pkg:hackage` one.
3. **Given** a closure member whose name is in the package set but whose
   version is not, **When** the scan emits, **Then** it keeps `pkg:generic`.

---

### User Story 2 - a name collision does not mint a wrong identity, and says so (Priority: P2)

A closure contains GNU `bash 5.3p9`, `curl 8.20.0`, `ghc 9.4.8` and
`brotli 1.2.0`. Hackage also has packages called `bash`, `curl`, `ghc` and
`brotli` — small, unrelated Haskell libraries. An operator must never be
handed `pkg:hackage/bash@5.3p9`, because that coordinate belongs to someone
else and a vulnerability feed would answer for the wrong software.

**Why this priority**: it is the half that makes US1 safe rather than merely
useful, and it is the plausible wrong implementation. Measured: **26 of 276**
components on one fixture are name collisions. The existing code comment at
the emission site already warns that "a wrong-but-well-formed PURL would be
worse than a generic one: it would collide with whatever really owns that
coordinate" — this story is that warning, enforced and tested.

**Independent Test**: assert that no component in the emitted document carries
a `pkg:hackage` PURL whose version differs from every version the pinned set
records for that name, and that each such near-miss carries an annotation
naming why it was left generic.

**Acceptance Scenarios**:

1. **Given** a closure member `bash 5.3p9` and a Hackage package `bash 0.1.8`,
   **When** the scan emits, **Then** the component is `pkg:generic/bash@5.3p9`
   and **not** any `pkg:hackage` coordinate.
2. **Given** that same member, **When** the scan emits, **Then** it carries an
   annotation recording that a same-named Hackage package exists and was
   deliberately not used, so the decision is auditable rather than silent.

---

### User Story 3 - an unresolved closure member says why (Priority: P3)

Of 276 generic members, 190 are not Haskell at all — `autoconf`, `bc`,
`CoreSymbolication`, `python3.13-idna`. Those are correctly generic. But an
operator reading the document cannot tell "waybill looked and this is not a
Haskell package" from "waybill did not look".

**Why this priority**: narrower than US1 and US2, and it is transparency
rather than identity. It matters because the first question asked of a
`pkg:generic` component is whether anything was attempted.

**Independent Test**: assert every `pkg:generic` closure member carries a
reason distinguishing "not in the package set", "name matched, version did
not", and "no version recorded".

**Acceptance Scenarios**:

1. **Given** a closure member `autoconf 2.72`, whose name is in no Hackage
   package set entry, **When** the scan emits, **Then** it carries a reason
   saying the pinned set has no such package.
2. **Given** a closure member with no version recorded by nix at all, **When**
   the scan emits, **Then** it carries a reason distinguishing that from a
   failed lookup — the lookup could not be attempted, which is a different
   fact.

---

### Edge Cases

- **A name collision whose versions coincide.** The exact-pair rule rests on
  both fields agreeing. A C library and a Hackage package sharing a name *and*
  a version would be mis-identified. FR-005 requires a second, independent
  signal for exactly this reason, and FR-006 requires the two to be measured
  against each other rather than assumed to agree.
- **A closure member with no version.** 14 of 276 on the measured fixture.
  `(name, "")` cannot be an exact pair, so it stays generic — but it must say
  that the lookup was impossible rather than unsuccessful (US3).
- **A closure member that is also a design-tier component.** The measured
  fixture already reports 47 such merges. Reclassifying a closure member to
  `pkg:hackage` creates new opportunities for a design-tier twin to collide,
  which FR-008 covers. The pre-existing duplication defect itself is #1046 and
  is not in scope.
- **The pinned revision's package set is unavailable** — offline, cache cold,
  fetch refused. Every member stays generic and the document says the pass did
  not run, rather than silently reporting 276 unresolvable components.
- **Two closure derivations for one `(pname, version)`.** Build variants
  already collapse to one component by identity; reclassification must not
  reintroduce the duplicates that collapse removed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A closure-appended component whose `(name, version)` is an exact
  pair in the pinned revision's Haskell package set MUST be emitted with a
  `pkg:hackage` PURL rather than `pkg:generic`.
- **FR-002**: Such a component MUST carry the license that revision declares,
  by the same rules the existing resolution pass uses — including preserving a
  non-canonicalisable declaration rather than dropping it.
- **FR-003**: Such a component MUST carry the content hash that revision
  records.
- **FR-004**: A component whose name appears in the package set but whose
  version does not MUST keep `pkg:generic`. Matching on name alone is
  forbidden.
- **FR-005**: Reclassification MUST require a second signal independent of the
  package set, so that identity does not rest on a single coincidence of two
  strings.
- **FR-006**: The agreement between the two signals MUST be measured and
  recorded, not assumed. A signal that disagrees with the other on any
  component is a finding to explain before it is trusted.
- **FR-007**: A component left generic MUST carry a reason distinguishing: the
  name is absent from the set; the name matches but no version does; the two
  signals disagree; no version was recorded so no lookup was possible; and the
  package set was unavailable.
- **FR-007a**: **Principle V audit for FR-007's carrier.** Audited against the
  vendored schemas at `waybill-cli/tests/fixtures/schemas/`, not from memory
  (the m1156 method). **No native construct carries "why a candidate identity
  was rejected for this component", so a parity-bridging `waybill:*` row is
  required.**
  - *CycloneDX 1.6* — `component.evidence.identity[]`
    (`componentIdentityEvidence`) is the only native construct about identity
    determination: `field` (enum including `purl`), `confidence` (number 0–1),
    `concludedValue` (string), and `methods[]` carrying `technique` (closed
    enum: `source-code-analysis`, `binary-analysis`, `manifest-analysis`,
    `ast-fingerprint`, `hash-comparison`, `instrumentation`,
    `dynamic-analysis`, `filename`, `attestation`, `other`), `confidence` and
    `value`. It grades confidence in a **concluded** value and names the
    technique used to reach it; it has no slot for a rejection reason.
    `confidence: 0` would assert "unsure what the PURL is", which collapses all
    five FR-007 outcomes into one number and says something different from
    "the pinned set has no such package" — the same rejection m1156 recorded
    for the adjacent semantic. `pedigree.notes` describes lineage from an
    ancestor.
  - *SPDX 2.3* — `Package` has `comment` (untyped free text, no schema),
    `sourceInfo` ("how the package was acquired and/or changed from the
    original source"), `primaryPackagePurpose` (closed enum of purposes:
    `OTHER`/`INSTALL`/…/`FILE`, about what a package **is**), and
    `annotations[]`. No member carries an identity-resolution outcome.
  - *SPDX 3.0.1* — `software_Package`'s effective members are `comment`,
    `description`, `summary`, `software_sourceInfo`, `software_primaryPurpose`,
    `software_additionalPurpose`, `software_packageUrl`, `externalIdentifier`,
    `externalRef` and the artifact/element bases. None expresses why an
    identifier was not concluded; the first three are untyped prose.
  - *Existing `waybill:` rows are not reusable either.* **C151
    `waybill:unresolved-reason`** is defined as emitted "iff and only if the
    same component carries `waybill:sbom-tier: "design"`", the same
    constraint #1268 hit at `generate/cpe.rs:63`; closure components are build
    tier. **C170 `waybill:haskell-version-unresolved-reason`** is a closed enum
    answering why a declared dependency has no **version**, and every
    component here has one — what is unresolved is identity.
  - *Therefore*: one new row, documented in
    `docs/reference/sbom-format-mapping.md` with this justification clause and
    landing with its parity extractor in the same change (next free id is C201
    at time of writing).
- **FR-008**: Reclassification MUST NOT produce two components for one
  identity. A reclassified closure member and an identically-identified
  design-tier component MUST reconcile to one.
- **FR-009**: When the pinned package set is unavailable, every closure member
  MUST stay `pkg:generic` and the document MUST record that the pass did not
  run.
- **FR-010**: The pass MUST NOT add network access, subprocess calls or
  filesystem reads beyond those the scan already performs. The package set is
  already fetched, parsed and cached in-process.
- **FR-011**: Whether a closure is recorded at all, which derivations it
  contains, and how their roles are classified MUST be unchanged. This feature
  changes how a closure member is *named and described*, never which members
  there are.
- **FR-012**: A reclassified component MUST carry `dependsOn` edges derived
  from the Haskell dependencies the pinned package set **declares** for it.
- **FR-012a**: Those edges MUST NOT be derived from store references.
  Milestone 1066's decision stands: a store reference is not a declared
  dependency, and it continues to drive a component's *role*. The two facts
  travel in two carriers, so neither misrepresents the other — an operator
  reading `dependsOn` is reading declared dependencies in a closure-derived
  component exactly as in a manifest-derived one.
- **FR-012b**: A component that is **not** reclassified MUST NOT gain edges.
  Edges follow the declaration, so a member the package set does not identify
  has no declaration to follow, and inventing one from its store references is
  what FR-012a forbids.
- **FR-012c**: An edge whose target is not itself a component in the document
  MUST NOT be emitted as a dangling reference, and MUST NOT be dropped in
  silence. Two branches, by whether the absent target is a GHC boot library:
  - **A boot library** MUST be carried as a versionless compiler-supplied
    component. Measured: 13 of 278 such targets, and all three distinct ones
    (`directory` ×11, `ghc-heap`, `pretty`) are boot libraries, which m926's
    `boot_libraries` already identifies from the revision's own configuration
    (research R6). Dropping these was considered and rejected: the dependency
    is real and declared, and a count that silently omits 13 edges is the
    shape #1218 was about.
  - **Anything else** MUST have its edge omitted and the omission **reported**.
    There is no basis on which to emit a component for it: the only evidence
    it exists is its name appearing in another package's dependency list, and
    minting a component from that is the same class of claim FR-004 forbids
    for identity. Measured 0 such targets on the real closure, so this is the
    branch that must not be left to chance rather than the common one.

  An earlier draft of this requirement said simply "MUST carry the target as a
  versionless compiler-supplied component", which the second branch cannot
  satisfy without inventing a package. The two-branch form is what the
  implementation does and what the tests assert.
- **FR-013**: Every component whose identity this feature changes MUST record
  that the closure tier is where it came from, so a consumer can tell a
  closure-derived `pkg:hackage` component from a manifest-derived one.
- **FR-013a**: **Principle V audit for FR-013's carrier: no new field.** The
  signal already exists and is already audited. **C182
  `waybill:closure-role`** is emitted only under `--nix-closure` and therefore
  sits on exactly this population, and **C5 `waybill:sbom-tier`** records the
  tier; both carry their own Principle V audits in
  `docs/reference/sbom-format-mapping.md`. FR-011 keeps both unchanged through
  reclassification, which is what makes FR-013 a property to **preserve and
  assert** rather than one to add — and preservation through a PURL rewrite is
  exactly the kind of claim #980 showed can fail silently.
- **FR-014**: Every requirement MUST be covered by a test that fails when the
  behaviour is removed, mutation-tested rather than assumed. The negative case
  — FR-004's forbidden name-only match — MUST have its own test, because an
  over-matching implementation produces a confidently wrong identity, which is
  worse than the opaque one it replaces.
- **FR-015**: Existing wire values MUST NOT change meaning. A consumer reading
  the current document's `pkg:hackage` components must read the new document's
  the same way.

### Key Entities

- **Closure-appended component**: a component the closure tier adds because a
  derivation in the build closure emits it, as distinct from one a manifest
  declared. Today always `pkg:generic`.
- **The pinned package set**: the Haskell package definitions at the revision
  `flake.lock` pins — 19,437 `(pname, version)` pairs at the measured
  revision. Already fetched, parsed and cached per revision.
- **Exact pair**: a `(name, version)` present in the pinned set with both
  fields agreeing. The unit of evidence FR-001 acts on.
- **Name collision**: a closure member whose name is in the pinned set under a
  different version. 26 of 276 measured; the population FR-004 protects.
- **Corroborating signal**: whatever second, package-set-independent fact
  FR-005 requires before an identity is minted. Realised as the **GHC-built
  signal** — the derivation's `nativeBuildInputs` referencing a GHC store path
  — which is the name plan.md, the contracts and the tasks use for it
  (research R5).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the measured fixture, **59 of the 60** closure members whose
  `(name, version)` is an exact pair in the pinned set carry `pkg:hackage`
  PURLs. Measured today: 0 of 60. The 60th is `hashable 1.4.7.0`, which the
  pinned set holds only under the versioned alias `hashable_1_4_7_0`: the
  default-attribute rule reaches 59, and the reverse `pname → attributes`
  index that would reach the 60th is deferred because it introduces a new
  ambiguity when two aliases share a version (research R4, plan Risks). The
  gap is measured and stated rather than discovered — a criterion of 60 would
  report a milestone that met its design as having failed. *(Superseded
  2026-11-12 by #1282: the alias lookup was adopted once the ambiguity was
  measured as absent; see research R4.)*
- **SC-002**: Those components carry licenses at no lower a rate than the
  already-resolved components do. Measured today: 0 of 60 closure members,
  against 47 of 65 resolved ones.
- **SC-003**: Zero components in the emitted document carry a `pkg:hackage`
  PURL whose version is absent from the pinned set for that name. The 26
  measured collisions — `bash 5.3p9`, `curl 8.20.0`, `ghc 9.4.8` among them —
  all stay generic.
- **SC-004**: Every remaining `pkg:generic` closure member carries a reason.
  Measured today: 0 of 276.
- **SC-005**: The document contains no two components sharing one identity.
- **SC-005a**: Reclassified components carry out-edges, and every edge target
  is a component present in the document. Measured today: 0 out-edges from any
  closure-appended component.
- **SC-005b**: No `dependsOn` edge in the document is derived from a store
  reference. The role classification remains the only thing store references
  drive, and its counts are unchanged (SC-006).
- **SC-006**: A flake that produces a closure today produces the same closure,
  with the same derivation count and the same role classification.
- **SC-007**: Every other ecosystem's output is byte-identical.
- **SC-008**: Every requirement is covered by a test that fails when the
  behaviour is removed, mutation-tested.
- **SC-009**: Every probe committed under `measurements/` re-runs and
  reproduces its recorded finding.

## Clarifications

### Session 2026-11-03

- Q: Should reclassified Hackage closure members get `dependsOn` edges, and of
  what kind? → A: **Edges from the dependencies the pinned package set
  declares** — not from store references. Milestone 1066's role model is
  untouched: store references still drive roles, declared dependencies drive
  edges, and the two facts stay in two carriers. The mechanism is proven by the
  65 already-resolved components, which carry 245 out-edges from this same
  source.

## Assumptions

- **CPE is out of scope.** #1282's title names it, but **zero** of the 65
  already-resolved `pkg:hackage` components carry a CPE either, so it is not
  something this feature inherits by fixing identity — it is a separate gap
  affecting Hackage components generally. Scoping it here would mean building
  a Hackage-to-CPE mapping, which is its own feature and belongs with #1245.
- **#1046 stays separate.** #1282 states the distinction itself: #1046 covers
  a closure member duplicating a design-tier twin, this covers members with no
  twin at all. FR-008 keeps *this* feature from creating new duplicates; it
  does not fix the existing ones.
- **`pkg:hackage` is the right type**, and #952 ("is there a workable PURL
  identity for Nix?") does not gate this. The 65 resolved components already
  use `pkg:hackage` and have since milestone 926; this feature makes more
  components consistent with an existing choice rather than making a new one.
- The measured fixture is representative enough to size the work but is **one
  project on one platform at one revision**. The ratios (60 exact, 26
  collisions, 190 absent) are what planning is sized against; #1282 also
  measured 103 exact of 316 on a real library, which is the same order.
- The pinned package set remains the authority on Haskell identity, as it has
  been since milestone 926. This feature does not introduce a second source of
  truth for versions, licenses or dependency edges.
- **#1282's edge comparison is answered, not matched.** sbomnix emits 8,230
  edges on the same project by treating store references as dependencies. This
  feature will emit far fewer, because it only emits declared ones. A smaller
  honest graph is the intended outcome; if the count is the measure, this
  feature will look worse than sbomnix and be more correct.
