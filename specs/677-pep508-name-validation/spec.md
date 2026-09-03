# Feature Specification: Reject phantom pip components with malformed names (PEP 508 validation)

**Feature Branch**: `677-pep508-name-validation`
**Created**: 2026-09-03
**Status**: Draft
**Input**: User description: "Reader-agnostic name validation to reject phantom components with malformed names (e.g., Cookiecutter placeholders like `{{package-name}}`). First cut applies PEP 508 name validation at the pip reader's emission point; scans containing template directories emit zero phantom components with a WARN log naming the offending pyproject.toml path. Follow-up work extends the pattern to other readers ecosystem-by-ecosystem. Anchor to Principle IX (Accuracy). Closes #768."

## Clarifications

### Session 2026-09-03

- Q: On a manifest whose `[project].name` fails PEP 508 validation, what's the emission scope? → A: **Whole-manifest reject** — emit zero components from that manifest (no main-module, no declared-deps, no optional-deps). Template manifests are wholesale placeholder; their dep lists aren't authoritative either.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - SBOM operator scanning a monorepo with a project template (Priority: P1)

An operator runs `waybill sbom scan` on a real-world monorepo that includes a Cookiecutter-style Python project skeleton at some path like `experimental/ml/project-skeleton/`. That directory has a `pyproject.toml` containing literal Jinja placeholders such as `name = "{{package-name}}"`. Today, waybill emits a phantom `pkg:pypi/{{package-name}}@0.0.0` component with `waybill:component-role=main-module`, polluting the SBOM. The operator expects the emitted SBOM to contain zero components sourced from template directories — the raw placeholders are not real package identities.

