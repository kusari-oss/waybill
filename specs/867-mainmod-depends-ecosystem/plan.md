# Implementation Plan: Declared dependencies must resolve regardless of the requirer's PURL type

**Branch**: `867-mainmod-depends-ecosystem` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/867-mainmod-depends-ecosystem/spec.md`

## Summary

A component's manifest-declared dependencies are discarded when the
requirer's PURL type does not match the ecosystem its dependency names belong
to. Measured: `bitwarden/android` resolves **0 of 9** declared dependencies,
leaving a 103-component island unreachable.

The fix is to let a reader record the ecosystem of the names it read, and
resolve against that instead of against the requirer's identity. Absence of a
recording preserves today's behaviour exactly, which makes adoption
per-reader and keeps every untouched reader byte-identical by construction.

Research established that this is a **key substitution, not a search-strategy
change** — one `HashMap::get` per declared dependency, before and after — so
the performance question the clarification deferred is answered structurally
rather than budgeted. It also found an existing annotation family for
"declared but unresolved" whose Principle V audit is reusable, so this feature
adds one document-scope aggregate rather than a new vocabulary.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly.
`waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `std::collections::HashMap`,
`serde`/`serde_json` (annotation emission), `tracing` (the FR-005 log half).
**Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan, matching every reader
milestone since 002.
**Testing**: `cargo +stable clippy --workspace --all-targets` and
`cargo +stable test --workspace`, via `./scripts/pre-pr.sh`. Corpus evidence
via the m770 quality corpus and the committed public-corpus goldens.
**Target Platform**: Linux / macOS / Windows, unchanged.
**Project Type**: CLI + library (three-crate workspace).
**Performance Goals**: Within run-to-run noise of baseline. Measured baseline
on the `gradle-bitwarden-android` target at `d9c4f21e`: **920 / 603 / 600 ms**
across three consecutive runs; the 600 ms pair is steady state and is what the
confirmation compares against. Justification is structural (research R1), and
the measurement confirms rather than establishes it.
**Constraints**: Output byte-identical for every reader that has not adopted
(SC-004a) and for every same-ecosystem scan (SC-004).
**Scale/Scope**: Two readers adopt (gem, nuget). One new optional field, one
lookup-key change, one document-scope annotation, one existing annotation
broadened.

## Constitution Check

*GATE: passed before Phase 0; re-checked after Phase 1 below.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | PASS — no new dependencies at any layer, no C, no linkage change. |
| **III. Fail Closed** | PASS — an unresolved declaration produces no edge and no invented component. The feature never degrades toward guessing; it narrows what is searched. |
| **IV. Type-Driven Correctness** | PASS — the recording is an optional field whose `None` is a meaningful state ("not adopted"), not a sentinel. Adoption is visible in the type rather than by convention. |
| **V. Specification Compliance** | PASS with a recorded audit. The per-component half reuses C115, whose Principle V audit already concluded KEEP-NO-NATIVE. The new document-scope count inherits C104's rejection of `compositions[].aggregate` (composition completeness ≠ graph reachability). No native field is being bypassed. |
| **VIII. Completeness** | **This is the principle the feature serves.** Discarding a declared dependency is a false negative in the primary graph. Currently silent, which Principle VIII explicitly forbids: an unguaranteeable gap "MUST signal the gap per Principle X". |
| **IX. Accuracy** | PASS, and load-bearing in the design. Confining lookup to the recorded ecosystem is an accuracy choice: a same-named package in another ecosystem is a different package, and attaching it would be a false positive. This is why FR-006 removes the ambiguity class rather than arbitrating it. |
| **X. Transparency** | PASS — FR-005/a add the signal that is missing today, and FR-005a's always-emit-even-at-zero rule is what makes the signal readable rather than merely present. |
| **XI / XII. Enrichment** | N/A — no external data source; resolution is entirely within the scan. |

**No violations. No complexity deviations to justify.**

One tension worth naming rather than burying: Principle VIII (minimise false
negatives) and Principle IX (minimise false positives) pull in opposite
directions on the accepted edge case where a reader records the *wrong*
ecosystem. The design resolves it toward IX — a wrong recording causes a miss
rather than a wrong edge — and routes the correction to the reader, where the
assertion was made. That is a deliberate choice, recorded in the spec's Edge
Cases.

## Project Structure

### Documentation (this feature)

```text
specs/867-mainmod-depends-ecosystem/
├── plan.md              # This file
├── spec.md              # Feature specification (clarified)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── declared-dependency-resolution.md
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/
├── package_db/
│   ├── mod.rs                 # PackageDbEntry: the new optional field
│   ├── gem.rs                 # first adopter — application main module
│   └── nuget/mod.rs           # second adopter — generic-identity fallback
└── mod.rs                     # resolution: lookup key + normalisation,
                               # unresolved counting

waybill-cli/src/generate/
└── ...                        # document-scope count emission (all 3 formats)

waybill-cli/src/parity/extractors/
└── mod.rs                     # EXTRACTORS entry for the new catalogue row

docs/reference/
└── sbom-format-mapping.md     # new document-scope row; C115 scope broadened
```

## Phase plan

**Phase 1 — carrier.** Add the optional field. Nothing reads it yet, so
output is provably unchanged; this is the commit that makes SC-004a arguable.

**Phase 2 — resolution.** Use the recorded ecosystem for both lookup and
normalisation (D-1, D-3). Still no adopter, so still no output change.

**Phase 3 — gem adopts.** The measured case. `bitwarden/android` goes 0 → 9.
Perf confirmation against the 600 ms steady-state baseline lands here, since
this is the first commit where the path is exercised.

**Phase 4 — reporting.** Document-scope count, C115 broadened, catalogue row
plus its extractor **in the same commit** (a row without an extractor fails
`every_catalog_row_has_an_extractor` and `holistic_parity`).

**Phase 5 — nuget adopts.** The second reader, through its generic-identity
fallback.

**Phase 6 — corpus.** Re-author `gradle-bitwarden-android`'s expectations from
measurement. Its current `edges 346..424` was authored against fabricated
fallback edges and is not a target to aim at.

Phases 1–2 are deliberately output-neutral. If either changes a byte of
output, the adoption gate is not working and the rest of the plan is unsafe.

## Sequencing constraints

- The catalogue row and its extractor are one commit, never two.
- Phase 6 follows Phase 3 and 5, never accompanies them — expectations are
  re-authored against final behaviour, and against a CI measurement rather
  than a local one.
- Every check must be observed **failing** against the pre-change build before
  it is trusted (SC-007). This repo has shipped a schema gate that passed
  because its `$ref`s resolved to stubs and validated nothing.

## Post-Design Constitution Re-Check

Re-evaluated after Phase 1 artifacts. **Still no violations.**

Two things the design surfaced that strengthen rather than weaken the
assessment:

- Principle V's audit burden turned out to be **lighter than assumed**, not
  heavier: C115 already carries a completed KEEP-NO-NATIVE audit for this
  exact concept, so the feature reuses a conclusion instead of re-deriving
  one, and adds one aggregate row rather than a vocabulary.
- Principle IV is served better than first planned: making the field optional
  rather than defaulted encodes "has this reader adopted" in the type, which
  is what converts SC-004a from a test obligation into a structural property.

**Complexity tracking**: no entries. No principle is being deviated from, so
there is nothing to justify.
