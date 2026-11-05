# Phase 0 Research: 1279-closure-dep-field-coverage

Every figure is measured against a named closure on a named system, or it
says it is not. The system label is load-bearing here: the darwin and linux
closures of the same project disagree by a factor of five on one of this
milestone's two populations, and a figure quoted without its system is the
mistake #1348's own research made.

---

## R1 — What do the four newly-consulted attributes actually do? **Measured.**

The spec declined to predict this (SC-004, SC-011) because it is the
milestone's output. Measured with `measurements/role-delta-probe.py`, which
reimplements the classifier twice and differs only in the field set:

### x86_64-linux — **the closure the corpus goldens pin**

| role | before | after | delta |
|---|---|---|---|
| artifact-input | 270 | 268 | **−2** |
| build-tooling | 118 | 121 | +3 |
| both | 42 | 45 | +3 |
| unreferenced | 1,319 | 1,315 | −4 |

7 derivations change role, all 7 named: 3 `unreferenced`→`build-tooling`,
3 `artifact-input`→`both`, 1 `unreferenced`→`artifact-input`.

**`artifact-input` goes down, not up**, which is worth stating because it
looks wrong: three of its members gain a tooling reference and become `both`,
while only one arrives. A plan that assumed every delta was positive would
have mis-specified SC-011.

### aarch64-darwin, for contrast

| role | before | after | delta |
|---|---|---|---|
| artifact-input | 294 | 298 | +4 |
| build-tooling | 170 | 172 | +2 |
| both | 62 | 63 | +1 |
| unreferenced | 749 | 742 | −7 |

8 derivations, 6 identities: `compiler-rt`, `compiler-rt-libc`, `libcxx`,
`nss-cacert`, `xcbuild`, `find-xml-catalogs-hook`. `libcxx` and `compiler-rt`
are C++ runtime libraries that genuinely ship, so on this system the
correction is squarely the one #1346 describes.

**Decision**: the plan's success criteria are stated against **x86_64-linux**,
because that is what the goldens pin and therefore what any assertion will be
checked against. The darwin figures are kept as the contrast that shows the
populations are not system-stable.

---

## R2 — What changes in emitted output? **One false exclusion, measured.**

Joined to the committed golden **on the matching system** — the linux closure
against the linux goldens. The 1278 milestone made the opposite mistake here:
it read `scope` from the golden while computing roles from a darwin closure
and joined by name, and reported nine false exclusions where there were six.

| component | golden role / scope / source | corrected role |
|---|---|---|
| `numactl` | `unreferenced` / **`excluded`** / `closure-role` | **`artifact-input`** |
| `binutils-patchelfed-ld-wrapper` | `artifact-input` / none | `both` |
| `bootstrap-stage0-binutils-wrapper` | `artifact-input` / none | `both` |
| `find-xml-catalogs-hook` | `artifact-input` / none | `both` |

- **1 component loses a false `scope: "excluded"`**: `numactl`, referenced
  into the artifact through an attribute waybill does not read, and currently
  claimed absent from it.
- **0 components gain an exclusion.** Both the `unreferenced`→`build-tooling`
  moves keep `excluded` (both roles assert it) and the
  `artifact-input`→`both` moves keep asserting nothing.
- **2 of the 7 role-changing derivations are absent from the golden**, so they
  are currently dropped as unreferenced-and-unpatched and will become emitted
  components.

**Decision**: SC figures name `numactl` specifically. A criterion that said
"the false exclusions are retracted" without naming them is the shape that
let 1278 ship a count of nine for a population of six.

---

## R3 — Where does the FR-002 marker live? **Native-first audit.**

Read from the vendored schemas at `waybill-cli/tests/fixtures/schemas/`, not
from memory:

