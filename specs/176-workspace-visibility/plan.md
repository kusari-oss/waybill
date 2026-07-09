# Implementation Plan: Monorepo workspace-member visibility for scoped SBOM consumption

**Branch**: `176-workspace-visibility` | **Date**: 2026-07-08 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/176-workspace-visibility/spec.md`

## Summary

**Primary requirement**: emit two new annotations that surface workspace membership operators can already see in `mikebom:source-files` but currently have to derive by hand: (a) per-component `mikebom:workspace-member = ["<path1>", "<path2>", ...]` naming the workspace(s) each component belongs to; (b) doc-scope `mikebom:workspaces-detected = ["<path1>", ...]` enumerating all detected workspaces. Plus one advisory log line at scan time when N > 1.

**Technical approach**: **derivation-only, not detection**. Every `PackageDbEntry` already carries a `source_path: String` field (per `package_db/mod.rs:64`) that identifies the manifest / lockfile / DB file that produced it. This threads into `ResolutionEvidence.source_file_paths` (per `resolution.rs:283`) which is ALREADY emitted as the `mikebom:source-files` annotation. The workspace root is simply `dirname(source_path)` for each entry — no new reader logic, no new detection walk, no plumbing chain change.

The langflow + test-tensorflow-models audits confirmed this: emitted `mikebom:source-files` values already read like `official/requirements.txt`, `src/frontend/package-lock.json`, `.git/hooks/*.sample` — every workspace-attributable path already has its manifest naming convention baked in.

Four surgical changes, all at emission time:

1. **New helper `derive_workspace_root(source_file_path: &str) -> Option<String>`** in a small utility module (e.g., `mikebom-cli/src/scan_fs/workspace_root.rs`). Logic: parse the path as `Path`, take `parent()`, canonicalize to forward-slash form. For root-level manifests (`Cargo.toml`, `package.json` at scan-root), returns `Some(".")` to represent "scan root" as a first-class workspace path (matches the m068 pip precedent). For malformed paths (empty string, non-UTF-8), returns `None` — the caller then omits the annotation per Q1/FR-002. Also normalize any `path+file://<abs>` URI-form source paths back to scan-root-relative paths using the scan root as the strip prefix.

2. **Per-component emission at metadata / annotations construction time** — inject the annotation into each component just before serialization. Concretely: in the three format emitters (`cyclonedx/metadata.rs`, `spdx/annotations.rs`, `spdx/v3_annotations.rs`), for each component being emitted, collect `component.evidence.source_file_paths.iter().filter_map(derive_workspace_root).collect::<BTreeSet<_>>()`; if the set is non-empty, emit `mikebom:workspace-member` with `serde_json::to_string(&sorted_vec)`; if empty, omit the annotation entirely (per Q1).

3. **Doc-scope `mikebom:workspaces-detected` emission** — during emission, compute the union of every per-component workspace set into a single sorted `BTreeSet<String>`. Emit at doc scope with the same array-encoded string shape. Absent when the union is empty (matches FR-003).

4. **Advisory log at emission-tail site in scan_cmd.rs** — same pattern as m173's FR-004 and m175's FR-002. Predicate: `workspaces_detected.len() > 1 && !components.is_empty()`. When true, emit exactly one `tracing::info!` line with the stable substring per FR-004. No suppression flag (matches m173's implicit-only, YAGNI'd from m175).

**Parity infrastructure**: two new parity-catalog rows — C120 per-component `mikebom:workspace-member`, C121 doc-scope `mikebom:workspaces-detected`. Six new extractor helpers (`c120_cdx`/`c120_spdx23`/`c120_spdx3` + `c121_*`) using the standard macros. Both rows are `SymmetricEqual` per FR-011 cross-format parity requirement.

**Docs**:
- **New `docs/reference/monorepos.md`** — the file the FR-004 advisory log points at. Explains workspace-membership concept, m068/m066/etc. reader precedent, jq recipes for per-workspace CVE triage, cross-references to `mikebom:source-files` (which was the pre-176 derivation vector).
- **Enrichment of `docs/reference/reading-a-mikebom-sbom.md`** — new subsection near the existing `mikebom:source-files` section documenting C120/C121, KEEP-NO-NATIVE audit note (CDX `component.group` is closest but semantically different — it's the component's authoring org/project, not the scan-target workspace boundary), remediation jq recipes.
- **New rows in `docs/reference/sbom-format-mapping.md`** — C120 + C121, both **KEEP-NO-NATIVE** with the Principle V audit citing `component.group` / SPDX 3 `Element.namespace` as rejected alternatives.

**Golden regeneration**: expected on every existing golden fixture since every component gains a `mikebom:workspace-member` annotation + document-scope gains `mikebom:workspaces-detected`. SC-004 gate: the ONLY additions permitted are these two annotations; all other bytes byte-identical. Post-regeneration jq diff assertion codifies this.

**Blast radius**: ~50 lines for the derive helper + tests, ~30 lines per emitter (CDX + SPDX 2.3 + SPDX 3) = ~90 lines emission, ~10 lines advisory log, ~30 lines parity (2 rows + 6 helpers), ~200 lines docs. Golden delta: +2 annotations per component + 1 doc-scope annotation per SBOM → the byte delta per golden depends on component count but is bounded to those two categories. 1 new integration test file at `mikebom-cli/tests/workspace_visibility.rs` with 5-6 test functions.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–174; no nightly required).

**Primary Dependencies**: Existing only — `std::path::{Path, PathBuf}` (dirname derivation), `serde_json` (JSON-array-in-string encoding matching m134/m147/m173 precedent), `std::collections::BTreeSet` (sorted-deduplicated storage), `tracing` (advisory log). **Zero new Cargo dependencies.** No subprocess calls. No network access.

**Storage**: N/A — pure emission-time derivation; no persistence.

**Testing**: `cargo test` — 1 new integration test at `mikebom-cli/tests/workspace_visibility.rs` covering US1/US2/US3 acceptance criteria + 4-5 unit tests for the `derive_workspace_root` helper edge cases (path+file URI, root-level manifest, empty string, non-UTF-8 fallback). Existing byte-identity golden regression suite regenerated with the +2-annotations delta gated by SC-004 verification.

**Target Platform**: All hosts mikebom builds on — Linux, macOS, Windows (m100-experimental). FR-010 requires forward-slash normalization on Windows.

**Project Type**: cli (mikebom sbom-generation CLI).

**Performance Goals**: N/A — the derivation is `O(N * K)` over components * source-file-paths-per-component (typically K ≤ 3); happens once per scan at emission time.

**Constraints**: SC-004 byte-identity gate for the non-added regions of every existing golden fixture. Post-regeneration, `jq -S 'del(.components[]?.properties[]? | select(.name == "mikebom:workspace-member")) | del(.metadata.properties[]? | select(.name == "mikebom:workspaces-detected"))'` on pre-176 vs post-176 outputs must produce byte-identical result.

**Scale/Scope**: Small-to-medium. 1 new helper module, 3 emitter files edited, 2 parity extractor entries (with 6 helper functions), 3 docs files (1 new + 2 edited), 33 golden fixtures regenerated.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No new Cargo dependencies. Pure Rust addition.
- **II. eBPF-Only Observation**: ✅ `mikebom-ebpf` untouched.
- **III. Fail Closed**: ✅ Derivation returns `Option<String>` — `None` cases (malformed paths, non-UTF-8) omit the annotation (fail-open in the emission direction is correct — a component without workspace attribution is semantically distinct from one with a "" workspace).
- **IV. Type-Driven Correctness**: ✅ Uses `BTreeSet<String>` for sorted-deduplicated storage — enforces the wire-shape invariant (alphabetical, deduplicated) at the type level. No new types introduced; derivation happens over existing `Vec<String>` fields.
- **V. Specification Compliance**: ⚠️ **AUDITED — KEEP-NO-NATIVE justified**. Two new `mikebom:*` annotations — each audited against CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 for existing native constructs:
  - **CDX `component.group`**: closest native field — "The grouping name or identifier. This will often be a shortened, single name of the company or project that produced the component." Semantically the component's authoring organization/project (e.g., `com.fasterxml.jackson.core` for `jackson-core`), NOT the scan target's workspace boundary. Different concept.
  - **SPDX 2.3 `Package.sourceInfo`**: free-text summary of package data source. Not machine-parseable.
  - **SPDX 3 `Element.namespace`**: identity-URI scoping, not workspace-boundary. Different concept.
  - **CDX 1.6 `component.evidence.identity[].techniques[].value`**: per-technique confidence hint, not workspace membership.
  Rejected alternatives named in the C120/C121 mapping rows; migration path codified if either standard adopts a workspace-boundary vocabulary.
- **VI. Three-Crate Architecture**: ✅ Change contained to `mikebom-cli` (emission + parity + docs). No `mikebom-common` or `mikebom-ebpf` changes.
- **VII. Test Isolation**: ✅ Integration test uses per-test tempdir; no shared state.
- **VIII. Completeness**: ✅ **Improved**. Post-176 an operator + downstream consumer can accurately scope emitted components to their originating workspace — the emitted SBOM answers "which subproject is this dep from?" in one jq call. Pre-176 the operator had to walk `mikebom:source-files` values and derive by hand.
- **IX. Accuracy**: ✅ **Improved**. The workspace annotation is a machine-materialized derivation of an existing native signal (`evidence.source_file_paths` → CDX `component.evidence.identity[].techniques[].value`); making it explicit doesn't fabricate anything, it surfaces truth mikebom already knows.
- **X. Transparency**: ✅ **Directly serves Principle X**. The advisory log tells operators when their scan target is monorepo-shaped; the annotations make the shape queryable post-hoc.
- **XI. Enrichment**: N/A — no enrichment path touched.
- **XII. External Data Source Enrichment**: N/A.

**Strict Boundaries check**:
- **New subprocess**: ✅ None.
- **New network access**: ✅ None.
- **New filesystem writes**: ✅ None. Purely emission-time derivation over existing in-memory data.
- **New `mikebom:*` annotation namespaces**: ⚠️ Two new (C120 + C121). Both KEEP-NO-NATIVE audited per Principle V.
- **New Cargo dependencies**: ✅ Zero.
- **Strict Boundary §5 (file-tier no-duplicates)**: ✅ Preserved. File-tier components explicitly omit the annotation (per FR-002 / Q1); no duplication logic touched.

**Verdict**: All principles pass. Zero violations. Milestone improves Principles VIII/IX/X (Completeness, Accuracy, Transparency).

## Project Structure

### Documentation (this feature)

```text
specs/176-workspace-visibility/
├── plan.md              # This file
├── research.md          # Phase 0 — source_path threading audit + Q1 derivation rules
├── data-model.md        # Phase 1 — derive_workspace_root helper + emission call sites
├── quickstart.md        # Phase 1 — 3-scenario verification recipe (US1 CVE-triage, US2 advisory, US3 doc-scope)
├── contracts/           # Phase 1 — annotation wire shapes + monorepos.md doc contract
├── checklists/          # Requirements checklist (spec-phase output)
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── workspace_root.rs                # NEW ~80 lines: derive_workspace_root helper + unit tests
│   ├── generate/
│   │   ├── cyclonedx/
│   │   │   └── metadata.rs                  # ~15 lines: per-component + doc-scope emission
│   │   ├── spdx/
│   │   │   ├── annotations.rs               # ~15 lines: SPDX 2.3 emission
│   │   │   └── v3_annotations.rs            # ~15 lines: SPDX 3 emission
│   │   └── mod.rs                           # (unchanged)
│   ├── parity/
│   │   └── extractors/
│   │       ├── mod.rs                       # +4 lines: C120 + C121 rows + imports
│   │       ├── cdx.rs                       # +2 lines: c120_cdx + c121_cdx helpers
│   │       ├── spdx2.rs                     # +2 lines: c120_spdx23 + c121_spdx23 helpers
│   │       └── spdx3.rs                     # +2 lines: c120_spdx3 + c121_spdx3 helpers
│   └── cli/
│       └── scan_cmd.rs                      # ~15 lines: advisory log at emission-tail
└── tests/
    └── workspace_visibility.rs              # NEW ~200 lines: US1/US2/US3 integration tests

docs/reference/
├── monorepos.md                             # NEW ~150 lines: workspace concept + jq recipes + m068/m066 reader precedent
├── reading-a-mikebom-sbom.md                # ~80 lines: new subsection for C120/C121
└── sbom-format-mapping.md                   # ~2 lines: C120 + C121 KEEP-NO-NATIVE rows

mikebom-cli/tests/fixtures/golden/
├── cyclonedx/*.cdx.json                     # 11 files regenerated (+2 annotation lines per component + 1 doc-scope)
├── spdx-2.3/*.spdx.json                     # 11 files regenerated (envelope-wrapped)
└── spdx-3/*.spdx3.json                      # 11 files regenerated (typed Annotation graph elements)
```

**Structure Decision**: pure emission-time addition. One new helper module (`workspace_root.rs`) hosts the derivation logic; three emitter files gain small per-component + doc-scope emission blocks; parity infra adds two rows and six helpers matching the m172/m173 precedent verbatim. Advisory log is a single INFO-level `tracing::info!` at the emission-tail site in `scan_cmd.rs`, matching m173 FR-004 and m175 FR-002 pattern. All 33 goldens regenerated with the +2-annotations delta; SC-004 gates verify no other byte changes.

## Complexity Tracking

No constitution violations to justify. The plan is a straight-line emission-time derivation with zero new detection logic or reader plumbing.
