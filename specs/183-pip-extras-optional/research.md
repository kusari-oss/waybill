# Research: pip / poetry / uv optional-dependency classification (m183)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Decisions

### Decision 1 — Derivation-annotation value: `"pip-optional-dependencies"`

**Decision**: Emit `mikebom:optional-derivation = "pip-optional-dependencies"` for all three pip-family classifications (US1 poetry.lock, US2 pyproject.toml `[project.optional-dependencies]`, US3 uv.lock `optional-dependencies` sub-tables).

**Rationale**:
- Mirrors m180/m181's reuse of `"npm-optional-dependencies"` across yarn v1 / yarn Berry / npm / pnpm — all four pkg-managers surface the same underlying npm-registry `optionalDependencies` concept.
- All three pip sources point at the SAME underlying PEP 621 / setuptools extras-gated concept. The specific manifest format is captured by `evidence.source_file_paths`, not by the derivation value.
- Keeps the C122 value-set small and enumerable: `{cargo-optional-true, npm-optional-dependencies, pip-optional-dependencies}` after m183. Three values total.

**Alternatives considered**:
- **`"pip-extras-require"`** (the placeholder currently in `cdx.rs:866` docstring) — misleading: `extras_require` is the setup.py legacy field name, which is OUT OF SCOPE for m183 per spec Assumption. The three m183 sources use `optional-dependencies` (pyproject + uv.lock) or `optional = true` (poetry.lock) — none use `extras_require`.
- **Per-source values** (`poetry-optional`, `pyproject-optional-dependencies`, `uv-optional-dependencies`) — violates the m180/m181 uniform-value convention. Would add three catalog entries to track.

**Follow-up**: The `cdx.rs:866` docstring placeholder (`pip-extras-require`) MUST be updated to `pip-optional-dependencies` during m183 implementation as a documentation fix. Zero behavior change.

---

### Decision 2 — Dev-wins-over-optional precedence (US1 acceptance 4)

**Decision**: When a poetry.lock package has BOTH `groups = ["dev"]` (or `category = "dev"` in v1) AND `optional = true`, the DEV classification wins. `LifecycleScope::Development` is emitted; `mikebom:optional-derivation` MUST NOT appear on that component.

**Rationale**:
- Matches poetry's own semantic — a `dev` + `optional` package requires BOTH `--with dev` AND `--extras foo` to install. The dev-group is the outer gate.
- Simplifies the classifier: dev vs non-dev is the primary axis; optional only refines the non-dev branch.
- Preserves `--include-dev=false` filtering behavior — the dev target is already filtered by `is_non_runtime()`.
- Prevents visual noise: a dev component with a redundant optional-derivation annotation would confuse SBOM consumers looking for "why is this component filtered?"
- Enforces the m180-established "one derivation per component" invariant — a component classified as Dev cannot ALSO carry the optional-derivation annotation.

**Alternatives considered**:
- **Optional-wins-over-dev** — would flip the axis but produce the same downstream filter behavior (both are `is_non_runtime()`). Rejected because it contradicts poetry's own install-gate semantics.
- **Both classifications emitted** — would produce a component with `LifecycleScope::Development` AND `mikebom:optional-derivation` annotation. Rejected because it violates the "one derivation per component" invariant and doesn't map to a single SPDX 2.3 relationship type.

---

### Decision 3 — Lockfile-precedes-manifest (FR-006)

**Decision**: When both `poetry.lock` and `pyproject.toml` (or `uv.lock` and `pyproject.toml`) exist in the same project root, the LOCKFILE's classification takes precedence for any component that appears in both. The manifest's `[project.optional-dependencies]` is NOT consulted for components already present in the lockfile.

**Implementation** (planning-phase): The pip `read_all` dispatcher already invokes the readers in a specific order:
1. `read_poetry_lock` (lockfile — highest priority)
2. `read_uv_lock` (lockfile)
3. `read_pipfile_lock` (lockfile — pre-existing dev/runtime split)
4. `read_requirements_txt` (manifest)
5. `read_dist_info` (installed tier)
6. `build_pip_main_module_entry` (manifest, pyproject.toml)

The main-module extractor (US2 code site) runs LAST. It can consult a HashSet of PURLs already classified by the lockfile readers and skip re-classification for collision cases. This is the same deduplication pattern used by m180's npm/pnpm coordinator at `mikebom-cli/src/scan_fs/mod.rs:1120`.

