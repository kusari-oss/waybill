# Phase 1 Data Model — m674 uv.lock reader

**Date**: 2026-09-02

## Overview

Four concrete data-shape changes inside `waybill-cli/src/scan_fs/package_db/uv/`:

1. **`UvSource` enum** — 6-variant discriminator per uv.lock source shape (`registry` / `git` / `path` / `url` / `editable` / `virtual`).
2. **`UvLockfile` + `UvPackage` structs** — TOML deserialization targets for the top-level lockfile document + individual `[[package]]` entries.
3. **`UvHashArtifact` struct** — parsed `{ url, hash, size }` inline table used by both `sdist` (Option) and `wheels[]` (Vec).
4. **New `waybill:python-lockfile-format` per-component annotation** (C157) — emitted with value `"uv"` on every m674-sourced component.

No SBOM wire-format changes. Per-component annotations extend the existing per-format extractor infrastructure. In-process state per scan (matches every reader milestone since m002).

---

## Enum 1: `UvSource` — 6-variant discriminator

**File**: `waybill-cli/src/scan_fs/package_db/uv/source_variant.rs`

```rust
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub(crate) enum UvSource {
    Registry { registry: String },
    Git { git: String, rev: String },
    Path { path: String },
    Url { url: String },
    Editable { editable: String },
    Virtual { virtual: String },
}
```

**PURL construction rule per variant** (per FR-004 through FR-007 + research.md §R4):

| Variant | PURL shape | Extra annotations |
|---|---|---|
| `Registry` | `pkg:pypi/<normalized-name>@<version>` | `waybill:pypi-source-url=<registry>` if registry ≠ `https://pypi.org/simple` |
| `Git` | `pkg:generic/<name>@<version>` | `waybill:source-type=git`, `waybill:source-url=<git-url>@<rev>` |
| `Path` | `pkg:generic/<name>@<version>` | `waybill:source-type=path`, `waybill:source-url=file://<path>` |
| `Url` | `pkg:generic/<name>@<version>` | `waybill:source-type=url`, `waybill:source-url=<url>` |
| `Editable` | **SKIP** (return `None` from `to_entry`) | — (handled by m127 root selector + m670 main-module) |
| `Virtual` | **SKIP** (return `None`) | — (virtual packages aren't installable code) |

**Serde deserialization strategy**: `#[serde(untagged)]` dispatches on the first-key-present in the inline `source = { ... }` table. Because each variant's discriminator field name is unique (registry / git / path / url / editable / virtual), the untagged strategy has no ambiguity.

**Validation rules**:
- Every variant MUST have its discriminator field populated with a non-empty string (empty string = malformed → serde parse fails → whole file WARN + skip per m223 fail-open).
- `Git` variant MUST have BOTH `git` and `rev` populated. Missing `rev` → serde parse fails.
- `Path` may be relative or absolute; PURL construction preserves as-observed.

---

## Struct 2: `UvLockfile` — top-level document

**File**: `waybill-cli/src/scan_fs/package_db/uv/lockfile.rs`

```rust
#[derive(Debug, serde::Deserialize)]
pub(crate) struct UvLockfile {
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) revision: Option<u32>,
    #[serde(default)]
    pub(crate) requires_python: Option<String>,
    #[serde(default)]
    pub(crate) resolution_markers: Vec<String>,
    #[serde(default)]
    pub(crate) supported_markers: Vec<String>,
    #[serde(default, rename = "package")]
    pub(crate) packages: Vec<UvPackage>,
    // Freeform tables ignored via serde default behavior:
    // `[options]`, `[manifest]`, and any future top-level tables.
}
```

**Version-gate** (per FR-003): reader MUST reject `version != 1` at m674 v1 scope with a WARN + skip. Implementation:

```rust
if lockfile.version != 1 {
    tracing::warn!(
        version = lockfile.version,
        "uv-lock reader: unsupported uv.lock schema version (expected 1); skipping"
    );
    return None;
}
```

**Field notes**:
- `revision` is Astral's internal patch counter; recorded for provenance but not required.
- `requires_python` is a PEP 440 constraint (e.g. `>=3.10`). m674 does NOT filter emissions by interpreter constraint — v2 extension point.
- `resolution_markers` + `supported_markers` are optional (may not appear on small-dep uv.locks). m674 v1 does NOT filter emissions by marker (per Assumptions in spec.md).
- The `#[serde(rename = "package")]` on `packages` maps the TOML `[[package]]` array-of-tables into a Rust `Vec<UvPackage>`.
- Unknown top-level fields (`[options]`, `[manifest]`, future additions) are silently ignored via serde's default behavior — no `#[serde(deny_unknown_fields)]` (matches m223's format-evolution-tolerance posture).

