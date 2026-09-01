# Feature Specification: Fix critical Python under-detection

**Feature Branch**: `670-pip-under-detection-fix`
**Created**: 2026-08-31
**Status**: Draft
**Input**: User description: "fix the critical python issues"

## Context

The kusari-sandbox `test-*` sweep on 2026-08-31 surfaced dramatic under-detection of Python components on real Python OSS projects. Waybill scans of source trees produce component counts that are 1-2 orders of magnitude below the declared-dependency reality. See issue **#743** for the sweep data.

| Project | Total components | pypi components | Expected order-of-magnitude |
|---|---:|---:|---:|
| microsoft/markitdown | 5 | 4 | ~50 |
| OctoPrint | 13 | 3 | ~100 |
| python/cpython | 187 | 16 | ~100 |

Diagnostic on what those pypi components actually are: **the project itself and its sub-projects** (e.g., `markitdown@0.0.0-unknown`, `markitdown-mcp@0.0.0-unknown`, `OctoPrint@0.0.0-unknown`) plus a handful of tooling deps in cpython's `Doc/tools/`. **Zero third-party runtime dependencies** are surfaced for any of the three projects.

Root cause hypothesis: waybill's Python reader today relies primarily on installed `.dist-info` directories (site-packages). Source trees without a materialized virtualenv are not being read from their declared-dependency manifests (`pyproject.toml`, sibling lockfiles, `requirements.txt` files, `setup.py`).

## Clarifications

### Session 2026-08-31

- Q: When a directory contains 2+ lockfiles for the same manifest (e.g., `uv.lock` + `poetry.lock` during a tool migration), what's waybill's posture? → A: Read all discovered lockfiles; m191 reconciler collapses same-PURL entries by PURL; version disagreements are preserved as multiple `source_file_paths` entries so diagnostic evidence remains visible downstream.
- Q: When walking for `requirements*.txt`, should waybill apply Python-specific pruning by default? → A: Yes. Waybill default-prunes well-known virtualenv / vendored-site-packages directories (`**/site-packages/`, `**/.venv/`, `**/venv/`, `**/.tox/`, `**/*.egg-info/`, `**/build/`, `**/.eggs/`) before descending. Overridable via `--include-python-vendored` (or equivalent). Matches m174's VCS-directory skip pattern.
- Q: Does v1 support the Poetry-legacy `[tool.poetry.dependencies]` / `[tool.poetry.group.*.dependencies]` sections in `pyproject.toml`? → A: Yes. Waybill reads Poetry-legacy sections alongside PEP 621. Precedence: PEP 621 `[project.dependencies]` if present; otherwise fall back to Poetry-legacy. `[tool.poetry.dev-dependencies]` and `[tool.poetry.group.<name>.dependencies]` are tagged with the same optional-scope mechanism (m179/m180) using the group name as the scope tag.
- Q: How does waybill scope-tag components discovered from `requirements*.txt` files? → A: Filename + parent-dir heuristic. Bare `requirements.txt` at project-standard locations → `main` scope. Files matching `requirements-dev*.txt`, `requirements-test*.txt`, `requirements-docs*.txt`, `dev-requirements.txt`, `test-requirements.txt`, `docs-requirements.txt`, OR files under a `docs/`, `test[s]/`, `ci/` parent-dir → `optional` scope with a `waybill:python-req-file-scope` annotation naming the derived scope (`dev`, `test`, `docs`, `ci`).
- Q: When a PEP 508 direct-URL / VCS entry (`some-pkg @ git+https://.../@rev`) is parsed, what PURL type does waybill emit? → A: `pkg:pypi/<name>@<rev-or-unresolved>` plus a `waybill:direct-url-source` annotation carrying the full URL and resolved rev. Version is the git rev when resolvable; otherwise `unresolved` with the m236 reason. Matches pip's own PEP 610 `direct_url.json` metadata shape (PyPI-ecosystem slot preserved, source URL preserved as evidence).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Modern Python project scan surfaces its declared dependencies (Priority: P1)

An SBOM operator scans a modern Python source tree (`pyproject.toml`-based, with a paired lockfile such as `uv.lock`, `poetry.lock`, `pdm.lock`, or `Pipfile.lock`). Waybill emits one component per declared/locked dependency, including transitive deps from the lockfile.

