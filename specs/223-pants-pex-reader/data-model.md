# Phase 1: Data Model — Pants pex-lockfile reader

**Feature**: 223-pants-pex-reader
**Date**: 2026-07-31

The feature adds 4 new module-private Deserialize types (Pex lockfile
shape) + 1 module-private Deserialize type (`pants.toml` minimal
shape). It reuses existing shared types (`PackageDbEntry`, `Purl`,
`LifecycleScope`, `Hash`) without extension. All new types satisfy
Constitution Principle IV (explicit fields, `#[serde(default)]` for
optional keys, no `serde_json::Value` bag types).

---

## New types (all module-private to `scan_fs::package_db::pants`)

### `PexLockfile` — top-level lockfile shape

Deserialize target for the Pex lockfile JSON root. Only fields waybill
consumes are declared; unknown fields are ignored via serde's default
behavior.

```rust
#[derive(Debug, Deserialize)]
struct PexLockfile {
    /// Pex format version (e.g., "2.10.0"). Used for compatibility
    /// guard: `^2\.` accepted, anything else → WARN + skip.
    pex_version: String,
    /// One or more locked resolves (typically 1; multi-platform locks
    /// can be >1). Every resolve's `locked_requirements` are unioned.
    locked_resolves: Vec<LockedResolve>,
}
```

**Validation rules**:
- `pex_version` MUST match `^2\.` — otherwise the whole file is
  skipped with a WARN diagnostic naming the unsupported version.
- `locked_resolves` MAY be empty — that's an INFO (empty lockfile),
  not a WARN.

### `LockedResolve` — one resolve block inside `locked_resolves`

```rust
#[derive(Debug, Deserialize)]
struct LockedResolve {
    locked_requirements: Vec<LockedRequirement>,
    // Ignored fields: marker, platform_tag (v1 doesn't reason about
    // per-marker or per-platform variants).
}
```

### `LockedRequirement` — one locked distribution

The core payload. Maps 1:1 to an emitted `PackageDbEntry` in the
happy path.

```rust
#[derive(Debug, Deserialize)]
struct LockedRequirement {
    /// PyPI-canonicalized package name (already kebab-case at Pex
    /// generation time, but we re-normalize per R3 for safety).
    project_name: String,
    /// Pinned version string. Missing/empty → skip this entry with
    /// WARN (unpinned entries are FR-002-noncompliant).
    version: String,
    /// PEP 508 requirement strings for inter-package dependencies.
    /// Feeds `PackageDbEntry.depends: Vec<String>` after project-name
    /// extraction (strip version specifiers + markers + extras).
    #[serde(default)]
    requires_dists: Vec<String>,
    /// Python version constraint. Recorded as
    /// `waybill:requires-python` annotation for downstream tooling.
    #[serde(default)]
    requires_python: Option<String>,
    /// One or more artifacts per entry (typically wheel + sdist).
    /// Feeds `PackageDbEntry.hashes`. URL prefix drives PyPI-vs-generic
    /// PURL type dispatch per R1.
    #[serde(default)]
    artifacts: Vec<Artifact>,
}
```

**Validation rules**:
- `project_name` MUST be non-empty.
- `version` MUST be non-empty.
- `artifacts` MAY be empty — but a PyPI-source lockfile always has ≥1
  artifact per entry; empty → treat as generic/no-artifact-hash.
- Empty entries (missing name or version) → WARN + skip that entry
  (do NOT abort the whole lockfile).

### `Artifact` — one downloadable artifact reference

```rust
#[derive(Debug, Deserialize)]
struct Artifact {
    /// Always "sha256" in Pex 2.x. Recorded verbatim into
    /// `PackageDbEntry.hashes[].algorithm`.
    algorithm: String,
    /// Hex-encoded hash. Recorded into `PackageDbEntry.hashes[].value`.
    hash: String,
    /// Fetch URL. Prefix drives PURL type:
    /// - "https://files.pythonhosted.org/" → pkg:pypi/*
    /// - "git+*" → pkg:generic/* with source-type=git
    /// - "http(s)://*" (non-PyPI) → pkg:generic/* with source-type=url
    /// - "file://*" or absolute path → pkg:generic/* with
    ///   source-type=local (path stripped of scan-root prefix for
    ///   FR-009 privacy per spec)
    url: String,
}
```

### `PantsConfig` — minimal `pants.toml` shape (per R4)

Deserialize target for the config file. Only the `[python].lockfile`
key is parsed; every other section ignored. Missing / malformed →
falls back to FR-001 default glob without failing.

```rust
#[derive(Debug, Default, Deserialize)]
struct PantsConfig {
    #[serde(default)]
    python: PythonSection,
}

#[derive(Debug, Default, Deserialize)]
struct PythonSection {
    /// Custom lockfile path override. Interpreted relative to the
    /// scan root. Absent → use FR-001 default glob.
    #[serde(default)]
    lockfile: Option<String>,
}
```

---

## Enums (new, module-private)

### `ArtifactSourceType` — for the FR-009 `waybill:source-type` annotation

```rust
enum ArtifactSourceType {
    /// URL starts with "https://files.pythonhosted.org/"
    Pypi,
    /// URL starts with "git+"
    Git,
    /// URL starts with "http://" or "https://" (non-PyPI)
    Url,
    /// URL starts with "file://" or is an absolute path
    Local,
}

impl ArtifactSourceType {
    fn from_url(url: &str) -> Self { /* per R1 dispatch */ }
    fn as_annotation_str(self) -> &'static str {
        match self {
            Self::Pypi => "pypi",  // present for symmetry; not emitted for pkg:pypi PURLs
            Self::Git => "git",
            Self::Url => "url",
            Self::Local => "local",
        }
    }
}
```

