# Contract: Classifier Decision Matrix (m183)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Data model**: [../data-model.md](../data-model.md)

## Scope

Canonical per-user-story classification tables. This is the single-source-of-truth for what each pip-family reader emits for every combination of input fields. Tasks / implementers MUST reference these tables when writing unit tests.

## US1 — `poetry.lock` `[[package]]` entries

Extends `pip/poetry.rs::read_poetry_lock` at line ~67. The classifier consults BOTH the existing `poetry_is_dev(tbl)` helper AND a new `poetry_is_optional(tbl)` helper.

### Full input × output table

| `groups`/`category` | `optional = true/false` | Emitted `lifecycle_scope` | Emitted `mikebom:optional-derivation` | Pre-m183 behavior | Change? |
|---|---|---|---|---|---|
| `dev` (v1 `category = "dev"` or v2 `groups = ["dev"]`) | `optional = true` | `Development` | (absent) | `Development` (absent) | NONE |
| `dev` | `optional = false` | `Development` | (absent) | `Development` (absent) | NONE |
| `dev` | absent | `Development` | (absent) | `Development` (absent) | NONE |
| `main` (v1 `category = "main"` or v2 `groups = ["main"]`) | **`optional = true`** | **`Optional`** ✱ | **`"pip-optional-dependencies"`** ✱ | `Runtime` (absent) | ✱ CHANGED |
| `main` | `optional = false` | `Runtime` | (absent) | `Runtime` (absent) | NONE |
| `main` | absent | `Runtime` | (absent) | `Runtime` (absent) | NONE |
| absent / unrecognized | **`optional = true`** | **`Optional`** ✱ | **`"pip-optional-dependencies"`** ✱ | `None` (absent) | ✱ CHANGED |
| absent / unrecognized | `optional = false` | `None` | (absent) | `None` (absent) | NONE |
| absent / unrecognized | absent | `None` | (absent) | `None` (absent) | NONE |

**Legend**:
- ✱ = new m183 behavior. Two rows change.
- All other rows preserve pre-m183 byte-identity per FR-011 / SC-005.
- Dev-wins-over-optional (Decision 2) is enforced by the classifier: the `dev` branch never emits the derivation annotation.

## US2 — `pyproject.toml [project.optional-dependencies]`

Extends the `read` dispatcher's main-module-extraction step at `pip/mod.rs:399+` via a new helper `optional_deps_from_pyproject`.

### Input parsing

For a project root's `pyproject.toml`:

```toml
[project]
dependencies = ["requests>=2.0", "urllib3"]

[project.optional-dependencies]
dev = ["pytest>=7.0", "black"]
test = ["pytest", "pytest-cov"]
docs = ["sphinx"]
```

The helper returns:

- `regular_direct_deps: {"requests", "urllib3"}` — from `[project.dependencies]`
- `optional_direct_deps: {"pytest", "black", "pytest-cov", "sphinx"}` — union of all `[project.optional-dependencies].<extra>` arrays, MINUS any name that also appears in `regular_direct_deps` (diamond-shape: `pytest` here appears in BOTH `[project.dependencies]` and `[project.optional-dependencies].test` — Runtime wins, so `pytest` is REMOVED from `optional_direct_deps`).

Wait — the example above is inconsistent. Let me redo:

If `[project.dependencies] = ["requests", "urllib3"]` and `[project.optional-dependencies].test = ["pytest", "pytest-cov"]`, then:
- `regular_direct_deps: {"requests", "urllib3"}`
- `optional_direct_deps: {"pytest", "black", "pytest-cov", "sphinx"}` (nothing to remove)

If additionally `pytest` were in `[project.dependencies]` too, the diamond-shape rule removes `pytest` from `optional_direct_deps`.

### Classifier decision × input

| Package appears in | Classifier decision | Notes |
|---|---|---|
| Only `[project.dependencies]` | `LifecycleScope::Runtime` (unchanged from pre-m183) | No annotation |
| Only `[project.optional-dependencies].<any extra>` | **`LifecycleScope::Optional`** ✱ + `"pip-optional-dependencies"` ✱ | New m183 behavior |
| BOTH (diamond-shape) | `LifecycleScope::Runtime` (Runtime wins per FR-005) | No annotation |
| Neither | Classification not applicable (not a direct dep) | Existing behavior |

### Precedence with lockfile (FR-006 / Decision 3)

The US2 post-pass check `entry.lifecycle_scope.is_none()` skips any entry ALREADY classified by a lockfile reader:

| Lockfile status | Manifest status | Final classification |
|---|---|---|
| Lockfile classified (Runtime/Dev/Optional) | Manifest lists as optional | Lockfile wins (post-pass no-op) |
| Lockfile has no entry | Manifest lists as optional | Optional (post-pass applies) |
| Lockfile has no entry | Manifest lists as regular | None (post-pass no-op) |

## US3 — `uv.lock [[package]].optional-dependencies.<extra>`

Extends `pip/uv_lock.rs::read_uv_lock`. The parser walks each `[[package]]` entry's `optional-dependencies` sub-table (if present) and accumulates a HashSet of `(parent_purl, optional_child_name)` pairs.

### Sub-table shape (verified against uv 0.5+)

```toml
[[package]]
name = "my-app"
version = "0.1.0"
source = { virtual = "." }
dependencies = [
    { name = "requests" },
]

[[package.optional-dependencies]]
dev = [
    { name = "pytest" },
    { name = "black" },
]
test = [
    { name = "pytest-cov" },
]
```

Note: uv.lock uses `[[package.optional-dependencies]]` (double-bracket, singular sub-table with per-extra keys), NOT `[[package.optional-dependencies.<extra>]]` (per-extra sub-array). The parser MUST match uv's schema exactly.

### Diamond-shape handling (per-package)

For each `[[package]]` entry:
- `primary_dep_names: HashSet<String>` = names from `dependencies = [...]`
- For each `<extra>` array in `optional-dependencies`:
  - For each `{ name = "..." }` entry: if `!primary_dep_names.contains(name)`, add to `optional_direct_deps`

The check is per-package: a name appearing in package A's `dependencies` AND package B's `optional-dependencies.<extra>` does NOT trigger diamond-shape at the classifier level (they may or may not resolve to the same PURL depending on version ranges — a follow-up milestone can address this if operator demand arises).

## Shared invariants (all three user stories)

1. **Derivation-annotation value**: exactly `"pip-optional-dependencies"` — NEVER emit any other value from an m183 code path.
2. **One-derivation-per-component**: a component classified as Dev CANNOT ALSO carry `mikebom:optional-derivation`. Enforced by Decision 2.
3. **Lockfile-precedence**: US2's manifest classifier NEVER overrides a lockfile classification. Enforced by the `is_none()` check in the post-pass (Decision 3).
4. **C122 parity byte-identity**: the annotation MUST appear byte-identically in CDX 1.6, SPDX 2.3, and SPDX 3.0.1. Enforced by the existing C122 `Directionality::SymmetricEqual` extractor (registered in m179).
5. **`--include-dev=false` filtering**: `LifecycleScope::Optional` targets are filtered via `is_non_runtime()`, same as `Dev`/`Build`/`Test`. Enforced by m179's existing `is_non_runtime()` extension.
6. **`--spdx2-relationship-compat=basic`**: all new `OPTIONAL_DEPENDENCY_OF` emissions collapse to natural-direction `DEPENDS_ON`. Enforced by m228's basic-mode contract.
