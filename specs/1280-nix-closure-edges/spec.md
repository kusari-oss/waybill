# Feature Specification: the nix closure tier emits its dependency graph

**Feature Branch**: `1280-nix-closure-edges`
**Created**: 2026-11-05
**Status**: Draft
**Input**: User description: "The nix closure tier emits its dependency graph, read from the `inputs.drvs` field nix records"

## Context

The closure tier contributes components and **no dependency relationships at
all**. That is not an inference — the catalogue says so in C205's own row
("it emits none today"), and a grep for `Relationship` across
`waybill-cli/src/scan_fs/package_db/nix/closure/` returns nothing.

Measured on the `nix-closure-moat` corpus target at its pin, **x86_64-linux**,
1,749 derivations:

| | today |
|---|---|
| closure components emitted | 381 |
| **dependency** edges the closure contributes | **0** |
| components the document reports as orphans | 352 of 406 |
| `waybill:graph-completeness` | `partial` |

So every closure component is an orphan by construction. A consumer gets 381
packages with no statement of how any of them relate, and the document says
its graph is incomplete because it is.

**One nuance, measured rather than assumed.** The closure is not entirely
absent from the relationship arrays: milestone 1277 emits **103**
`BUILD_TOOL_OF` edges in SPDX 2.3, one per component it classifies as absent
from the artifact, pointing *member → root*. Those are a claim about artifact
membership, not about consumption, they point the wrong way to make anything
reachable from the root, and CycloneDX has no equivalent so carries none. The
zero above is therefore specifically **dependency** edges, in every format —
which is what a consumer asking "what does this depend on" reads, and what the
orphan pass walks.

Meanwhile `nix derivation show` records **`inputs.drvs`** on every derivation:
the derivations whose outputs that build consumed. waybill never reads it. It
parses the *declared* attributes instead (`buildInputs`, `nativeBuildInputs`,
…), which is what a package says it needs rather than what the build used, and
it uses them only to assign the C182 role — never to emit an edge.

`inputs.drvs` holds **10,342** edges on this closure and reaches 1,748 of the
1,749 derivations from a single root. On aarch64-darwin it holds **8,230** —
equal, to the edge, to the count sbomnix reports, which is how we know sbomnix
is emitting this graph and waybill is not.

Probe: `measurements/input-graph-probe.py`.

## Clarifications

### Session 2026-11-05

- Q: What does the closure root's edges attach to, given the root derivation is not itself an emitted component? → A: **The document root component.** The closure is the closure *of* the described artifact, so the claim is exact, needs no identity matching, and holds for every `--nix-closure-attr` target including system configurations that correspond to no package.
- Q: Which relationship type should the new edges use, given that milestone 1277 already emits `BUILD_TOOL_OF` for the same components? → A: **A uniform dependency edge regardless of role** — `dependsOn` / `DEPENDS_ON` / `dependsOn` — with m1277's `BUILD_TOOL_OF` edges left exactly as they are. The two answer different questions and are both true.
- Q: How should an edge be projected when its target is a derivation waybill does not emit as a component? → A: **Through non-package derivations only** — walk through a derivation that declares no `pname` (a source fetch, a wrapper, a setup hook), and stop at one that does. Measured against three alternatives before the question was put; see `measurements/README.md`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer can see what the closure's components depend on (Priority: P1)

Someone receives an SBOM produced with `--nix-closure` and asks the ordinary
question: what does this artifact depend on, and through what? Today the answer
is "381 components, no edges, and the document admits it is incomplete". After
this change the closure's components are connected to the root and to each
other, so the question has an answer.

**Why this priority**: it is the whole feature, and it is the thing that makes
every other closure signal usable. A component with no edges cannot be placed
in a build; a role annotation on an unconnected node says what kind of thing
it is without saying where it sits.

**Independent Test**: scan a flake with `--nix-closure`, and assert the emitted
document contains dependency relationships whose endpoints are closure
components. Fully testable with a stub `nix` and a fixture closure; delivers
the connected graph on its own with no other story implemented.

**Acceptance Scenarios**:

1. **Given** a closure whose root consumed package `A`, which consumed package
   `B`, **When** the document is emitted, **Then** there is a path from the
   root to `B`, and `A` is on it.
