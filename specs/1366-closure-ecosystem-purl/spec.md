# Feature Specification: an ecosystem identity for the closure components a feed could match

**Feature Branch**: `1366-closure-ecosystem-purl`
**Created**: 2026-11-06
**Status**: Draft
**Issue**: #1365 — *Nix closure: 781 of 942 components carry pkg:generic, so no vulnerability feed can match them*

> **Numbering note.** Spec directory numbers and GitHub issue numbers are two
> separate sequences in this repository and do **not** correspond. This is spec
> **1366**; it implements issue **#1365**. Issue #1366 is an unrelated feature
> (the pname-bearing `unreferenced` coverage gap). Cite the issue, never the
> bare number.

## Context

waybill emits a component for `urllib3 2.6.3`. It is correctly named,
correctly versioned, and **no vulnerability feed can match it**, because its
identifier is `pkg:generic/urllib3@2.6.3` and `pkg:generic` carries no
ecosystem. The same holds for `requests`, `lxml`, `jinja2`, `certifi` and
every other Python package in a nix closure.

This is not a coverage problem. The packages are present. The document
describes them accurately in every respect except the one a consumer uses to
ask "is this build affected by CVE-X".

Measured on the pinned `nix-closure-moat` closure (aarch64-darwin, nixpkgs
`a799d3e3`, 1,275 derivations), **781 of 942 components carry
`pkg:generic`**, decomposing as:

| bucket | count | correct identifier |
| --- | --- | --- |
| closure members — patch files, source tarballs, build fragments | **539** | `pkg:generic` **is correct**; changing it is a regression |
| Python packages | **83 derivations / 75 names** | an ecosystem-qualified identifier |
| C and system packages (`openssl`, `curl`, `zlib`, `apple-sdk`) | **150** | no PURL ecosystem exists — a CPE |
| build hooks, compilers and interpreters | 16 | `pkg:generic` is correct |

So the actionable population is ~226, and this feature addresses the **75
Python names**. The 150 needing a CPE are a different carrier and a different
decision; see *Out of scope*.

The comparison tool is not ahead here — its components use a PURL type the
specification does not define, or carry no identifier at all. Neither document
answers a vulnerability query today. This is open ground rather than a
deficit.

## Clarifications

### Session 2026-11-06

- Q: How is an ecosystem candidate confirmed as a published package?
  → A: **Confirm via the existing enrichment resolution path.** It already
  resolves coordinates in the target ecosystem, already normalises names to
  the ecosystem's own form, and already caches its results on disk.

**What this buys — RETRACTED, see the measurement.** This section originally
read: *"`librt` has no published project of that name and `roman-numerals` is
published under a different one; both fail to resolve and are declined, which
is US2 made true by construction rather than hoped for."* Measured against
deps.dev on 2026-11-06, **both resolve**, and all 75 candidates confirm —
`librt@0.9.0` is mypyc's runtime and `roman-numerals@4.1.0` is a real project,
so both are correct matches rather than false positives. The claim was
inferred, not measured; `librt@1.0.1`, a version this closure does not
contain, does 404, which is how a version-shaped guess produces a confident
wrong answer.

What confirmation actually buys on the measured population is therefore
**nothing**: it declines 0 of 75. It remains correct as a requirement — a
different closure can hold a genuinely local name — but the benefit that
priced the network dependency below is not present here, and US2's mechanism
has no witness on this closure. See
`measurements/README.md` and `measurements/depsdev-confirmation-probe.py`.

**What it costs, stated plainly.** The closure tier has made no network call
until now. Under this answer it inherits the enrichment path's: an offline
scan confirms nothing, so FR-009 and SC-006 govern and the document is exactly
today's. That is a real behavioural change to a tier that was previously
hermetic, and it is the reason this question was not defaulted.

**What it does not establish.** Resolution confirms that a project of that
name exists at that version — necessary, and not sufficient to prove *this
derivation* is that project. For a name that is both a published project and a
plausible local one (`build`, `toml`), existence-confirmation accepts it.
Since the measurement above shows confirmation declining **nothing** on this
closure, this residual risk is not a corner case but the whole of the risk:
the feature's accuracy rests on the candidate rule, not on confirmation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer matches a closure's Python packages against a feed (Priority: P1)

Someone has an SBOM of a nix-built artifact and a vulnerability feed. They ask
the feed which components are affected. Today every Python package in the
closure is invisible to that query, because its identifier names no ecosystem.
After this feature, each one carries an identifier the feed recognises, and
the answer includes them.

**Why this priority**: It is the whole point. Every other story here is a
safeguard on this one. A component that is present but unmatchable is
indistinguishable, to the consumer asking the question, from a component that
was never emitted — which is the gap milestone 1282 just spent a milestone
closing from the other direction.

**Independent Test**: Scan the pinned closure, take the components the feature
re-identifies, and confirm each carries an ecosystem-qualified identifier
whose name and version match what the closure records. Verifiable without the
other two stories.

**Acceptance Scenarios**:

1. **Given** a closure containing a Python package nixpkgs identifies by a
   versioned interpreter prefix, **When** the closure is scanned, **Then** the
   emitted component carries an ecosystem-qualified identifier naming that
   package and version.
