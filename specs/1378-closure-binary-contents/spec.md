# Feature Specification: the Go modules and Rust crates inside a Nix-built binary

**Feature Branch**: `1378-closure-binary-contents`
**Created**: 2026-11-09
**Status**: Draft
**Issue**: #1396 — *Go modules and Rust crates compiled into a Nix-built binary are invisible to the closure tier (0 vs 163 Go modules on gh), though the binary readers recover them*

> **Numbering note.** Spec directory numbers and GitHub issue numbers are two
> separate sequences in this repository. This is spec **1378**; it implements
> issue **#1396**. Cite the issue, never the bare number.

## Context

A `--nix-closure` scan enumerates derivations. A Go or Rust program's
dependencies are not derivations: nixpkgs fetches them as one bundle and
compiles them into the binary. So the closure document lists the bundle as a
single opaque entry and none of what is in it.

Measured on waybill `main@49a24a63`, nixpkgs `a799d3e3`, aarch64-darwin, three
identical runs each (`measurements/FINDINGS.md`):

| | Go modules in `gh` 2.93.0 | crates in `ripgrep` 15.1.0 |
| --- | --- | --- |
| waybill closure scan | 0 of 494 components | 0 of 599 components |
| waybill pointed at the built output | 163 | 42 |
| the language's own tooling, as a reference | 163 | 42 |
| sbomnix 1.9.0, buildtime and runtime | 0 | 0 |

The capability exists. Waybill already reads a built Go or Rust binary
correctly — its counts match the language toolchains' own. The closure scan
simply never looks at an output.

## Why it matters

For a Go or Rust program these modules are most of its vulnerability surface,
and they carry ecosystem identifiers a vulnerability database can match
directly. A PURL-keyed scanner reports 0 findings on Go or Rust packages from
either closure document, and 65 (gh) and 3 (ripgrep) from the documents
produced by reading the built outputs. Those findings are unadjudicated; the
point is that today the question cannot be asked at all.

No comparable tool answers it either. sbomnix lists the same single bundle
under an identifier no database indexes.

## What measurement established, and what it constrains

