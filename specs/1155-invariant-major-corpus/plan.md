# Implementation Plan: Universal Semantic-Invariant Layer for the Public Corpus

**Branch**: `1155-invariant-major-corpus` | **Date**: 2026-10-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1155-invariant-major-corpus/spec.md`

## Summary

Add the missing middle layer to the public corpus harness: a catalog of
semantic invariants, each evaluated against every corpus target, with a
committed declarations file as the gate and a per-run matrix artifact
for reading a fix's reach.

The harness already proves the universal-checker shape works —
`layer0_document_integrity` runs against all 17 targets — but it only
asks whether a document is internally consistent, never whether the
graph it describes is meaningful. Every semantic check lives in one of
17 per-target functions, so a defect class is checked only where someone
noticed it.

Technical approach: a new `invariants/` module inside the existing
harness, holding a registry of `Invariant` rows whose checkers return a
cell outcome (pass / fail-with-kinds / not-applicable / error) rather
than the harness's existing short-circuiting `Result<(),
AssertionFailure>`. The catalog is a markdown table in
`docs/reference/`, gated bidirectionally against the registry exactly as
`every_catalog_row_has_an_extractor` gates the C-row catalog. Zero new
Cargo dependencies; no change to how documents are produced.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. Test-only code; the shipped `waybill` binary is untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (document walking, artifact emission), `toml = "0.8"` (declarations file; already a direct dep used by `cargo.rs`, pip and pants readers), `regex` (catalog doc parsing; already direct since m013). **Zero new Cargo dependencies.**
**Storage**: One committed file, `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml` (non-passing cells only). One uncommitted per-run artifact, `target/invariants/run-<sha>.json`.
**Testing**: `cargo test` for the registry/catalog/declaration gates (hermetic, no network). **Revised during implementation**: the matrix is hermetic too. The 17 committed golden triples under `waybill-cli/tests/fixtures/public_corpus/` carry `dependencies[]`, every `waybill:*` property and both subject pointers, and layer 2's masking touches only hashes, serial numbers, timestamps and the workspace path — none of which any invariant reads. So the full 7 x 17 matrix, the gate and the mutation check all run in the default lane. The dispatched lane additionally evaluates freshly emitted documents; the two are complementary, since the goldens catch a refresh that imports a defect and the lane catches a defect before a refresh can hide it.
**Target Platform**: Same as the corpus lane — Linux CI runner and local macOS/Linux dev hosts.
**Project Type**: Internal test infrastructure for a Rust CLI workspace.
**Performance Goals**: The layer adds no scans, no network calls and no container pulls (FR-016 / SC-008); it reads documents the run already emitted. Evaluation of ~119 cells over in-memory JSON is expected to be immaterial against a corpus run measured in minutes — asserted as "no new I/O", not as a latency figure.
**Constraints**: Must not enter `scripts/pre-pr.sh`, whose two mandatory commands stay as `CLAUDE.md` specifies. Must not alter emitted SBOMs, so no golden churn from this feature.
**Scale/Scope**: 17 corpus targets × 7 initial invariants = 119 cells. The declarations file holds only non-passing cells; 12 are known failing on the nine measured targets, with 8 of the 17 targets unmeasured.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | **PASS.** The delivered layer is Rust in the existing test harness. The Python probe at `measurements/invariants.py` is a measurement artifact committed under `specs/` for reproducibility, per the measurement guidance in `CLAUDE.md`; it is not built, not shipped, and not the deliverable. Deliberately kept out of the build so the instrument never shares code with its subject. |
| **IV. Type-Driven Correctness** | **PASS, and load-bearing.** Cell state is an enum, not a bool — the whole point of FR-003 is that *not-applicable* cannot collapse into *pass*. Violation kinds are typed constants per invariant (FR-012a), so a checker that can only produce a count fails to compile into the registry. |
| **V. Specification Compliance** | **N/A.** This feature emits no SBOM fields and invents no `waybill:` annotation. It is a gate on compliance, not a participant in it. |
| **VII. Test Isolation** | **PASS.** No eBPF, no privileges. The registry, catalog and declaration gates are hermetic `cargo test` units; the corpus matrix inherits the existing dispatched lane's gating unchanged (spec Assumption 3). |
| **VIII. Completeness / IX. Accuracy / X. Transparency** | **PASS — this feature serves them.** Each initial invariant is a machine check on a principle that is currently asserted only by prose and per-target spot checks. |
| **Zero new dependencies** | **PASS.** FR-016's sibling constraint; all four crates used are already direct deps. |

**No violations. Complexity Tracking section omitted.**

One build-level note carried into Phase 1: integration tests under
`waybill-cli/tests/` are separate crates and do not inherit the
`waybill-cli` crate root's `deny(clippy::unwrap_used)`. The existing
harness nonetheless avoids `unwrap` in most paths; new code follows the
surrounding style rather than relying on the lint boundary.

## Project Structure

### Documentation (this feature)

```text
specs/1155-invariant-major-corpus/
├── spec.md                      # /speckit.specify + /speckit.clarify output
├── plan.md                      # This file
├── research.md                  # Phase 0 output
├── data-model.md                # Phase 1 output
├── quickstart.md                # Phase 1 output
├── contracts/
│   ├── invariant-catalog.md     # catalog doc format + the bidirectional gate
│   └── declarations.md          # committed declarations file contract
├── checklists/requirements.md   # spec quality checklist
├── measurements/
│   ├── invariants.py            # the probe behind every number in the spec
│   └── findings.md              # method, matrix, limitations
└── tasks.md                     # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/tests/corpus_harness_195/
├── invariants/
│   ├── mod.rs                   # Invariant row, REGISTRY, cell evaluation driver
│   ├── catalog.rs               # parse docs/reference/sbom-invariants.md; bidirectional gate
│   ├── declarations.rs          # load + compare invariant-declarations.toml
│   ├── report.rs                # console table + target/invariants/run-<sha>.json
│   └── checks/
│       ├── mod.rs
│       ├── file_tier_edges.rs       # I1
│       ├── identity_unique.rs       # I2
│       ├── scope_propagation.rs     # I3  (restated per FR-019)
│       ├── root_edge_parity.rs      # I4
│       ├── completeness_honesty.rs  # I5
│       ├── version_sentinels.rs     # I6
│       └── orphan_explained.rs      # I7  (restated per FR-019)
├── invariant-declarations.toml  # COMMITTED gate; non-passing cells only
├── expectations.rs              # was layer1_assertions.rs (FR-020 rename)
├── manifest.rs                  # `layer1` field → `expectations`; + `reader_ecosystems`
├── harness.rs                   # AssertionFailure.invariant_name → check_name
├── cache.rs  js_filter.rs  layer2_golden.rs   # untouched
└── mod.rs

