# SBOM Invariant Catalog

**Version**: v1 | **Canonical home**: this file (`docs/reference/sbom-invariants.md`).

An **invariant** is a property expected of *any* SBOM waybill emits. This
file is the human-readable source of truth for the set; the machine side
is `REGISTRY` in
`waybill-cli/tests/corpus_harness_195/invariants/mod.rs`. A hermetic
`cargo test` gate asserts that neither side holds a row the other lacks,
mirroring `every_catalog_row_has_an_extractor` for the format-mapping
catalog (`waybill-cli/src/parity/extractors/mod.rs:922`).

The distinction this catalog rests on:

| Term | Means | Lives in |
|---|---|---|
| **invariant** | a property of *any* emitted SBOM | this catalog + `invariants/`, and `layer0_document_integrity` |
| **expectation** | a claim about *one specific* corpus target | `corpus_harness_195/expectations.rs` |

A check that mentions a particular target's package, version or verdict
is an expectation. A check that would be equally true of a document from
any target is an invariant and belongs here.

For the per-annotation wire shapes these invariants read, see
[`sbom-format-mapping.md`](sbom-format-mapping.md).

## Numbering rules

1. **Append-only.** A new invariant takes `max(n) + 1`. Numbers are never reassigned when rows are added, removed or reordered.
2. **Retire, don't edit.** Refining a statement's wording keeps the number. Changing *what the row means* retires it and allocates a new one. The test of "changed meaning" is whether an existing declaration against it would now be accepting a different defect.
3. **Never reuse.** A retired number stays in this table, marked `Retired`, with a pointer to its successor if it has one. It may never be reallocated.
4. **Retired rows have no checker.** The registry must not contain an entry for a retired number.

## The catalog

| ID | Statement | Precondition | Kinds | Status |
|---|---|---|---|---|
| I1 | No dependency edge targets a file-tier component | document contains at least one file-tier component | `file-tier-dependency-edge` | Active |
| I2 | `(name, version, type)` identifies at most one component | at least two components | `duplicate-library-identity`, `duplicate-file-identity`, `duplicate-application-identity`, `duplicate-other-identity` | Active |
| I3 | A component reachable from the subject only through non-runtime edges carries a non-runtime marker | the document declares at least one non-runtime dependency edge | `unmarked-non-runtime-reachable` | Active |
| I4 | CycloneDX and SPDX 2.3 agree on the subject's direct dependency set | both documents emitted, each with a resolvable subject | `root-direct-only-in-cdx`, `root-direct-only-in-spdx` | Active |
| I5 | A `complete` graph-completeness verdict implies nothing was excluded from the judgement | document-scope completeness verdict is `complete` | `complete-with-unreachable` | Active |
| I6 | A package does not appear under several different unknown-version spellings | at least one component carries an unknown-version sentinel | `mixed-unknown-version-spelling` | Active |
| I7 | A component not reachable from the subject by any dependency edge carries a marker explaining why | subject resolvable and at least one non-subject component | `unreachable-and-unexplained` | Active |
| I8 | A component with no version carries an annotation explaining why it has none | at least one component has an absent or empty version | `versionless-and-unexplained` | Active |

### I1

**Catches**: a file being described as something another component
*depends on*. A file is contained in a component, not required by it;
`dependsOn` is the wrong edge type and a consumer walking the graph for
a vulnerability-relevant dependency set will pick up maintainer scripts
and shell snippets.

**When it fires**: find the offending component's tier annotation
(`waybill:component-tier` = `file`) and the pass that attached the edge.
The usual source is orphan-anchoring (milestone 194 US1), which anchors
unreachable components to the subject with a plain `dependsOn` rather
than a containment relationship. Retyping it is a parity-model change
across all three formats — see #1206.

### I2

**Catches**: the same package emitted twice under one identity, which
double-counts it in any consumer that aggregates by name and version,
and makes a "how many dependencies" answer wrong.

**When it fires**: the kind tells you which population duplicated, and
that determines the cause. `duplicate-file-identity` is usually two
readers claiming the same path; `duplicate-library-identity` is usually
a reconciler miss, where two manifest sources produced different PURLs
for one package and the milestone-191 pass did not merge them.

The kinds split on component type deliberately. A duplicated file and a
duplicated library are different defects with different fixes, and one
kind covering both would let a declaration accepting the first silently
absorb the second — which is the whole point of pinning a kind
(FR-012). `duplicate-other-identity` exists so a component type the
corpus has never emitted cannot make a checker produce a kind this row
does not list; today the corpus emits only `library`, `file` and
`application`.

### I3

**Catches**: a development, test, build or optional dependency that
reads as a runtime one. A consumer filtering to runtime components gets
a component that is not in the shipped artifact, which inflates the
attack surface it believes it has.