1. **The scanned package is not a component.** Neither closure document contains
   an entry for gh or ripgrep itself; the document's subject is a placeholder
   (#1398, open). There is currently nothing to attach recovered modules to for
   the one member that matters most.
2. **Outputs are mostly absent.** 96 of gh's 894 output paths and 102 of
   ripgrep's 1,003 were present on the measuring host — and only because the
   packages had been installed. On a machine that has never built or fetched
   the package, none are.
3. **Reading what is present is cheap and quiet.** All 104 distinct present
   outputs were read in 29 seconds; exactly two yielded modules or crates, the
   two scanned packages. No language toolchain was present, so what a
   toolchain's own binaries would contribute is unmeasured.
4. **The binary does not always know its own version.** gh's embedded build
   information names its module with no version; the closure knows it is
   2.93.0. ripgrep's binary does name itself and its version.
5. **Today's two binary readers describe containment differently.** Crates are
   nested inside the binary's entry; Go modules sit beside it.
6. **An output is not always a directory.** One of the 104 is a single file.

## Clarifications

### Session 2026-11-09

- Q: The scanned package is not a component today (#1398). Does this feature wait for #1398, include the minimum of it, absorb it, or attach to non-root members only? → A: Include the minimum — this feature makes the closure root a component carrying the closure's name and version. The document subject and the package's CPE stay with #1398.
- Q: Which construct relates a member to the modules compiled into it — plain dependency, dependency plus static-link, or containment? → A: Plain dependency in all three formats. Recovered modules are top-level components and the member depends on them.
- Q: Which members are read — every member with a present output, only shipped members, or only the closure root? → A: Every member with a present output. Contents inherit the member's classification, so a build tool's modules are emitted as build-only.
- Q: When a binary names its own module, does that ecosystem identity become the member's primary identifier? → A: Only when confirmed. The binary's path joined to the closure's version becomes primary if the registry confirms that name and version exist; otherwise it is recorded as an unconfirmed candidate and the closure identifier stays primary.
- Q: What happens when the closure root's own contents could not be read? → A: The scan succeeds by default and records the absence; an operator opt-in makes an unread closure root a scan failure.

### Rebase 2026-11-10

- The first clarification above ("include the minimum of #1398") was overtaken:
  milestone 1377 (#1405) landed #1398 on main while this was in progress. This
  feature now uses main's evaluated package and adds no root mechanism of its
  own. FR-019 to FR-021 are rewritten accordingly.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See what is compiled into the package I scanned (Priority: P1)

A security engineer scans a flake that builds a Go or Rust program, on a
machine where the program has been built. The resulting document lists the
modules or crates compiled into that program, each with an identifier a
vulnerability database can match, as dependencies of the program.

**Why this priority**: This is the issue. It turns a document on which a
vulnerability scanner finds nothing about the program's own code into one on
which it can.

**Independent Test**: Scan the gh and ripgrep wrapper flakes with their outputs
present; count the Go modules and crates in the document against the language
toolchain's own count for the same binary; run a PURL-keyed scanner on the
document.

**Acceptance Scenarios**:

1. **Given** a flake building gh 2.93.0 whose output is present, **When** a
   closure scan runs, **Then** the document contains the same 163 Go modules
   the Go toolchain reports for that binary, each a dependency of the gh
   component.
2. **Given** a flake building ripgrep 15.1.0 whose output is present, **When** a
   closure scan runs, **Then** the document accounts for the same 42 crates
   recorded in the binary: 41 as dependencies of the ripgrep component, and
   the 42nd — ripgrep's own crate — as that component itself (User Story 3).
3. **Given** either document, **When** it is emitted in each of the three
   supported formats, **Then** the same modules and the same
   member-to-module relationships appear in all three.
4. **Given** the gh document, **When** a PURL-keyed vulnerability scanner reads
   it, **Then** it reports findings on Go modules where it reported none before.

---

### User Story 2 - Know when the contents could not be read (Priority: P1)

A security engineer scans the same flake on a fresh CI runner, where nothing
has been built. No modules can be recovered. The document says so: which
members' outputs were absent and therefore unread, so that "no Go modules
listed" is never mistaken for "no Go modules present".

**Why this priority**: The absent case is the common one — roughly nine outputs
in ten on a developer machine, all of them on a fresh runner. Shipping Story 1
without this makes the document's meaning depend silently on the state of the
machine that produced it.

**Independent Test**: Scan a flake whose outputs are not present; confirm no
modules are emitted, the scan succeeds, and the document states how many
members were read and how many could not be.

**Acceptance Scenarios**:

1. **Given** a closure none of whose outputs are present, **When** a scan runs,
   **Then** it succeeds, emits no recovered modules, and the document records
   that no member's contents were read and why.
2. **Given** a closure with some outputs present, **When** a scan runs,
   **Then** the document records how many members were read, how many were
   absent, and how many were read and yielded contents.
3. **Given** a member whose output was absent, **When** a consumer inspects
   that member, **Then** it can tell this member's contents were not examined.
4. **Given** a closure whose root output is absent and the operator has opted
   in to strict behaviour, **When** a scan runs, **Then** it fails, names the
   root and the reason, and writes no document.
5. **Given** strict behaviour and a root that is present but embeds nothing
   (a C program), **When** a scan runs, **Then** it succeeds.
6. **Given** the same flake scanned on two machines with different outputs
   present, **When** the two documents are compared, **Then** every difference
   in recovered modules is explained by a recorded difference in what was read.

---

### User Story 3 - One identity per package, not two (Priority: P2)

The program appears once. The closure's entry for gh and the binary's
description of itself are the same thing; the document does not list gh 2.93.0
and, separately, a versionless Go module for the same program.

**Why this priority**: Without it Story 1 produces a duplicate of the most
important component, one copy with a version and one without, and consumers
must guess which carries the dependencies.

**Independent Test**: In the gh and ripgrep documents, count entries describing
the scanned program.

**Acceptance Scenarios**:

1. **Given** a binary that names its own module without a version (gh),
   **When** its contents are attached, **Then** the closure member remains the
   single entry for the program, keeps the closure's version, and carries the
   module path the binary reported.
2. **Given** a binary that names itself with a version (ripgrep), **When** its
   contents are attached, **Then** there is still a single entry for the
   program, and the binary's self-description is recorded on it.
3. **Given** a binary whose self-reported version disagrees with the closure
   member's, **When** its contents are attached, **Then** the closure's version
   is used and the disagreement is visible on the component.
4. **Given** a program whose module name and closure version the registry
   confirms, **When** the document is emitted, **Then** the program's primary
   identifier is the ecosystem one and a PURL-keyed scanner can match
   advisories filed against the program itself.
5. **Given** the same program scanned offline, or a program the registry does
   not know, **When** the document is emitted, **Then** the closure identifier
   is primary and the ecosystem identity appears only as an unconfirmed
   candidate.

---

### User Story 4 - The same shape whichever language (Priority: P3)

A consumer walking the document finds recovered Go modules and recovered Rust
crates related to their member in the same way.

**Why this priority**: The two existing readers differ; a consumer should not
need to know which language a member was written in to find its contents.

**Independent Test**: Compare how the gh and ripgrep documents relate modules to
their member.

**Acceptance Scenarios**:

1. **Given** the gh and ripgrep documents, **When** the relationship between a
   member and its recovered modules is read, **Then** it is the format's
   ordinary dependency relationship in both, in each format.
2. **Given** the ripgrep document, **When** its components are listed,
   **Then** no crate is nested inside another component.

---

### Edge Cases

- **A member's output is a single file**, not a directory. It is read if it is
  a binary and counted as read either way; it is never a scan failure.
- **A member has several outputs** (for example a program and its
  documentation), some present and some not. Contents are recovered from those
  present; the member is recorded as partially read.
- **A member's output contains several binaries** that share modules. Each
  module appears once per distinct identity under that member.
- **Two members embed the same module at the same version.** One component,
  related to both members.
- **Two members embed the same module at different versions.** Two components,
  each related to its own member.
- **A present output contains a wrapper script** around the real binary (gh's
  launcher is a shell script; the binary sits beside it under another name).
  The binary is still found.
- **A binary carries no embedded build information** (a C program, or a Rust
  binary built without it). The member is recorded as read with nothing
  recovered — distinct from not read.
- **A build-tool member's output is present and embeds modules** (a language
  toolchain). Its modules follow that member's classification; they are not
  promoted to shipped content.
- **An output path is present but unreadable**, or reading it fails part-way.
  The scan continues; that member is recorded as not read, with the reason.
- **The closure scan itself degraded** before producing members. Nothing is
  read, and the existing degradation record is the explanation.
- **The store is not at its conventional location**, or the scan runs where no
  store exists. Every member is recorded as absent.

## Requirements *(mandatory)*

### Functional Requirements

**Reading**

- **FR-001**: Under a closure scan, for every closure member with at least one
  output present on the scanning machine, the system MUST examine that output
  for embedded build information using the same recognition the standalone
  binary scan already uses, so that the two never disagree about what a binary
  contains.
- **FR-002**: The system MUST NOT build, fetch or otherwise bring an absent
  output into existence. This feature reads only what is already present.
  Whether a scan may ever realise outputs is a separate decision, deferred to
  #1034.
- **FR-003**: Reading MUST be part of the closure scan with no additional flag:
  the operator who asked for the closure has asked for what is in it. The only
  operator control this feature adds is the strict opt-in of FR-015b. Outputs
  are looked for in the store directory nix itself is configured to use.
- **FR-004**: A failure to read any one output MUST NOT fail the scan or
  suppress any other member's contents.

**Attaching**

- **FR-005**: Each recovered module or crate MUST be emitted as a component
  with its ecosystem identifier and version exactly as the standalone binary
  scan would emit it.
- **FR-006**: Each recovered module MUST be related to the closure member whose
  output it was recovered from, as that member's dependency, in all three
  output formats.
- **FR-007**: The relationship between a member and its recovered contents MUST
  be each format's ordinary dependency relationship, regardless of language.
  Recovered modules MUST be top-level components, never nested inside the
  member or inside a binary's own entry. Containment and static-link
  relationships MUST NOT be used for this.
- **FR-008**: A module recovered from several members at one identity MUST be
  one component related to each of them.
- **FR-009**: Recovered modules MUST carry the same evidence of origin the
  standalone binary scan records (that they were read from a built binary, and
  which file).
- **FR-010**: Recovered modules MUST take the artifact-presence classification
  of the member they were recovered from, so contents of a build-only member
  are not presented as shipped. A module recovered from both a shipped and a
  build-only member MUST be presented as shipped.

**Identity**

- **FR-011**: A binary's description of itself MUST be reconciled to the
  closure member it was recovered from; the document MUST NOT contain a second
  component for the same program.
- **FR-012**: On reconciliation the closure member's name and version MUST be
  kept, and any disagreement between the closure's version and a version the
  binary reports for itself MUST be visible on the component.
- **FR-012a**: The member's ecosystem identity is the module or crate name the
  binary reports for itself joined to the closure's version. It MUST become
  the member's primary identifier only when the ecosystem's registry confirms
  that name and version exist, under the same confirmation rule already
  applied to closure components (#1365).
- **FR-012b**: When the identity cannot be confirmed — the registry does not
  know it, the scan is offline, or confirmation fails — the closure identifier
  MUST stay primary and the ecosystem identity MUST be recorded as an
  unconfirmed candidate, distinguishable from a confirmed one. An unconfirmed
  join MUST never be presented as the primary identifier.

**Saying what was not read**

- **FR-013**: The document MUST record, at document scope, how many members
  were eligible, how many had an output present and were read, how many were
  absent, how many failed to read, and how many yielded contents.
- **FR-014**: A member whose contents were not examined MUST be distinguishable
  from a member that was examined and found to embed nothing.
- **FR-015**: When no member could be read, the document MUST state that and
  the reason, and by default the scan MUST still succeed.
- **FR-015a**: When the closure root's contents could not be read, the system
  MUST say so prominently to the operator at scan time, not only in the
  document.
- **FR-015b**: The operator MUST be able to opt in to strict behaviour, under
  which a closure root whose contents could not be read fails the scan with a
  message naming the root and the reason, and no document is written. Strict
  behaviour applies to the closure root only; unread build inputs never fail
  a scan. A root that was read and embeds nothing is not a failure. A root in
  the *partial* state passes with a warning, since which output holds the
  program is not knowable from the closure; *absent*, *failed*, *no outputs*
  and a closure with no single root all fail.
- **FR-016**: The completeness the document claims for the affected ecosystems
  MUST reflect unread members: a document with unread members MUST NOT present
  its Go or Rust contents as complete.

**Standards**

- **FR-017**: Before any waybill-specific property or annotation is introduced,
  each format MUST be audited for a native construct, and the native construct
  used where one exists (Constitution Principle V). The audit so far, to be
  confirmed in planning:
  - *Member-to-contents relationship*: CycloneDX 1.6 has the dependency graph
    and nested components; SPDX 2.3 has dependency, containment and
    static-link relationships; SPDX 3 has the corresponding relationship types.
    A native construct exists in all three, and the ordinary dependency
    relationship is the one chosen (FR-007): it is the only one present in all
    three with the same meaning, and the only one that lets a module shared by
    two members be a single component. No waybill-specific row is expected.
  - *Unread members / incomplete contents*: CycloneDX 1.6 has compositions with
    an aggregate completeness value, which waybill already emits. SPDX 2.3 and
    SPDX 3 have no equivalent of comparable granularity. A waybill-specific
    document-scope record is expected for the counts in FR-013, under the
    carve-out for information the standards do not express, with the native
    CycloneDX construct used alongside it.
  - *Candidate identity on the member* (FR-012b): all three formats allow more
    than one identifier on a component, and CycloneDX 1.6 has identity
    evidence with a confidence. Native; #1245 audits the same slots.
- **FR-018**: The three formats MUST agree on recovered components,
  relationships and the unread record, enforced by the existing cross-format
  parity mechanism.

**Dependency on the scanned package being a component**

- **FR-019**: Recovered contents of the scanned package MUST be attached to
  the component that represents it. That component is the evaluated package
  milestone 1377 (#1398) emits and makes the document's subject; this feature
  adds nothing to how it is emitted. When a closure has several unconsumed
  derivations and names no evaluated package, no member is treated as the
  root and the read record says so.
- **FR-020**: *(Delivered by milestone 1377.)* The evaluated package's direct
  dependencies are anchored to it.
- **FR-021**: This feature MUST NOT change the document's subject or how its
  CPE is chosen; both belong to milestone 1377. When this feature promotes
  the evaluated package's identifier (FR-012a), whatever milestone 1377 and
  the existing CPE synthesis do for a component of that ecosystem applies.

### Key Entities

- **Closure member**: a derivation in the scanned package's build closure, with
  a name, a version where it declares one, a role, and one or more outputs.
- **Output**: a store path a member produces. Present or absent on the scanning
  machine; a directory or a single file.
- **Recovered module**: a Go module or Rust crate named by build information
  embedded in a binary inside an output. Has an ecosystem identifier and a
  version.
- **Self-description**: what a binary says about its own module or crate —
  possibly without a version.
- **Read record**: per member, one of five states — *read* (every declared
  output present and examined), *partial* (some present, some absent),
  *absent* (none present), *failed* (a present output could not be examined)
  and *no outputs* (the member declares no output path) — summarised at
  document scope.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the gh wrapper flake with its output present, the closure
  document lists 163 Go modules, equal to the Go toolchain's count for the same
  binary. Today: 0.
- **SC-002**: On the ripgrep wrapper flake with its output present, the closure
  document accounts for all 42 crates recorded in the binary: 41 recovered as
  dependencies plus ripgrep's own crate reconciled into the ripgrep component.
  The document therefore holds 42 cargo identifiers when the program's
  identity is confirmed and 41 when confirmation is off. Today: 0.
- **SC-002a**: gh's own module is not among the 163 the toolchain lists as
  dependencies, so SC-001's 163 is the recovered count; the gh component adds
  a 164th Go identifier when its identity is confirmed.
- **SC-003** *(positive control)*: A PURL-keyed vulnerability scanner run on
  the gh closure document reports the same Go findings it reports on the
  standalone scan of gh's output — 65 on 12 packages when measured, 0 on the
  closure document today. The figure is expected to move with the advisory
  database; the criterion is equality with the standalone scan, not 65.
- **SC-004** *(negative control)*: On a closure none of whose outputs are
  present, the document lists 0 recovered modules, the scan succeeds, and the
  document states that no member was read and why.
- **SC-004a**: With strict behaviour opted in, the SC-004 scan fails and
  writes no document; the SC-001 and SC-002 scans are unaffected by the opt-in.
- **SC-005**: In each of the gh and ripgrep documents exactly one component
  describes the scanned program, and it carries the closure's version.
- **SC-005a**: Neither scan reports the closure root as missing from the
  components; the unanchored-dependency count is 0 for both targets (today 16
  and 8).
- **SC-006**: The recovered components and their relationships are identical
  across all three formats for both targets.
- **SC-007**: A closure scan of a flake with no Go or Rust member is unchanged
  apart from the read record: same components, same relationships.
- **SC-008**: Added scan time stays within the measured cost of reading the
  present outputs individually — 29 seconds for 104 outputs on the measuring
  host, expressed as time per output read, not as a ratio to total scan time.
- **SC-009**: Every difference in recovered modules between two scans of one
  flake on machines with different outputs present is accounted for by the
  documents' own read records.

## Assumptions

- **Enhancement, not a new reader.** The closure scan and the binary readers
  both exist; this feature connects them. No new recognition of binary formats
  or ecosystems is in scope.
- **Every present member is read, not only shipped ones** (decided; see
  Clarifications). Measurement found two yielding members of 104 and no noise,
  but no toolchain was present to test. FR-010 is the safeguard: contents
  inherit the member's classification. Planning MUST still measure a closure
  with a Go or Rust toolchain present, to size what build-only contents add
  and to confirm they are classified build-only. The decision is revisited
  only if that measurement shows the safeguard failing.
- **No realisation.** The default-no in the issue discussion is taken as "not
  in this feature". An opt-in to realise is a follow-on tied to #1034, because
  realising runs builds and its cost is unmeasured.
- **Present means present at the conventional store location on the scanning
  machine** at scan time.
- **Measured on two targets, one nixpkgs revision, macOS only.** Linux binaries
  and closures containing more than one Go or Rust program are expected to
  behave the same and MUST be exercised before this ships.

## Out of Scope

- The crate-to-crate dependency edges inside a Rust binary. The binary path
  drops all 85 recorded for ripgrep today; that is an existing, separate
  defect. Until it is fixed, recovered crates relate to their member only.
- CPEs for recovered modules and any statement about vulnerabilities
  (#1039; such statements belong in VEX).
- A runtime-only closure mode (#1034).
- Realising absent outputs.
- Reading the fetched dependency bundle (`…-go-modules`, `…-vendor`) as a
  source of the same information. It was absent on the measuring host and
  would describe what was fetched rather than what was compiled in.
- Naming the document's subject and giving the scanned package a CPE. Both
  were delivered by milestone 1377 (#1398, #1394), not here.

## References

- #1396 (this), #1398 (subject), #1034 (closure depth and scope),
  #431 (store recognition), #1245 (artifact-derived identity), #1039 (VEX).
- `measurements/FINDINGS.md` — setup, commands, results, what was not
  established, and a probe error caught on the way.
