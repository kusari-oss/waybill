### I9

**Catches**: a component the pinned revision's compiler supplies carrying
`optional` or `excluded`. Milestone 1276 (#1340).

The claim is a conformance one rather than a preference. CycloneDX 1.6
defines `optional` as "not capable of being called due to them **not
being installed** or otherwise accessible by any means", and instructs
that a component which IS installed but is prohibited from being called
"must be scoped as `required`"; `excluded` as "not reachable within a
call graph at runtime". A GHC boot library is installed in every build —
the compiler ships it, whatever the flags say — and is in the artifact's
call graph, so both values are false of it by the schema's own text. A
consumer filtering on scope to learn what is in the artifact would
wrongly drop it.

**Boundary with I3**: these two partition. I3 asserts that a component
reachable only through non-runtime edges carries a non-runtime marker
*unless the compiler supplies it*; this row asserts that one the compiler
supplies carries no such marker at all. Every component satisfies exactly
one of the two preconditions. Both identify the class from the same
signal — C170 `waybill:haskell-version-unresolved-reason` =
`compiler-supplied` — and they must continue to, or the pair becomes
locally satisfiable while jointly incoherent.

**When it fires**: find how the scope was assigned. If it was propagated
transitively, the propagation's compiler-supplied immunity has stopped
working — that is the population milestone 1276 fixed, measured at 10
components across two targets, all of them `transitive`. If it was
assigned directly by a reader, this is the origin milestone 1276
deliberately did not build a suppression path for, on the grounds that
its population was zero; the fix is to decide where the mark should be
suppressed rather than to relax this row.

**What it does not catch**: a component that is runtime-present for some
other reason — an OS package in a rootfs scan, a language stdlib
collapsed by the file tier. Measured at the time this row was added,
those classes carry no lifecycle scope at all (142 and 175 components,
every one scope-free), so they are not in this row's population. If a
future feature gives them scopes, the same reasoning applies and this
note is where to start.

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
| I3 | A component reachable from the subject only through non-runtime edges carries a non-runtime marker, unless the compiler supplies it | the document declares at least one non-runtime dependency edge | `unmarked-non-runtime-reachable` | Active |
| I4 | CycloneDX and SPDX 2.3 agree on the subject's direct dependency set | both documents emitted, each with a resolvable subject | `root-direct-only-in-cdx`, `root-direct-only-in-spdx` | Active |
| I5 | A `complete` graph-completeness verdict implies nothing was excluded from the judgement | document-scope completeness verdict is `complete` | `complete-with-unreachable` | Active |
| I6 | A package does not appear under several different unknown-version spellings | at least one component carries an unknown-version sentinel | `mixed-unknown-version-spelling` | Active |
| I7 | A component not reachable from the subject by any dependency edge carries a marker explaining why | subject resolvable and at least one non-subject component | `unreachable-and-unexplained` | Active |
| I8 | A component with no version carries an annotation explaining why it has none | at least one component has an absent or empty version | `versionless-and-unexplained` | Active |
| I9 | A component the compiler supplies carries no non-runtime scope | at least one component is marked compiler-supplied | `compiler-supplied-non-runtime` | Active |
| I10 | A closure member's non-runtime marker agrees with the evidence that decided it, or the member says it was exempted | at least one component carries a closure role | `closure-tooling-without-non-runtime-marker`, `closure-statically-linked-but-marked` | Active |

### I10

**Catches**: a closure member nix referenced only through a tooling field,
or through no dependency field at all, carrying no marker that says it does
not ship. Milestone 1277 (#1342).

The claim is a conformance one, and it is the mirror of I9's. CycloneDX 1.6
states that "if scope is not specified, `required` scope SHOULD be assumed
by the consumer of the BOM", and defines `required` as "The component is
required for runtime". A derivation reached only through
`nativeBuildInputs`, `depsBuildBuild`, `depsBuildHost` or
`nativeCheckInputs` builds the artifact and is not in it, so `required` —
which is what an absent scope means — is false of it. I9: a component that
**is** in the artifact scoped as absent. This: a component that is **not**
in the artifact reading as present.

**The population** is the components carrying C182
`waybill:closure-role` with a value of `build-tooling` or `unreferenced`.
`artifact-input` and `both` are deliberately excluded: `ARTIFACT_FIELDS` is
`buildInputs`, `propagatedBuildInputs` and `depsHostHost`, and nixpkgs'
Haskell builder passes test dependencies through `buildInputs`, so the role
does not establish runtime presence either way. Measured, `hspec` is
`artifact-input` and does not ship. The row reports n/a where no such role
exists, which is every target that does not run `--nix-closure` — 16 of 17
on the corpus.

**The exemption, and why it is read from C203.** A package the pinned
revision's compiler supplies is installed in every build, so milestone 1277
asserts nothing about it even in a tooling role, and I9 asserts the same
thing from the other side. This row honours that by accepting a C203
`waybill:scope-classification-source` of `exempt-compiler-supplied`.

It does **not** reuse I9's own signal, and the reason is worth recording:
I9 identifies compiler-supplied components by C170
`waybill:haskell-version-unresolved-reason == "compiler-supplied"`, which
milestone 1275 sets on the *synthetic* components it emits for declared
edges to boot libraries the closure does not contain. A closure member that
happens to be a boot library carries no such property — measured, the exempt
member in milestone 1277's fixture carries only `waybill:closure-identity`
and `waybill:closure-role`. An invariant reusing C170 would therefore not
recognise the exemption and would report correct output as a defect.

That discovery is why the exemption is recorded in the document at all.
Milestone 1277's FR-005a was amended during implementation: with an
exemption collapsed into absence, a component the rule deliberately left
alone and a component the rule failed to reach are the same document, and
neither a reader nor this row can tell them apart. Milestone 1276 learned
the same thing at document scope, which is why C202 has three states.

**Boundary with I3 and I9.** The three partition. I3: reachable only
through non-runtime edges → carries a non-runtime marker, unless
compiler-supplied. I9: compiler-supplied → carries no such marker. I10:
non-shipping closure role → carries one, unless compiler-supplied. The
exemption is the seam where all three meet, which is why each states it.
I3's own entry records getting a boundary wrong once, when a loose one put
two instances of a single defect class under the wrong number.

**Milestone 1423 (#1423): the evidence, not the role.** With the runtime
closure available, a member's mark comes from that closure plus static
linking rather than from its role, so the role alone no longer says what
the mark must be: openssh's `dns-root-data` is `build-tooling` and ships,
and curl's libxml2 is `artifact-input` and does not. C203 says how each
member was decided, and the row reads it:

- **Population**: every component carrying C182, in any role.
- `exempt-compiler-supplied`, `exempt-evaluated-package`: pass.
- `runtime-closure`, `reachability-walk`: pass, marked or not. The native
  mark says which way the evidence went, and nothing else in the document
  contradicts it.
- `statically-linked`: must be **unmarked**; a mark is
  `closure-statically-linked-but-marked` (the converse check).
- `closure-role`, `closure-role+lifecycle-propagation`, or no C203 at all:
  the milestone-1277 rule above, unchanged, so a pre-1423 document still
  passes. An unrecognised C203 value falls here too, as the stricter
  reading.

The row reports n/a only where no component carries a closure role at all.

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

**The vocabulary spans every ecosystem since feature 1218.** Until then
`generate/orphan_reason.rs` skipped anything that was not Go or npm, so an
unreachable maven, pypi, bazel, github or generic component could not be
explained even in principle — there was no code to assign. Feature 1218
removed both guards and added `build-toolchain`, `foreign-build-system` and
`unreferenced-lock-node` to C45 (#1214).

**Two populations deliberately still fail I7**, and their declarations are
the record of it rather than an oversight. Both are waybill's own defects,
so a marker on them would assert a property of the scanned repository that
is really a property of waybill, and would close the cell reporting it:

- `python-flask` (13) — `requirements.txt` components are emitted with no
  out-edges, so a declaring module's transitive closure dangles. The edges
  are in the lockfile; the emitter never wrote them. **#1234.**
- `maven-guice` (3) — the maven dual-key insert emits both the declared and
  the workspace-resolved coordinate and wires only the resolved one, so an
  older coordinate sits unreferenced beside the version the build uses.
  **#1235.**

So a fresh I7 failure is worth triaging in two directions before reaching
for vocabulary: *is this a class nothing can describe yet*, or *is this an
edge a reader should have written*? Feature 1218's measurements
(`specs/1218-explain-every-orphan/measurements/i7-populations.md`) found the
split was 18 to 16 on a population that had been read as entirely the
first.

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
component is inherently versionless, it needs a marker that says so. The
general one is `waybill:version-absence` = `by-construction` (C197,
milestone 1156), meaning there was never a version to find; the narrower
ones are `waybill:component-tier` = `file`, `waybill:closure-role` for a
Nix store path, and for the Haskell reader
`waybill:haskell-version-unresolved-reason`. If resolution was attempted
and failed, the annotation is `waybill:unresolved-reason` (C151,
milestone 236), which exists precisely for that. C197 and C151 are
mutually exclusive on one component: they are the two halves of the
question this invariant asks.

Acceptance of C197 is keyed to the **pinned value**, not to the key's
presence. A value this build does not recognise is not an explanation, so
a later enum member cannot silence a cell on a checker that cannot read
it.

Note that `waybill:component-kind` = `lockfile-resolve` is still **not**
accepted as an explanation, even though it is co-present on exactly the
components C197 now explains. `component-kind` (milestone 1064) names
what KIND of thing a component is and reserves enum extension, so
binding "has no version" to its presence would make every future kind
inherit a promise its name does not make — and a consumer reading only
`component-kind` would still be unable to tell an anchor from a failed
resolution.

The four Pants cells milestone 1156 closed are held by a named-target
positive control rather than by a declaration, because `Pass` and
`NotApplicable` both pass the lane and only the first is what the fix
achieved. A cell going `NotApplicable` is invisible to
`unfalsifiable_invariants` whenever the invariant stays falsifiable on
some other target, which I8 does via `image-postgres16`. The general fix
for that — a committed registry of cells whose `Pass` is load-bearing —
is deferred to its own spec; until it lands, these four are the only
cells protected, and by a test that names them.

**Boundary with I6**: I6's population is components carrying an
unknown-version *sentinel* (`unknown`, `v0.0.0-unknown`, `NOASSERTION`),
and it asks whether one package appears under several spellings of
unknown. I8's population is the narrower absent-or-empty case and it
asks whether the absence is accounted for at all. A sentinel is a
statement on the wire — a poor one, but a statement; an empty field says
nothing. A document can pass either and fail the other.
