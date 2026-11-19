# Wire contract: requirements file scope

Every case is asserted in CycloneDX, SPDX 2.3 and SPDX 3 through the parity extractors for C42 (lifecycle scope) and C155 (`waybill:python-req-file-scope`). "Optional" is CycloneDX `scope: excluded`, the SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` relationship and SPDX 3 `LifecycleScopeType`, exactly as a file labelled by its parent produces today (FR-008).

Packages are `waybill-fixture-*`. Paths are relative to the scan root.

| Case | Tree | Expected |
|---|---|---|
| W-1 | `proj/docs/sphinx/requirements.txt` | Optional, C155 `docs` (FR-002) |
| W-2 | `proj/docs/userguide/requirements.txt` and `proj/Doc/x/y/requirements.txt` | both `docs`; case-insensitive |
| W-3 | `proj/tests/corpus/dbt/requirements-x.txt`; `proj/test/integration/requirements.txt` | both `test` (FR-002) |
| W-4 | `proj/requirements-doc.txt`; `proj/doc-requirements.txt` | both `docs` (FR-001); `doc-requirements.txt` is read (R2) |
| W-5 | `proj/requirements-development.txt`; `proj/requirements-documentation.txt`; `proj/testing-requirements.txt`; `proj/requirements-tests.txt` | `dev`, `docs`, `test`, `test` (FR-006, R1) |
| W-6 | `proj/requirements-docker.txt`; `proj/requirements-testinfra.txt`; `proj/requirements-devel.txt` | runtime, no C155 (word boundary) |
| W-7 | `proj/tests/docs/sphinx/requirements.txt`; `proj/docs/tests/x/requirements.txt` | `docs`; `test`: nearest wins (edge case) |
| W-8 | `proj/.github/docs/x/requirements.txt`; `proj/docs/.github/x/requirements.txt` | `docs`; `ci`: one search across both kinds (FR-003) |
| W-9 | `proj/tests/fixtures/requirements-docs.txt`; `proj/docs/x/requirements-dev.txt` | `docs`; `dev`: the name beats an ancestor (edge case) |
| W-10 | a package in `proj/docs/sphinx/requirements.txt` and in `proj/requirements.txt` | required, no C155 (FR-007) |
| W-10b | a package in `a/docs/sphinx/requirements.txt` and `b/requirements.txt` (the docs declaration merged first) | required, no C155 (FR-007; added at T016, see `measurements/mutations.md`) |
| W-11 | W-1 to W-5 together | no component is unreachable without an orphan reason (R5) |
| W-12 | the scan root itself named `tests`, holding `proj/x/requirements.txt` | runtime: the root's name labels nothing (FR-004) |
| W-13 | `tests/vendored/proj/requirements.txt` (a whole project under `tests/`) | `test` (FR-004, clarified) |
| W-14 | `proj/examples/a/requirements.txt`, `proj/benchmarks/b/`, `proj/tools/c/`, `proj/test_data/d/`, `proj/documents/e/`, `proj/testing_utils/f/`, `proj/doctor/g/` | all runtime (FR-005, partial names) |
| W-ctl | a tree with `requirements.txt`, `requirements-dev.txt`, `docs/requirements.txt` and `.github/requirements.txt` only | byte-identical to a golden of `main`'s output (FR-009) |
| W-rep | W-1 to W-14, scanned 20 times | identical each time |
