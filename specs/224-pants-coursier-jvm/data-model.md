# Phase 1: Data Model — Pants coursier JVM lockfile reader

**Feature**: 224-pants-coursier-jvm
**Date**: 2026-08-01

The feature adds 5 new module-private Deserialize types (coursier
lockfile shape) + 2 module-private Deserialize types (`pants.toml`
minimal shape) + 1 pure `Coordinate` struct (coord-string parse
result). Reuses existing `PackageDbEntry`, `Purl`, `LifecycleScope`,
`ContentHash` verbatim. All new types satisfy Constitution Principle IV.

---

## New types (all module-private to `scan_fs::package_db::pants_jvm`)

### `CoursierLockfile` — top-level lockfile shape

Deserialize target for the TOML body (after the metadata header is
stripped). Only fields waybill consumes are declared; unknown fields
are ignored via serde's default behavior.

```rust
#[derive(Debug, Deserialize)]
struct CoursierLockfile {
    /// Zero or more locked distributions. May be empty for a
    /// partially-generated lockfile (INFO, not WARN).
    #[serde(default)]
    entries: Vec<Entry>,
}
```

### `Entry` — one locked distribution

Maps 1:1 to an emitted `PackageDbEntry` in the happy path.

```rust
#[derive(Debug, Deserialize)]
struct Entry {
    /// PEP 508-like coordinate strings for direct declared deps.
    /// Empty in many lockfile snapshots (coursier prunes to leaves).
    #[serde(default, rename = "directDependencies")]
    direct_dependencies: Vec<String>,
    /// PEP 508-like coordinate strings for transitive resolved deps.
    /// This is the ground truth for the dependency graph.
    #[serde(default)]
    dependencies: Vec<String>,
    /// The artifact filename (e.g., "guava-31.0.1-jre.jar"). Recorded
    /// as `waybill:file-name` annotation for diagnostic value only —
    /// no downstream logic depends on it.
    #[serde(default)]
    file_name: Option<String>,
    /// The Maven coordinate triple + optional classifier + packaging.
    coord: EntryCoord,
    /// Optional artifact hash. Absent when the artifact was resolved
    /// but not downloaded (rare — some `url=not_provided` scenarios).
    #[serde(default)]
    file_digest: Option<EntryFileDigest>,
}
```

### `EntryCoord` — Maven coordinate + optional qualifiers

```rust
#[derive(Debug, Deserialize)]
struct EntryCoord {
    group: String,
    artifact: String,
    version: String,
    /// "jar" (Maven default), "war", "pom", "aar" (Android), etc.
    /// Emitted as PURL `?type=<value>` qualifier when != "jar".
    #[serde(default)]
    packaging: Option<String>,
    /// Optional Maven classifier ("sources", "javadoc", platform tags
    /// like "linux-x86_64"). Emitted as PURL `?classifier=<value>`
    /// qualifier when present + non-empty.
    #[serde(default)]
    classifier: Option<String>,
    /// Optional fetch URL. When present, emitted as `waybill:source-url`
    /// annotation (reuses m223 C144 catalog row).
    #[serde(default)]
    url: Option<String>,
}
```

### `EntryFileDigest` — sha256 fingerprint

```rust
#[derive(Debug, Deserialize)]
struct EntryFileDigest {
    /// Hex-encoded sha256. Recorded as `ContentHash` on the
    /// PackageDbEntry via `HashAlgorithm::Sha256`.
    fingerprint: String,
    /// Byte length of the serialized artifact. Recorded as
    /// `waybill:artifact-byte-length` annotation for diagnostic.
    /// Optional — not every entry has it.
    #[serde(default)]
    serialized_bytes_length: Option<u64>,
}
```

### `PantsMetadata` — header comment block schema

Parsed from the `# --- BEGIN PANTS LOCKFILE METADATA` comment block
(strip-`# `-and-concat → JSON parse). Only two fields matter to
waybill's validation gate.

