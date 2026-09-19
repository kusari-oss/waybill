# Implementation Plan: Same-named resolves in different Pants namespaces

**Branch**: `922-fix-resolve-namespace-merge` | **Date**: 2026-09-19 | **Spec**: [spec.md](./spec.md)
**Issue**: [#919](https://github.com/kusari-oss/waybill/issues/919)

## Summary

`--split=resolve` groups on the bare resolve name, so a Python `default` and a
JVM `default` merge into one document containing both. The fix is to group on
the namespace-qualified resolve, which requires recording the namespace per
component — the emitted component set is the only thing the grouping can
consult, and today it cannot tell the two apart.

Phase 0 settled the one question the spec left open and turned up two things
that change the work.

**The namespace is a scalar, measurably.** Zero components in the corpus and
exactly one in the fixtures carry plural membership, and that one is in two
*Python* resolves. Cross-namespace membership cannot arise today because the
Python readers emit `pypi`/`generic` and the coursier reader emits `maven`,
and dedup can only union components sharing a PURL. That is a property of
today's readers rather than an enforced invariant, so the accessor detects
plurality and refuses to guess instead of assuming it away.

**Ecosystem inference is not merely unreliable, it is empty for a large class
of components.** A *Python* resolve routinely contains `pkg:generic/*`
entries — milestone 223 emits them for non-PyPI Pex sources, measured on both
`pants-example-python` and `pants-example-django`. `generic` carries no
namespace signal at all.

**The blast radius is smaller than the spec assumed.** Three corpus targets
carry resolve membership, not five: `pants-example-golang` and
`-javascript` have none.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json`, `tracing`. **Zero new Cargo dependencies.**
**Storage**: none. Per-component metadata, emitted and never read back.
**Testing**: `cargo +stable test --workspace`, plus the public-corpus lane for golden movement.
**Target Platform**: unchanged.
**Project Type**: SBOM emitter + split grouping. Additive wire change, plus a behavioural fix to grouping.
**Performance Goals**: none. One scalar per component.
**Constraints**: non-colliding repositories must stay byte-identical (FR-004); the membership annotation keeps its v0.9.0 key and shape (FR-006a); C161 is out of scope (#924).
**Scale/Scope**: one catalogue row and three extractors; a namespace recorded by three readers; the grouping key; filename/manifest qualification on collision; the degenerate-split count; two fixtures and a corpus target.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | No dependencies added. **Pass.** |
| **II. eBPF-Only Observation** | Not engaged. **N/A.** |
| **III. Fail Closed** | Engaged and load-bearing. A component that somehow carries two namespaces is a violated assumption, and the accessor returns "cannot answer" and warns rather than picking one — the m911 `read_single` shape. Guessing here would silently file a component into the wrong resolve's document, which is the defect being fixed. **Pass.** |
| **IV. Type-Driven Correctness** | The namespace is the existing closed `python \| jvm` set, not a string. The grouping key becomes a qualified type rather than a `String`, so a future caller cannot accidentally key on the bare name again — which is the whole defect. **Pass.** |
| **V. Standards-native first** | Audited, and **this time it does come back empty** — see below. **Pass.** |
| **VI. Three-Crate Architecture** | No crate boundary moves. **Pass.** |
| **VII. Test Isolation** | Fixtures crate-local; the corpus target is the existing pinned-SHA mechanism. **Pass.** |
| **VIII. Completeness** | Improved rather than threatened: a merged document currently over-reports one resolve and under-reports nothing, but every consumer filtering by resolve receives components that resolve does not pin. **Pass.** |
| **IX. Accuracy** | **The principle this milestone exists to serve.** A document claiming to be resolve `default` while containing another namespace's packages is a false statement about what a resolve pins. **Pass.** |
| **X. Transparency** | Served: a consumer can determine which namespace a resolve belongs to from the document. **Pass.** |
| **XI / XII. Enrichment** | Not engaged. **N/A.** |

### Principle V audit — no native carrier, and the contrast is worth recording

Milestone 912's audit found a native carrier that existed and was deliberately
unused, and its catalogue row says so. This one genuinely comes back empty,
and recording the difference matters: a catalogue where every row claims
"no native construct" is a catalogue nobody checks.

Audited for "which build-tool configuration section declares the dependency
set this component belongs to":

- **CycloneDX `component.group`** — the package's own group (Maven groupId, npm
  scope). Overloading it would corrupt an identity field.
- **PURL namespace segment** (`pkg:maven/<namespace>/<name>`) — same objection,
  and worse: it is part of the component's identity, so writing a build-tool
  concept there would change the PURL and break matching.
- **CycloneDX `compositions[]`** — describes completeness of an aggregate, not
  which configuration section declared one.
- **SPDX 2.3 `Package.sourceInfo`** — free text with no defined semantics; a
  consumer cannot parse it reliably.
- **SPDX 3 `software_Package` fields** — no notion of a build tool's resolve
  namespace.

No format models a build tool's configuration namespace, because it is a
Pants-specific concept. The annotation is justified by **absence**, which the
C164 row must state plainly — and must not be confused with C163's row, whose
justification is parity around a carrier that does exist.

**No violations. Complexity Tracking omitted.**

## Project Structure

```text
specs/922-fix-resolve-namespace-merge/
├── plan.md               # This file
├── spec.md               # 16 FRs, 11 SCs, 4 clarifications
├── research.md           # Phase 0 — R1..R7, with measurements
├── data-model.md         # Phase 1
├── contracts/
│   └── resolve-namespace.md
├── quickstart.md         # Phase 1 — verification per SC
├── measurements/         # singularity.txt, singularity-fixtures.txt
└── checklists/requirements.md
```

```text
waybill-cli/src/
├── scan_fs/package_db/
│   ├── pants_resolve.rs                     # scalar namespace accessor + plural guard
│   ├── pants/lockfile.rs, pants_jvm/lockfile.rs, pip/uv_lock.rs
│   │                                        # write the namespace (3 readers, 4 sites)
│   └── ../../resolve/deduplicator.rs        # merge policy for the new key
├── generate/
│   ├── split.rs                             # qualified grouping key; group count; filename/manifest
│   ├── cyclonedx/metadata.rs … spdx/*       # per-component emission (C164)
└── parity/extractors/{mod,cdx,spdx2,spdx3}.rs

docs/reference/sbom-format-mapping.md        # C164, justified by ABSENCE
waybill-cli/tests/fixtures/pants_namespace_collision*/
```

**Structure Decision**: the same three layers milestone 912 used — readers
record, emitters carry, catalogue registers — plus a grouping change that
milestone did not need. The grouping is the defect; the annotation is what
makes fixing it possible.

## Phase ordering

1. **The namespace, per component.** Scalar accessor with a plural guard,
   written by three readers, merged by dedup. Nothing downstream can be
   correct before this exists.
2. **US1 — qualified grouping.** The key becomes namespace-qualified, and
   filenames plus manifest ids qualify where a collision exists.
3. **US3 — the group count.** Taken after regrouping, so a repository whose
   only resolves collide splits rather than falling back. Separate because a
   fix that regroups but counts early leaves this silently broken.
4. **US2 — emission.** C164 across three formats, catalogue row, extractors.
5. **Regression gates and goldens.** The collision fixture variant for FR-003,
   a polyglot corpus target, and the golden refresh for three Pants targets.

## Post-design Constitution re-check

Re-evaluated after Phase 1. No new violations.

Principle V's assessment is the one to carry forward, and it is the opposite
of milestone 912's: that audit found a carrier and declined it, this one finds
nothing. Both rows sit in the same catalogue and a reader must be able to tell
which is which, so C164's justification clause says "no format models a build
tool's configuration namespace" rather than reusing C163's parity language.

Principle IV does more work here than it appears to. The defect is that a
`String` grouping key silently accepted a bare name; making the key a type
that cannot be constructed from a bare name is what stops the same bug being
reintroduced by the next person who touches the split.
