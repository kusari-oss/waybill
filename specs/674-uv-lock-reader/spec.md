# Feature Specification: m674 — uv.lock reader enhancement (per-variant PURL + hash extraction + Pants FR-002 fallback)

**Feature Branch**: `674-uv-lock-reader`
**Created**: 2026-09-02
**Status**: Implemented (pivoted 2026-09-02 from "new reader" → "enhance existing m106 reader")

**Input**: User description: "uv.lock reader for the UV Python package manager (recovers Pants-with-uv-backend + generic uv-managed projects)"

## Scope pivot (2026-09-02)

**Original framing (rejected)**: Add a new `uv/` package_db reader.

**Actual scope (shipped)**: Enhance the existing m106 uv.lock reader at `waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs` (originally shipped for issue #276). Discovery of the pre-existing reader happened during implementation of T013 (multi-source integration test) when unexpected `pkg:pypi/*` components appeared alongside the m674 module's output. m106's reader was already discovering `<rootfs>/uv.lock` and emitting components — but missing per-variant PURL selection, hash extraction, and the C157 annotation. Pivoted to enhance-existing shape; preserves m106's workspace-mode + m183 optional-deps handling.

**What enhanced**:
- Per-source-variant PURL selection (registry → pypi; git/path/url → generic; editable/virtual → pypi for m106 backward-compat).
- SHA-256 hash extraction from `sdist` + `wheels[]`, deduped across multi-platform wheels.
- New C157 `waybill:python-lockfile-format=uv` per-component annotation for format provenance.
- `waybill:pypi-source-url` annotation for non-default registries (private mirrors).
- `waybill:source-url` annotation for git/path/url variants.
- New `parse_uv_lock_bytes` entry point for the Pants FR-002 fallback.
- Pants FR-002 hook in `pants/mod.rs::read` — on PEX parse failure, try uv-lock parse and emit with `waybill:pants-resolve` preserved.

## Motivation

Astral's [uv](https://github.com/astral-sh/uv) is a fast Python
package manager (a pip / pip-tools / poetry / pipenv replacement)
that has been broadly adopted since its 2024 release. It emits a
TOML-format lockfile at `<repo-root>/uv.lock` with a well-documented
schema. Waybill today has NO reader for this format — every uv-
managed Python project emits with 0 pypi components from waybill's
main-line readers (pip-poetry / pip-pipenv / pip-requirements-txt),
with rescue only via the m670 `pyproject.toml`-declared-deps
fallback (which produces `version=null` unresolved entries with no
transitive detail and no hashes).

Two ecosystems benefit from a uv.lock reader:

1. **Standalone uv-managed Python projects** — the fast-growing
   ecosystem including `meilisearch/meilisearch-python`, `NRCan/geo-deep-learning`,
   many MCP servers (`democratize-technology/chronos-mcp`,
   `traceloop/opentelemetry-mcp-server`), and thousands of smaller
   public and private projects.
2. **Pants monorepos using `uv` as the resolver backend** — modern
   Pants (2.31+) supports UV as a resolver, generating uv.lock
   shape files instead of pex-lockfile JSON. Discovered during the
   m673 post-implementation sweep: `lablup/backend.ai` (265 MB
   Backend.AI GPU orchestration platform) has 9 uv.lock files that
   the m672 `[python.resolves]` discovery correctly locates but
   the m223 PEX-JSON parser correctly rejects. Waybill emits 133
   pypi from pyproject.toml fallback where the actual lockfiles
   contain 500+ resolved-with-hashes entries.

This milestone adds a new reader at `waybill-cli/src/scan_fs/package_db/uv/`
that parses uv.lock files and emits components identical in shape
to the m223 Pants PEX reader (`pkg:pypi/<name>@<version>` for
registry-sourced packages, `pkg:generic/*` with source annotations
for git / path / url / editable sources).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Standalone uv-managed Python project (Priority: P1)

An operator runs `waybill sbom scan` on a Python project that uses
`uv` as its package manager. The repo has `<root>/pyproject.toml`
+ `<root>/uv.lock` (uv's default output location). The operator
expects the SBOM to contain every locked distribution from uv.lock
with `pkg:pypi/` PURLs and SHA-256 hashes.

**Why this priority**: This is the "hello world" of uv-managed
projects. Missing this means waybill emits nothing usable on the
fastest-growing Python packaging ecosystem.

**Independent Test**: Craft a synthetic fixture with `<repo-root>/pyproject.toml`
+ `<repo-root>/uv.lock` (real uv-shape TOML with 3 synthetic packages,
each with registry source + sdist + wheel URLs). Assert the scan
emits 3 pypi components with correct PURLs + hashes.

**Acceptance Scenarios**:

1. **Given** a repo with `<root>/pyproject.toml` + `<root>/uv.lock` (3 packages from PyPI registry), **When** the scan runs, **Then** exactly 3 components emit with `pkg:pypi/<name>@<version>` PURLs AND each carries SHA-256 hashes from the lockfile's wheels array AND scan-exit 0.
2. **Given** a repo with `<root>/uv.lock` containing multi-source packages (some registry, some git, some path), **When** the scan runs, **Then** registry packages emit as `pkg:pypi/*` AND non-registry packages emit as `pkg:generic/*` with `waybill:source-type` + `waybill:source-url` annotations (matching m223 Pants non-PyPI shape at pants/lockfile.rs).
3. **Given** a repo with `<root>/uv.lock` but no `<root>/pyproject.toml`, **When** the scan runs, **Then** the uv.lock is still parsed and emits components (uv.lock is self-describing; pyproject.toml is not a prerequisite).

---

### User Story 2 - Pants monorepo using uv as resolver backend (Priority: P1)

An operator runs waybill on a Pants monorepo that uses `uv` as its
Python resolver backend (modern Pants 2.31+ setting). The
`pants.toml` `[python.resolves]` map names one or more `*.lock`
files that are uv-shape TOML (not PEX-shape JSON). Waybill's m672
discovery correctly locates the files; today the parser incorrectly
rejects them as invalid PEX.

**Why this priority**: This is the `lablup/backend.ai` case observed
in the m673 post-implementation sweep. A 265 MB Backend.AI GPU
orchestration monorepo emits 133 pypi from pyproject.toml fallback
today; with m674 the 9 declared uv-shape lockfiles yield 500+
transitive pypi entries with full resolved detail.

**Independent Test**: Craft a fixture with `<repo-root>/pants.toml`
declaring `[python.resolves]` naming 2 uv-shape lockfiles at
`3rdparty/python/*.lock`. Assert both files are parsed by the new
uv reader (NOT the pants pex reader — the pants reader silent-skips
or WARNs, and the uv reader picks them up), AND components emit
with `waybill:pants-resolve=<name>` annotations matching the m223
Pants scope-tag convention.

**Acceptance Scenarios**:

1. **Given** a Pants monorepo with `pants.toml` `[python.resolves]` naming 2 uv-shape `.lock` files, **When** the scan runs, **Then** both files are parsed by the uv reader AND each emitted component carries a `waybill:pants-resolve=<name>` annotation matching the pants.toml map key.
2. **Given** a Pants monorepo with mixed lockfile formats — some PEX-shape (m223 reader parses) + some uv-shape (m674 reader parses), **When** the scan runs, **Then** every lockfile from every format emits its components without cross-reader contamination AND with correct per-file resolve-name tags.
3. **Given** the same `backend.ai` shape (m672 `[python.resolves]` naming uv-shape lockfiles + m673 wide-scope repo-root discovery also finding some), **When** the scan runs, **Then** the m672 explicit-config path takes precedence (via existing m672 canonicalize+dedup) AND the emitted resolve-name uses the pants.toml map key.

---

### User Story 3 - Interaction with existing pip + m670 readers (Priority: P2)

An operator runs waybill on a repo that has BOTH a `pyproject.toml`
with declared deps (m670 fallback surface) AND a `uv.lock`. Today
m670 emits `version=null` unresolved components from pyproject.toml
alone. With m674, uv.lock takes precedence — every uv.lock entry
emits with its resolved version + hashes, and the pyproject.toml-
only entries are suppressed via the m191 reconciler's existing
higher-tier-wins policy.

**Why this priority**: Defense in depth. Prevents duplicate
components (one unresolved from m670, one resolved from m674) from
appearing in the same SBOM.

**Independent Test**: Fixture with `<root>/pyproject.toml` declaring
3 deps + `<root>/uv.lock` with 3 packages (matching names) plus 5
transitive-only packages. Assert 8 total pypi components emit (3
top-level with resolved version + hashes; 5 transitive from
uv.lock only), zero duplicates.

**Acceptance Scenarios**:

1. **Given** a repo with pyproject.toml (declaring `foo`, `bar`, `baz`) + uv.lock (resolving `foo==1.0` + `bar==2.0` + `baz==3.0` + transitives `x==0.1`, `y==0.2`), **When** the scan runs, **Then** 5 components emit — 3 top-level with resolved versions + 2 transitives — AND NO `version=null` unresolved entries appear.
2. **Given** a repo with ONLY pyproject.toml (no uv.lock, no requirements.txt, no poetry.lock), **When** the scan runs, **Then** m670 declared-deps fallback fires normally (pre-m674 behavior preserved) — verifies m674 doesn't accidentally suppress m670 when uv.lock is absent.
3. **Given** a repo with uv.lock but no pyproject.toml, **When** the scan runs, **Then** the uv reader emits components based on uv.lock alone (no m670 interaction needed).

---

### Edge Cases

- **uv.lock schema versions**: v1 is the current shape. Future v2 / v3 may exist. Reader accepts `version = N` where N ∈ {1} for m674 v1; unknown versions WARN + skip (matches m223 Pex-version handling pattern).
- **Multi-source packages**: uv.lock supports `source = { registry = "..." }`, `{ git = "URL", rev = "SHA" }`, `{ path = "..." }`, `{ url = "..." }`, `{ editable = "..." }`, `{ virtual = "..." }`. Each shape maps to a specific PURL construction rule (see FRs).
- **Editable / virtual sources**: `source = { editable = "." }` refers to the root project itself (the pyproject.toml's own package). Emit as `pkg:generic/<pyproject-name>@<pyproject-version>` OR skip (implementation decision — see FR-006).
- **Empty uv.lock**: `version = 1\n[options]\n...\n` with no `[[package]]` entries. Parse succeeds; emit 0 components; INFO log fires with `packages_emitted=0`.
- **Malformed uv.lock**: `serde` deserialization fails on shape drift. WARN + skip (matches m223 Pex parse-failure semantics).
- **Discovery path**: uv.lock at repo-root is the convention. m674 does NOT recurse (v1 scope). Subdirectory uv.lock files (`services/api/uv.lock`) are v2 extension point.
- **Interaction with the m673 Pants signal**: a `pants.toml` `[python.resolves]` entry naming a uv-shape file gets discovered by m672 but rejected by the m223 PEX parser. m674's uv reader should attempt to parse those files independently.
- **Duplicate wheel URLs**: uv.lock may list many wheel artifacts per package (multiple platforms). Emit ONE component per package (dedup by name+version); attach a set of wheel URLs as annotations or in `evidence.identity[].methods[]` per m223 convention.
- **`resolution-markers` + `dependency-groups`**: uv.lock records marker constraints (e.g. `python_version < '3.11'`) and dependency groups. m674 v1 emits every locked package regardless of resolution-marker filtering — matches how the pip reader treats pyproject dep groups (annotations record the group, but no filtering).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The uv reader MUST discover `<scan_root>/uv.lock` at repo root and treat every file matching that path as a uv-format candidate.
- **FR-002**: The uv reader MUST also opportunistically attempt to parse any `.lock` file discovered by the m672 Pants `[python.resolves]` explicit-config path that FAILED the m223 PEX-JSON parse. Rationale: modern Pants with uv resolver backend generates uv-shape lockfiles at Pants-declared paths. Implementation: either (a) hook into the m673 discovery pipeline as a fallback parser or (b) run a second discovery pass on the same paths. Design detail deferred to `plan.md`.
- **FR-003**: The uv reader MUST parse uv.lock v1 schema (top-level `version = 1`). For unknown schema versions (`version = 2` or higher), the reader MUST emit a WARN naming the observed version + skip the file (matches m223 Pex 1.x rejection pattern).
- **FR-004**: For each `[[package]]` entry with `source = { registry = "..." }`, the reader MUST emit a `pkg:pypi/<name>@<version>` PURL. The `<name>` MUST be normalized via the existing `pip::normalize_pypi_name_for_purl` helper (shared with pip + m223 pants readers).
- **FR-005**: For each `[[package]]` entry with `source = { git = "URL", rev = "SHA" }`, the reader MUST emit a `pkg:generic/<name>@<version>` PURL with `waybill:source-type=git` + `waybill:source-url=<URL>@<rev>` annotations (matching m223 Pants non-PyPI shape at C1 + C144 catalog rows).
- **FR-006**: For each `[[package]]` entry with `source = { editable = "..." }` OR `source = { virtual = "..." }`, the reader MUST skip the entry — those represent the pyproject.toml's own package (self-reference); the m670 pip reader + m127 root selector already handle main-module emission.
- **FR-007**: For each `[[package]]` entry with `source = { path = "..." }` OR `source = { url = "..." }`, the reader MUST emit a `pkg:generic/*` PURL with the appropriate source annotations (matches FR-005 shape).
- **FR-008**: For every wheel URL in `[[package]].wheels[]` AND the sdist URL in `[[package]].sdist`, the reader MUST attach the SHA-256 hash to the emitted component's `hashes[]` field. When multiple wheel URLs are present (multi-platform), attach one hash per distinct-hash wheel — dedup by hash-hex to avoid duplicating the same hash across e.g. all `manylinux_*_x86_64` variants.
- **FR-009**: For every `[[package]]` entry, the reader MUST emit a `waybill:source-files` annotation naming the source `uv.lock` path (matches m223 Pants convention). This preserves round-tripability for audit.
- **FR-010**: When a Pants context is detected (m672 discovery had explicit-config, OR m673 discovery found the file under a Pants layout), the emitted component MUST carry a `waybill:pants-resolve=<name>` annotation matching the m223 Pants convention. Otherwise (standalone uv projects), the annotation is absent.
- **FR-011**: The reader MUST emit a `waybill:python-lockfile-format=uv` per-component annotation identifying the source-format for downstream consumers. This distinguishes uv-lock-sourced components from pex-lock-sourced (m223), poetry-lock-sourced (m670-adjacent), and requirements-txt-sourced (m670) components.
- **FR-012**: The reader-complete INFO log MUST fire iff at least one uv.lock was discovered. Log line shape: `INFO ... uv reader complete lockfiles_discovered=N lockfiles_parsed_ok=M lockfiles_skipped_corrupt=K components_emitted=C`. Matches m223 + m672 log conventions.
- **FR-013**: The reader MUST NOT emit any log line on scans of repos with NO uv.lock and NO Pants signal that could imply uv usage (byte-identity for non-uv repos — matches m223 SC-003 + m672 FR-012).
- **FR-014**: The m191 reconciler MUST prefer uv.lock-sourced components over m670-declared-deps unresolved components (higher-tier-wins per FR-014 clarification 2026-09-02). Existing reconciler semantics apply; m674 emits with a higher confidence tier tag if needed for the reconciler to notice.
- **FR-015**: The `pkg:pypi` PURL emitted for a registry-sourced package MUST match byte-for-byte what the pip reader would emit for the same package if it appeared in a poetry.lock or pip-compile output — same name normalization, same version handling, no drift.

### Key Entities *(include if feature involves data)*

- **uv.lock file**: TOML document with top-level `version = 1` + `requires-python = "..."` + optional `resolution-markers = [...]` + array-of-tables `[[package]]`. Discovered at `<scan_root>/uv.lock` or via Pants integration (FR-002).
- **`[[package]]` entry**: TOML table with fields `name` (string), `version` (string), `source` (inline table with one of `registry` / `git` / `path` / `url` / `editable` / `virtual` keys), `dependencies` (optional array of tables naming other packages), `sdist` (optional inline table with `url` + `hash` + `size`), `wheels` (optional array of tables — one per wheel artifact — with `url` + `hash` + `size`).
- **Source variant enum**: A 6-variant enum `UvSource { Registry(url), Git{url, rev}, Path(path), Url(url), Editable(path), Virtual(name) }`. Drives per-variant PURL construction rules per FR-004 through FR-007.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a synthetic fixture with `<repo-root>/uv.lock` naming 3 registry-sourced packages + 2 transitive packages, waybill emits exactly 5 `pkg:pypi/*` components with SHA-256 hashes.
- **SC-002**: On the `lablup/backend.ai` shape (m673 sweep observed: 9 uv-shape lockfiles named by `[python.resolves]`), waybill emits ≥ 400 pypi components across all 9 resolves (target: recover the transitive detail m673 misses today). Previously (pre-m674): 0 components from those files.
- **SC-003**: On the `meilisearch/meilisearch-python` shape (standalone uv-managed project — 53 packages in uv.lock), waybill emits ≥ 50 pypi components.
- **SC-004**: On a repo with ONLY pyproject.toml (no uv.lock), pre-m674 emitted SBOM is byte-identical to post-m674 output. Locked by golden-fixture regression test.
- **SC-005**: On a repo with pyproject.toml + uv.lock, m670's `version=null` unresolved emissions are SUPPRESSED for packages that also appear in uv.lock — verified by asserting zero components with `version=null` in the emitted SBOM when uv.lock is present.
- **SC-006**: uv reader adds ≤ 10 ms scan-time overhead on a repo with no uv.lock (byte-identity + fast-path check for the discovery directory).
- **SC-007**: uv reader adds ≤ 100 ms per `.lock` file it successfully parses (measured against `meilisearch-python`'s 53-package uv.lock and `st2`'s 275-package pex-lock — both should be under 100 ms).

## Assumptions

- **uv.lock v1 schema**: current schema per Astral's [documentation](https://docs.astral.sh/uv/reference/settings/#lockfile). Future v2 handled per FR-003 (WARN + skip).
- **PyPI-name normalization**: shared with pip + m223 pants readers via `pip::normalize_pypi_name_for_purl` — no drift on cross-format identity.
- **Multi-platform wheels**: uv.lock lists many wheels per package (Linux/macOS/Windows × Python 3.x). SC-001 counts ONE component per package (dedup by name+version), NOT one component per wheel artifact.
- **Pants integration via FR-002**: implementation detail; the design choice between "hook into m673 discovery as fallback" vs. "run a second discovery pass" is deferred to plan.md.
- **`editable = "."` / `virtual = "..."` sources**: represent the pyproject.toml's own package; FR-006 skips these because m127 root selector + m670 main-module emission already handle them. Emitting them would create duplicate main-module components.
- **`resolution-markers` filtering**: NOT applied in v1. Every locked package emits regardless of the resolution-marker constraint. Matches how pip reader handles pyproject dep groups. Marker-aware emission is a v2 extension point.
- **Non-recursive discovery**: only `<scan_root>/uv.lock` (repo-root) is discovered in v1. Subdirectory uv.lock files are v2 extension point (mirror m673 non-recursive semantics).
- **No support for `.uv-cache` / `.venv/` scanning**: those are runtime caches, not lockfiles. Out of scope forever.
- **Interaction with m670 pyproject.toml declared-deps** (FR-014): the m191 reconciler is expected to handle this via existing higher-tier-wins policy (uv.lock is a lockfile-tier source, pyproject.toml declared-deps is a design-tier source). If reconciler drift is discovered, the m191 mechanism gets extended in plan.md — not this spec.
- **Zero new Cargo dependencies**: `toml = "0.8"` is already a workspace dep (used by cargo, pip, pants config parsers). No new crates needed.
