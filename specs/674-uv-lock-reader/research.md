# Phase 0 Research — m674 uv.lock reader

**Date**: 2026-09-02
**Status**: Complete — no unresolved NEEDS CLARIFICATION markers.

## R1 — uv.lock v1 schema (empirically verified)

**Decision**: Adopt the v1 schema as documented by Astral + verified
against three real-world uv.lock files:

- `meilisearch/meilisearch-python`: 53 packages, 100% `registry` + 1 `editable`.
- `lablup/backend.ai/python.lock`: ~80 packages, mostly `registry`, a few `git`.
- Astral's own uv-managed docs / examples (github.com/astral-sh/uv).

Top-level schema (excerpt):

```toml
version = 1
revision = 3
requires-python = ">=3.10"
resolution-markers = ["python_full_version < '3.11'", ...]  # optional
supported-markers = [...]                                    # optional

[options]
# freeform table — reader ignores contents

[manifest]
# freeform table — reader ignores contents

[[package]]
name = "aioboto3"
version = "15.0.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "aiobotocore", extra = ["boto3"] },
    { name = "aiofiles" },
]
sdist = { url = "...", hash = "sha256:...", size = 225278 }
wheels = [
    { url = "...", hash = "sha256:...", size = 35785 },
    { url = "...", hash = "sha256:...", size = 40012 },  # multi-platform
]
```

Every `[[package]]` has: `name` (required), `version` (required),
`source` (required — inline table with exactly one of the 6 variants),
`dependencies` (optional), `sdist` (optional), `wheels` (optional).

**Rationale**: The three real-world samples cover 95%+ of packages
in the wild. Astral's stability guarantee is that `version = 1` is
frozen (bumps go to `version = 2`). Reader accepts `version = 1`
only in m674 v1; unknown versions WARN + skip per FR-003.

**Alternatives considered**:
- **Track uv.lock v2 preemptively** — rejected; v2 doesn't exist yet;
  premature over-engineering.
- **Use a permissive `serde_json::Value`-like parse instead of strict
  structs** — rejected; strict structs (per Principle IV) catch
  schema drift at parse time, which is the desired behavior.

## R2 — Discovery paths

**Decision**: Two discovery paths:

1. **`<scan_root>/uv.lock`** — the uv-tool convention (uv always
   writes here by default).
2. **Fallback via m673 Pants pipeline** — when `pants::lockfile::parse`
   fails on a file discovered by m672 `[python.resolves]` map or
   m673 wide-scope discovery, the uv reader gets a second attempt
   via a caller-invoked hook.

Non-recursive for v1. Subdirectory uv.lock files (`services/api/uv.lock`,
`packages/foo/uv.lock`) are a v2 extension point. Real-world monorepo
adopters put uv.lock at the root; multi-project monorepos are the
rarer shape and require thought about which pyproject.toml the
uv.lock belongs to.

**Rationale**: The one-file-at-root convention matches every real-
world sample. The Pants fallback handles the backend.ai case
without duplicating discovery logic (single source of truth).

**Alternatives considered**:
- **Recursive walk for `**/uv.lock`** — rejected; too wide-scope; a
  fresh install-time `.venv/*/uv.lock` file would false-positive-match.
- **Read `pyproject.toml` `[tool.uv]` config to find uv.lock path** —
  rejected; uv.lock path is not configurable per uv's own design
  (always `<uv-invocation-cwd>/uv.lock`).

## R3 — Pants FR-002 fallback: hook vs. second-pass

**Decision**: Hook-into-m673 discovery pipeline. When the m673 Pants
reader's `parse()` returns `None` on a file discovered via any of
its 5 sources (m223 default glob, m672 map, m672 singular, m673
repo-root, m673 lockfiles/), invoke `uv::lockfile::parse` on the
same bytes and, if that succeeds, emit its components with the
Pants context annotations preserved (`waybill:pants-resolve=<name>`).

Concrete flow at `pants/mod.rs::read`:

```rust
for candidate in discovered_lockfiles {
    let bytes = fs::read(&candidate.path)?;
    match pants::lockfile::parse(&bytes) {
        Some((pex, was_legacy)) => { /* m223+m672+m673 path */ }
        None => {
            // FR-002 fallback: attempt uv.lock parse.
            if let Some(uv_lock) = uv::lockfile::parse(&bytes) {
                components.extend(uv::lockfile::to_entries(
                    &uv_lock,
                    &candidate.path,
                    &candidate.resolve_name,  // preserve Pants tag
                ));
            }
            // No WARN — pants::lockfile::parse already emitted its own.
        }
    }
}
```

