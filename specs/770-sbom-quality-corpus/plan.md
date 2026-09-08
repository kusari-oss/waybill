# Implementation Plan: Nightly SBOM Quality Regression Corpus

**Branch**: `770-sbom-quality-corpus` | **Date**: 2026-09-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/770-sbom-quality-corpus/spec.md`

## Summary

Add a `quality` subcommand to the existing `xtask` task-runner crate that reads a committed
TOML corpus of pinned public git repositories, shallow-fetches each at its pinned commit,
scans it with `waybill --offline`, and records four measurement families: scan wall time,
`sbomqs` quality score, component counts split package-tier / file-tier, and an
independently computed flatness triple (relationship count, components with outgoing
relationships, greatest depth from root). Each measurement may carry a hand-authored
acceptable range; any observation outside its range fails the run. A nightly workflow
invokes it and retains the JSON report.

The design reuses three things already in the tree: the m195 corpus cache layout, the
`sbomqs` discovery-and-pin pattern from `waybill-cli/tests/sbomqs_parity.rs`, and the m669
`xtask bench` shape for CLI flags, atomic report writes, and workflow wiring.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–676; no nightly required — user-space, dev-tooling-only work).
**Primary Dependencies**: Existing only — `clap`, `serde`/`serde_json`, `chrono`, `tempfile` (all already in `xtask/Cargo.toml`), plus `toml = "0.8"` promoted into `xtask/Cargo.toml`. `toml` is already in the workspace lockfile via `waybill-cli`, so this adds **zero new transitive dependencies at the lockfile level** and nothing to the shipped `waybill` binary. External runtime tools: `git` and `sbomqs` (version-pinned; same shell-out posture as `spdx3-validate` in m078 and `trivy`/`syft` in m083).
**Storage**: Corpus config committed at `xtask/corpus/quality-corpus.toml`. Per-run reports at `target/quality/run-<waybill-sha>.json` (gitignored). Repository cache at `~/.cache/waybill/quality-corpus/<target-name>/<pinned-sha>/`, mirroring the m090 fixture-cache and m195 corpus-cache layout. CI restores no Actions cache for it (research R6).
**Testing**: `cargo +stable test --workspace`. Unit tests cover range evaluation, flatness computation, corpus parsing and malformed-range rejection — all hermetic, no network. The single end-to-end test that fetches a repository is environment-gated so the default gate stays offline (Constitution VII).
**Target Platform**: Linux x86_64 for the scheduled job (reference architecture, matching m669); macOS and Windows for local developer runs.
**Project Type**: Developer tooling — a subcommand of the existing `xtask` crate. Not shipped to users, not linked into the `waybill` binary.
**Performance Goals**: Full corpus within the scheduled job's 90-minute budget. Measured cold cost is ~95 s of fetching plus ~140 s of scanning across 18 targets; dominated by `go-kubernetes` (~103 s) and by building `waybill` itself.
**Constraints**: Every scan runs with network access disabled. Only the scan is timed. No recursive sub-repository retrieval. Adding a repository must be a configuration-only change.
**Scale/Scope**: 18 repositories at first landing, ~2.2 GB of working disk, four measurement families each independently rangeable, one new subcommand, one new workflow file.

## Constitution Check

*GATE: evaluated against `.specify/memory/constitution.md` (12 principles). Re-checked after
Phase 1 design — result unchanged.*

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | **PASS** | All new code is Rust. `sbomqs` and `git` are invoked as external processes, never linked. Precedent: m078 (`spdx3-validate`), m083 (`trivy`/`syft`), m053/m173 (`git`, `go`). |
| II. eBPF-Only Observation | **N/A** | This feature performs no dependency discovery. It measures documents waybill already produced. |
| III. Fail Closed | **PASS — reinforced** | FR-016 fails the run when `sbomqs` is absent rather than scoring it as passing; FR-007 fails on unfetchable targets; FR-019 exits non-zero on any violation. A missing signal is never a passing signal. |
| IV. Type-Driven Correctness | **PASS** | `TargetName`, `MetricRange`, and `Flatness` are newtypes/enums, not raw `String`/`bool` pairs across boundaries. No `unwrap()` in non-test code; test modules carry the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard per house convention. |
| V. Specification Compliance | **N/A** | Consumes CycloneDX; emits none. Introduces no `waybill:*` annotation, so the bullet-6 format audit is not engaged. |
| VI. Three-Crate Architecture | **PASS with documented pre-existing tension** | This feature adds **no crate**. See Complexity Tracking. |
| VII. Test Isolation | **PASS** | Unit tests need no privileges and no network. The single network-dependent end-to-end test is environment-gated, matching the m195 corpus-suite posture. |
| VIII. Completeness | **PASS with documented caveat** | Offline scanning means the gate measures waybill's offline floor and cannot observe online-ladder improvements. Recorded as a spec Assumption and in research R2 so a reviewer is not left to infer it. |
| IX. Accuracy | **N/A** | No component resolution performed. |
| X. Transparency | **PASS — reinforced** | FR-013 records waybill's self-assessment alongside the independent measurement precisely so divergence is visible; FR-023/024 require expected-vs-observed in every violation; unmeasurable targets are reported distinctly from failing ones. |
| XI. Enrichment | **N/A** | No enrichment path touched. |
| XII. External Data Source Enrichment | **N/A** | `sbomqs` scores an existing document; it introduces no components and feeds nothing back into SBOM generation. |

## Project Structure

### Documentation (this feature)

```text
specs/770-sbom-quality-corpus/
├── plan.md              # This file
├── spec.md              # Phase -1 output
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── corpus-config.md     # The committed TOML corpus schema
│   ├── quality-report.md    # The emitted JSON report schema
│   └── xtask-quality-cli.md # Subcommand flag surface + exit codes
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
xtask/
├── Cargo.toml                      # + toml = "0.8" (already in workspace lockfile)
├── corpus/
│   └── quality-corpus.toml         # NEW — the committed corpus + hand-authored ranges
└── src/
    ├── main.rs                     # + Cli::Quality(quality::QualityArgs) variant
    └── quality/                    # NEW module
        ├── mod.rs                  # CLI args, orchestration, exit-code policy
        ├── config.rs               # TOML corpus parsing + validation (dup names, bad ranges)
        ├── fetch.rs                # shallow fetch at pinned sha; cache reuse
        ├── measure.rs              # timed waybill invocation; per-target timeout
        ├── analyze.rs              # component split + flatness triple from the CDX document
        ├── score.rs                # sbomqs discovery, version pin check, score extraction
        ├── evaluate.rs             # range comparison → violations
        └── report.rs               # JSON report + human summary rendering

