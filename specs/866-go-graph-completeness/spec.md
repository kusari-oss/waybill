# Feature Specification: Go scans assert dependency edges they never read, and then report the result as complete

**Feature Branch**: `866-go-graph-completeness`
**Created**: 2026-09-13
**Status**: Draft
**Input**: User description: "go graph flatness and completeness mis-report"

Closes #857, #829 and #871.

**#871 was folded in during implementation.** It is the same defect in
the standards-native channel: CycloneDX `compositions[]` asserts
`aggregate: complete` with every component listed under `dependencies`,
which the CycloneDX 1.6 schema defines as "the dependency graph is
complete for these components" — while 7 of `go-cobra`'s 8 components
have no edges. Filed separately at first, then folded in because fixing
#829 alone makes the document *self-contradicting*: the `waybill:`
annotation becomes honest (`partial`) while the spec-native field keeps
overclaiming (`complete`), and the native one is what a consumer reads.
Before this milestone both channels agreed and were both wrong; shipping
#829 without #871 would have left them disagreeing.

## The defect in one sentence

When the Go module graph cannot be resolved, waybill attaches the whole
module set to the main module — asserting direct dependency
relationships it never read from any manifest — and those invented edges
then satisfy the reachability check that decides whether the document
declares itself complete.

The fabrication is not merely alongside the mis-report. It **causes** it.

## Clarifications

### Session 2026-09-13

- Q: What counts as "backed by a declared requirement"? → A: **Any file
  waybill parses that states a *requirer → required* relationship.** For
  Go that means `go.mod` `require` entries (including `// indirect`)
  across every module in the scanned tree, honouring `replace`
  directives, plus `vendor/modules.txt` when the tree is vendored.
  Explicitly **not** `go.sum`: it is a hash list that names modules
  without stating who requires them, which is exactly why the fallback
  had nothing to attribute and filled the gap by inventing edges.

  Stated as a general rule rather than a Go carve-out so the same test
  applies if another ecosystem is ever found to synthesise edges.

  The same answer added a second requirement: the backing MUST be
  recorded in the emitted document, not merely checked at build time.
  Without it, "every edge is backed" is a claim a consumer has to take
  on trust, and the FR-009 gate becomes the only thing able to verify
  it — the property goes unverifiable the moment the document leaves the
  build. Recording the source also makes the warm-versus-cold difference
  legible in the document itself, which Story 3 needs anyway.
  (FR-002a, FR-002b, SC-003a.)

- Q: Does the no-unbacked-edges rule bind every ecosystem, or Go only?
  → A: **Universal as a principle; enforced for Go in this milestone;
  the other ecosystems are measured, not assumed.** FR-001 states the
  rule for the system, FR-013 scopes enforcement, and FR-014 makes the
  cross-ecosystem measurement a deliverable rather than a guess.

  Chosen over enforcing everywhere now because nothing outside Go has
  been measured. A grep found comparable fallback paths in the NuGet and
  Gradle readers, but both appear to read real manifests and would
  therefore be backed — appearance is not measurement, and committing to
  fix what may not be broken is how scope escapes.

- Q: What relationship does a component stranded by edge removal get?
  → A: **None.** It stays in the inventory and is marked unattached by
  the existing orphan machinery, which also drives the completeness
  reason code. No relationship is synthesised to the document root, so
  the graph continues to assert only what was read.

  Two consequences this milestone must handle rather than inherit: a
  consumer who traverses only the dependency graph will not see these
  components, so the unattached marking is what makes them findable; and
  `waybill:orphan-reason` today records how an edge was *derived*, not
  that a component is unattached — FR-007a keeps those two meanings
  separable.

## Why this matters

A consumer cannot re-derive a dependency graph; that is what they came
to the SBOM for. They decide whether to trust it for transitive analysis
by reading the document's completeness declaration. Today that
declaration is wrong in the most damaging direction — most confident
exactly where the graph is least true — and the edges underneath it
include relationships that do not exist in the project being scanned.

## Measured evidence

All figures measured against `main` at `09805cad` with a binary built
from that commit. Probes committed under `measurements/`.

**These numbers supersede those quoted in #857 and #829.** Both issues
were written before m860 (`359d2f44`, 2026-09-13) changed root anchoring
and main-module retention, and their figures — 490 edges, depth 1,
`flat` — no longer describe the product. The defect survived that change
in a different shape; see Corrections.