| candidate | what it says | verdict |
|---|---|---|
| CDX `dependencies[].dependsOn` | a real dependency edge | **Rejected as the primary carrier** — see below |
| CDX `compositions[].aggregate` | `incomplete` and six narrower variants: the *graph* is incomplete | Already used for the document-level claim (#1348); says nothing per component |
| SPDX 3 `Relationship.completeness` | `complete` / `incomplete` / `noAssertion` | Same: a property of a relationship, and there is no relationship here to qualify |
| SPDX 3 relationship types | 59 of them | None means "referenced through a mechanism the producer does not model" |
| SPDX 2.3 | no completeness vocabulary at all | — |

**The interesting rejected option is the first.** waybill could emit a genuine
dependency edge for a `defaultNativeBuildInputs` reference: nix states the
relationship, and CDX and SPDX both have a native way to express it. The
closure tier emits **no edges at all** today, so this would be new surface.

**Decision**: the marker is a per-component `waybill:` annotation naming the
attributes, and **no edge is emitted**.

**Rationale**: an edge is a stronger claim than this milestone is entitled to
make. `defaultNativeBuildInputs` is stdenv injecting its own machinery, not
the package declaring a dependency, and emitting it as `dependsOn` would put
stdenv's internals into the dependency graph of every derivation stdenv
touches — on the darwin closure that is 311 declarations across 19
derivations, which would land in the graph-completeness and orphan-reason
passes as well. The clarification that produced FR-012 chose disclosure over
classification precisely to avoid that, and an edge is classification by
another name.

**Alternative explicitly left open**: if a later milestone decides the closure
tier should emit edges, these are a reasonable place to start, and the marker
becomes redundant rather than wrong.

---

## R4 — The consulted set must have one source (FR-004)

**Measured**: `classify::consulted_fields()` already assembles
`ARTIFACT_FIELDS ∪ TOOLING_FIELDS ∪ {patches}`, and `RawClosure::read_outcome`
takes it as an argument — so the disclosure's complement already derives from
the same two constants the classifier reads. #1348 built that deliberately.

**Decision**: widen the two constants and nothing else. SC-009's test asserts
the two agree rather than trusting that they do.

**One trap found while reading it**: `depsBuildHost` and `nativeCheckInputs`
are in `TOOLING_FIELDS` but are declared on **zero** derivations of either
closure. They are consulted-but-never-declared, which is a third state
alongside declared-and-read and declared-and-unread. FR-005 covers
declared-but-empty; a never-declared field must also not surface as a gap,
and the disclosure already gets this right because it iterates over what each
derivation declares rather than over the field list.

---

## R5 — Does the marker need a catalog row? **Yes, and it lands with its extractors.**

A new per-component annotation needs a row in
`docs/reference/sbom-format-mapping.md` and three extractors in the same
change: a documented row with no extractor fails
`every_catalog_row_has_an_extractor`, and the project has been bitten by
adding doc rows ahead of emission code.

**Decision**: one row, `Directionality::SymmetricEqual`, identical payload in
all three formats — the same disposition C204 took, and for the same reason.
The next free number is confirmed when the row is written rather than
reserved here.

---

## R6 — Which figures are system-specific, and how badly?

| population | x86_64-linux | aarch64-darwin |
|---|---|---|
| unconsulted declarations | 230 | 353 |
| derivations carrying them | 23 | 19 |
| derivations whose role changes | 7 | 8 |
| FR-002 population (unreferenced, named only by an unclassified attribute) | **3** | **16** |

**The last row differs by more than five times.** The stdenv population is not
system-stable, so FR-012's "323 of 353" framing — taken from the darwin
census — is a darwin fact. On linux the same two attributes account for a much
smaller share.

**Decision**: the spec's measured figures are re-labelled by system, and the
success criteria are stated against linux. This is not pedantry: the spec
currently quotes 353 and 323 without a system, and those are the numbers a
reviewer would check against a golden that will show 230 and a much smaller
stdenv share.

---

## R7 — What can move these numbers

nixpkgs is pinned by the target's `flake.lock`, so the inputs are stable, but
the attribute *population* is a nixpkgs property and a bump can change it.
The invariant criteria (SC-001, SC-002) are written to survive that; the
counted ones are dated.

A second mover, learned the hard way on 2026-11-04: evaluating a fixture flake
with a real `nix` can **rewrite its `flake.lock`**, which changes the closure
underneath a measurement. Probes here read a closure JSON captured once; they
do not evaluate anything themselves, and the capture command is recorded in
each probe's header.
