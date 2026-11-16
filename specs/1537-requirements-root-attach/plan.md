# Implementation Plan: Declared requirements attach to the project that declares them

**Branch**: `1537-requirements-root-attach` (base `main` at `1d25955c`) | **Date**: 2026-11-16 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1537-requirements-root-attach/spec.md`

## Summary

Issues #1566 and #1538. Two separate defects combine.

**First defect: requirements packages have no declared edge.** A pip `requirements*.txt` package reaches the root only through the "attach graph-tops when the root has no edges" fallback. Anything that gives the root an edge strands it:
- a submodule (#1566);
- a root `pyproject.toml` with its own dependency. This was measured with no submodule, so it predates #1515 (research R1).

**Second defect: the fallback lives in four hand-synchronised copies.** They add root anchors at different points relative to the gate, and differ in three more ways (R2). That is why CycloneDX and SPDX 2.3 orphan what SPDX 3 attaches. The completeness classifier never sees anchors, so every document says `complete`.

**The fix, in three parts:**
1. **Declared edges** (R3–R5). A scan-time pass, `scan_fs/declared_attach.rs`, gives each requirements-declared package an edge from the project that declares it. The project is the nearest enclosing main module of any ecosystem, or a submodule, found through an `OwnerIndex`. At the scan root, or with no owner, the package becomes a *declared root anchor*, using the reader-anchor channel #1515 added. Duplicate declarations now keep every declaring file.
2. **One root-edge rule** (R2, R6). `generate/root_edges.rs` computes the root's direct targets once. All three emitters and the completeness classifier use it. Anchors never switch the fallback off; the fallback applies to any root; only dependency edges other than resolve anchors count toward the gate; graph-tops exclude nested components.
3. **Measurement.** Corpus goldens and the quality corpus run in CI. #1538's super-project is measured in CI through a new fixture-repository workflow (R10). The SC-008 root-only completeness count is recorded as a follow-up (R11).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: Existing only. `serde_json`, `tracing`, `waybill_common::resolution::{ResolvedComponent, Relationship, RelationshipType}` and `waybill_common::types::purl`. **Zero new Cargo dependencies** (FR-011).
**Storage**: N/A. Everything is in process for one scan.
**Testing**:
- **Unit:** the `OwnerIndex` precedence (R3); the ancestor walk; `root_edges` covering all four R2 decisions and anchor ordering; the pip duplicate merge (R5).
- **Integration:** a new `waybill-cli/tests/declared_requirements.rs`. Each case is built as a temporary git repository with `waybill-fixture-*` names and checked in all three formats, together with the completeness verdict:
  - #1566's repro, with and without a submodule;
  - `pyproject.toml` + `requirements.txt`;
  - nested directories with and without projects;
  - a submodule with its own `pyproject.toml`;
  - a duplicate declaration;
  - a `-dev` scope;
  - a supplement;
  - an image-style placeholder root.
- **Conformance:** the CycloneDX schema and `spdx3-validate`.
- **CI:** corpus goldens (SC-004), the quality corpus (SC-005), `measure-source.yml` (SC-006), and the root-only probe (SC-008).

**Target Platform**: as today. The fixtures run `git`, so the test file is gated `#![cfg(unix)]` at file scope if Windows `git` behaviour diverges. Decided at T002 by running the Windows lane.
**Project Type**: CLI.
**Performance Goals**: no measurable change. Scan time is recorded in the quality-corpus run.
**Constraints**: deterministic; no network; the anchor channel stays emit-time, because the root is chosen per format.
**Scale/Scope**: #1566's super-project has 160 → 11 root edges and 73 orphans (reporter). #1538's has 62 of 66 orphaned (reporter). Both are re-measured in CI by T020.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates. |
| III. Fail Closed | Pass. No edge is invented. Every edge the pass adds comes from a file that names its target. The fallback's scope is narrowed by one rule (nested components) and unified, never widened beyond #236's empty-root case (FR-004, FR-005). |
| IV. Type-Driven Correctness | Pass. One `root_edges` function replaces four copies. `OwnerIndex` makes the owner choice a single total order (R3). |
| V. Specification Compliance | Pass. Native `dependsOn` / `DEPENDS_ON` and the existing scope-typed SPDX spellings. No new wire row: completeness and orphan reasons reuse their existing rows. |
| VII. Test Isolation | Pass. Synthetic temporary repositories. TheRock only in CI (R10). |
| VIII. Completeness | Pass. Declared packages that were dropped from the graph are restored. |
| IX. Accuracy | Pass. It removes one format's false attachment, CycloneDX nested graph-tops, and every format's false `complete`. Precedence is deterministic and documented. |
| X. Transparency | Pass. FR-008's population is empty by construction (R7), and that is enforced by a test. Format disagreements are written down per decision (R2). The SC-008 numbers are published as a follow-up. |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation

