# Findings — 1279 (#1346)

What this milestone corrected about its **own** reasoning. Kept because the
same mistakes keep arriving in slightly different clothes, and because a
milestone that records only its conclusions teaches nothing about how it
reached them.

## F1 — The two closures populate different attributes, not different amounts

Found in `/speckit-analyze`, HIGH. FR-012 named `defaultNativeBuildInputs` and
`defaultBuildInputs` as the deliberately-unclassified pair. Measured on the
closure the goldens pin, `defaultBuildInputs` holds **zero** store paths and
`extraNativeBuildInputs` — named in neither the consulted set nor FR-012 —
holds three. On darwin it is the reverse.

This is milestone 1348's error one level up. That milestone quoted a census
without its system; this one enumerated a *membership* from the wrong system,
which a count-based review would never have caught. The fix names all three
attributes, and `classify.rs` says why in the constant's own doc comment.

**The general form**: when two instances of a measurement disagree, check
whether they disagree about the *set* before assuming they disagree about the
number.

## F2 — The marker count was a different question's right answer

The contract was drafted saying the marker appears on **3** components. It
appears on **9**. Three is the count of `unreferenced` members named *only* by
an unclassified attribute — which `role-delta-probe.py` prints under that
exact heading, and which is a correct answer to a question the contract was
not asking.

The real chain, measured: 211 declarations → 16 closure members → 13 emitted
as components → 9 identities after build-variant merging. Each step drops
something for its own reason, and none is negligible.

**The general form**: a figure lifted from a probe's output carries the
probe's question with it. Re-derive it for the question actually being asked,
or state which question it answers.

## F3 — A matching total can carry a wrong cause

The first draft of the role-change table in `README.md` attributed `patchelf`'s
`unreferenced` → `build-tooling` move to `propagatedNativeBuildInputs`. It is
`depsBuildTarget`. The total was right — seven derivations, three to tooling —
and SC-004's accounting clause would have read as satisfied.

Both new tooling-side attributes produce the same role, so the role cannot
tell you which one did it. The attribution has to be measured per member, and
now is.

## F4 — An `unreferenced` member is not always emitted, which caps the fix

Discovered during implementation, not planning. `emits_as_component` drops an
`unreferenced` member that applies no patch (C182, m1035 FR-004a), so the
marker — which can only ride on a component — cannot reach the very case
#1346 describes unless the member happens to apply a patch or happens to have
a sibling variant that does.

Measured consequence: of the 16 named members, three are not emitted; two have
sibling variants that are, so their identity carries the marker anyway. One
does not — **`xz 5.8.3`** — and that member is the residue of #1346's ask this
milestone does not close. It is not a mislabelled component, since the
document makes no claim about it at all; closing it would mean changing
milestone 1035's emission rule, which is a separate decision with its own
blast radius.

Recorded rather than quietly scoped out, because "the fix is complete" and
"the fix reaches everything it can reach" are different claims.

## F5 — `STDENV_INJECTED_FIELDS` is test-only on purpose

The constant recording FR-012's exclusion is `#[cfg(test)]`. That began as a
dead-code warning and turned out to be the right shape: if production code
consulted the list, the exclusion would be a *behaviour* that could be wrong,
and SC-011 — a stdenv-named member has the same role with and without the
marker — is only trivially true while nothing branches on it. The exclusion is
an absence from the two classifier constants; the constant exists so that
absence is asserted rather than assumed.

## F6 — T016 landed before its tests, and the mutation is what repairs that

The classifier constants were widened before T009–T015 were written, so the
US1 tests were never observed failing first. The evidence that stands in for
it is T031, a mutation: with `propagatedNativeBuildInputs` unreadable, the
members it credits stop being emitted at all. Recorded rather than glossed —
a mutation proves a test *can* fail, which is the property the missing red
run would have established.

## Verified rather than changed

- **T020**: `resolve/closure_role_scope.rs` needs no edit.
  `"artifact-input" | "both" => None` already asserts no scope, which is why
  classifying the target triples artifact-side is what removes `numactl`'s
  false `scope: "excluded"` without touching m1277's rule. Had this task
  edited that file, the plan's confinement argument would have broken and
  SC-008 would have needed re-checking against every corpus target.
