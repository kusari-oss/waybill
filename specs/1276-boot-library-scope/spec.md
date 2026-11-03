# Feature Specification: a package the compiler ships is never scoped as absent from the artifact

**Feature Branch**: `1276-boot-library-scope`
**Created**: 2026-11-03
**Status**: Draft
**Input**: #1340 — "A GHC boot library can be classified `optional`, though the compiler ships it in every build"

## Context

A GHC boot library can be emitted with a non-runtime lifecycle scope because
its only path through the dependency graph runs via a non-runtime component.
Measured on `slack-web`, offline against the nixpkgs revision its own
`flake.lock` pins, `pretty` is scoped `excluded` — although GHC links it into
every build of that project whatever the flags say.

#1340 frames this as needing "a decision rather than a patch", on the grounds
that both readings have a case: the scope is a true statement about the
*dependency relationship*, and a false one about whether the *artifact
contains* the package.

**Read against the CycloneDX 1.6 schema, it is not a tie.** The schema's own
`meta:enum` text, in the vendored copy at
`waybill-cli/tests/fixtures/schemas/cyclonedx-1.6.json`:

| value | the schema's definition |
|---|---|
| `required` | "The component is required for runtime" |
| `optional` | "not capable of being called due to them **not being installed or otherwise accessible by any means**. Components that are installed but due to configuration or other restrictions are prohibited from being called **must be scoped as `required`**." |
| `excluded` | "Components that are excluded provide the ability to document component usage for test and other non-runtime purposes. **Excluded components are not reachable within a call graph at runtime.**" |

A boot library is installed — the compiler ships it — and is reachable in the
artifact's call graph. So `optional` is wrong by the first definition, which
explicitly instructs that an installed-but-unreachable component "must be
scoped as `required`", and `excluded` is wrong by the second. The
relationship reading is not merely the less useful of two valid readings; it
is the one CycloneDX's own text rules out.

That changes what this feature is. It is not a semantics choice between two
defensible positions — it is a **conformance defect** against a specification
Constitution Principle V requires waybill to conform to.

## The population, measured

Across the 17 committed corpus goldens:

| class | components | carrying a non-runtime scope |
|---|---|---|
| GHC boot libraries | 78 | **10** |
| OS packages (`apk`/`deb`/`rpm`/`opkg`) | 142 | **0** — none carries any scope at all |
| file-tier components | 175 | **0** — none carries any scope at all |

The 10 are in two targets: `haskell-security-advisories` (8) and
`nix-closure-moat` (2). On `slack-web` the figure is 1 (`pretty`).

**This answers #1340's open question** — whether exempting boot libraries
implies the same for "other always-present classes, OS packages in a rootfs
scan or language stdlibs collapsed by the file tier". Those classes do not
carry a lifecycle scope at all: 142 and 175 components, every one of them
scope-free. They are not participants in the question, so a boot-library rule
takes no position on them. That is a measurement, not an assumption — and it
is reported with its positive control, because a bare zero cannot distinguish
"never mis-scoped" from "never examined".

## Clarifications

### Session 2026-11-03

- Q: When several GHC series are candidates, which series' configuration determines the exempt set? → A: Narrow to the series the flake names when it names one; union across candidates only as the fallback (Option B).
- Q: Does the implementation build for a direct mark on a boot library, and how? → A: Fix it in the propagation's immune set, and add a document-level invariant so an unanticipated origin fails visibly rather than being silently corrected (Option B).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - what the compiler ships is not reported as absent (Priority: P1)

An operator filters a document by scope to answer "what is actually in this
artifact", which is what the CycloneDX definitions above describe. Today that
filter silently drops `pretty` from `slack-web`, and 10 components across the
corpus — packages that are present in every build. After this story, a
package the pinned revision identifies as compiler-supplied is never reported
as absent from the artifact.

**Why this priority**: it is the feature, and it is a conformance defect
rather than an improvement. A consumer acting on `excluded` is acting on a
claim the schema defines as false for these components.

**Independent Test**: scan `slack-web` offline against its pinned revision and
assert `pretty` carries no non-runtime scope; assert the same across every
corpus target for all 78 boot-library components.

**Acceptance Scenarios**:

