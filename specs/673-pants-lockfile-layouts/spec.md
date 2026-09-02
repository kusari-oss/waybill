# Feature Specification: m673 — extend Pants lockfile discovery to repo-root + `lockfiles/` conventions

**Feature Branch**: `673-pants-lockfile-layouts`
**Created**: 2026-09-02
**Status**: Draft
**Input**: User description: "m673 (extend Pants lockfile discovery to repo-root + lockfiles/ conventions)"

## Clarifications

### Session 2026-09-02

- Q: Should content-detection (`pex_version` gate) apply to the m223 `3rdparty/python/*.lock` default glob too, or only to the new FR-001/FR-002 paths? → A: Scoped to FR-001/FR-002 only (Option B). `3rdparty/python/` retains m223's WARN-and-skip on parse failure — that directory is conventionally Pants-only, so a WARN there catches genuine operator mistakes rather than being a false-positive on unrelated `.lock` files.

## Motivation

Milestone-223 shipped a Pants pex-lockfile reader defaulting to the
`3rdparty/python/*.lock` glob. Milestone-672 added `//`-frontmatter
tolerance + `pants.toml` `[python.resolves]` bare-string map support.
A smoke test against Pants's own official example repos (2026-09-02)
uncovered a third canonical Python-lockfile layout m672 doesn't
cover — the layout Pants 2.31+ uses by default:

| Repo | Lockfile path | m672 discovery hits it? |
|---|---|---|
| `pantsbuild/example-python` | `<repo-root>/python-default.lock` | ❌ No |
| `pantsbuild/example-django` | `<repo-root>/lockfiles/python-default.lock` | ❌ No |
| Legacy Pants ≤ 2.29 setups | `3rdparty/python/<resolve>.lock` | ✅ Yes (m223) |