---

## Existing types being reused (no changes)

### `waybill_common::types::purl::Purl`

Reused verbatim. Constructed via `Purl::new("pkg:pypi/<name>@<version>")`
or `Purl::new("pkg:generic/<name>@<version>")` after R3 normalization.
Newtype validates PURL shape at construction — an invalid string
returns `Err` which we handle as a per-entry WARN + skip.

### `waybill_common::resolution::LifecycleScope`

Reused verbatim. Variants used: `Runtime` (default resolve + unknown
names), `Dev` (resolves matching R2 allowlist). No new variants.

### `waybill_common::types::hash::{ContentHash, HashAlgorithm}`

Reused verbatim. Every `Artifact` in a `LockedRequirement` becomes
one `ContentHash` entry with `HashAlgorithm::Sha256` + hex value.

### `PackageDbEntry` (defined in `scan_fs::package_db::mod.rs`)

Reused verbatim. Field mapping from a `LockedRequirement`:

| PackageDbEntry field | Source in LockedRequirement | Notes |
|----------------------|-----------------------------|-------|
| `purl` | `project_name` + `version` + artifact URL prefix | See R1 + R3 for construction rules |
| `name` | `project_name` (normalized) | Matches PURL name segment |
| `version` | `version` | Verbatim |
| `source_path` | Absolute path to the lockfile | Enables source-provenance tracking |
| `depends` | `requires_dists[]` (project names extracted) | See R1 extraction rule |
| `lifecycle_scope` | R2 name-allowlist lookup on resolve filename stem | `Runtime` (default) or `Dev` |
| `sbom_tier` | `Some("source".to_string())` | Lockfile-derived |
| `evidence_kind` | `EvidenceKind::Lockfile` (existing enum variant) | Matches pip reader's usage |
| `hashes` | `artifacts[]` → `ContentHash` list | Every artifact = one hash |
| `licenses` | `Vec::new()` | Pex lockfile format doesn't carry license strings |
| `requirement_ranges` | `Vec::new()` | Pex locks are pinned; no ranges |
| `extra_annotations` | See below | 2–4 keys per entry |

### `extra_annotations` mapping

For each emitted `PackageDbEntry`:

| Annotation key | Value | Presence |
|----------------|-------|----------|
| `waybill:pants-resolve` | Resolve name (e.g., `"default"`, `"mypy"`) | Always present |
| `waybill:requires-python` | `requires_python` verbatim (e.g., `">=3.8"`) | Present iff lockfile records it |
| `waybill:source-url` | `artifacts[0].url` verbatim | Present iff PURL is `pkg:generic/*` (non-PyPI) |
| `waybill:source-type` | `ArtifactSourceType::as_annotation_str` result | Present iff PURL is `pkg:generic/*` |

Each of the 3 new annotation keys (`waybill:pants-resolve`,
`waybill:source-url`, `waybill:source-type`) requires a matching row
in `docs/reference/sbom-format-mapping.md` + a matching extractor
entry in `waybill-cli/src/parity/extractors/mod.rs` per the m071
parity-extractor gate (memory
`feedback_sbom_format_mapping_extractor_gate`). Tasks phase includes
this work.

---

## Discovery + orchestration data flow

```text
┌─────────────────────────┐
│  scan_fs::package_db::  │
│    mod.rs::read_all     │
└───────────┬─────────────┘
            │ calls
            ▼
┌─────────────────────────┐
│  pants::read(scan_root) │
│  (new orchestrator)      │
└───────────┬─────────────┘
            │
            ├─── 1. Read pants.toml (if present)
            │    → PantsConfig (per R4)
            │
            ├─── 2. Enumerate candidate lockfile paths:
            │    a) 3rdparty/python/*.lock (FR-001 default glob)
            │    b) PantsConfig.python.lockfile (if set, FR-004)
            │
            ├─── 3. For each candidate path:
            │      Read + parse via PexLockfile (R1)
            │      On corruption / version-mismatch → WARN + skip
            │
            ├─── 4. For each valid lockfile:
            │      For each LockedRequirement in every LockedResolve:
            │        - Normalize project_name (R3)
            │        - Dispatch PURL type via Artifact URL prefix (R1)
            │        - Classify LifecycleScope via R2 allowlist
            │          keyed on lockfile filename stem
            │        - Build PackageDbEntry with the mapping above
            │
            ├─── 5. Emit FR-010 INFO log:
            │      "pants-pex reader: N lockfiles, M components"
            │
            └─── 6. Return Vec<PackageDbEntry> to read_all
                     (m191 reconciler handles FR-005 dedup at emit time)
```

---

## Storage / persistence

None. All state (parsed lockfiles + intermediate `LockedRequirement`
vectors + final `Vec<PackageDbEntry>`) lives on the stack for the
duration of a single scan. No caches, no databases, no persistent
files written. Matches every language-reader milestone since 002.

---

## Compatibility

- **Existing pip / poetry / uv readers**: unchanged. Coexistence via
  m191 reconciler's PURL-level dedup path. FR-005 covers the specific
  dedup rule; the reconciler infrastructure is already in place.
- **Existing goldens**: unchanged when no Pex lockfiles present per
  FR-007 + SC-003 (feature adds zero cost when unused).
- **Existing types**: `PackageDbEntry`, `Purl`, `LifecycleScope`,
  `ContentHash`, `EvidenceKind` all reused at existing shape — no
  new variants, no field additions, no serde-boundary shifts.
- **CI**: adds no new jobs. Existing `Lint + test (linux-x86_64)` +
  `-macos` + `-windows` lanes cover the new test binary.
