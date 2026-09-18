# Implementation Plan: A split document says which resolve it is

**Branch**: `912-split-doc-self-describing` | **Date**: 2026-09-18 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/912-split-doc-self-describing/spec.md`
**Issue**: [#914](https://github.com/kusari-oss/waybill/issues/914)

## Summary

`--split=resolve` emits one SBOM per Pants resolve. Some of those documents
cannot say which resolve they are: every document-scope signal describes the
repository, and the ownership statement is byte-identical across them. A
reader holding one file sees several resolve names and nothing identifying
the file.

The fix is a document-scope statement of the document's own resolve, carried
identically by all three emitters, present on every per-resolve document and
absent everywhere else.

Phase 0 changed two things about how big this is.

**The Principle V audit did not come back empty.** A native carrier exists in
every format — `metadata.component`, `documentDescribes`, `rootElement` — and
already works for declared Python resolves. It fails for the rest only
because it points at a *component* and we decline to invent one. The
annotation is justified by **parity**, not absence: SPDX 3's `Bundle.context`
is the one structured native option and CycloneDX has no equivalent.

**The namespace this feature needs is recorded nowhere.** Making the identity
unambiguous across `[python.resolves]` and `[jvm.resolves]` requires readers
to record which section declared a resolve. Nothing does. That is the largest
piece of work here, and it touches the same three readers m911 touched.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (annotation value), `tracing`. **Zero new Cargo dependencies.**
**Storage**: none. Document-scope metadata, emitted and never read back.
**Testing**: `cargo +stable test --workspace`. Crate-local Pants fixtures, including a **JVM** one (R3) and a namespace-collision one (C-2).
**Target Platform**: unchanged.
**Project Type**: SBOM emitter. Additive wire change.
**Performance Goals**: none. One string per emitted document.
**Constraints**: no component is synthesised (FR-006); the repository-wide ownership statement stays repository-wide (FR-007); `--split=resolve` grouping is #919's problem and is not fixed here.
**Scale/Scope**: one new catalogue row plus three extractors; a namespace field set by three readers; the split's identity derivation; fixtures for JVM and namespace collision.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | No dependencies added. **Pass.** |
| **II. eBPF-Only Observation** | Not engaged — describes a document, changes nothing about discovery. **N/A.** |
| **III. Fail Closed** | Engaged weakly and correctly: the identity is absent rather than empty where it does not apply, so "no answer" and "the question does not apply" stay distinguishable. **Pass.** |
| **IV. Type-Driven Correctness** | The namespace is a closed set (`python`, `jvm`), which is a type rather than a string convention. **Pass.** |
| **V. Standards-native first** | **The gate that shaped this feature**, and the one place the assessment needs stating carefully rather than briefly — see the note below the table. **Pass, by the parity exemption, on a narrower construct than the audit first surfaced.** |
| **VI. Three-Crate Architecture** | No crate boundary moves. **Pass.** |
| **VII. Test Isolation** | Fixtures crate-local and self-contained. **Pass.** |
| **VIII. Completeness** | Not about component coverage. The related concern — that a document under-reports — is #902's, already shipped. **N/A.** |
| **IX. Accuracy** | The principle doing the most work here. FR-006 exists because inventing an owning component would assert an ownership the repository never declared. C-6 exists because a merged document represents two resolves and stating either alone would be false. **Pass — and it is the constraint that makes the feature necessary.** |
| **X. Transparency** | Directly served: the feature's entire content is telling a reader what they are holding. **Pass.** |
| **XI / XII. Enrichment** | Not engaged. **N/A.** |

**No violations. Complexity Tracking omitted.**

### Principle V in full, because the short version elides the step that matters

The constitution permits a `waybill:*` field in exactly two cases: finer-grained
information the standard cannot express, or **a parity gap where one format has
the native field and another does not**. Two different native constructs are in
play here, and collapsing them makes the exemption look like it fits when it
does not:

1. **The document-subject carrier** — `metadata.component`, `documentDescribes`,
   `rootElement`. Research R1 found this in **all three** formats, already
   working for a declared Python resolve. There is no parity gap in it. It is
   unusable here for a different reason: it names a *component*, and FR-006
   (Principle IX) forbids inventing the one a discovered resolve lacks.
2. **SPDX 3 `Bundle.context`** — the only native construct that states a
   document's subject *without* a component. CycloneDX has no equivalent;
   SPDX 2.3 has none. **This** is the parity gap, and it is the one the
   annotation bridges.

So the honest chain is: the all-format construct is declined on Principle IX
grounds, which leaves only a single-format construct, which is a parity gap,
which is an enumerated exemption. Asserting "parity" without naming which
construct would be substituting (2) for (1) silently — the exemption would
appear to fit a carrier that is present everywhere.

Worth flagging for a future constitution amendment: "the native construct
exists but using it would require fabricating data" is a third exemption the
enumerated list does not contain, and this feature only avoids needing it
because `Bundle.context` happens to exist. A feature without that luck would be
stuck between two principles with no written way out.

One thing recorded rather than buried: **this feature's value has not been
established.** R6 notes that nobody has confirmed a consumer reads a split
document without its manifest — and the manifest already maps every file to
its resolve. If no such reader exists, this is tidiness. Asking costs one
message; building costs the work below. The plan does not resolve that, and
should not pretend the question was answered by filing the issue.

## Project Structure

### Documentation (this feature)

```text
specs/912-split-doc-self-describing/
├── plan.md                      # This file
├── spec.md                      # 12 FRs, 10 SCs, 3 clarifications
├── research.md                  # Phase 0 — R1..R6
├── data-model.md                # Phase 1 — 3 entities
├── quickstart.md                # Phase 1 — verification per SC
├── checklists/requirements.md   # 16/16
├── contracts/
│   └── resolve-identity.md      # Phase 1 — C-1..C-6
└── tasks.md                     # Phase 2 — NOT created by /speckit.plan
```

### Source (repository root)

```text
waybill-cli/src/
├── scan_fs/package_db/
│   ├── pants/, pants_jvm/, pip/uv_lock.rs   # record the language namespace
│   └── pants_resolve.rs                     # namespace alongside membership
├── generate/
│   ├── split.rs                             # derive + attach the identity
│   ├── cyclonedx/metadata.rs                # doc-scope emission
│   └── spdx/{annotations,v3_annotations}.rs  # doc-scope emission
└── parity/extractors/{mod,cdx,spdx2,spdx3}.rs