- **T024/T028**: transitivity needs no extra pass, and the reason is more
  specific than "the walk follows it". `classify` is not a walk from a root —
  it is a flat scan over every derivation in the closure, crediting whatever
  each one names, so a member two hops of propagation away is credited by the
  second hop's own declaration at whatever depth it sits. A root-anchored walk
  would have needed a propagation phase. The first draft of that comment said
  "the reachability walk below already follows whatever these fields name",
  which is true of the outcome and wrong about the mechanism.

## F7 — SC-013 could not distinguish a new claim from a new component

Sharpened during implementation. SC-013 said `numactl` loses its exclusion
"and no component gains an exclusion". Measured, two components the milestone
**newly emits** arrive as `build-tooling` and therefore arrive with
`scope: "excluded"`: `patchelf 0.15.2` and `setup-debug-info-dirs-hook
26.05pre-git`.

That is not an existing component gaining a claim — it is a component
appearing, having previously been absent precisely because an unread attribute
made it look like nothing depended on it. Only the first would be a
regression, and the criterion as drafted would have failed on the second. It
now says "no component already in the golden gains an exclusion".

Noted because the failure mode is subtle: the criterion would have gone red
for the milestone working correctly, and the cheapest way out of a red
criterion is to loosen it without understanding which of the two cases fired.

## F8 — SC-011 named two of the three stdenv attributes

The same error as F1, in a success criterion rather than a functional
requirement, and surviving F1's remediation because that pass fixed FR-012 and
the consulted set and did not re-read the criteria. Fixed to name all three.

**The general form**: a remediation scoped to "where the defect was found"
leaves the defect wherever else the same list was written out. Grep for the
list, not for the section.

## F9 — Consulting an attribute invalidated another milestone's test premise

The first full gate run failed with **eight** failures, six of them in
`waybill-cli/tests/closure_structured_attrs.rs`, milestone 1278's suite. Its
fixture declared a dependency in `depsBuildTarget` as *the attribute
classification does not read*, and used that to assert the C204 disclosure
names an unread field without crediting it. This milestone consults
`depsBuildTarget`, so the fixture's closure became fully consulted, the
disclosure correctly disappeared, and six assertions about it correctly failed.

The failures were the right answer, not a regression — but they are easy to
mistake for one, because the failing file belongs to a different milestone and
its diff is empty.

The fix re-points that fixture at `defaultNativeBuildInputs`, chosen for a
reason rather than as the next available name: FR-012 excludes stdenv's
injected attributes deliberately, so it is unlikely to be consulted by a later
milestone for the same reason it is not consulted now. The comment says so, so
the next person to widen the set knows whether this fixture is in their way.

**The general form**: a test whose premise is "X is outside the set" has a
dependency on that set which no import or type signature records. Widening a
set, grep for its complement.

## F10 — The two remaining failures were a fixture I had edited underneath them

The other two: my own T008 unit tests, written against the first draft of the
fixtures and then not re-run after the fixtures were rewritten to give the
stdenv-named members a patch (so they emit at all) and to add the T044 row.
Both asserted the old shape.

Unremarkable except for how it was found — only by the full gate, because the
earlier run of those two tests predated the fixture edit by twenty minutes.
A per-file run after each edit is not the same as a run of everything after
all of them.

## F11 — A backgrounded gate produced nothing and looked like a gate still running

The second gate invocation was launched in the background as part of a
compound command — a heredoc append, a `rm`, and then `./scripts/gate.sh`.
Nothing ran. The task's output file was created, stayed zero bytes, no
`target/gate/*.log` appeared, no process was alive, and the heredoc's append
to this file had not landed either. For several minutes it was indistinguishable
from a long compile.

Relevant because it is the same failure class this project already records
twice over: **a check that did not run looks identical to a check that
passed**, and here it also looked identical to a check still running. The
tell was not the task status but the artifacts — no log file, no process, and
the *first* half of the compound command visibly not applied.

So: launch a long gate as a single command with no compound prefix, and
confirm it has started by the existence of its log rather than by the absence
of a failure.
