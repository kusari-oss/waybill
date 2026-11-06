# Feature Specification: licenses for nix closure components

**Feature Branch**: `1281-closure-license-meta`
**Created**: 2026-11-06
**Status**: Draft
**Input**: User description: "Gap B" — close the one axis on which sbomnix currently reports more than waybill

## Context

Measured on the `nix-closure-moat` corpus target at its pin, aarch64-darwin,
both tools scanning the same project:

| | sbomnix | waybill (post-milestone-1280) |
|---|---|---|
| components | 1,274 | 413 |
| **licensed** | **301** | **137** |
| licensed rate | 23% | **33%** |
| with a content hash | 0 | 136 |
| with no PURL at all | 106 | 0 |

waybill's license *rate* is better and its identity is better. In absolute
terms sbomnix licenses more, and the reason is not only that it emits three
times the components: **96 components waybill already emits are licensed by
sbomnix and unlicensed by waybill**, and all 96 are closure-tier. They are
ordinary nixpkgs packages — `bash`, `autoconf`, `automake`, `bison`, `brotli`
— and the licenses they would gain are ordinary SPDX identifiers: MIT (23),
GPL-3.0-or-later (17), BSD-3-Clause (15), GPL-2.0-or-later (10),
BSD-2-Clause (10), and 3 carrying nixpkgs' `publicDomain`, which is **not** a
valid SPDX identifier.

This is the only measured axis on which the other tool is ahead.

**The data is not in anything waybill reads today.** Measured: the closure JSON
from `nix derivation show` contains **zero** license-bearing attributes — no
`meta`, no `license`, no `spdx`. `meta` is evaluation-time metadata and is not
part of a derivation, so no amount of re-reading the closure recovers it.

**How the other tool gets it**, read from its source rather than inferred
(`package_meta.nix` in sbomnix 1.9.0): a **second Nix evaluation** over the
flake's nixpkgs input, which walks package roots and returns metadata together
with each package's `drvPath` and `outPath`; the caller then accepts only rows
whose store path matches a component it already has.

Both join keys are already in waybill's hand — the closure JSON carries every
derivation's `.drv` path and every output path, and milestone 1280 already
indexes by output basename.

## Clarifications

### Session 2026-11-06

- Q: How should the license data be acquired, given the closure JSON carries none? → A: **A second, pure `nix eval` over the pinned nixpkgs**, returning `(drvPath, outPath, licenses)` and joined to components by exact store path — the mechanism sbomnix uses. Measured feasible before the question was put: it needs no `--impure`, so it passes the existing `argv_is_safe` guard, and it returns both join keys from the same evaluation that produces the license.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer can see what a closure component is licensed under (Priority: P1)

Someone receives an SBOM produced with `--nix-closure` and asks the question a
license-compliance reviewer always asks: under what terms does this component
ship? For 96 components of the measured closure the answer today is silence —
not "unknown", simply an absent field — while another tool reading the same
flake answers it.

**Why this priority**: it is the feature, and it is the only axis on which a
comparable tool currently reports more. A component with a PURL, a hash and no
license is less useful to a compliance reviewer than one with all three.

**Independent Test**: scan a flake with `--nix-closure`, and assert that
closure components carry `licenses[]` entries whose values came from the pinned
nixpkgs revision. Testable with a stub evaluation and a fixture closure.

**Acceptance Scenarios**:

1. **Given** a closure member that nixpkgs declares as MIT, **When** the
   document is emitted, **Then** that component carries a license whose SPDX
   identifier is `MIT`.
2. **Given** a closure member nixpkgs declares with several licenses,
   **When** the document is emitted, **Then** all of them are carried, not the
   first.
3. **Given** the same closure scanned twice, **When** the two documents are
   compared, **Then** the license sets are byte-identical.

---

### User Story 2 - A license nixpkgs states in its own vocabulary is preserved, not faked (Priority: P2)

nixpkgs names licenses through `lib.licenses.<attr>`, and not every attribute
is an SPDX identifier. Measured in this population: 3 of the 96 are
`publicDomain`, which SPDX has no identifier for.

**Why this priority**: inventing an SPDX id for an attribute nobody mapped is
how a license field stops being evidence — the wording milestone 1273 used when
it built the mapping table this feature reuses. The failure is silent and a
compliance reviewer cannot detect it.