**Rationale**:
- The lockfile IS the resolved, ground-truth view. The manifest is the INPUT. In the case of a mismatch (e.g., pyproject.toml declares `foo` as optional but poetry.lock resolved it as a hard runtime dep of another package), the lockfile wins.
- Prevents double-emission of the derivation annotation. Every component gets classified exactly once.
- Matches m179's Go-graph ladder-precedence philosophy: authoritative source wins.

**Alternatives considered**:
- **Manifest-precedes-lockfile** — rejected: the manifest is the INPUT (what the user WANTED), not the RESOLVED view. Emitting classifications based on the input while a stricter resolved-view exists would misrepresent the actual dependency graph.
- **Both classifications emitted with different derivations** — rejected: violates the "one derivation per component" invariant. Also complicates C122 parity extraction (which expects one value per component).

---

### Decision 4 — US2 plumbing: downstream classifier pass, NOT signature change to `build_pip_main_module_entry`

**Decision**: `build_pip_main_module_entry` at `pip/mod.rs:399` returns the current `PackageDbEntry` for the main-module UNCHANGED. A NEW helper — `optional_deps_from_pyproject(project_table: &toml::Value) -> HashSet<String>` — returns the set of direct-dep names classified as optional by the manifest. The `read` dispatcher builds this set and applies `LifecycleScope::Optional` to matching child components AFTER the main-module + child components are all resolved.

**Rationale**:
- Minimizes the signature change to `build_pip_main_module_entry` (its callers in `scan_fs/mod.rs` don't need to know about optional-classification).
- The "matching child components" logic already exists in m180's `apply_lifecycle_scope_to_edges` post-pass at `mikebom-cli/src/scan_fs/mod.rs:1261` — reuse it verbatim.
- Preserves the diamond-shape rule: if a child appears in BOTH `[project.dependencies]` AND `[project.optional-dependencies]`, the direct-deps-set (regular) contains it, so Runtime wins.

**Alternatives considered**:
- **Split `depends` into `runtime_depends` + `optional_depends` on `PackageDbEntry`** — heavier data-model change. Would require updating every reader that constructs a `PackageDbEntry` OR every consumer that reads `depends`. Rejected as too invasive for m183's scope.
- **Emit two main-module entries (one for regular deps, one for optional)** — corrupts the main-module component identity. Rejected.
- **Inline the classifier logic into `build_pip_main_module_entry`** — mixes manifest-parsing with graph-resolution concerns. Rejected for separation-of-concerns.

---

### Decision 5 — US3 (uv.lock) sub-table shape

**Decision**: uv.lock's optional-dependencies live at `[[package]].optional-dependencies.<extra>` per uv 0.5+'s schema. The reader walks each `[[package]]` entry's `optional-dependencies` table (if present), iterates the per-extra sub-tables, and accumulates a HashSet of `(parent_purl, child_name)` pairs. Post-pass classifies each matching child component as `LifecycleScope::Optional` unless it also appears in the primary `dependencies = [...]` array (diamond-shape).

**Rationale**:
- Matches uv's own documented schema (verified against uv 0.5.13's `uv.lock`-generation output on real projects).
- Reuses the same post-pass classifier logic as US2 — one classifier pass per pip-family manifest, sharing the same derivation-annotation value.
- The diamond-shape rule works identically because we compare against the same package's `dependencies` array.

**Alternatives considered**:
- **`optional-dependencies = { extra = [...] }` inline table form** — uv's schema DOES support both the sub-table form (`[[package]].optional-dependencies.dev = [...]`) and the inline-table form. The parser MUST handle both. Documented as a parser-implementation detail, not a design decision.
- **Assume all uv-lock optional-deps are Dev** — rejected. uv's `optional-dependencies` are extras-gated per PEP 621; they may or may not be dev-scoped. The uniform-Optional classification (per spec Assumption 3) matches the m183 delivery scope.

---

## Bug Discovery: `cargo-optional-feature` in m183 spec

**Observation**: The m183 spec.md originally used `"cargo-optional-feature"` as a reference to m179's derivation-annotation value. Actual m179 code uses `"cargo-optional-true"` at `cargo.rs:1088`. Spec.md corrected to match.

**Impact**: Documentation-only. No implementation change needed.

## Bug Discovery: `pip-extras-require` placeholder in C122 docstring

**Observation**: The C122 catalog docstring at `mikebom-cli/src/parity/extractors/cdx.rs:866` lists `pip-extras-require` as one of the expected value-set entries. This is a placeholder set during m179 authoring (before m183 was scoped). The value is misleading because `extras_require` is the setup.py legacy field name, which m183 explicitly excludes.

**Impact**: Documentation-only. m183 implementation MUST update the docstring to `pip-optional-dependencies` (matches the m183 delivery per Decision 1). Zero behavior change on any code path.