2. **Given** a closure member reached only through a derivation that declares
   no `pname` (a source fetch, a wrapper, a setup hook), **When** the document
   is emitted, **Then** that member is still connected — the projection walks
   through the fetch rather than stopping at it.
3. **Given** a closure member reached only through a derivation that declares a
   `pname` and is not emitted, **When** the document is emitted, **Then** the
   member is reported as an orphan whose reason names that cause, rather than
   being connected by a transitive edge nix did not state.
4. **Given** the same closure scanned twice, **When** the two documents are
   compared, **Then** the relationship sets are byte-identical.

---

### User Story 2 - The graph stops claiming an incompleteness it caused itself (Priority: P2)

A consumer reads `waybill:graph-completeness` and the native carriers behind it
(CycloneDX `compositions[].aggregate`, SPDX 3 `Relationship.completeness`) to
decide how much to trust the document. Today the closure guarantees those say
"incomplete" for a reason that has nothing to do with what nix knows: waybill
simply did not emit the edges.

**Why this priority**: it is a correctness consequence rather than a new
capability, and it follows from US1 rather than needing its own mechanism. It
matters because an always-incomplete signal is an ignored signal.

**Independent Test**: compare the orphan count and completeness verdict before
and after on the same closure, with no other change.

**Acceptance Scenarios**:

1. **Given** a closure in which every emitted component is reachable from the
   root, **When** the document is emitted, **Then** no emitted closure
   component is reported as an orphan.
2. **Given** a closure that genuinely leaves a component unreachable,
   **When** the document is emitted, **Then** that component is still reported
   as an orphan with a reason, and the completeness claim still says so.

---

### User Story 3 - The role annotations become a filter rather than a gate (Priority: P3)

An operator wants everything the closure knows about, and wants to narrow it
afterwards — "show me only what goes into the artifact", "hide the build
tooling". The C182 role is already the right vocabulary for that. What it
cannot do today is filter a graph, because there is no graph.

**Why this priority**: it needs nothing new; it is what US1 makes possible, and
stating it keeps the role annotations from being mistaken for this feature's
job. Any change to what the roles *mean* is out of scope here.

**Independent Test**: assert that each emitted relationship's endpoints carry
their existing role annotations unchanged, so a consumer can partition the
graph by role without waybill having pre-partitioned it.

**Acceptance Scenarios**:

1. **Given** the document after this change, **When** the C182 role census is
   compared against the same scan before it, **Then** the census is identical —
   emitting edges does not reclassify anything.

---

### Edge Cases

- **A dependency cycle.** nix derivations cannot form one, but the reader must
  not assume it: a malformed or hand-written closure must terminate rather
  than recurse forever, and the walk must be bounded by the visited set.
- **A derivation that depends on itself**, directly or through its own
  outputs — the self-edge must be dropped, not emitted.
- **Several derivations collapsing to one component** (build variants merged on
  `(pname, version)`). Their edges merge too, and the result must be
  deduplicated, or one component emits the same relationship many times.
- **A dependency that is not in the closure at all.** `inputs.drvs` can name a
  `.drv` the closure's own map does not contain; such an entry is dropped.
- **A closure with exactly one derivation**, which has no edges to emit — the
  document must carry no relationships rather than a malformed empty structure.
- **The root derivation is not itself an emitted component.** Measured: on
  this closure `moat-0.1` is referenced by nothing, so it has no C182 role and
  applies no patches, and the existing emission rule drops it. A projection
  anchored only on emitted components therefore has no anchor — see FR-004.
- **A component consumed only by a package that is not emitted.** Measured:
  five on x86_64-linux, sixteen on aarch64-darwin, every one with that same
  cause. FR-010 stops at the package, so the component stays an orphan and
  FR-011 requires the reason to say which kind of orphan it is.
- **A closure scanned on a system whose derivation set differs.** The figures
  in this spec are x86_64-linux; aarch64-darwin has a different population and
  the criteria say which system they are stated against.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The closure tier MUST emit dependency relationships between the
  components it emits, derived from the `inputs.drvs` map that `nix derivation
  show` records for each derivation.
