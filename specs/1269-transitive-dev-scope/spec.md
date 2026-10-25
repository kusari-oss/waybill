# Feature Specification: Transitive propagation of non-runtime lifecycle scope

**Feature Branch**: `1269-transitive-dev-scope`
**Created**: 2026-10-25
**Status**: Draft
**Input**: User description: "Propagate non-runtime lifecycle scope transitively across the resolved dependency graph (#1209). Readers assign dev/optional scope and nothing propagates it, so on pydantic 103 components reachable only through PEP 735 dev-group members carry no scope at all and read as runtime dependencies. New emit-time pass reusing generate/graph_completeness/bfs.rs and the root_selector seed set, NOT scan_fs/mod.rs:1283 (which runs before dedup and the m191 reconciler, so its component set is not final); edge typing re-runs after it. One lattice, Runtime > Optional > Build > Test > Development, strongest scope across all inbound paths wins. Phase A BFS from the roots along edges whose target has no non-runtime scope yields the runtime-reachable set, which is absolutely immune from propagation; Phase B BFS from each non-runtime variant's declared members, strongest first, assigning only to components carrying no scope. Strictly additive: never changes a reader-assigned scope, so npm output is unchanged by construction. One new catalog row waybill:lifecycle-scope-derivation (declared/transitive) with its parity extractor in the same change, because a propagated scope is otherwise indistinguishable from a declared one on the wire. Positive control required as a synthetic fixture: a component on both a runtime and a dev path keeps runtime, since a pass that over-propagates passes every count-based test while silently demoting real runtime dependencies. Per-ecosystem verification on uv, npm, cargo and gem, recorded per ecosystem. Zero new Cargo dependencies."

## Why this is not a greenfield feature

The detector already exists and already fails. Invariant **I3**
(`waybill-cli/tests/corpus_harness_195/invariants/checks/scope_propagation.rs`) —
*"a component reachable from the subject only through non-runtime edges
carries a non-runtime marker"* — implements the reachability test this
feature needs and is declared as a known failure against #1209 on five
corpus targets:

| target | ecosystem signal | count at declaration | declared reason |
|---|---|---|---|
| `python-flask` | pypi / `uv.lock` extras | 51 | docs + test extras resolved from `uv.lock` carry no marker; m183 classified declared extras only |
| `haskell-language-server` | cabal test-suite / benchmark stanzas | 26 | test and benchmark stanza deps reachable only by non-runtime edges |
| `haskell-security-advisories` | cabal | 13 | same defect |
| `nix-closure-moat` | cabal (through the flake, not the closure) | 9 | same defect |
| `rust-ripgrep` | cargo `[dev-dependencies]` | 2 | cargo propagates declared dev-deps, not through them |

Source: `invariant-declarations.toml`. Those counts are **context only
and never compared** by the lane, by that file's own rule — they say
where the defect is, not how big it must be after the fix.

This has three consequences for the shape of the work:

1. **The acceptance gate is already built.** The declarations file fails
   the lane on a *stale* declaration, so deleting those five entries in
   the same change as the fix is both the acceptance evidence and the
   thing that forces the evidence to be real.
2. **The per-ecosystem record the issue asks for already exists in
   draft.** Five targets across three ecosystems are measured on the
   current build. The issue's own numbers (103 / 76 / 21 on `pydantic`)
   were taken with waybill **0.10.1** against an external pin that is not
   a corpus target, and must not be used as a target without
   re-measurement.
3. **`npm-express` carries no I3 declaration**, which is the measured
   form of "npm is already correct": it is a passing cell today and must
   still be one afterwards.

## Two populations, one defect

The issue and I3 describe the same defect reached from opposite sides,
and conflating them is a known hazard — the code comment on I3 records
that *"two instances of one defect class landed under the wrong number
during the investigation"*.

- **Population A — reachable, unmarked.** Reachable from the subject,
  but every path crosses a non-runtime component. This is I3's
  population and the five declarations above.
- **Population B — unreachable, unmarked.** Not reachable from the
  subject at all, but reachable from a declared non-runtime component.
  This is `pydantic`'s 103: its 76 PEP 735 group members are themselves
  unreachable, so their transitives are counted as orphans and fall to
  I7 (`unreachable-and-unexplained`), not I3.

A propagation pass seeded from **declared non-runtime components**,
independent of root reachability, covers both. A pass seeded from
root reachability covers only A. The feature MUST cover both.

### A correction to the issue's "Done when"

