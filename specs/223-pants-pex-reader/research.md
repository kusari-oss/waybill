# Phase 0: Research — Pants pex-lockfile reader

**Feature**: 223-pants-pex-reader
**Date**: 2026-07-31
**Status**: Complete

Resolves the technical unknowns identified in the plan's Technical
Context. Each item follows the Decision / Rationale / Alternatives
shape.

---

## R1 — Pex lockfile JSON schema (Pex 2.x)

<!-- verified: 2026-07-31 against
     https://raw.githubusercontent.com/pantsbuild/pants/main/3rdparty/python/user_reqs.lock
     (Pants's own dogfood lockfile, 79 locked reqs) -->

**Decision**: Parse the Pex lockfile as a top-level JSON object with the
schema shape below. Only extract the fields waybill needs — no attempt
to model the full schema.

**Concrete shape** (verified empirically):

```jsonc
{
  "pex_version": "2.x.y",              // used for version-compatibility guard
  "style": "strict",                    // ignored
  "requirements": ["..."],              // input reqs; ignored (locked_requirements is authoritative)
  "locked_resolves": [                  // usually len==1, can be >1 for multi-platform resolves
    {
      "marker": null,                   // Python marker; ignored for MVP
      "platform_tag": null,             // platform tag; ignored for MVP
      "locked_requirements": [
        {
          "project_name": "annotated-doc",    // PyPI-normalized package name (kebab-case)
          "version": "0.0.4",                 // pinned version
          "requires_python": ">=3.8",         // requires-python constraint; recorded as annotation
          "requires_dists": [                 // PEP 508 requirement strings (dependency edges)
            "typing-extensions>=4.0.0; python_version < \"3.9\""
          ],
          "artifacts": [                       // 1..N artifacts per entry (typically wheel + sdist)
            {
              "algorithm": "sha256",
              "hash": "571ac1dc...bed320",
              "url": "https://files.pythonhosted.org/packages/.../annotated_doc-0.0.4-py3-none-any.whl"
            }
          ]
        }
        // ... more locked_requirements
      ]
    }
  ],
  // Other top-level fields we ignore: allow_builds, allow_prereleases,
  // allow_wheels, build_isolation, constraints, elide_unused_requires_dist,
  // excluded, only_builds, only_wheels, overridden, path_mappings,
  // pip_version, prefer_older_binary, requires_python, resolver_version,
  // target_systems, transitive, use_pep517, use_system_time
}
```

**Extraction rules for FR-002 / FR-003 / FR-008 / FR-009**:

1. **Component identity**: `project_name` (already PyPI-normalized to
   kebab-case at Pex generation time) → PURL name segment.
   `version` → PURL version segment.
2. **PURL type**: PyPI if `artifacts[].url` starts with
   `https://files.pythonhosted.org/`, else `pkg:generic/*` per Q2-A.
   Multiple artifacts per entry (wheel + sdist) → prefer the wheel
   URL for the source-type check; if any artifact is non-PyPI, the
   whole entry is treated as `pkg:generic/*` (PyPI wheels can't be
   mixed with non-PyPI sources in one lock entry).
3. **Hashes**: emit every `artifact` as a Hash entry (`algorithm` +
   `hash`) so consumers see both wheel + sdist hashes.
4. **Dependency edges**: parse `requires_dists[]` PEP 508 strings —
   for each, extract just the `project_name` (strip version specifiers,
   markers, extras). Emit as `depends: Vec<String>` on the
   `PackageDbEntry`; the m191 reconciler + graph builder resolve these
   to PURLs at emit time (matches the pip reader's approach).
5. **License**: NOT present in Pex lockfile format. FR-002's "license
   if the lockfile records one" is a null clause for Pex — the format
   doesn't carry PyPI Trove-classifier license strings. Licenses come
   from downstream enrichment (deps.dev / ClearlyDefined) if enabled.

**Version-compatibility guard (FR-006 / SC-005)**:

- Accept `pex_version` matching `^2\.` (any 2.x).
- Reject `pex_version` starting with `1.` or anything else → WARN
  "unsupported Pex lockfile version <ver>; skipping" + skip the file.
- Missing `pex_version` field: treat as unparseable → WARN + skip.

**Alternatives considered**:

- **Model the full Pex schema via `#[derive(Deserialize)]` on every
  top-level field** — rejected: brittle against Pex format evolution
  (Pex adds fields regularly; our parser breaks every time). The
  minimal-extraction approach with `#[serde(default)]` on optional
  fields is stable.
- **Parse via `serde_json::Value` bag and dig via `.get()` chains** —
  rejected: violates Principle IV (type-driven correctness) — bag-of-JSON
  in the hot path is exactly what our Constitution flags. Explicit
  Deserialize types are cheap here.
- **Shell out to `pex-lockfile-cli` for extraction** — rejected: adds
  a runtime binary dep (Pex itself, requires Python) waybill's readers
  never take on.

---

## R2 — Dev-resolve name allowlist for FR-008 lifecycle-scope tagging

**Decision**: Ship a `const &[&str]` allowlist of known Python-dev-tool
resolve names. When a resolve's name (parsed from the lockfile's
filename stem, e.g., `3rdparty/python/mypy.lock` → `mypy`) is in the
allowlist, tag components from that resolve as
`LifecycleScope::Dev`. Otherwise tag as `LifecycleScope::Runtime`.

**Allowlist** (from Q1 answer B + widened by common Pants community
usage observed across public repos):

```rust
const DEV_RESOLVE_NAMES: &[&str] = &[
    // Formatters + linters
    "black", "ruff", "isort", "yapf", "autopep8",
    // Type checkers
    "mypy", "pyright", "pyre",
    // Test runners
    "pytest", "unittest", "nose",
    // Coverage
    "coverage", "coveragepy",
    // Security scanners
    "bandit", "safety",
    // Docs / packaging
    "sphinx", "docs",
    // Generic dev-scope names Pants users commonly pick
    "lint", "test", "dev", "ci", "check", "tools",
];
```

Case-insensitive match. Multi-word resolve names (`unit-tests`,
`dev_tools`) get normalized to lowercase and checked as-is
(underscore-vs-hyphen NOT normalized — operators picking exact names
get exact matches).

**Rationale**:
- Q1 answer B mandated a name-allowlist approach.
- The default `default.lock` resolve stays `Runtime` (matches
  operator intuition; production code deps go here).
- Unknown resolve names default to `Runtime` (safe default per FR-008:
  under-flagging is preferable to over-flagging non-dev deps as dev,
  which would hide them from downstream security tooling that filters
  by scope).
- Every component always gets the `waybill:pants-resolve=<name>`
  annotation regardless of scope, so operators can spot-check + re-tag
  downstream if the heuristic gets it wrong.

**Alternatives considered**:

- **Regex-based classifier** — rejected: allowlist is more auditable
  and easier to expand; regex risk (matching `dev-container` as dev)
  outweighs marginal expressiveness gain.
- **Parse `pants.toml` `[python.resolves_to_lockfiles]` to learn
  resolve→scope mapping** — rejected: Pants config doesn't declare
  resolve scope explicitly (there's no `[python.resolves.mypy].scope
  = "dev"` idiom); operators would need to opt in per repo. Allowlist
  covers 90%+ of cases with zero operator work.
- **Read `[tool.pants]` in `pyproject.toml`** — rejected: uncommon in
  practice; out of scope for MVP.
- **Manifest-explicit override via a new CLI flag** — rejected: adds
  CLI surface for a corner case. If operators need custom scope
  mapping, they can annotate downstream via the existing
  `--supplement-cdx` flag (milestone 119).

---

## R3 — PyPI PURL normalization (matches existing pip reader)

**Decision**: Match the existing pip reader's PURL construction
verbatim. Concretely, for a Pex lockfile entry with
`project_name = "Foo_Bar"` and `version = "1.2.3"`:

1. Lowercase the name: `"foo_bar"` (PyPI's canonical form is lowercase).
2. Replace runs of `_` / `.` with a single `-`: `"foo-bar"` (PEP 503
   normalization; PyPI canonicalizes underscores + dots to hyphens for
   deduplication).
3. Construct PURL: `Purl::new("pkg:pypi/foo-bar@1.2.3")` — the
   `waybill_common::types::purl::Purl` newtype validates the shape at
   construction.

This ensures dedup collisions with the pip reader's output (which
already applies the same normalization to `requirements.txt` entries)
land on the same PURL string, feeding correctly into the m191
reconciler for FR-005.

**Verification during Phase 1**:
Read `waybill-cli/src/scan_fs/package_db/pip/*.rs` for the exact
normalization helper the pip reader uses. If it's factored out to a
public helper (e.g., `pip::normalize_pypi_name`), reuse it verbatim.
If it's inline, extract it to a `pip` module public helper as a
prerequisite so both readers share one implementation. (T003 in
`/speckit-tasks` will pick this up.)

**Rationale**:
- Consistency with pip reader is a hard correctness requirement
  (FR-005 dedup literally cannot work otherwise).
- PEP 503 normalization is a well-defined, stable spec — no risk of
  drift.
- Matching helper extraction (if needed) is a one-line refactor.

**Alternatives considered**:

- **Duplicate the normalization inline in the pants reader** — rejected:
  drift risk when PyPI updates their normalization spec (they haven't
  in years, but the risk exists).
- **Skip normalization; use Pex's `project_name` verbatim** — rejected:
  Pex normalizes at generate-time but the format doesn't *guarantee*
  it, and even if it did, matching pip reader's helper is still the
  right coupling.

---

## R4 — `pants.toml` parsing (minimal, config-schema-decoupled)

**Decision**: Parse `pants.toml` with `toml = "0.8"` (already a workspace
dep) into a minimal struct that captures only the `[python].lockfile`
key. Ignore every other section. Unknown values / missing key: fall
back to the FR-001 default glob without failing.

```rust
#[derive(Debug, Default, Deserialize)]
struct PantsConfig {
    #[serde(default)]
    python: PythonSection,
}

#[derive(Debug, Default, Deserialize)]
struct PythonSection {
    /// Custom lockfile path override. Absent = use FR-001 default glob.
    #[serde(default)]
    lockfile: Option<String>,
    // Future: resolves_to_lockfiles: HashMap<String, String>
    // — Pants's multi-resolve table. Not parsed in v1; multi-resolve
    // detection uses the default-glob filename convention instead.
}
```

**Rationale**:
- Full Pants config schema is huge and evolves regularly; coupling to
  it is a maintenance burden.
- The `[python].lockfile` key is the ONE thing that changes discovery
  behavior — everything else waybill doesn't care about.
- `#[serde(default)]` on every field means missing keys / wrong types
  gracefully fall through to defaults; no hard-fail on config drift.

**Alternatives considered**:

- **Parse the full `[python]` section, including
  `resolves_to_lockfiles` for multi-resolve mapping** — rejected:
  filename-convention discovery (`<resolve>.lock` under `3rdparty/python/`)
  works for 90%+ of real Pants repos and avoids coupling to the config
  schema's multi-resolve idiom (which has evolved between Pants 2.14 /
  2.16 / 2.19). Revisit if operator demand emerges.
- **Skip `pants.toml` parsing entirely, rely on glob-only** — rejected:
  US3 (P3) explicitly requires config-driven path discovery for
  non-standard layouts. Simple opt-in support here.

---

## R5 — Fixture strategy + prior-art check for SC-006

**Decision**: Ship 6 synthetic fixtures under `waybill-cli/tests/fixtures/pants_pex/`:

1. `minimal_python/` — US1 baseline: 3 synthetic locked packages
   (`waybill-fixture-a`, `-b`, `-c`), each with real PyPI-shape URL
   patterns pointing at `files.pythonhosted.org` (URL is synthetic —
   we're testing our parser, not fetching the artifact).
2. `multi_resolve/` — US1 scenario 4: `default.lock` + `mypy.lock` +
   `pytest.lock`, each with 2 synthetic packages.
3. `pants_toml_custom_path/` — US3: `pants.toml` declares
   `[python].lockfile = "build-support/py.lock"` + lockfile at that
   path; NO file at `3rdparty/python/`.
4. `with_requirements_txt/` — US2 dedup: `default.lock` +
   `requirements.txt`, both naming `waybill-fixture-shared==1.0.0`.
5. `non_pypi_entries/` — Q2 A coverage: `default.lock` with git-URL +
   direct-URL + local-path artifact entries alongside a normal PyPI
   entry.
6. `corrupt_lockfile/` — SC-005: intentionally truncated JSON (opens
   with `{"pex_version": "2.10.0", "locked_resolves": [{"locked_req`
   — deliberately unterminated). Verifies fail-open WARN behavior.

Every fixture uses synthetic package names (`waybill-fixture-*`) per
memory `feedback_fixture_synthetic_package_names`. No real PyPI
coordinates. Fixture directory sits inside the repo (small — total
< 20 KB across all 6 fixtures) rather than the m090 sibling-repo
fixture cache; the fixture-cache is for large realistic corpuses, not
tiny hand-crafted parser tests.

**Prior-art check for SC-006** (Trivy / Syft Pex coverage — done during
research):

- **Syft**: has explicit Pex-lockfile support as of Syft 1.19+ via the
  `python-pex-cataloger` (per Syft's release notes + `cmd/syft/cataloger`
  code). Recognizes `.pex.lock` files with the JSON shape verified in R1.
- **Trivy**: no dedicated Pex-lockfile cataloger as of the version pinned
  in waybill's `docs/audits/` reports (v0.71.1). Trivy falls back to
  `requirements.txt` if present; Pex lockfiles are invisible.
- **Snyk / GitHub Dependabot**: no Pex-lockfile support (both are
  requirements.txt / poetry.lock / uv.lock only).

**SC-006 revision**: waybill's ground-truth comparison should be
against Syft for coverage-parity claims. Trivy comparison is a "we do
more" data point.

**Rationale**: SC-006's ±5% tolerance was drafted before the prior-art
check. With Syft as the ground-truth comparator, ±5% is realistic.
Against Trivy, waybill will always be 100% (Trivy sees zero).

**Alternatives considered**:

- **Use real-world Pants repos** (e.g., a fork of Pants's dogfood tree)
  as fixture — rejected: violates
  `feedback_fixture_synthetic_package_names` (real PyPI coordinates
  trip Kusari Inspector); large repos are also slow to check into git.
- **Generate fixtures on the fly at test time via `pex` binary** —
  rejected: needs `pex` on the test runner (Python + Pex install);
  fixture-generation reproducibility issues; violates
  `feedback_fixture_synthetic_package_names`.

---

## Summary of resolved unknowns

| Plan Technical Context item | Status | Resolved by |
|-----------------------------|--------|-------------|
| Exact Pex lockfile JSON schema | ✅ | R1 (empirically verified against Pants dogfood lockfile) |
| Dev-resolve name allowlist for FR-008 | ✅ | R2 (allowlist + case-insensitive match + `waybill:pants-resolve` annotation always present) |
| PyPI PURL normalization matching pip reader | ✅ | R3 (reuse pip reader helper; extract to public helper if inline today) |
| `pants.toml` parse depth | ✅ | R4 (minimal-parse; `[python].lockfile` only; graceful fallback on missing / invalid) |
| Fixture strategy + Syft/Trivy prior-art for SC-006 | ✅ | R5 (6 synthetic fixtures; SC-006 comparator = Syft, not Trivy) |

All Technical Context unknowns resolved. Ready for Phase 1.
