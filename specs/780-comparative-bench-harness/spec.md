# Feature Specification: Private comparative benchmark harness

**Feature Branch**: `780-comparative-bench-harness`
**Created**: 2026-09-09
**Status**: Draft
**Input**: User description: "Private, deterministic comparative benchmark harness measuring waybill against other SBOM tools without publishing results"

## Context

An ad-hoc comparison of waybill against three other SBOM generators produced
six different conclusions in a single afternoon — waybill was variously the
fastest tool tested, 5× slower, finding 22% of the packages, finding more
packages than anyone, and equivalent on licences. Every reversal came from
the method rather than the tools:

- **Raw component counts were compared across tools that count different
  things.** One tool reported 2,355 Go components where 427 distinct modules
  existed; it emits each module once per manifest that requires it. The
  "waybill finds 22%" conclusion was an artefact of this and was wrong by
  roughly a factor of five.
- **Two measurements ran concurrently**, inflating one by 2.7×.
- **A conclusion drawn on a 17 MB project was generalised to a 397 MB one**,
  where it did not hold.
- **One tool's cheapest mode was compared against another's default.**
- **Repeat runs of an identical command differed by 1.5×** and nothing
  flagged it.

The cost of this is not wasted time. It is that any confident claim —
"faster than the alternatives", "more accurate than the alternatives" —
currently rests on whichever measurement was taken last. Publishing such a
claim and then discovering the test was wrong is materially worse than
having no claim at all, because it spends credibility that is difficult to
recover.

This feature builds the missing instrument: a comparison that produces the
same answer twice, refuses to answer when it cannot do so reliably, and
keeps its output private until someone deliberately decides otherwise.

## Clarifications

### Session 2026-09-09

- Q: Where should the harness live, given the repository is public? → A:
  In-repository but **tool-agnostic** — the harness compares waybill against
  a set of tools defined in operator-supplied configuration, and the
  committed source names no specific competing tool. Useful development
  tooling stays versioned alongside the code it measures, while the
  competitive framing stays out of a public repository. A private sibling
  repository was rejected as drifting from the code under measurement;
  naming tools in committed source was rejected because at least one
  candidate tool sits in the project's ambiguous external-naming tier.

- Q: Enrichment modes depend on third-party APIs whose latency is not
  reproducible. Can they be measured deterministically? → A: No, and the
  harness must not pretend otherwise. Measure both, but separate the
  claims: **offline timings** are gated and authoritative; **enriched
  timings** are recorded as *indicative*, carry a wider tolerance, and are
  marked non-reproducible. **Coverage metrics from enriched runs remain
  authoritative** — which packages receive licences is stable even when
  latency is not.

- Q: What counts as one package, given FR-006 requires a shared reduction
  rule but does not state it? → A: The full package identity **including
  version**, normalised (lowercase type, qualifiers dropped, consistent
  namespace form). `foo@1.0` and `foo@2.0` count as two. The duplication
  that motivated this feature was the same name *and* version repeated once
  per manifest, so identity-with-version removes it without discarding real
  distinctions; version-stripping would understate a tool that correctly
  resolves several versions of a package in a monorepo.

- Q: What constitutes the truth set for a target, given several defensible
  derivations exist with materially different results? → A: The derivation
  method is **declared per target and recorded with every score**, and the
  harness refuses to compare scores produced by different methods. No single
  method is mandated globally: a lockfile union is always available but is a
  superset of what is actually built, while querying the ecosystem's own
  resolver is authoritative but needs a toolchain and often network. Each
  target uses the strictest method available to it, and which one was used
  is visible next to the number rather than assumed.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A measurement that survives being repeated (Priority: P1)

An engineer runs the comparison twice on the same machine and gets the same
answer. When conditions prevent that — a noisy host, a target whose truth
cannot be established, a tool that failed — the harness says so instead of
producing a number.

**Why this priority**: Everything else is worthless without it. A harness
that yields a different verdict per invocation is not a slower version of a
good harness; it actively misleads, because each run looks authoritative in
isolation.

**Independent Test**: Run the comparison twice on an unchanged tree and
confirm the reported figures agree within the harness's own stated
tolerance, and that the second run's verdict matches the first's.

**Acceptance Scenarios**:

1. **Given** an unchanged target and toolset, **When** the comparison is run
   twice, **Then** both runs produce the same verdict and figures agreeing
   within the declared tolerance.
2. **Given** a run whose repeat measurements disagree beyond tolerance,
   **When** results are reported, **Then** the harness declines to state a
   comparative verdict and names the tool and metric whose spread was too
   wide.
