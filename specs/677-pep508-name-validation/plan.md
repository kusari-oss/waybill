# Implementation Plan: Reject phantom pip components with malformed names (PEP 508 validation)

**Branch**: `677-pep508-name-validation` | **Date**: 2026-09-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/677-pep508-name-validation/spec.md`

## Summary

Issue #768: `waybill sbom scan` on a Cookiecutter-style Python project skeleton emits a phantom `pkg:pypi/{{package-name}}@0.0.0` main-module component because the pip reader does not validate the `[project].name` field against PEP 508 before emission. Fix: pre-filter `project_roots` in the pip reader's `read()` function — parse `pyproject.toml`, extract the effective name (`[project].name` OR `[tool.poetry].name`), apply PEP 508's regex, drop the whole `project_root` from downstream emission if the name fails. Log exactly one WARN per rejected manifest. Add `names_rejected=<N>` to the reader's structured completion log.

Design surface: one new module `waybill-cli/src/scan_fs/package_db/name_validation.rs` exposing `is_pep508_name(name: &str) -> bool` plus a `NameValidationError` type. Pip reader adds a `filter_project_roots_by_name` pre-pass that consumes the module. Future readers add sibling `is_<ecosystem>_name` functions to the same module (FR-005 pattern).

Whole-manifest reject per spec Clarifications Session 2026-09-03 Q1: dropping the `project_root` from the filtered set causes BOTH the `pyproject_declared_deps` loop AND the `build_pip_main_module_entry` loop to skip that manifest — no main-module, no `[project.dependencies]`, no `[project.optional-dependencies]` emit from a rejected manifest.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain, no nightly).

**Primary Dependencies**: Existing only — `regex = "1"` (already a workspace direct dep; used pervasively), `tracing` (existing WARN log surface), `toml = "0.8"` (already used by the pip reader for pyproject.toml parsing). **Zero new Cargo dependencies at the workspace level** (SC-006).

**Storage**: N/A. Validation is stateless per invocation.

**Testing**: `cargo test -p waybill --bin waybill scan_fs::package_db::pip` for existing unit + new-in-file tests. `cargo test --test scan_python_m670` and neighboring pip integration tests for regression coverage.

**Target Platform**: All platforms waybill supports. No platform-specific code.

**Project Type**: Single-project (waybill-cli). Change surface confined to `waybill-cli/src/scan_fs/package_db/pip/` + `waybill-cli/src/scan_fs/package_db/name_validation.rs` (new file) + a synthetic test fixture + an integration test.

**Performance Goals**: N/A (bug fix; regex compile-once via `OnceLock` matches the reader's existing patterns).

**Constraints**:
- SC-006: zero new Cargo dependencies.
- SC-007: fix code diff ≤ 200 lines of production code across the reader + helper.
- FR-006: byte-identical output for pre-fix passing fixtures — hard non-regression requirement.

**Scale/Scope**: 1 new module (~50 lines including tests-in-file), 1 filter pass wired into `pip/mod.rs::read()` (~20 lines), 1 new synthetic fixture directory + 1 new integration test (~40 lines). Estimated total production diff: ~70 lines. Comfortably under SC-007's 200-line ceiling.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)** — ✅ PASS. Pure Rust; no new deps of any kind.
- **Principle II (eBPF-Only Observation)** — ✅ N/A. Fix is in the filesystem-scan mode's pip reader — not a new discovery source.
- **Principle III (Fail Closed)** — ✅ PASS. This is the OPPOSITE of fail-closed at the reader level — it's fail-loud on malformed names. But the spec explicitly targets a fail-open path where the reader silently emits phantoms; the fix converts that into fail-loud (WARN + drop). Constitution's supplemental-reader fail-open carve-out is preserved: the rest of the scan continues, other manifests continue to emit.
- **Principle IV (Type-Driven Correctness)** — ✅ PASS. New `NameValidationError` type carries structured failure reason. No new `.unwrap()` in production paths.
- **Principle V (Specification Compliance)** — ✅ PASS + REINFORCED. PEP 508 is the domain-authoritative name specification for PyPI. Validating names against it improves conformance, does not add new `waybill:*` annotations.
- **Principle VI (Three-Crate Architecture)** — ✅ PASS. All changes stay in `waybill-cli`.
- **Principle VII (Test Isolation)** — ✅ PASS. Unit tests run in default lane. No new privileged operations.
- **Principle VIII (Completeness)** — ✅ PASS. Completeness measures "dependencies actually fetched but absent from the SBOM". This fix REJECTS a phantom (a name that would never resolve on PyPI); it does not silently drop a real dep. Whole-manifest reject explicitly drops declared-deps from a rejected manifest, but those deps are ALSO placeholders (their names may be valid but the containing manifest is a template — the deps are not "actually fetched during a build", they're template scaffolding).
- **Principle IX (Accuracy)** — ✅ PASS + PRIMARY MOTIVATION. Principle IX says: "PURL resolution ... MUST be validated before inclusion; ambiguous or low-confidence matches MUST be flagged rather than silently included as definitive." A `pkg:pypi/{{package-name}}` PURL is a low-confidence (zero-confidence) match — validation catches it before inclusion.
- **Principle X (Transparency)** — ✅ PASS. WARN log (FR-002) + `names_rejected` structured completion count (FR-003) provide operator-visible signal for every rejection.
- **Principle XI + XII (Enrichment / External Data Sources)** — ✅ N/A. No enrichment path touched.

**Gate result: PASS with no violations. No Complexity Tracking entries.**

## Project Structure

### Documentation (this feature)

```text
specs/677-pep508-name-validation/
├── plan.md              # This file
├── spec.md              # Feature spec (already exists)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── name-validation-module.md
├── checklists/
│   └── requirements.md  # Already generated
└── tasks.md             # Phase 2 output (not yet)
```

### Source Code (repository root)

Change surface:

```text
waybill-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           ├── name_validation.rs         # NEW — is_pep508_name + NameValidationError + tests
│           ├── mod.rs                     # +1 `pub(crate) mod name_validation;` line
│           └── pip/
│               └── mod.rs                 # ADD filter_project_roots_by_name pre-pass in read()
│                                          # ADD names_rejected counter to reader-complete log
└── tests/
    ├── fixtures/
    │   └── pip/
    │       └── malformed_name_placeholder/  # NEW synthetic fixture directory
    │           └── pyproject.toml           # `name = "{{package-name}}"` + valid dep list
    └── scan_python_m677.rs                  # NEW integration test — verifies zero components + one WARN
```

**Structure Decision**: Single-project layout. All changes under `waybill-cli/`. New module lives alongside other package-db shared code (`waybill-cli/src/scan_fs/package_db/`). No `waybill-common` touch — the helper's audience is `waybill-cli` readers, not cross-crate consumers.

## Complexity Tracking

*Empty — Constitution Check passed with no gates violated.*