1. **Given** `pretty`, a package nixpkgs binds to `null` in the revision's
   compiler configuration, reachable in the graph only through a
   `buildable: False` stanza, **When** the document is emitted, **Then**
   `pretty` carries no non-runtime scope.
2. **Given** the same project, **When** the document is emitted, **Then** the
   8 non-boot components below the same disabled stanza **keep** their
   non-runtime scope — the correction is confined to packages the compiler
   supplies.
3. **Given** a document where a boot library is reachable by a runtime path
   as well, **When** it is emitted, **Then** its scope is unchanged, because
   the defect was never about that component.

---

### User Story 2 - the exemption is auditable and derived, not asserted (Priority: P2)

An operator who notices that one component is immune to a rule the others
obey needs to see why, and a maintainer needs to know the immunity is not a
hand-maintained list that will drift. After this story, each exempted
component records that the compiler supplies it, and the set of such packages
continues to come from the pinned revision's own configuration.

**Why this priority**: an unexplained exemption is indistinguishable from a
bug, and a list of package names maintained by hand is the thing that goes
stale. The information already exists per revision; this story is about not
losing it at emission.

**Independent Test**: assert every component the rule exempts carries the
recorded reason, and that the exempted set is empty when the revision's
compiler configuration is unavailable.

**Acceptance Scenarios**:

1. **Given** an exempted component, **When** the document is emitted, **Then**
   it records that its runtime presence is owed to the compiler rather than to
   a path through the graph.
2. **Given** a scan whose pinned compiler configuration cannot be read,
   **When** the document is emitted, **Then** no component is exempted and
   the scan still succeeds — absence of evidence is not evidence of immunity.

---

### User Story 3 - the correction is visible rather than silent (Priority: P3)

An operator comparing two documents of the same project across waybill
versions sees components change scope and needs to know a rule changed rather
than the project. After this story the document records how many components
the rule exempted.

**Why this priority**: narrower than US1 and US2, and it is transparency
rather than correctness. It matters because this feature changes a value
consumers filter on, and a changed filter result with no recorded cause is
the shape that gets misread as a regression in the project.

**Acceptance Scenarios**:

1. **Given** a scan that exempts components, **When** the document is emitted,
   **Then** it records the count at document scope.
2. **Given** a scan that exempts none, **When** the document is emitted,
   **Then** it records zero rather than omitting the record — "the rule
   exempted nothing" and "the rule did not run" are different facts.

---

### Edge Cases

- **A boot library that is directly marked rather than reached by
  propagation.** Measured population: **0** — all 10 affected components
  across the corpus carry
  `waybill:lifecycle-scope-derivation = transitive`. The schema's reasoning
  does not depend on how the scope arose, so such a mark would still be wrong;
  FR-002a makes it visible as a failing invariant rather than building a
  suppression path for a population that does not exist.
- **A project whose flake names no compiler series.** The exempt set falls
  back to the union across candidates, which over-exempts by up to 15 names
  (FR-004a). That is a bounded, measured cost rather than an unknown, and it
  is the same posture m926 already takes for version resolution.
- **A boot library that is also genuinely runtime-reachable.** Its scope is
  already correct and must not change; this is the control that keeps the rule
  from being "set every boot library to runtime".
- **The compiler configuration is unavailable** — offline, cache cold, an
  unsupported lock shape. The boot set is then empty, nothing is exempted, and
  no component is silently treated as immune (US2 scenario 2).
- **A project with no boot libraries at all.** The rule exempts nothing and
  the document says so rather than omitting the record (US3 scenario 2).
- **A component that is compiler-supplied in one revision and not in
  another.** The set is per revision by construction, so the same package may
  be exempt in one document and not in another. That is correct and is why
  FR-004 forbids a maintained list.
- **Milestone 1275's closure-derived edges reach boot libraries.** Measured in
  that milestone's findings F12: a non-runtime closure member now has declared
  out-edges, and its scope propagates along them to a boot library. That
  widened this defect's population, and `t057` in
  `waybill-cli/tests/nix_closure_hackage_identity.rs` pins the behaviour and
  names this issue. **That test asserts the defect**, so this feature must
  update it to assert the correction (FR-010).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A component the pinned revision identifies as compiler-supplied
  MUST NOT carry a non-runtime lifecycle scope.