3. **Given** two tools measured in the same session, **When** timings are
   compared, **Then** they were executed one at a time, never concurrently.
4. **Given** a host that is not of the reference class, **When** the
   comparison is run, **Then** figures are recorded but the verdict is
   withheld with the host class named.

---

### User Story 2 - Counting the same things (Priority: P1)

Comparisons are made on identities that mean the same thing across tools —
distinct packages — rather than on raw output counts that reflect each
tool's internal representation.

**Why this priority**: This single defect produced the largest and most
embarrassing reversal. It is not a refinement of the measurement; without
it the measurement is of the wrong quantity.

**Independent Test**: Feed the harness output from a tool known to emit
duplicate entries per package and confirm the reported count matches the
distinct package count, not the raw entry count.

**Acceptance Scenarios**:

1. **Given** a tool that emits the same package once per manifest that
   requires it, **When** its output is scored, **Then** the package counts
   once.
2. **Given** two tools whose outputs are scored, **When** counts are
   compared, **Then** both were reduced to distinct package identities by
   the same rule.
3. **Given** a component carrying no package identity, **When** counts are
   reported, **Then** it is reported separately rather than silently
   inflating or deflating either tool's total.

---

### User Story 3 - Scoring against truth, not against each other (Priority: P2)

Where the true set of packages for a target can be derived from the target
itself, each tool is scored against that truth — so "found more" becomes
"was more nearly right".

**Why this priority**: Tool-versus-tool comparison cannot distinguish one
tool finding more from another finding things that are not there. An
external anchor removes the judgement call that produced several of the
reversals.

**Independent Test**: On a target whose true package set is known by
construction, confirm each tool's reported score matches a hand-computed
one.

**Acceptance Scenarios**:

1. **Given** a target whose true package set is derivable, **When** tools
   are scored, **Then** each receives a score reflecting both what it found
   and what it reported that is absent from truth.
2. **Given** a target whose truth cannot be derived, **When** results are
   reported, **Then** counts are shown, accuracy scoring is omitted, and
   the omission is stated.

---

### User Story 4 - The harness proves itself before judging anyone (Priority: P2)

Before scoring any tool, the harness verifies on a fixture of known content
that it recovers the known answer. If it cannot, it scores nothing.

**Why this priority**: Every reversal in the motivating episode was a
harness defect, not a tool defect. A measuring instrument that cannot
demonstrate its own accuracy has no standing to rank anything.

**Independent Test**: Deliberately break a scoring rule and confirm the
self-check fails and the run aborts before any tool is measured.

**Acceptance Scenarios**:

1. **Given** a fixture whose true package set is known by construction,
   **When** the harness runs, **Then** it first confirms it recovers that
   set exactly.
2. **Given** a self-check that fails, **When** the harness runs, **Then**
   it aborts with the discrepancy named and measures no tool.

---

### User Story 5 - Results stay private by construction (Priority: P1)

Running the comparison cannot accidentally publish its results. Output
lands only where published artefacts do not.

**Why this priority**: The repository is public, so anything committed —
and any continuous-integration artefact — is world-readable. Privacy has
to be structural; a convention that results "shouldn't be committed" fails
the first time someone runs the obvious command.

**Independent Test**: Run the comparison, then confirm the working tree is
clean and no result file is in a location that would be published.

**Acceptance Scenarios**:

1. **Given** a completed comparison, **When** repository status is checked,
   **Then** no new tracked or untracked-but-publishable file exists.
2. **Given** the committed source, **When** it is read, **Then** no
   specific competing tool is named; the comparison set comes from
   operator-supplied configuration that is itself not published.
3. **Given** results, **When** they are read, **Then** they state measured
   quantities with conditions and uncertainty, and do not assert
   superiority.

---

### User Story 6 - Comparing like with like (Priority: P3)

Tools are compared in explicitly matched configurations, so a tool's
cheapest mode is never set against another's richest.

**Why this priority**: Corrective, and narrower than the others, but it
produced one of the reversals and is cheap to prevent.

**Independent Test**: Request a comparison of mismatched modes and confirm
the harness declines or labels the mismatch prominently.

**Acceptance Scenarios**:

1. **Given** tools configured in differing enrichment modes, **When** they
   are compared, **Then** the mismatch is stated with the results.
2. **Given** a tool run in more than one mode, **When** results are
   reported, **Then** each mode is a separate row, never merged.

### Edge Cases

