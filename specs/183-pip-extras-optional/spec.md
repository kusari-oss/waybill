# Feature Specification: pip / poetry / uv optional-dependency classification

**Feature Branch**: `183-pip-extras-optional`
**Created**: 2026-07-10
**Status**: Draft
**Input**: User description: "m183 — extends the m179 unified optional-dependency classification to the Python ecosystem. Three pip-family readers plumb the missing `LifecycleScope::Optional` classification: (a) `poetry.lock` currently ignores the per-package `optional = true` field and emits `optional = true` non-dev packages as `LifecycleScope::Runtime` (silent misclassification — those are extras-gated, not runtime); (b) `pyproject.toml [project.optional-dependencies].<extra>` deps are currently flattened into the main-module's `depends` list with no per-edge scope, so extras-only deps emit as regular Runtime edges; (c) `uv.lock` optional-dependencies groups are not currently distinguished from regular dependencies. All three surface the same underlying PEP 621 / setuptools `extras_require` semantic and share the `mikebom:optional-derivation = \"pip-optional-dependencies\"` C122 parity annotation."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — `poetry.lock` `optional = true` maps to `LifecycleScope::Optional` (Priority: P1)

Poetry ships with two orthogonal classification dimensions in `poetry.lock`: `category` / `groups` (dev vs runtime) AND `optional` (extras-gated or not). Today mikebom's poetry reader at `pip/poetry.rs:67` only consults the dev dimension — `poetry_is_dev` returns Some(true)/Some(false)/None — and maps that to `LifecycleScope::Development` or `Runtime`. The `optional = true/false` field is READ into the parser (line 178+ regression fixture) but never consulted for classification.

The bug: a poetry package declared as `[tool.poetry.dependencies].foo = { version = "1", optional = true }` is only installed via `poetry install --extras foo-feature`. Semantically it maps to `LifecycleScope::Optional` per m179. Instead mikebom currently emits it as Runtime (because `poetry_is_dev` returns `Some(false)` for a package whose `groups = ["main"]`, regardless of the `optional` flag), producing a false-positive Runtime edge that misleads downstream SBOM consumers running pico-style filter analyses.

**Why this priority**: Same pico filter-parity gap m179/m180/m181 closed for Go, Cargo, npm, pnpm, yarn — extended to the Python-ecosystem lockfile with the biggest installed base. Poetry is used by tens of thousands of Python projects; the misclassification is silently emitted on every poetry.lock scan today.

**Independent Test**: Scan a poetry project whose `poetry.lock` has at least one `[[package]]` entry with `optional = true` and `category = "main"` (or `groups = ["main"]` in the v2+ dialect). Verify (a) the target component gets `LifecycleScope::Optional` + the `mikebom:optional-derivation` annotation, (b) CDX emits `scope: "excluded"` on it, (c) SPDX 2.3 emits `<child> OPTIONAL_DEPENDENCY_OF <parent>` under `--spdx2-relationship-compat=full`.

**Acceptance Scenarios**:

1. **Given** a poetry.lock with `[[package]]` entry `foo` where `optional = true` and `category = "main"` (or `groups = ["main"]`), **When** mikebom emits SPDX 2.3 under `--spdx2-relationship-compat=full`, **Then** `foo` MUST appear as source-side of an `OPTIONAL_DEPENDENCY_OF` edge — NOT as a plain `DEPENDS_ON` target and NOT as `DEV_DEPENDENCY_OF`.
2. **Given** the same scan, **When** mikebom emits CDX 1.6, **Then** `foo` MUST carry `scope: "excluded"` + `mikebom:optional-derivation = "pip-optional-dependencies"`.
3. **Given** a poetry.lock with `[[package]]` entry `bar` where `optional = false` and `category = "dev"` (or `groups = ["dev"]`), **When** mikebom emits SPDX 2.3, **Then** `bar` MUST continue to emit as `DEV_DEPENDENCY_OF` — the fix does NOT change dev-classification.
4. **Given** a poetry.lock with `[[package]]` entry `baz` where `optional = true` AND `category = "dev"`, **When** mikebom classifies, **Then** the dev classification wins — `baz` emits as `DEV_DEPENDENCY_OF`, NOT `OPTIONAL_DEPENDENCY_OF`, and the derivation annotation MUST NOT appear on it (dev-scope is more specific than extras-scope; `--include-dev=false` still filters it out).

