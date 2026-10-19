# Implementation Plan: Every unreachable component says why it has no edge

**Branch**: `1218-explain-every-orphan` | **Date**: 2026-10-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1218-explain-every-orphan/spec.md`
**Issue**: #1214. Splits out #1234 (missing edges) and #1235 (duplicate coordinates).

## Summary

Three new values on the existing `waybill:orphan-reason` annotation (catalog
row **C45**), plus the deletion of the two ecosystem guards that make the
classifier Go/npm-only, so that the eighteen corpus components whose
edgelessness is a genuine property of the artifact say which kind of
edgelessness it is. One existing code — `dead-lockfile-entry` — is reused
unchanged for the largest of the four mechanisms, because measurement shows
all ten of its components are the multi-version case the code already names.

Nothing is added to the catalog and nothing to the extractor set. The whole
emission change lives in `generate/orphan_reason.rs`, which already has the
enum, the per-code counters and the single unconditional call site; the
feature adds variants and predicates and removes two `continue`s. Two
supporting changes sit outside it: the pip reader emits the existing C55
`source-mechanism` so all three mechanism-C components identify their build
system machine-readably, and the call site stops discarding the counts it
already receives.

**Sixteen of the thirty-four offenders are deliberately left failing.** They
are waybill's own defects — thirteen missing `requirements.txt` edges
(#1234) and three duplicate maven coordinates (#1235) — and a marker on
them would record a reader bug as a fact about the scanned repository while
closing the cell that reports it. The two I7 declarations covering them are
rewritten rather than deleted, which is what keeps them visible.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. No `+toolchain` argument when running the
gate — #1189, and the constitution's own table says so since v4.0.1 (#1232).
`waybill-ebpf` untouched.
**Primary Dependencies**: existing only — `serde_json` (annotation values;
already pervasive in this module), `std::collections::{HashMap, HashSet}`
(already imported by `orphan_reason.rs`). **Zero new Cargo dependencies**,
at any level including the lockfile.
**Storage**: none. Annotations are emitted into the SBOM and never read
back. Two committed TOML edits in
`waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`; no
runtime state and no per-run artifact.
**Testing**: `cargo test --workspace --no-fail-fast`. Unit tests beside
`orphan_reason.rs`'s existing `#[cfg(test)]` module; the hermetic invariant
lane (`invariants::tests::golden_matrix_is_evaluable_and_gated`); the parity
suite; a positive control per FR-017 and a mutation test per new value per
FR-018. The offline probe at `measurements/i7-populations.py` is re-run as
the closing step (FR-019).
**Target Platform**: unchanged — the emission path is platform-neutral and
runs on every host the workspace already supports.
**Project Type**: CLI / library — a change to the SBOM emission pipeline in
`waybill-cli`, plus one reader annotation.
**Performance Goals**: none beyond "not measurably worse". The classifier is
O(N) index build plus O(N × S) classification over in-memory components;
`orphan_reason.rs:148-156` records N ≤ 3000, S ≤ 5 and sub-millisecond per
scan on real fixtures. Removing the ecosystem guards widens the index from
Go/npm components to all components, which raises N for the index build
only, not its complexity class.
**Constraints**: no new filesystem read, subprocess or network call on the
emission path (FR-006, SC-010, enforced by the harness's existing
no-new-I/O check). Component counts, edge counts and the reachable set
byte-identical across all seventeen goldens (SC-009) — this feature changes
annotations only.
**Scale/Scope**: 17 corpus targets, 34 I7 offenders, 18 in scope. Three new
enum variants, three new predicates, two guard deletions, one reader
annotation, one log line, one catalog row edited, five declaration entries
touched (three deleted, two rewritten).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

Constitution v4.0.1.

| Principle | Applies | Finding |
|---|---|---|
| **I. Pure Rust, Statically Linked** | yes | No new dependency of any kind, so the linkage baseline is untouched. The one new Python file is a measurement probe under `specs/`, not shipped and not built. **PASS** |
| **II. eBPF-Only Observation** | no | User-space emission only; `waybill-ebpf` is not touched. |
| **III. Fail Closed** | yes | FR-011 is this principle applied to vocabulary: a component matching no mechanism stays unexplained and keeps failing I7, and no catch-all value may be introduced. The failure mode this guards is a marker that explains everything and so distinguishes nothing. **PASS** |
| **IV. Type-Driven Correctness** | yes | The vocabulary is a Rust enum (`OrphanReasonCode`), not strings at call sites, so a new mechanism cannot be emitted without an `as_str()` arm and a `tally()` arm — the compiler enumerates the work. Test code using `.unwrap()` must sit under the existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention. **PASS** |
| **V. Specification Compliance** | yes | The native-field audit was done and is recorded on C45's row already: no native CDX/SPDX construct expresses *why* a component is unreachable, so the annotation is the Principle V finer-info carve-out. This feature adds values to that existing carve-out rather than inventing a parallel one — and FR-012 refuses C151 partly on this ground, since two annotations meaning the same thing is the parity liability Principle V guards against. **PASS** |
| **VI. Three-Crate Architecture** | yes | All changes in `waybill-cli`. No crate boundary moves; `waybill-common`'s `ResolvedComponent` is read, not changed. **PASS** |
| **VII. Test Isolation** | yes | The hermetic lane runs under plain `cargo test` with no privileges and no network. No new I/O is added (FR-006). The corpus lane is untouched. **PASS** |
| **VIII. Completeness** | yes | Directly served: the feature's whole purpose is that a document account for every component it contains. Deliberately *partial* on sixteen components, which is why SC-004 asserts they still fail rather than letting the count quietly improve. **PASS** |
| **IX. Accuracy** | yes | The governing tension of this feature, and the reason it is half the size #1214 implies. Explaining #1234's and #1235's components would be *inaccurate* — it would assert a property of the artifact that is in fact a property of waybill. Recorded as the deferral rationale rather than as a scope cut. **PASS** |
| **X. Transparency** | yes | FR-012a: the classifier's counts are computed and discarded at `scan_cmd.rs:4939` despite m167's own module header specifying the log. A scan where nothing matched and a scan where the classifier never ran are currently indistinguishable to an operator. **PASS** |
| **XI–XII. Enrichment / External Data** | no | No enrichment, no external source, no network. |
| **Development Workflow — Pre-PR Verification (MANDATORY)** | yes | `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain` argument. The constitution's table agrees since v4.0.1, so this row carries no caveat. **PASS** |

