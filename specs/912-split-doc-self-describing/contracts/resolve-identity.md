# Contract: Split-document resolve identity

**Feature**: `912-split-doc-self-describing` (#914)
**Consumers**: anything reading a per-resolve split SBOM.

Additive. Nothing that parses today stops parsing.

---

## C-1. A per-resolve split document states its resolve

Carried at document scope, in all three formats, decoding to the same value
(the m911 precedent: encoding is each format's business, the decoded value is
the contract).

- Present on **every** per-resolve document, including those whose root
  component already names the resolve (FR-004). A consumer must not have to
  establish provenance in order to know where to read identity.
- **Absent** — not empty — on an unsplit document, a `--split=workspace` or
  `--split=directory` document, and anything produced before this feature
  (FR-009, C-4).

## C-2. The identity names the resolve and its namespace

`[python.resolves]` and `[jvm.resolves]` are separate namespaces in
`pants.toml`, so one repository can declare `default` in both. An identity
that cannot tell them apart fails at its only job.

This requires readers to **record** the namespace, which none does today.
Inferring it from members' PURL ecosystem works on current fixtures and is
rejected: it holds only because every fixture resolve is single-ecosystem.

## C-3. Where a root also names the resolve, they agree

A declared Python resolve's document states its resolve twice — in
`metadata.component` / `documentDescribes` / `rootElement`, and in the
identity. FR-005 requires agreement, **asserted by a test**. Two fields
stating one fact drift; a test is what stops it.

## C-4. The repository-wide statement stays repository-wide

The existing ownership annotation continues to describe the repository, not
the document, and its value is identical across a repository's split
documents. Narrowing it would make the documents differ — but at the cost of
the reader's view of what else exists, the same loss the split already
declines to inflict on component membership.

The two statements are separately readable: removing either leaves the other
intact (SC-009).

## C-5. No component is invented

A discovered resolve gains no anchor, in split output or anywhere else
(FR-006). This is the constraint that makes the feature necessary: the native
carrier for "what is this document about" — `metadata.component`,
`documentDescribes`, `rootElement` — points at a component, and we decline to
create one the repository never declared.

**The KEEP-NO-NATIVE audit must say this honestly.** A native carrier exists
and is deliberately unused. The claim that no construct exists would be
false, and the catalogue is not the place for a convenient one.

The decisive reason an annotation is used instead is **parity**: SPDX 3's
`Bundle.context` is the one structured native option that means roughly the
right thing, and CycloneDX has no equivalent. A row carried by one emitter
and not the others fails both FR-004 and the project's parity gate.

## C-6. A merged document states both resolves

#919 lets two same-named resolves from different namespaces collapse into one
document. That document represents both, so stating either alone is false; it
states both.

Self-correcting: once #919 lands the state cannot arise, and a consumer that
sees two identities is looking at a defect rather than a shape it must
support.

---

## Verification

| Contract | How verified |
|---|---|
| C-1 | a convention-only fixture: each document names its own resolve, read from content alone |
| C-1 (absence) | unsplit, `--split=workspace`, `--split=directory` → no identity present |
| C-1 (filename) | rename a document; the answer is unchanged |
| C-2 | a fixture declaring one name under both namespaces → distinguishable identities |
| C-3 | a declared-Python document: identity and root name the same resolve |
| C-4 | ownership value byte-identical across a repository's documents, and unchanged from before |
| C-5 | component counts per document unchanged; no anchor appears for a discovered resolve |
| C-6 | the #919 fixture, until #919 lands |

**A JVM fixture is required, not optional** (R3). Anchoring is Python-only, so
a JVM Pants repository has no anchors at all — every one of its documents is
in the failing case. Python-only fixtures would let a Python-only
implementation look complete.
