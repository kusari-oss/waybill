# Contract: the unclassified-reference marker

The per-component record that nix references a member through an attribute
waybill does not classify. It is what stops `unreferenced` meaning two things
(FR-002) without classifying stdenv's machinery (FR-012).

## Wire shape

A new per-component annotation, one catalog row, identical payload in all
three formats, `Directionality::SymmetricEqual`. A **JSON array of attribute
names, encoded as a string**, sorted:

```json
["defaultBuildInputs","defaultNativeBuildInputs"]
```

**It names the attributes** (FR-013). "Referenced by something waybill does
not classify" without saying what is a smaller version of the same defect: it
tells a consumer a gap exists and not where. Sorted so the value is
byte-stable.

## Emission gating

| member's role | named by an unclassified attribute | marker |
|---|---|---|
| `unreferenced` | yes | **present** — this is the case the milestone exists for |
| `unreferenced` | no | absent — genuinely nothing depends on it |
| any other role | yes | **present** — the reference is still unclassified, and a reader comparing two `artifact-input` members should be able to tell that one of them also has an unread reference |
| any other role | no | absent |

Absence therefore means "no unclassified attribute names this member", never
"not checked". The FR-007 guard is what makes that trustworthy: an attribute
the classifier does not recognise is reported rather than ignored, so an
unread attribute cannot masquerade as an absent one.

**On the pinned x86_64-linux closure the marker appears on 9 components.**
Corrected during implementation; this contract was drafted saying 3, and the
error was a category one rather than an arithmetic one. The chain, measured
with `measurements/remainder-census.py` against the same closure the goldens
pin:

| | count |
|---|---|
| store-path declarations in unconsulted attributes | 211 |
| **closure members** those declarations name | 16 |
| of those, members emitted as components at all | 13 |
| after build-variant merging on `(pname, version)` | **9** |

Each step loses something for a different reason, and none of them is
negligible. 211→16 because `defaultNativeBuildInputs` names the same few
members from 13 derivations. 16→13 because an `unreferenced` member applying
no patch is not emitted, so the marker has nowhere to land — the reach
limitation stated above. 13→9 because a closure holds several derivations per
identity.

The nine: six `gcc-wrapper` variants (`bootstrap-stage{-xgcc,1,2,3,4}` and
the final one), `patchelf`, `update-autotools-gnu-config-scripts-hook`, all
through `defaultNativeBuildInputs`; and `glibc`, through
`extraNativeBuildInputs`. `patchelf` is also one of the two components this
milestone newly emits, so it carries both a role and the marker — the T044
row, present in the real closure as well as the fixture.

## Principle V audit

Read from the vendored schemas at `waybill-cli/tests/fixtures/schemas/`, not
from memory:

| claim | native carrier | disposition |
|---|---|---|
| this member is referenced through a mechanism the producer does not model | none in CDX 1.6, SPDX 2.3 or SPDX 3.0.1 | **KEEP-NO-NATIVE** |
| the graph is incomplete | CDX `compositions[].aggregate`, SPDX 3 `Relationship.completeness` | already used at document scope by #1348; says nothing per component |
| X depends on Y | CDX `dependencies[].dependsOn`, SPDX relationship types | **rejected — see below** |

**The rejected option is the interesting one.** nix states the relationship
and both formats can express it, so waybill could emit a real dependency edge
instead of a marker. It is rejected because an edge is a stronger claim than
this milestone is entitled to make: `defaultNativeBuildInputs` is stdenv
injecting its own machinery, not a package declaring a dependency, and
emitting it as `dependsOn` would put stdenv's internals into the dependency
graph of every derivation stdenv touches — 311 declarations across 19
derivations on the darwin closure — where they would also reach the
graph-completeness and orphan-reason passes. FR-012 chose disclosure over
classification precisely to avoid that, and an edge is classification by
another name.

Left open rather than closed: if a later milestone decides the closure tier
should emit edges at all — it emits none today — these are a reasonable place
to start, and this marker becomes redundant rather than wrong.

## Parity

Three extractors, one per format, landing in the **same change** as the
catalog row: a documented row with no extractor fails
`every_catalog_row_has_an_extractor`, and this project has added doc rows
ahead of emission code before.
