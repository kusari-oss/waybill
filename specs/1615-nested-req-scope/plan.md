# Implementation Plan: Label pip requirements files under docs and test directories by those directories

**Branch**: `1615-nested-req-scope` | **Date**: 2026-11-19 | **Spec**: [spec.md](spec.md)

## Summary

The pip requirements classifier labels a file by its parent directory, its name, or (since spec 1613) a listed dot-directory above it. A Sphinx site at `docs/sphinx/requirements.txt`, a test corpus at `tests/corpus/x/requirements.txt` and a file named `requirements-doc.txt` all read as runtime. On TheRock that is 14 files and a predicted 98 required packages.

**Fixes:**
- **R1–R2:** the name rule takes a set of words per scope (`doc`/`docs`/`documentation`, `dev`/`development`, `test`/`tests`/`testing`, `ci`). Reading follows the classifier, so the newly labelled `<word>-requirements*.txt` names are read.
- **R3:** when the parent and the name say nothing, one nearest-first search over the ancestors below the scan root checks each directory as a listed dot-directory or as `docs`/`doc`/`documentation`/`test`/`tests`. The search is pip-only: `listed_ancestor_class` and spec 1613's `ci_scope` pass are untouched.
- **R4–R5:** scope merge, C155's single value and attachment are unchanged.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- `pip/requirements_txt.rs`: `classify_requirements_scope`, `classify_requirements_scope_under`, `is_requirements_file`, `matches_scope_filename`;
- `project_roots::listed_dot_dir_class`.

**Zero new Cargo dependencies** (FR-011).

**Storage**: N/A. Per-file classification in process.

**Testing**:
- **Unit tests** in `requirements_txt.rs`:
  - the filename table extended with R1's words and boundary negatives;
  - the ancestor search: nearest wins across kinds, the root bound, a path outside the root, partial names.
- **Integration:** `waybill-cli/tests/scan_nested_req_scope.rs`, cases W-1..W-14, W-ctl and W-rep (`contracts/wire.md`), each asserted in all three formats through the C42 and C155 extractors.
- **Mutations** (`measurements/mutations.md`), one per rule:
  - FR-001 and FR-006's words;
  - FR-002's names;
  - FR-003's single search;
  - FR-004's root bound;
  - FR-005's exact names;
  - R2's reading of new names;
  - FR-007 relies on existing code: one mutation removes spec 1613 R8's label removal, one makes the deduplicator's scope merge prefer the non-runtime scope.
- **CI:**
  - TheRock measure-source, two interleaved runs against `main`, with a document diff, `parity-check` and a repeat comparison across runners;
  - the public corpus at the branch with `repeat=2`;
  - the quality corpus against a `main` control.

**Target Platform**: the waybill CLI on Linux, macOS and Windows. Paths are compared by component, so separators do not matter.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-007, TheRock's source scan at ≤ 1.05× `main`'s median (R8). No new file reads on TheRock.

**Constraints**:
- FR-009: byte-identical output for a tree with no newly labelled file;
- non-pip components unchanged (FR-010);
- `ci_scope` behaviour unchanged.

**Scale/Scope**: TheRock reads 71 requirements files; 14 change label, plus 1 by FR-006.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies, no subprocess. |
| III. Fail closed | ✅ An unmatched file stays runtime, as today. Only exact directory names and whole scope words label. A path outside the scan root gets no new signal (R3). |
| IV. Type-driven correctness | ✅ The word sets are typed constants, one per C155 value. The labels come from the existing `RequirementsScope`, so no new string can reach C155. |
| V. Specification compliance | ✅ Scope is native in each format (CycloneDX `scope`, SPDX 2.3 relationship type, SPDX 3 `LifecycleScopeType`). C155's closed value set is unchanged; no new row. |
| VII. Test isolation | ✅ Synthetic `waybill-fixture-*` trees in temp dirs; TheRock is scanned only in CI. |
| VIII. Completeness | ✅ No component is added or removed except packages of the newly read `<word>-requirements*.txt` names (R2). |
| IX. Accuracy | ✅ Documentation and test-only packages stop reading as runtime: 98 predicted on TheRock. A package with any runtime declaration stays runtime (FR-007). |
| X. Transparency | ✅ C155 names the scope, and C18 names the file that gave it. |

Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1615-nested-req-scope/
├── spec.md
├── plan.md              # this file
├── research.md          # R1–R8
├── data-model.md
├── quickstart.md
├── contracts/wire.md    # W-1..W-14, W-ctl, W-rep
├── checklists/requirements.md
└── measurements/        # later: mutations.md, therock.md, corpus.md, gate-log.md
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs   # R1 word sets; R2 reading; R3 ancestor search
waybill-cli/tests/scan_nested_req_scope.rs                   # W cases
waybill-cli/tests/fixtures/nested_req_scope/ctl{,-golden}/   # W-ctl
docs/ecosystems.md                                           # pip: how a requirements file is labelled
docs/reference/sbom-format-mapping.md                        # C155's description: the signals
CHANGELOG.md
```

**Structure decision.** All logic stays in `requirements_txt.rs`, beside the classifier it extends. `project_roots.rs` is read and not changed, because its function also serves the walker and `ci_scope` (R3).