**Independent Test**: assert that a member whose nixpkgs attribute has no SPDX
equivalent carries the declaration in a form marked as uncanonicalised, and
that no component carries an SPDX identifier the mapping table does not hold.

**Acceptance Scenarios**:

1. **Given** a member nixpkgs declares as `publicDomain`, **When** the document
   is emitted, **Then** the declaration is preserved and is **not** presented
   as a canonical SPDX expression.
2. **Given** a member nixpkgs declares as `mit`, **When** the document is
   emitted, **Then** it carries the SPDX identifier `MIT`.

---

### User Story 3 - The scan still succeeds when licenses cannot be obtained (Priority: P3)

The acquisition needs an evaluation that can fail: no `nix` on the host, a
flake whose inputs cannot be resolved, a budget exceeded, an evaluation the
safety guard refuses.

**Why this priority**: the closure tier already degrades for all of these and
this must not become the first part of it that does not. A license gap is a
smaller loss than a failed scan.

**Independent Test**: scan with an evaluation that fails, and assert the
document is still emitted, carries its components, and records why the licenses
are absent.

**Acceptance Scenarios**:

1. **Given** an evaluation that fails, **When** the scan completes, **Then** the
   document is emitted with its components and no licenses from this source,
   and the reason is recorded in the document rather than only logged.
2. **Given** no `--nix-closure`, **When** the scan completes, **Then** output is
   byte-identical to today.

---

### Edge Cases

- **A member nixpkgs does not describe at all.** Not every closure member is a
  nixpkgs package; bootstrap and fetch derivations are not. Absence must be
  distinguishable from "not looked up".
- **A member already carrying a license from another reader.** The Haskell
  package-set pass (milestone 1273) already licenses some closure members.
  Measured: 136 of 388 closure components are licensed today. A second source
  must not overwrite, duplicate or contradict the first.
- **A license expression nixpkgs states as a composite** (`with lib.licenses;
  [ asl20 mit ]`) — several entries, not one string to split.
- **A store path that matches more than one package root**, so the join is
  ambiguous.
- **An evaluation that succeeds and returns nothing**, which must not be
  reported as success with zero coverage indistinguishable from a closure whose
  members genuinely have no licenses.
- **A closure of a system configuration rather than a package**, where the
  nixpkgs input may not be reachable the same way.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Closure components MUST carry the licenses the pinned nixpkgs
  revision declares for them, where that revision declares one.
- **FR-002**: The join between a declared license and a component MUST be by
  **exact store path** — the derivation's `.drv` path or an output path — and
  never by name, or by name and version. Name matching across a 1,749-derivation
  closure is how a wrong license reaches a compliance reviewer with no signal
  that it is wrong.
- **FR-003**: An attribute with no SPDX equivalent MUST be preserved in a form
  marked uncanonicalised, and MUST NOT be presented as a canonical SPDX
  expression. An SPDX identifier MUST NOT be emitted for an attribute the
  mapping table does not hold.
- **FR-004**: Where a member already carries a license from another reader, the
  two MUST be reconciled without duplication and without silently replacing an
  existing declaration.
- **FR-005**: Several declared licenses MUST all be carried.
- **FR-006**: Failure to acquire licenses MUST degrade — the document is still
  emitted with its components — and the reason MUST be recorded in the document,
  not only logged (Constitution XII.3).
- **FR-007**: A scan without `--nix-closure` MUST produce byte-identical output
  to today.
- **FR-008**: The document MUST record how many closure components were offered
  a license and how many received one, so a reader can tell a closure whose
  members have no declared licenses from an acquisition that silently returned
  nothing.
- **FR-009**: Licenses MUST be emitted through each format's native license
  construct, not a `waybill:` annotation.
- **FR-010**: Two scans of one closure MUST produce byte-identical license sets.
- **FR-011**: Licenses MUST be acquired by a **second Nix evaluation over the
  pinned nixpkgs**, returning each package's `drvPath`, output path and declared
  licenses together, and joined to components by FR-002's exact store path.

  **It MUST run pure.** Measured: `builtins.getFlake` on a *locked* `github:`
  flakeref needs no `--impure`, unlike the local-path case milestone 1035
  measured, so the invocation passes `argv_is_safe` unchanged. An
  implementation that reaches for `--impure` has taken a wrong turn — the guard
  will refuse it, and correctly.