**Why this priority**: `pyproject.toml` is the current Python packaging standard (PEP 621). Almost every modern Python project uses it. Getting this right unlocks correct SBOMs for the majority of active OSS. The markitdown fixture in the sweep is the canonical case (yields 4 components today, should yield ~50).

**Independent Test**: Clone `kusari-sandbox/test-markitdown` (28 MB); scan with `waybill sbom scan --path <clone> --offline --no-deep-hash`; verify the emitted CDX contains ≥ 30 `pkg:pypi/*` components (up from 4 today). No installation of the project, no virtualenv creation, no network access required.

**Acceptance Scenarios**:

1. **Given** a source tree containing `pyproject.toml` + `uv.lock`, **When** waybill scans it, **Then** every entry in the lockfile is emitted as a separate `pkg:pypi/*` component with its resolved version.
2. **Given** a source tree containing `pyproject.toml` + `poetry.lock`, **When** waybill scans it, **Then** every locked package is emitted with its resolved version.
3. **Given** a source tree containing `pyproject.toml` with `[project.dependencies]` but no lockfile, **When** waybill scans it, **Then** each declared dependency is emitted as a design-tier component (versioned via its declared constraint or as `unresolved` per m236).
4. **Given** a source tree containing `pyproject.toml` with `[project.optional-dependencies]` groups, **When** waybill scans it, **Then** each optional-group dependency is emitted with the appropriate optional-scope tag (matching the m179/m180 pattern used for npm).

---

### User Story 2 - Legacy-heavy source tree surfaces its requirements files (Priority: P2)

An SBOM operator scans a Python source tree that uses one or more `requirements*.txt` files, potentially at non-root paths (e.g., `docs/requirements.txt`, `tests/requirements.txt`, `Tools/scripts/requirements.txt`). Waybill discovers every such file and emits one component per pinned entry.

**Why this priority**: Requirements files remain the canonical Python legacy-dep-declaration mechanism and appear pervasively in projects that also ship `pyproject.toml`. The cpython fixture (16 components today, should yield ~100) is the canonical case — cpython uses requirements files across `Doc/`, `Tools/`, and CI configs.

**Independent Test**: Clone `kusari-sandbox/test-cpython` (189 MB); scan with `waybill sbom scan --path <clone> --offline --no-deep-hash`; verify emitted CDX contains ≥ 50 `pkg:pypi/*` components (up from 16 today).

**Acceptance Scenarios**:

1. **Given** a source tree containing `requirements.txt` at the root, **When** waybill scans it, **Then** each pinned package is emitted as a component.
2. **Given** a source tree containing `requirements*.txt` files at arbitrary tree depths, **When** waybill scans it, **Then** every discovered file is parsed and its packages emitted.
3. **Given** a `requirements.txt` referencing `-r other-requirements.txt`, **When** waybill parses it, **Then** the referenced file is also parsed (recursively, with a documented depth limit to prevent cycles).
4. **Given** an unpinned entry (`requests`, no version constraint), **When** waybill parses it, **Then** the component is emitted with `version = "unresolved"` and the m236 `waybill:unresolved-reason` annotation set to a Python-specific reason string.

---

### User Story 3 - Legacy setup.py source tree surfaces its install_requires (Priority: P3)

An SBOM operator scans a Python source tree that uses `setup.py` with an `install_requires=[...]` literal list (no `pyproject.toml`, no lockfile). Waybill statically parses the list and emits one component per entry.

**Why this priority**: Legacy but still widely deployed. The OctoPrint fixture (3 components today, should yield ~50) is the canonical case. Static parse only — no arbitrary Python code execution (Principle I: pure Rust, zero embedded interpreters).

**Independent Test**: Clone `kusari-sandbox/test-OctoPrint` (30 MB); scan with `waybill sbom scan --path <clone> --offline --no-deep-hash`; verify emitted CDX contains ≥ 30 `pkg:pypi/*` components (up from 3 today).

**Acceptance Scenarios**:

