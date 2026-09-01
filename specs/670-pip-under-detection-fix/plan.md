# Implementation Plan: Fix critical Python under-detection

**Branch**: `670-pip-under-detection-fix` | **Date**: 2026-08-31 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/670-pip-under-detection-fix/spec.md`

## Summary

Waybill's `sbom scan` command dramatically under-detects Python components on source trees that ship Python declaration files. On real OSS projects surveyed in the 2026-08-31 sweep, only the project itself (and any nested sub-projects) is being emitted — zero third-party runtime deps. This milestone extends the existing `waybill-cli/src/scan_fs/package_db/pip/` reader family to parse `pyproject.toml` (PEP 621 + PEP 735 + Poetry-legacy), the four canonical lockfile formats (`uv.lock`, `poetry.lock`, `pdm.lock`, `Pipfile.lock`), recursive `requirements*.txt` discovery with venv-pruning, and static `setup.py` / `setup.cfg` parsing. Success is measured against the three failing sweep fixtures: markitdown (4 → ≥30), OctoPrint (3 → ≥30), cpython (16 → ≥50) pypi components emitted.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–669; no nightly required for this user-space-only work).

**Primary Dependencies**:

- `toml = "0.8"` — pyproject.toml, uv.lock, poetry.lock, pdm.lock parsing (already a workspace dep, used by `cargo.rs` + `pip/`)
- `serde` / `serde_json` — Pipfile.lock (JSON format) + annotation-value construction (workspace)
- `regex = "1"` — requirements.txt PEP 508 line-fragment extraction, setup.py `install_requires=[...]` literal-list detection (workspace)
- `waybill_common::types::purl::Purl` — PURL construction + validation
- `waybill_common::resolution::{LifecycleScope, RelationshipType}` — m179/m180 optional-scope mechanism (workspace)
- `waybill_common::types::hash::ContentHash` — SHA-1/SHA-256 emission via m138 precedent (workspace)
- `tracing` (workspace) — info/warn/debug logs on parse failures + venv-prune decisions
- `anyhow` / `thiserror` (workspace) — error propagation

**Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process for the lifetime of a single scan; mirrors every ecosystem-reader milestone since 002.

**Testing**:
- `cargo test --workspace` unit + integration suites (existing convention)
- Fixture-integration tests against pinned `kusari-sandbox/test-{markitdown,OctoPrint,cpython}` corpora, fetched via the milestone-090 build.rs pattern; run against golden SBOMs stored in `waybill-cli/tests/fixtures/public_corpus/{markitdown,OctoPrint,cpython}/`
- Golden regen via `MIKEBOM_UPDATE_GOLDENS=1 cargo test -p waybill-cli --test transitive_parity_python`
- Cross-fixture no-regression check via the milestone-195 harness

**Target Platform**: Linux + macOS + Windows (matches milestone 100 host-portability posture); no filesystem semantics diverge across hosts for this milestone.

**Project Type**: CLI tool — single Cargo workspace (`waybill-cli` / `waybill-common` / `waybill-ebpf`) per the constitutional Three-Crate Architecture (Principle VI). This milestone touches only `waybill-cli/src/scan_fs/package_db/pip/`.

**Performance Goals**:
- test-markitdown scan wall-clock ≤ 549 ms (from ~49 ms baseline; +500 ms budget for new manifest parsing)
- test-cpython scan wall-clock ≤ 5.575 s (from ~575 ms baseline; +5 s budget for recursive `requirements*.txt` discovery)
- No walk regression on non-Python sweep fixtures (± 5% wall-clock)

**Constraints**:
- **Principle I**: no Python interpreter invocation, no exec, no `python -c`, no import of `setup.py`. All parsing is static.
- **Principle II / Strict Boundary #1** (see Constitution Check below for divergence rationale): filesystem-scan-based component *discovery* is used, following the established `scan_fs/package_db/` reader pattern from m002 onward
- **Principle V**: FR emissions use standards-native fields (CDX `scope`, SPDX 2.3 `DEV/BUILD/TEST_DEPENDENCY_OF`, SPDX 3 `LifecycleScopeType`) via the existing m179/m180 `LifecycleScope::Optional` variant. `waybill:*` annotations used ONLY for finer-grained data the standards do not express (m236 `waybill:unresolved-reason`, new `waybill:python-req-file-scope`, new `waybill:direct-url-source`).
- **Principle IV**: no `.unwrap()` in production; test modules using `.unwrap()` gated with `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- No network access at scan time (matches sweep `--offline` posture and every recent ecosystem-reader milestone).