- A tool exits non-zero, times out, or produces unparseable output — it
  must be recorded as failed rather than scored as finding nothing.
- A tool is absent from the machine.
- A configured tool's version differs from the version recorded on a prior
  run being compared against.
- The target's truth set is derivable for one ecosystem but not others in
  the same tree.
- A target contains several ecosystems and tools differ in which they cover.
- The first run of a tool is slower than later ones because of cold caches.
- Repeat runs disagree; the harness must distinguish this from a real
  difference between tools.
- Two tools are accidentally invoked concurrently.
- Results from different host classes are compared.
- A third-party enrichment service rate-limits or degrades mid-run, so a
  tool's timing reflects the service rather than the tool — and, worse, its
  coverage silently drops because lookups failed. Degraded coverage must be
  distinguishable from a tool genuinely finding less.
- A third-party service is unreachable entirely, making an enriched mode
  unmeasurable rather than slow.

## Requirements *(mandatory)*

### Functional Requirements

**Determinism**

- **FR-001**: Every timed measurement MUST be repeated a configured minimum
  number of times, and the harness MUST report both a central value and the
  observed spread.
- **FR-001a**: Repeated measurements of different tools on the same target
  MUST be interleaved rather than run in per-tool blocks, so that drift in
  machine conditions affects every tool alike instead of biasing whichever
  ran first.
- **FR-001b**: Timing MUST be reported as a within-session ratio between
  tools, accompanied by its spread. Absolute timings MUST be recorded as
  context and MUST NOT be the basis of a comparative statement, because
  absolutes on a non-reference host have been observed to vary by more than
  a factor of two between runs.
- **FR-001c**: The spread gate applies to timing measurements only. Coverage
  and accuracy metrics MUST be identical across repeats; a difference is a
  defect and MUST fail the run rather than be averaged or tolerated.
- **FR-002**: When observed spread for any measurement exceeds a configured
  tolerance, the harness MUST withhold the comparative verdict and name the
  affected tool and metric.
- **FR-002a**: Timing measurements MUST be classified by whether the mode
  under measurement contacts a third-party service. Offline timings are
  authoritative and subject to the standard tolerance. Timings for modes
  that contact third-party services MUST be labelled indicative, carry a
  separately configured wider tolerance, and MUST NOT be used to state that
  one tool is faster than another.
- **FR-002b**: Coverage and accuracy metrics derived from a run that
  contacted third-party services remain authoritative, because the set of
  packages a tool resolves does not depend on how long the service took to
  answer. The harness MUST report such metrics without the indicative
  caveat that applies to their timings.
- **FR-003**: Measured runs MUST be executed one at a time. The harness MUST
  NOT run two measured invocations concurrently.
- **FR-004**: Each run MUST record the host class, and MUST withhold the
  comparative verdict when the host is not of the reference class.
- **FR-005**: Each run MUST record the exact version of every tool measured
  and the exact revision of every target, and MUST refuse to compare results
  whose tool versions or target revisions differ.

**Comparable measurement**

- **FR-006**: Package counts MUST be computed over distinct package
  identities, with the same reduction rule applied to every tool. A package
  identity is the full package URL **including version**, normalised so that
  cosmetic differences between tools do not register as different packages:
  lowercase type, consistent namespace form, and qualifiers removed. Two
  versions of the same package are two identities.
- **FR-006a**: The harness MUST report, alongside every distinct count, the
  raw entry count it was reduced from. A large gap between the two means a
  tool emits duplicates, which is worth seeing rather than silently
  normalising away.
- **FR-007**: Components carrying no package identity MUST be counted and
  reported separately, never folded into a tool's package total.
- **FR-008**: Where the true package set for a target is derivable from the
  target, the harness MUST score each tool for both what it found and what
  it reported that truth does not contain.
- **FR-008a**: Each target MUST declare which method derived its truth set.
  The method MUST be recorded with every score computed from it, and the
  harness MUST refuse to compare scores whose truth sets were derived by
  different methods.
- **FR-008b**: Where a target supports more than one derivation method, the
  harness MUST use the strictest available and record that choice. A method
  that yields a superset of what is actually built MUST be labelled as such,
  because scoring against a superset penalises a tool for correctly omitting
  what the build does not use.
- **FR-009**: Where truth is not derivable, the harness MUST report counts
  without accuracy scoring and MUST state that accuracy was not scored.
- **FR-010**: Tools MUST be compared only in explicitly matched
  configurations; any mismatch MUST be stated alongside the results.
- **FR-011**: A tool that fails, times out, or emits unparseable output MUST
  be recorded as failed and MUST NOT be scored as having found nothing.