**When it fires**: walk back from the component to the subject and find
the edge whose scope was not propagated. Scope propagation is milestone
052's `apply_lifecycle_scope_to_edges` plus milestones 179-184 per
ecosystem; a gap is usually one ecosystem's classifier not having been
extended.

**Boundary with I7**: this invariant is about components that *are*
reachable, by a path that is entirely non-runtime. I7 is about
components not reachable at all. Every component is one or the other, so
a defect satisfies exactly one of the two preconditions — which was not
true of the first draft of these rows, and is why the gem and Go
instances of one defect class landed under the wrong number during the
investigation (FR-019).

### I4

**Catches**: the two formats disagreeing about what the thing being
described directly depends on. Whichever one a consumer reads, they are
getting a different answer to the same question, and only one can be
right.

**When it fires**: compare the two direct sets by PURL. **Before
concluding there is a defect, confirm the SPDX side was read
correctly**: SPDX 2.3 expresses non-runtime dependencies with *inverse*
relationship types — `DEV_DEPENDENCY_OF`, `OPTIONAL_DEPENDENCY_OF`,
`TEST_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`, `PROVIDED_DEPENDENCY_OF` —
which place the subject in `relatedSpdxElement`, not `spdxElementId`.
Treating only `DEPENDS_ON` as an edge fabricates a mismatch on every
target that has any scoped dependency; `maven-guice` alone carries 35
such relationships against 28 `DEPENDS_ON`.

### I5

**Catches**: a completeness verdict that contradicts the document it
describes. `complete` is the strongest claim waybill makes about a
graph; if the same document also contains components unreachable from
the subject, the verdict is overclaiming and a consumer who trusts it
will not go looking for the gap.

**When it fires**: the verdict is computed by milestone 158's
graph-completeness pass. Either the pass is not counting a population it
should, or a later pass added components after the verdict was fixed.
Check the ordering before changing the threshold.

### I6

**Catches**: one package appearing under two spellings of "we could not
resolve a version" — absent, empty, `unknown`, `v0.0.0-unknown` — so it
is counted twice and neither entry is matchable against an advisory
feed.

**When it fires**: pick one spelling per ecosystem and make the readers
agree. Note what is *not* a sentinel: `v0.0.0-20260928230214-8a89bd…`
is a legitimate Go pseudo-version, and `0.0.0.0` is a legitimate .NET
version. A substring match on `0.0.0` flags roughly forty innocent
components across the corpus — the check compares against an exact
sentinel set for that reason.

### I7

**Catches**: a component in the document that nothing explains. It is
neither reachable from the subject nor annotated with a reason, so a
consumer cannot tell whether it is a real part of the artifact or a
reader artefact.

**When it fires**: either the component should have an edge and a
resolver missed it, or it genuinely has no edge and needs a
`waybill:orphan-reason`. Milestone 167 defined the reason vocabulary and
milestone 194 anchored several populations; a new unexplained population
is usually a reader that was added after those passes.

**Boundary with I3**: see I3 above. I7's population is the unreachable
one; I3's is reachable-but-only-non-runtime.

### I8

**Catches**: a component whose `version` is absent or empty with nothing
saying why. Two very different situations produce an empty version field
— the component has no version to have (a file, a synthetic resolve
anchor), or waybill looked for one and failed — and the field alone
cannot distinguish them. The first is complete information; the second
is a gap a consumer should act on.

**When it fires**: identify which of the two populations it is. If the
component is inherently versionless, it needs a marker that says so —
the existing ones are `waybill:component-tier` = `file`,
`waybill:closure-role` for a Nix store path, and for the Haskell reader
`waybill:haskell-version-unresolved-reason`. If resolution was attempted
and failed, the annotation is `waybill:unresolved-reason` (C151,
milestone 236), which exists precisely for this.

Note that `waybill:component-kind` = `lockfile-resolve` is **not**
accepted as an explanation, though a Pants resolve anchor is inherently
versionless. `component-kind` (milestone 1064) describes resolve
ownership, not version absence, and reading it as an explanation would
silence the population this invariant reports while leaving a consumer
unable to tell an anchor from a failed resolution. The fix is a marker
that states the anchor is versionless, not a wider accepted set here.

**Boundary with I6**: I6's population is components carrying an
unknown-version *sentinel* (`unknown`, `v0.0.0-unknown`, `NOASSERTION`),
and it asks whether one package appears under several spellings of
unknown. I8's population is the narrower absent-or-empty case and it
asks whether the absence is accounted for at all. A sentinel is a
statement on the wire — a poor one, but a statement; an empty field says
nothing. A document can pass either and fail the other.
