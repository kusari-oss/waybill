# Implementation Plan: Trustworthy `.cabal` dependency parsing

**Branch**: `895-fix-cabal-parser` | **Date**: 2026-09-16 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/895-fix-cabal-parser/spec.md`

## Summary

waybill's Haskell reader fabricates components. On a cabal-only project it
assembles package names out of adjacent configuration fields and source
comments, and it writes version *constraints* into the slot reserved for
resolved *versions*. Two of twenty-four components on the project that
surfaced this do not exist on Hackage.

The four observed symptoms reduce to two causes. The first is that a cabal
field block is terminated on the wrong rule — blank line, column zero, or end
of file, where the format's rule is indentation relative to the field line.
The second is independent: the constraint is sanitised into the identifier's
version segment.

The approach is to fix the termination rule from the format's actual
semantics (measured, research R1), emit versionless identifiers matching the
convention every major reader already uses, and carry the constraint solely
in the record catalogued as C20. Build tools stop claiming to be library
packages. A Haskell target joins the public corpus so the accuracy guarantee
is re-checked nightly rather than asserted once.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly.
**Primary Dependencies**: Existing only — `regex` (already a direct dep), `serde`/`serde_json`, `tracing`. **Zero new Cargo dependencies.** No subprocess calls, no network access.
**Storage**: N/A — all parsing state is in-process for a single scan, as with every reader since milestone 002.
**Testing**: `cargo test --workspace` via `./scripts/pre-pr.sh`; the Haskell reader's 23 in-src unit tests plus four integration targets (`haskell_cabal_baseline`, `haskell_edge_cases`, `haskell_stack_discrimination`, `haskell_tier_fallbacks`). Second phase adds a public-corpus target with CI-generated goldens.
**Target Platform**: Every platform waybill supports; nothing platform-specific.
**Project Type**: CLI — single Rust workspace.
**Performance Goals**: No regression. The parsing change is local to one reader over files of a few kilobytes; scan time is not expected to move measurably, and any claim that it did must be established by an interleaved A/B on identical machine state rather than against a separately-taken baseline.
**Constraints**: Emission for projects with no `.cabal` content must stay byte-identical. No committed golden may change in the first phase (research R5).
**Scale/Scope**: One reader file (`waybill-cli/src/scan_fs/package_db/haskell.rs`), one catalogue label, plus corpus-fixture additions in the second phase.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | PASS. No new dependencies of any kind; no C, no dynamic linkage, no subprocess. |
| **III. Fail Closed** | PASS, with a clarified reading. An unreadable entry emits nothing for that entry (FR-012) — the failure is closed at the granularity that matters. Closing at *list* granularity was considered and rejected in clarification: one bad entry would delete its readable siblings, trading a false positive for a larger false negative. |
| **IV. Type-Driven Correctness** | PASS. The declaration kind (library vs build tool) and the executable name become modelled fields rather than string conventions, so the `package:executable` shape cannot silently flow into a package-name slot again. |
| **V. Specification Compliance** | PASS. The constraint stays in an annotation because no CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native field models a *declared constraint* as distinct from a *resolved version* — audited in research R3. The catalogue row already exists (C20); its label is corrected, not replaced. |
| **VIII. Completeness** | PASS, and improved. Per-entry rather than per-list failure handling (FR-012) minimises what is dropped, and the skipped count is reported so under-reporting is visible rather than silent. |
| **IX. Accuracy** | **This is the principle the feature exists to restore.** Fabricated components are false positives of the worst kind — indistinguishable from real data. FR-003 and FR-004 state the invariant; contract A-1 and A-8 make it measurable against the source file rather than against the parser's own report. |
| **X. Transparency** | PASS. The skipped-entry count is emitted including when zero (FR-012b), so "fully readable" and "field missing" stay distinguishable — the same reasoning as C159. |

**No violations to justify.** The one clause worth flagging is the Fail Closed
reading above: it is a deliberate interpretation recorded in clarification,
not an oversight.

**Post-Phase-1 re-check**: unchanged. The design adds no dependency, no
subprocess, no network call, and no new annotation vocabulary.

## Project Structure

### Documentation (this feature)

```text
specs/895-fix-cabal-parser/
├── plan.md                        # This file
├── spec.md                        # Feature specification
├── research.md                    # Phase 0 — R1..R6, all measured
├── data-model.md                  # Phase 1 — FieldBlock, DeclaredDependency, EmittedComponent
├── quickstart.md                  # Phase 1 — before/after verification procedure
├── contracts/
│   └── cabal-parsing.md           # Phase 1 — clauses A-1..A-8 with today/after status
├── checklists/
│   └── requirements.md            # Spec quality checklist (16/16)
└── tasks.md                       # Phase 2 — created by /speckit.tasks, not here
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   ├── scan_fs/package_db/
│   │   └── haskell.rs             # The whole of the parsing change:
│   │                              #   field-block termination (R1)
│   │                              #   comment handling inside a block
│   │                              #   entry splitting + per-entry skip count
│   │                              #   build-tool package:executable split
│   │                              #   versionless identifier construction
│   └── generate/                  # Verified, not modified — the constraint
│                                  #   annotation already flows to all three
│                                  #   emitters via extra_annotations
└── tests/
    ├── haskell_cabal_baseline.rs  # Existing — must stay green (contract A-7)
    ├── haskell_edge_cases.rs      # Existing + new edge-case coverage
    ├── haskell_stack_discrimination.rs  # Existing — out-of-scope path, untouched
    ├── haskell_tier_fallbacks.rs  # Existing — must stay green
    └── <new integration target>   # The #891 reproducer + the A-8 measurement
                                   #   comparing emitted names against the file