The impact is severe: on `example-python`, waybill emits 4
components with `version=null` (m670's `pyproject.toml` fallback)
against a lockfile that contains 8+ pinned entries with SHA-256
hashes + full transitive closure. **100% of the resolved-detail is
missed** because the Pants reader never fires on repos using the
modern layout — which is Pants's OWN documented default.

This milestone extends discovery to the two additional canonical
Python-lockfile paths (repo root + `lockfiles/`) so the m223 + m672
readers behave sensibly on Pants 2.31+ default layouts.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Repo-root `<resolve>.lock` files get discovered (Priority: P1)

An operator runs `waybill sbom scan` on a Pants 2.31+ monorepo that
declares no explicit `[python.resolves]` map — they use Pants's own
default, which places `<default_resolve>.lock` at the repo root
(`pantsbuild/example-python` shape). The operator expects the SBOM
to contain every locked dependency from that file, tagged with the
resolve name derived from the filename stem.

**Why this priority**: This is the shape Pants's own official example
projects use, and the shape a first-time Pants user gets when they
run `pants generate-lockfiles` on a fresh setup. Missing this shape
means waybill fails on the "hello world" of Pants Python usage.

**Independent Test**: Craft a synthetic fixture with `pants.toml` +
`<repo-root>/python-default.lock` (a valid PEX lockfile with a `//`-
frontmatter block). Assert the scan emits every locked component
with `pkg:pypi/*` PURLs + `waybill:pants-resolve=python-default`
annotation, without requiring any `pants.toml` override.

**Acceptance Scenarios**:

1. **Given** a repo with `pants.toml` (no `[python.resolves]` map) AND a repo-root `python-default.lock` (PEX shape with valid `pex_version` + `//`-frontmatter block), **When** `waybill sbom scan` runs, **Then** every `locked_requirement` in the lockfile emits a component tagged with `waybill:pants-resolve=python-default` AND the scan exits 0.
2. **Given** a repo with multiple repo-root `*.lock` files (e.g. `python-default.lock`, `mypy.lock`, `pytest.lock`) — matching multi-resolve Pants setups without an explicit `[python.resolves]` map, **When** the scan runs, **Then** each recognized PEX lockfile emits its components with the resolve name from its filename stem.
3. **Given** a repo with a repo-root `something.lock` file that is NOT a PEX lockfile (e.g. a `Cargo.lock`, `bun.lock`, `poetry.lock`, or some tooling's opaque `.lock` shape), **When** the scan runs, **Then** the file is NOT mis-parsed as a PEX lockfile AND the appropriate ecosystem reader handles it AND no WARN fires from the Pants reader about the file.

---

### User Story 2 - `lockfiles/` directory gets discovered (Priority: P1)

An operator runs waybill on a Pants monorepo that keeps its Python
lockfiles in a dedicated `lockfiles/` directory (`pantsbuild/
example-django` shape). This is a common convention for repos that
have MANY resolves and want a dedicated location for them.

**Why this priority**: This is the shape `example-django` uses, and
the shape Pants's own docs recommend for multi-resolve setups larger
than 2-3 resolves. Missing it means waybill fails on the majority of
"medium-to-large" Pants Python setups.

**Independent Test**: Craft a fixture with `<repo-root>/lockfiles/
python-default.lock` + `<repo-root>/lockfiles/mypy.lock`. Scan the
fixture; assert both files emit their components with correctly-
tagged resolve names.

**Acceptance Scenarios**:

1. **Given** a repo with `pants.toml` (no `[python.resolves]` map) AND `<repo-root>/lockfiles/python-default.lock` + `<repo-root>/lockfiles/mypy.lock`, both valid PEX lockfiles, **When** the scan runs, **Then** both files emit their components with resolve-name tags matching the filename stems (`python-default`, `mypy`).
2. **Given** a repo with BOTH `<repo-root>/lockfiles/foo.lock` AND `<repo-root>/3rdparty/python/foo.lock` naming the same resolve, **When** the scan runs, **Then** the file is parsed exactly once (canonical-path dedup per m672 FR-009 semantics extended to the new layouts).
3. **Given** a repo with `<repo-root>/lockfiles/README.md` + `<repo-root>/lockfiles/python-default.lock`, **When** the scan runs, **Then** the `README.md` file is ignored by the Pants reader (extension-gated to `.lock`) AND the PEX lockfile still emits normally.

---

### User Story 3 - Content-detection guards against false-positive matches (Priority: P2)

Waybill must not mis-identify a non-PEX `.lock` file (Cargo,
bun.lock, poetry.lock, other tooling) as a Pants PEX lockfile
just because it sits at the repo root or under `lockfiles/`. When
the Pants reader encounters a `.lock` file whose top-level JSON does
NOT declare a `pex_version` field matching `^2\.`, it MUST silently
skip the file (no WARN — this is not a corrupt Pants lockfile, it's
a non-Pants file that happens to share the extension).

**Why this priority**: Repo-root and `lockfiles/` are wide-scope
locations. A false positive (e.g. Pants reader loudly WARNing about
someone's `Cargo.lock`) is a UX regression that would poison the
milestone's reputation.

**Independent Test**: Fixture with `<repo-root>/Cargo.lock` (real
cargo shape) + `<repo-root>/lockfiles/poetry.lock` (real poetry
shape) — both are `.lock` files but neither is a PEX lockfile. Scan
the fixture; assert (a) the Pants reader emits NO components from
those files, (b) the Pants reader emits NO WARN about them, (c) the
Pants reader-complete log shows those files were NOT counted in
`lockfiles_discovered`.

**Acceptance Scenarios**:

1. **Given** a repo with a repo-root `Cargo.lock` (cargo shape, no `pex_version` field), **When** the scan runs, **Then** the Pants reader silently skips it (no WARN, no component, no counter increment) AND the cargo reader handles it normally.
2. **Given** a repo with `<repo-root>/lockfiles/poetry.lock` (poetry shape — top-level `[metadata]` TOML, not JSON), **When** the scan runs, **Then** the Pants reader silently skips it AND the pip reader handles it (existing pip-poetry pathway).
3. **Given** a repo with `<repo-root>/broken.lock` (starts with `{` but has no `pex_version` field — e.g. a bespoke JSON file with `.lock` extension), **When** the scan runs, **Then** the Pants reader silently skips it (no false-positive WARN — the file just isn't a Pants lockfile).

---

### Edge Cases

- A repo has BOTH `<repo-root>/python-default.lock` AND `<repo-root>/lockfiles/python-default.lock` (unusual — the operator would probably not do this deliberately). The reader emits both with the same resolve name `python-default` — downstream m191 reconciler collapses duplicate PURLs anyway.
- A repo's `lockfiles/` directory contains subdirectories (e.g. `lockfiles/python/foo.lock`). Discovery is **non-recursive** for the new layouts — only files directly under `lockfiles/` count. Subdirectory content is out of scope for v1 (v2 extension point if needed).
- A repo has `pants.toml` `[python.resolves]` declaring an explicit path that OVERLAPS with one of the new default paths. The explicit declaration wins (map key is authoritative per m672 FR-009).
- A `.lock` file at repo root with binary content (not JSON at all — e.g. a compiled artifact). `serde_json::from_slice` fails; reader silently skips (US3 content-detection covers this).
- A `.lock` file with valid JSON but `pex_version = "1.9.0"` (Pex 1.x). Existing m223 behavior: the reader logs a WARN "unsupported Pex lockfile version" and skips. Kept unchanged — a Pex 1.x lockfile is still a Pants lockfile in shape; it just names a version we don't support.
- The `lockfiles/` directory is a symlink to another directory (e.g. `lockfiles → ../shared/lockfiles`). Canonicalization follows the symlink per m672 FR-009 semantics.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The Pants reader MUST enumerate `<repo-root>/*.lock` files as lockfile candidates in addition to the existing `<repo-root>/3rdparty/python/*.lock` default glob.
- **FR-002**: The Pants reader MUST enumerate `<repo-root>/lockfiles/*.lock` files as lockfile candidates. Discovery is non-recursive — only files directly under the `lockfiles/` directory are considered.
- **FR-003**: For every `.lock` file discovered via FR-001 (repo-root) or FR-002 (`lockfiles/`), the reader MUST content-detect that the file is a valid PEX lockfile before treating it as one. Detection MUST accept a file iff, after `//`-frontmatter stripping (per m672 FR-001), the top-level JSON contains a `pex_version` field whose string value matches `^2\.` (identical to the existing m223 accept-criterion). Content-detection is **scoped to the wide-scope FR-001 + FR-002 discovery paths only** — per the 2026-09-02 clarification, files under the m223 `3rdparty/python/*.lock` default glob and files declared via `pants.toml` explicit overrides (`[python].lockfile` singular + `[python.resolves]` map) retain m223 semantics (attempt-to-parse + WARN-on-failure) because those paths are conventionally Pants-owned; content-detection there would hide genuine operator mistakes.
- **FR-004**: When a `.lock` file discovered via FR-001 or FR-002 fails the FR-003 content-detection check (missing `pex_version`, non-JSON content, wrong version prefix), the reader MUST silently skip the file — NO WARN, NO counter increment, NO false-positive component. This prevents the reader from spamming logs about Cargo/Poetry/bun lockfiles that happen to share the extension. The narrow-scope paths (m223 default glob + explicit overrides) continue to emit m223's WARN-and-skip on genuine parse failure — this is intentional (see FR-003 clarification note).
- **FR-005**: The reader MUST canonicalize every candidate path via `std::fs::canonicalize` and dedup per m672 FR-009 semantics. When the same file appears via multiple discovery paths (e.g. via `[python.resolves]` map explicit path AND via the FR-001 repo-root enumeration), it is parsed exactly once and the map-declared resolve name wins.
- **FR-006**: The `pants-pex reader complete` INFO log line MUST fire when at least one Pants signal is present (per m672 FR-010). The m672 signal set MUST be extended to include: repo-root `*.lock` files that content-detect as PEX lockfiles, OR the presence of a `<repo-root>/lockfiles/` directory. A non-Pants repo (no `pants.toml`, no `3rdparty/python/`, no repo-root PEX lockfile, no `lockfiles/`) MUST remain silent.
- **FR-007**: When a Pants reader silently skips a `.lock` file via FR-004 content detection, it MUST leave that file available for downstream readers (cargo, pip-poetry, bun, etc.) without contaminating any per-reader state. Byte-identity for the SBOM's cargo/pip-poetry/bun components MUST be preserved.
- **FR-008**: File-tier byte-identity: every `.lock` file that fails FR-003 content detection MUST still be considered by the file-tier walker (m133 orphan mode + m671 source-tree mode). Silent skip in the Pants reader means the file is invisible to Pants, NOT invisible to the whole scan.
- **FR-009**: The reader MUST NOT recurse into subdirectories of `<repo-root>/lockfiles/`. Deeper nesting (`lockfiles/python/foo.lock`, `lockfiles/team-a/mypy.lock`) is out of scope for v1 — deferred to a v2 milestone if demand emerges.

### Key Entities *(include if feature involves data)*

- **PEX-lockfile content signature**: A file whose bytes, after m672 `//`-frontmatter stripping, parse as JSON AND have a top-level `pex_version` field whose string value matches `^2\.`. Used as the FR-003/FR-004 accept/reject discriminator on the wide-scope repo-root and `lockfiles/` discovery paths.
- **Canonical Pants Python-lockfile directory**: One of `<repo-root>`, `<repo-root>/lockfiles/`, `<repo-root>/3rdparty/python/`. The reader enumerates `*.lock` files in each; non-recursive for repo-root and `lockfiles/`, non-recursive for `3rdparty/python/` (matches m223).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the `pantsbuild/example-python` shape (Pants 2.31+ default, `<repo-root>/python-default.lock`), waybill emits ≥ 8 pypi components tagged with `waybill:pants-resolve=python-default` — matching the number of `project_name` entries in the lockfile. Previously (m672): 0 components from the Pants reader.
- **SC-002**: On the `pantsbuild/example-django` shape (`<repo-root>/lockfiles/python-default.lock`), waybill emits every pypi component from the lockfile with the correct resolve-name tag. Previously (m672): 0.
- **SC-003**: On a repo with `<repo-root>/Cargo.lock`, `<repo-root>/lockfiles/poetry.lock`, AND `<repo-root>/lockfiles/python-default.lock` (PEX shape), waybill emits (a) cargo components from Cargo.lock via the cargo reader, (b) poetry components from poetry.lock via the pip reader, (c) pypi components from python-default.lock via the Pants reader — no cross-reader contamination.
- **SC-004**: On a repo with only cargo / poetry / bun `.lock` files (no PEX lockfile anywhere) AND those files sit at repo-root OR under `lockfiles/` (the FR-001/FR-002 wide-scope paths), the Pants reader emits **zero WARN log lines** about those files. Locked by a byte-identity assertion on the stderr's Pants-reader log-line count. Note: a non-PEX `.lock` file under `3rdparty/python/` continues to WARN per m223 (that directory is conventionally Pants-only; a WARN there catches genuine operator mistakes).
- **SC-005**: On repos that currently rely ONLY on `3rdparty/python/*.lock` (m223 layout) OR on `[python.resolves]` map (m672 layout), emitted SBOMs are byte-identical to pre-m673 output. Golden-fixture test locks this invariant.
- **SC-006**: On a repo with NO Pants signals of any kind (no `pants.toml`, no `3rdparty/python/`, no repo-root PEX lockfile, no `lockfiles/` directory), the Pants reader emits ZERO log lines — matches m223 SC-003 and m672 FR-012.

## Assumptions

- **The three canonical Pants Python-lockfile layouts** are: (a) `<repo-root>/*.lock`, (b) `<repo-root>/lockfiles/*.lock`, (c) `<repo-root>/3rdparty/python/*.lock`. This assumption is drawn from Pants's own official example repositories (`pantsbuild/example-python`, `pantsbuild/example-django`) plus common documented practice. If a fourth canonical layout exists (e.g. Pants 2.35+ introduces a new convention), that's a future extension.
- **Non-recursive discovery is correct for v1**. Multi-team monorepos with many resolves sometimes nest lockfiles under `lockfiles/<team>/foo.lock`. That shape is deferred to a v2 milestone — deliberate to keep the discovery blast radius bounded.
- **Content-detection via `pex_version`** is a robust discriminator. Every valid PEX lockfile Pants has ever emitted (from Pex ≥ 2.0) declares this top-level field. Non-PEX `.lock` files (Cargo, Poetry, bun, npm, pnpm) either aren't JSON, aren't top-level-object JSON, or don't have a `pex_version` field. There is no known false-positive shape.
- **Byte-identity on pre-m672 layouts** is preserved by only ADDING candidate paths — never removing or reordering the m223/m672 discovery paths. The m672 canonicalization + dedup already handles duplicates via canonical-path collision.
- **The FR-004 silent-skip requirement** is critical for UX. Non-PEX `.lock` files are common (every Rust repo has `Cargo.lock`, every modern JS repo has `bun.lock` or similar). WARN-ing about them would spam logs on the majority of scanned repos.
- **File-tier and downstream readers remain unaffected**. The Pants reader's silent-skip on non-PEX `.lock` files does not touch the file bytes or any downstream state — cargo/pip/bun readers see the file exactly as they would today.
- Symlink behavior matches m672: `std::fs::canonicalize` follows symlinks; a `lockfiles/` symlinked to another directory is followed.
- The `lockfiles/` directory name is case-sensitive on Linux/macOS default filesystems. Pants's own docs consistently spell it lowercase; case-mismatched variants (`Lockfiles/`, `LOCKFILES/`) are out of scope.