---

### User Story 2 — `pyproject.toml [project.optional-dependencies].<extra>` deps classify as Optional (Priority: P1)

PEP 621's `[project.optional-dependencies]` table declares extras-gated deps in the modern non-poetry pyproject shape. Today mikebom's main-module reader at `pip/mod.rs:474` reads both `[project.dependencies]` AND `[project.optional-dependencies].*` into a flat `depends: Vec<String>` list on the synthetic main-module component. When the graph resolver later builds edges from that list, EVERY dep — regular AND extras-gated — gets emitted as a Runtime edge from the main module. Extras-only deps thus appear as false-positive Runtime edges.

This is the same shape bug as US1 but for the pyproject-only case (project uses PEP 621 without poetry / uv). The fix: split the `depends` list so extras-only deps get flagged for downstream classification, and the emitted target components carry `LifecycleScope::Optional` + the derivation annotation.

**Why this priority**: PEP 621's `[project.optional-dependencies]` is the modern standard for non-poetry / non-uv projects (stdlib-only tooling projects, setuptools-based projects, PyPA-recommended layouts). Same filter-parity gap as US1 for the growing set of PEP-621-native projects.

**Independent Test**: Scan a project with a `pyproject.toml` declaring `[project.optional-dependencies].dev = ["pytest"]` and `[project.dependencies] = ["requests"]`. Verify (a) the `pytest` component gets `LifecycleScope::Optional` (or Dev, per Assumption 3 below), (b) the `requests` component stays `LifecycleScope::Runtime`, (c) CDX and SPDX 2.3 filter-parity holds.

**Acceptance Scenarios**:

1. **Given** a pyproject.toml with `[project.optional-dependencies].dev = ["pytest"]` and NO poetry / uv lockfile in the tree, **When** mikebom emits SPDX 2.3 under Full mode, **Then** `pytest` MUST appear as source-side of `OPTIONAL_DEPENDENCY_OF` (or `DEV_DEPENDENCY_OF` per Assumption 3 heuristic), NOT as regular `DEPENDS_ON`.
2. **Given** the same project, **When** mikebom emits CDX 1.6, **Then** `pytest` MUST carry `scope: "excluded"` + `mikebom:optional-derivation = "pip-optional-dependencies"`.
3. **Given** a pyproject.toml with a dep appearing in BOTH `[project.dependencies]` AND `[project.optional-dependencies].<extra>` (diamond-shape), **When** mikebom classifies, **Then** Runtime wins — the dep stays Runtime, NO derivation annotation is emitted (matches the m180 US2 + m181 US1 diamond-shape convention).

---

### User Story 3 — `uv.lock` optional-dependency groups classify as Optional (Priority: P2)

uv's lockfile format (uv 0.5+) stores per-package optional-dependencies as `[[package]].optional-dependencies.<extra>` sub-tables (similar shape to Cargo.toml's `[dependencies].foo = { optional = true }` but per-package rather than per-workspace). mikebom's `uv_lock.rs` today reads only the primary `dependencies = [...]` array (line 65+) and ignores the optional-dependencies sub-tables entirely. Extras-only children thus don't appear in the graph at all, OR they appear but without the Optional classification, depending on whether another package's `dependencies` array pulls them in.