- **FR-002**: The rule MUST be enforced in the transitive propagation, by
  treating a compiler-supplied package as immune to it. Immunity is the right
  shape rather than a repair: the propagation already maintains an immune set
  for runtime-reachable components, and a package the compiler ships is
  runtime-present for the same reason — so the propagation should never reach
  it, rather than reaching it and being corrected afterwards.
- **FR-002a**: A compiler-supplied component carrying a non-runtime scope from
  **any** origin MUST be detectable as a defect, not silently corrected. A
  document-level invariant MUST assert that no compiler-supplied component
  carries a non-runtime scope, so a direct mark — or a future producer this
  specification did not anticipate — surfaces as a failing invariant.
  Measured: all **10** affected components derive their scope transitively
  (`waybill:lifecycle-scope-derivation = transitive`), so the direct-mark
  population is **0** today. Building a second suppression path for it would
  be speculative; leaving it undetectable would let the defect return
  silently, which is the asymmetry this requirement resolves.
- **FR-003**: A component that is **not** compiler-supplied MUST keep the
  scope it has today. The correction is confined to the exempt class; the 8
  non-boot components below `slack-web`'s disabled stanza are the measured
  control.
- **FR-004**: The exempt set MUST be derived from the pinned revision's own
  compiler configuration, never from a package-name list maintained in
  waybill. A package the compiler supplies at one revision and not another
  MUST be exempt only where the revision says so.
- **FR-004a**: When the project's flake names a compiler series, the exempt
  set MUST be that series' configuration alone. The union across candidate
  series is permitted **only** as the fallback for a project that names none.
  Measured at revision `a799d3e3`: the union across 8 series holds 50 names,
  32 of which every series nulls, so the union exempts up to **15** names a
  given series does not — packages that compiler does not ship. Over-exempting
  is the same defect as the one being fixed, pointing the other way, and
  FR-011 names it the plausible wrong implementation. The narrower path is
  also the common one: `slack-web`'s per-revision cache holds only
  `configuration-ghc-9.10.x.nix`, which means its flake named a series and no
  union was ever formed.
- **FR-005**: When the compiler configuration cannot be read, the exempt set
  MUST be empty, the scan MUST still succeed, and no component may be treated
  as immune by default.
- **FR-006**: Each exempted component MUST record that its runtime presence is
  owed to the compiler, distinguishably from a component that is runtime
  because a runtime path reaches it.
- **FR-006a**: **Principle V audit for FR-006's carrier.** If FR-006 is
  satisfied by a new `waybill:*` property, this specification MUST cite the
  audit of each target format for an existing native construct, performed
  against the vendored schemas rather than from memory. The constitution
  requires the citation in the Functional Requirements, and milestone 1275
  recorded what happens when a plan asserts a reuse without checking it: C151
  is design-tier-only and C170 answers a different question, so that feature
  needed a new row after all.
- **FR-007**: The document MUST record how many components the rule exempted,
  including zero, so the correction is visible rather than silent.
- **FR-008**: Scopes other than the exempt class MUST be unchanged in every
  other ecosystem. No OS-package, file-tier, npm, Go, cargo, Maven or Python
  component changes scope.
- **FR-009**: All three emitted formats MUST agree about the corrected scope.
  A correction visible in CycloneDX and absent from SPDX is the asymmetry the
  parity discipline exists to catch.
- **FR-010**: Milestone 1275's `t057`, which asserts the defect, MUST be
  updated to assert the correction. It MUST NOT be deleted: the fixture it
  drives is the only deterministic reproduction of a boot library inheriting a
  scope through closure-derived edges.
- **FR-011**: Every requirement MUST be covered by a test that fails when the
  behaviour is removed, mutation-tested rather than assumed. FR-003's control
  — that non-boot components keep their scope — MUST have its own test,
  because the plausible wrong implementation exempts too much and would make
  every component below a disabled stanza look runtime.
- **FR-012**: Existing wire values MUST NOT change meaning. A consumer reading
  `required`, `optional` or `excluded` today reads the new document's the same
  way; what changes is which components carry them.
- **FR-013**: The feature MUST NOT add network access, subprocess calls or
  new filesystem reads. The compiler configuration is already retrieved and
  cached per revision.

### Key Entities