2. **Given** that same closure, **When** the emitted document is queried by
   ecosystem-qualified identifier, **Then** the packages are found — where
   before the query returned nothing.
3. **Given** a package whose recorded name differs from its published project
   name only by case or separator, **When** it is identified, **Then** the
   identifier uses the published ecosystem's normalised form rather than the
   recorded spelling.

---

### User Story 2 - An unconfirmed coordinate is never invented (Priority: P1)

A name that looks like a published package is not necessarily one. If the
feature emits an ecosystem identifier for a name whose project does not exist,
or exists as something else, it has asserted a coordinate that belongs to
someone else — or to nobody.

> **No candidate on the pinned closure demonstrates this.** `librt` and
> `roman-numerals` were cited here as examples until they were measured; both
> resolve, and confirmation declines 0 of 75 (`measurements/README.md`). The
> story stays — it states the property the feature must not violate — but it is
> verified against synthetic fixtures, not against this closure.

**Why this priority**: Equal to US1, because a wrong-but-well-formed
identifier is **worse than the generic one it replaced**. A generic identifier
is merely unmatchable; a wrong ecosystem identifier matches the wrong thing,
and a consumer acting on it reaches a confident false conclusion. This is the
same argument the closure emitter already makes for declining a nix-specific
PURL type.

**Independent Test**: Present the feature with a candidate that cannot be
confirmed and assert it retains today's identifier unchanged.

**Acceptance Scenarios**:

1. **Given** a candidate that cannot be confirmed as a published package,
   **When** the component is emitted, **Then** it carries exactly the
   identifier it carries today.
2. **Given** a closure of only unconfirmable candidates, **When** it is
   scanned, **Then** the emitted document is unchanged from today's.

---

### User Story 3 - A reader can tell which identities were upgraded, and on what basis (Priority: P2)

A consumer reading the document can see which components received an
ecosystem identity and what evidence supported it, rather than having to infer
it from the identifier's shape.

**Why this priority**: Below the first two because the document is already
correct without it. It earns its place because this feature changes an
identifier a consumer may already be matching on, and an unannounced identity
change is indistinguishable from a different component appearing.

**Independent Test**: Scan the pinned closure and confirm every re-identified
component carries the disclosure, and that components left generic do not.

**Acceptance Scenarios**:

1. **Given** a component the feature re-identified, **When** the document is
   read, **Then** it discloses that the identity was derived and on what
   evidence.
2. **Given** a component left with its generic identifier, **When** the
   document is read, **Then** it carries no such disclosure.

---

### Edge Cases

- **A name that is a published project, but not this one.** `build` and `toml`
  are real published projects *and* plausible local names. Confirmation has to
  distinguish "a project by this name exists" from "this is that project".
- **Two closure derivations resolving to one coordinate.** Build variants
  already merge; this must not make two genuinely different artifacts collide
  under one new identifier.
- **A package whose recorded version is not the published version.** nixpkgs
  patches and sometimes re-versions. The identifier must not claim a published
  release that does not correspond to what was built.
- **Confirmation unavailable.** Whatever confirms a candidate may be
  unreachable — offline, rate-limited, or absent. The feature must degrade to
  today's behaviour rather than guess, and must say so.
- **A closure with no Python packages.** The common case for many projects;
  the document must be unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST identify, among closure components, those that
  are packages published to a software ecosystem, using evidence recorded in
  the closure itself.
- **FR-002**: The system MUST emit an ecosystem-qualified identifier for a
  candidate **only** when that candidate is confirmed, and MUST otherwise emit
  the identifier it emits today.
- **FR-003**: The system MUST confirm a candidate against the **existing
  enrichment resolution path** before emitting an ecosystem identifier for it,
  and MUST treat "the published ecosystem has no such package at this version"
  as a decline rather than as a failure.
- **FR-004**: Identifiers MUST use the target ecosystem's own normalisation
  rules, so that a component matches a feed entry that spells the name
  differently.
- **FR-005**: The system MUST NOT change the identifier of any closure member
  — the patch files, source tarballs and build fragments for which a generic
  identifier is correct. Measured: 539 of the 781 on the pinned closure.
- **FR-006**: The system MUST NOT change the identifier, name or version of a
  component contributed by any other reader.
- **FR-007**: The system MUST disclose, per component, that an identity was
  derived and what evidence supported it (US3).
- **FR-008**: The system MUST record, at document scope, how many candidates
  were found, how many were confirmed, and how many were declined — so a
  reader can tell a closure with no Python packages from a confirmation step
  that silently did nothing.
- **FR-009**: The system MUST behave identically to today when confirmation is
  unavailable, and MUST record that this happened rather than failing the scan.
- **FR-010**: The system MUST NOT emit an ecosystem identifier for a component
  whose recorded version cannot be attributed to a published release of that
  project.
- **FR-011**: Confirmation MUST reuse the enrichment path's existing result
  cache, so that a repeated scan of one closure does not re-ask about packages
  it has already resolved.
