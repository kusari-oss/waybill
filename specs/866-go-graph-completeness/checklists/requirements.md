# Specification Quality Checklist: Go scans assert dependency edges they never read, and then report the result as complete

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-13
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

### Analyze session 2026-09-13

Eight findings, zero CRITICAL, all remediated. Two mattered:

- **Edge provenance moved into the MVP** (C3). plan.md had sequenced it
  as a separate later phase; tasks.md had folded it into US1. The two
  documents disagreed on what "ship the MVP" meant. Resolved in favour
  of MVP inclusion: backed edges without stated backing ask a consumer
  to take the correction on trust, which replaces one unverifiable claim
  with another. plan.md now records why the original sequencing was
  wrong. US1 gained two acceptance scenarios (C5), since FR-002a
  previously had none anywhere in the spec.
- **T020 was not executable as written** (C1). It put a `kubernetes`
  regression test in `waybill-cli/tests/`, but kubernetes is not a
  public-corpus target — no golden exists — and its 396 MB clone cannot
  sit in the default lane. Moved to the m770 quality-corpus lane. It
  cannot be dropped in favour of cobra: SC-001 names both targets, and
  only the 39-module workspace exercises the `replace` handling.

Also fixed: plan/tasks phase numbering collided so "Phase 3" meant
different things (C2); three warm-cache tests needed network and a Go
toolchain without saying so, against a spec assumption that read as
project-wide (C4); plan.md promised a performance ceiling it never set,
now deferred to T047 rather than shipping an unmeasured number (C6);
`reason_codes.rs` was edited by a task but missing from the plan's tree
(C7); SC-003's overlap with SC-001 needed its distinguishing case named
(C8).

Post-remediation: 47 tasks, sequential, 0 malformed, 0 dangling
cross-references, 17/17 FRs and 10/10 SCs covered.

### Clarify session 2026-09-13

Three questions asked and answered; all integrated into the spec.
Requirements grew 12 → 17 and success criteria 8 → 10, so the session
changed the feature rather than merely confirming it:

- **Backing definition** resolved as a general *requirer → required*
  rule, with `go.sum` explicitly excluded. Added a second requirement in
  the same answer: edge provenance must be recorded in the document, not
  merely checked at build time (FR-002a/b, SC-003a).
- **Scope** kept universal as a principle, enforced for Go, with the
  other ecosystems measured rather than assumed (FR-013, FR-014,
  SC-006a). This corrected a real overclaim: the scope-boundary table
  had said the cmake/uv targets have "no edges to fabricate", but the
  edge-truth probe parses `go.mod` and never examined their own
  manifests. The table now says so.
- **Stranded components** get no synthesised relationship; the existing
  orphan machinery marks them (FR-007, FR-007a, SC-005). Surfaced a
  three-way distinction that must not collapse: parent-unknown vs.
  genuinely-no-dependencies vs. edge-derived-by-a-weaker-tier.

Terminology normalised to "unbacked" in normative text.

### The earlier clarification, and the correction to it

FR-006 originally asked whether unresolvable indirect requires should be
attached to the main module or left unattached. It was answered
"unattached — fix the reporting only", on the stated premise that
attaching them was a *future* risk to avoid.

Re-measurement showed that premise was wrong: m860 already attaches
them, so the chosen principle was not status-quo-preserving but a
request to **remove** edges that exist today. The question was re-put
with corrected evidence and answered again: remove the unbacked edges.

The spec now reflects the second answer. The original framing is not
preserved in the spec body because it described a product state that no
longer exists; it is recorded here so the reversal is not mistaken for
drift.

### Stale evidence was the main risk to this spec

The first draft was built on #857 and #829's figures, which predate
m860. So did the binary on disk. Rebuilding and re-measuring changed
three substantive claims:

| claimed | actual on current main |
|---|---|
| graph is flat, depth 1 | depth 2 |
| 72% of edges lost | cobra emits the same 7 edges cold and warm; they are misattributed, not lost |
| 6 of 8 components orphaned | all 6 are attached; the marker is provenance, not reachability |

The defect is real and worse than described — waybill asserts
relationships it never read — but none of the original three symptoms
survived contact with a current build. Recorded in
`measurements/README.md` under "A warning about stale evidence".

### Two P1 stories, deliberately

Stories 1 and 2 are both P1. The template prefers a single highest
priority, and the split was considered. They are kept equal because
shipping Story 1 alone leaves the document declaring `complete` over a
graph whose edges have just been *removed* — strictly worse for a
consumer than today. Neither is independently shippable to a user, even
though each is independently testable.

### Implementation-detail judgement calls

- Named properties appear only in evidence and measurement sections,
  which report what was observed on disk. The requirements refer to "the
  completeness declaration", "a per-ecosystem coverage signal", "a
  declared requirement" — the observable contract.
- The "invented edges cause the false declaration" section names the
  reachability mechanism. Retained deliberately: it is the causal link
  between the two issues and the reason FR-004 exists. Without it FR-004
  reads as arbitrary. It is diagnosis, not prescription.

### Success criteria that guard against the wrong fix

- **SC-004** exists because the cheapest way to satisfy SC-002 is to
  stop emitting `complete` at all. The warm-cache control proves a
  correct graph can still earn it.
- **SC-005** exists because the cheapest way to satisfy SC-001 is to
  drop the stranded components entirely rather than report them.
- **SC-007** exists because the cheapest way to generalise this fix is
  to flag every shallow graph, which would mislabel the two targets
  measured as legitimately topology-free.

### Baselines

Every success criterion carries a measured baseline from the current
build. No figure in this spec is derived arithmetic presented as
observation, and the one figure taken from someone else's measurement is
attributed where it appears.
