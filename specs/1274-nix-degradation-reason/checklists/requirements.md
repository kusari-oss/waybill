# Specification Quality Checklist: the nix-closure tier says why it degraded

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-03
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous
- [X] Success criteria are measurable
- [X] Success criteria are technology-agnostic (no implementation details)
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified
- [X] Scope is clearly bounded
- [X] Dependencies and assumptions identified

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification

## Notes

**On naming code in a spec.** The spec names `salient_stderr`,
`out.stderr.trim().chars().take(200)` and two file paths. That is a
deliberate exception to "no implementation details", and it is load-bearing
rather than decorative: the feature's central fact is that **one fix exists
and was not applied at the second site**, which cannot be stated without
identifying the two sites. A reader who is told only "the detail is wrong"
would reasonably design a new rule, which is precisely the duplication
FR-008 forbids. Everything else is stated behaviourally.

**On the measured numbers.** Every figure in the Context table and in SC-001
through SC-003 comes from `measurements/reproduce-1283.sh`, run at 0.14.0 on
`aarch64-darwin` against nixpkgs `a799d3e3`. The probe is committed and
builds its own fixtures, so the findings are re-runnable rather than
asserted.

**The probe carries a control, and needed one.** A `callCabal2nix` flake that
degrades proves nothing on its own — a broken fixture degrades too. The
handwritten flake evaluates 1,170 derivations and appends 303 components in
the same run, which is what makes the subject's degradation attributable to
the refusal.

**The probe's first version contradicted itself.** It printed nix's refusal
and then `nix did NOT refuse` on the following line: `grep … | head -1 | sed
… || echo` fires the `||` branch because `head -1` closes the pipe and grep
takes SIGPIPE, so a successful match still exits non-zero. Fixed by capturing
into a variable, and the script now **exits 1** if the control assumption
fails rather than carrying on to report findings about something else. Noted
because a harness that contradicts itself one line apart is worse than one
that says nothing.

**One clarification was asked and answered in-session**: whether the new
reason should name import-from-derivation specifically or describe a general
"declined by policy" class. Specific, because the operator's next action
differs per cause — which is why `DegradationReason` is a closed set and why
`offline-requested`, `tool-absent` and `ifd-refusal-unverified` are already
three variants rather than one.

**Deliberately narrow.** #1283's second question (whether to support these
flakes), #1282, and #1340 are all listed Out of Scope with the reason. The
spec's own argument for the split is that answering "why did it degrade" is
worth having under *every* answer to "should we support it", so it does not
need the scoping decisions (#971, #1034) that the rest does.

**FR-015 asks for a negative test specifically.** An over-matching classifier
converts one uninformative answer into one misleading answer, which is a
worse failure than the current state — so "an evaluation failure that is not
a refusal keeps the generic reason" is a requirement rather than an
afterthought.