docs/reference/sbom-format-mapping.md        # new row, with an honest audit
waybill-cli/tests/fixtures/pants_*/          # JVM + namespace-collision fixtures
```

**Structure Decision**: the work splits into a reader layer (record the
namespace), an emit layer (derive and attach the identity), and a catalogue
layer. The reader layer is the surprise — the spec reads like a one-annotation
feature and it is not, because the thing that makes the identity unambiguous
does not exist yet.

## Phase ordering

1. **Namespace recording.** Readers record which Pants section declared each
   resolve. Nothing downstream can be unambiguous until this exists, and it
   is the largest piece.
2. **US1 (P1) — identity on every per-resolve document.** Derivation,
   emission across three formats, catalogue row and extractors.
3. **US2 (P2) — uniformity.** Declared documents carry it too, and a test
   asserts it agrees with the root. Small once US1 lands; separate because it
   is the requirement most likely to be quietly dropped as redundant.
4. **Fixtures and polish.** The JVM fixture (R3) and the namespace-collision
   fixture (C-2) are not optional: Python-only fixtures would let a
   Python-only implementation look complete, and the collision fixture is the
   only thing exercising FR-001a.

## Post-design Constitution re-check

Re-evaluated after Phase 1. No new violations.

Principle V's assessment changed during research and is the one to carry
forward: the audit found a native carrier rather than an absence, so the
catalogue row's justification is parity and FR-006, not "no construct
exists". Writing the easier claim would have been wrong and would have sat in
the catalogue unchallenged.

Principle IX is what the whole feature is downstream of. Every constraint that
makes this harder than a one-line annotation — no invented component, both
resolves on a merged document, absent rather than empty — is that principle
declining to let the document say something the repository never established.
