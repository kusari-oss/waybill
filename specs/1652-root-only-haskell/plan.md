# Implementation Plan: `root-only` keeps the packages the root builds, and records what it dropped

**Branch**: `1652-root-only-haskell` | **Date**: 2026-11-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1652-root-only-haskell/spec.md`. Refs #1676, #1045, #1032, #1673, #1675.

## Summary

FR-001's measurement changed the diagnosis (research R1, `measurements/baseline.md`).

**Why nested Haskell packages drop:**
- A root Haskell package is never recognised as root-level, because milestone 215's `source_dir_for` keeps the file name `package.yaml` or `*.cabal`.
- As a result:
  - **every one of the 8 Haskell corpus targets falls back to a full scan** under `root-only` and `strict`;
  - a tree whose root is another ecosystem drops all its Haskell packages and reads `complete`.
- The projection itself already follows edges across main modules.
- The log's dropped-project count is wrong on 4 of 4 measured cases.

**What changes:**
1. **Root detection** (R1): `project_discovery` decides root-level by directory, through a new `root_dir`. `split.rs` is not touched.
2. **Declared members** (R4): the Haskell reader resolves a root `cabal.project` or `stack.yaml` `packages:` to package directories. Under `root-only`, their main modules join the in-scope roots, and with no root package they are the roots.
3. **The true drop set** (R3): it is computed after filtering. The report and log count it.
4. **C244 `waybill:project-discovery-dropped`** (R5): the count, the reason and the sorted main-module PURLs, in all three formats.
5. **Reason code `project-discovery-scope`** (R6): completeness is never `complete` when projects were dropped, and CycloneDX `compositions[]` agrees.
6. **C200 `manifests-dropped-by-scope`** (R7): added only when non-empty.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde`/`serde_json`, `serde_yaml` (`stack.yaml`), `globset` (the `cabal.project` forms, already used by #1032), `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A. In process, per scan.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`. The tests:
- unit tests in `project_discovery/`, `haskell.rs`, `haskell/hpack.rs` and `graph_completeness/reason_codes.rs`;
- a new integration file, `waybill-cli/tests/project_discovery_haskell.rs`, covering the six fixture trees plus a symlinked root, in all three formats with `sbom parity-check`;
- C244's parity extractors;
- mutations (R9);
- a verification corpus dispatch.

**Target Platform**: every host.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: no measurable cost.
- One extra directory test per main module.
- One set difference after filtering.
- The reader parses `stack.yaml` once more, only when one sits at the root.

**Constraints**:
- FR-007: `all` is byte-identical, `--split` is unchanged, and #1045's fall-back is kept.
- FR-008: zero new dependencies.

**Scale/Scope**:
- `generate/project_discovery/{mod,filter}.rs` (`root_dir`, declared roots, the drop set);
- `scan_fs/package_db/haskell.rs` (`StackYaml.packages`, the resolver, the summary field);
- `scan_fs/package_db/haskell/hpack.rs` (`amend_scope_dropped`);
- `cli/scan_cmd.rs` (wiring);
- `generate/mod.rs` (a new `ScanArtifacts` field);
- the three C244 emitters: `cyclonedx/metadata.rs`, `spdx/annotations.rs`, `spdx/v3_annotations.rs`;
- `graph_completeness/{mod,document,reason_codes}.rs` and its four callers;
- `cyclonedx/compositions.rs`;
- `parity/extractors/{mod,cdx,spdx2,spdx3}.rs`;
- docs: `project-discovery.md`, `sbom-format-mapping.md` (C244, C105, C200), `cli-reference.md` if it describes the modes;
- CHANGELOG.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate. |
| III. Fail closed | Pass. A declaration that matches nothing is treated as no declaration, so the scan falls back rather than emptying itself (R4). A drop is never silent: the log, C244, completeness and C200 all carry it. |
| IV. Type-driven correctness | Pass. The drop is a typed set in `ProjectDiscoveryReport`. The reason code is a closed enum variant. `compositions` gets an explicit match arm, not a wildcard (R6). No `.unwrap()` in production. |
| V. Specification compliance | Pass. Audit for C244: CycloneDX `compositions` can say incomplete, and R6 uses it for that, but no format can name what scoping left out. KEEP-NO-NATIVE. C140 cannot be extended compatibly, because it is a string (R5). |
| VIII. Completeness | This is the principle the feature restores. A scoped document says what it excludes. |
| IX. Accuracy | The log's count is corrected (R3). `complete` is no longer claimed over dropped projects (R6). |
| X. Transparency | C244 names every dropped main module. C200 names every dropped manifest. |
| Strict boundaries | Pass. `all` output and `--split` are untouched (R8). |

**Post-design re-check:** unchanged. Every new field is emitted only on the filtered branch, and `all` returns at the filter's first line.

## Project Structure

### Documentation (this feature)

```text
specs/1652-root-only-haskell/
├── spec.md
├── plan.md               # this file
├── research.md           # R1–R9
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── scope-filter.md   # root detection, declared roots, the drop set
│   └── wire.md           # C244, the reason code, C200's key
├── measurements/         # FR-001: scripts, baseline, and later the after-table
└── checklists/requirements.md
```

### Source Code (repository root)

```text
waybill-cli/src/
├── generate/project_discovery/
│   ├── mod.rs            # root_dir; is_root_level uses it; ProjectDiscoveryReport.dropped_main_modules
│   └── filter.rs         # declared_member_dirs parameter; drop set; dir_key seeding via root_dir
├── scan_fs/package_db/
│   ├── haskell.rs        # StackYaml.packages; resolve_root_declared_members; summary field
│   └── haskell/hpack.rs  # amend_scope_dropped
├── generate/graph_completeness/
│   ├── reason_codes.rs   # ProjectDiscoveryScope
│   ├── mod.rs            # pushed on both paths
│   └── document.rs       # for_document parameter
├── generate/cyclonedx/{metadata,compositions,builder}.rs
├── generate/spdx/{annotations,v3_annotations,document,v3_document}.rs
├── generate/orphan_reason.rs   # passes the drop to for_document
├── generate/mod.rs             # ScanArtifacts.project_discovery_dropped
├── cli/scan_cmd.rs             # wiring
└── parity/extractors/{mod,cdx,spdx2,spdx3}.rs   # C244
waybill-cli/tests/
├── project_discovery_haskell.rs
└── fixtures/project_discovery_haskell/   # the six trees, waybill-fixture-* names
docs/reference/{project-discovery,sbom-format-mapping}.md
CHANGELOG.md
```

## Complexity Tracking

No constitution violations. One deliberate boundary is recorded:
- **The directory fix lives in `project_discovery`, not in `split.rs`'s shared `is_manifest_basename`.** That keeps `--split`'s file names and directory groups unchanged (FR-007).
- **`--split`'s own Haskell `source_dir` gap is #1691**, not fixed here.
