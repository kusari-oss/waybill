# Specification Quality Checklist: Capture that survives a kernel change

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-29
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
- [ ] Success criteria are technology-agnostic (no implementation details)
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

Two items are deliberately marked incomplete. Both would have passed on a
literal reading, which is the kind of pass this project treats as worse than
a failure.

**"No [NEEDS CLARIFICATION] markers remain" — now PASS, on substance.** This
was failed on the first pass even though a grep for the literal string
returned zero, because the two open questions sat in their own section and
nothing was resolved by that formatting. Both have since been answered and
recorded under `## Clarifications`: adopt the IPv6 coverage the stable
interface provides, and narrow capture to opens that succeeded.

Folding those answers in surfaced two contradictions that the unanswered
spec had carried without anyone noticing, which is the argument for treating
this item as substantive rather than a grep:

- **SC-001 and story 1 scenario 3 both compared against "at least 902
  file-open events".** That figure was measured under request-based
  semantics. Narrowing to successful opens makes a *lower* count correct, so
  both would have failed a working implementation. Restated as content —
  every crate root present — with the count moved to SC-001a as a ratio
  against a baseline an implementation task must establish, since no
  successful-opens-only measurement exists yet.
- **FR-004 forbade losing any entry the current build records**, which is
  precisely what narrowing does on purpose. Now excepts failed requests
  explicitly, with FR-004a stating the new rule and FR-004b covering the
  host where the result of an open cannot be determined — that case degrades
  to requests and records a gap rather than guessing.

**"Success criteria are technology-agnostic" — FAIL, and accepted.** SC-002
and SC-003 name the runner image and the CI lane. That is not incidental
leakage that can be rewritten away: the measurable outcome of this feature
*is* that the observation no longer depends on the host environment, and the
runner image is the variable that was shown to determine it. Stating the
criterion without naming that variable would make it unverifiable. Recorded
as a known deviation rather than reworded into something vaguer.

The clarify session narrowed where that dependence is allowed to sit. SC-002
no longer *tests* against an image version — the failing image is leaving
the pool, so a criterion pinned to it would go quiet rather than red — and
is now verified by making an interface unavailable on purpose. The 14/10
split stays as the historical baseline that motivated the work. Story 3's
independent test and first acceptance scenario were restated to match.

**On verifying a deliberate loss of entries.** SC-001b permits the read sets
to shrink by the failed requests they used to carry, which is a diff that
could hide a join bug dropping successful opens as well. Accepting it on
inspection, or reconciling only its size against a discard count, would pass
for both a correct implementation and a broken one. FR-004c therefore
requires capture to be able to report each discarded request *with the path
and the result that caused the discard*, and SC-001b is verified entry by
entry against that list.

**On "written for non-technical stakeholders"** — marked pass, with the
caveat that the subject matter is kernel observation. The spec is written so
that a reader who does not know what a kprobe is can still follow it: it
speaks of "capture interfaces" that are either usable or not on a given host,
and of whether a recorded input set can be trusted. Kernel specifics appear
only in the evidence section, where they are the measurement.

**On measurability** — every number in this spec is traceable to an
observation, per the repository's standing rule: 902 and 443 file-open events
from the integration harness on two runner images; 100 runs classified, 84
passed / 11 failed / 3 cancelled / 2 without the lane; image attribution read
from job logs for all 11 failures and a 14-pass sample. The spec states that
the pass side is a sample. One of the 11 failures (2026-10-14) has a
different, unidentified cause and is not claimed by this feature, but is
still counted against the SC-003 baseline rather than excused.
