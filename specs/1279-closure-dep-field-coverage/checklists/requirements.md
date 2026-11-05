# Specification Quality Checklist: `unreferenced` must mean "nothing depends on this"

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-05
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

**Two items fail, and they are the same item.** Both open questions are
scope-defining rather than detail-level, which is why the scope box is also
unticked:

1. **The cross-compilation triples.** `depsTargetTarget` and
   `depsTargetTargetPropagated` name dependencies of the *target* platform —
   what a compiler being built hands to things it later compiles. They are
   neither in the artifact nor tooling that built it. Classifying them under
   an existing role is a judgement; adding a fifth role value changes the C182
   closed enum and every consumer of it, including invariant I10. The three
   options differ in blast radius by an order of magnitude.
2. **stdenv's defaults.** `defaultNativeBuildInputs` and
   `defaultBuildInputs` carry **323 of the 353** unread declarations, on 19
   derivations. They are stdenv's own machinery, not a package's declared
   dependencies. Classifying them grows `build-tooling` substantially;
   *not* classifying them but marking the distinction satisfies #1346's actual
   ask at a fraction of the change. Which one is wanted determines whether
   this milestone is small or large.

Neither has a defensible default, so neither was guessed. The spec is
otherwise complete and the baseline is measured, not estimated: the census
probe is committed at `measurements/attribute-census.py` with its invocation
and date, and reports 6 populated unconsulted attributes carrying 353
store-path declarations, against the issue's original "7 of ~20" estimate.

Worth noting for the clarify session: the measured distribution argues
*against* treating the six as one population. One attribute carries 88% of
the declarations and is the least package-like of them; the propagating one
carries 7 declarations but is the only one whose effect is not local. A
requirement written over "the six" would be wrong in both directions.


## Clarify session 2026-11-05

Two questions asked, two answered; both open items closed and the scope box
with them. **16 of 16.** Sections touched: Clarifications, Functional
Requirements (FR-010 … FR-013 added), Success Criteria (SC-010, SC-011),
Edge Cases, Assumptions.

| Taxonomy category | Before | After |
|---|---|---|
| Functional scope & behaviour | Partial — two scope-defining decisions open | Resolved |
| Domain & data model | Partial — whether the role enum gains a fifth value | Resolved: it does not |
| Terminology & consistency | Partial — a fifth role would need a name | Resolved, moot |
| Interaction, non-functional, integration, edge cases, constraints, completion signals, placeholders | Clear | Clear |

**I reversed my own recommendation between writing the spec and asking.** The
spec's first draft leaned toward classifying the cross-compilation triples as
`build-tooling`. Preparing the question made the consequence explicit:
milestone 1277 asserts CycloneDX `scope: "excluded"` for tooling roles, and a
cross-compiler ships its target-platform dependencies inside its own output,
so that assertion can be false — the defect class of #1342 and #1348, which
this very milestone sits downstream of. `artifact-input` asserts no scope and
therefore makes no false claim. Recorded because the reversal came from asking
"what does this assert?" rather than "what is it semantically", and that is
the question that should have been asked first.

**Both answers reduced scope rather than growing it**, which is worth noting
against the instinct to be thorough: 323 of the 353 unread declarations are
now addressed by disclosure instead of classification, so the census moves
only for the 30 that genuinely change a role, and SC-011 can assert that the
stdenv pair contributes exactly zero movement. A version of this milestone
that classified everything would have been larger, slower, and unable to
attribute its own census change.

One deliberate debt is recorded in Assumptions rather than hidden:
`artifact-input` now covers two distinguishable things, and if a genuinely
cross closure makes that imprecision expensive, a fifth role value is the
follow-up and should carry its own milestone.


## Plan session 2026-11-05

Phase 0 and Phase 1 complete. All 16 items still pass after the plan's edits
to the spec. Sections touched in the spec: *Why this exists* (system labels),
Success Criteria (SC-012, SC-013 added; SC-001 and SC-010 amended),
Clarifications (one ratio labelled by system).

Three Phase 0 findings changed the spec rather than confirming it:

- **The role delta was measured, and `artifact-input` falls by two.** Three of
  its members gain a tooling reference and become `both` while only one
  arrives. SC-012 now states the direction explicitly, because a reviewer
  checking the arithmetic would otherwise read a negative delta as an error.
