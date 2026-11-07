# Feature Specification: what `eligible_unmapped` is actually counting

**Feature Branch**: `1373-hackage-cpe-decision`
**Created**: 2026-11-07
**Status**: Draft
**Issue**: #1375

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1373**; it resolves issue **#1375**.

## Summary

Milestone 1367 added `waybill:cpe-coverage` (C209) so that *"nothing to work
with"* and *"the pass did not run"* would stop being the same document. On the
pinned closure its larger number reads **284**, and issue #1375 asked whether
that is a gap or a metric overstating one.

It was answerable by measurement, and the measurement says **both, mixed into
one number**:

| population | components | can a feed address it? |
| --- | --- | --- |
| `pkg:generic` | 738 | **no** — this is the real gap |
| `pkg:hackage` | 163 | **yes**, via OSV — not a gap |
| `pkg:pypi` | 75 | yes, via OSV, *and* it receives CPEs |

A single count that adds the first two together cannot be read, and the row
was added precisely to stop that kind of conflation.

## Why `pkg:generic` is the real gap, measured

Every component in the document carries a PURL — `no_purl` is **0**. So
"unmatchable" is never about having no identifier; it is about whether any
feed indexes the identifier's type. Probed against OSV with known-vulnerable
versions:

| PURL | advisories |
| --- | --- |
| `pkg:pypi/urllib3@1.26.4` | 22 |
| `pkg:npm/lodash@4.17.11` | 6 |
| `pkg:hackage/aeson@2.0.0.0` | 2 |
| `pkg:generic/openssl@3.0.0` | **0** |
| `pkg:generic/zlib@1.2.11` | **0** |
| `pkg:generic/curl@7.60.0` | 1 — and it is `OSV-2017-79`, an OSS-Fuzz finding, not curl's CVEs |

`openssl 3.0.0` and `zlib 1.2.11` both have well-known CVEs. They return
nothing because `pkg:generic` names no registry, so there is nothing to index
it against. **For a `pkg:generic` component a CPE is the only route to a feed
at all** — which is why the curated `GENERIC_LIBRARY_CPES` table exists, and
why the ~117 generic names it does not cover are a genuine gap worth counting.

For `pkg:hackage` the opposite holds, and the numbers are not close:

- NVD's CPE dictionary has an entry for **1 of 163** names (`aeson`, 95). All
  171 pairs probed, zero unanswered, controls at 385 and 0.
- OSV returns advisories for **6 of 163** using waybill's own emitted PURLs,
  with version filtering demonstrably applied.
- The three with live advisories that NVD has **no dictionary entry for** —
  `base`, `Cabal`, `process` — are reachable only through OSV.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — an operator reads the coverage row and acts on it (P1)

An operator scanning a Nix closure wants to know how much of the document a
vulnerability scanner can actually do something with, and where to spend
effort if the answer is "less than I want".

**Why this priority**: it is the row's entire purpose. Today the operator
reads 284, infers 284 blind spots, and is wrong by roughly half.

**Independent test**: scan the pinned closure, read C209, and check that the
number describing unreachable components matches a hand count of components
whose PURL type no feed indexes and which carry no CPE.

**Acceptance**
1. **Given** a closure with both `pkg:generic` and `pkg:hackage` components,
   **When** C209 is read, **Then** the components already addressable by a
   feed are distinguishable from those that are not.
2. **Given** a document with no closure, **When** C209 is read, **Then** it is
   absent exactly as today.

### User Story 2 — a reader can tell which ecosystems are carrying the gap (P2)

**Why this priority**: "284 unmapped" names no action. "117 of them are
`pkg:generic` names absent from the curated table" names one.

**Independent test**: the row identifies the ecosystems contributing to each
count, and the figures sum to the totals.

### User Story 3 — the decision not to synthesize Haskell CPEs is recorded (P3)

**Why this priority**: without it, the next reader sees 163 unmapped
`pkg:hackage` components and re-opens #1375. The measurement cost two sweeps
and should not be repeated to reach the same answer.

**Independent test**: a documented statement, traceable to the committed
probe output, of why no `hackage` arm exists.

### Edge cases

- **A closure with no `pkg:generic` components at all** — the unreachable
  count is 0 and must be emitted as 0, not omitted. Absence already means "no
  closure ran" (C204/C208 convention) and must keep meaning only that.
- **A component with both an ecosystem PURL and an operator-supplied CPE** —
  counted once, and not double-counted across categories.
- **An ecosystem waybill emits but no probe covered** — must fall to the
  conservative side rather than being claimed addressable on no evidence.
- **The 24 versionless `pkg:hackage` PURLs** — they never reach the
  synthesizer (`is_release_shaped("")` is false) and so are in neither count
  today. Whatever replaces the counts must still account for them somewhere,
  or 24 components vanish from a coverage metric.

## Requirements *(mandatory)*

- **FR-001**: C209 MUST distinguish components no feed can address from
  components the synthesizer simply had no arm for.
- **FR-002**: The counts MUST sum to the eligible population, so a reader can
  verify the row against the component count rather than trusting it.
- **FR-003**: The classification of a PURL type as feed-addressable MUST be
  traceable to a recorded probe, not asserted. A type with no probe is treated
  as not addressable.
- **FR-004**: No component's identifiers change. This feature alters a
  document-scope metric and MUST NOT add, remove or modify any `cpes` entry.
- **FR-005**: A scan with no nix closure MUST produce a byte-identical
  document, as C209 is already gated on a closure having been recorded.
- **FR-006**: The existing `with_candidates` meaning MUST be preserved, since
  it is already in committed goldens and is not in question.
- **FR-007**: The reason no `hackage` CPE arm exists MUST be recorded in the
  source alongside the measured figures, so the next reader finds the answer
  where they would look for the arm.
- **FR-008**: The 24 versionless components MUST be accounted for rather than
  silently absent from every count.

## Success Criteria *(mandatory)*

- **SC-001**: On the pinned closure, the count of components no feed can
  address equals an independently computed figure from the emitted document,
  derived without reference to the implementation.
- **SC-002**: The counts sum to the eligible population; asserted, not
  commented.
- **SC-003**: A scan with no closure is byte-identical pre and post, compared
  structurally in all three formats.
- **SC-004**: No component's `cpes` array changes on the pinned closure —
  measured before and after, not argued.
- **SC-005**: Every PURL type the implementation calls addressable appears in
  the committed probe output with a non-zero result.
- **SC-006**: The 24 versionless components appear in exactly one count.

## Assumptions

- OSV's ecosystem coverage is a property of the feed and changes over time.
  The probe output is dated and the spec claims only what it measured on that
  date; a type's classification is a recorded observation, not a permanent
  truth.
- `pkg:generic` components remain the population for which the curated CPE
  table is the only route. Growing that table is #1375's sibling concern and
  is **out of scope** here.

## Out of scope

- Adding any CPE arm, `hackage` or otherwise.
- Growing `GENERIC_LIBRARY_CPES`.
- Consuming OSV at scan time. waybill emits identifiers; it does not query
  feeds, and nothing here changes that (vulnerabilities belong in VEX).
- The versionless-PURL consumer consequence, which is a documentation matter
  and already fully disclosed by existing annotations.