**Why this priority**: This is a real-world accuracy defect observed in a customer scan (issue #768 reproduction). Consumers of the SBOM see a plausible-looking `pkg:pypi/{{package-name}}` entry, potentially trigger false-positive vulnerability alerts against a non-existent package, or waste triage cycles chasing a name that will never resolve on PyPI. Principle IX (Accuracy) makes this a blocking correctness bug, not a nice-to-have.

**Independent Test**: A synthetic fixture with a `pyproject.toml` containing `name = "{{package-name}}"` produces zero components in the emitted SBOM plus exactly one WARN log line naming the offending file path.

**Acceptance Scenarios**:

1. **Given** a directory with a `pyproject.toml` whose `[project].name` is `"{{package-name}}"` (or any string that fails PEP 508's name-regex) AND whose `[project.dependencies]` list contains valid names like `"requests"` and `"click"`, **When** the operator runs `waybill sbom scan --path <parent>`, **Then** the emitted SBOM contains **zero** components sourced from that manifest (no main-module, no `requests`, no `click`) AND the scan stderr contains one WARN log line naming the offending path + the malformed name. Whole-manifest reject per Session 2026-09-03 clarification.
2. **Given** the same directory, **When** the operator inspects the emitted SBOM, **Then** no component has `waybill:component-role=main-module` with a purl containing `{{`, `}}`, or any other non-PEP-508 character.
3. **Given** the operator's real production monorepo (with valid package names), **When** the operator runs the scan, **Then** the emitted SBOM is byte-identical to a pre-fix scan of the same input — no legitimate component is silently dropped.

---

### User Story 2 - waybill maintainer extending validation to another reader (Priority: P2)

A future waybill contributor wants to add the same name-validation gate to a different reader (npm, maven, gem, cargo, etc.). They need a clear pattern to follow — an ecosystem-agnostic validation utility plus a per-ecosystem regex/predicate — and reasonable test scaffolding to verify the new reader's coverage.

**Why this priority**: Issue #768's "reader-agnostic" framing anticipates follow-up work. Building the pip validation as reader-agnostic infrastructure (shared helper + per-ecosystem call sites) makes those follow-ups cheap. P2 because the future contributor scenario benefits from the initial fix's shape choices even though the fix ships pip-only.

**Independent Test**: A new module (or utility function) exposes a stable API surface that a follow-up reader can invoke. Documentation explains the extension pattern.

**Acceptance Scenarios**:

1. **Given** the pip reader now invokes a validation helper, **When** a future contributor adds npm validation, **Then** they can reuse the same helper interface — supplying only the npm-specific name predicate — without modifying the pip integration.
2. **Given** the helper's API, **When** documented in the source-level or spec-level artifacts, **Then** it names the "PEP 508 for pypi, PEP 508-analog per ecosystem" pattern explicitly.

---

### User Story 3 - Transparency for skipped malformed names (Priority: P3)

An operator whose scan silently drops a component wants to know why. Any component that would have been emitted but was rejected at validation must be visible in the scan output as a warning — not silently dropped.

**Why this priority**: Transparency is Constitution Principle X. This story ensures the fix does not create a new class of silent-drop. P3 because the WARN log is already implicit in acceptance scenario 1 of US1; this story extracts the "operator visibility" concern into its own testable line.

**Independent Test**: Every synthetic fixture whose emission would have been rejected produces exactly one WARN log line with a stable format (fixture path + malformed name + reason).

**Acceptance Scenarios**:

1. **Given** any fixture containing a malformed name, **When** the scan completes, **Then** the scan stderr contains exactly one WARN line per rejected emission with structured fields for `path`, `name`, and `reason`.
2. **Given** RUST_LOG=info level, **When** the scan runs, **Then** the reader's structured completion log (per m068 convention) reports a `names_rejected=<N>` field alongside the existing `components_emitted=<M>` field.

---

### Edge Cases

- **Empty name** (e.g., `name = ""` in `pyproject.toml`): fails PEP 508. Reject.
- **Whitespace-only name** (`name = "   "`): fails PEP 508. Reject.
- **Valid PyPI name with unusual case** (e.g., `Django`, `PyYAML`): the PyPI name regex is case-insensitive per PEP 508 — accept, preserving the case for downstream PURL construction.
- **Name that is a placeholder but happens to match PEP 508** (e.g., `PLACEHOLDER`, `TODO`, `example-package`): validation is regex-based, not semantic. These pass through and emit as normal components. Out of scope for this feature — no reasonable regex distinguishes these from real names.
- **Version field also templated** (`version = "{{version}}"`): version validation is a separate concern. If the version is malformed but the name is valid, this feature does NOT reject the component. Out of scope.
- **Malformed name + valid declared-dep list**: whole manifest is rejected (all sourced components dropped) per the Session 2026-09-03 clarification. Template manifests are not partially trustworthy; their dep lists are placeholder scaffolding, not authoritative dep declarations.
- **Name contains dots/hyphens/underscores** (`my-pkg`, `my_pkg`, `my.pkg`): all valid per PEP 508. Accept.
- **Name begins or ends with a separator** (`.pkg`, `pkg-`): fails PEP 508 (must start and end with alphanumeric). Reject.
- **`pyproject.toml` has no `[project]` block**: unchanged from current behavior — the pip reader doesn't emit a main-module component from that file. Not affected by this fix.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The pip reader MUST validate the `[project].name` field of every parsed `pyproject.toml` against PEP 508's name regex (`^[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?$`, case-insensitive) before constructing a `pkg:pypi/*` main-module component.
- **FR-002**: When the validation fails, the reader MUST NOT emit ANY component sourced from that manifest — the main-module component, declared dependencies (`[project].dependencies`), and optional/dev dependencies (`[project.optional-dependencies]`) are all dropped as a bundle per the Session 2026-09-03 clarification. Reader MUST log exactly one WARN line with structured fields for the source file path and the offending name.
- **FR-003**: The reader's structured completion log MUST report a `names_rejected=<N>` field alongside the existing counts, where N is the count of manifests whose names failed validation in this scan.
- **FR-004**: Validation MUST be surfaced as a reusable helper (utility function or module) so that follow-up work applying the same pattern to non-pip readers can extend the helper rather than duplicating the validation logic.
- **FR-005**: The helper MUST accept an ecosystem-specific name predicate (or regex) rather than hard-coding PEP 508. First cut only wires the pip reader with the PEP 508 predicate; the helper's shape supports future wire-ups without modification.
- **FR-006**: For any input where every parsed manifest has a valid name, the emitted SBOM MUST be byte-identical to a pre-fix scan of the same input — this is a hard non-regression requirement.
- **FR-007**: New tests MUST cover the following behavior categories. Clauses (a)–(h) map to **unit tests** on the name-validation helper — the contract at `contracts/name-validation-module.md` decomposes them into 14 concrete test rows (7 accept + 7 reject cases). Clause (i) maps to an **integration test** covering the whole-manifest reject at the reader level.
  - (a) valid name emits normally
  - (b) `{{...}}` placeholder rejects with WARN
  - (c) empty-string name rejects
  - (d) whitespace-only rejects
  - (e) name-starts-with-separator rejects
  - (f) name-ends-with-separator rejects
  - (g) `Django`/`PyYAML`-style case-mixed names accepted (case preserved for downstream normalization)
  - (h) `my.pkg`/`my-pkg`/`my_pkg`/`zope.interface` valid separators accepted; also common invalid characters (`@`, whitespace) rejected as reason-class "contains invalid character(s)"
  - (i) whole-manifest reject — a manifest with malformed `[project].name` and valid `[project.dependencies]` emits zero components from that manifest (per Session 2026-09-03 clarification)

### Key Entities

- **Name validation helper**: a stateless function or module in shared code (candidate location: `waybill-common` or `waybill-cli/src/scan_fs/`) that takes a string and an ecosystem-specific predicate and returns `Ok(())` or a structured `NameValidationError` naming the reason for rejection.
- **PEP 508 name predicate**: a concrete instantiation of the helper's predicate parameter, applying PEP 508's regex to a PyPI package name.
- **Pip reader integration point**: the emission site in the pip reader where a `pkg:pypi/*` main-module component is constructed from a parsed `pyproject.toml`. The validation call gates this construction.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning a synthetic fixture whose `pyproject.toml` has `name = "{{package-name}}"` emits zero `pkg:pypi/{{package-name}}*` components. Pre-fix baseline for the same fixture: 1 phantom component.
- **SC-002**: The same fixture's scan stderr contains exactly one WARN log line naming the offending file path and the malformed name string.
- **SC-003**: The reader's structured completion log reports `names_rejected=1` on the same fixture.
- **SC-004**: Scanning an existing valid-name pip fixture emits an SBOM byte-identical to the pre-fix scan output. Non-regression check against the existing pip integration test suite.
- **SC-005**: 14 new unit tests land per the contract's testing table at `contracts/name-validation-module.md` (covering FR-007 clauses (a)–(h)) + 1 new integration test covering FR-007 clause (i). All 15 tests pass. The contract's 14-row testing table is the authoritative count for the unit-test surface.
- **SC-006**: Zero new Cargo dependencies at the workspace level. `regex` is already in the workspace.
- **SC-007**: Fix code diff stays under 200 lines of production code across the reader + helper.

## Assumptions

- The pip reader currently has a single or small number of emission points where a `pkg:pypi/*` main-module component is constructed from a parsed `pyproject.toml`. Planning-phase research confirms the exact site(s) — if there are multiple, all get the same gate.
- PEP 508's name regex is authoritative for PyPI package names. Alternatives (PEP 426, PyPA packaging library's rules) resolve to the same regex character class in practice.
- The tester's monorepo where this bug was observed uses a Cookiecutter template that leaves Jinja placeholders in `[project].name`. Similar templates (Cruft, Copier) produce the same shape. If a future template system uses a different placeholder syntax, PEP 508 validation still rejects the shape (`{`, `}`, `$`, `%`, `<`, `>` are all non-PEP-508 characters).
- The helper's shape is designed for future extension but MUST NOT be wired to any reader other than pip in this feature. Extending to npm, maven, gem, cargo, etc. is explicitly follow-up work.
- Version-field validation is out of scope for this feature. A template `version = "{{version}}"` alongside a valid `name` does NOT trigger rejection. If version-field validation becomes desired, that is a separate feature.
- No changes required to `waybill-common/` beyond potentially housing the shared helper. If the helper lives in `waybill-cli/src/scan_fs/` instead, no `waybill-common` touch is needed.