- **FR-012**: The evaluation MUST be subject to the same safety machinery the
  closure tier already uses — the import-from-derivation refusal verified
  rather than merely requested, the argv guard, and a budget — and MUST NOT
  introduce a second, parallel set of controls.
- **FR-013**: The enumeration MUST survive a package whose evaluation throws.
  Measured: enumerating `legacyPackages` fails after 7.5s on an
  unsupported-system throw that a `tryEval` around the attribute access does
  **not** contain. This is one of the two `tryEval` traps milestone 1050
  recorded; a package that cannot be evaluated contributes no license and MUST
  NOT abort the pass.
- **FR-014**: Where nixpkgs states an `spdxId` for a license, that identifier
  MUST be used directly rather than re-derived from the attribute name.
  nixpkgs' own `lib.licenses.<attr>.spdxId` is the authority, and a second
  mapping maintained beside it is a second thing to fall out of date.

### Key Entities

- **Declared license**: what the pinned revision states for a package, in
  nixpkgs' own vocabulary. Already modelled with two states — canonical SPDX,
  or preserved-and-marked-uncanonicalised — by milestone 1273.
- **Store-path join key**: the `.drv` path or output path shared by the closure
  and whatever supplies the licenses. The only identifier the two sides agree
  on.
- **Acquisition outcome**: whether licenses were sought, obtained, or
  unavailable, and why — the FR-006 and FR-008 record.

## Success Criteria *(mandatory)*

All figures are the `nix-closure-moat` corpus target at its pin. The measured
scan is **aarch64-darwin**; the committed goldens pin **x86_64-linux**, a
different closure, so figures are stated per system and the linux figures are
to be measured rather than scaled. (Scaling one system's result to the other is
an error milestone 1280 made three times.)

- **SC-001**: Closure components carrying a license rise from **137** on
  aarch64-darwin. The target is the 96 that another tool already licenses from
  the same flake; a run that gains none of them has not worked.
- **SC-002**: No component loses a license it carries today.
- **SC-003**: No component carries an SPDX identifier absent from the
  milestone-1273 mapping table.
- **SC-004**: Two scans of one closure emit byte-identical license sets,
  asserted by a test.
- **SC-005**: A scan whose license acquisition fails still emits a document with
  its full component set, and records why.
- **SC-006**: Every corpus target that does not scan a closure produces
  byte-identical goldens.
- **SC-007**: The C182 role census and the component count are unchanged — this
  feature adds licenses, not components.
- **SC-008**: Disabling the acquisition makes a test fail. A license source no
  test is sensitive to is not evidence that it is read.
- **SC-009**: After this feature, waybill's licensed-component count on the
  measured closure is **at least** what another tool obtains from the same
  flake for the components both emit — the 96 is the floor, not the ceiling.

## Assumptions

- **The pinned revision is the authority.** Licenses come from the revision the
  scanned flake pins, not from a newer nixpkgs and not from a package registry.
  A license is a property of the version being built.
- **Scope is closure components.** Components from other readers are not
  touched; their licensing is those readers' concern.
- **Milestone 1273's two-state license type is reused**, not re-derived: it
  already models the `publicDomain` case this population contains. Its
  *mapping table* may turn out to be unnecessary here, because nixpkgs carries
  `spdxId` on the license attrset itself (FR-014) — the table exists for a path
  that reads `hackage-packages.nix` without evaluating, which is not this path.
- **No component is added or removed.** This feature changes the `licenses[]`
  field and nothing else about the component set.
- **The existing per-revision nixpkgs cache is reusable** for whatever this
  acquires, under the same pinned-revision layout milestones 926 and 1275 use.

## Out of scope

- Emitting the fetch and auxiliary derivations as components ("Gap A") — a
  separate, larger change with its own identity scheme.
- Licenses for any reader other than the nix closure tier.
- Changing how licenses are canonicalised, or extending the attribute mapping
  table beyond what the measured population needs.
- Concluded-versus-declared license distinction.