- **Exactly one component loses a false exclusion, and it is `numactl`.**
  SC-013 names it. A criterion reading "the false exclusions are retracted" is
  how milestone 1278 shipped a count of nine for a population of six, and the
  fix is to name rather than to count.
- **Every figure in the spec was darwin-measured and unlabelled.** The FR-002
  population is **3 on linux against 16 on darwin** — five-fold — and the
  goldens pin linux. Labels added throughout and the criteria restated against
  linux. This is the same class of defect as #1348's own research, which
  quoted a census without its system and had to be retracted.

One design option was considered and rejected in Phase 1, recorded because it
was tempting and nearly taken: emitting a real `dependsOn` edge for the
unclassified references instead of a marker. Both CDX and SPDX can express it
natively, and Principle V prefers native carriers — but an edge is a stronger
claim than this milestone is entitled to make, and it would put stdenv's
machinery into the dependency graph of every derivation stdenv touches. The
native-first rule does not mean "use a native field for a claim you should not
be making".


## Analyze session 2026-11-05

Six findings, no CRITICAL, no constitution violations. All six remediated; all
16 checklist items still pass. 14 FRs' worth of coverage is unchanged at
26/26 requirements with at least one task, now with none partial.

| ID | Severity | What it was | How it was closed |
|---|---|---|---|
| F1 | **HIGH** | FR-012 enumerated **two** stdenv-injected attributes; the pinned x86_64-linux closure has **three** populated-or-declared. `extraNativeBuildInputs` (3 declarations on linux) was in neither the consult list nor the deliberately-unclassified list — a scope hole on the exact closure every criterion is stated against. Found by re-running the census on linux instead of trusting the darwin table the spec was written from. | FR-012 names all three, with the reason recorded. Knock-ons in `data-model.md`, `contracts/consulted-set.md` P6, the quickstart trap, and tasks T002/T010/T026. |
| F2 | MEDIUM | `data-model.md`'s "count (linux)" column was darwin-derived. Declared-empty was 5 not 4, and the surviving pair named `defaultBuildInputs` — **zero paths on linux** — while omitting `extraNativeBuildInputs`, which has 3. The count happened to be right; the membership did not. | Table re-derived from the linux closure and split into before/after columns: 4→8 consulted-populated, 2 never-declared, 5 declared-empty, 6 (230 decls) → 2 (211 decls) unconsulted-populated, with the two survivors named. |
| F3 | MEDIUM | SC-010's population is **five** members and only `numactl` was asserted. Four already carry no scope, so a criterion checking only the changing one would pass unchanged if the triples were later reclassified as tooling and all five gained an exclusion. | SC-010 names all five; T021 asserts all five. |
| F4 | MEDIUM | The marker's gating row "any other role + an unclassified reference → marker present" had no task. Every marker test used a member whose *only* reference is unclassified, so a gating rule that emitted only for `unreferenced` members would have passed. | New **T044**, appended rather than inserted so no existing ID moved. |
| F5 | LOW | SC-004's second clause — the difference accounts for every member whose role changes — had no task. T033 recorded the census, T037 scored the prediction, neither did the attribution. | SC-004 states the attribution explicitly; T037 carries it. |
| F6 | LOW | T041's compare-mode confirmation mapped to no requirement. | FR-008 extended to cover verification, with the reason: a regen run cannot verify its own output, and a local corpus run reports `ok` in 0.00s having skipped. |

**Three claims were verified rather than assumed, and all three held.** Of the
five triple-reachable members only `numactl` carries `scope: "excluded"`, so
SC-013's named-not-counted figure is right. SC-001's "230 on x86_64-linux" is
exact. And SC-012's census, including `artifact-input` *falling* by two,
matches research R1.

**F1 is the one worth remembering.** It is the same failure as the census
mislabelling retracted in milestone 1278 — a figure taken from one system and
used as though it described another — but one level up: not a wrong *number*,
a wrong *membership*, which no amount of checking the arithmetic would have
caught. The two closures do not merely differ in magnitude; they populate
different attributes. Enumerating a closed set from a single capture is the
error, and running the census on both captures is the fix.