### Edges asserted but never read

`spf13/cobra` @ v1.9.1, whose `go.mod` declares exactly four direct
requires:

| | golang edges | backed by a declared require | synthetic stdlib | **unbacked** |
|---|---:|---:|---:|---:|
| warm module cache (control) | 5 | 4 | 1 | **0** |
| cold cache | 7 | 4 | 1 | **2** |

The two unbacked edges assert `cobra → blackfriday` and
`cobra → check.v1`. Neither appears in cobra's `go.mod`; both are
transitive (`blackfriday` via `go-md2man`, `check.v1` via `yaml.v3`).
With a warm cache they attach to their real parents instead.

`kubernetes/kubernetes` @ v1.37.0, cold cache, 39 `go.mod` files:

```
emitted golang edges                   2984
  backed by a declared require         2392
  synthetic stdlib node                  38
  NOT backed — asserted, never read     554   (18.6%)
```

Sampled and hand-verified: `k8s.io/api`'s own `go.mod` declares none of
`go-difflib`, `testify`, `go.yaml.in/yaml/v3`, `gopkg.in/yaml.v3` or
`k8s.io/streaming`, yet all five are emitted as its direct dependencies.
The probe counts `// indirect` entries as backed, so it is deliberately
generous; only edges with no manifest backing anywhere are counted.

### The invented edges cause the false declaration

Both targets emit, in the same document:

```
waybill:graph-completeness     = complete
waybill:go-transitive-coverage = unknown
```

kubernetes reports `reachable_count=494 total_count=494 orphan_count=0`.
Every component is reachable **because** the unbacked edges attach it.
Remove them and those components have no incoming edge — they become
exactly the orphans the existing machinery is built to detect.

So the reachability check is not merely a weak proxy here. It is
measuring a property the fabrication manufactures.

### `complete` remains earnable

The warm-cache control scan of cobra resolves the real module graph,
emits zero unbacked edges, and correctly reports
`graph-completeness = complete` with `go-transitive-coverage = complete`.
Whatever this feature changes must preserve that.

## Corrections to the issues as filed

Recorded because both issues will be read by whoever implements this.

- **#829's "flat, depth 1" shape is stale.** Post-m860 the graph is
  depth 2 (root → main module → dependencies). Flatness is no longer the
  detectable symptom; unbacked edges are.
- **#857's "72% of edges lost" does not generalise.** cobra emits seven
  non-root edges cold *and* warm — the same count. Cold does not lose
  them, it **misattributes** them. Edge loss at kubernetes scale is a
  separate consequence, not the defining one.
- **"Components are orphaned" is not what is happening.** Every
  component carrying `waybill:orphan-reason` is attached. That property
  is a provenance marker recording how an edge was derived, not a
  reachability claim, and it does not identify the unbacked edges: in
  cobra both unbacked edges point at marked components, but four marked
  components sit on perfectly good edges.

## Scope boundary established by measurement

#829 asks whether `cmake-nlohmann-json` and `uv-meilisearch-python`,
which also report `complete` on flat graphs, share this root cause.
They were scanned. **They do not.**

| | Go targets | cmake / uv targets |
|---|---|---|
| Go edges not backed by a manifest | 2 of 7; 554 of 2984 | n/a — these targets emit no Go edges |
| ecosystem coverage signal | `unknown` | none emitted |
| formats read | `go.mod`, `go.sum` | `Pipfile.lock`, scattered cmake/bazel declarations |
| topology available in the input | yes — the attempt failed | none; these formats carry no parent-child structure |

For the Go targets a resolution path existed, failed, was recorded as
failed, and the gap was filled with invented edges. For cmake/uv no
topology ever existed in the input. Different questions; only the first
is in scope.

**One limit on that claim.** The edge-truth probe parses `go.mod`, so it
can only judge Go edges. Whether the cmake and uv targets emit edges
unbacked by *their own* manifests has **not** been measured — what was
measured is that they emit no contradictory completeness declaration and
no ecosystem coverage signal. FR-014 covers closing that gap.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every emitted edge is one waybill actually read (Priority: P1)