1. **Given** a `setup.py` file containing a `setup(..., install_requires=["a>=1", "b<2", "c==3.0"], ...)` call with literal list, **When** waybill scans the tree, **Then** each list entry is parsed and emitted as a component.
2. **Given** a `setup.py` where `install_requires` is set dynamically (e.g., loaded from a variable), **When** waybill scans the tree, **Then** the file is skipped with a debug-level log and no components are fabricated (safe under-detection is preferred over hallucination).
3. **Given** a `setup.cfg` file containing `[options] install_requires = ...`, **When** waybill scans the tree, **Then** the entries are parsed and emitted (INI-style, no code execution).

---

### Edge Cases

- **Multiple manifests in one tree**: A project with both `pyproject.toml` AND `requirements.txt` MUST have both readers fire; components MUST be deduplicated by PURL at the reconciliation step (m191 substrate).
- **Lockfile vs manifest disagreement**: When a `pyproject.toml` declares constraints and a paired lockfile locks specific versions, the lockfile version wins (produces one component; the manifest constraint is discarded but its scope-tagging is preserved).
- **Multiple lockfiles in one directory** (e.g., `uv.lock` + `poetry.lock` during a tool migration): Waybill reads all of them. The m191 reconciler collapses same-PURL entries into one component. If two lockfiles lock the SAME package at DIFFERENT versions, both `source_file_paths` are preserved on the reconciled component so downstream consumers can see the disagreement in evidence.
- **Sub-project trees**: monorepos with multiple `pyproject.toml` files at nested paths (e.g., `packages/<name>/pyproject.toml`) MUST each be treated as an independent Python project (m127 workspace-member pattern; matches how markitdown ships 4 sub-projects).
- **Cyclic `-r` references** in requirements files: MUST be detected and short-circuited without crashing (bounded recursion depth, cycle tracking).
- **Requirements-file syntax quirks**: comments (`# ...`), blank lines, environment markers (`; python_version >= '3.10'`), URL-based entries (`git+https://...`), and editable installs (`-e .`) MUST be handled without crashing; unsupported constructs MUST warn-and-skip (not warn-and-fail-scan).
- **PEP 508 markers**: environment markers (`;`-suffixed) that evaluate to false on the scanning host MUST NOT cause the entry to be omitted — the component is a *declared* dep, not an *effective* dep, and should be emitted with its marker preserved as an annotation.

## Requirements *(mandatory)*

### Functional Requirements

**Manifest reading**:

- **FR-001**: Waybill MUST read `pyproject.toml` `[project.dependencies]` array and emit one component per entry.
- **FR-002**: Waybill MUST read `pyproject.toml` `[project.optional-dependencies]` groups and emit each entry with the group name preserved as scope metadata (matching the m179/m180 optional-scope annotation shape).
- **FR-003**: Waybill MUST read `pyproject.toml` `[dependency-groups]` (PEP 735) when present.
- **FR-003a**: Waybill MUST read Poetry-legacy `[tool.poetry.dependencies]`, `[tool.poetry.dev-dependencies]`, and `[tool.poetry.group.<name>.dependencies]` sections in `pyproject.toml`. Precedence: if PEP 621 `[project.dependencies]` is present it is authoritative; otherwise waybill falls back to Poetry-legacy sections. Poetry group / dev-dependencies MUST be scope-tagged via the m179/m180 optional-scope mechanism using the group name (`dev`, or `<group-name>`) as the scope value.
- **FR-004**: Waybill MUST discover `requirements*.txt` files at arbitrary depths under the scan root (subject to existing exclusion mechanisms — `--exclude`, `.gitignore`-style, walker safeguards). Additionally, waybill MUST default-prune well-known Python virtualenv / vendored-site-packages directories from the walk: `**/site-packages/`, `**/.venv/`, `**/venv/`, `**/.tox/`, `**/*.egg-info/`, `**/build/`, `**/.eggs/`. The default-prune list is overridable via an `--include-python-vendored` flag (or equivalent opt-in mechanism).
- **FR-005**: Waybill MUST parse discovered `requirements*.txt` files, handling comments, blank lines, `-r file` references, environment markers, and URL-based entries per PEP 508.
- **FR-005a**: Waybill MUST scope-tag components emitted from `requirements*.txt` files using a filename + parent-directory heuristic. Bare `requirements.txt` at project-standard locations (project root, or beside a `pyproject.toml`/`setup.py`) → `main` scope. Files matching `requirements-dev*.txt`, `requirements-test*.txt`, `requirements-docs*.txt`, `dev-requirements.txt`, `test-requirements.txt`, `docs-requirements.txt`, OR any `requirements*.txt` under a `docs/`, `test[s]/`, or `ci/` parent-directory → `optional` scope. Waybill MUST emit a `waybill:python-req-file-scope` annotation naming the derived scope name (`dev`, `test`, `docs`, `ci`) on each such component.
- **FR-005b**: When waybill parses a PEP 508 direct-URL or VCS-URL entry (e.g., `some-pkg @ git+https://github.com/foo/bar.git@abc123` or `some-pkg @ https://example.com/pkg-1.0.tar.gz`), waybill MUST emit `pkg:pypi/<name>@<version>` where `<version>` is the git rev when the URL is a VCS reference and it is statically resolvable from the URL, otherwise `unresolved` (with a m236 `waybill:unresolved-reason` annotation using a Python-specific reason string such as `python-direct-url-unresolved`). Waybill MUST attach a `waybill:direct-url-source` annotation carrying the full URL and (if resolvable) the resolved rev, matching the shape of pip's PEP 610 `direct_url.json` metadata.
- **FR-006**: Waybill MUST statically parse `setup.py` for a top-level `setup(install_requires=[literal-string-list], ...)` call and emit each string as a component. Non-literal / dynamic constructs MUST be skipped safely (log + skip, do not crash, do not execute Python).
- **FR-007**: Waybill MUST parse `setup.cfg` `[options] install_requires` entries.

