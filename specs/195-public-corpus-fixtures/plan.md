# Implementation Plan: Public SBOM Regression Corpus

**Branch**: `195-public-corpus-fixtures` | **Date**: 2026-07-14 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/195-public-corpus-fixtures/spec.md`

## Summary

Move the Kusari-private pico corpus pattern into mikebom's own public repo as a
6-target regression corpus that scans public source repos + at least one polyglot
container image and asserts a **hybrid two-layer** invariant model per target:
Layer 1 = code-defined coarse assertions with class-of-bug-oriented diagnostics
(fast fail); Layer 2 = full-SBOM byte-identity golden (drift catcher). Corpus is
opt-in — gated behind an environment variable in the cargo test path AND runs on
a dedicated CI workflow (nightly `schedule` against `main` + `workflow_dispatch`
against any branch). Zero addition to the default `./scripts/pre-pr.sh` inner
loop.

Delivered as a new `mikebom-cli/tests/public_corpus.rs` integration test module
plus a `mikebom-cli/tests/fixtures/public_corpus/` snapshot tree, a
`.github/workflows/public-corpus.yml` cron/dispatch workflow, and a
`scripts/corpus/refresh-pins.sh` helper for pin refreshes. Zero new Cargo
dependencies, zero new `mikebom:*` annotations.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–194; no nightly required for this user-space-only test-infra work).
**Primary Dependencies**: Existing only — `std::process::Command` (subprocess spawn for `git clone` + `docker pull` — same pattern as milestone 053's `git describe` ladder, milestone 090's `git` shell-out, milestone 173's Go cache-warmer), `serde` / `serde_json` (invariant-file + SBOM parse), `tempfile` (per-target scratch dir), `sha2` + `data-encoding` (pin verification), `tracing` (info/warn logs), `anyhow` / `thiserror` (error classes). Reuses milestone-090's `~/.cache/mikebom/fixtures/<sha>/` cache pattern verbatim (extended to `~/.cache/mikebom/corpus/<pin>/`). **Zero new Cargo dependencies.**
**Storage**: Per-user cache at `~/.cache/mikebom/corpus/<source-id>/<pin>/` where `<source-id>` is a stable hash of the source URL and `<pin>` is the commit SHA or image digest. Cache-key layout mirrors milestone 090's fixture cache exactly. Golden SBOMs live in-repo at `mikebom-cli/tests/fixtures/public_corpus/<target-name>/{cdx,spdx-2.3,spdx-3}.json`.
**Testing**: `cargo test --test public_corpus` (opt-in via `MIKEBOM_RUN_PUBLIC_CORPUS=1` env var). Integration tests spawn the pre-built `mikebom` binary via `env!("CARGO_BIN_EXE_mikebom")` (matches the milestone-101 Windows smoke pattern). Corpus tests skip with a `println!` when the env gate is off.
**Target Platform**: Same as mikebom itself — Linux + macOS (any x86_64 or arm64 host). Windows corpus target support is out of scope for MVP (matches milestone 100/101 Windows-experimental posture — corpus is nightly-Linux-first).
**Project Type**: Test-infrastructure augmentation. No changes to library code, CLI surface, or emitted SBOM shape.
**Performance Goals**: End-to-end corpus run (cold cache) completes in under 30 minutes on a standard laptop per SC-005. Warm-cache re-runs (repos already cloned, images already pulled) complete in under 5 minutes.
**Constraints**: (a) zero new Cargo deps; (b) no impact on default `./scripts/pre-pr.sh` runtime per SC-004; (c) public-only source URLs per FR-003 / FR-004 — no `*.kusari.*` hostnames anywhere in the manifest; (d) reproducibility: two consecutive runs same-input MUST be byte-identical per SC-006; (e) `git` is a hard prereq (existing mikebom dev-setup assumption); (f) `docker` (or equivalent OCI-pull tool) is a hard prereq per Q3 clarification for the container-image target.
**Scale/Scope**: 6 initial corpus targets (5 source-tree + 1 container-image) sized so cold-cache scan wall-clock fits inside SC-005's 30-min budget. Extensible to more targets in follow-up milestones without harness changes.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. Corpus harness is pure Rust (integration tests + `std::process::Command` shell-outs to `git`/`docker`). No new C toolchains. `git` and `docker` are external prerequisites, not mikebom-shipped dependencies (matches milestone 090 for `git`, milestone 165 for `docker`).
- **II. eBPF-Only Observation** — ✅ N/A. This feature is test infrastructure; no discovery code path.
- **III. Fail Closed** — ✅ PASS. Corpus MUST fail loudly on any invariant mismatch or corpus-infra failure per FR-009 / FR-012. Two distinct failure classes surface with distinct diagnostics.
- **IV. Type-Driven Correctness** — ✅ PASS. Corpus target manifest is a typed struct (`CorpusTarget { name, source: SourceKind, pinned: PinnedRef, ecosystem: Ecosystem, invariants: InvariantSet }`) with enum-driven source-kind (git vs OCI) and enum-driven ecosystem tag. No stringly-typed manifest.
- **V. Specification Compliance** — ✅ PASS. Corpus does not introduce new `mikebom:*` annotations, new PURL types, or new format extensions. It **consumes** the standard emitted output; it does not extend the emitter.
- **VI. Three-Crate Architecture** — ✅ PASS. All corpus code lives in `mikebom-cli/tests/` (integration test target of the CLI crate). No changes to `mikebom-common` or `mikebom-ebpf`.
- **VII. Test Isolation** — ✅ PASS. Corpus tests spawn the released binary through `env!("CARGO_BIN_EXE_mikebom")` (same pattern as milestone 101); per-target scratch dirs via `tempfile::tempdir()`. No cross-test state pollution.
- **VIII. Completeness** — ✅ PASS. The corpus IS the Completeness protection layer for real-world tree shapes — it's an instrument that measures the Completeness principle in action.
- **IX. Accuracy** — ✅ PASS. Full-SBOM golden byte-identity catches any drift; coarse assertions catch class-of-bug regressions.
- **X. Transparency** — ✅ PASS. Corpus emits structured diagnostics per FR-009 identifying (a) target, (b) invariant, (c) observed vs expected, (d) suggested next action.
- **XI. Enrichment** — ✅ N/A.
- **XII. External Data Source Enrichment** — ✅ N/A.
- **Strict Boundary §5 (file-tier)** — ✅ PASS. Corpus does NOT change file-tier emission mode. Whatever mikebom's default emits, that's what the corpus asserts against.

**Result**: All principles PASS. No violations to justify in Complexity Tracking.

**Post-Phase-1 re-check** (per template):

Every entity and contract shape designed in Phase 1 (research.md, data-model.md, contracts/, quickstart.md) is compatible with every principle. Specifically:

- The typed `CorpusTarget` manifest (data-model.md Entity 1) directly satisfies Principle IV (Type-Driven Correctness) — no stringly-typed target definitions.
- The distinct `AssertionFailure` vs `CorpusInfraError` shapes (Entities 3 + 5) directly satisfy FR-012 and Principle X (Transparency) — the two failure classes never conflate.
- The Layer 1 assertion functions per target (research §R4 + contracts/corpus-harness.md) enable class-of-bug-oriented diagnostics per FR-009.
- The `~/.cache/mikebom/corpus/` layout mirrors milestone 090's fixture cache exactly (research §R3) — no new operator-facing cache convention.
- Layer 2 reuses existing golden-diff masking (research §R5) — no duplication of the mikebom-established byte-identity idiom.
- Nightly + workflow_dispatch CI shape (research §R6) enforces "no impact on default per-PR lane" per SC-004 and FR-006 / FR-006a.
- Zero new Cargo dependencies confirmed by the research design — every helper is `std::process::Command` + existing workspace crates.

No new constitution risks surfaced during Phase 1. All gates remain PASS.

## Project Structure

### Documentation (this feature)

```text
specs/195-public-corpus-fixtures/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 6 target-selection decisions + harness design decisions
├── data-model.md        # Phase 1 output — CorpusTarget, InvariantSet, CorpusCache types
├── quickstart.md        # Phase 1 output — how to run the corpus locally + refresh pins
├── contracts/
│   └── corpus-harness.md  # Corpus-harness invocation surface: env vars + exit codes + diagnostic shape
├── checklists/
│   └── requirements.md  # Spec-quality checklist (already created)
├── scratch/             # Working notes; not part of the spec
└── tasks.md             # Phase 2 output (created by /speckit-tasks, NOT this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── tests/
│   ├── public_corpus.rs                # NEW — top-level integration test entry
│   ├── public_corpus/                  # NEW — sub-module for corpus harness code
│   │   ├── mod.rs                      # NEW — harness: clone/pull, invoke mikebom, apply layers
│   │   ├── manifest.rs                 # NEW — typed CorpusTarget definitions (const table)
│   │   ├── layer1_assertions.rs        # NEW — per-target coarse-assertion functions
│   │   ├── layer2_golden.rs            # NEW — full-SBOM byte-identity comparison (reuses cdx_regression pattern)
│   │   └── cache.rs                    # NEW — ~/.cache/mikebom/corpus/ layout + cache-key hashing
│   └── fixtures/
│       └── public_corpus/              # NEW — golden snapshots per target per format
│           ├── go-cobra/
│           │   ├── cdx.json
│           │   ├── spdx-2.3.json
│           │   └── spdx-3.json
│           ├── rust-ripgrep/{cdx,spdx-2.3,spdx-3}.json
│           ├── npm-express/{cdx,spdx-2.3,spdx-3}.json
│           ├── python-flask/{cdx,spdx-2.3,spdx-3}.json
│           ├── maven-guice/{cdx,spdx-2.3,spdx-3}.json
│           └── image-postgres16/{cdx,spdx-2.3,spdx-3}.json

scripts/
└── corpus/
    ├── refresh-pins.sh                 # NEW — helper: resolve upstream HEAD → commit SHA + image digest
    └── regen-goldens.sh                # NEW — helper: run corpus with MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1

.github/workflows/
└── public-corpus.yml                   # NEW — schedule (nightly UTC) + workflow_dispatch job
```

**Structure Decision**: Test-infrastructure augmentation inside the existing `mikebom-cli` crate. Zero changes to library code paths, CLI surface, or `mikebom-common`. The corpus lives as a cargo integration-test target (opt-in via `MIKEBOM_RUN_PUBLIC_CORPUS=1`) mirroring the milestone-101 Windows-experimental smoke-test pattern (env-gated integration test that skips silently when the gate is off). Golden fixtures per target per format live under `mikebom-cli/tests/fixtures/public_corpus/` following the existing golden-regression convention. A dedicated GitHub Actions workflow at `.github/workflows/public-corpus.yml` handles the nightly-`main` schedule + manual `workflow_dispatch`.

## Complexity Tracking

No constitution violations to justify. All principles pass on first check.