**Why this priority**: uv is the fastest-growing Python package manager (Astral's uv shipped 2024, now widely adopted for CI). Deferring uv to P2 gains implementation simplicity — US1 (poetry) covers the biggest installed base, and uv can land as an additive slice once the shared classifier is established in US1/US2.

**Independent Test**: Scan a uv-managed project whose `uv.lock` has a `[[package]].optional-dependencies.dev = [{ name = "pytest" }]` sub-table. Verify pytest emits with `LifecycleScope::Optional` + the derivation annotation + CDX `scope: "excluded"`.

**Acceptance Scenarios**:

1. **Given** a uv.lock with `[[package]] name = "my-app"` and `[[package.optional-dependencies]].dev = [{ name = "pytest" }]`, **When** mikebom scans, **Then** `pytest` MUST be classified as `LifecycleScope::Optional` + carry the derivation annotation.
2. **Given** the same scan, **When** mikebom emits CDX 1.6, **Then** `pytest` MUST show `scope: "excluded"`.
3. **Given** uv.lock diamond-shape (`pytest` appears in BOTH `dependencies = [...]` AND `optional-dependencies.dev = [...]` of the same package), **When** mikebom classifies, **Then** Runtime wins.

---

### Edge Cases

- **poetry.lock v1 with `category = "main"` + `optional = true`** (Assumption 2): the OPTIONAL classification wins over the implicit main/runtime; the emitted target gets `LifecycleScope::Optional` (not Runtime). Matches poetry's own semantic where `--extras` gates installation regardless of category.
- **poetry.lock v2+ with `groups = ["main", "some-optional-feature"]`**: the presence of `optional = true` at the package level is authoritative; `groups` naming does NOT downgrade / upgrade the classification.
- **poetry.lock package appearing in the `groups = ["dev"]` set AND flagged `optional = true`** (US1 acceptance 4): dev wins. `LifecycleScope::Development` is emitted; `mikebom:optional-derivation` MUST NOT appear on that component. Rationale: `--include-dev=false` already filters the dev target; a redundant Optional annotation would be visual noise and violate the "one derivation per component" invariant m180 established.
- **pyproject.toml `[project.optional-dependencies]` where the same dep name appears in multiple `<extra>` groups** (`dev = ["pytest"]` AND `test = ["pytest"]`): mikebom classifies the target once, as `LifecycleScope::Optional`. The specific extra-name is not preserved in the emitted SBOM (a follow-up milestone can attach `mikebom:extras-groups: ["dev", "test"]` if operator demand arises).
- **`pyproject.toml` present alongside `poetry.lock` in the same project root**: US1 (lockfile) takes precedence over US2 (manifest) for classification of components that appear in both. Lockfile is the resolved, ground-truth view. Prevents double-emission of the derivation annotation.
- **`pyproject.toml` present alongside `uv.lock`**: US3 (lockfile) takes precedence over US2 (manifest). Same rationale.
- **Editable / workspace-member packages** (poetry `[tool.poetry.dev-dependencies].foo = { path = "../foo" }`; uv `source = { editable = "..." }`): editable-target classification is unchanged from pre-m183 behavior; the `optional`/extras flag applies only to non-workspace-member packages. Workspace-member classification is out of m183 scope (deferred, per Assumption 5).
- **`--spdx2-relationship-compat=basic`**: all typed dep-scope edges collapse to natural-direction `DEPENDS_ON` per m228. Same as m179/m180/m181.
- **`--include-dev=false`**: `LifecycleScope::Optional` components are filtered via `is_non_runtime()`, same as m180/m181.
- **Legacy `setup.py` with `extras_require = {...}`**: OUT OF SCOPE for m183. mikebom does not currently parse setup.py (it's a Python-executable script — parsing it would need a Python interpreter or an AST-safe parser; both are heavier than mikebom's constitution allows). Documented as a Known Limitation.
- **`requirements.txt`**: has no first-class optional-deps syntax — extras appear only as `pkg[extra]` reference syntax on parent requirement lines. Existing behavior preserved; m183 does NOT retrofit requirements.txt classification.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `poetry.lock` reader (`pip/poetry.rs`) MUST consult the per-package `optional = true/false` field alongside the existing dev-classification. When `optional = true` AND the package is NOT dev-classified, the emitted component MUST get `LifecycleScope::Optional` + `mikebom:optional-derivation = "pip-optional-dependencies"`. When `optional = true` AND the package IS dev-classified, the dev classification wins (US1 acceptance 4).
- **FR-002**: The pyproject.toml main-module extractor (`pip/mod.rs::build_pip_main_module_entry` at line ~399) MUST distinguish `[project.dependencies]` from `[project.optional-dependencies]` sub-tables when building the `depends` list. The specific plumbing — split into two lists, per-edge annotation, or downstream classifier pass — is a planning-phase decision, but the emitted graph MUST result in target components appearing under the `[project.optional-dependencies].<extra>` sub-tables getting `LifecycleScope::Optional` classification unless they also appear in `[project.dependencies]` (diamond-shape rule per US2 acceptance 3).
- **FR-003**: The `uv.lock` reader (`pip/uv_lock.rs`) MUST parse per-package `optional-dependencies.<extra>` sub-tables and classify the named target components as `LifecycleScope::Optional` (unless they also appear in the primary `dependencies = [...]` array — diamond-shape rule, Runtime wins).
- **FR-004**: All three sources (poetry.lock, pyproject.toml, uv.lock) MUST emit the SAME derivation annotation value: `mikebom:optional-derivation = "pip-optional-dependencies"`. This single value covers all pip-family manifests, mirroring m181's reuse of `"npm-optional-dependencies"` across yarn v1 / yarn Berry / npm / pnpm.
- **FR-005**: When a component appears in BOTH `[project.dependencies]` AND `[project.optional-dependencies].<extra>` of the same pyproject.toml, Runtime wins — no derivation annotation is emitted, `LifecycleScope` stays Runtime. Same rule as m180/m181 diamond-shape.
- **FR-006**: Lockfile classification (US1 poetry.lock, US3 uv.lock) MUST take precedence over manifest classification (US2 pyproject.toml) when both are present in the same project root. The lockfile is the resolved, ground-truth view; the manifest is the input.
- **FR-007**: Under `--spdx2-relationship-compat=basic`, all new `OPTIONAL_DEPENDENCY_OF` emissions collapse to natural-direction `DEPENDS_ON` per m228's contract.
- **FR-008**: The `--include-dev=false` filter MUST filter out `LifecycleScope::Optional` components alongside `Dev/Build/Test` via `is_non_runtime()`.
- **FR-009**: The CDX 1.6 emission MUST show `scope: "excluded"` for every m183-classified `LifecycleScope::Optional` component — auto-inherited via the m179 `is_non_runtime()` extension.
- **FR-010**: The SPDX 3.0.1 emission MUST NOT include a native `lifecycleScope: optional` value on any relationship (SPDX 3.0.1 has no such enum value per m179 FR-017); the annotation IS the SPDX 3 classification carrier.
- **FR-011**: For scans that do NOT exercise the new pip signal (all non-pip fixtures + pip fixtures with zero optional-declared deps), the emitted CDX 1.6, SPDX 2.3, and SPDX 3.0.1 documents MUST be byte-identical to the pre-m183 baseline (regression guard, same shape as m180 FR-012 / m181 FR-011).
- **FR-012**: The existing pip regression tests MUST continue to pass byte-identically — the m183 change is additive except for the poetry.lock classification-fix path (US1), which will show additive changes on any regression fixture that exercises `optional = true` packages.
- **FR-013**: The poetry.rs classification change MUST NOT introduce a new Cargo dependency — `toml` and the existing `poetry_is_dev` helper cover the parsing needs.

### Key Entities

- **Component (`ResolvedComponent`)**: Reuses m179's `LifecycleScope::Optional` variant and the `mikebom:optional-derivation` annotation. Zero new types.
- **`mikebom:optional-derivation` annotation**: New value `"pip-optional-dependencies"` — the third derivation-source flavor after m179's `"cargo-optional-true"` and m180/m181's `"npm-optional-dependencies"`. Registered in the m179 C122 parity extractor's value-set.
- **`poetry.lock` `optional` field** (per-package boolean): the ground-truth flag consulted by US1.
- **`pyproject.toml [project.optional-dependencies].<extra>` sub-tables**: PEP 621 extras-gated deps consulted by US2.
- **`uv.lock` `[[package]].optional-dependencies.<extra>` sub-tables**: uv-format extras consulted by US3.
- **Shared classifier logic**: the three readers converge on the same `is_non_runtime()`-compatible LifecycleScope::Optional target — no per-source enum extension, matches m180/m181's shared-value convention.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of a poetry.lock fixture with N `optional = true` (non-dev) packages MUST have the SET of pip-emitted PURLs marked `scope: "excluded"` in CDX 1.6 equal to the SET of PURLs that appear as source-side of any typed dep-scope relationship (`OPTIONAL_DEPENDENCY_OF` + existing `DEV_DEPENDENCY_OF` if the package is dev-classified) in SPDX 2.3. Same pico filter-parity gate as m179/m180/m181 SC-001, extended to pip.
- **SC-002**: Same set-equality gate for pyproject.toml fixtures with `[project.optional-dependencies]` sub-tables.
- **SC-003**: Same set-equality gate for uv.lock fixtures with `optional-dependencies` sub-tables.
- **SC-004**: Zero mikebom regression fixture SPDX 2.3 golden files experience NET-DECREMENT in `*_DEPENDENCY_OF` edge counts pre-vs-post m183. Net-INCREMENT is expected on the poetry.lock fixture that exercises the m183 fix (US1 — a fixture entry with `optional = true` will move from `DEPENDS_ON` to `OPTIONAL_DEPENDENCY_OF`).
- **SC-005**: Zero drift in any mikebom CDX 1.6 golden file that does not exercise the new signal (regression guard on non-pip fixtures — same shape as m180 SC-003 / m181 SC-004).
- **SC-006**: Zero drift in any mikebom SPDX 3.0.1 golden file (m183 does not touch SPDX 3 emission per FR-010).
- **SC-007**: Under `--spdx2-relationship-compat=basic`, pip goldens show zero new `OPTIONAL_DEPENDENCY_OF` edges — every optional-classified edge is natural-direction `DEPENDS_ON` (m228 escape hatch preserved).
- **SC-008**: Existing pip regression tests (poetry.rs unit tests + pip/mod.rs main-module extraction + uv_lock.rs unit tests) MUST continue to pass, allowing for additive changes on any test that exercises `optional = true` packages (specifically the poetry.rs regression fixture around line 178+).
- **SC-009**: The `mikebom:optional-derivation` annotation with value `"pip-optional-dependencies"` MUST appear byte-identically in CDX 1.6, SPDX 2.3, and SPDX 3.0.1 output for every pip fixture that exercises the m183 classification. C122 parity extractor (registered in m179) validates this automatically via `SymmetricEqual` polarity.

## Assumptions

- The three pip-family sources (poetry.lock, pyproject.toml, uv.lock) share the same underlying setuptools / PEP 621 `extras_require` semantic. mikebom uses ONE derivation-annotation value across all three — `"pip-optional-dependencies"` — mirroring m180/m181's use of `"npm-optional-dependencies"` across yarn / npm / pnpm.
- Poetry's `optional = true` field IS the canonical source-of-truth for extras-gated packages in poetry.lock. The `category` / `groups` field carries orthogonal information (dev vs main) and does NOT downgrade the optional classification unless the group is `dev` (US1 acceptance 4). Documented via the poetry lockfile spec.
- For US2 (pyproject.toml), if the project ALSO has `[project.optional-dependencies].dev = [...]` where the `<extra>` name is `dev`, `docs`, `test`, `lint`, etc., mikebom classifies the target as `LifecycleScope::Optional` — NOT `LifecycleScope::Development` — because pyproject.toml's `optional-dependencies` are semantically extras-gated regardless of naming. A follow-up milestone MAY add a heuristic that maps `dev`/`test`/`docs`/`lint` extras-names to more-specific `LifecycleScope::Development` / `Test` / `Build` classifications, but m183's initial delivery is uniform Optional-classification for simplicity. The `--include-dev=false` filter still catches these via `is_non_runtime()`.
- The m183 scope covers the ROOT project's pyproject.toml + poetry.lock + uv.lock. Workspace-member manifests (poetry's `[tool.poetry.dev-dependencies].foo = { path = "../foo" }`, uv's `source = { editable = "apps/web" }`) are out of scope. Workspace-member cross-reference is deferred to a follow-up milestone (analogous to m181's yarn-workspace deferral).
- `setup.py`'s `extras_require = {...}` is OUT OF SCOPE. mikebom does not currently parse setup.py (it's a Python-executable file — parsing it hermetically requires either a Python interpreter or a proper AST parser, both beyond m183's zero-new-dep target). Documented as a Known Limitation. Users on setup.py-only projects retain the pre-m183 behavior (no Optional classification).
- `requirements.txt` is OUT OF SCOPE. requirements.txt has no first-class optional-deps syntax — extras appear only as `pkg[extra]` on parent requirement lines, which is unambiguous input-format, not classification data. Preserved as pre-m183 behavior.
- Golden fixture regeneration (`MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`) will show additive-only changes on the specific pip regression fixture that exercises `optional = true` packages (currently poetry.rs at line ~178). All other fixtures (non-pip + pip without optional flags) will show zero drift per SC-005.
- The lockfile-precedence rule (FR-006) prevents double-emission of the derivation annotation when both `poetry.lock` and `pyproject.toml` are present. The lockfile IS the resolved view — the manifest is the input. No cross-source annotation collision is possible under this rule.