- **FR-012**: Confirmation MUST NOT be able to fail a scan. An unreachable,
  slow or rate-limited confirmation yields FR-009's behaviour — today's
  identifiers, and a record that confirmation did not run.

### Key Entities

- **Ecosystem candidate**: a closure component the recorded evidence suggests
  is a published package — the package name, the version, the ecosystem, and
  the evidence that suggested it.
- **Confirmation outcome**: whether a candidate was confirmed, declined, or
  could not be checked, and why. Drives both the identifier and FR-008's
  record.
- **Derived identity disclosure**: the per-component statement that an
  identifier was derived rather than recorded, and on what basis.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the pinned closure, the number of components carrying an
  ecosystem-qualified identifier rises from **0** to the number confirmed, and
  that number is **measured and recorded from a waybill run**, not predicted.
  **Measured 2026-11-06 from a waybill run**: 74 candidates, **74 confirmed**,
  0 declined, 0 unchecked. `pkg:pypi` rises from 0 to 74 and `pkg:generic`
  falls 781 → 707, with the component count unchanged at 942.

  The direct deps.dev probe counted **75** candidate identities, and the
  one-component difference was investigated rather than rounded off:
  `python3.13-build-1.4.4` is `unreferenced`, so the closure emits no
  component for it at all. It belongs to the coverage gap under *Out of
  scope*, not to this feature's population — incidental confirmation that the
  two are independent. See `measurements/README.md`.
- **SC-002**: **Zero** components carry an ecosystem identifier that
  confirmation did not support.
- **SC-003**: The 539 closure members are **byte-identical** before and after.
- **SC-004**: Components contributed by other readers are byte-identical
  before and after.
- **SC-005**: A query by ecosystem-qualified identifier against the emitted
  document returns the confirmed packages, demonstrated end to end on at least
  the names with known vulnerability histories: `urllib3`, `requests`, `lxml`,
  `jinja2`, `certifi`.
- **SC-006**: With confirmation unavailable, the emitted document is
  byte-identical to today's, and the document says confirmation was
  unavailable.
- **SC-007**: Disabling the feature makes a test fail. A population no test is
  sensitive to is not evidence that it is emitted.
- **SC-008**: Every corpus target that does not scan a closure produces
  byte-identical goldens.

## Out of scope

- **The ~150 C and system packages.** `openssl`, `curl` and `zlib` have no
  PURL ecosystem; the right carrier is a CPE, which is a different mechanism
  and a different decision. Tracked separately under #1365.
- **The 32-name coverage gap** (issue #1366). Those components are not emitted
  at all; this feature changes the identity of components that are. The two
  are independent, and closing the coverage gap first would add unmatchable
  components rather than answerable ones.
- **Ecosystems other than the one US1 addresses.** The mechanism should
  generalise, but the pinned closure holds one Perl package and a handful of
  compilers, so no second ecosystem has a population large enough to measure
  against. Adding one is a later increment, not a requirement here.
- **Changing how Hackage components are identified.** Those already carry an
  ecosystem-qualified identifier.
- **Vulnerability matching itself.** waybill emits a document a feed can
  match; it does not perform the match. Vulnerabilities belong in VEX.

## Assumptions

- **The closure is the only source of candidate evidence.** Candidates are
  derived from records already fetched — no second evaluation, no maintained
  list of package names.
- **Scoped to one ecosystem.** The population measured on the pinned closure
  is Python: 83 derivations, 75 distinct names. Out of scope above explains
  why a second ecosystem is not specified here.
- **The identity is the component's, not the derivation's.** Build variants
  continue to merge as they do today; this feature changes what a component is
  called, not how many there are.
- **Confirmation establishes existence, not derivation identity.** Resolving
  `<name>@<version>` in the target ecosystem proves a published package by
  that name and version exists. It does not prove the closure derivation is
  that package. **Measured 2026-11-06: it declines 0 of 75 candidates**, so on
  this closure it is a filter that filters nothing; the two candidates
  previously named as failing it both resolve. FR-007's disclosure exists so
  a consumer can see the identity was derived on this basis rather than
  recorded, and no requirement here claims more than existence.
- **Today's identifier is the floor.** Every path that does not produce a
  confirmed ecosystem identity emits exactly what it emits today, so the
  feature cannot make the document worse.
- **Figures are from one closure and one system.** All counts are
  aarch64-darwin at a pinned nixpkgs revision. The corpus goldens pin
  x86_64-linux, which is a different closure — a figure from one is wrong in
  membership when quoted against the other, an error the closure milestones
  have made repeatedly.

## Evidence

Every count above was measured on 2026-11-06 and is reproducible:

- `specs/1276-boot-library-scope/measurements/sbomnix-comparison.md` — the
  comparison, the decomposition of the 781, and four recorded probe errors.
- `specs/1276-boot-library-scope/measurements/pypi-identity-probe.py` — the
  candidate rule and its counts.
- `waybill debug closure-dump --closure-json <file>` — per-derivation reality
  from the reader's own functions, rather than a model of it.
- `specs/1366-closure-ecosystem-purl/measurements/` — the confirmation probe,
  its two negative controls, and the retraction of the `librt` /
  `roman-numerals` claim this spec was originally written on.
