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
| dependency edges the closure contributes | **0** |
| components the document reports as orphans | 352 of 406 |
| `waybill:graph-completeness` | `partial` |

So every closure component is an orphan by construction. A consumer gets 381
packages with no statement of how any of them relate, and the document says
its graph is incomplete because it is.

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
2. **Given** a closure member reached only through a derivation waybill does
   not emit as a component (a source fetch, a wrapper, a setup hook),
   **When** the document is emitted, **Then** that member is still connected —
   the projection does not stop at a node that was filtered out.
3. **Given** the same closure scanned twice, **When** the two documents are
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
  component reachable in nix's own graph is reachable in the emitted document.
  The derivation that nothing else consumes is the closure's root and MUST
  serve as that anchor even when the existing emission rule does not emit it
  as a component — otherwise, measured, 381 of 381 components stay orphaned
  while 3,088 edges are emitted, which is worse than emitting none because the
  graph then looks populated and is not.
- **FR-005**: A dependency naming a derivation absent from the closure's own
  map MUST be dropped, and a self-dependency MUST NOT be emitted.
- **FR-006**: The walk MUST terminate on any input, including one containing a
  cycle, and MUST be bounded by a visited set rather than by recursion depth.
- **FR-007**: The relationships MUST appear in all three output formats with
  the same meaning, through each format's native dependency vocabulary —
  CycloneDX `dependencies[].dependsOn`, SPDX 2.3 relationship types, SPDX 3
  `Relationship` elements — and not through a `waybill:` annotation. This is
  Constitution Principle V: a dependency edge is the one thing all three
  formats model natively.
- **FR-008**: The emitted relationship count MUST be recorded in the existing
  document-scope closure record (C184), so a consumer can tell a closure that
  contributed no edges from one that was never read.
- **FR-009**: `--nix-closure` MUST remain the only way to reach this behaviour.
  A scan without it MUST produce byte-identical output to today.
- **FR-010**: How an edge is projected when the dependency is a derivation
  waybill does not emit as a component is [NEEDS CLARIFICATION: see Question 1
  — direct-only, full transitive projection, or projection through non-package
  derivations only. Measured on x86_64-linux: 3,088 edges and 381 orphans,
  16,187 edges and 0 orphans, or a figure between the two.]

### Key Entities

- **Derivation input map**: per derivation, the set of other derivations whose
  outputs the build consumed. nix records it; waybill currently discards it.
  Present on all 1,749 derivations of the measured closure.
- **Closure component**: the `(name, version)` identity several derivations may
  collapse onto. Edges are between these, not between derivations, so the merge
  applies to relationships as well as to components.
- **Closure root**: the single derivation no other derivation consumes. One on
  the measured closure (`moat-0.1`), and not itself an emitted component.

## Success Criteria *(mandatory)*

### Measurable Outcomes

All figures are the `nix-closure-moat` corpus target at its pin,
**x86_64-linux** — the closure the committed goldens pin. The aarch64-darwin
closure has a different derivation population, so a figure measured there does
not transfer; both were measured, and `measurements/` records each.

- **SC-001**: The closure tier contributes a non-zero number of dependency
  relationships, where today it contributes exactly 0.
- **SC-002**: No emitted closure component is reported as an orphan unless it
  is genuinely unreachable in nix's own graph. Measured target on
  x86_64-linux: orphans fall from **352** to **0** for closure components.
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