**Lockfile reading**:

- **FR-008**: Waybill MUST read `uv.lock` and emit one component per locked package.
- **FR-009**: Waybill MUST read `poetry.lock` and emit one component per locked package.
- **FR-010**: Waybill MUST read `pdm.lock` and emit one component per locked package.
- **FR-011**: Waybill MUST read `Pipfile.lock` and emit one component per locked package.

**Correctness posture**:

- **FR-012**: When both a lockfile and a manifest are present in the same directory, the lockfile is authoritative for versions; the manifest MAY contribute scope/optional metadata to the reconciled component. When multiple lockfiles are present in the same directory, waybill MUST read all of them; the m191 reconciler collapses same-PURL entries and preserves every source-file path in evidence (no priority order, no conflict-fail).
- **FR-013**: When a component's version cannot be resolved (unpinned requirements entry with no lockfile), waybill MUST emit the component with `version = "unresolved"` and set the m236 `waybill:unresolved-reason` annotation with a Python-specific reason string (e.g., `"python-requirements-txt-unpinned"`).
- **FR-014**: Waybill MUST NOT fabricate components — every emitted `pkg:pypi/*` MUST trace to at least one manifest / lockfile / installed-dist source in the scanned tree, recorded via the existing `source_file_paths` / evidence mechanism.
- **FR-015**: Waybill MUST NOT execute Python code, even indirectly (no `python setup.py egg_info`, no import, no exec). All parsing is static.
- **FR-016**: Waybill MUST warn-and-skip on parse failures (malformed lockfile, unreadable manifest) without failing the overall scan (matches the #742 error-recovery pattern requested for the cargo v1/v2 issue).

**Reconciliation & deduplication**:

- **FR-017**: When multiple readers surface the same package (e.g., `requirements.txt` and `pyproject.toml` both declare `requests`), the m191 reconciler MUST collapse them into one component, preserving all source-file evidence paths.
- **FR-018**: Sub-projects (nested `pyproject.toml` files) MUST each generate their own "main module" component per m064's mechanism; their declared dependencies MUST be attributed to the sub-project.

### Key Entities

- **Python source tree**: The root path being scanned. May contain zero or more of: `pyproject.toml`, `setup.py`, `setup.cfg`, `requirements*.txt`, and paired lockfiles.
- **Python manifest**: A file declaring dependency intent (`pyproject.toml`, `setup.py`, `setup.cfg`, `requirements*.txt`). Provides constraints, not necessarily resolved versions.
- **Python lockfile**: A file recording resolved versions (`uv.lock`, `poetry.lock`, `pdm.lock`, `Pipfile.lock`). Authoritative for versions when paired with a manifest.
- **Python component**: A `pkg:pypi/<name>@<version>` component, emitted with source-file evidence pointing back to the manifest / lockfile that declared it.
- **Scope tag**: `main` / `optional` / `dependency-group:<name>` metadata attached to a component via the existing m179/m180 `LifecycleScope` mechanism.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning `kusari-sandbox/test-markitdown` (28 MB) emits ≥ 30 `pkg:pypi/*` components (up from 4 today). Verified by an automated fixture-integration test in the sweep-parity suite.
- **SC-002**: Scanning `kusari-sandbox/test-OctoPrint` (30 MB) emits ≥ 30 `pkg:pypi/*` components (up from 3 today).
- **SC-003**: Scanning `kusari-sandbox/test-cpython` (189 MB) emits ≥ 50 `pkg:pypi/*` components (up from 16 today).
- **SC-004**: For each of the three fixtures above, ≥ 90% of emitted `pkg:pypi/*` components have a resolved `version` (not `"unresolved"`), when a lockfile is present in the tree.
- **SC-005**: For each of the three fixtures above, every emitted `pkg:pypi/*` component has at least one `source_file_paths` evidence entry pointing to a manifest or lockfile in the scanned tree (FR-014 verification).
- **SC-006**: No regression on any existing sweep repo — total component counts across all 21 `kusari-sandbox/test-*` fixtures MUST stay within ± 5% of baseline for non-Python repos and MUST monotonically increase for Python-containing repos.
- **SC-007**: Scan wall-clock time for `test-markitdown` MUST NOT increase by more than 500ms (from ~49ms baseline; new manifest parsing is bounded).
- **SC-008**: Scan wall-clock time for `test-cpython` MUST NOT increase by more than 5 seconds (from ~575ms baseline; recursive `requirements*.txt` discovery is the dominant new cost).

## Assumptions

- **The sweep-fixture scans are the ground truth for measuring success**. All three fixtures are pinned in `kusari-sandbox` at their current HEAD; the sweep's `--depth 1` clone posture is preserved.
- **Static parsing is sufficient**. No Python interpreter is invoked at scan time. Dynamic `setup.py` invocations are treated as unresolvable and are skipped safely (Principle I).
- **Existing reader infrastructure is the foundation**. This work extends `waybill-cli/src/scan_fs/package_db/pip/` — it does not create a new top-level module family. Reuses m064 main-module, m179/m180 optional-scope, m191 reconciler, and m236 `waybill:unresolved-reason` verbatim.
- **PURL canonicalization is unchanged**. The `pkg:pypi/*` PURL shape is the purl-spec form (name-normalized per PEP 503). No custom escape schemes.
- **Existing walker safeguards apply**. `--exclude`, `.gitignore`, and the m054/m114 safe-walk symlink/cycle handling all continue to gate directory descent. Recursive `requirements*.txt` discovery uses the same walker; no bespoke traversal.
- **Alt lockfile formats are stable**. `uv.lock` (TOML), `poetry.lock` (TOML), `pdm.lock` (TOML), and `Pipfile.lock` (JSON) formats are treated as their upstream tools currently document them; format drift is a future maintenance concern, not a v1 blocker.
- **No network access**. All parsing is offline (matches the sweep's `--offline` posture).
- **Downstream reconciliation is unchanged**. The m191 reconciler already merges same-PURL components; this milestone adds new *inputs*, not new *merge logic*.

## Out of Scope (v1)

The following are explicitly deferred to future milestones:

- **Constraints files** (`constraints.txt`, PEP 665). Deferred — usually paired with requirements files, but the resolution semantic is compound.
- **Editable installs** (`-e .`, `-e git+...`). Emitted with `unresolved` version + reason string; full resolution deferred.
- **Namespace packages resolved via `[tool.setuptools.packages.find]`**. Detection of sub-project boundaries in monorepos uses m127 workspace-member logic; namespace-package resolution is a follow-up.
- **Non-PyPI index sources** (`--index-url`, `--extra-index-url`). Component name/version is extracted; the index-URL is preserved as evidence but does NOT change the PURL type from `pkg:pypi`.
- **Docker/OCI image Python-tier scan enrichment**. This milestone is source-tree-only. Container-image Python detection (site-packages inside layers) is a separate concern.
- **hatch / bazel-based Python**. Detected by their sibling readers (m106, m103), not this milestone.
