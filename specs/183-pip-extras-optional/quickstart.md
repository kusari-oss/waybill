# Quickstart: pip / poetry / uv optional-dependency classification (m183)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Operator flow

### Scenario 1 — poetry-managed project with extras-gated deps

Before m183, mikebom scanning a poetry-managed project with a `[tool.poetry.dependencies].foo = { version = "1", optional = true }` declaration would emit `foo` as a plain Runtime dependency in the SBOM. Downstream filter analyses (pico, other tools) had no way to distinguish `foo` (extras-gated) from `requests` (always installed).

After m183:

```bash
mikebom sbom scan \
    --path ./my-poetry-project \
    --format cyclonedx-json,spdx-2.3-json
```

The emitted SBOMs classify `foo` as:

- **CDX 1.6**: `components[].scope = "excluded"` + `properties[]` includes `mikebom:optional-derivation = "pip-optional-dependencies"`
- **SPDX 2.3** (under `--spdx2-relationship-compat=full`, the default): `foo OPTIONAL_DEPENDENCY_OF <root>` instead of the pre-m183 `<root> DEPENDS_ON foo`
- **SPDX 3.0.1**: annotation-only classification (no native `OPTIONAL_DEPENDENCY_OF` in SPDX 3)

Downstream tools running pico-style filter analyses can now consume the `scope: "excluded"` (CDX) or `OPTIONAL_DEPENDENCY_OF` (SPDX 2.3) signal to exclude extras-gated deps from vulnerability analysis / build reproducibility checks / license aggregation.

### Scenario 2 — modern PEP 621 project (no lockfile)

Some projects use PEP 621's `[project.optional-dependencies]` in pyproject.toml without a lockfile (setuptools-based, PyPA-recommended layouts). Before m183, all deps — regular AND extras-gated — appeared as Runtime edges from the main-module component.

After m183:

```toml
# pyproject.toml
[project]
name = "my-app"
version = "1.0"
dependencies = ["requests>=2.0"]

[project.optional-dependencies]
dev = ["pytest>=7.0"]
docs = ["sphinx"]
```

mikebom emits `requests` as `LifecycleScope::Runtime`; `pytest` and `sphinx` as `LifecycleScope::Optional` with the derivation annotation.

### Scenario 3 — uv-managed project

uv's lockfile (uv 0.5+) declares optional-dependencies as `[[package.optional-dependencies]]` sub-tables. Before m183, mikebom ignored the sub-tables entirely.

After m183:

```toml
# uv.lock (excerpt)
[[package]]
name = "my-app"
version = "0.1.0"
source = { virtual = "." }
dependencies = [{ name = "httpx" }]

[[package.optional-dependencies]]
test = [{ name = "pytest" }, { name = "pytest-asyncio" }]
```

mikebom classifies `pytest` and `pytest-asyncio` as `LifecycleScope::Optional` (unless they also appear in the primary `dependencies` array — diamond-shape, Runtime wins).

## Filter parity in action

The core pico-style analysis a downstream SBOM consumer can now run:

```bash
# Before m183: `pytest` shows up as vulnerable-in-scope
mikebom sbom scan --path ./my-poetry-project | \
    jq '.components[] | select(.scope != "excluded") | .name'
# → requests, pytest, black, sphinx  ← FALSE POSITIVES

# After m183: extras-gated deps are correctly filtered
mikebom sbom scan --path ./my-poetry-project | \
    jq '.components[] | select(.scope != "excluded") | .name'
# → requests  ← ACCURATE
```

## Precedence rules (operator-visible)

- **Lockfile > manifest**: if both `poetry.lock` and `pyproject.toml` are present, the lockfile classification wins for any component in both.
- **Runtime > Optional (diamond-shape)**: if a package is declared in BOTH `[project.dependencies]` AND `[project.optional-dependencies].<extra>`, Runtime wins (no derivation annotation emitted).
- **Dev > Optional**: if a poetry package has BOTH `groups = ["dev"]` AND `optional = true`, Dev wins (already filtered by `--include-dev=false`).

## Developer flow — verifying an m183 classification

To verify a specific component's classification in an emitted SBOM:

```bash
# CDX 1.6
jq '.components[] | select(.name == "pytest") | {scope, properties}' \
    mikebom.cdx.json

# Expected output (m183):
# {
#   "scope": "excluded",
#   "properties": [
#     { "name": "mikebom:optional-derivation", "value": "pip-optional-dependencies" },
#     ...
#   ]
# }

# SPDX 2.3
jq '.relationships[] | select(.spdxElementId == "SPDXRef-pytest-*" or .relatedSpdxElement == "SPDXRef-pytest-*") | .relationshipType' \
    mikebom.spdx.json

# Expected output (m183 under --spdx2-relationship-compat=full):
# "OPTIONAL_DEPENDENCY_OF"
```

## Failure modes

There are none new in m183 — the classifier is purely additive on the read path. Existing failure modes (malformed pyproject.toml, missing poetry.lock, etc.) surface via `tracing::warn!` and skip-and-continue, unchanged from pre-m183.

## When NOT to expect the classification

- **setup.py-only projects**: mikebom does NOT parse setup.py's `extras_require` (out of scope per spec Assumption 5). No classification emitted.
- **requirements.txt-only projects**: no first-class optional-deps syntax exists. No classification emitted.
- **Workspace-member manifests** (poetry `[tool.poetry.dev-dependencies].foo = { path = "../foo" }`, uv `source = { editable = "..." }`): m183 scope is the ROOT project's pyproject.toml + poetry.lock + uv.lock only. Workspace-member cross-reference is DEFERRED.
- **Basic-mode SPDX 2.3** (`--spdx2-relationship-compat=basic`): all typed dep-scope edges collapse to `DEPENDS_ON` per m228. The classifier still runs; only the emission form differs.

## Cross-references

- Spec: [spec.md](./spec.md)
- Plan: [plan.md](./plan.md)
- Classifier decision matrix: [contracts/classifier-decision-matrix.md](./contracts/classifier-decision-matrix.md)
- Derivation value set: [contracts/derivation-value-set.md](./contracts/derivation-value-set.md)
- Research decisions: [research.md](./research.md)