Someone consumes a waybill SBOM for a Go service and does blast-radius
or transitive-vulnerability analysis. Every dependency relationship they
traverse must be one that exists in the scanned project — and they must
be able to check that for themselves, from the document alone, rather
than taking it on trust.

**Why this priority**: This is the root defect, it is a correctness
violation rather than a reporting weakness, and — per the evidence above
— removing the invented edges is what makes the completeness signal
correct as a consequence. It is both the most serious problem and the
enabling fix for Story 2.

**Independent Test**: Scan a Go project with no reachable module cache
and confirm every emitted dependency relationship is declared in some
manifest in the scanned tree. Requires no completeness change.

**Acceptance Scenarios**:

1. **Given** a Go project scanned with no reachable module cache,
   **When** every emitted dependency relationship is checked against the
   manifests in the scanned tree, **Then** each one is backed by a
   declared requirement.
2. **Given** a dependency whose parent cannot be determined, **When** the
   SBOM is emitted, **Then** no relationship is asserted for it rather
   than a plausible one being chosen.
3. **Given** the same project scanned with a warm cache, **When** the two
   documents are compared, **Then** the cold scan asserts no relationship
   absent from the warm scan.
4. **Given** any emitted dependency relationship, **When** a consumer
   reads its provenance, **Then** it names a declaring source they can
   locate in the scanned tree without re-running the scan.
5. **Given** a document emitted in each supported format, **When** the
   provenance of the same relationship is compared across them, **Then**
   each format carries it — through that format's native construct
   wherever one exists.

---

### User Story 2 - The completeness declaration can be trusted (Priority: P1)

Someone decides, from the document's own completeness declaration,
whether its graph supports transitive analysis.

**Why this priority**: Equal-first with Story 1, not below it. Story 1
without Story 2 produces a graph that is honest edge-by-edge while the
document still declares itself complete — arguably worse, because the
missing edges are now genuinely missing *and* still unreported. The two
ship together.

**Independent Test**: Scan a Go project with no reachable module cache
and confirm the document does not declare itself complete and states why.

**Acceptance Scenarios**:

1. **Given** a Go project whose module graph could not be resolved,
   **When** the SBOM is emitted, **Then** it does not declare itself
   complete, and names a reason a consumer can act on.
2. **Given** any document declaring itself complete, **When** its
   per-ecosystem coverage signals are inspected, **Then** none reports
   `unknown`.
3. **Given** a Go project scanned with a warm cache resolving the full
   module graph, **When** the SBOM is emitted, **Then** it still declares
   itself complete — a correct resolution must not be reported as
   degraded.
4. **Given** a document that declares itself complete, **When** its
   components are checked for reachability, **Then** the declaration
   reflects whether the emitted relationships are trustworthy, not only
   whether every component happens to be attached to something.

---

### User Story 3 - The degradation is diagnosable (Priority: P2)

Someone gets a degraded graph in CI and needs to know why and what to
change.

**Why this priority**: Below the correctness stories because a consumer
who knows not to trust the graph is already protected. This is about
letting them recover the graph they wanted. The remedy is real — the
warm-cache control demonstrates it — so pointing at it is actionable
rather than theoretical.

**Independent Test**: Scan with no reachable module cache and confirm the
document alone explains the cause and remedy.

**Acceptance Scenarios**:

1. **Given** a scan whose module graph could not be resolved, **When** a
   consumer reads the document alone, **Then** they can determine that
   transitive resolution did not succeed, and why.
2. **Given** such a scan, **When** an operator reads the diagnostic,
   **Then** it identifies the condition they can change rather than only
   the symptom.

---

### User Story 4 - This cannot silently return (Priority: P3)

A maintainer changes resolution, emission or classification and needs to
be told if invented edges or a false declaration have reappeared.

**Why this priority**: Prevention. Last because it delivers nothing to a
consumer directly — but this defect reached committed goldens and stayed
there, and survived m860 by changing shape while every check kept
passing.

**Independent Test**: Reintroduce the defect; confirm the project fails.

**Acceptance Scenarios**:

1. **Given** an emitted document containing a dependency relationship not
   backed by any manifest in the scanned tree, **When** the project's
   checks run, **Then** they fail and name the relationship.
