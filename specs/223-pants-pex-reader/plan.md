# Implementation Plan: Pants pex-lockfile reader

**Branch**: `223-pants-pex-reader` | **Date**: 2026-07-31 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/223-pants-pex-reader/spec.md`

## Summary

Adds a new source-tier reader at `waybill-cli/src/scan_fs/package_db/pants/` that
discovers Pex lockfiles (default `3rdparty/python/*.lock`, or `pants.toml`-configured
paths) and emits one `pkg:pypi/*` or `pkg:generic/*` component per locked
distribution. Reuses the m191 reconciler for FR-005 dedup against `requirements.txt`
/ `poetry.lock` / `uv.lock`. Zero new Cargo dependencies (Pex lockfiles are JSON;
`serde_json` is already a workspace dep). Multi-resolve support with lifecycle-scope
tagging by name allowlist (Q1 answer B). Non-PyPI entries fall back to
`pkg:generic/*` with source-url annotations (Q2 answer A).

**Critical Phase 0 items** (research must resolve):
1. Exact Pex lockfile JSON schema shape at version 2.x (top-level fields, per-lock
   entry fields, inter-package `requires` edge format). Need a real sample to
   confirm before writing the parser.
2. Which name-allowlist convention for dev-resolve tagging matches actual Pants
   community usage (e.g., is `mypy` always the mypy resolve, or does everyone name
   it differently?). This affects FR-008 accuracy.
3. Prior-art check on `pkg:pypi/*` PURL construction for names with non-canonical
   characters (dots, underscores, uppercase) — must match the existing pip
   reader's normalization to avoid dedup misses.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones
001–222; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `serde_json` (Pex lockfile parsing;
workspace pervasive), `toml = "0.8"` (`pants.toml` parsing; already used by
`cargo.rs` + `pip/`), `waybill_common::types::purl::{Purl, encode_purl_segment}`
(PURL construction), `tracing` (INFO/WARN diagnostics), `anyhow`/`thiserror` (error
propagation). **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; mirrors every language-reader
milestone since 002.
**Testing**: `cargo test --workspace` per Constitution Principle VII (no privilege
escalation, unprivileged CI runners). New test binary
`waybill-cli/tests/pants_pex_reader.rs` for integration coverage; per-module
`#[cfg(test)]` blocks for the parser + resolve-classifier unit tests. Synthetic
fixtures under `waybill-cli/tests/fixtures/pants_pex/` (per
`feedback_fixture_synthetic_package_names` — never real PyPI coordinates).
**Target Platform**: Linux + macOS + Windows (matches m100+ Windows-host support;
lockfile parsing is pure filesystem-read + JSON parse).
**Project Type**: Rust CLI (three-crate workspace per Principle VI).
**Performance Goals**:
- Reader must add <100ms to scan runtime on a repo with a single 500-entry Pex
  lockfile (matches the pip reader's `poetry.lock` parse cost — same-magnitude
  JSON blob).
- Default emit path (no Pex lockfiles found) must be byte-identical to today's
  goldens per SC-003 (feature adds zero cost when unused).
**Constraints**:
- Byte-identical golden output when no Pex lockfiles present (SC-003 / FR-007).
- Fail-open on per-lockfile corruption per FR-006 / SC-005 (WARN + skip, not
  scan-abort).
- No shell-out to `pants` binary (see Assumptions §"No Pants binary invocation").
- New `waybill:pants-resolve` + `waybill:source-url` + `waybill:source-type`
  annotations MUST have matching entries in
  `docs/reference/sbom-format-mapping.md` + `parity/extractors/mod.rs` per the
  m071 parity-extractor gate (memory `feedback_sbom_format_mapping_extractor_gate`).
**Scale/Scope**: 3 user stories (P1/P2/P3), 10 functional requirements, 6 success
criteria. Estimated diff: ~500 LOC production (reader + resolve classifier) +
~300 LOC tests + 3 parity-catalog rows + 3 extractor entries + 4–6 synthetic
fixture files. No changes to CLI surface, no new subcommands.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Applies? | Verdict | Notes |
|-----------|----------|---------|-------|
| I. Pure Rust, Zero C | ✅ | PASS | Zero new Cargo dependencies. Existing `serde_json` + `toml` handle both file formats. No transitive C-native additions expected. Verified post-implementation by rerunning the workspace's existing `no_c_dependencies_in_tree` regression test. |
| II. eBPF-Only Observation | ➖ | N/A | User-space reader; `waybill-ebpf` untouched. |
| III. Fail Closed | ✅ | PASS | FR-006 mandates WARN-and-skip on per-lockfile corruption (a scan-wide-halt would be worse UX for repos with mixed valid/corrupt lockfiles); the scan as a whole exits non-zero only on non-recoverable errors (matches existing pip / cargo / npm reader posture — per-file corruption is a WARN, per-scan failure is a hard exit). |
| IV. Type-Driven Correctness | ✅ | PASS | Reuses `waybill_common::types::purl::Purl` newtype (validates PURL shape at construction), existing `PackageDbEntry` struct with strong-typed fields (`Purl`, `Option<LifecycleScope>`, `Vec<Hash>`, etc.). New Pex lockfile Deserialize types (`PexLockfile`, `LockedRequirement`, `Artifact`) are all `#[derive(Deserialize)]` with explicit fields — no `serde_json::Value` bag types in the hot path. `#[cfg_attr(test, allow(clippy::unwrap_used))]` at test-mod level per existing convention. |
| V. Specification Compliance | ✅ | PASS | Native-fields-first (Principle V bullet 5): PURLs, hashes, licenses all go to native slots. `waybill:pants-resolve` + `waybill:source-url` + `waybill:source-type` annotations are used ONLY for signals that lack a standards-native equivalent (resolve name isn't representable in CDX/SPDX native fields; source-url beyond PyPI isn't representable in a PURL). Each new `waybill:*` key gets a corresponding parity-catalog row + extractor entry per m071. |
| VI. Three-Crate Architecture | ✅ | PASS | All new code lands in `waybill-cli`. No new crates. |
| VII. Test Isolation | ✅ | PASS | Reader runs without root/CAP_BPF. Integration tests use synthetic fixtures under `waybill-cli/tests/fixtures/pants_pex/`; unprivileged `cargo test --workspace` covers them. No network access (lockfiles are on-disk). |
| VIII. Completeness | ✅ | PASS | Coverage delta: adds Python-package discovery for Pants repos where currently zero components emit. Not degrading completeness anywhere. |
| IX. Accuracy | ✅ | PASS | Source-tier fidelity: lockfile is authoritative; artifact hashes recorded verbatim; `sbom_tier="source"` correctly claimed. No fabrication. |
| X. Transparency | ✅ | PASS | FR-010 INFO log records lockfile count + component count per scan — exactly the visibility signal Principle X requires. WARN diagnostics on per-file corruption name the offending file + reason. |
| XI. Enrichment | ➖ | N/A | Metadata-only feature; no enrichment source added. |
| XII. External Data Source Enrichment | ➖ | N/A | No external data source. Reader is purely filesystem-local. |

**Result**: PASS on all 12 principles. Zero gate violations. No entries required
in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/223-pants-pex-reader/
├── plan.md                                    # This file
├── spec.md                                    # /speckit.specify + /speckit.clarify output
├── research.md                                # Phase 0 (this command)
├── data-model.md                              # Phase 1 (this command)
├── quickstart.md                              # Phase 1 (this command)
├── contracts/                                 # Phase 1 (this command)
│   └── pex-lockfile-schema.md                 # Schema shape + version compatibility contract
├── checklists/
│   └── requirements.md                        # /speckit.specify output (16/16 PASS)
└── tasks.md                                   # /speckit.tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           ├── mod.rs                         # +pub mod pants; registration
│           └── pants/                         # NEW module directory
│               ├── mod.rs                     # Public read() entry + orchestrator
│               ├── lockfile.rs                # Pex-lockfile JSON parser (Deserialize types)
│               ├── config.rs                  # pants.toml minimal-parse (only [python].lockfile)
│               └── resolve_classifier.rs      # Resolve-name → LifecycleScope allowlist
├── tests/
│   ├── pants_pex_reader.rs                    # NEW integration test file (US1 + US2 + US3)
│   └── fixtures/
│       └── pants_pex/                         # NEW synthetic fixtures directory
│           ├── minimal_python/                # US1 baseline: valid lockfile, no requirements.txt
│           │   ├── 3rdparty/python/default.lock
│           │   └── BUILD                      # Optional; parser doesn't read it, present for realism
│           ├── multi_resolve/                 # US1 scenario 4: default + mypy + pytest
│           │   └── 3rdparty/python/{default,mypy,pytest}.lock
│           ├── pants_toml_custom_path/        # US3: non-default lockfile path
│           │   ├── pants.toml
│           │   └── build-support/py.lock
│           ├── with_requirements_txt/         # US2: dedup against requirements.txt
│           │   ├── 3rdparty/python/default.lock
│           │   └── requirements.txt
│           ├── non_pypi_entries/              # Q2 A: git-URL + direct-URL entries
│           │   └── 3rdparty/python/default.lock
│           └── corrupt_lockfile/              # SC-005: fail-open on corruption
│               └── 3rdparty/python/default.lock
docs/
└── reference/
    └── sbom-format-mapping.md                 # +3 catalog rows (waybill:pants-resolve, waybill:source-url, waybill:source-type)

waybill-cli/src/parity/
└── extractors/
    └── mod.rs                                 # +3 extractor entries matching the new catalog rows
```

**Structure Decision**: Module-directory layout (`package_db/pants/`) matches
existing multi-file readers like `pip/`, `npm/`, `gradle/`, `nuget/` per the
Explore-agent codebase survey. Single-file layout (like `bazel.rs` at 445 LOC)
was considered but rejected — the resolve-classifier + config-parser + lockfile-parser
warrant separation for testability, and expected ~500 LOC total is at the
module-directory-worth threshold.

Reader-surface contract (matches existing readers):
- `pub fn read(scan_root: &Path) -> Vec<PackageDbEntry>` — orchestrator entry
  at `pants/mod.rs`, called from `scan_fs/package_db/mod.rs::read_all` dispatcher.
- Emits `PackageDbEntry { purl, name, version, source_path, depends,
  lifecycle_scope, sbom_tier: Some("source"), evidence_kind, hashes,
  licenses, requirement_ranges, extra_annotations }` per the shared struct.
- Fail-open on per-file corruption (returns entries from valid lockfiles; WARN
  on invalid ones).

Dedup against `requirements.txt` / `poetry.lock` / `uv.lock` is handled by the
m191 reconciler in `scan_fs/mod.rs`; the new reader emits its entries with
`sbom_tier="source"` + the source-lockfile path in `source_path`, and the
reconciler's PURL-level dedup handles the collision per FR-005. No new dedup
infrastructure needed.

## Complexity Tracking

> Populated only if Constitution Check has violations that must be justified.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _none_ | — | — |

## Phase Progression

- [x] Phase 0: research.md generated (Pex lockfile schema + dev-resolve allowlist + PURL normalization)
- [x] Phase 1: data-model.md, contracts/pex-lockfile-schema.md, quickstart.md generated + agent context updated
- [x] Constitution re-check post-design: still PASS on all 12 principles

## Follow-ups (out-of-scope for this branch)

- **Coursier lockfile reader** (Pants JVM side): Pants uses coursier locks at
  `3rdparty/jvm/*.lockfile` for JVM targets. Separate feature; the waybill Maven
  reader handles `pom.xml`/`gradle` but not coursier lock format specifically.
  Estimated as a separate ~600 LOC reader when demand appears.
- **BUILD file walker (design-tier)**: Pants target declarations
  (`python_source(...)`, `python_requirement(...)`) in `BUILD` files carry
  design-tier signal. The lockfile is the authoritative source-tier artifact so
  BUILD parsing is nice-to-have but not required for v1 SBOM correctness.
- **eBPF trace of `pex` / `pants` binary invocations**: waybill's tracing angle
  could capture the actual pex-resolve invocation at build time (option D from
  the investigation). Separate spec; applicable to all build systems.
- **`pyproject.toml` Pants-config equivalent**: some Pants setups embed
  Pants-specific config in `pyproject.toml` under `[tool.pants]`. Not standard
  practice; out of scope until observed in the wild.