---

## Struct 3: `UvPackage` — per-package `[[package]]` entry

**File**: `waybill-cli/src/scan_fs/package_db/uv/lockfile.rs`

```rust
#[derive(Debug, serde::Deserialize)]
pub(crate) struct UvPackage {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) source: UvSource,
    #[serde(default)]
    pub(crate) dependencies: Vec<UvDependency>,
    #[serde(default)]
    pub(crate) sdist: Option<UvHashArtifact>,
    #[serde(default)]
    pub(crate) wheels: Vec<UvHashArtifact>,
    // Freeform metadata fields ignored (metadata, resolution-markers, etc.).
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct UvDependency {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) extra: Vec<String>,
    // Freeform fields ignored (marker, etc.).
}
```

**Validation rules** (all enforced by serde's strict-parse behavior):
- `name` MUST be a non-empty string.
- `version` MUST be a non-empty string. m674 does NOT validate PEP 440 version syntax — trusts uv's own emission.
- `source` MUST be a valid `UvSource` variant. Missing / unknown discriminator field → parse fails → whole file WARN + skip.
- `dependencies` may be absent (empty array default).
- `sdist` may be absent (Option::None default).
- `wheels` may be absent (empty array default).

**Emission mapping** (`to_entry` → `Option<PackageDbEntry>`, per FR-004 through FR-011):
- Return `None` iff `source` is `Editable` or `Virtual` (FR-006 skip).
- Otherwise, construct `PackageDbEntry` with:
  - PURL per `UvSource` variant table (§ Enum 1 above).
  - `sbom_tier = Some("lockfile")` (m003 convention — feeds m191 reconciler).
  - `hashes` from `sdist.hash` + every `wheels[*].hash`, deduped by hex-value.
  - Per-component annotations:
    - `waybill:python-lockfile-format = "uv"` (FR-011, C157 new).
    - `waybill:source-files = "<uv.lock-path>"` (FR-009).
    - `waybill:pants-resolve = <name>` iff called via Pants FR-002 fallback (research.md §R3 — the caller passes the resolve name; standalone uv path passes None).
    - Non-registry variants also carry `waybill:source-type` + `waybill:source-url` per § Enum 1 table.

---

## Struct 4: `UvHashArtifact` — shared `{ url, hash, size }` inline table

**File**: `waybill-cli/src/scan_fs/package_db/uv/lockfile.rs`

```rust
#[derive(Debug, serde::Deserialize)]
pub(crate) struct UvHashArtifact {
    pub(crate) url: String,
    pub(crate) hash: String,   // "sha256:<hex>" form
    #[serde(default)]
    pub(crate) size: Option<u64>,
    // Freeform metadata (upload-time, etc.) ignored.
}
```

Used by BOTH `UvPackage.sdist: Option<UvHashArtifact>` AND `UvPackage.wheels: Vec<UvHashArtifact>`. Same shape everywhere uv.lock emits it (per research.md §R1 empirical verification).

**Hash parsing rule** (per FR-008): the `hash` field is `"sha256:<64-hex>"`. Reader splits on `:`; iff prefix is `"sha256"` AND suffix is 64 hex chars, construct `ContentHash { algo: Sha256, value: <hex> }`. Any other prefix (uv doesn't emit MD5 / SHA-1 in current v1 but future guardrail) → skip that hash entry with a TRACE log (not WARN — hash-algorithm drift is not corruption).

---

## New parity catalog row: C157 `waybill:python-lockfile-format`

**File**: `docs/reference/sbom-format-mapping.md`

Add row after m671's C156:

| C-row | Field name | Value shape | Directionality | Emitted-when |
|---|---|---|---|---|
| **C157** | `waybill:python-lockfile-format` | Closed-enum string. v1: `"uv"`. Future: `"pex"` for m223 back-attribution, `"poetry"` / `"pipenv"` for future format-explicit tagging. | `SymmetricEqual` | Iff a Python component was sourced from a lockfile whose format the reader identifies. Absent on m670 pyproject.toml-declared-only components (they're design-tier, no lockfile). |

Register in `parity/extractors/mod.rs::EXTRACTORS` + add 3 macro invocations (`c157_cdx`, `c157_spdx23`, `c157_spdx3`) at the corresponding extractor files. Component-scope. Per the m670 C154 / m671 C156 registration pattern.

**Standards-native audit** (Principle V):

- CDX has no dedicated slot for "which lockfile format produced this component" — `evidence.identity[].technique` is closest but it's a free-form string, not closed-vocabulary.
- SPDX 2.3 has no equivalent field on `Package`.
- SPDX 3 has no equivalent on `software_Package`.
- **KEEP-NO-NATIVE**: closed-vocabulary annotation is the machine-actionable choice.

---

## Data-flow diagram

```
                    ┌──────────────────────────────┐
                    │      discovery entry-points  │
                    └──────────────────────────────┘
                                    │
              ┌─────────────────────┴─────────────────────┐
              │                                            │
              ▼                                            ▼
    <scan_root>/uv.lock                       (Pants FR-002 fallback)
    (standalone uv projects)                  m673 pipeline invokes
              │                                uv::lockfile::parse when
              │                                pants::lockfile::parse
              │                                returns None
              │                                            │
              └──────────────────────┬─────────────────────┘
                                     │
                                     ▼
                       ┌──────────────────────────┐
                       │  uv::lockfile::parse     │
                       │  → Option<UvLockfile>    │
                       │  (WARN + None on         │
                       │   version != 1 OR        │
                       │   serde-parse failure)   │
                       └──────────────────────────┘
                                     │
                                     ▼
                     for each UvPackage in .packages:
                                     │
                       ┌─────────────┴────────────┐
                       ▼                          ▼
              UvSource::Editable          all other UvSource variants
              UvSource::Virtual                    │
                       │                          ▼
                       ▼                match on variant → build PURL
                     SKIP               + collect hashes from sdist +
                    (FR-006)            wheels + emit annotations
                                                  │
                                                  ▼
                                        PackageDbEntry
                                        {
                                          purl: <per-variant>,
                                          sbom_tier: Some("lockfile"),
                                          hashes: [<sha256s>...],
                                          annotations: [
                                            waybill:python-lockfile-format=uv,
                                            waybill:source-files=<path>,
                                            waybill:pants-resolve=<name> (if via Pants),
                                            waybill:source-type=... (non-registry),
                                            waybill:source-url=... (non-registry),
                                          ],
                                        }
                                                  │
                                                  ▼
                                        (m191 reconciler dedup with m670
                                         pyproject-declared entries via
                                         higher-tier-wins policy)
```

---

## Non-goals

- **No new `Serialize` impls** on any struct — deserialize-only.
- **No new public API** — everything is `pub(crate)`; the reader is invoked via the existing `read_all` dispatcher.
- **No recursive discovery** — v1 only walks `<scan_root>/uv.lock` (FR-002 fallback re-uses m673 pipeline; no independent recursion).
- **No marker-based filtering** — every locked package emits regardless of `requires-python` / `resolution-markers` / dep-group membership (v2 extension point per spec Assumptions).
- **No emit for `editable` / `virtual` sources** — handled by m127 root selector + m670 main-module.
- **No wheel-per-platform expansion** — one component per (name, version) with multiple hashes attached.