.github/workflows/
└── quality-corpus.yml              # NEW — nightly + workflow_dispatch
```

**Structure Decision**: The feature lives entirely inside the existing `xtask` crate as a
sibling of the m669 `bench` module, which it deliberately mirrors in shape (module-per-concern,
`Args`-derive CLI struct, atomic JSON report write, a workflow that builds `waybill` release
then invokes the subcommand). No `waybill-cli`, `waybill-common`, or `waybill-ebpf` file is
touched. The corpus TOML lives beside the code that reads it rather than in the sibling
fixtures repository, because it is hand-edited configuration under review — not bulk test data.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Constitution VI caps the workspace at three crates; `xtask` is a fourth member with no recorded amendment | This feature adds **no** crate — it extends `xtask`, which m669 introduced. The tension is pre-existing and is surfaced here so a reviewer citing VI has the answer in front of them rather than having to litigate it in review. | Putting the code in `waybill-cli` instead would satisfy VI's letter but drag `toml`, corpus parsing, and subprocess orchestration into the shipped binary — violating FR-032 and Principle VI's own "prevents premature modularization" rationale far more seriously. Whether `xtask`'s own existence warrants a retroactive amendment is out of scope for this milestone and is flagged for the maintainers. |
| `toml = "0.8"` added to `xtask/Cargo.toml` | The corpus is hand-authored configuration whose ranges need explanatory comments; JSON cannot carry them. | JSON via the already-present `serde_json` was rejected specifically because a reviewer cannot record *why* a bound is what it is. The dependency is already in the workspace lockfile via `waybill-cli`, so the true cost is zero new transitive crates and nothing in the shipped binary. |