**Result**: no violations, so Complexity Tracking stays empty. Re-checked
after Phase 1: the design adds three enum variants, three predicates and one
log line, and removes two guards — nothing that trades a principle for
convenience. The one judgement that could be read as a Principle VIII
shortfall (leaving sixteen unexplained) is a Principle IX requirement, and
SC-004 makes it a tested commitment rather than an omission.

## Project Structure

### Documentation (this feature)

```text
specs/1218-explain-every-orphan/
├── spec.md                              # 23 FRs, 17 SCs, 6 clarifications
├── plan.md                              # this file
├── research.md                          # R1-R9, every claim line-cited
├── data-model.md                        # the vocabulary and the predicates
├── quickstart.md                         # how to verify each mechanism
├── contracts/
│   └── orphan-reason-vocabulary.md      # the three wire strings, frozen
├── measurements/
│   ├── i7-populations.md                # the 34, by mechanism
│   └── i7-populations.py                # offline probe, re-run at close
└── checklists/
    └── requirements.md                  # 16/16
```

### Source Code (repository root)

```text
waybill-cli/src/
├── generate/
│   └── orphan_reason.rs                 # the feature: +3 variants, +3 predicates, -2 guards
├── cli/
│   └── scan_cmd.rs                      # :4939 stop discarding the counts (FR-012a)
└── scan_fs/package_db/pip/
    └── mod.rs                           # emit C55 source-mechanism (FR-002a)

waybill-cli/tests/
└── corpus_harness_195/
    └── invariant-declarations.toml      # -3 entries, 2 rewritten

docs/reference/
├── sbom-format-mapping.md               # C45 row: drop the Go/npm restriction, add values + order
└── sbom-invariants.md                   # I7's prose, if it restates the restriction

specs/236-unresolved-reason/
└── spec.md                              # :185 reader inventory += bazel, nix/lockfile (FR-015)
```

Single project, existing layout. No new module, no new file in `src/`.

## Phase 0 — research

Complete. [research.md](./research.md) resolves nine questions; the four
that changed the design:

- **R1** — the classifier is not scope-gated, so feature 1217's FR-008a
  asymmetry has no analogue. Checked first, because that trap cost a whole
  requirement there.
- **R2** — `waybill:source-files` is *derived at emit time* from
  `evidence.source_file_paths` and relativized against the scan roots, so
  the classifier sees raw paths, not the spellings this feature's
  measurements quote. Predicates must match on file name or route through
  `emitted_source_files`.
- **R4** — deleting the guard widens the sibling index to every ecosystem,
  which mechanism B needs, but also lets `maven-guice`'s `guice@5.1.0` find
  its reachable `guice@7.0.0` sibling. A naive widening classifies #1235's
  three as `dead-lockfile-entry` and silently closes its cell.
- **R6** — C151 is design-tier-only and mechanism E is the only design-tier
  population, so FR-015's reader extension closes **no** cell. A plan that
  counted it toward SC-001 would come up five components short.

## Phase 1 — design

Complete: [data-model.md](./data-model.md),
[contracts/orphan-reason-vocabulary.md](./contracts/orphan-reason-vocabulary.md),
[quickstart.md](./quickstart.md). Agent context updated.

The design's shape in one line: a totally ordered list of predicates over
`(sbom_tier, source paths, workspace-member, source-mechanism, ecosystem,
has_reachable_sibling)`, evaluated most-specific-first, with the three new
arms placed strictly below m167's so FR-004 holds by position rather than by
case analysis.

## Phase 2 — task generation approach

`/speckit.tasks` will produce phases in this order. The sequencing
constraint worth stating now: **the vocabulary contract must be frozen
before any emission task**, because consumers key on the wire strings, and
**the guard deletion must land with the ordering rule in the same task**,
because the window between them is exactly when #1235's three get
misclassified.

1. **Setup & foundational** — freeze the three wire strings in the contract;
   add the enum variants, `as_str()` arms, counts fields and `tally()` arms
   with no predicate wired, so the type surface exists before any behaviour.
2. **US1 (P1)** — mechanism B. Delete both guards **and** add the ordering
   rule in one change; widen the sibling index; assert `maven-guice`'s three
   are *not* classified (SC-004's first half).
3. **US2 (P2)** — mechanisms C and D, plus FR-002a's pip C55 emission.
   Closes `go-opentelemetry` and `haskell-security-advisories`.
4. **US3 (P3)** — mechanism E. Closes `haskell-language-server`.
5. **Record** — C45's row, the three declaration deletions, the two
   rewrites, m236's inventory, the `cmake.rs` note (FR-016).
6. **Falsifiability** — the positive control, the per-value mutation tests,
   the counts log, and the closing probe re-run.

MVP is phases 1–2. Each of 2, 3 and 4 is independently shippable, and 3 and
4 are the ones that flip cells.

## Complexity Tracking

Empty. No principle is traded, no new dependency is added, and no
abstraction is introduced that the existing enum does not already imply.