The issue states that `pydantic`'s *"reported orphan count drops
accordingly"*. It will not, and the feature does not promise it.
`orphaned-components-detected` is computed purely from reachability in
`generate/graph_completeness/`, which does not read `lifecycle_scope` at
all. Marking a component non-runtime does not attach it, so an orphan
that is also a dev transitive stays an orphan — correctly, since it is
genuinely unattached. Reducing that count is an anchoring question
(I7's), filed separately. Conflating the two would let this feature
claim a number it did not move.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer filtering to runtime components stops seeing dev tooling (Priority: P1)

An operator feeds a waybill SBOM to an advisory scanner, or filters it
to the components that ship. Today a transitive dependency of a
development, test, build or optional dependency is emitted with a real
coordinate, a real version and nothing marking it as non-production, so
it is indistinguishable from a runtime dependency. Dev-tooling advisories
are attributed to the subject's runtime surface.

**Why this priority**: it is the defect. It is over-reporting rather
than under-reporting, and it is worse than an orphan, because an orphan
is visibly unattached while these look load-bearing.

**Independent Test**: run the corpus lane on `python-flask` with its I3
declaration deleted; the cell must pass. One target, one ecosystem, no
other story required.

**Acceptance Scenarios**:

1. **Given** a component whose every inbound path crosses a component
   carrying a non-runtime scope, **When** the SBOM is emitted, **Then**
   that component carries a non-runtime scope in CycloneDX, SPDX 2.3 and
   SPDX 3.
2. **Given** `python-flask`, `haskell-language-server`,
   `haskell-security-advisories`, `nix-closure-moat` and `rust-ripgrep`,
   **When** the corpus invariant lane runs with their I3 declarations
   removed, **Then** every I3 cell passes and no new declaration is added
   in their place.
3. **Given** a component that is unreachable from the subject but
   reachable from a declared non-runtime component (population B),
   **When** the SBOM is emitted, **Then** it carries a non-runtime scope
   — propagation does not require root reachability.
4. **Given** a dependency cycle among non-runtime components, **When**
   the pass runs, **Then** it terminates and every member of the cycle
   is marked.

---

### User Story 2 - A real runtime dependency is never demoted (Priority: P1)

A package reachable both as a runtime dependency and as a dependency of
dev tooling keeps runtime scope. A consumer must never lose a component
that genuinely ships.

**Why this priority**: also P1, and not negotiable. A pass that
over-propagates satisfies every count-based test in User Story 1 while
silently demoting shipping dependencies — under-reporting, invisible in
orphan counts, and strictly worse than the bug being fixed. Shipping
US1 without US2 is net-harmful, which is why it is a separate story with
its own evidence rather than a line in US1's acceptance.

**Independent Test**: a synthetic fixture with one component on both a
runtime and a dev path, asserted to keep runtime scope and to carry no
derivation marker. Testable with no corpus target and no network.

**Acceptance Scenarios**:

1. **Given** a component reachable from the subject by at least one path
   containing no non-runtime component, **When** the pass runs, **Then**
   its scope is unchanged, whatever other paths reach it.
2. **Given** a component carrying a reader-assigned scope, **When** the
   pass runs, **Then** that scope is unchanged — the pass only fills an
   absent scope and never overwrites one.
3. **Given** a component reachable from both a declared `optional` and a
   declared `development` component and from no runtime path, **When**
   the pass runs, **Then** it carries `optional`: the strongest
   non-runtime scope across all inbound paths wins, because `optional`
   claims the component may ship and `development` claims it does not,
   and the weaker claim about production is the safer one to publish.
4. **Given** `npm-express`, whose lockfile already stamps every
   transitive and which carries no I3 declaration today, **When** the
   pass runs, **Then** its three emitted documents are byte-identical to
   the pre-change goldens.

---

### User Story 3 - A consumer can tell what waybill was told from what waybill inferred (Priority: P2)

A propagated scope and a reader-declared scope are indistinguishable on
the wire: both produce the same scope value and the same CycloneDX
`scope: excluded`. A consumer deciding how much to trust a
classification cannot see which is which.

**Why this priority**: P2 because US1 delivers the correctness fix
without it, and Principle X transparency is what makes the inference
auditable rather than just applied. It is in the same change because a
scope whose provenance is unrecorded is the kind of silent conflation
this project has had to retrofit before.

**Independent Test**: emit any document containing both a declared and a
propagated non-runtime component and read the two values off the
component properties in all three formats.

**Acceptance Scenarios**:

1. **Given** a component whose scope came from a reader, **When** the
   SBOM is emitted, **Then** it carries a derivation marker valued
   `declared`.
2. **Given** a component whose scope came from this pass, **When** the
   SBOM is emitted, **Then** it carries a derivation marker valued
   `transitive`.
3. **Given** a component with no non-runtime scope, **When** the SBOM is
   emitted, **Then** it carries no derivation marker at all — the marker
   explains a scope, it does not assert one.
4. **Given** all three formats, **When** the parity suite runs, **Then**
   the marker round-trips with equal values in each and the catalog row
   and its extractor land together.

---

### User Story 4 - The reach of the fix is recorded per ecosystem, not inferred from one target (Priority: P2)

Someone later asks "does this work for gem?". The answer must be a
record, not an inference from the one target that was eyeballed.

**Why this priority**: P2. The issue's own history is the argument:
`specs/194-close-orphan-gaps/spec.md:60` deferred this exact class with
*"filed separately if they surface"*, and surfacing depended on someone
looking at the right target.

**Independent Test**: read the per-ecosystem table produced by the
feature and confirm each row cites either a corpus target or a named
synthetic fixture.

**Acceptance Scenarios**:

1. **Given** the ecosystems named as suspect — pypi/uv, npm, cargo, gem,
   maven/gradle optional, composer `require-dev`, mix, rebar, cabal,
   nix — **When** the feature completes, **Then** each has a recorded
   outcome of *fixed*, *already correct*, or *not reached*, each citing
   the evidence it rests on.
2. **Given** that the corpus has **no gem target** (17 targets:
   `go-cobra`, `go-opentelemetry`, `haskell-aeson`,
   `haskell-language-server`, `haskell-security-advisories`,
   `image-postgres16`, `maven-guice`, `nix-closure-moat`, `npm-express`,
   `pants-clojure-polyglot`, `pants-example-django`,
   `pants-example-golang`, `pants-example-javascript`,
   `pants-example-jvm`, `pants-example-python`, `python-flask`,
   `rust-ripgrep`), **When** gem is claimed verified, **Then** the claim
   cites a synthetic fixture and says so, rather than implying corpus
   coverage.
3. **Given** an ecosystem recorded as *not reached*, **When** the
   feature completes, **Then** the reason is stated, so the next person
   extends a record rather than re-running the investigation.

---

### Edge Cases

- **Cycles among non-runtime components** — the pass must terminate and
  mark every member. A visited set, not recursion depth.
- **Multiple roots.** The seed set is per-ecosystem, not a single
  subject. A component runtime-reachable from *any* seed is immune.
- **No root resolvable for an ecosystem.** Phase A cannot establish
  immunity there. The pass MUST NOT propagate into an ecosystem whose
  root could not be identified, because immunity cannot be established
  and the failure direction is demotion of a real runtime dependency.
  Such components keep their absent scope and the existing
  `multi-ecosystem-partial-root` signal already reports the condition.
- **A declared non-runtime component that is itself runtime-reachable.**
  Three of `pydantic`'s 76 declared-optional components are reachable
  from the root yet carry `scope: excluded`. This feature does not
  resolve that: it is a reader/declaration conflict, not a graph
  question, and a graph pass silently overriding a reader's declaration
  is a worse outcome than the inconsistency. Filed separately.
- **An empty or single-component graph** — no seeds, no edges, no
  change, no diagnostic noise.
- **Documents with no non-runtime component at all** — byte-identical
  output, which is most documents.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A component whose every inbound path crosses a component
  carrying a non-runtime scope MUST be emitted with a non-runtime scope.
- **FR-002**: A component reachable from any root seed by at least one
  path containing no non-runtime component MUST keep its existing scope.
  Runtime reachability is absolute immunity from propagation.
- **FR-003**: The pass MUST be strictly additive: it assigns a scope
  only to a component that carries none, and MUST NOT change,
  strengthen or weaken a scope assigned by a reader.
- **FR-004**: When multiple inbound paths would assign different scopes,
  the **strongest** MUST win, ordered `Runtime > Optional > Build >
  Test > Development`.
- **FR-005**: Propagation MUST NOT require root reachability: a
  component reachable only from an unreachable declared non-runtime
  component (population B) MUST be marked.
- **FR-006**: The pass MUST propagate the same scope variant rather than
  introducing a new one, so that every existing consumer-side filter and
  every format's native scope mapping keeps working unchanged.
- **FR-007**: Components emitted with a non-runtime scope MUST carry a
  derivation marker distinguishing a reader-declared scope from one this
  pass inferred, with exactly two values in v1: `declared` and
  `transitive`.
- **FR-008**: A component with no non-runtime scope MUST NOT carry the
  derivation marker.
- **FR-009**: The derivation marker MUST be emitted in CycloneDX, SPDX
  2.3 and SPDX 3 with equal values, and its catalog row and parity
  extractor MUST land in the same change — a row without an extractor
  fails the parity suite by design.
- **FR-010**: Dependency edges MUST agree with component scope after
  the pass: an edge into a component the pass marked MUST carry the
  corresponding typed relationship in every format.
- **FR-011**: The pass MUST terminate on cyclic graphs and mark every
  member of a non-runtime cycle.
- **FR-012**: The pass MUST NOT propagate into an ecosystem for which
  no root seed could be resolved.
- **FR-013**: An ecosystem whose reader already marks transitives MUST
  produce byte-identical output. `npm-express` is the named control.
- **FR-014**: Deleting the five I3 declarations MUST happen in the same
  change as the fix, per the declarations file's stale-entry rule.
- **FR-015**: The feature MUST produce a per-ecosystem record of
  outcomes (*fixed* / *already correct* / *not reached*), each citing a
  corpus target or a named synthetic fixture.
- **FR-016**: Zero new Cargo dependencies at any workspace level.
- **FR-017**: The standards-native audit for the derivation marker MUST
  be performed against the vendored schemas under
  `waybill-cli/tests/fixtures/schemas/`, not from memory, and its
  method and quotations recorded.
- **FR-018**: The feature MUST NOT claim any reduction in
  `orphaned-components-detected`. Scope marking does not change
  reachability, and that count is derived from reachability alone.

### Key Entities

- **Lifecycle scope** — the per-component classification
  (`runtime`, `optional`, `build`, `test`, `development`) that decides
  whether a component is part of the shipped artifact. Totally ordered
  for this feature by FR-004.
- **Root seed set** — the per-ecosystem subjects reachability is
  measured from. Already derived for the existing completeness
  reporting; this feature consumes it rather than defining it.
- **Runtime-reachable set** — components reachable from a seed by a path
  containing no non-runtime component. The immunity set of FR-002.
- **Declared non-runtime member** — a component a reader assigned a
  non-runtime scope to. The propagation seeds, independent of
  reachability.
- **Scope derivation** — per-component provenance of the scope:
  `declared` by a reader, or `transitive` by this pass.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Zero components reachable only through non-runtime
  components are emitted without a non-runtime marker, across all 17
  corpus targets and all three formats — excepting only components in an
  ecosystem for which no root seed could be resolved, which FR-012
  deliberately declines to touch and which any such exception MUST name.
  Expressed as a count of zero rather than as a reduction, so it cannot
  be satisfied by partial progress.
- **SC-002**: All five I3 declarations are deleted and the corpus
  invariant lane is green with none re-added.
- **SC-003**: A positive control exists as a synthetic fixture in which
  a component reachable by both a runtime and a dev path keeps runtime
  scope, and it fails if propagation is made unconditional. Mutation-
  demonstrated, not merely present.
- **SC-004**: `npm-express`'s three emitted documents are byte-identical
  to their pre-change goldens.
- **SC-005**: Every ecosystem in FR-015's list has a recorded outcome
  citing named evidence; no row reads "assumed".
- **SC-006**: A consumer can separate declared from inferred scope for
  every non-runtime component in all three formats.
- **SC-007**: Golden churn is accounted for: every changed golden is
  explained by a propagated scope or a derivation marker, and any other
  change is treated as a regression rather than as churn.
- **SC-008**: `pydantic`'s unscoped-transitive count is **re-measured on
  the current build** and reaches zero. The 103 from the issue was
  observed with waybill 0.10.1 and is a provenance note, not a target.

## Assumptions

- The five I3 declarations describe one defect class. Their reasons say
  so explicitly; if implementation finds any of them to be a different
  defect, that one is split out rather than silently carried.
- Counts in `invariant-declarations.toml` are context, not targets, by
  that file's own rule. SC-001 is therefore count-of-zero rather than a
  delta against 101.
- Corpus goldens are the acceptance surface for population A. They are
  `#[ignore]`d locally and the PR lane skips them, so acceptance requires
  a corpus workflow dispatch on the feature branch, and goldens are never
  generated locally.
- Population B needs a synthetic fixture, because no corpus target
  reproduces `pydantic`'s shape and the external pin that does is not
  part of the corpus.
- gem, composer, mix and rebar have no corpus target, so any claim about
  them rests on synthetic fixtures and must say so.
- The existing per-ecosystem root selection and reachability machinery is
  reused rather than reimplemented; this feature adds a propagation pass,
  not a second notion of reachability.
- The pass runs where the component set is final — after de-duplication
  and design/source reconciliation — and edge typing is re-established
  afterwards so FR-010 holds. The four existing unit tests that call the
  edge-typing function directly pin it to its current position and will
  need to move with it.
