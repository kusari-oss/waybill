# Implementation Plan: Persisted reproducible benchmark suite

**Branch**: `669-bench-harness` | **Date**: 2026-08-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/669-bench-harness/spec.md`

## Summary

Extend the existing `xtask` crate with two new subcommands — `bench` (runs the fixture × mode matrix producing JSON + Markdown) and `bench-docs` (generates the operator-facing numbers page from a committed baseline) — plus a fixture-curation set in the sibling test-fixtures repo and a CI workflow that runs `xtask bench` on every release tag with regression detection against `docs/perf/baseline.json`. Zero runtime crates touched; all measurement code lives under `xtask/`.

Technical approach (5-PR arc per spec Assumptions):
1. **PR 1**: fixture curation in test-fixtures repo (file shuffling; ~150 LOC of fixture manifests + directory reorg)
2. **PR 2**: `xtask bench` driver — median-of-5 with warmup, `sysinfo` for peak RSS, per-fixture 5-min timeout (Q3), JSON emission at `target/bench/run-<sha>.json`, Markdown table on stdout (~450 LOC in `xtask/src/bench/`)
3. **PR 3**: JSON schema v1 + initial `docs/perf/baseline.json` + regression-comparison logic (`--baseline` flag → exit-code + diff artifact) (~200 LOC)
4. **PR 4**: `xtask bench-docs` — reads baseline.json, emits `docs/perf/numbers.md` with fixture-SHA + commit-SHA citations (~150 LOC)
5. **PR 5**: `.github/workflows/bench.yml` — runs suite on release tags, posts edit-in-place PR comment via `actions/github-script`, integrates release-prep pre-flight stale-baseline check per Q2 (~250 LOC of YAML)

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–668). No nightly required for `xtask/bench` code paths (nightly stays scoped to the `xtask/ebpf` command's kernel-side build). GitHub Actions YAML for the CI workflow.
**Primary Dependencies**: **One new dev-only Cargo dep** — `sysinfo = "0.39.6"` under `xtask/Cargo.toml` for cross-platform peak-RSS measurement (Linux/macOS/Windows-portable per SC-002 reproducibility target). Existing workspace crates reused: `clap` (subcommand parsing — already in xtask/Cargo.toml), `serde`/`serde_json` (JSON schema emission), `anyhow` (error propagation), `tempfile` (per-run scratch dirs — already dev-dep), `tracing` (progress logging), `chrono` (RFC 3339 timestamps for run metadata). Zero deps added to `waybill-cli`/`waybill-common`/`waybill-ebpf` (FR-015 + SC-009). External runtime deps at bench time: `git` (fetch fixture-cache from sibling repo — same pattern as m090 build.rs), a working `waybill` binary at `target/release/waybill` (self-referential; the tool being benchmarked).
**Storage**: Committed baseline at `docs/perf/baseline.json` in the waybill main repo. Per-run outputs at `target/bench/run-<git-sha>.json` (gitignored). Fixture cache at `~/.cache/waybill/fixtures/<sha>/` (existing m090 infrastructure; extended with `benchmark/` subdirectory per PR 1). Corpus cache at `~/.cache/waybill/fingerprints/<sha>/` (existing m108 infrastructure).
**Testing**: `cargo test --workspace` unchanged. New `xtask` crate tests under `xtask/tests/` for the JSON schema round-trip + regression-diff computation. The benchmark suite itself is exercised via a synthetic mini-matrix (2-fixture, 2-mode) in xtask tests to keep CI fast.
**Target Platform**: Reference architecture for docs citations = Linux x86_64 GitHub-hosted-runner class per spec Assumptions. Local runs supported on Linux/macOS/Windows via `sysinfo` cross-platform portability. Windows support is aspirational (matches m100's Windows-host build stance); the docs numbers page pins Linux x86_64.
**Project Type**: Development/CI tooling. Zero user-space runtime impact per FR-015 + SC-009.
**Performance Goals**: Full matrix on reference runner class ≤ 90 min per SC-008. Fixture-cache fetch on cache-miss ≤ 60 sec per SC-007. Reproducibility budget: same commit + fixture-SHA + host → medians within 25% per SC-002 (matches milestone-094 dual_format_perf posture).
**Constraints**: FR-015 (no user-space deps), FR-016 (xtask-only, no new top-level crate), FR-019 (fixtures in sibling test-fixtures repo, not main repo), Q3 (5-min per-fixture timeout default). SC-009 (zero Cargo diff at workspace-runtime layer — sysinfo lands under `xtask/Cargo.toml` only, not workspace root).
**Scale/Scope**: ~14 fixtures × ~5 modes = ~70 fixture-mode combinations per run. Baseline JSON size: ~30 KB. Numbers page: ~1000 lines Markdown. CI workflow runs weekly + on every release tag (2 release channels per m229).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Applicable principles for a dev-tooling feature that ships no runtime code**:

| Principle | Applies? | Status |
|---|---|---|
| I. Pure Rust, Zero C | Partial | ✅ `xtask/bench` code is pure Rust. `sysinfo` is pure Rust with optional platform bindings (uses `libc`/`windows-sys` under the hood but no C toolchain in waybill's build). Trivially compatible. |
| II. eBPF-Only Observation | No | Not an observation feature. |
| III. Fail Closed | **Yes** | ✅ FR-010 (release CI blocks on regressions) + FR-012 timeout (fixture-timeout fails release) + Q2 (release-prep fails loudly on stale baseline). Every failure path fails-closed by design. |
| IV. Type-Driven Correctness | Partial | ✅ JSON schema (data-model.md) uses typed Rust structs with `serde`; regression thresholds are `Duration` + typed dimension enums. No stringly-typed comparison logic. |
| V. Specification Compliance | No | Feature doesn't touch SBOM emission — pre-existing CISA 2026 conformance surface is unchanged. |
| VI. Three-Crate Architecture | **Yes** | ✅ FR-016 mandates all bench tooling lives under `xtask/`. Zero new top-level crates. Zero changes to `waybill-cli`/`waybill-common`/`waybill-ebpf`. |
| VII. Test Isolation | **Yes** | ✅ xtask tests use `tempfile::tempdir()` per test; no shared fixture-cache state between tests; bench-run scratch dirs are per-invocation. Matches existing perf-test isolation posture. |
| VIII. Completeness | No | Not an SBOM-completeness feature. |
| IX. Accuracy | Partial | ✅ Reproducibility target (SC-002 25% budget) is the accuracy anchor. All measured dimensions have documented sourcing. |
| X. Transparency | **Yes** | ✅ FR-013 (record waybill-commit-SHA + fixture-SHA per result) + FR-017 (host-metadata for noise weighting) + FR-014 (docs cite pinned pairs). Every quoted number traceable to source. |
| XI. Enrichment | No | Not an enrichment feature. |
| XII. External Data Source Enrichment | No | No external data source (fixtures are curated, corpus is pre-existing). |

**Verdict**: PASS. Six applicable principles honored explicitly by design. `sysinfo` is a pure-Rust cross-platform crate — no C toolchain, no libbpf, no FFI-safety concerns. Zero waivers required.

## Project Structure

### Documentation (this feature)

```text
specs/669-bench-harness/
├── plan.md              # This file
├── research.md          # Phase 0: sysinfo choice, JSON schema v1, comment-edit-in-place mechanism, RSS-measurement portability, fixture-manifest shape
├── data-model.md        # Phase 1: Fixture / Result / Run / Baseline / Regression Diff structs + JSON serialization
├── quickstart.md        # Phase 1: 5-step operator recipe (fresh checkout → first benchmark run → reproduce → refresh baseline → cite in docs)
├── contracts/
│   ├── json-schema.md            # Contract: baseline.json + run.json shape (versioned)
│   ├── xtask-bench-cli.md        # Contract: `xtask bench` + `xtask bench-docs` flag surface
│   └── ci-workflow.md            # Contract: `.github/workflows/bench.yml` step shape + regression-comment mechanism
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root — CHANGES ONLY)