2. **Given** a document declaring itself complete while an ecosystem
   coverage signal reports `unknown`, **When** the project's checks run,
   **Then** they fail.

### Edge Cases

- **A genuinely direct-only project.** A module with no transitive
  structure is legitimately shallow and must still be able to earn
  `complete`. Shape alone must never be the test — this is what
  separates the Go targets from the cmake/uv ones.
- **Partial resolution.** Some modules resolve, others do not. The
  declaration must reflect the unresolved remainder rather than the
  majority case.
- **Multi-module workspaces.** A requirement may be declared in a
  sibling module's manifest, and `replace` directives redirect module
  paths. A requirement declared anywhere in the scanned tree that
  legitimately backs an edge must not be treated as fabricated.
- **Test-only and indirect requires.** A `// indirect` entry is still a
  relationship read from a manifest, not an invention.
- **An empty graph.** Nothing to reach, nothing to invent; trivially
  complete.
- **Removing edges strands components.** Expected: those components
  become genuinely unreachable and must be reported as such rather than
  dropped. A consumer traversing only the dependency graph will not
  encounter them at all — which is correct, since nothing in the scanned
  tree says they belong anywhere, but it means the unattached marking is
  the only thing making them findable.
- **A component with genuinely no dependencies** looks identical to an
  unattributable one if both simply lack relationships. They must stay
  distinguishable (FR-007a).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every emitted dependency relationship MUST truthfully
  state where it came from. A relationship whose recorded provenance
  names a source that does not contain it MUST NOT be emitted. Where a
  parent cannot be determined from any source, no relationship is
  asserted.

  *This is the corrected form.* The requirement originally read "must be
  backed by a declared requirement in the scanned tree", which is too
  narrow: it would forbid the deps.dev dep-graph enrichment
  (`enrich/deps_dev_graph.rs`), whose edges are legitimately sourced
  from outside the tree and say so. The defect being fixed was never
  "an edge from outside the tree" — it was an edge claiming `go.mod` as
  its source when `go.mod` does not contain it. Truthful provenance is
  the invariant; locality is not.
- **FR-002**: A **declared requirement** is a statement, in a file
  waybill parses, that one named component requires another. A file that
  merely enumerates components without attributing them to a requirer
  does not qualify. For Go this means `go.mod` `require` entries —
  including those marked indirect — from any module in the scanned tree,
  with `replace` directives honoured, plus `vendor/modules.txt` when the
  tree is vendored; and it does **not** include `go.sum`.
- **FR-002a**: Each emitted dependency relationship MUST carry
  provenance identifying the declaring source that backs it, sufficient
  for a consumer to locate that declaration in the scanned tree without
  re-running the scan.