## Constitution Alignment

**Principle V** (v1.4.0): Direct continuation of the m179+m180+m181 KEEP-BOTH polarity. Native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` is the primary signal (elevated from the current `DEPENDS_ON`); `mikebom:optional-derivation = "pip-optional-dependencies"` is the finer-grained supplement carrying WHICH pip-family construct produced the classification. Zero new `mikebom:*` invention — flows through the C122 catalog row m179 registered. No new Principle V audit surface.

**Principle IX** (Accuracy): The pico filter-parity gap m179 closed for Go + Cargo, m180 for npm + pnpm, m181 for yarn v1 + Berry, now closes for pip / poetry / uv — same measurable accuracy gain (SC-001, SC-002, SC-003). Additionally corrects the US1 classification bug where `optional = true` poetry packages were being emitted as Development (misleading pico consumers today).

**Principle X** (Transparency): SBOM consumers can distinguish "this component was classified optional by a pip-family manifest" via the shared `mikebom:optional-derivation = "pip-optional-dependencies"` value + `evidence.source_file_paths` field pointing at the specific `poetry.lock` / `pyproject.toml` / `uv.lock` variant.

## Deferred to Future Milestones

- **`setup.py extras_require`**: parsing legacy setup.py requires either a Python interpreter shell-out or an AST parser; both violate m183's zero-new-dep target.
- **Workspace-member pyproject.toml / poetry.lock cross-reference**: analogous to m181's yarn-workspace deferral.
- **Per-extra classification** (`mikebom:extras-groups: ["dev", "test"]`): if operator demand arises, the extra-name provenance can be preserved as a supplementary annotation. m183 delivers the binary Optional-vs-not classification only.
- **`dev` / `test` / `docs` extras-name heuristic**: mapping `[project.optional-dependencies].dev = [...]` to `LifecycleScope::Development` instead of `Optional`. Uniform Optional-classification is m183's initial delivery for simplicity; a heuristic-based downgrade is a natural follow-up.
- **requirements.txt classification retrofit**: no first-class syntax exists — deferred until a broader "requirements.txt profiles" workstream (if any).