```text
xtask/
├── Cargo.toml              # ADD: sysinfo = "0.32" dev-tool dep (dev-only per FR-015 + SC-009)
├── src/
│   ├── main.rs             # Extend Cli enum: Ebpf | Bench(BenchArgs) | BenchDocs(BenchDocsArgs)
│   └── bench/              # NEW module tree
│       ├── mod.rs          # Entry point + BenchArgs parsing
│       ├── matrix.rs       # Fixture × mode enumeration
│       ├── run.rs          # Per-run execution: warmup + 5 timed samples + median
│       ├── measure.rs      # sysinfo-backed peak-RSS + wall-clock + output-byte capture
│       ├── schema.rs       # Serde structs for Run/Result/Fixture/Baseline
│       ├── compare.rs      # Regression-diff logic (per-dimension threshold)
│       └── docs.rs         # bench-docs: Markdown emission from baseline

waybill-test-fixtures/       # SIBLING REPO (not part of main repo)
└── benchmark/              # NEW top-level directory
    ├── manifest.json       # Fixture registry: name → { path, modes, expected-scan-time-class }
    ├── source-tier/        # One fixture per ecosystem
    │   ├── cargo-workspace-medium/
    │   ├── npm-monorepo-medium/
    │   ├── ...             # 12 ecosystems total per FR-002
    ├── container-images/   # docker-saved tarballs
    │   └── debian-slim.tar
    └── binaries/           # binary-introspection fixture set
        └── linux-binaries-50/

docs/
├── perf/                   # NEW directory
│   ├── baseline.json       # Committed baseline (updated per-release via xtask bench --update-baseline)
│   └── numbers.md          # Generated by xtask bench-docs from baseline.json
└── (other docs unchanged)

.github/workflows/
└── bench.yml               # NEW: runs xtask bench on release tags + weekly cron; regression-comment via github-script

.gitignore                  # ADD: target/bench/
```