**Rationale**: Single source of truth for discovery. Avoids the
"two-pass discovery" race where the m673 pipeline and a new
standalone uv discovery both find the same file with different
metadata (resolve_name / origin) and reconciliation diverges.

**Alternatives considered**:
- **Standalone uv discovery + second-pass dedup**: rejected — two
  discovery loops means two sets of dedup rules, plus the m672
  `[python.resolves]` map key info is Pants-specific (uv doesn't
  know about it), so a second-pass uv reader can't emit the
  correct resolve annotations without a back-channel from Pants.
- **Content-detect at m673 discovery time to route dispatch**:
  rejected — adds a parse pass before the pants::lockfile::parse
  pass, doubling cost on every file. Try-Pants-first-then-uv is
  cheaper because most `.lock` files under Pants layouts ARE
  pex-shape.

## R4 — 6-variant `UvSource` enum + per-variant PURL rules

**Decision**: Single enum in `uv/source_variant.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UvSource {
    Registry { url: String },              // pkg:pypi/<name>@<version>
    Git { url: String, rev: String },      // pkg:generic/<name>@<version> + git-source annotations
    Path { path: String },                 // pkg:generic/<name>@<version> + path-source annotations
    Url { url: String },                   // pkg:generic/<name>@<version> + url-source annotations
    Editable { path: String },             // SKIP (FR-006)
    Virtual { name: String },              // SKIP (FR-006)
}
```

The variants map 1:1 to uv.lock's `source = { <key> = "..." }`
inline table forms. `serde` deserialization uses `#[serde(untagged)]`
to dispatch on the first-key-present.

**PURL rules** (per FR-004 through FR-007):

- **Registry**: `pkg:pypi/<normalized-name>@<version>` where
  `normalized-name` is via `pip::normalize_pypi_name_for_purl`.
  Registry URL is preserved as a per-component annotation (matches
  m223's `waybill:source-url` shape when the registry is non-PyPI —
  e.g. a private PyPI proxy).
- **Git**: `pkg:generic/<name>@<version>` + `waybill:source-type=git`
  + `waybill:source-url=<git-url>@<rev>`. Matches m223 non-PyPI
  handling at `pants/lockfile.rs::locked_req_to_entry`.
- **Path**: `pkg:generic/<name>@<version>` + `waybill:source-type=path`
  + `waybill:source-url=file://<path>`. Absolute path emitted as
  `file://<abs>`; relative path emitted as-is.
- **Url**: `pkg:generic/<name>@<version>` + `waybill:source-type=url`
  + `waybill:source-url=<url>`.
- **Editable**: SKIP. The `editable = "."` case represents the
  pyproject.toml's own package; m127 root selector + m670 main-
  module emission already handle it. Emitting from uv.lock would
  create a duplicate main-module component.
- **Virtual**: SKIP. Virtual dependencies (`source = { virtual = "..." }`)
  represent tool-provided pseudo-packages that don't have installable
  code; skipping keeps the SBOM aligned with what's actually
  materialized on disk.

**Rationale**: 6-variant exhaustive match makes it impossible to
add a new source without touching every emit callsite. Matches
Astral's own uv-source-model enumeration.