- **FR-002b**: The provenance in FR-002a MUST be expressed through a
  construct native to each target format where one exists, and only
  through a waybill-specific extension where none does (Constitution
  Principle V). Any catalogue row added for it requires a matching
  extractor in the same change.

  **Status at close.** CycloneDX uses native `compositions[].aggregate`
  (#871, this milestone). SPDX 2.3 has no native construct and keeps the
  annotation bridge, which is final rather than interim. SPDX 3.0.1 has
  a native `Relationship.completeness` field that this milestone does
  **not** adopt — deferred to #878 because the signal is computed after
  the relationships are built (`v3_document.rs:777` vs `:685`) and the
  reorder risks byte-identity goldens. No information is lost: SPDX 3
  carries the same facts via `waybill:graph-completeness` and
  `waybill:orphan-reason`.
- **FR-015**: The system MUST NOT assert, in any standards-native
  completeness construct, that a component's dependency graph is
  complete unless that component's outgoing edges were actually
  resolved. Enumerating components is a separate claim from resolving
  their edges and the two MUST NOT share a list. (#871)
- **FR-016**: Components that are real dependencies whose position in
  the graph could not be determined MUST be declared as such, in every
  emitted format, through that format's own vocabulary for partial
  knowledge where one exists and through the annotation bridge where
  none does. In CycloneDX that is `compositions[].aggregate` —
  `unknown` / `incomplete` — which removes the need for a
  waybill-specific extension there. SPDX 2.3 has no such construct and
  uses annotations. SPDX 3's native `Relationship.completeness` is
  identified but deferred to #878; the annotation carries it meanwhile,
  so the requirement is satisfied in substance in all three formats.
- **FR-003**: The system MUST NOT declare a graph complete when any
  per-ecosystem coverage signal in the same document reports that its
  coverage is unknown or partial.
- **FR-004**: The completeness determination MUST reflect whether the
  emitted relationships are trustworthy, not solely whether every
  component is attached to something.
- **FR-005**: When the system declines to declare a graph complete, it
  MUST state a reason from the documented vocabulary, distinguishing
  "the input carried no topology" from "topology existed but could not
  be resolved".
- **FR-006**: The system MUST remain able to declare a well-connected,
  fully-resolved graph complete.
- **FR-007**: Components left without any relationship as a consequence
  of FR-001 MUST remain present in the document's inventory and be
  reported as unattached. The system MUST NOT synthesise a relationship
  to the document root, or to anything else, to make them reachable.
- **FR-007a**: A component whose parent could not be determined MUST be
  distinguishable from one genuinely known to have no dependencies, and
  from one whose edge was merely derived by a lower-confidence
  resolution tier. These are three different states and MUST NOT
  collapse into one marking.
- **FR-008**: When transitive resolution degrades the emitted graph, the
  cause and the remedy MUST be determinable from the emitted document
  alone, without the consumer knowing how the scan was invoked.
- **FR-009**: The project MUST detect, at build time, both a
  relationship whose stated provenance does not contain it and a
  document declaring itself complete while contradicting that claim
  elsewhere. The check MUST evaluate each relationship against the
  source it actually names, NOT against the scanned tree
  unconditionally — enrichment-sourced edges are correct and a
  tree-only check would fail them.
- **FR-017**: Relationships sourced from outside the scanned tree
  (currently the deps.dev dep-graph enrichment, Maven-only and
  on by default) MUST remain distinguishable from locally-observed
  ones, and MUST NOT override locally-observed facts. The existing
  design already satisfies this — deps.dev supplies topology while the
  local scan supplies versions, and coords never seen on disk are
  marked `declared-not-cached` — so this requirement pins behaviour
  rather than changing it.
- **FR-010**: The project MUST measure emitted graph structure
  independently of the document's own self-report, so a defect in the
  reporting cannot conceal itself.
- **FR-011**: Committed expectations that encode the current edge counts
  MUST be re-authored. Edge counts are expected to **fall** — unbacked
  edges are being removed — and that reduction MUST NOT be recorded as a
  regression.
- **FR-012**: The system MUST behave identically whether a module cache
  is absent, empty, or unreachable.
- **FR-013**: FR-001 states a rule for the system as a whole. Enforcement
  and build-time gating in this milestone are scoped to Go. Narrowing
  the rule itself to Go is explicitly rejected — a second ecosystem found
  to synthesise edges is a violation of FR-001, not a new principle.
- **FR-014**: This milestone MUST determine, by measurement rather than
  inspection, whether any non-Go ecosystem also emits relationships
  unbacked by a declared requirement. The result MUST be recorded
  whichever way it comes out, and any ecosystem found to violate FR-001
  MUST get its own tracked issue rather than being absorbed here.

### Key Entities

- **Dependency relationship**: An assertion that one component directly
  depends on another. Consumed for traversal. Must correspond to
  something read from a manifest.
- **Completeness declaration**: The document-scope statement of how far
  the graph can be trusted.
- **Per-ecosystem coverage signal**: An ecosystem-specific statement of
  whether transitive resolution succeeded. Already emitted for Go and
  already correct; not currently consulted by the completeness
  declaration.
- **Declared requirement**: A statement, in a file waybill parses, that
  one named component requires another — the sole legitimate basis for a
  relationship. A file that enumerates components without attributing
  them to a requirer (Go's `go.sum`) does not qualify.
- **Edge provenance**: The record, carried on an emitted relationship, of
  which declaring source backed it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Zero emitted dependency relationships name a source that
  does not contain them, on both Go corpus targets. Baseline: 2 of 7
  (`go-cobra`) and 554 of 2984 (`go-kubernetes`) claimed `go.mod` while
  absent from it.
- **SC-011**: Maven dep-graph enrichment continues to emit its edges
  unchanged. Baseline measured on the `transitive_parity/maven` fixture
  with an empty `~/.m2`: 9 components and 9 `dependsOn` edges without
  enrichment, 32 components and 55 `dependsOn` edges with it
  (`new_components=23 new_edges=50 queries_err=0`; the scan log's
  `relationships=8`/`58` counts all relationship types, not just
  `dependsOn`). The maven reader alone produces ZERO *transitive* edges
  in that state — the parity baseline is `EXPECTED_WAYBILL_EDGE_COUNT =
  0` — so this is the difference between having a transitive graph and
  having none.
- **SC-002**: Zero documents declare themselves complete while carrying
  a per-ecosystem coverage signal of `unknown`. Baseline: both Go corpus
  targets do.
- **SC-003**: A cold-cache scan asserts no relationship that a
  warm-cache scan of the same commit does not also assert. Baseline:
  `go-cobra` asserts 2 such relationships today. Overlaps SC-001 but is
  not implied by it: an edge can be backed by a manifest (satisfying
  SC-001) and still be attributed to the wrong parent, which only a
  comparison against the resolved graph reveals.
- **SC-003a**: Every emitted dependency relationship carries provenance
  naming its declaring source, and following that provenance locates the
  declaration in the scanned tree. Verified by resolving the provenance
  for a sample rather than by its mere presence. Baseline: no
  relationship carries edge provenance today.
- **SC-004**: A warm-cache scan that fully resolves the module graph
  still declares itself complete. Baseline: true today and MUST remain
  true — this is the criterion that prevents "fix" by never emitting
  `complete` again.
- **SC-005**: Components stranded by the removal of unbacked edges are
  still present in the inventory and reported as unattached, with no
  relationship synthesised to reach them. Verified by component count
  before and after (no component disappears) AND by confirming no new
  root-originating relationship replaced the removed edge. Baseline:
  `go-cobra` strands 2 components (`blackfriday`, `check.v1`).
- **SC-006**: Reintroducing either defect — an unbacked relationship, or
  `complete` alongside `unknown` coverage — causes the project's checks
  to fail. Verified by reverting the fix and observing failure, not by
  inspection.
- **SC-006a**: Every ecosystem represented in the corpus has been
  checked for unbacked relationships against its own manifest formats,
  and the outcome recorded. Baseline: only Go has been checked; the
  result for every other ecosystem is currently unknown, not clean.
- **SC-007**: No target whose source format carries no topology is
  reported as defective. Baseline: `cmake-nlohmann-json` and
  `uv-meilisearch-python`, measured as emitting no unbacked edges.
- **SC-009**: No document asserts `aggregate: complete` with a
  `dependencies` list for an ecosystem whose components are not all
  reachable. Baseline: `go-cobra` lists all 8 components there today
  while 7 have no edges, in the same document that reports `partial`.
- **SC-010**: Components whose graph position is unresolved appear
  under a partial-knowledge aggregate rather than being dropped or
  silently included. Baseline: no such record is emitted today.
- **SC-008**: An operator comparing documents from a host with and
  without a module cache can determine from the documents alone that the
  scans differed and why.

## Assumptions

- **Edge counts will fall and that is the intended outcome.** The
  quality-corpus bounds for the Go targets are authored around current
  counts and will need re-authoring downward. Per the lesson recorded on
  #830, they must be re-measured with `$GOMODCACHE`, `$GOPATH` and
  `$HOME` all isolated — bounds authored on a developer machine with a
  populated cache describe edges no clean runner can resolve.
- **Public-corpus goldens will change** for at least `go-cobra` and
  `pants-example-golang`, which are the two committed goldens carrying
  the defect.
- **The cmake and uv cases are out of scope.** Measured and shown to be
  a different question.
- **The per-ecosystem coverage signal is correct.** It already reports
  accurately; the defect is that the completeness declaration ignores it.
- **Scanning without a module cache is a first-class case**, not a
  degraded one. It is how CI and container scanning work.
- **No network access is introduced *by the product*.** Offline scanning
  stays offline. This does not bind the test harness: the warm-cache
  control that several success criteria depend on requires `go mod
  download` and therefore network plus a Go toolchain. Those tests
  belong in an opt-in lane, never the default `cargo test --workspace`
  pair.
- **Existing published SBOMs are not rewritten.** This changes what
  future scans emit.