- **FR-002**: The reader MUST NOT derive relationships from the declared
  dependency attributes (`buildInputs` and the rest). Those stay the sole
  input to the C182 role, and this separation is what keeps the role census
  unchanged by this feature (SC-005). Measured: the declared attributes would
  yield 2,403 component-level edges against `inputs.drvs`' 3,088 direct, and
  would also inherit the attributes' own coverage gaps.
- **FR-003**: Relationships MUST be deduplicated and ordered deterministically,
  so that two scans of one closure produce byte-identical documents.
- **FR-004**: The graph MUST be anchored such that every emitted closure
  component reachable in nix's own graph is reachable in the emitted document
  **whenever the closure's root derivation is representable as a component**.
  The derivation that nothing else consumes is the closure's root, and the
  edges projected from it MUST resolve through the same identity join as every
  other node's.

  **Refined during implementation, and the refinement is a narrowing.** The
  first draft required those edges to attach to the *document root component*
  unconditionally. That cannot be done from the reader: `target_ref`, the
  string all three emitters use for the subject, is derived inside each builder
  from `select_root` and main-module detection
  (`cyclonedx/builder.rs:832`), so reproducing it at reader level would be a
  second copy of the logic `select_root` exists to centralise. Anchoring an
  unrepresentable root belongs beside
  `graph_completeness::anchor_retained_mainmod_edges`, which solves the same
  problem for other readers with `target_ref` in hand.

  Measured, the narrowing costs nothing on any closure observed: the root
  `moat-0.1` is already `pkg:hackage/moat@0.1` from the Haskell manifest
  reader, which the document root already depends on, so the join resolves it
  and the emitted scan reports `unanchored_root_edges = 0`.
