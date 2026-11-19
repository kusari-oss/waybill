# Feature Specification: Label pip requirements files under docs and test directories by those directories

**Feature Branch**: `1615-nested-req-scope`
**Created**: 2026-11-19
**Status**: Draft
**Input**: #1614. Python requirements files under `docs/` and `test/` directories deeper than their parent, and `requirements-doc.txt`, are read as runtime.

## Background

The pip requirements scope classifier (C155 `waybill:python-req-file-scope`) labels a requirements file from three signals, first match wins:
1. **Its immediate parent directory:** `docs`, `doc`, `documentation` → docs; `test`, `tests` → test; `ci`, `.ci` → ci.
2. **Its name:** `requirements-<scope>*.txt` or `<scope>-requirements*.txt`, for the scope words `dev`, `test`, `docs`, `ci`.
3. **Since spec 1613,** the nearest listed CI or docs dot-directory among its ancestors below the scan root (`.github`, `.ci`, …, `.sphinx`).

A file no signal labels reads as runtime: its packages are required.

Two shapes slip through:
- **`<project>/docs/sphinx/requirements.txt`** reads as runtime, because its parent is `sphinx`.
- **`requirements-doc.txt`** reads as runtime, because the name rule knows `docs` and not `doc`.

Measured from waybill's own document of ROCm TheRock's source tree: CI measure-source run 44477517009, waybill `99e6e2f5` (content equal to `main` after #1617). TheRock was never cloned locally.

| | count |
|---|---|
| requirements files waybill reads | 71 |
| ... labelled runtime | 51 |
| ... of those, with a `doc` name or a `docs`/`doc`/`test`/`tests` directory above their parent | 14 |
| pip packages waybill marks required | 265 |
| ... declared only in those 14 files | 98 |
| ... declared in those files and also in a runtime file outside them | 11 |

The 14 files:
- **12 Sphinx documentation sites,** 1 to 105 packages each: `docs/sphinx/requirements.txt` in hipdnn (`docs/user-guides/sphinx/`), stinkytofu, cuid, hipify, hipcc, half, openmp, amd-llvm, primbench, rocjitsu (`docs/sphinx/sphinx/`) and hip, and rccl's `docs/userguide/requirements.txt`.
- **1 archived docs file:** rocprofiler-compute's `docs/archive/requirements-doc.txt`.
- **1 test corpus:** rocjitsu's `tests/corpus/dbt/requirements-gfx1250-dbt.txt` (2 packages).

Other names seen among the 51 runtime files, recorded for the decisions below:

| Shape | Files | Packages |
|---|---|---|
| under `examples/` or `example/` | 3 | 2, 8, 6 |
| under `benchmark/` | 1 | 2 |
| under `tools/` | 3 | 2, 4, 6 |
| under `test_data/` | 1 | 2 |
| `requirements-development.txt` (a long form of `dev`; labelled by FR-006) | 1 | 3 |
| `website/handbook/requirements.txt` | 1 | 2 |

**Why the existing passes miss them** (traced in the source at `main` `52b96f1d`):
- **Spec 1533's test-scope pass** (`scan_fs/test_scope.rs`) scopes only binaries, file-tier components and installed-library records, and its directory rule runs only for an installed tree. A pip package from a source tree is none of those, so rocjitsu's test corpus is not labelled.
- **Spec 1269's transitive dev-scope propagation** carries a non-runtime scope down from a non-runtime declaration. It does not create one, so a package whose only declaration reads as runtime stays required.

Outside pip, the same shape is small on TheRock: 15 required components are declared only under `docs`/`doc`/`documentation`/`test`/`tests` directories. All 15 are Cargo crates from one test crate's lockfile (`rccl/contrib/nccl4rust/tests/cuda-oxide-smoke/Cargo.lock`).

## Clarifications

### Session 2026-11-19

- Q: Where does the ancestor search stop: the scan root, or the root of the project that declares the file? → A: The scan root. Anything under a `docs` or `tests` directory below the scan root is non-runtime, a vendored project included, as for spec 1613's dot-directories.
- Q: Does the name rule recognise the long forms of its scope words (`development`, `documentation`, `testing`)? → A: Yes.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A documentation site's requirements are not runtime dependencies (Priority: P1)

An SBOM consumer reviewing a source tree's runtime dependencies sees Sphinx, its themes and its extensions listed as required, because a project keeps its documentation requirements at `docs/sphinx/requirements.txt`. On TheRock, about a hundred such packages hide the real runtime set.

**Why this priority**: 13 of the 14 files are documentation, and a reviewer acts on the required set.

**Independent Test**: Scan a tree with `docs/sphinx/requirements.txt` and `docs/userguide/requirements.txt`. Each declared package is Optional, with C155 `docs`.

**Acceptance Scenarios**:

1. **Given** `proj/docs/sphinx/requirements.txt` declaring `waybill-fixture-theme`, **When** the tree is scanned, **Then** the package is Optional with C155 `docs`, in all three formats.
2. **Given** the same package also declared in `proj/requirements.txt`, **When** the tree is scanned, **Then** it is required, as spec 1613's merge rule gives today.
3. **Given** `proj/docs/requirements.txt`, whose parent already labels it, **When** the tree is scanned, **Then** its output is unchanged.

---

### User Story 2 - A file named for documentation is labelled docs (Priority: P1)

A project names its documentation requirements `requirements-doc.txt`. Today the name rule recognises `requirements-docs.txt` but not this spelling, so the file reads as runtime.

**Why this priority**: The same defect as US1 in its other form, and the issue's title names it.

**Independent Test**: Scan a tree with `requirements-doc.txt` and `doc-requirements.txt` at the project root. Both are labelled docs.

**Acceptance Scenarios**:

1. **Given** `proj/requirements-doc.txt`, **When** the tree is scanned, **Then** its packages are Optional with C155 `docs`.
2. **Given** `proj/doc-requirements.txt`, **When** the tree is scanned, **Then** the same.
3. **Given** `proj/requirements-docker.txt`, **When** the tree is scanned, **Then** it stays runtime: `docker` is not the word `doc`.
4. **Given** `proj/requirements-development.txt`, `proj/requirements-documentation.txt` and `proj/testing-requirements.txt`, **When** the tree is scanned, **Then** they are labelled dev, docs and test.

---

### User Story 3 - Requirements inside a test tree are test-only (Priority: P2)

A project keeps requirements for a test corpus or harness at `tests/<suite>/requirements.txt`. Nothing in that directory ships, so its packages are test dependencies.

**Why this priority**: Correct and cheap, but small on the measured tree (1 file, 2 packages).

**Independent Test**: Scan a tree with `tests/corpus/dbt/requirements-gfx1250-dbt.txt`. Its packages are Optional with C155 `test`.

**Acceptance Scenarios**:

1. **Given** `proj/tests/corpus/dbt/requirements-x.txt`, **When** the tree is scanned, **Then** its packages are Optional with C155 `test`.
2. **Given** `proj/test/integration/requirements.txt`, **When** the tree is scanned, **Then** the same.
3. **Given** the scan root itself is a directory named `tests`, **When** it is scanned, **Then** the scan root's name labels nothing, as for spec 1613's dot-directories.

---

### Edge Cases

- **Nearest directory wins.** `docs/tests/requirements.txt` is test, by its parent. `tests/docs/sphinx/requirements.txt` is docs: `docs` is nearer than `tests`. The same rule decides between these directories and spec 1613's listed dot-directories: `.github/docs/x/requirements.txt` is docs, and `docs/.github/x/requirements.txt` is ci.
- **Existing signals keep their precedence.** The parent directory, then the name, then an ancestor. `tests/fixtures/requirements-docs.txt` stays docs, by its name.
- **Case.** Directory names and name words match case-insensitively, as the parent rule does today (`Doc/`, cpython's spelling).
- **Partial words.** `documents/`, `testing_utils/`, `test_data/` and `doctor/` label nothing. A directory matches only as a whole name.
- **A package in several scoped files.** C155 holds one value. The existing choice applies unchanged when a package is declared in, for example, a docs file and a test file.
- **Paths at the scan root.** `requirements.txt` at the root, or a file whose only ancestors are the scan root and its own parent, behaves as today.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The name rule MUST recognise `doc` as the docs scope word: `requirements-doc*.txt` and `doc-requirements*.txt` are docs. The word boundary is the existing one, so `requirements-docker.txt` is unaffected.
- **FR-002**: When neither the parent directory nor the name labels a file, the nearest ancestor directory strictly below the scan root, and above the file's parent, that is named `docs`, `doc` or `documentation` MUST label it docs, and one named `test` or `tests` MUST label it test.
- **FR-003**: FR-002's directories and spec 1613's listed dot-directories MUST be one ancestor search: the nearest match of either kind labels the file.
- **FR-004**: The ancestor search MUST run to the scan root, excluding the scan root itself, as spec 1613's does. A project vendored under a `tests/` or `docs/` directory is labelled by that directory: none of it ships.
- **FR-005**: Directory names other than FR-002's MUST NOT label a file. That covers `examples`, `example`, `benchmarks`, `benchmark`, `samples`, `tools`, `scripts`, `test_data` and `website`. They hold code that runs, or data, and no convention marks their requirements as non-runtime.
- **FR-006**: The name rule MUST also recognise the long forms of its scope words: `development` (dev), `documentation` (docs), and `testing` and `tests` (test), with the existing word boundary. (`tests` added at planning, research R1: `requirements-tests.txt` fails the boundary today as `requirements-doc.txt` does.) On TheRock this labels `rocprofiler-compute/requirements-development.txt` (3 packages) dev.
- **FR-007**: A package declared both in a file FR-001 to FR-003 label and in a file that reads as runtime MUST stay required, by spec 1613's existing merge rule.
- **FR-008**: A labelled package MUST carry the native non-runtime scope in each format (CycloneDX `scope`, SPDX 2.3 relationship type, SPDX 3 `LifecycleScopeType`) and C155 with its scope word, exactly as a file labelled by its parent does today. No new catalog row.
- **FR-009**: A tree with no requirements file that FR-001 to FR-003 and FR-006 newly label MUST produce output byte-identical to `main`.
- **FR-010**: Only pip requirements files change. Components from other ecosystems declared only under docs or test directories stay as they are (TheRock: 15 Cargo crates in one test crate). A follow-up issue records them.
- **FR-011**: No new Cargo dependencies.

### Key Entities

- **Requirements file scope (C155)**: one of `dev`, `test`, `docs`, `ci`, or absent for runtime. This feature changes which files receive a value; the value set and the row are unchanged.
- **Labelling signal**: parent directory, name, or nearest ancestor (a listed dot-directory, or after this feature a docs or test directory). The first three are evaluated in that order.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On TheRock's source tree, every package declared only in the 14 files is non-runtime with C155 `docs` or `test`. Predicted 98, from the document of run 44477517009. The count is a prediction until the CI run measures it.
- **SC-002**: On TheRock, no package also declared in a runtime file outside the 14 changes scope (11 predicted), and no file outside FR-001 to FR-003's shapes changes label.
- **SC-003**: The three formats agree on C42, C62 and C155 (`waybill sbom parity-check` shows no gap that `main` does not), and there are no duplicate `bom-ref`s.
- **SC-004**: Two scans at one commit on two runners give identical documents, timestamps and serials aside.
- **SC-005**: The public corpus invariant gate stays green. Every golden change is traced to a file FR-001 to FR-003 label.
- **SC-006**: The quality corpus is compared with a `main` control. Every changed target is traced package by package (python-flask, the pants Python examples and pytorch are the likely movers).
- **SC-007**: TheRock's source scan time stays within 1.05× `main`'s median, measured with the interleaved measure-source workflow.
- **SC-008**: Every rule (FR-001, FR-002, FR-003, FR-004, FR-005, FR-006, FR-007) has a mutation that a test catches.

## Assumptions

- **The measured counts come from waybill's own document of TheRock,** read for the labels it assigns. The 98 and 11 are predictions of this feature's effect, labelled so; the CI run after implementation measures them.
- **`doc`, `docs` and `documentation` as directories,** and `test` and `tests`, are the conventions this feature targets; the parent rule already uses them. Spec 1533's list (`unittests`, `test_plugins`, `.upstream-tests`) is for installed trees and is not adopted.
- **TheRock is measured only in CI,** through `kusari-sandbox/waybill-fixture-therock-sdk`. Every fixture uses synthetic `waybill-fixture-*` package names.
- **No new annotation.** The existing derivation of a labelled file's scope already explains it, through C155 and the file in C18.
