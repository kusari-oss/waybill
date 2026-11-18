# Implementation Plan: Read dependency manifests in CI and docs dot-directories

**Branch**: `1613-dot-directory-manifests` | **Date**: 2026-11-18 | **Spec**: [spec.md](spec.md)

## Summary

waybill's walkers skip every dot-directory. On TheRock that hides:
- 8 pip requirements files (including the only declarations of about a dozen CI-only packages);
- 4 CMake cache scripts;
- the inputs a future GitHub Actions or Dockerfile reader needs.

**Fixes:**
- **R1/R2:** the pip, npm and CMake registrations opt into the listed CI and docs directories through the shared walker's existing `descend_into` (contract C10), matched case-insensitively. The skip helper, the file tier and the self-walking readers are unchanged.
- **R3 (FR-004a):** the pip selector also reads `<scope>-requirements*.txt` and any `*requirements*.txt` under a listed directory. That reaches `.ci/all_requirements.txt` and TheRock's two `dev-requirements.txt` files; `tox-requirements.txt` stays out.
- **R4:** the pip scope classifier labels a file under a listed directory `ci` or `docs` (C155, unchanged row).
- **R5:** a post-dedup pass gives other readers' components that live only under listed directories `Development`, with C62 `ci-directory` or `docs-directory`, two new values of an open enum.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only:
- `walk_registry` (`ReaderRegistration::descend_into`, `globset_from_patterns_case_insensitive`);
- `pip::requirements_txt` (`classify_requirements_scope`, `read_requirements_files`);
- `scan_fs::test_scope` (the model for the R5 pass);
- `globset`, `serde_json`, `tracing`.

**Zero new Cargo dependencies** (FR-013).

**Storage**: N/A. In process per scan.

**Testing**:
- **Unit:**
  - the listed-directory glob (case, nesting, non-listed names);
  - classifier precedence (parent, then name, then ancestor);
  - the widened selector (`dev-requirements.txt` read, `tox-requirements.txt` not, `all_requirements.txt` read only under `.ci`);
  - the R5 pass (only-under-listed, mixed locations, already-scoped).
- **Integration:** `waybill-cli/tests/scan_dot_directory_manifests.rs`, cases D-1 to D-12 (`contracts/wire.md`), each asserted in all three formats.
  - **Byte-identity control:** a tree with no listed directories, scanned before and after.
  - **Determinism:** 20 repeated scans.
- **Mutations** (`measurements/mutations.md`), each rule removed or inverted:
  - the listed glob (no `descend_into`);
  - case-insensitivity;
  - the nested-skip guarantee (`descend_into` forced to include `.venv`);
  - classifier ancestor walk;
  - selector widening;
  - the `tox-requirements` exclusion;
  - the R5 pass;
  - R5's "every source file" condition (made "any");
  - FR-006 (overwrite an existing scope).
- **CI:**
  - TheRock via `measure-source` (interleaved, ≥3 pairs, plus a document diff against `main`);
  - the public corpus at the branch and at `main` (`repeat=2`);
  - the quality corpus against `main`.

**Target Platform**: the waybill CLI on Linux, macOS and Windows. No platform-gated code.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: TheRock's source scan at ≤ 1.05× `main`'s median, interleaved (SC-005). The new reads are 8 requirements files and 4 CMake files; the listed directories hold about 1,000 of 335,551 files.

**Constraints**:
- byte-identical output for a tree with no listed directory and no `<scope>-requirements*.txt`;
- public-corpus goldens unchanged (predicted: none of their dot-directory content is readable, and they contain 0 names the widened selector adds).

**Scale/Scope**: TheRock: 8 requirements files under listed directories, 2 `dev-requirements.txt` elsewhere, 4 `.github` CMake scripts. Public corpus: 0 changes predicted.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies, no subprocess. |
| III. Fail closed | ✅ Only named directories are opened; an unknown dot-directory stays skipped. An unlabellable requirements name (`tox-requirements.txt`) stays unread rather than being read as runtime. |
| IV. Type-driven correctness | ✅ The listed set is one typed constant shared by the registrations, the classifier and the R5 pass. Scope uses the existing `LifecycleScope` merge. |
| V. Specification compliance | ✅ Scope is native: CycloneDX `scope`; SPDX 2.3 `DEV_DEPENDENCY_OF` / `OPTIONAL_DEPENDENCY_OF`; SPDX 3 `LifecycleScopeType`. C62 gains two values of an open enum, with the catalog row updated in the same commit; C155 is unchanged. |
| VII. Test isolation | ✅ Synthetic `waybill-fixture-*` trees in temp dirs. TheRock is never scanned locally. |
| VIII. Completeness | ✅ 10 requirements files on TheRock become readable. |
| IX. Accuracy | ✅ Every newly read declaration carries a non-runtime label; nothing CI-only reads as shipped content. |
| X. Transparency | ✅ C155 and C62 name the derivation on every affected component (FR-008: no document-scope record, by decision). |

Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1613-dot-directory-manifests/
├── spec.md
├── plan.md            # this file
├── research.md        # R1–R7
├── data-model.md
├── quickstart.md
├── contracts/wire.md  # D-1..D-12, the wire contract
├── checklists/requirements.md
└── measurements/      # mutations.md, therock.md, gate-log.md (implementation)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/
├── package_db/project_roots.rs        # + LISTED_DOT_DIRECTORIES, listed_dot_dir_class()
├── package_db/pip/mod.rs              # registration descend_into; project-root marker widened
├── package_db/pip/requirements_txt.rs # selector widened; classifier ancestor rule
├── package_db/npm/mod.rs              # registration descend_into
├── package_db/cmake.rs                # registration descend_into
├── ci_scope.rs                        # new: the R5 and R8 pass
└── mod.rs                             # declares ci_scope
waybill-cli/src/cli/scan_cmd.rs        # calls ci_scope::apply after dedup and propagation, beside the test-scope pass
docs/reference/sbom-format-mapping.md  # C62 values
docs/ecosystems.md                     # pip: which requirements names are read
waybill-cli/tests/scan_dot_directory_manifests.rs
```

**Structure Decision**: the listed set and its classes live beside `should_skip_default_descent`, in `project_roots.rs`, so the skip and its exceptions are read in one place.
