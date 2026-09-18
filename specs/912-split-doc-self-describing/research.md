# Phase 0 Research: A split document says which resolve it is

**Feature**: `912-split-doc-self-describing` | **Issue**: [#914](https://github.com/kusari-oss/waybill/issues/914)
**Date**: 2026-09-18

Every claim is traceable to a file and line in this workspace or to committed
corpus output.

---

## R1 — The Principle V audit, and it does NOT come back empty

The clarify answer placing the identity in its own document-scope annotation
was made **conditional** on this audit. The audit's result is more
interesting than either outcome the spec anticipated.

### A native carrier exists, and we are refusing to use it

Every format already has a first-class way to say what a document is about:

| format | carrier | what waybill does today |
|---|---|---|
| CycloneDX 1.6 | `metadata.component` | set to the resolve anchor for a **declared** resolve |
| SPDX 2.3 | `documentDescribes` | same |
| SPDX 3 | `SpdxDocument.rootElement`, `software_Sbom.rootElement` (`v3_document.rs:382`, `:427`) | same |

That is exactly the semantic this feature needs, and it already works — for
declared resolves. It fails for discovered ones for a single reason: it
points at a **component**, and we decline to invent one (FR-006, m868).

**So the gap is not in the standards.** It is a consequence of a deliberate
refusal on our side. The KEEP-NO-NATIVE justification for any new row must
say that plainly rather than claim no construct exists — the honest phrasing
is "a native carrier exists and is unusable here because using it would
require asserting an ownership the repository never declared".

### The structured alternatives, and why they do not fit

- **CycloneDX `compositions[]`** — `aggregate` plus `assemblies`/`dependencies`
  scoping. The closest native concept for "this is a partial view", but it
  describes **completeness**, not identity: it answers "is anything missing",
  not "which slice is this".
- **CycloneDX BOM-Link / `externalReferences[type=bom]`** — points from this
  BOM at another. It could say "I relate to the parent scan", which is not
  "I am the `lint` slice of it".
- **SPDX 2.3 relationship types** (`VARIANT_OF`, `DESCRIBED_BY`, `AMENDS`) —
  all relate two documents or elements; none names a subset axis.
- **SPDX 3 `Bundle.context`** — genuinely close: a `Bundle` is an
  `ElementCollection` whose `context` states the relationship its members
  share. This is the only structured native option that means roughly the
  right thing.

### The decisive argument is parity, not absence

`Bundle.context` exists in SPDX 3 and has **no CycloneDX equivalent**. FR-004
requires one reading procedure across formats, and the project's parity gate
requires a catalogue row to be carried by all three emitters. A construct
present in one format and absent in another cannot satisfy either.

**Decision**: a new document-scope annotation, carried identically by all
three emitters. The clarify answer stands — but on parity grounds, and with
the KEEP-NO-NATIVE audit stating that a native carrier exists and is
deliberately unused, not that none was found.

**Alternative rejected — use the native carrier where it works and the
annotation where it does not.** Declared resolves would answer via
`metadata.component`, discovered ones via the annotation. That is precisely
the two-code-paths outcome FR-004 exists to prevent, and the consumer would
have to establish provenance to know which to read.

---

## R2 — "Unambiguous across namespaces" needs a namespace that is not recorded

The clarify answer requires the identity to distinguish resolves sharing a
name across `[python.resolves]` and `[jvm.resolves]`. Checking what exists:

**Nothing records the Pants language namespace.** `waybill:pants-resolve`
carries a bare name. The only signal is indirect — the *members'* PURL
ecosystem:

```
pants-example-python   members are pkg:pypi/*     anchor: pkg:generic/<name>, component-kind=lockfile-resolve
pants-example-jvm      members are pkg:maven/*    anchor: NONE
```

**Decision**: the readers must record the namespace explicitly. Inferring it
from member ecosystems is possible today only because each resolve happens to
be single-ecosystem, and that is a property of the fixtures rather than a
guarantee — a Pants resolve is a lockfile, and nothing forbids a future
reader emitting mixed types from one.

**Consequence for scope**: this touches the same three readers m911 touched.
It is the largest piece of work in an otherwise small feature, and the tasks
should say so rather than presenting the feature as a one-line annotation.

---

## R3 — Anchoring is Python-only, which widens who needs this

Milestone 868 emits a resolve anchor from the **Pex** reader only. The
coursier/JVM reader emits none:

```
pants-example-jvm: waybill:component-kind = lockfile-resolve  ->  NONE
```

So a JVM Pants repository has **no anchors at all**, declared or not — and
every one of its per-resolve documents names the repository, exactly like the
convention-discovered Python case.

The spec frames the gap as "declared works, discovered does not". That is
true within Python and false across the product: an entire ecosystem is in
the failing case regardless of whether its resolves are declared.

**Decision**: no change to anchoring — that is m868's design and #914 is not
the place to revisit it. But the feature's value is larger than the spec
claims, and a JVM fixture belongs in the test set, because the Python-only
fixtures would let a Python-only implementation look complete.

---

## R4 — Interaction with #919, which is unfixed by design

[#919](https://github.com/kusari-oss/waybill/issues/919): `--split=resolve`
groups on the bare resolve name (`split.rs:219`), so same-named resolves from
different namespaces merge into one document. Shipped in v0.9.0.

**Decision**: define the identity from the resolve's own namespace-qualified
name, derived at read time, not from the split's grouping key. The identity
is then already correct for the world where #919 is fixed, and in today's
world a merged document is the one place the identity cannot be stated
truthfully.

**What a merged document should do** is a real question this feature must not
dodge: it genuinely represents two resolves. Stating either one alone would
be false. The options are to state both, or to state that the document is
ambiguous. Deferred to the plan's data model rather than resolved here,
because it is a presentation question about a state that should not exist.

---

## R5 — Absent, not empty

FR-009 and SC-008 require a pre-feature document, an unsplit document, and a
non-resolve split document to be distinguishable from one that has an
identity.

**Decision**: the annotation is absent entirely rather than present with an
empty value. This matches how the project already handles "the question does
not apply" — the m868 ownership annotation is absent on scans that found no
Pex lockfile (contract A-7), precisely so "nothing needed guessing" stays
distinguishable from "the field is missing".

---

## R6 — What has NOT been established

- **Whether any consumer reads a split document without its manifest.** The
  feature's whole justification is that such a reader exists. #914 records the
  question and nobody has answered it. If the answer is no, this feature is
  tidiness rather than value — worth asking the consumer before building it.
- **Whether a real polyglot Pants repository declares colliding resolve
  names.** #919's severity depends on it; so does how hard R2 has to work.
- **Whether a resolve can legitimately contain components of mixed ecosystem**,
  which is the assumption behind R2's rejection of ecosystem-inference.

## Open items carried into Phase 1

None blocking. R1 changes the *justification* for the clarify answer without
changing the answer; R2 and R3 enlarge the work; R4 leaves one presentation
question for the data model.
