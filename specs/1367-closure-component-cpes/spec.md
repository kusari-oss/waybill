# Feature Specification: a CPE for the closure components a feed could match

**Feature Branch**: `1367-closure-component-cpes`
**Created**: 2026-11-06
**Status**: Draft
**Issue**: #1371 — *CPE synthesis runs before the nix-closure merge, so no closure component ever gets a CPE*

> **Numbering note.** Spec directory numbers and GitHub issue numbers are two
> separate sequences in this repository and do **not** correspond. This is spec
> **1367**; it implements issue **#1371**. Issue **#1367** is an unrelated
> complaint (the closure edge undershoot). Cite the issue, never the bare
> number.

## Context

Milestone 1366 (#1365) gave 74 of a nix closure's components a `pkg:pypi`
identifier a vulnerability feed can match. It left the C and system packages
out of scope on a stated ground: *"`openssl`, `curl` and `zlib` have no PURL
ecosystem; the right carrier is a CPE, which is a different mechanism and a
different decision."*

That carrier is empty. A `--nix-closure` scan emits **zero CPEs**, for every
component in the document.

Six components match milestone 097's curated CPE table **exactly by name**,
carry clean release versions, and get nothing. Measured on the pinned
`nix-closure-moat` closure:

| component | version | CPE today |
| --- | --- | --- |
| `openssl` | 3.6.2 | none |
| `curl` | 8.20.0 | none |
| `zlib` | 1.3.2 | none |
| `sqlite` | 3.51.2 | none |
| `pcre2` | 10.46 | none |
| `llvm` | 21.1.8 | none |

This is not a missing feature and not a gap in the table. The synthesis pass
runs over the component set **before the closure's components are added to
it**, so it never sees them. Every closure component is invisible to a
mechanism that already exists, is already tested, and would already identify
these six.

## Why it matters

These are the CVE-bearing core of a nix closure. By this repository's own
recorded probe data, `openssl:openssl` has 368 NVD dictionary entries,
`sqlite:sqlite` 372, `haxx:curl` 228 and `zlib:zlib` 84.

Milestone 1366's measured result was that 234 of 942 components became
answerable to a vulnerability query. The remaining 707 are decomposed below,
and the single largest actionable group is the one this feature addresses.

## The population, measured

On the pinned closure (aarch64-darwin, nixpkgs `a799d3e3`), of the 707
components still carrying `pkg:generic`:

| bucket | count | what a CPE would mean |
| --- | --- | --- |
| **closure members** — waybill's own C206 classification | **539** | nothing; see the verification below |
| non-member software, **already in the curated table** | **6** | a CPE today, if the pass could see them |
| non-member software with a release-shaped version, **not mapped** | **117 names** | a CPE only if the table grows |
| non-member software whose version is not a release | 6 | out of reach; a CPE's version field is a release |

The 6 are what fixing the ordering delivers on its own.

### The 539 were verified, not assumed

"Members never want a CPE" is a claim about waybill's classification, and a
classification can be wrong. Three independent checks, 2026-11-06:

1. **Do members carry versions?** **1 of 539 does** — `bootstrap-stage0-clang`,
   whose version is the literal `boot`. A CPE's version field is a release
   identifier, so 538 of 539 are excluded by FR-003 **regardless of their
   role**, and the 539th by its version's shape. FR-002 is therefore
   belt-and-braces rather than the load-bearing gate, which is worth knowing:
   a future change that made members version-bearing would not quietly start
   emitting CPEs for them.
2. **Is any member actually CVE-tracked software?** **0 exact-name matches**
   against the curated table or a list of advisory-dense names. 31 names
   *begin* with one — `bash-5.3.tar.gz`, `bash53-001`…`bash53-009`,
   `bzip2-1.0.8.tar.gz`, `c-ares-1.34.6.tar.gz` — and every one is a tarball
   or a patch, carries no version, and is classified `source`. The software
   those sources build is emitted **separately**: `bash 5.3p9` is a non-member
   component in the 117.
3. **Is the member classification itself sound?** Partly not, and it is now
   **issue #1374**: all 12 members classified `package` are source trees, a
   patch file and an environment, because that role is assigned by name shape
   alone. It does not affect this feature — none carries a version — but the
   role is exactly the signal a later milestone would reach for, so it is
   recorded rather than relied on.

**A correction this verification produced.** An earlier draft of this table
claimed a bucket of "35 build scaffolding — hooks, wrappers, bootstrap
stages". That was an **artifact of the analysis script**, not a waybill
classification: waybill does not distinguish those from other components, and
the grouping swept in `bootstrap_cmds 138`, which is real Apple software, on a
`startswith("bootstrap")` test. Four of that group carry release-shaped
versions — `pkg-config-wrapper 0.29.2`, `pkgconf-wrapper 2.5.1`,
`clang-wrapper 21.1.8`, `cctools-binutils-darwin-wrapper 1010.6` — and would
receive CPEs if their names were ever mapped. They are counted among the 117.

## Clarifications

### Session 2026-11-06

- Q: Does this feature also expand the curated mapping table, or only make the
  existing table reach closure components?
  → **A: Fix the ordering only.** The bug is that the synthesis pass cannot
  see closure components; that is what this feature fixes. Growing the table
  stays the one-line PR milestone 097 designed it to be, and becomes effective
  for closures for free once this lands.

**What this delivers**: the 6 measured components, and every future mapping
row reaching closures without further work.

**What it costs, stated plainly**: 117 more names stay unmatchable, including
`bash`, `bzip2`, `expat`, `brotli` and `c-ares`, which are advisory-dense. A
milestone that shipped 6 identifiers is a modest return, and the reason to
accept it is that conflating an ordering bug with a curation effort puts the
fix behind a judgement call about which of 117 names deserve a row.

**What it does not establish**: that the 6 are the right 6. A mapping row is a
name match, not a proof that the closure's `llvm 21.1.8` is the product NVD
calls `llvm:llvm`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer matches a closure's C libraries against NVD (Priority: P1)

Someone has an SBOM of a nix-built artifact and an NVD-backed scanner. They ask
which components are affected by a known OpenSSL advisory. Today the closure's
`openssl 3.6.2` carries no CPE and no ecosystem PURL, so the scanner cannot see
it at all. After this feature it carries a CPE candidate the scanner can match.

**Why this priority**: It is the feature. Every other story is a safeguard on
it, and the component in question is the single most advisory-dense piece of
software in a typical closure.

**Independent Test**: Scan the pinned closure and confirm the six
table-matching components carry CPE candidates whose vendor, product and
version correspond to what the closure records. Verifiable without the other
stories.

**Acceptance Scenarios**:

1. **Given** a closure containing a component whose name matches a curated
   mapping row and which carries a release version, **When** the closure is
   scanned, **Then** the emitted component carries a CPE candidate for that
   vendor, product and version.
2. **Given** that same closure, **When** the emitted document is queried for
   components carrying a CPE, **Then** the six are found — where before the
   query returned nothing at all.
3. **Given** a component that a reader other than the closure contributed,
   **When** it is emitted, **Then** its CPEs are exactly what they are today.

---

### User Story 2 - A component that is not software does not acquire a CPE (Priority: P1)

A closure is mostly not packages. 539 of its generic components are patch
files, source tarballs and build fragments, and 35 more are hooks, wrappers
and bootstrap stages. A CPE on any of those asserts that a named product at a
named version is present, which is false, and a scanner acting on it reports a
vulnerability in a patch file.

**Why this priority**: Equal to US1, because a wrong CPE is **worse than no
CPE**. An absent CPE is merely unmatchable; a wrong one matches the wrong
thing, and the consumer reaches a confident false conclusion. This is the same
argument milestone 1366 made for declining an unconfirmed ecosystem
identifier, applied to a carrier with no confirmation step available.

**Independent Test**: Scan the pinned closure and assert that no component
carrying a closure member role, and no build-scaffolding component, carries a
CPE.

**Acceptance Scenarios**:

1. **Given** a component carrying a closure member role, **When** it is
   emitted, **Then** it carries no CPE.
2. **Given** a component whose recorded version is not a release — a git
   revision, a date, a bootstrap marker — **When** it is emitted, **Then** it
   carries no CPE.

---

### User Story 3 - A component is not given two sets of CPEs (Priority: P2)

Synthesis today runs once, deliberately after deduplication, so a component
found by two readers gets one set of candidates rather than two. Reaching
closure components must not undo that.

**Why this priority**: Below the first two because the document is correct
without it in the common case, and because the hazard is narrow — it applies
only to a component that both another reader and the closure produced. It
earns its place because the existing placement is load-bearing and its reason
is recorded in a comment that a later edit could easily miss.

**Independent Test**: Scan a project where a component is contributed by both
a package-database reader and the closure, and confirm it carries one set of
CPE candidates, unchanged from today's.

**Acceptance Scenarios**:

1. **Given** a component that an earlier reader already supplied CPEs for,
   **When** the closure also contributes it, **Then** its CPEs are unchanged.
2. **Given** any component, **When** the document is emitted, **Then** no CPE
   value appears twice on it.

---

### Edge Cases

- **A nix version string NVD cannot match.** `libcxx 21.1.6+apple-sdk-26.4`,
  `libedit 20251016-3.1`, `kyua 0.13-unstable-2024-01-22`. These are real
  software whose nixpkgs version is not a release identifier in NVD's sense.
  FR-003 declines them; a candidate built from such a version could not match
  anything, and would misrepresent coverage.
- **A name that is a curated row but a different product.** `llvm 21.1.8` in a
  closure is the compiler; the curated row points at the NVD `llvm:llvm`
  namespace. The mapping is a name match, not a proof of identity, and the
  emitted value is a *candidate* rather than an assertion.
- **A version that is not a release.** `bash-interactive 5.3p9`, `cctools
  1010.6`, `apple-sdk 26.4`. A CPE's version field is a release identifier;
  anything else produces a candidate that cannot match.
- **A component the closure merged into another reader's.** It belongs to that
  reader. Recomputing its CPEs from the closure's view of it would overwrite
  better evidence with worse.
- **A closure with no software in the table.** The common case for many
  projects; the document must be unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST apply CPE synthesis to components contributed by
  the nix closure, using the same mapping the synthesizer already applies to
  components from every other source.
- **FR-002**: The system MUST NOT emit a CPE for a component carrying a
  closure member role. Measured: 539 of the 707 on the pinned closure. This
  requirement is deliberately redundant with FR-003 on the measured
  population — only 1 member carries a version at all — so that a later change
  making members version-bearing cannot quietly start emitting CPEs for
  tarballs.
- **FR-003**: The system MUST NOT emit a CPE for a component whose recorded
  version is not a release identifier.
- **FR-004**: The system MUST NOT change the CPEs of a component contributed
  by any other reader, including one the closure merged into.
- **FR-005**: A component MUST NOT carry the same CPE value more than once,
  and MUST NOT have CPEs computed for it more than once.
- **FR-006**: The system MUST emit CPEs for closure components in every output
  format that carries them today, with the same structure those formats
  already use.
- **FR-007**: A component with no mapping row MUST emit no CPE and no
  diagnostic, preserving the existing silent-skip behaviour.
- **FR-008**: The system MUST record, at document scope, how many components
  received CPE candidates — so a reader can tell a closure with no mapped
  software from a synthesis step that did not run.

### Key Entities

- **CPE candidate**: a vendor, product and version triple asserted as a
  possible NVD identity for a component, emitted alongside rather than instead
  of the component's PURL.
- **Mapping row**: the curated association between a software name and one or
  more NVD vendor/product pairs.
- **Synthesis coverage**: the per-document count of components that received
  candidates, and the count that were eligible but unmapped.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the pinned closure, the number of components carrying a CPE
  rises from **0** to at least the **6** that match the curated table today,
  and the exact number is **measured and recorded**, not predicted.
- **SC-002**: **Zero** components carrying a closure member role carry a CPE.
- **SC-003**: **Zero** components carry a CPE whose version field is not a
  release identifier.
- **SC-004**: Components contributed by other readers are byte-identical
  before and after, including their CPEs.
- **SC-005**: A scan of a project with no nix closure produces a
  byte-identical document.
- **SC-006**: `openssl`, `curl`, `zlib` and `sqlite` — the four most
  advisory-dense names in the measured population — each carry at least one
  CPE candidate, demonstrated end to end.
- **SC-007**: Disabling the feature makes a test fail. A population no test is
  sensitive to is not evidence that it is emitted.
- **SC-008**: Every corpus target that does not scan a closure produces
  byte-identical goldens.

## Out of scope

- **A name-as-vendor fallback for unmapped software.** Emitting
  `cpe:2.3:a:<name>:<name>:<version>` for any component with a version would
  reach ~117 more names, and would also emit candidates for things that are
  not NVD-tracked products at all. Milestone 097 chose curated-only
  deliberately (its FR-003), and reversing that is an accuracy decision with
  its own evidence requirements, not a side effect of fixing an ordering bug.
- **Vulnerability matching itself.** waybill emits a document a scanner can
  match; it does not perform the match.
- **The ecosystem-PURL population.** Milestone 1366 addressed those; a
  component that already carries an ecosystem-qualified PURL does not need a
  CPE to be matchable.
- **The 539 closure members and the 35 scaffolding components.** They are not
  software and must stay without CPEs; FR-002 and FR-003 state that as a
  requirement rather than leaving it to the mapping table.

## Assumptions

- **A CPE is a candidate, not an assertion.** The existing synthesizer emits
  multiple candidates per component precisely because no authoritative source
  of CPE identifiers exists; a downstream matcher takes the union. This
  feature inherits that posture and does not strengthen it.
- **The mapping table is the gate.** A closure component gets a CPE only if
  its name is already a curated row. Growing the table is a separate,
  ongoing effort that this feature makes effective for closures rather than
  undertakes.
- **Figures are from one closure and one system.** All counts are
  aarch64-darwin at a pinned nixpkgs revision. The corpus goldens pin
  x86_64-linux, which is a different closure — a figure from one is wrong in
  membership when quoted against the other.
- **The existing placement has a reason.** Synthesis runs after deduplication
  so a component found by two readers gets one set of candidates. Whatever
  makes closure components reachable must preserve that, which is why FR-005
  is a requirement rather than an implementation note.

## Evidence

Every count above was measured on 2026-11-06 and is reproducible:

- `specs/1366-closure-ecosystem-purl/measurements/` — the closure capture, the
  component decomposition and the comparison harness.
- `waybill debug closure-dump --closure-json <file>` — per-derivation reality
  from the reader's own functions.
- Issue #1371 — the six table-matching components, and the two code locations
  whose ordering explains why they carry nothing.
- Issue #1374 — the member-role misclassification this spec's verification of
  the 539 turned up. Adjacent, not blocking.
