# Feature Specification: the curated CPE table covers 12 names; 51 more exist

**Feature Branch**: `1375-generic-cpe-table`
**Created**: 2026-11-07
**Status**: Draft
**Issue**: #1381

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1375**; it implements issue **#1381**.

## Summary

`pkg:generic` components are the **only** population a CPE helps. Milestone
1373 established that `pkg:hackage` and `pkg:pypi` are already answerable
through OSV, while `pkg:generic` names no registry —
`pkg:generic/openssl@3.0.0` returns **zero** from OSV despite its CVEs. For
these components a CPE is the sole route to any feed.

`GENERIC_LIBRARY_CPES` covers **12** slugs. After milestone 1374 removed the
patches and tarballs from the count, the unaddressable population is **194
components / 186 distinct names**, and **51 of those names have entries in
NVD's CPE dictionary**. None of the 51 is in the table.

## What the measurement already settled

### The vendor is almost never the package name

Every one of the 186 names was queried against NVD with the **vendor
wildcarded**, so the vendor is discovered rather than guessed. 186 queries,
**zero errors**, controls behaved.

| name | vendor NVD files it under |
| --- | --- |
| `bash` | `gnu` |
| `libxml2` | `xmlsoft` |
| `expat` | `libexpat` |
| `brotli` | `google` |
| `gawk` | `fossies` |
| `less` | `greenwoodsoftware`, `gnu` |

`expat` → `libexpat` is **exactly the defect
`scripts/probe-cpe-dictionary.sh`'s own header cites** (anchore/syft#4788). A
table built by assuming `(name, name)` would match nothing for nearly all of
these, and would do so silently.

### But the most-common vendor is not the answer either

`ninja` returns 126 dictionary entries across three vendors —
`ninjaframework` (a Java web framework), `itrsgroup`, `visualshortcodes` (a
WordPress plugin). **The build tool is not in NVD at all.** A row there would
tell a scanner that software is present which is not: worse than a silent
zero, because a silent zero is at least not a false claim.

`clang` is mixed: `llvm` is Clang, `arm` is ARM's different product. `less` is
genuinely two vendors for one product.

**Telling these apart needs each entry's title, and no count can supply it.**
The sweep produces **candidates, not rows**.

### 53 of the 186 can never be in NVD

`bootstrap-stage0-stdenv-darwin`, `apple-sdk-private-frameworks-hook`,
`cctools-binutils-darwin-wrapper` and 50 others are nixpkgs construction
names, not software anyone files advisories against. Their absence is
structural, not a gap — and worth recording as measured rather than leaving
someone to re-probe them.

All 51 names with dictionary entries are canonical upstream names; none is
nixpkgs-shaped.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — a scanner can match a closure's C libraries (P1)

An operator feeds a nix-closure SBOM to a CPE-keyed scanner and expects
`bash`, `bzip2`, `libxml2` and the rest to be matchable.

**Why this priority**: it is the entire value. These components have no other
route to any feed.

**Independent test**: scan the pinned closure and confirm that components
whose names have a verified table row carry a CPE whose vendor is the one NVD
files that product under.

**Acceptance**
1. **Given** a component named `bash`, **When** CPEs are synthesized,
   **Then** it carries `cpe:2.3:a:gnu:bash:<version>:*:*:*:*:*:*:*` — not
   `bash:bash`.
2. **Given** a component named `ninja`, **When** CPEs are synthesized,
   **Then** it carries **none**, because NVD's `ninja` entries are other
   products.

### User Story 2 — a wrong row cannot be added silently (P2)

**Why this priority**: the failure mode is silence. A wrong vendor returns
zero, which is indistinguishable from a clean component, and upstream has
shipped this defect at least twice.

**Independent test**: adding a table row without a recorded dictionary
observation fails the suite.

### User Story 3 — the names that were checked and rejected stay rejected (P3)