- **FR-004b**: When the closure root resolves to no component, its projected
  edges MUST be dropped and counted, and the count logged — never attached to
  the subject by assertion. An edge from the described artifact to a member
  only the root reached would be a dependency nix did not state.

  The root derivation MUST NOT itself be emitted as a component on account of
  this feature. Measured, it is already represented: `moat-0.1` corresponds to
  `pkg:hackage/moat@0.1`, which the manifest reader emits and the document root
  already depends on. Emitting a closure-tier `pkg:generic/moat@0.1` beside it
  would be the divergent-PURL defect (#1216).

  The anchor is not optional. Measured, without it 381 of 381 components stay
  orphaned while 3,088 edges are emitted — worse than emitting none, because
  the graph then looks populated and is not.
- **FR-004a**: The attachment MUST NOT depend on matching the closure root's
  identity against an existing component. The closure spells identities
  `pkg:generic/…` and the manifest spelled this one `pkg:hackage/…`, so any
  such match crosses a PURL-type boundary, and a wrong match silently
  reparents the entire graph. The document root is always present and requires
  no match.
- **FR-005**: A dependency naming a derivation absent from the closure's own
  map MUST be dropped, and a self-dependency MUST NOT be emitted.
- **FR-006**: The walk MUST terminate on any input, including one containing a
  cycle, and MUST be bounded by a visited set rather than by recursion depth.
- **FR-007**: The relationships MUST appear in all three output formats with
  the same meaning, through each format's native dependency vocabulary —
  CycloneDX `dependencies[].dependsOn`, SPDX 2.3 `DEPENDS_ON`, SPDX 3
  `dependsOn` — and not through a `waybill:` annotation. This is Constitution
  Principle V: a dependency edge is the one thing all three formats model
  natively.
- **FR-007a**: The relationship type MUST be the same whatever the endpoint's
  C182 role. The role MUST NOT select a relationship type.

  This is what keeps the three documents carrying one graph. SPDX 2.3 could
  express `BUILD_TOOL_OF` where the target is `build-tooling` and CycloneDX
  cannot, so mapping role to type would make the formats disagree about the
  graph's shape and force the parity extractor to compensate — the failure
  mode this project has already paid for once, where an extractor hand-split a
  pipe-string to paper over a shape difference.

  It also keeps the role usable as a *filter*: a consumer can partition this
  graph by role precisely because the role did not shape it.
- **FR-007b**: Milestone 1277's `BUILD_TOOL_OF` edges MUST be left unchanged —
  103 of them in SPDX 2.3 on the measured closure. They state that a member is
  absent from the artifact; these state what consumed what. Two true statements
  about one member, answering different questions, and neither is a substitute
  for the other: `BUILD_TOOL_OF` points member → root and so cannot make
  anything reachable from the root, which is why it has not prevented a single
  one of the 352 orphans.
- **FR-008**: The emitted relationship count MUST be recorded in the existing
  document-scope closure record (C184), so a consumer can tell a closure that
  contributed no edges from one that was never read.
- **FR-009**: `--nix-closure` MUST remain the only way to reach this behaviour.
  A scan without it MUST produce byte-identical output to today.
- **FR-010**: When a dependency names a derivation waybill does not emit as a
  component, the walk MUST continue through it **if and only if that derivation
  declares no `pname`** — a source fetch, a wrapper, a setup hook — and MUST
  stop at any derivation that declares one, whether or not that derivation is
  emitted.

  The rule is semantic rather than a tuning parameter, which is why it is this
  one: a tarball is not a dependency, it is how a dependency arrived, so
  passing through it preserves the real edge. A package *is* a dependency, so
  passing through it would assert `A depends on C` when the truth is
  `A depends on B depends on C` — a transitive-closure artefact, not a fact
  nix stated.

  Measured on x86_64-linux, against the three alternatives: 3,429 edges and 5
  residual orphans, where direct-only gives 3,088 and **381 of 381** orphaned,
  full transitive gives 16,187 and 0, and direct-plus-root-anchor gives 3,105
  and 7 by manufacturing edges from the root that are true of everything and
  therefore say nothing.
- **FR-011**: A closure component left unreachable by FR-010 **whose C182 role
  asserts that something references it** — `artifact-input`, `build-tooling` or
  `both` — MUST carry an orphan reason naming the cause. A component whose role
  is `unreferenced` MUST NOT: that role already says nothing references it, and
  a second explanation is what the milestone-1218 guard exists to prevent.

  **Narrowed during implementation, and the narrowing is the interesting
  part.** The first draft asked for a reason on every residual orphan. The
  milestone-1218 classifier treats `waybill:closure-role` *itself* as a
  sufficient explanation, and invariant I7's `is_explained` mirrors it, so the
  code could never fire — correctly, by that convention.
  
  The convention is right for `unreferenced` and wrong for the other three. A
  component whose role says *something depends on this* and which nothing in
  the graph reaches is a contradiction between two of the document's own
  statements, and C182 cannot be the explanation because it is one half of the
  problem. Measured on aarch64-darwin: **20 of the 25** unreachable closure
  components are in that position (14 `build-tooling`, 6 `artifact-input`), and
  the remaining 5 are `unreferenced` and stay explained by their role.

  Both the classifier guard and I7's mirror are narrowed together. The frozen
  `explained_pre_1156` copy is deliberately **not** touched: it is a historical
  snapshot used as a drift detector.
- **FR-012**: The projection MUST remain linear in the size of the closure.
  A walk restarted from scratch for each of the 381 emitted components over a
  10,342-edge graph is 381 full traversals; the measured closures are small
  enough to hide that, which is precisely why the requirement is stated rather
  than left to be discovered on a larger one.

### Key Entities

- **Derivation input map**: per derivation, the set of other derivations whose
  outputs the build consumed. nix records it; waybill currently discards it.
  Present on all 1,749 derivations of the measured closure.
- **Closure component**: the `(name, version)` identity several derivations may
  collapse onto. Edges are between these, not between derivations, so the merge
  applies to relationships as well as to components.
- **Closure root**: the single derivation no other derivation consumes. One on
  the measured closure (`moat-0.1`), not itself an emitted component, and
  already represented in the document by `pkg:hackage/moat@0.1` from the
  manifest reader.
- **Document root component**: the component the document describes
  (`nix-closure-moat@d069055` on the measured target). The anchor the closure
  root's edges attach to.

## Success Criteria *(mandatory)*

### Measurable Outcomes

All figures are the `nix-closure-moat` corpus target at its pin,
**x86_64-linux** — the closure the committed goldens pin. The aarch64-darwin
closure has a different derivation population, so a figure measured there does
not transfer; both were measured, and `measurements/` records each.

- **SC-001**: The closure tier contributes a non-zero number of dependency
  relationships, where today it contributes exactly 0.
- **SC-002**: Closure-component orphans fall substantially, and the residue
  carries the FR-011 reason. **The figures first stated here — 5 on
  x86_64-linux, 16 on aarch64-darwin — came from a probe, not the
  implementation, and the implementation measures 25 on darwin.** The probe
  keyed identity on `pname` alone while the reader falls back to the
  derivation's `name`. Scored against a measurement of the binary; the
  pre-change figure of 352 on linux is from the committed golden and stands.
  Measured so far on aarch64-darwin: **359 → 25**. **Not to zero, and deliberately
  not**: the five are reachable only through a package waybill does not emit,
  FR-010 declines to walk through a package, and FR-011 requires each to say so.
  Connecting them would mean either asserting a transitive edge nix did not
  state or emitting the intermediate packages, and the second is the next
  milestone's job.
- **SC-003**: The `orphaned-components-detected` clause disappears from
  `waybill:graph-completeness-reason`, and the CycloneDX and SPDX 3 native
  completeness carriers agree with whatever verdict remains — they render from
  one shared helper, so disagreement is a defect rather than a difference.

  **`graph-completeness` itself may well stay `partial`**, and the criterion is
  worded against the reason rather than the verdict for that reason. Today's
  value is `partial` with the reason
  `orphaned-components-detected: 352 component(s) not reachable from root;
  transitive-edges-unresolvable: hackage` — two independent causes, and this
  feature addresses only the first. A criterion saying "completeness becomes
  complete" would fail for the milestone working perfectly.
- **SC-004**: Two scans of the same closure emit byte-identical relationship
  sets, asserted by a test rather than by inspection.
- **SC-005**: The C182 role census is **unchanged** —
  268 / 121 / 45 / 1,315 across artifact-input / build-tooling / both /
  unreferenced, and 237 / 92 / 41 / 11 at component level. A run in which the
  census moves means relationships have leaked into classification, which
  FR-002 exists to prevent.
- **SC-006**: Every corpus target that does not scan a closure produces
  byte-identical goldens.
- **SC-007**: A closure containing a cycle terminates. Asserted with a fixture
  that nix could not produce, because the guard must not depend on nix's
  guarantees.
- **SC-008**: The emitted edge count is traceable to the closure it came from
  via C184, and a degraded closure records no count rather than zero.
- **SC-009**: Removing the `inputs.drvs` read makes a test fail. A graph source
  no test is sensitive to is not evidence that it is being read.
- **SC-010**: The 103 SPDX 2.3 `BUILD_TOOL_OF` relationships are still
  present and still 103. A run in which they move means this feature has
  reached into milestone 1277's carrier, which FR-007b forbids.

## Assumptions

- **The closure tier's component set does not change.** This feature emits
  edges between the components waybill already emits; it does not add the 1,134
  derivations that carry no `pname`. That is the next milestone, and it is
  deliberately separate so this one's effect on the graph is attributable.
  Measured: 522 of those 1,134 have exactly one named consumer, so they can be
  attached when the time comes.
- **The C182 role keeps its current meaning and its current inputs.** Roles
  come from declared attributes; edges come from `inputs.drvs`. Two axes, and
  the operator filters on the first across the second.
- **Build variants stay merged** on `(pname, version)`. Their edges merge with
  them.
- **`inputs.drvs` is available wherever the tier runs at all.** It is a field
  of the same `nix derivation show` output the reader already parses, present
  in format version 4 on both measured closures. A closure that lacks it
  degrades to today's behaviour rather than failing.
- **No new dependency on `nix` beyond what the tier already requires.** No new
  subprocess, no new flag, no network.

## Out of scope

- Emitting the fetch, patch and wrapper derivations as components. Next
  milestone; the ownership measurement is already done.
- Changing what C182's roles mean, or which attributes feed them.
- Un-merging build variants.
- Making the closure tier emit edges for non-nix ecosystems, or changing any
  other reader's graph.
