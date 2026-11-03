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

## Clarification round, 2026-11-03

Four clarifications now recorded — one during spec-writing, three in the
`/speckit.clarify` pass. **All three of the new ones produced requirements
rather than wording**, and two of them found defects in the spec itself.

| # | question | answer | produced |
|---|---|---|---|
| 1 | where the explanatory detail lives | document **and** log, inside the existing annotation | FR-002a, FR-002b, FR-016; SC-002 sharpened |
| 2 | what the classifier anchors on | the disabled-clause, on an error line only | FR-005a, FR-005b; SC-004 and an edge case sharpened |
| 3 | precedence when both failures appear | the refusal wins, detail keeps both lines | FR-005c; a dangling cross-reference repaired |

**Q1 changed the scope.** Putting the detail in the document means the
annotation's shape goes from a bare string to an object, so committed goldens
churn and any consumer keying on the string breaks. FR-016 was added for the
goldens, and FR-002b records that extending a row with three existing
extractors is not *adding* a row, so FR-014's condition stays out of play.
The log-only alternative was rejected because this feature exists precisely
because the *artifact* cannot say why the tier declined.

**Q2 was measured before it was asked.** The question names a false positive
I verified during the session rather than one I imagined: a flake whose
`nixConfig` requests `allow-import-from-derivation` and then fails on a
missing attribute emits

```
warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.
error: attribute 'this-attribute-does-not-exist' missing
```

so the obvious anchor — the bare setting name — reports a missing attribute
as a policy refusal. Real flakes carry such settings (`slack-web` sets this
one), so it is not hypothetical, and FR-005's own warning about
over-matching would have been violated by the simplest implementation.

**Q3 found a cross-reference to a rule that did not exist.** Assumptions
claimed precedence was "settled by FR-005's ordering"; FR-005 contained no
ordering. The gap was invisible from FR-005's own text and surfaced only
because the pass went looking for the rule the sentence pointed at. FR-005c
now states the precedence, and the Assumptions paragraph records the error
rather than quietly replacing it — a spec that cites itself can be wrong
about itself, and the citation reads as settled either way.

**One clarification was asked and answered during spec-writing**: whether the
new reason should name import-from-derivation specifically or describe a
general "declined by policy" class. Specific, because the operator's next
action differs per cause — which is why `DegradationReason` is a closed set
and why `offline-requested`, `tool-absent` and `ifd-refusal-unverified` are
already three variants rather than one.

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