```text
specs/1537-requirements-root-attach/
├── spec.md, plan.md, research.md (R1–R13), data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/   repro arms, golden review, quality corpus, measure-source (CI), root-only probe, mutations, gate log
```

### Source Code

```text
waybill-cli/src/generate/root_edges.rs                     # NEW: the one root-edge rule (R2)
waybill-cli/src/generate/mod.rs                            # declare module
waybill-cli/src/generate/cyclonedx/dependencies.rs         # use root_edges; delete gate + anchor copy
waybill-cli/src/generate/spdx/document.rs                  # use root_edges; delete #236 block + anchor copy (:595, :729–)
waybill-cli/src/generate/spdx/v3_document.rs               # use root_edges; delete gate + anchor copy (:746–, :862)
waybill-cli/src/generate/graph_completeness/mod.rs         # adjacency from root_edges (:221–272) (R6)
waybill-cli/src/scan_fs/declared_attach.rs                 # NEW: OwnerIndex + pass (R3, R4)
waybill-cli/src/scan_fs/mod.rs                             # run the pass before lifecycle edge typing; ScanResult.declared_root_anchors
waybill-cli/src/scan_fs/package_db/git_submodule/mod.rs    # expose submodule dir → PURL map (no wire change)
waybill-cli/src/scan_fs/package_db/mod.rs                  # carry the map in diagnostics
waybill-cli/src/scan_fs/package_db/pip/mod.rs              # merge_without_override keeps every declaring file (R5)
waybill-cli/src/cli/scan_cmd.rs                            # add declared root anchors to reader anchors (:4462–)
waybill-cli/tests/declared_requirements.rs                 # NEW: integration tests
docs/ecosystems.md, CHANGELOG.md                           # behaviour + Changed entry for the format-parity corrections
```

Outside the repository, in `kusari-sandbox/waybill-fixture-therock-sdk`: `.github/workflows/measure-source.yml` and its report script (R10).

## Phasing

1. **Measure first.**
   - Commit the R1 repro arms as failing integration tests.
   - Measure the vcpkg arm (R12) and file its follow-up.
   - Run `measure-source.yml` at `main` to re-establish SC-006's baselines in CI.
2. **`root_edges` alone.** Switch all four consumers with no attachment change, then regenerate the corpus.
   - This isolates R2's format-parity shifts (R9 cases 3–7) from R3's new edges, so the golden review has one variable per step.
   - A change that cannot be explained stops here.
3. **Declared attachment.** Add `OwnerIndex`, the pass, the anchors, and the duplicate merge. Regenerate and review again (R9 cases 1, 2 and 4).
4. **Close-out.**
   - Mutations: one per FR, plus each R2 decision.
   - The gate.
   - The quality corpus.
   - `measure-source.yml` at the branch.
   - The SC-008 probe and its follow-up issue.
   - The docs, and the PR.

## Complexity Tracking

None. `root_edges` removes four copies of one rule. The pass is one index and one ancestor walk.