docs/reference/sbom-invariants.md    # the catalog; sibling of sbom-format-mapping.md
waybill-cli/tests/public_corpus.rs   # drives the new layer alongside layer 0 + expectations
```

**Structure Decision**: The invariant layer lands *inside* the existing
`corpus_harness_195` module rather than as a new test crate, because it
consumes the same `EmittedSboms` the harness already produces and must
run in the same lane. `checks/` holds one file per invariant so a new
invariant is an added file plus a registry line plus a catalog row —
which is the shape that makes US1 cheap and is the whole point of the
feature. `layer0_document_integrity` stays where it is (spec Assumption
6) and is not re-homed, though the terminology rename reaches its
failure construction.

`CorpusTarget` gains a second, differently-shaped ecosystem field rather
than having its existing one widened. `ecosystem: Ecosystem`
(`manifest.rs:15`) classifies the target — one of seven coarse
categories, one of which (`PolyglotImage`) is not an ecosystem at all —
and `cross_ecosystem_coverage_check` asserts against it. FR-015 needs a
different question answered: which reader ecosystems does this target
exercise, named as `docs/ecosystems.md` names them. Conflating the two
would either break the m195 check or force `PolyglotImage` into a
coverage universe it does not belong in.

## Phase 0 — Research

Nine decisions, all recorded in [research.md](./research.md). The three
that most shape the implementation:

- **The result shape must change, but not the existing signature.** FR-004 cannot be satisfied by `Result<(), AssertionFailure>`, which reports one failure and stops. The invariant layer gets its own outcome type; `CorpusTarget`'s per-target hook keeps its signature and is merely renamed, so US3's migration is a move between two mechanisms rather than a rewrite of both.
- **The declarations file is TOML, not the plain-text allowlist shape.** m115/m117's `walk.audit-allowlist.txt` is the precedent for *what is committed* (non-passing entries only) but not for *how*: a declaration carries a reason, a pinned violation kind, a count and an optional issue, which a `<file>:<content>` line cannot hold. Sorted array-of-tables keyed by `(invariant, target)` satisfies FR-014c's line stability.
- **Ecosystem coverage is derived, not hand-listed.** `docs/ecosystems.md` already carries a coverage matrix that can be parsed exactly as the parity gate parses its catalog. Notably that doc's own prose claims "all nine ecosystems Waybill supports" while ~40 reader modules exist — direct evidence that a second hand-maintained list would rot, and a defect worth reporting separately.

## Phase 1 — Design & Contracts

- [data-model.md](./data-model.md) — `Invariant`, `CellOutcome`, `ViolationKind`, `Declaration`, `MatrixReport`, and the state machine a cell moves through, including which transitions fail the lane.
- [contracts/invariant-catalog.md](./contracts/invariant-catalog.md) — the catalog doc's row format, the `I<n>` numbering rules (append-only, retire-never-reuse), and the bidirectional registry gate.
- [contracts/declarations.md](./contracts/declarations.md) — the committed file's schema, the kind-pinned comparison, and the four conditions that fail the lane.
- [quickstart.md](./quickstart.md) — adding an invariant, declaring a known failure, reading the matrix, and the mutation check each new invariant must pass.

## Constitution Re-Check (post-design)

Re-evaluated against the Phase 1 artifacts. **No new violations.**

| Principle | Post-design assessment |
|---|---|
| **I. Pure Rust** | Still PASS, and the design strengthens the separation: the Python probe stays under `specs/` as a measurement artifact while the delivered checkers are Rust in `invariants/checks/`. Research R8 deliberately declined a build script, so nothing new enters the build graph. |
| **IV. Type-Driven Correctness** | Strengthened. `CellOutcome` makes *not-applicable* unrepresentable as *pass*, and the cell verdict table in `data-model.md` is exhaustive over (outcome × declaration) with every lane-failing transition named. The quickstart's checker skeleton puts the precondition first, returning `NotApplicable`, so the vacuity trap is hard to fall into by accident. |
| **V. Specification Compliance** | Still N/A — no SBOM field, no `waybill:` annotation. Confirmed by the data model: the only persisted shapes are a test fixture and a `target/` artifact. |
| **VII. Test Isolation** | Still PASS, and now split explicitly: the catalog↔registry and declaration-schema gates are hermetic `cargo test` units (`contracts/invariant-catalog.md`), while the matrix needs the dispatched lane. `scripts/pre-pr.sh` is untouched. |
| **VIII / IX / X** | Still served. Each initial invariant is a machine check on a principle currently asserted by prose. |
| **Zero new dependencies** | Confirmed across all Phase 1 artifacts: `serde_json`, `toml`, `regex` — all already direct deps. |

Two design decisions worth flagging to a reviewer as deliberate
deviations from the nearest precedent, neither a constitution issue:

1. **The declarations file is TOML, where the walker-audit allowlist is plain text.** Spec Assumption 2 adopts the allowlist *model*, not its format; a declaration carries four fields a positional line cannot hold (research R2).
2. **The bidirectional catalog gate exceeds FR-008**, which only requires rows-without-checkers. Reading `every_catalog_row_has_an_extractor` showed it already asserts both directions, and a checker with no catalog row is an invariant nobody can review (research R5).

## Complexity Tracking

Not applicable — the Constitution Check records no violations, before or after design.