- **Compiler-supplied package**: a package the pinned revision's compiler
  configuration binds to `null`, meaning the compiler ships it rather than
  building it from Hackage. 78 such components across the corpus.
- **Non-runtime lifecycle scope**: `optional` or `excluded`. The schema
  defines both as claims about whether the package can be called at runtime,
  not about why the graph reaches it.
- **The exempt class**: the compiler-supplied packages, and only those. 10 of
  the 78 are mis-scoped today; the other 68 already carry no scope.
- **The propagation's immune set**: the components a non-runtime scope is not
  propagated to. The concept already exists — it is how runtime-reachable
  components are protected — so this feature extends an existing notion rather
  than introducing one.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On `slack-web`, offline against its pinned revision, `pretty`
  carries no non-runtime scope. Measured today: `excluded`.
- **SC-002**: Across the corpus, **0 of 78** boot-library components carry a
  non-runtime scope. Measured today: 10, in `haskell-security-advisories` (8)
  and `nix-closure-moat` (2).
- **SC-003**: On `slack-web`, the 8 non-boot components below the disabled
  stanza still carry their non-runtime scope. Measured today: 8, and the
  figure must not move.
- **SC-004**: No component outside the exempt class changes scope, in any
  corpus target. Measured today: **579** components across the corpus carry a
  non-runtime scope, of which 10 are compiler-supplied — so **569 must still
  carry one afterwards**. Every other ecosystem's output is byte-identical.
- **SC-005**: Every exempted component records why, and the document records
  the count. Measured today: **0** — no component records that its runtime
  presence is owed to the compiler, and no document records an exemption
  count, because neither exists yet.
- **SC-006**: All three formats report the same scope for every component.
- **SC-006a**: For a project whose flake names a compiler series, the exempt
  set equals that series' nulled names exactly. Measured at revision
  `a799d3e3`: 41 names for `9.10.x`, against 50 for the union — so a correct
  implementation exempts 41 and an over-exempting one 50, and the two are
  distinguishable by count alone.
- **SC-007**: Every requirement is covered by a test that fails when the
  behaviour is removed, mutation-tested.
- **SC-007a**: The document-level invariant of FR-002a holds on every corpus
  target, and is demonstrated to fail when a compiler-supplied component is
  given a non-runtime scope by any means. An invariant never seen to fail is
  not yet evidence of anything.
- **SC-008**: Every probe committed under `measurements/` re-runs and
  reproduces its recorded finding.

## Assumptions

- **The schema's text settles the semantics, so this feature does not
  re-litigate them.** #1340 presents two readings as equally defensible; the
  vendored CycloneDX 1.6 `meta:enum` definitions quoted in Context do not.
  Should a reviewer read those definitions differently, that disagreement is
  about this feature's premise and belongs in the issue rather than in the
  implementation.
- **An exempted component is left with no scope rather than an explicit
  `required`.** The schema states that an unspecified scope SHOULD be assumed
  `required`, so the two are equivalent to a conforming consumer, and 68 of
  the 78 boot-library components already carry no scope. Removing the wrong
  value makes the 10 consistent with the 68; stamping all 78 `required` would
  change 68 components that are already correct.
- **Other "always present" classes are out of scope, by measurement.** OS
  packages and file-tier components carry no lifecycle scope at all — 142 and
  175 components, every one scope-free — so they cannot be mis-scoped by this
  mechanism. If a future feature gives them scopes, the same reasoning will
  apply and this spec's measurement is where to start.
- **This branch is stacked on milestone 1275.** It is cut from
  `1275-closure-hackage-identity` rather than from `main`, because FR-010
  requires updating `t057` and the fixture it drives, which exist only there.
  If 1275 lands first the stack collapses naturally; if it does not, this
  feature's PR carries 1275's commits.
- **`pretty` is the measured subject and is a real Hackage coordinate.** The
  measurement probes use real coordinates deliberately, as the project's
  convention allows, because the subject of the measurement is this specific
  dependency graph. Test fixtures use `waybill-fixture-*` names only.
- **The defect is not a regression.** #1340 records that before milestone
  1273's #1281 fix the same component was runtime-reachable through a mark
  that should never have been withdrawn — wrong for a different reason.
  Neither answer was precise; what changed is which imprecision is visible.