**Explicitly unchanged**:
- `waybill-cli/`, `waybill-common/`, `waybill-ebpf/` — zero code changes
- Workspace root `Cargo.toml` + `Cargo.lock` — no additions at the workspace-runtime layer (SC-009 anchor)
- All other workflows (`ci.yml`, `release.yml`, `nightly.yml`, `ebpf-canary.yml`) — bench.yml is a new lane, doesn't perturb existing lanes
- `scripts/pre-pr.sh` — unchanged; existing gate still passes byte-identically (bench-run is opt-in via `cargo run -p xtask -- bench`, not a pre-PR gate)

## Post-Design Constitution Re-check

*GATE: after Phase 1 artifacts, re-check that the design doesn't drift.*

Re-checked after writing research.md, data-model.md, contracts/, quickstart.md:

- Fail Closed (Principle III): ✅ Contract `ci-workflow.md` C-3 requires `continue-on-error: false` on the bench-run step; C-6 requires the regression-comparison step to `exit 1` on any dimension breach. Contract `xtask-bench-cli.md` C-5 codifies the release-prep pre-flight fail-loud behavior per Q2.
- Type-Driven Correctness (Principle IV): ✅ Contract `json-schema.md` defines the schema as typed Rust structs (data-model.md) — no `serde_json::Value` untyped shuffling. Regression-diff uses `Duration` + typed `Dimension` enum.
- Three-Crate Architecture (Principle VI): ✅ Every code artifact lands under `xtask/src/bench/`. `sysinfo` is a `[dependencies]` entry in `xtask/Cargo.toml` only — grep on workspace-root Cargo.toml + waybill-*/Cargo.toml shows zero delta.
- Test Isolation (Principle VII): ✅ Contract `xtask-bench-cli.md` C-8 requires per-run scratch dirs via `tempfile::tempdir()`; no shared bench-run state across parallel test invocations.
- Transparency (Principle X): ✅ Contract `json-schema.md` mandates `waybill_commit_sha` + `fixture_sha` + `runner_uname` in every Result record; contract `xtask-bench-cli.md` C-4 mandates docs-gen embeds both SHAs per row.

**Verdict**: PASS post-design. No drift from pre-Phase-0 verdict.

## Phase Outputs Index

- **Phase 0** research → [research.md](./research.md) — R1-R7 decisions incl. sysinfo choice, JSON schema versioning, comment-edit mechanism, fixture-manifest shape, RSS portability, matrix enumeration, corpus-mode integration.
- **Phase 1** data model → [data-model.md](./data-model.md) — Fixture / Result / Run / Baseline / Regression Diff Rust structs + JSON serialization + validation rules V1-V8.
- **Phase 1** contracts:
  - [contracts/json-schema.md](./contracts/json-schema.md) — C1-C6 baseline.json + run.json shape (versioned + additive-only).
  - [contracts/xtask-bench-cli.md](./contracts/xtask-bench-cli.md) — C1-C8 flag surface for `xtask bench` + `xtask bench-docs` + release-prep pre-flight.
  - [contracts/ci-workflow.md](./contracts/ci-workflow.md) — C1-C7 `bench.yml` job structure + edit-in-place PR comment mechanism.
- **Phase 1** quickstart → [quickstart.md](./quickstart.md) — 5-step operator recipe: setup → first run → reproduce → refresh baseline → cite in docs.
- **Phase 1** agent context update → CLAUDE.md's `## Active Technologies` section gets a m669 entry (auto-appended by `.specify/scripts/bash/update-agent-context.sh claude`).

## Progress Tracking

- [X] Phase 0 research complete
- [X] Phase 1 data model complete
- [X] Phase 1 contracts complete
- [X] Phase 1 quickstart complete
- [X] Phase 1 agent context updated
- [X] Post-design Constitution re-check
- [ ] Phase 2 tasks (via `/speckit.tasks`)