**Self-validation**

- **FR-012**: Before measuring any tool, the harness MUST verify against a
  fixture of known content that it recovers the known package set exactly.
- **FR-013**: A failed self-check MUST abort the run before any tool is
  measured, and MUST name the discrepancy.

**Privacy**

- **FR-014**: Results MUST be written only to locations excluded from
  publication, and the harness MUST NOT write into published documentation.
- **FR-015**: Committed source MUST NOT name any specific competing tool;
  the comparison set MUST come from operator-supplied configuration that is
  itself excluded from publication.
- **FR-016**: The harness MUST NOT be wired into any automation whose
  artefacts are publicly readable.
- **FR-017**: Reported output MUST state measured quantities together with
  their conditions and uncertainty, and MUST NOT assert that any tool is
  superior to another.

### Key Entities

- **Comparison Run**: One execution — the targets, the tools and their
  versions, host class, timestamp, and every measurement taken.
- **Tool Configuration**: An operator-supplied description of a tool to
  measure: how to invoke it, which mode that represents, and how to locate
  its output. Not published.
- **Target**: A tree to scan, pinned to an exact revision, optionally with a
  derivable truth set.
- **Truth Set**: The packages a target genuinely contains, derived from the
  target rather than from any tool's output. Carries the method that derived
  it and whether that method is exact or a superset.
- **Measurement**: One tool, one target, one mode — repeated runs, their
  central value and spread, distinct package count, identity-less component
  count, accuracy scores where available, and outcome.
- **Verdict**: The harness's statement of whether the run's preconditions
  were met, and if not, which failed.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Two consecutive runs of an offline mode on an unchanged tree
  produce identical verdicts and timings agreeing within the declared
  tolerance.
- **SC-001a**: Two consecutive runs of an enriched mode on an unchanged tree
  produce identical coverage and accuracy figures, even where their timings
  differ; timings are reported as indicative and are never the basis of a
  speed comparison.
- **SC-002**: Given output containing the same package identity repeated
  once per manifest that requires it, the reported distinct count equals the
  number of distinct identities, and the raw count is reported alongside it
  — the specific error that produced a five-fold misstatement is not
  reproducible.
- **SC-002a**: Given a target legitimately containing two versions of one
  package, both are counted; the reduction rule does not merge them.
- **SC-003**: Deliberately corrupting a scoring rule causes the self-check to
  fail and the run to abort before any tool is measured.
- **SC-004**: A run whose repeat measurements disagree beyond tolerance
  produces no comparative verdict, and names what disagreed.
- **SC-005**: After any run, the repository working tree contains no new
  publishable file.
- **SC-006**: Committed source contains no name of any specific competing
  tool.
- **SC-007**: On a fixture whose true package set is known by construction,
  every tool's accuracy score matches an independently hand-computed score.
- **SC-007a**: Every accuracy score is displayed with the truth-derivation
  method that produced it, and an attempt to compare scores derived by
  different methods is refused rather than silently performed.
- **SC-008**: A tool that fails is reported as failed, and is absent from
  accuracy scoring rather than scored as having found nothing.
- **SC-009**: Every reported figure is accompanied by the conditions under
  which it was obtained — target revision, tool version, mode, host class,
  repeat count, spread.

## Assumptions

- Tools are already installed by the operator; the harness measures what is
  present rather than installing anything.
- The comparison is run deliberately by a person, not on a schedule. This
  follows from the privacy requirement, since scheduled automation in this
  project publishes readable artefacts.
- Derivable truth is available for at least one ecosystem to begin with;
  the design should permit adding others without rework. Truth derivation is
  per-ecosystem and per-target, so a tree spanning several ecosystems may be
  scored for some and counted-only for others.
- Existing benchmark infrastructure in the project — its result schema, its
  host-class classification, and its target caching — is reused rather than
  duplicated.
- "Reference class" carries the same meaning here as in the existing
  benchmark tooling.
- Comparative figures are for internal decisions. Publishing any of them is
  a separate, deliberate act outside this feature.

## Out of Scope

- Publishing comparative results anywhere public.
- Any claim about waybill's standing relative to other tools. This feature
  builds the instrument; conclusions are drawn by people afterwards.
- Optimising waybill in response to what the harness measures.
- Measuring anything other than SBOM generation — vulnerability scanning,
  policy evaluation, and signing are all excluded.
- Replacing the existing single-tool performance benchmark, which continues
  to serve regression detection against waybill's own history.