```rust
#[derive(Debug, Deserialize)]
struct PantsMetadata {
    /// Semantic version of the metadata block schema. Only `1` is
    /// supported; other values → WARN + skip the whole file.
    version: u32,
    /// Original coordinate strings the operator passed to `pants
    /// generate-lockfiles`. Diagnostic; not extracted into components.
    #[serde(default)]
    generated_with_requirements: Vec<String>,
}
```

### `PantsConfig` + `JvmSection` — minimal `pants.toml` shape (per R4)

```rust
#[derive(Debug, Default, Deserialize)]
struct PantsConfig {
    #[serde(default)]
    jvm: JvmSection,
}

#[derive(Debug, Default, Deserialize)]
struct JvmSection {
    /// Default resolve name; when absent, waybill uses the filename
    /// stem of each discovered lockfile.
    #[serde(default)]
    default_resolve: Option<String>,
    /// Map of resolve name → lockfile path (relative to scan root).
    /// Empty when the operator relies on the default glob only.
    #[serde(default)]
    resolves: std::collections::HashMap<String, String>,
}
```

### `Coordinate` — coord-string parse result (R2)

```rust
pub(crate) struct Coordinate {
    pub group: String,
    pub artifact: String,
    pub version: String,
}
```

No `Deserialize` — this is the runtime result of parsing a
`dependencies[]` / `directDependencies[]` string via
`coordinate::parse_coord_string(s: &str) -> Option<Coordinate>`.

---

## Existing types being reused (no changes)

### `waybill_common::types::purl::Purl`

Reused verbatim. Constructed via `Purl::new(&purl_str)` after inline
PURL string construction per R3 option B.

### `waybill_common::resolution::LifecycleScope`

Reused verbatim. Variants used: `Runtime` (default resolve + unknown
allowlist names), `Development` (JVM-dev-tool allowlist matches).

### `waybill_common::types::hash::{ContentHash, HashAlgorithm}`

Reused verbatim. Each `EntryFileDigest.fingerprint` becomes one
`ContentHash::sha256(hex)` entry.

### `PackageDbEntry` (defined in `scan_fs::package_db::mod.rs`)

Reused verbatim. Field mapping from one `Entry`:

| PackageDbEntry field | Source in Entry | Notes |
|----------------------|-----------------|-------|
| `purl` | `EntryCoord` → `pkg:maven/<group>/<artifact>@<version>[?classifier=...&type=...]` | Per R3 option B inline construction |
| `name` | `EntryCoord.artifact` | Matches Maven reader convention (artifact-id, not group-id) |
| `version` | `EntryCoord.version` | Verbatim |
| `source_path` | Absolute path to the lockfile | Enables source-provenance tracking |
| `depends` | `Entry.dependencies[]` parsed via R2 → `<group>:<artifact>:<version>` normalized strings | Same graph-edge format Maven reader emits |
| `lifecycle_scope` | R4-derived resolve name → R4 JVM-dev-tool allowlist lookup | `Runtime` (default + unknown), `Development` (allowlisted) |
| `sbom_tier` | `Some("source".to_string())` | Lockfile-derived |
| `evidence_kind` | Matches m223's `None` posture (pants readers don't set — no per-reader vocab entry in the closed enum). See maven-file entry pattern. | `None` for v1 |
| `hashes` | `EntryFileDigest.fingerprint` → 1 `ContentHash::sha256` | Empty vec if `file_digest` absent |
| `licenses` | `Vec::new()` | Coursier lockfile format doesn't carry licenses |
| `requirement_ranges` | `Vec::new()` | Coursier locks are pinned; no ranges |
| `extra_annotations` | See below | 2–4 keys per entry |
| All other fields | `None` / default | Match m223 posture |

### `extra_annotations` mapping

**Always present**:

| Key | Value | Catalog row |
|-----|-------|-------------|
| `waybill:pants-resolve` | Resolve name (e.g., `"default"`, `"junit"`, `"scalatest"`) | C143 (shipped in m223) |

**Present iff data available**:

| Key | Value | Condition | Catalog row |
|-----|-------|-----------|-------------|
| `waybill:source-url` | `EntryCoord.url` verbatim | Present iff non-null non-empty | C144 (shipped in m223) |
| `waybill:file-name` | `Entry.file_name` verbatim (e.g., `"guava-31.0.1-jre.jar"`) | Present iff non-null; diagnostic | **NEW — needs catalog row?** See below |
| `waybill:artifact-byte-length` | `EntryFileDigest.serialized_bytes_length` as u64 stringified | Present iff `file_digest` + inner field non-null | **NEW — needs catalog row?** See below |

**Decision on `waybill:file-name` + `waybill:artifact-byte-length`**:
these are diagnostic-only signals (help operators reconcile SBOM
contents against on-disk coursier cache). They don't affect the
component's identity, hashes, or dep graph.

**Two options**:
- **Emit them + add catalog rows C145 + C146** — matches m071 parity
  gate. Adds ~30 LOC of parity work.
- **Don't emit them** — keep the diagnostic value in the WARN /
  INFO log stream at scan time only, not in the persistent SBOM.

**Chosen**: **Don't emit them in v1**. Rationale:
- The v1 spec explicitly says "zero new parity work vs m223" as a
  differentiator; adding 2 rows undoes that saving.
- Diagnostic-only signals are better delivered via `RUST_LOG=debug`
  than SBOM annotations (SBOM consumers rarely need per-artifact byte
  lengths for security analysis).
- If operators request the signals later, adding C145 + C146 is a
  cheap follow-up (this is why the reader stores them in a
  module-local structure — code can be augmented without schema
  breakage).

The Entry-side fields (`file_name`, `serialized_bytes_length`) remain
Deserialized (already-in-the-struct cost is zero) but the emit path
discards them. Follow-up spec can wire them into annotations if
demand emerges.

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
│  pants_jvm::read(root)  │
└───────────┬─────────────┘
            │
            ├─── 1. Read pants.toml (if present)
            │    → PantsConfig (per R4)
            │
            ├─── 2. Enumerate candidate lockfile paths:
            │    a) 3rdparty/jvm/*.lock (FR-001 default glob)
            │    b) PantsConfig.jvm.resolves[] (FR-004)
            │
            ├─── 3. For each candidate:
            │      a) Header discriminator scan (FR-011):
            │         match "# --- BEGIN PANTS LOCKFILE METADATA"
            │         Missing → INFO + skip
            │      b) Parse metadata JSON, verify version == 1
            │         Mismatch → WARN + skip
            │      c) Strip header comment block, parse TOML body
            │         via CoursierLockfile
            │         Parse error → WARN + skip
            │
            ├─── 4. For each Entry in every CoursierLockfile:
            │      - Construct PURL via R3 option B (inline)
            │      - Extract sha256 from file_digest (if present)
            │      - Parse dependencies[] coord strings via R2
            │      - Classify LifecycleScope via R4-derived resolve name
            │      - Build PackageDbEntry with the mapping above
            │
            ├─── 5. Emit FR-010 INFO log:
            │      "pants-coursier-jvm reader complete
            │         lockfiles_discovered=<N> lockfiles_parsed_ok=<N>
            │         lockfiles_skipped_corrupt=<N>
            │         lockfiles_skipped_non_pants=<N>
            │         components_emitted=<N>"
            │
            └─── 6. Return Vec<PackageDbEntry> to read_all
                     (m191 reconciler handles FR-005 dedup at emit time
                      against the existing Maven reader)
```

**Note**: FR-010 log adds one field vs m223 (`lockfiles_skipped_non_pants`)
because FR-011 introduces a new skip class not present in m223 (pex
lockfiles are always Pants-generated in practice; coursier lockfiles
can exist standalone).

---

## Storage / persistence

None. All state (parsed lockfiles + intermediate `Entry` vectors +
final `Vec<PackageDbEntry>`) lives on the stack for the duration of a
single scan. Matches m223 + every language-reader milestone since 002.

---

## Compatibility

- **Existing Maven reader**: unchanged. Coexistence via m191
  reconciler's PURL-level dedup path. FR-005 covers dedup rules.
- **Existing pants-python reader (m223)**: independent module
  (`pants/` vs `pants_jvm/`). Both discover `pants.toml` but read
  different top-level sections (`[python]` vs `[jvm]`). Two separate
  `PantsConfig` types in two separate `config.rs` files by design
  (see R4 note).
- **Existing goldens**: unchanged when no coursier lockfiles present
  (FR-007 + SC-003 enforce this).
- **CI**: adds no new jobs. Existing lint+test lanes cover the new
  test binary.
- **Parity catalog**: reuses C143 + C144 verbatim; no new rows,
  extractors, or `_anno!` entries.