**Why this priority**: `ninja` cost a title query and a judgement. Without a
record, the next person re-probes it, sees 126 entries, and adds the row this
feature exists to refuse.

**Independent test**: a reader can tell, for any name in the population,
whether it was accepted, rejected and why, or never checked.

### Edge cases

- **A name NVD has under several legitimate vendors** — `less`, `c-ares`,
  `bzip2` — emits a candidate per vendor. The table's `&[(vendor, product)]`
  shape and `push_unique` already support this.
- **A name whose NVD entries are all a different product** — `ninja`. No row.
  The decision is recorded so it is not revisited by accident.
- **A name with one right vendor and one wrong** — `clang`: `llvm` yes, `arm`
  no. One row, one vendor.
- **A nixpkgs construction name** — no row and no probe needed; recorded as
  structurally absent.
- **A component whose version is not release-shaped** — already declined
  upstream of this table by `is_release_shaped`, and unchanged here.

## Requirements *(mandatory)*

- **FR-001**: Every new table row MUST have a recorded dictionary observation,
  enforced offline by the existing `every_table_pair_exists_in_the_nvd_dictionary`
  test (#1246).
- **FR-002**: Every new row MUST additionally be **title-confirmed**: the
  dictionary entries under that vendor MUST be the same product as the
  component. Dictionary presence alone is not sufficient, and `ninja` is the
  worked example.
- **FR-003**: A name whose entries are a different product MUST get **no
  row**, and the rejection MUST be recorded with the title that justified it.
- **FR-004**: A name with several legitimate vendors MUST emit a candidate for
  each.
- **FR-005**: No component's existing identifiers change. Components already
  carrying CPEs, and components of every other ecosystem, are untouched.
- **FR-006**: The emitted CPE's **vendor MUST be the one NVD files the product
  under**, never the component name by default.
- **FR-007**: The disposition of **every** name in the measured population
  MUST be recorded — accepted with which vendors, rejected with why, or
  structurally absent — so the population is answered once rather than
  re-probed.
- **FR-008**: `unaddressable` MUST fall by exactly the number of components
  that gain a CPE, and `with_candidates` MUST rise by the same number. No
  other count moves.

## Success Criteria *(mandatory)*

- **SC-001**: Every table row, old and new, has a recorded observation; the
  existing offline test passes with no row exempted.
- **SC-002**: Every **new** row is traceable to a committed title query
  showing the product identity, not only an entry count.
- **SC-003**: `ninja` carries no CPE after this feature, and the reason is
  recorded in a form a future contributor encounters before adding it.
- **SC-004**: On the pinned closure, the C209 delta is exactly
  `with_candidates +N` / `unaddressable −N` for the same N; every other count
  unchanged.
- **SC-005**: No component that carried a CPE before carries a different one
  after, compared as a set.
- **SC-006**: The recorded dispositions cover all 186 names in the measured
  population, with none left in an "unknown" state.

> **N is deliberately not fixed here.** It is at most 51 and will be lower,
> because title confirmation rejects some candidates — `ninja` already. The
> count is established by a task and recorded; quoting a number now would be
> quoting the sweep's entry counts as though they were verified rows, which
> is the exact error FR-002 exists to prevent.

## Assumptions

- NVD's dictionary grows, so the recorded observations are dated facts rather
  than invariants. The existing test asserts a pair *was seen*, not a count,
  which is the right strength.
- Dictionary presence is not advisory coverage. A verified row makes a
  component **matchable**; whether any advisory exists for it is a separate
  question this feature does not answer, as the existing header already notes.

## Out of scope

- Any ecosystem other than `pkg:generic`. `pkg:hackage` and `pkg:pypi` are
  reachable through OSV and were settled in milestone 1373.
- Changing how a slug is matched. The lookup is an exact match on the
  lowercased component name and stays that way.
- nixpkgs-specific aliases (`cmake-minimal` → `kitware:cmake`). None of the 51
  verified names needs one, so adding the mechanism would be speculative.