docs/reference/sbom-format-mapping.md   # C20 label: singular -> plural (R3)

# Second phase only:
waybill-cli/tests/corpus_harness_195/manifest.rs       # New Haskell target
waybill-cli/tests/fixtures/public_corpus/<target>/     # Goldens — CI-generated
```

**Structure decision**: Single workspace, one reader file. The feature touches
no shared infrastructure, which is why the first phase can land without
golden churn. Emitter code is read during verification but not modified — the
constraint annotation already reaches all three formats through the standard
`extra_annotations` channel, and research R3 confirmed the extractors are
correct.

## Phasing

Two phases, deliberately ordered (research R6).

**Phase A — the parser.** Self-contained: source, unit tests, synthetic
integration fixtures. Delivers the accuracy fix and every contract clause
except the nightly re-check. Can land alone.

**Phase B — the corpus target.** Needs an external mirror created, a pin
chosen, and goldens produced by a CI round-trip. Delivers SC-002 and SC-007.

Landing A first means B's first goldens record correct output. Adding B first
would commit goldens full of fabricated components and then immediately
refresh them — which is precisely how the drift this project keeps fighting
accumulates.

## Risks

| Risk | Mitigation |
|---|---|
| A fix that handles only one cabal layout | The reproducer carries both layouts (hpack-style and `cabal init`-style) in one file, measured in R1. A single-layout fix fails it. |
| A new test that passes against the defect | SC-008 requires teeth-checking each one by reverting only the production hunks. This project has shipped such a test before; quickstart §7 gives the procedure. |
| Scope creeping into the resolver path | R2 scopes the versionless change to cabal-declared dependencies. The `ghc` placeholder keeps its sentinel and its passing test. |
| Corpus goldens generated locally | They embed runner-absolute paths and would pass only on the generating machine. Phase B follows the documented CI procedure; quickstart §9 restates it. |
| The corpus repo turns out to carry a freeze file | Then it exercises the lockfile path, not this one. R4 measured three candidates and rejected one on exactly this ground; the chosen repo is re-checked at implement time rather than assumed. |
