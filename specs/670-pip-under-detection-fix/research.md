# Phase 0 Research: Python under-detection fix

**Feature**: 670-pip-under-detection-fix
**Date**: 2026-08-31
**Status**: Complete

Five clarifications were resolved in the `/speckit.clarify` session (see `spec.md ## Clarifications`). This research fills the remaining unknowns needed before Phase 1 design.

## R1: Existing `waybill-cli/src/scan_fs/package_db/pip/` module state

**Decision**: Extend the existing module; do NOT rename or restructure.

**Rationale**: The module already exists with `dist_info.rs` (site-packages reader from m002 baseline) and `mod.rs` (dispatcher). Adding new sibling files preserves the m002-onward per-ecosystem directory convention. Renaming to `pip_manifest/` or `python/` would break the existing dispatcher signature at `scan_fs/mod.rs` and require touching untold call-sites.

**Alternatives considered**:
- New sibling module `python/` — rejected: 200+ milestone precedent uses ecosystem name, not language name (there's `pip/` not `python/`, `npm/` not `javascript/`).
- Rename `pip/` to `python/` — rejected: gratuitous churn; the ecosystem PURL type is `pypi`, and `pip` is the canonical operator-facing tool name.

## R2: Format-shape reference

Format specifications for the artifacts to be read. Fields listed are the ones this milestone touches.

### `pyproject.toml` (PEP 621, PEP 735, Poetry-legacy)

```toml
# PEP 621 canonical shape
[project]
name = "example"
version = "1.0.0"
dependencies = ["requests>=2.28", "click>=8.0"]      # FR-001

[project.optional-dependencies]                        # FR-002
docs = ["sphinx>=7"]
test = ["pytest>=8"]

# PEP 735 dependency-groups
[dependency-groups]                                    # FR-003
lint = ["ruff>=0.5"]

# Poetry-legacy (PEP 621 fallback)
[tool.poetry.dependencies]                             # FR-003a
python = "^3.11"
requests = "^2.28"
click = "^8.0"

[tool.poetry.dev-dependencies]                         # FR-003a (scope=dev)
pytest = "^8.0"

[tool.poetry.group.docs.dependencies]                  # FR-003a (scope=docs)
sphinx = "^7"
```

**Version constraint syntax**: PEP 440 in PEP 621; Poetry-caret (`^1.0`) and Poetry-tilde (`~1.0`) syntax in `[tool.poetry.*]`. Waybill emits the constraint STRING in a `waybill:version-constraint` annotation but the PURL version is `unresolved` when only a constraint is present (per FR-013).

### `uv.lock` (uv format, TOML)

Uv's `uv.lock` is documented at <https://docs.astral.sh/uv/reference/#lockfile>. Key shape:

```toml
version = 1
requires-python = ">=3.11"

[[package]]
name = "requests"
version = "2.31.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "charset-normalizer" },
    { name = "idna" },
    { name = "urllib3" },
]

[[package.wheels]]
url = "https://files.pythonhosted.org/packages/.../requests-2.31.0-py3-none-any.whl"
hash = "sha256:..."
```

**Emission mapping**: each `[[package]]` entry → one `pkg:pypi/{name}@{version}` component. `hash` in `[[package.wheels]]` populates the CDX `hashes[]` array with SHA-256.

### `poetry.lock` (Poetry format, TOML)

Documented at <https://python-poetry.org/docs/repositories/#poetry-lock-files>. Key shape:

```toml
[[package]]
name = "requests"
version = "2.31.0"
description = "Python HTTP for Humans."
category = "main"
optional = false
python-versions = ">=3.7"

[package.dependencies]
charset-normalizer = ">=2,<4"
idna = ">=2.5,<4"
urllib3 = ">=1.21.1,<3"

[metadata.files]
requests = [
    { file = "requests-2.31.0-py3-none-any.whl", hash = "sha256:..." },
    { file = "requests-2.31.0.tar.gz", hash = "sha256:..." },
]
```

**Emission mapping**: `[[package]]` → PURL; `category` = "main"/"dev"/"docs" → `LifecycleScope` per m179/m180 mapping (main → Main, dev → Optional with `dev` scope-name, etc.); `metadata.files[].hash` → CDX `hashes[]`.

### `pdm.lock` (PDM format, TOML)

Documented at <https://pdm-project.org/en/latest/usage/lockfile/>. Shape is very close to poetry.lock:

```toml
[[package]]
name = "requests"
version = "2.31.0"
requires_python = ">=3.7"
summary = "Python HTTP for Humans."
groups = ["default"]

[[package.dependencies]]
name = "charset-normalizer"
specifier = ">=2,<4"

[[package.files]]
url = "https://files.pythonhosted.org/packages/.../requests-2.31.0-py3-none-any.whl"
hash = "sha256:..."
```

**Emission mapping**: `groups` → scope-tag(s) via m179/m180. Multiple groups → one component with one scope; if a package is in both `default` and `test`, `default` wins (main scope).

### `Pipfile.lock` (Pipenv format, JSON)

Documented at <https://github.com/pypa/pipfile>. Shape:

```json
{
    "_meta": { "hash": { "sha256": "..." } },
    "default": {
        "requests": {
            "version": "==2.31.0",
            "hashes": ["sha256:..."],
            "index": "pypi"
        }
    },
    "develop": {
        "pytest": {
            "version": "==8.0.0",
            "hashes": ["sha256:..."],
            "index": "pypi"
        }
    }
}
```

**Emission mapping**: top-level `default` → `LifecycleScope::Main`; `develop` → `LifecycleScope::Optional` with `dev` scope-name. Package `version` field has `==` prefix that must be stripped.

### `requirements*.txt` (PEP 508 line format)

Line-based; parseable via existing `regex` workspace dep. Handled tokens:

- `pkg-name==1.2.3` — pinned; emit `pkg:pypi/pkg-name@1.2.3`
- `pkg-name>=1,<2` — constrained; emit `pkg:pypi/pkg-name@unresolved` + `waybill:version-constraint` annotation
- `pkg-name` (no version) — unpinned; emit `pkg:pypi/pkg-name@unresolved` + `waybill:unresolved-reason = "python-requirements-txt-unpinned"`
- `pkg-name==1.2.3 ; python_version >= '3.10'` — PEP 508 marker; emit + `waybill:pep508-marker` annotation
- `pkg-name @ git+https://.../@rev` — direct-URL; per FR-005b, emit `pkg:pypi/pkg-name@<rev-or-unresolved>` + `waybill:direct-url-source` annotation
- `-r other-requirements.txt` — recurse (bounded depth 10, cycle-detect via visited-set)
- `-e .` / `-e git+...` — editable; emit `pkg:pypi/<name>@unresolved` + `waybill:unresolved-reason = "python-editable-install"`
- `# comment` — skip
- `--index-url <url>` — record as `waybill:index-url` annotation; does not gate PURL type

**Reference regex** (already-in-workspace `regex = "1"` compatible):

```rust
static PEP508_LINE: OnceLock<Regex> = OnceLock::new();
fn pep508_line() -> &'static Regex {
    PEP508_LINE.get_or_init(|| Regex::new(
        r"^(?P<name>[A-Za-z0-9][A-Za-z0-9._-]*)(?P<extras>\[[^\]]+\])?\s*(?P<url>@\s*\S+)?\s*(?P<spec>[<>=!~][^\s;]*(?:,[<>=!~][^\s;]*)*)?\s*(?:;\s*(?P<marker>.+))?$"
    ).unwrap())
}
```

### `setup.py` (static parse, no exec)

Waybill does NOT execute Python code. Static parse looks for the AST-shape pattern:

```python
from setuptools import setup

setup(
    name="octoprint",
    version="1.9.0",
    install_requires=[
        "Flask>=2.0",
        "click>=8.0",
        # ...
    ],
    extras_require={
        "docs": ["sphinx>=7"],
    },
)
```

**Static-parse strategy**: locate the top-level `setup(` call via regex, then walk forward looking for `install_requires=[...]` and `extras_require={...}` **literal** subtrees. Extract string literals from the list; ignore any variable references, function calls, or f-strings (FR-006's dynamic-construct skip).

**Simplifying approach**: don't build a full Python AST. Use a small state machine over the token stream that recognizes only `install_requires` / `extras_require` at the setup-call site, then a bracket-balancing scan to extract the enclosed literal-string list. This gives us the OctoPrint-class of projects (which uses a literal list at the top-level setup() call) without pulling in a Python-AST crate.

**Rejected**: Depending on `rustpython-parser` or similar — adds a large new Cargo dependency, and we don't need a real AST. The FR-006 acceptance scenario 2 (dynamic → skip safely) is exactly what static parsing gives us.

### `setup.cfg` (INI format)

INI-shape via existing... hmm, actually no `configparser`-style INI parser is currently a workspace dep. The existing `toml = "0.8"` handles TOML but not classic INI. Two options:
- **Option 1**: hand-roll a mini INI parser for the `[options]` section only (~30 lines). Setup.cfg's `install_requires` is a multiline scalar under `[options]` — not full INI complexity.
- **Option 2**: skim for `install_requires =` inside `[options]` with regex.

**Decision**: Option 2. Zero new deps, ~15 lines of code. The full INI-spec coverage isn't needed — we only extract `[options] install_requires` (and later possibly `[options.extras_require]`).

## R3: Existing infrastructure reuse map

| Concern | Existing mechanism | Location | Reuse posture |
|---------|-------------------|----------|---------------|
| Main-module emission | m064 pattern (per-manifest main-module component) | `scan_fs/package_db/cargo.rs::build_main_module` (reference) | Copy pattern into `pip/pyproject_toml.rs::build_main_module` |
| Optional/dev-scope tagging | m179/m180 `LifecycleScope::Optional` + `RelationshipType::OptionalDependsOn` | `waybill-common/src/resolution.rs` | Use `LifecycleScope::Optional { scope_name: "dev".into() }` etc. |
| Reconciler (same-PURL dedup) | m191 pass at scan_fs/mod.rs | `waybill-cli/src/resolve/reconciler.rs` | No changes required; already dedups on PURL identity |
| Unresolved-reason annotation | m236 C151 catalog row | `waybill-cli/src/scan_fs/package_db/*` | Add `python-requirements-txt-unpinned`, `python-direct-url-unresolved`, `python-editable-install`, `python-setup-py-dynamic` to the locked reason-string vocabulary |
| Walker skip-patterns | m174 VCS-directory skip + m113 ExclusionSet | `scan_fs/walk.rs` + `scan_fs/package_db/exclude_path.rs` | Add Python-specific default-prune list (FR-004 extended) that plugs into the walker's skip predicate |
| Public-corpus fixture cache | m195 harness | `waybill-cli/tests/corpus_harness_195/` | Reuse verbatim; add markitdown/OctoPrint/cpython entries |

## R4: Ground-truth for SC-001/SC-002/SC-003 thresholds

**Decision**: The thresholds (≥30, ≥30, ≥50) are conservative floors, not the ideal ceilings. Ground-truth verification uses:

1. **uv pip compile** (out-of-band, in fixture-CI setup) run against each fixture's manifest — the resolved dependency list is the "expected" set.
2. **Cross-tool comparison**: run `syft` and `cdxgen` on the same fixtures (already available per the m165/m168 audit tooling); the intersection of what all three tools find is the strong-ground-truth baseline.

**Rationale**: We're deliberately not requiring exact-match with uv's output — that would over-constrain (waybill emits some annotations they don't; they emit some fields we don't). The floor thresholds (≥30/≥30/≥50) are calibrated so that:
- markitdown: full lockfile parse of the ~50-entry `uv.lock` → passing at ≥30 means we hit the majority
- OctoPrint: static setup.py extract of the ~50-entry `install_requires` → passing at ≥30 means the pattern works
- cpython: aggregate of ~10 requirements files each with 5-15 entries → passing at ≥50 means recursion + parsing both fire

**Alternatives considered**:
- Exact-count matching against a hand-verified list → rejected as brittle (fixtures drift when kusari-sandbox HEAD moves).
- Percentage-based thresholds ("waybill must emit ≥ 80% of what syft finds") → rejected as circular (syft has its own bugs; we're not aiming for parity with a specific tool).

## R5: Fixture-integration test strategy

**Decision**: Reuse the milestone-090 fixture-cache pattern + milestone-195 golden-SBOM comparison harness.

**Approach**:
1. `build.rs` fetches the pinned commit SHA of each fixture into `~/.cache/waybill/fixtures/<sha>/kusari-sandbox/test-{name}/`
2. New integration test `waybill-cli/tests/transitive_parity_python.rs` runs `waybill sbom scan` against each cached fixture
3. Compares emitted CDX against `waybill-cli/tests/fixtures/public_corpus/{name}/cdx.json` (golden)
4. Regeneration via `MIKEBOM_UPDATE_GOLDENS=1 cargo test -p waybill-cli --test transitive_parity_python`

**Cross-host stability**: Use the memory `feedback_cross_host_goldens` recipe verbatim — rewrite workspace path, strip content hashes (SHA-256 hex → `<hash-64>`), isolate HOME, mask serial + timestamp all-at-once. Verify via `LC_ALL=C sort` + normalized diff (memory `feedback_verify_golden_churn_normalized`).

**Fixture pinning**: The three fixtures MUST be pinned to specific commit SHAs recorded in `waybill-cli/tests/fixtures/public_corpus/{name}/pin.json`. When kusari-sandbox HEAD moves, we bump the pins deliberately and regen goldens.

## R6: Performance-budget sanity check

Reference: sweep numbers on 2026-08-31.

| Fixture | Baseline | Budget (spec) | New work | Expected |
|---------|---------:|---------------:|----------|----------|
| test-markitdown | 49ms | ≤ 549ms | 1 pyproject.toml + 1 uv.lock parse; 4 sub-pyproject.tomls | ~200ms (bounded TOML parse) |
| test-OctoPrint | 180ms | ≤ 680ms | 1 setup.py static parse (~30 KB source) + 1 requirements.txt | ~250ms |
| test-cpython | 575ms | ≤ 5575ms | Recursive `requirements*.txt` discovery across ~2000 dirs; parse ~10 files | ~2s worst case |

**Confidence**: budgets are generous; expected outcomes come in well under. If test-cpython crosses 3s, we investigate before merge.

**Regression sanity**: existing 20 sweep fixtures already scan in under 90s total. The default-prune-list additions (FR-004 extended) may SLIGHTLY reduce walker work on non-Python trees that happen to contain `.venv/` etc. → probably neutral or slightly positive.

## R7: Non-goals confirmation

Re-confirmed from spec's Out-of-Scope section:

- **Constraints files** (PEP 665) → next milestone
- **Editable installs full resolution** → v2 (v1 emits `unresolved`)
- **Namespace-packages via `[tool.setuptools.packages.find]`** → uses m127 workspace-member logic separately
- **Non-PyPI index sources** → captured as evidence, PURL type stays `pkg:pypi`
- **Docker/OCI image Python-tier scan enrichment** → separate concern (container layer walk)
- **Hatch / Bazel Python** → sibling readers m106/m103

## Summary

Zero remaining `NEEDS CLARIFICATION`. Format shapes confirmed. Infrastructure reuse map established. Test strategy pinned to m090+m195 precedent. Performance budgets sanity-checked. Ready for Phase 1.