**Scale/Scope**:
- 18 functional requirements across 3 user stories
- ~5-8 new source files in `waybill-cli/src/scan_fs/package_db/pip/`
- 3 fixture-integration tests
- 3 sweep-fixture SBOMs added as goldens

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Reviewed against all 12 principles + all 5 Strict Boundaries at Waybill Constitution v2.1.0:

| Principle | Status | Notes |
|-----------|--------|-------|
| I — Pure Rust, Zero C | **PASS** | No new C code, no FFI, no Python interpreter. Static parsing of setup.py via regex + AST-shape matching (no code execution). |
| II — eBPF-Only Observation | **DIVERGENT** (see Complexity Tracking) | This is a filesystem-scan reader, not an eBPF-trace observer. The divergence is standard practice for `sbom scan` and predates this milestone by 200+ milestones. |
| III — Fail Closed | **PASS** | Parse errors on individual files warn-and-skip (FR-016); overall scan continues. Consistent with every ecosystem reader's error posture since m002. |
| IV — Type-Driven Correctness | **PASS** | Uses existing `Purl` newtype, existing `LifecycleScope` enum, existing `ContentHash` newtype. No raw `String` PURLs cross boundaries. No `.unwrap()` in production. |
| V — Specification Compliance | **PASS** | Emitted components conform to CISA 2026, CDX 1.6, SPDX 2.3, SPDX 3.x, PURL spec via existing emission machinery. Three `waybill:*` annotations introduced (m236 already-catalog `waybill:unresolved-reason`, new `waybill:python-req-file-scope`, new `waybill:direct-url-source`) — each carries data the standards do not express. Audit result cited in FR-005a / FR-005b / FR-013 per §V bullet 5. |
| VI — Three-Crate Architecture | **PASS** | Changes confined to `waybill-cli` (specifically `waybill-cli/src/scan_fs/package_db/pip/`). No new crates. |
| VII — Test Isolation | **PASS** | All new tests are unit + integration under `cargo test --workspace`; no eBPF privileges required. |
| VIII — Completeness | **PASS** — this milestone directly addresses a completeness gap (the sweep's 4/3/16 vs ≥30/≥30/≥50 gap). Warn-and-skip on parse failure surfaces gaps transparently (Principle X). |
| IX — Accuracy | **PASS** | Warn-and-skip vs hallucinate is codified in FR-006 (dynamic setup.py → skip, no fabrication). Every emitted component has evidence pointing at a real manifest/lockfile file (FR-014). |
| X — Transparency | **PASS** | `waybill:unresolved-reason` (m236) surfaces unresolvable version cases; `waybill:direct-url-source` surfaces non-PyPI-index sources; `waybill:python-req-file-scope` surfaces the scope-derivation heuristic. All three are spec-native emission via the CDX `properties[]` / SPDX 2.3 / SPDX 3 `Annotation` channels. |
| XI — Enrichment | **N/A** | This milestone is discovery-side; no enrichment concerns beyond the existing infrastructure. |
| XII — External Data Source Enrichment | **N/A** | No external data sources touched at scan time. |
| **SB#1** — No lockfile-based dependency discovery | **DIVERGENT** (see Complexity Tracking) | Same rationale as Principle II; established practice. |
| SB#2 — No MITM proxy | **PASS** | No network activity in the reader. |
| SB#3 — No C code | **PASS** | Pure Rust. |
| SB#4 — No `.unwrap()` in production | **PASS** | See Principle IV. |
| SB#5 — No file-tier duplicates in default mode | **PASS** | This milestone emits package-tier components; existing m133 dedupe machinery unchanged. |

**Two divergences flagged (Principle II + SB#1)** — see [Complexity Tracking](#complexity-tracking) below.

## Project Structure

### Documentation (this feature)

```text
specs/670-pip-under-detection-fix/
├── plan.md                # This file
├── research.md            # Phase 0 output
├── data-model.md          # Phase 1 output
├── quickstart.md          # Phase 1 output
├── contracts/             # Phase 1 output
│   ├── README.md
│   ├── pyproject_toml.md
│   ├── requirements_txt.md
│   ├── setup_py_static.md
│   └── lockfiles.md
├── checklists/
│   └── requirements.md    # Existing (5/5 clarifications complete)
└── tasks.md               # Deferred to /speckit.tasks
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           └── pip/
│               ├── mod.rs                    # existing (dispatcher; extended)
│               ├── dist_info.rs              # existing (site-packages reader; unchanged)
│               ├── pyproject_toml.rs         # NEW — PEP 621 + PEP 735 + Poetry-legacy manifest
│               ├── requirements_txt.rs      # NEW — recursive discovery + PEP 508 line parse
│               ├── setup_py.rs               # NEW — static parse (regex + literal-list extract)
│               ├── setup_cfg.rs              # NEW — INI parse [options] install_requires
│               ├── uv_lock.rs                # NEW — TOML lockfile
│               ├── poetry_lock.rs            # NEW — TOML lockfile
│               ├── pdm_lock.rs               # NEW — TOML lockfile
│               ├── pipfile_lock.rs           # NEW — JSON lockfile
│               ├── venv_prune.rs             # NEW — default-prune pathset for the walker
│               ├── req_scope_heuristic.rs    # NEW — FR-005a filename+parent-dir → scope
│               └── direct_url.rs             # NEW — FR-005b PEP 508 direct-URL parsing
├── tests/
│   ├── fixtures/
│   │   └── public_corpus/
│   │       ├── markitdown/                   # NEW — golden SBOMs from m195 harness
│   │       ├── OctoPrint/                    # NEW
│   │       └── cpython/                      # NEW
│   └── transitive_parity_python.rs           # NEW — fixture-integration test
```

**Structure Decision**: The reader is added as new siblings under the existing `waybill-cli/src/scan_fs/package_db/pip/` module. This preserves the m002-onward convention that every ecosystem lives at `scan_fs/package_db/<ecosystem>/`, and lets the existing `pip/dist_info.rs` (site-packages reader) coexist with the new source-tree readers. The m191 reconciler at `scan_fs/mod.rs::apply_python_reconciler` (existing) collapses duplicate PURLs across all pip-family readers. Golden fixtures follow the m195 public-corpus-harness pattern at `waybill-cli/tests/fixtures/public_corpus/`.

## Complexity Tracking

Two constitutional principles (II + SB#1) require justification. The core `waybill sbom scan` command family has consistently violated Principle II's "eBPF-Only Observation" strict interpretation since milestone 002. The constitution's eBPF-first language was written for the `waybill trace` command (which observes live builds). `sbom scan` is a distinct command path — filesystem-based SBOM discovery from manifests + lockfiles — that has been the tool's primary user-facing command since it shipped 200+ milestones ago and produces the vast majority of end-user output.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Principle II — eBPF-Only Observation | Static Python source-tree scanning is the ONLY viable path for the `sbom scan` command. eBPF-based observation would require the operator to actually run their Python code (installs, tests, apps) with waybill attached, which is not what `sbom scan` does. | Requiring eBPF-only would make `sbom scan` unable to produce SBOMs for any Python project — regressing 200+ milestones of shipped functionality and eliminating waybill's dominant user surface. |
| Strict Boundary #1 — No lockfile-based dependency discovery | Same rationale. The `scan_fs/package_db/*` reader family (every ecosystem since m002) is entirely composed of lockfile/manifest readers used for discovery. | Rejecting this would delete every ecosystem reader in the codebase. The Strict Boundary was written before `sbom scan` existed as a shipped command; established practice is that SB#1 applies to `waybill trace`, not `waybill sbom scan`. |

**Reviewer note**: This divergence is not novel to milestone 670. Every specs entry from `002-python-npm-ecosystem` through `237-*` uses the same pattern; the constitutional posture has not been amended to reflect the emergence of `sbom scan` as the tool's primary command surface. Amending the constitution is out of scope for this milestone; the divergence is called out here to satisfy the gate.
