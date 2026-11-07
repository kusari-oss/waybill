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

## Clarifications

### Session 2026-11-07

- Q: What shape should C209 take, given `eligible_unmapped` mixes a real gap with a non-gap? → A: Split into reachable vs not — `with_candidates`, `addressable_without_cpe`, `unaddressable`
- Q: Should waybill emit a CPE for `aeson`, the one Haskell name of 163 NVD's dictionary has? → A: No `hackage` arm at all; record the measurement and the refusal

### The approved shape, with the measured figures rather than the illustrative ones

The numbers shown while choosing were illustrative. The real partition over
the pinned closure, computed from the emitted document:

| bucket | components | composition |
| --- | --- | --- |
| `with_candidates` | **78** | 72 `pypi`, 6 curated `generic` |
| `addressable_without_cpe` | **166** | 163 `hackage`, 3 `pypi` |
| `unaddressable` | **733** | 732 `generic`, 1 `github` |
| | **977** | sums to the component count |

**This inverts half the original story and is the reason the figures are here
rather than left as approved.** `eligible_unmapped: 284` was not only
overstating the Haskell part — it was **understating the real gap**. Its
release-shape gate excluded most `pkg:generic` components, so the count of
things no feed can address is **733**, not 284. A reader acting on 284 was
under-informed by a factor of 2.6, in the direction that matters.

The three buckets **partition every component**, rather than only the subset
that passed the synthesizer's eligibility gate. That is what makes FR-002
checkable against the component count, and it is also what accounts for the
24 versionless `pkg:hackage` components (FR-008): they land in
`addressable_without_cpe`, which is true of them — three of the six OSV hits
are versionless.

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
2. **Given** a document with no closure, **When** C209 is read, **Then** it
   carries the same three counts, because the row has never been
   closure-gated — 66 local goldens and 48 corpus goldens carry it today.

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

- **A document with no unaddressable components at all** — the count is 0 and
  must be emitted as 0, not omitted. `go-cobra`, `pants-example-javascript`
  and `python-flask` are all in this state today, so it is the common case
  rather than a hypothetical.
- **A component with both an ecosystem PURL and an operator-supplied CPE** —
  counted once, and not double-counted across categories.
- **An ecosystem waybill emits but no probe covered** — must fall to the
  conservative side rather than being claimed addressable on no evidence.
- **The 24 versionless `pkg:hackage` PURLs** — they never reach the
  synthesizer (`is_release_shaped("")` is false) and so are in neither count
  today. **Resolved by the partition**: they land in
  `addressable_without_cpe`, which is true of them — three of the six OSV hits
  on this closure are versionless. The edge case stays recorded because it is
  what rules out keeping the eligibility gate as the metric's denominator.

## Requirements *(mandatory)*

- **FR-001**: C209 MUST distinguish components no feed can address from
  components the synthesizer simply had no arm for.
- **FR-002**: The three counts MUST **partition every component** in the
  document — not only the subset passing the synthesizer's eligibility gate —
  so a reader can verify the row against the component count rather than
  trusting it. Partitioning the whole population is also what stops the 24
  versionless components falling outside every count, as they do today.
- **FR-003**: A PURL type counts as feed-addressable only when **both** hold,
  and both MUST be traceable to recorded output: (a) it maps to an ecosystem
  OSV declares in its published list, and (b) a probe with a known-vulnerable
  version returns that ecosystem's advisories. A type satisfying neither, or
  only one, is treated as not addressable.

  **Both halves are load-bearing.** `pkg:generic/curl@7.60.0` returns exactly
  one advisory, so condition (b) alone would classify `generic` as addressable
  — and the hit is `OSV-2017-79`, an OSS-Fuzz finding keyed on a project name,
  not curl's CVEs. OSV's published list contains no `generic` ecosystem, so
  condition (a) excludes it. Measured 2026-11-07: 47 declared ecosystems,
  `Hackage`, `PyPI`, `npm`, `Go`, `crates.io`, `Maven`, `Debian`, `Alpine`,
  `RubyGems` and `NuGet` all present; no `generic` and no `Generic`.
- **FR-004**: No component's identifiers change. This feature alters a
  document-scope metric and MUST NOT add, remove or modify any `cpes` entry.
- **FR-005**: Every document's C209 changes, and this MUST be planned for
  rather than discovered. The row is **not** closure-gated: it is emitted on
  every scan, and 66 local goldens plus 48 corpus goldens carry it. Replacing
  `eligible_unmapped` therefore churns all of them.

  **This requirement originally said the opposite** — that a non-closure scan
  stays byte-identical — which was false and would have been found only when
  the gate failed. Milestone 1367 added this row and churned 34 goldens for
  the same reason; the blast radius here is larger and is the inverse of
  milestone 1372's.
- **FR-006**: The existing `with_candidates` meaning MUST be preserved, since
  it is already in committed goldens and is not in question.
- **FR-007**: No `hackage` arm is added. The reason MUST be recorded in
  `generate/cpe.rs` beside the `match ecosystem` dispatch, with the measured
  figures, so the next reader finds the answer where they would look for the
  arm: 171 pairs probed and 1 hit (`aeson`, 95), while `base`, `Cabal` and
  `process` carry live HSEC advisories and have **no** dictionary entry — so a
  blanket arm would emit CPEs matching nothing for precisely the packages in
  this closure that have known vulnerabilities.
- **FR-008**: The 24 versionless components MUST be accounted for rather than
  silently absent from every count.

## Success Criteria *(mandatory)*

- **SC-001**: On the pinned closure, the count of components no feed can
  address equals an independently computed figure from the emitted document,
  derived without reference to the implementation.
- **SC-002**: The counts sum to the eligible population; asserted, not
  commented.
- **SC-003**: Every document difference is confined to the C209 value.
  Compared structurally in all three formats against the pre-change binary,
  the only differing JSON path is the `waybill:cpe-coverage` record — no
  component, relationship, licence or other annotation moves.
- **SC-004**: No component's `cpes` array changes on the pinned closure —
  measured before and after, not argued.
- **SC-005**: Every PURL type the implementation calls addressable satisfies
  both halves of FR-003 against committed output — present in OSV's declared
  ecosystem list **and** returning that ecosystem's advisories for a
  known-vulnerable version. A non-zero probe alone does not satisfy this.
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
