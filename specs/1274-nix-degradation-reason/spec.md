# Feature Specification: the nix-closure tier says why it degraded

**Feature Branch**: `1274-nix-degradation-reason`
**Created**: 2026-11-03
**Status**: Draft
**Input**: User description: "The `--nix-closure` tier does not say why it
degraded: the closure path keeps a blind 200-char stderr prefix instead of the
salient error line, and the import-from-derivation refusal has no distinct
reason code, so a deliberate policy refusal reads as a broken flake
(#1283 part 1)"

## Context

`--nix-closure` refuses import-from-derivation on purpose (#1034, #1114):
evaluating a flake that needs it means *building* something during
evaluation, which is a larger grant than evaluating. The refusal is a
defence, not a defect.

But `callCabal2nix` is the most common way of building a Haskell project with
Nix, and it requires import-from-derivation. So the tier degrades on those
projects by design — and the document does not say that is what happened. An
operator sees a generic failure and reasonably concludes waybill is broken on
their repository.

This is milestone 1273's defect class in a part of the codebase 1273 did not
touch: **the pass does not tell the truth about what it did.**

### Re-measured, because the issue is from 0.13.1

#1283 was measured at `96e7a545`. Part of it has since been fixed, and the
spec says only what is still true. `measurements/reproduce-1283.sh` is
committed; it builds two otherwise-identical flakes for one `.cabal` and
scans both.

| | handwritten flake (control) | `callCabal2nix` flake |
|---|---|---|
| nix itself | evaluates | `cannot build '…cabal2nix-….drv' during evaluation because the option 'allow-import-from-derivation' is disabled` |
| waybill log | `classified … derivations=1170` | `degrading … reason="evaluation-failed"` |
| waybill log `detail=` | — | `` `nix eval` failed: warning: Git tree '…' has uncommitted changes `` |
| document `waybill:nix-closure-degraded` | **absent** | `evaluation-failed` |
| components | 304 | 2 |

Two things are wrong, and they are separable:

1. **The detail names the wrong line.** It reports a *git warning*. The line
   that explains the failure — nix's own sentence about
   `allow-import-from-derivation` — never reaches the operator.
2. **The reason is the catch-all.** `evaluation-failed` covers a refused
   policy, a syntax error in the flake, and a type error in nixpkgs alike.

### The fix for (1) already exists, 100 lines away

`eval/invoke.rs::salient_stderr` starts at the first line mentioning an error
rather than keeping a blind prefix, and its doc comment names this exact
scenario:

> A blind prefix is the wrong thing to keep. Nix prints warnings before
> errors, so `stderr.take(200)` … returns "ignoring untrusted flake
> configuration setting …" and truncates before the line that says what
> actually went wrong. Measured on a Haskell flake: the operator was shown a
> warning about untrusted config while the real cause — `cannot build
> 'cabal2nix-….drv' during evaluation because the option
> 'allow-import-from-derivation' is disabled` — never appeared. That reads as
> "waybill is broken on my repo" rather than "waybill declined by policy".

The **closure** path does not use it. It keeps
`out.stderr.trim().chars().take(200)`. So this is drift between two copies of
one idea, in the same module tree — and the function two definitions below
the drift carries the comment *"two copies would drift, and the drift would
be exactly the thing the guard prevents"*, about the argv guard. The error
reporting drifted instead.

### What this feature is NOT

#1283's larger question — **whether to support these flakes at all** — is out
of scope. Its options (keep refusing; an opt-in that permits
import-from-derivation; recognise `callCabal2nix` and substitute a generated
expression) depend on #971 (whether nix evaluation becomes primary) and #1034
(how deep a closure is followed). #1282 depends on the same decisions.
Answering "why did it degrade" does not commit the project to any of them,
and is worth having under every one.

## Clarifications

### Session 2026-11-03

- Q: Where does the explanatory *detail* live — the document, the log, or
  both?
  → A: **Both, carried inside the existing annotation.**
  `waybill:nix-closure-degraded` becomes a small JSON object with `reason`
  and `detail`, the shape C173 and C175 already use for the nixpkgs records.
  A log reaches only the operator who ran the scan, and this feature exists
  because the *artifact* cannot say why the tier declined — putting the fix
  in the log alone would leave that untouched (milestone 1273's FR-018). It
  also keeps FR-014 out of play: extending an annotation that already has
  three extractors adds no catalog row. The cost taken deliberately is that
  the annotation's shape changes from a bare string to an object, so every
  committed golden carrying it churns and consumers keying on the string must
  be checked.

- Q: What must the classifier anchor on, given that a flake requesting the
  setting puts its name in stderr even when the failure is unrelated?
  → A: **nix's consequence clause, and only on a line identified as an
  error.** Both conditions, because either alone is insufficient. Measured
  during this session: a flake whose `nixConfig` requests
  `allow-import-from-derivation` and then fails on a missing attribute emits
  `warning: ignoring untrusted flake configuration setting
  'allow-import-from-derivation'.` followed by `error: attribute '…'
  missing` — so matching the bare setting name anywhere in stderr
  mis-classifies that as a policy refusal. Real flakes do this; `slack-web`
  sets the option. A reworded nix message falls through to the generic reason
  under FR-006 rather than being guessed at.

- Q: When stderr carries both an import-from-derivation refusal and an
  unrelated error, which reason wins?
  → A: **The refusal.** It is a fact about waybill's own policy and is
  actionable — the operator learns what they would have to accept — whereas a
  co-occurring flake error is frequently a *consequence* of the refusal
  rather than an independent failure. The cost accepted is that a genuine
  flake bug can sit behind a policy note, which FR-005c mitigates by keeping
  both lines in the detail. Rejected: classifying on whichever error nix
  printed first, because that makes the recorded reason depend on nix's
  output ordering, which is not a stability contract and could differ between
  versions for the same two failures.

- Q: Should the new reason be specific to import-from-derivation, or a
  general "the tier declined by policy" class?
  → A: **Specific.** The operator's next action differs per cause, which is
  the whole reason `DegradationReason` is a closed set rather than one
  "degraded" string. "The tier refused import-from-derivation" tells them
  what they would have to accept; a general "policy" reason does not. The
  existing vocabulary already draws this distinction — `offline-requested`,
  `tool-absent` and `ifd-refusal-unverified` are three separate variants
  rather than one.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — a refusal says it was a refusal (Priority: P1)

An operator runs `--nix-closure` on a Haskell project built with
`callCabal2nix`. The tier declines to evaluate, because doing so would build
a derivation during evaluation. Today the document records
`evaluation-failed` and the log's detail shows a git warning, so the operator
cannot tell a deliberate refusal from a broken flake, and has no way to learn
what the tier objected to.

**Why this priority**: it is the whole feature. Everything else here is a
consequence of getting this one signal right.

**Independent Test**: scan a `callCabal2nix` flake and assert the emitted
document names import-from-derivation as the cause, without reading the log.

**Acceptance Scenarios**:

1. **Given** a flake whose default package needs import-from-derivation,
   **When** it is scanned with `--nix-closure`, **Then** the document records
   a reason distinct from the generic evaluation failure.
2. **Given** that same scan, **When** the operator reads the document alone,
   **Then** they can tell the tier declined deliberately rather than failed.
3. **Given** a flake that evaluates without import-from-derivation, **When**
   it is scanned, **Then** no degradation is recorded at all and the output is
   unchanged.
4. **Given** a flake that fails evaluation for an unrelated reason — a syntax
   error, a missing attribute — **When** it is scanned, **Then** the reason is
   NOT the import-from-derivation one.

---

### User Story 2 — the detail names the line that explains the failure (Priority: P2)

An operator reads the degradation detail to find out what went wrong, and
gets whichever line nix printed first — in the measured case a git warning
about uncommitted changes, which has nothing to do with the failure.

**Why this priority**: it is narrower than US1 and partly subsumed by it —
once the reason is specific, the detail matters less for *this* cause. It
matters for every other cause, which is why it is not dropped: the same blind
prefix mis-reports a syntax error or a nixpkgs type error just as badly.

**Independent Test**: induce an evaluation failure whose explanatory line is
not the first line of stderr, and assert the recorded detail contains the
explanatory line.

**Acceptance Scenarios**:

1. **Given** a flake whose evaluation fails after nix has printed at least
   one warning, **When** it is scanned, **Then** the recorded detail contains
   the line that explains the failure.
2. **Given** stderr with no line identifiable as an error, **When** it is
   recorded, **Then** something is still recorded rather than nothing — an
   unparseable failure is not an excuse for silence.

### Edge Cases

- A flake that requests `allow-import-from-derivation` through its own
  `nixConfig`. waybill does not pass `--accept-flake-config`, so the request
  is ignored and the refusal still applies — the reason must be the refusal,
  not something about the flake's config. **And the converse, measured:**
  such a flake failing for an unrelated cause emits a *warning* naming the
  setting, which must NOT be classified as a refusal (FR-005a).
- A flake needing import-from-derivation for an attribute **other** than the
  one being evaluated, so evaluation succeeds. No degradation, no reason.
- nix wording changes between versions. The classifier keys on text, so a
  reworded message must degrade to the existing generic reason rather than
  being mis-classified or crashing.
- Both failures at once: an unrelated error *and* an
  import-from-derivation refusal in one stderr. The refusal wins (FR-005c),
  and the detail keeps both lines.
- A `nix` old enough not to recognise the setting at all — already covered by
  the existing `ifd-refusal-unverified` reason, which must keep its meaning
  and not be displaced by the new one.

## Requirements *(mandatory)*

### Functional Requirements

#### The reason (US1)

- **FR-001**: A `--nix-closure` run that declines because evaluation would
  require import-from-derivation MUST record a reason distinct from the
  generic evaluation failure.
- **FR-002**: That reason MUST be present in the **emitted document**, not
  only in the scan log. A log reaches only the operator who ran the scan; the
  document is what is read later. (The same requirement milestone 1273's
  FR-018 states for its own signals.)
- **FR-002a**: The explanatory detail MUST be in the document too, carried
  **inside the existing `waybill:nix-closure-degraded` annotation** as a
  structured value with a `reason` and a `detail` — the shape C173 and C175
  already use. A reason alone answers "was this policy?"; the detail answers
  "what did it object to?", and an operator reading the artifact months later
  has no log to fall back on.

  The annotation's shape therefore changes from a bare string to an object.
  That is a wire change: every committed golden carrying it churns, and any
  consumer keying on the bare string breaks. Both are consequences this
  feature accepts rather than discovers, and FR-016 covers the goldens.
- **FR-002b**: Because the carrier already exists in all three formats with
  three parity extractors, this feature MUST NOT add a catalog row for the
  detail. Extending a row is not adding one; FR-014's condition applies only
  if a row is nonetheless introduced.
- **FR-003**: The reason MUST also be logged, so an operator can act before
  the SBOM is published rather than only discover it afterwards.
- **FR-004**: The reason MUST reuse the existing degradation-reason carrier
  rather than introducing a parallel channel. `waybill:nix-closure-degraded`
  already exists and consumers key on it.
- **FR-005**: An evaluation failure that is **not** an import-from-derivation
  refusal MUST keep the generic reason. A classifier that over-matches turns
  one uninformative answer into one misleading answer, which is worse.
- **FR-005a**: The classifier MUST require **both** of: nix's consequence
  clause naming the disabled option, **and** that the clause appear on a line
  identified as an error rather than a warning. Each condition alone is
  insufficient.

  Measured, not reasoned: a flake whose `nixConfig` requests
  `allow-import-from-derivation` and then fails for an unrelated cause emits

  ```
  warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.
  error: attribute 'this-attribute-does-not-exist' missing
  ```

  so a classifier keyed on the bare setting name reports a missing attribute
  as a policy refusal. Real flakes carry such settings — `slack-web` sets
  this one — so the false positive is not hypothetical. The probe under
  `measurements/` covers this case as a negative test.
- **FR-005c**: Where stderr carries an import-from-derivation refusal **and**
  another error, the refusal MUST be the recorded reason, and the detail MUST
  retain the other error's line as well. Precedence is fixed by this
  requirement rather than by whichever branch of the classifier happens to
  run first, and it does NOT depend on the order nix printed the lines in —
  that order is not a stability contract.

  Keeping both lines is what stops this masking a genuine flake bug: the
  reason names the policy, and the detail still shows everything that failed.
- **FR-005b**: The matched text is an external behaviour of nix, so a change
  in its wording MUST degrade to the generic reason (FR-006) rather than
  being mis-classified, and MUST NOT panic or fail the scan. The probe is
  committed so the match can be re-verified when nix changes.
- **FR-006**: Where the cause cannot be determined from nix's output, the
  generic reason MUST be used. Degrading to the catch-all is correct
  behaviour, not a failure of this feature.

#### The detail (US2)

- **FR-007**: A recorded degradation detail MUST contain the line that
  explains the failure where nix emits one, rather than whichever line was
  printed first.
- **FR-008**: The closure path and the version-resolution path MUST select
  that line by the same rule. The defect this feature fixes is that they do
  not, so two implementations of the rule would reintroduce it.
- **FR-009**: A detail MUST be recorded even when no line can be identified
  as the explanation.

#### Cross-cutting

- **FR-010**: This feature MUST NOT change whether any flake evaluates. It
  changes only what is *recorded* about a refusal that already happens. A
  flake that degrades today degrades after this change, and one that
  evaluates today still evaluates.
- **FR-011**: This feature MUST NOT add a CLI flag, an external tool
  dependency, or a subprocess invocation, and MUST NOT make the refusal
  itself configurable. Whether to offer an opt-in is #1283's second question
  and depends on #971 and #1034.
- **FR-012**: No behaviour may vary with what is on the host's `PATH` beyond
  the existing `nix` dependency the tier already declares.
- **FR-013**: Every other degradation reason MUST keep its current wire
  string and meaning. `ifd-refusal-unverified` in particular is a different
  condition — the refusal could not be *confirmed* — and must not be
  conflated with the refusal taking effect.
- **FR-014**: A new wire value is observable by a second component, so it
  MUST be documented wherever the existing reasons are, and any new catalog
  row MUST ship with its parity extractor in the same change.
- **FR-015**: Every requirement above MUST be covered by a test that fails
  when the behaviour is removed, and the classifier MUST have a negative test
  — an evaluation failure that is not a refusal — because an over-matching
  classifier is the likely way to get this wrong.
- **FR-016**: Any committed golden or fixture asserting the annotation's old
  bare-string shape MUST be corrected rather than pinned, and the correction
  MUST be justified against a measurement rather than against this spec. A
  pre-change baseline MUST therefore be captured before the first reader or
  emitter change.

### Key Entities

- **Degradation reason**: the closed set of causes the nix tiers record for
  declining. A closed set because each member calls for different operator
  action.
- **Import-from-derivation**: building a derivation during evaluation. nix
  refuses it when `allow-import-from-derivation` is disabled, which is how
  waybill invokes it.
- **Explanatory line** (the code calls it the *salient* line, and the
  function is `salient_stderr`): the line of nix's stderr that explains a
  failure, as
  distinct from the first line, which is often a warning.
- **The closure path / the resolution path**: the two nix invocations that
  can fail. Today they select a detail by different rules.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning a `callCabal2nix` flake with `--nix-closure` yields a
  document naming import-from-derivation as the cause. Measured today:
  `evaluation-failed`, with no mention of it anywhere in the document.
- **SC-002**: The **document** for that scan contains nix's explanatory line,
  readable without access to the scan log. Measured today: the document
  carries no detail at all, and the log's detail contains
  `warning: Git tree '…' has uncommitted changes` while the explanatory line
  appears nowhere.
- **SC-003**: A flake that evaluates today still evaluates, with
  byte-identical output. Measured today on the control: 1,170 derivations
  classified, 304 components, no degradation recorded.
- **SC-004**: An evaluation failure that is not a refusal keeps the generic
  reason — including the measured false-positive case, where a flake requests
  the setting through its own `nixConfig` and then fails on a missing
  attribute.
- **SC-005**: The closure path and the resolution path produce the same detail
  for the same stderr.
- **SC-006**: Every requirement is covered by a test that fails when the
  behaviour is removed — mutation-tested, not assumed. On this feature's own
  evidence, the salient-line fix was written, documented, and then not applied
  at the second site, with every test still passing.
- **SC-007**: The output of every other ecosystem, and of every nix tier
  other than the degradation record, is byte-identical.
- **SC-008**: Every probe committed under `measurements/` re-runs and
  reproduces its recorded finding.

## Assumptions

- **nix's wording is an external behaviour and was measured, not read.** The
  sentence `cannot build '…' during evaluation because the option
  'allow-import-from-derivation' is disabled` was observed directly from nix
  3.20.0 (Determinate) on `aarch64-darwin` against nixpkgs `a799d3e3`, and is
  recorded in `measurements/reproduce-1283.sh`. A classifier keyed on text is
  only as good as the text, so the probe is committed and re-runnable when
  nix changes.
- **The refusal stays.** This feature does not relitigate #1034 or #1114. It
  assumes the defence is correct and makes it legible.
- **One reason wins per degradation.** The carrier records a single reason
  today, and this feature does not change that shape. Which reason wins when
  two causes are present is settled by **FR-005c** — the refusal — rather
  than by whichever branch of the classifier runs first.

  An earlier draft of this paragraph cited "FR-005's ordering", and FR-005
  contained no ordering: a cross-reference to a rule that did not exist. It
  is recorded here because the gap was invisible from the requirement's own
  text and only surfaced when the clarification pass went looking for the
  rule it pointed at.
- **`salient_stderr`'s rule is the right rule.** It was measured and
  documented in milestone 1034 and is not re-derived here; this feature
  applies it where it is missing. If the rule is wrong, it is wrong in both
  places and that is a separate finding.

## Dependencies

- The nix evaluation tier and its degradation-reason vocabulary (milestone
  1034).
- The runtime-closure tier that carries the un-applied fix (milestones 1035,
  1066).
- `salient_stderr` and the measurement behind it (milestone 1034).
- The document-scope `waybill:nix-closure-degraded` annotation.

## Out of Scope

- **#1283's second question** — whether, and how, to support flakes needing
  import-from-derivation. Depends on #971 and #1034.
- **#1282** — `--nix-closure` appending Hackage packages as `pkg:generic`
  with no edges, license or CPE. Same dependency.
- **#1340** — whether a GHC boot library should be classified optional.
  Unrelated to degradation reporting.