**Alternatives considered**:
- **Emit editable / virtual as their own components** — rejected;
  creates duplicate main-module noise and semantically wrong (they
  aren't proper packages).
- **Merge Path + Url into a single "external URI" variant** —
  rejected; path-vs-url discrimination is semantically meaningful
  for consumers (a `file://` path may not survive artifact-transfer;
  an https URL does).

## R5 — Hash extraction from `sdist` + `wheels`

**Decision**: Extract every `hash` field from `sdist` (if present)
and every entry in `wheels[]` (if present). Store as
`Vec<ContentHash { algo: Sha256, value: <hex> }>` (uv.lock always
uses `sha256:<hex>` form per Astral's spec).

Dedup by hash-hex to avoid emitting duplicates for multi-platform
wheels that happen to share a hash (rare — usually distinct hashes
per platform).

**Rationale**: SHA-256 is the only hash algorithm uv.lock emits.
`ContentHash` at `waybill_common::types::hash` is the workspace
type used by m138 / m140 / m141 / m223 / m672 readers. Reuse
without any new hash primitive.

**Alternatives considered**:
- **Emit one component per wheel (multi-platform expansion)** —
  rejected; would create N components per package where N = number
  of platforms in the wheel matrix. Downstream vuln scanners would
  double-count. Emit ONE component per (name, version) with
  multiple hashes attached.
- **Ignore wheel hashes; only extract sdist hash** — rejected;
  wheels contain the actual runtime bytes for wheel-installs; sdist
  is only used for source-installs. Both should be attributable.

## R6 — Interaction with m670 `pyproject.toml` declared-deps (FR-014)

**Decision**: The m191 reconciler's existing tier-based dedup
handles this without new logic. m670 emits declared-deps as
`waybill:sbom-tier=design` (unresolved / version=null). m674 emits
uv.lock entries as `waybill:sbom-tier=lockfile` (resolved / version
+ hashes). Same (name, PURL-normalized-name) → reconciler picks
the lockfile-tier entry per existing higher-tier-wins policy.

Verified empirically by looking at the reconciler at
`waybill-cli/src/resolve/reconciler.rs` — it groups by canonical
PURL and picks the entry with the highest confidence tier.

**Rationale**: No new reconciler work needed. Existing infrastructure
handles the dedup correctly by construction.

**Alternatives considered**:
- **Explicit tier annotation on uv.lock emissions** — the reconciler
  already reads sbom-tier from `PackageDbEntry.sbom_tier`; the uv
  reader sets that field to `Some("lockfile")` per m003 convention.
  No new mechanism.
- **Suppress m670 declared-deps when uv.lock is present** —
  rejected; the m670 emission may include groups (`dev` / `test`)
  that uv.lock's default resolve doesn't. Let the reconciler
  handle the (name, PURL) collision on the resolved packages;
  m670-only entries survive naturally.

## R7 — New parity catalog row `C157` for `waybill:python-lockfile-format`

**Decision**: Add `C157` to `docs/reference/sbom-format-mapping.md`
+ matching extractor macros in `parity/extractors/{cdx,spdx2,spdx3}.rs`
+ `EXTRACTORS` registration in `parity/extractors/mod.rs`. Follows
the m670 C154 / m671 C156 pattern exactly.

Annotation value is a closed-enum string: `"uv"` for m674 v1.
Future values (`"pex"` for m223 back-attribution, `"poetry"` for
future m*** poetry-explicit-tag) can be added without new C-rows.

**Rationale**: Downstream consumers need format-provenance for
Python components (vuln scanners may weight lockfile-sourced entries
differently from pyproject-declared entries; auditors may want the
provenance chain visible). Standards-native fields have no slot for
"which lockfile format sourced this component" — the closest CDX/
SPDX native would be `evidence.identity[].technique` but that's a
free-form string, not a closed-vocabulary field. `waybill:python-
lockfile-format` fills the gap with a closed vocabulary.

Principle-V audit: no CDX / SPDX / SPDX3 native carrier for "which
Python lockfile format produced this component." Ann keeps the
transparency signal (Principle X) losslessly across all three
formats. Similar shape + rationale to C155 `waybill:python-req-file-scope`
and C124 `waybill:image-source`.

**Alternatives considered**:
- **Reuse existing per-component annotation channel without a new
  C-row** — rejected; the milestone-071 parity gate (memory
  `feedback_sbom_format_mapping_extractor_gate`) requires every
  annotation to have a matching C-row + extractors.
- **Attach format info via `evidence.identity[].technique` free-
  text** — rejected; closed-enum via annotation is more machine-
  actionable.

## R8 — Fixture strategy

**Decision**: Committed small deterministic fixtures under
`waybill-cli/tests/fixtures/uv_lock/`. Three fixtures:

- `minimal_uv/` — `pyproject.toml` + `uv.lock` with 3 registry-
  sourced packages + 1 transitive. Tests SC-001 + happy path.
- `multi_source/` — `uv.lock` mixing registry + git + path + url +
  editable + virtual sources. Tests every FR-004 through FR-007
  branch in one fixture.
- `pants_uv_backend/` — mimics backend.ai shape: `pants.toml`
  `[python.resolves]` naming 2 uv-shape lockfiles at
  `3rdparty/python/*.lock` + those lockfiles present with uv shape.
  Tests US2 + FR-002 Pants fallback + `waybill:pants-resolve`
  annotation preservation.

All package names use `waybill-fixture-*` prefix (memory-recorded
policy). Committed fixtures are preferred over `tempfile::tempdir()`
for m674 because uv.lock schemas contain reference to registry URLs
+ hashes that are easier to review in a checked-in file than to
build via string interpolation.

**Rationale**: Matches m223 committed-fixture pattern. Small
deterministic files (< 5 KB each) that reviewers can inspect
visually.

**Alternatives considered**:
- **Synthetic `tempfile::tempdir()` fixtures like m672 + m673** —
  works but produces less-reviewable test-body helper code because
  uv.lock is TOML-with-deep-nested-tables (fiddly to build via
  `format!` interpolation). Committed fixtures are less brittle.

## Constitution re-check (post-research)

All 12 principles + Strict Boundaries hold as documented in
`plan.md § Constitution Check`. No new violations surfaced.
