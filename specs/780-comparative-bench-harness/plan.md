# Implementation Plan: Private comparative benchmark harness

**Branch**: `780-comparative-bench-harness` | **Date**: 2026-09-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/780-comparative-bench-harness/spec.md`

## Summary

Build `xtask compare`: a private, deterministic harness that measures waybill
against operator-nominated SBOM tools and refuses to answer when it cannot
answer reliably.

Phase 0 changed the shape of the work substantially. The statistical
machinery the spec implies — repeats, medians, spread gates — turns out to be
needed for **one** metric family, because this project has already
established that count metrics are exactly reproducible and wall-clock is
not usable as a gated metric at all. That halves the hard part and moves the
design's centre of gravity from "measure timing carefully" to "count the
right things and prove you counted them right".

## Technical Context

**Language/Version**: Rust stable, workspace toolchain. `xtask` only — the
shipped waybill binary is untouched.
**Primary Dependencies**: Existing `xtask` deps only — `clap`, `serde`,
`serde_json`, `chrono`, `tempfile`, `sysinfo`, plus `toml` (already promoted
into `xtask/Cargo.toml` by m770). **No new dependencies**, and deliberately
**not** `waybill-common` (research.md R4 — an instrument must not share code
with its subject).
**Storage**: `target/compare/run-<timestamp>.json`, gitignored. Targets cached
by the existing m770 pinned-SHA fetcher. Operator config at
`xtask/compare/tools.local.toml`, gitignored.
**Testing**: `cargo +stable test -p xtask`. The known-answer fixture is itself
the harness's runtime self-check and is additionally asserted in unit tests.
**Target Platform**: Developer machines primarily; reference-class hosts
produce a stronger verdict but are not required for the metrics that matter.
**Project Type**: Development tooling (xtask), sibling to `xtask bench` and
`xtask quality`.
**Performance Goals**: None for the harness itself.
**Constraints**: Output must be unpublishable by construction — public
repository, so committed files and CI artefacts are both world-readable.
Committed source must name no specific competing tool.
**Scale/Scope**: One new xtask subcommand; ~6 modules; one committed fixture;
one example config. No changes outside `xtask/`, `.gitignore`, and this spec
directory.

## Constitution Check

*GATE: evaluated before Phase 0 and re-evaluated after Phase 1. Both passes
reached the same verdicts.*

| Principle | Verdict | Notes |
|---|---|---|
| I. Pure Rust, Zero C | PASS | xtask is Rust. Measured tools are external binaries invoked as subprocesses — the same posture the constitution already accommodates for `spdx3-validate` (m078), the external scanners in the m083 audit harness, and `sbomqs` (m770). Nothing is linked. |
| II. eBPF-Only Observation | N/A | No dependency discovery. The harness reads tools' emitted output. |
| III. Fail Closed | **PASS, and it is the organising principle** | The harness withholds its verdict rather than guessing whenever a precondition fails, and aborts entirely if its own self-check fails. FR-011 forbids scoring a failed tool as having found nothing — the fail-open reading. |
| IV. Type-Driven Correctness | PASS | `PackageIdentity` is a dedicated type rather than a bare string, which is the whole mechanism by which FR-006 is enforced. `TruthMethod` and `NetworkMode` are enums, so a superset truth or an enriched timing cannot be silently treated as its opposite. No `.unwrap()` in non-test code. |
| V. Specification Compliance | N/A | Emits no SBOM. Reads CycloneDX produced by others. |
| VI. Three-Crate Architecture | PASS | Confined to `xtask`, which sits outside the three-crate boundary as `bench` and `quality` already do. |
| VII. Test Isolation | PASS | Pure logic — reduction, scoring, verdict — is unit-testable with no privileges and no network. Runs that invoke real tools are operator-initiated, never part of `cargo test`. |
| VIII. Completeness / IX. Accuracy | N/A | Measures others' completeness; asserts nothing about waybill's own emission. |
| X. Transparency | **PASS, strongly** | Every figure carries its conditions; the verdict states which preconditions failed. FR-017's prohibition on comparative adjectives is Principle X applied to the harness's own output — a consumer cannot act on a number whose reliability is hidden. |
| XI / XII. Enrichment | N/A | |

**Strict Boundaries**: none engaged. No lockfile discovery, no MITM, no C, no
production `.unwrap()`, no file-tier change.

**External-naming policy**: satisfied structurally rather than by judgement.
Committed source names no competing tool (FR-015); the comparison set lives
in gitignored operator configuration. This was the deciding factor in
choosing option C over naming tools in-tree, since at least one candidate
sits in the project's ambiguous tier.

**No violations. Complexity Tracking omitted as empty.**

## Project Structure

### Documentation (this feature)

```text
specs/780-comparative-bench-harness/
├── plan.md              # This file
├── spec.md
├── research.md          # Phase 0 — R1..R8
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   └── xtask-compare-cli.md
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit.tasks — not created here
```

### Source Code (repository root)

```text
xtask/
├── Cargo.toml                      # no new deps
├── src/
│   ├── main.rs                     # + Cli::Compare
│   └── compare/
│       ├── mod.rs                  # orchestration, interleaving, verdict
│       ├── config.rs               # ToolSpec / Target from TOML
│       ├── identity.rs             # PURL normalisation (hand-rolled, R4)
│       ├── truth.rs                # TruthMethod derivation
│       ├── score.rs                # found / missed / extra
│       ├── selfcheck.rs            # known-answer gate (FR-012)
│       └── report.rs               # stdout + target/compare/*.json
└── compare/
    ├── tools.example.toml          # placeholder names only (FR-015)
    ├── targets.toml                # pinned corpus, committed
    └── fixtures/
        └── known-answer-go/        # exact module set by construction

.gitignore                          # + xtask/compare/*.local.toml
                                    # + target/compare/
```

**Structure Decision**: a sibling of `xtask/src/bench/` and
`xtask/src/quality/`, reusing m770's pinned-SHA fetcher and m669's host-class
classification rather than reimplementing either. Nothing outside `xtask/`
changes except two `.gitignore` lines.

## Delivery order

1. **`identity.rs` + its tests.** The reduction rule is the feature's
   foundation and the source of its worst historical error. Written and
   tested first, against hand-authored expectations, including the
   two-versions-of-one-package case that discriminates the rule chosen in
   clarification from the one used in the ad-hoc comparison.
2. **`selfcheck.rs` + fixture.** The gate that lets the harness claim
   standing. Must fail loudly when `identity.rs` is deliberately broken —
   verified by doing exactly that.
3. **`truth.rs` + `score.rs`.** Derivation and scoring, with superset
   labelling. Pure functions, no subprocesses.
4. **`config.rs` + example.** Tool-agnostic config; assert the committed
   example names nothing real.
5. **`mod.rs` orchestration.** Interleaving, serialisation, verdict
   assembly.
6. **`report.rs`.** Output shaping, including the prohibition on comparative
   language.
7. **Wiring + gitignore + docs.**

Steps 1–4 need no external tool installed and no network, which is most of
the feature.

## Risks

| Risk | Handling |
|---|---|
| The harness reproduces the very errors it exists to prevent | Step 2 is a gate, not a test: the run aborts if the instrument cannot recover a known answer. Step 1's tests encode the specific historical error as a case. |
| Timing remains too noisy to be useful even as a ratio | Accepted and designed for. Ratios moved 1.38× across today's runs, which is better than absolutes' 1.53× but not by much (R1). Coverage and accuracy — the metrics that answer the real questions — are exact regardless. |
| Truth sets are supersets, so accuracy scores flatter over-reporting | Labelled at every point of use, in output and in the quickstart. Not solvable with offline-derivable truth; the honest response is to stop the reader misusing it. |
| Someone quotes an internal figure externally | FR-017 removes comparative language from the output; the quickstart ends with an explicit pre-flight check. This is mitigation, not prevention — the remaining exposure is a human one. |
| Scope creep into optimising waybill | Explicitly out of scope in the spec. The harness measures; it changes nothing it measures. |
