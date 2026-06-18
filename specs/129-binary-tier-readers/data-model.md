# Data Model: Image-tier binary-extracted package readers (milestone 129)

Four new in-memory entities. None persist beyond a single scan invocation (matching every milestone
since 002).

## Entity 1: `DotnetDepsJsonDocument`

Parsed representation of a single `.deps.json` file.

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct DotnetDepsJsonDocument {
    #[serde(rename = "runtimeTarget")]
    pub runtime_target: Option<RuntimeTarget>,
    pub libraries: BTreeMap<DepsJsonKey, LibraryEntry>,
    // Other top-level keys (`targets`, `compilationOptions`) are deserialized
    // into `serde_json::Value`-typed catch-alls for Phase 11 forward-compat.
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RuntimeTarget {
    pub name: String,            // e.g. ".NETCoreApp,Version=v8.0"
    pub signature: Option<String>,
}

/// Key shape in the `libraries` map: `"{name}/{version}"`.
/// Custom (De)serialize so we can fail-fast on malformed keys.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DepsJsonKey {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LibraryEntry {
    #[serde(rename = "type")]
    pub ty: LibraryType,
    pub serviceable: Option<bool>,
    pub sha512: Option<String>,
    pub path: Option<String>,
    pub hash_path: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LibraryType {
    Package,           // → emit pkg:nuget component (FR-008)
    Project,           // → SKIP — first-party assembly (FR-009)
    Referenceassembly, // → SKIP — reference-only assembly (no runtime semantics)
    #[serde(other)]
    Unknown,           // → SKIP + warn-level log
}
```

**Validation rules** (per FR-007..009):

- `libraries` map key MUST match the regex `^[^/]+/[^/]+$`. Malformed keys → log `warn` + skip entry.
- `LibraryType::Package` entries emit a `pkg:nuget/<name>@<version>` component.
- `LibraryType::Project` entries are SKIPPED.
- `LibraryType::Referenceassembly` entries are SKIPPED (reference assemblies are compile-time-only).
- `LibraryType::Unknown` (any other string) → log `warn` + skip.

**`From<&DepsJsonKey> for Purl`**: constructs `pkg:nuget/<urlencoded name>@<urlencoded version>`.

**Output**: each emitted component carries:

- `mikebom:sbom-tier = "image"` (FR-001)
- `mikebom:source-mechanism = "dotnet-deps-json"` (FR-002)
- `mikebom:source-files = "<path-to-.deps.json>"` (existing convention)
- `mikebom:cpe-candidates = "<derived>"` (FR-013)
- Optional `mikebom:image-presence = "declared-not-installed"` if the resolver can't find the assembly
  file at the path declared by `LibraryEntry.path` (edge case in spec).

---

## Entity 2: `ManagedPeAssembly`

Parsed representation of a single `.dll` file that has a CLR header.

```rust
#[derive(Debug, Clone)]
pub(crate) struct ManagedPeAssembly {
    pub assembly_name: String,                     // From `Assembly` table (token 0x20), Name column
    pub assembly_version: Version4Tuple,           // (major, minor, build, revision)
    pub assembly_file_version: Option<String>,     // From AssemblyFileVersionAttribute custom-attribute
    pub assembly_informational_version: Option<String>, // From AssemblyInformationalVersionAttribute
    pub culture: Option<String>,                   // Usually "neutral"
    pub public_key_token: Option<[u8; 8]>,         // Hex-rendered into annotation
    pub source_path: PathBuf,                      // Where the .dll lives in the rootfs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Version4Tuple {
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub revision: u16,
}

impl ManagedPeAssembly {
    /// Per FR-010 + clarification Q3: pick PURL version via fallback ladder.
    pub fn purl_version(&self) -> String {
        if let Some(v) = self.assembly_informational_version.as_ref() {
            return v.clone();
        }
        if let Some(v) = self.assembly_file_version.as_ref() {
            return v.clone();
        }
        format!("{}.{}.{}.{}",
            self.assembly_version.major,
            self.assembly_version.minor,
            self.assembly_version.build,
            self.assembly_version.revision,
        )
    }
}
```

**Validation rules** (per FR-010..011):

- `is_managed_assembly()` returns `true` iff `DataDirectory[14].VirtualAddress != 0 && Size != 0`.
- `assembly_name` MUST be a valid UTF-8 string from `#Strings` heap; if not, the assembly is SKIPPED
  with a `warn` log.
- The `purl_version()` fallback ladder ensures every emitted component carries a non-empty version.
- Per FR-011: if a `.deps.json` declaration covers the same `(name, version)` combination IN THE SAME
  IMAGE, the PE-derived component is SUPPRESSED by the milestone-105 dedup pipeline (the
  `.deps.json` carries higher-fidelity version data).

**Output**: each emitted component carries:

- `mikebom:sbom-tier = "image"`
- `mikebom:source-mechanism = "dotnet-assembly-metadata"`
- `mikebom:source-files = "<path-to-.dll>"`
- `mikebom:assembly-version-informational = "<value>"` (if present)
- `mikebom:assembly-version-file = "<value>"` (if present)
- `mikebom:assembly-version-runtime = "<4-tuple>"` (always present)
- `mikebom:cpe-candidates = "<derived>"`

---

## Entity 3: `CargoAuditablePayload`

Parsed representation of a `.dep-v0` ELF section's deflate-decompressed JSON payload.

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct CargoAuditablePayload {
    pub packages: Vec<CargoAuditablePackage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoAuditablePackage {
    pub name: String,
    pub version: String,                // cargo-auditable stores full semver string here
    pub source: CargoAuditableSource,
    #[serde(default)]
    pub kind: CargoAuditableKind,
    #[serde(default)]
    pub dependencies: Vec<usize>,       // Indices into `packages` (for the intra-binary graph)
    #[serde(default)]
    pub root: bool,                     // The application's own crate
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CargoAuditableSource {
    Local,             // path dep; FR-018
    CratesIo,          // default registry; serde alias "crates-io"
    Git,               // git+https://...#<sha>; FR-016 still emits but tags source-mechanism
    Unknown,
    // The wire format also serializes the URL inline for git/registry; we
    // store the variant only and recover the raw string from a sibling field
    // if needed for OSV-direct-match heuristics (Phase 11 forward-look).
}

#[derive(Debug, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CargoAuditableKind {
    #[default]
    Runtime,
    Build,
    Dev,
}

impl CargoAuditableKind {
    /// Per clarification Q1 + milestone 052: `kind` → `lifecycle_scope`.
    pub fn into_lifecycle_scope(self) -> LifecycleScope {
        match self {
            Self::Runtime => LifecycleScope::Runtime,
            Self::Build => LifecycleScope::Build,
            Self::Dev => LifecycleScope::Test, // matches milestone-052 source-tier mapping
        }
    }
}
```

**Validation rules** (per FR-014..020):

- The `.dep-v0` section's bytes MUST decompress as a raw deflate stream (no gzip frame, no zlib header).
  Decompression failure → log `warn` + skip binary.
- The deflated payload MUST parse as `CargoAuditablePayload` JSON. Parse failure → log `warn` + skip
  binary.
- Per FR-016: emit components for every package; `root: true` packages emit as the binary's main module
  (not a transitive dep). `kind`-derived scope flows into `ResolvedComponent.lifecycle_scope`.
- Per FR-018: `source: Local` packages emit with `mikebom:cargo-source-mechanism = "local-path"`
  annotation so downstream tools can suppress them from external-dep lists.
- Per FR-019: handles `EM_X86_64`, `EM_AARCH64`, `EM_ARM`, `EM_RISCV` ELF binaries. The `object` crate's
  cross-arch handling is automatic.

**Output**: each emitted component carries:

- `mikebom:sbom-tier = "image"`
- `mikebom:source-mechanism = "cargo-auditable-binary"`
- `mikebom:source-files = "<path-to-elf>"`
- `mikebom:cpe-candidates = "<derived>"`
- Native CDX `scope` / SPDX typed relationships per `lifecycle_scope` — NO `mikebom:lifecycle-scope`
  annotation (per Principle V audit; plan corrects spec FR-017 phrasing).
- Optional `mikebom:cargo-source-mechanism = "local-path"` for `CargoAuditableSource::Local`.

---

## Entity 4: `NestedArchive`

In-memory recursion state for the milestone-009 maven JAR reader's new nested-archive descent.

```rust
pub(crate) struct NestedArchiveWalker {
    /// Visited-set keyed on SHA-256 of each archive's bytes.
    /// Cycle protection — milestone-128 convention.
    visited: HashSet<[u8; 32]>,
    /// Current recursion depth; bounded at 8 per FR-021.
    depth: u8,
    /// Per-archive decompressed-size cap; 1 GB per FR-025.
    size_cap: u64,
    /// Emitter for newly-discovered nested components.
    out: Vec<PackageDbEntry>,
    /// Path of the OUTERMOST archive (for parse-failure annotations).
    outer_path: PathBuf,
}

impl NestedArchiveWalker {
    pub fn walk(&mut self, archive_bytes: &[u8]) {
        let sha = sha256_of(archive_bytes);
        if !self.visited.insert(sha) {
            return; // cycle detected; milestone-128 pattern
        }
        if self.depth >= 8 {
            tracing::warn!(
                outer = %self.outer_path.display(),
                "nested-archive depth limit (8) reached; further nesting skipped"
            );
            return;
        }
        // Open the archive (`zip::ZipArchive::new(Cursor::new(archive_bytes))`),
        // iterate entries, for each pom.properties: emit a pkg:maven component;
        // for each nested .jar/.war/.ear entry: extract bytes, increment depth,
        // recursively call self.walk(&inner_bytes).
        // (Implementation per FR-022..026; see contracts/reader-behavior.md.)
    }
}
```

**Validation rules** (per FR-021..026):

- Depth 8 levels (FR-021).
- Extension filter: `.jar`, `.war`, `.ear` only (FR-022, clarification Q2). `.zip` excluded.
- SHA-256 cycle detection (FR-024) — pathological self-referencing inputs return immediately.
- 1 GB per-nested-archive size cap (FR-025) — entries declaring >1 GB uncompressed are SKIPPED with
  a `warn` log; never extracted into memory.
- The OUTER `.jar` reader (milestone 009 top-level path) is unchanged; only the recursive helper is new.

**Output**: each emitted nested-JAR component carries:

- `mikebom:sbom-tier = "image"`
- `mikebom:source-mechanism = "maven-jar-nested"` (distinguishes from top-level `"maven-jar"`)
- `mikebom:source-files = "<outer-jar-path>!<nested-path>!<deeper-nested-path>..."` (`!` separator
  matches the JAR-URL convention, e.g. `app.jar!BOOT-INF/lib/dep.jar!META-INF/maven/...`)
- `mikebom:cpe-candidates = "<derived>"`
- Existing milestone-009 fields (`license`, `evidence`, etc.) unchanged for nested entries.

---

## Cross-entity dedup pipeline (milestone 105 reuse)

When the same `(purl-type, name, version)` is detected by multiple readers in a single scan, the existing
milestone-105 dedup pipeline at `mikebom-cli/src/scan_fs/dedup.rs` merges them into ONE
`ResolvedComponent` with a `mikebom:also-detected-via` annotation listing all source-mechanism variants
in sorted order. No new code is needed in dedup.rs for milestone 129 — adding the 4 new
`SourceMechanism` enum variants (`DotnetDepsJson`, `DotnetAssemblyMetadata`, `CargoAuditableBinary`,
`MavenJarNested`) is purely additive and the dedup logic handles them by-value.

## State transitions

None. All four entities are constructed once per file/section parse and then consumed by the
`PackageDbEntry` → `ResolvedComponent` conversion. The lifecycle is:

```text
File on disk
  → entity (parsed)
    → Vec<PackageDbEntry>
      → ResolvedComponent (via existing milestone-105 dedup)
        → CDX/SPDX2.3/SPDX3 emission (via existing format builders)
          → SBOM bytes on disk
```

The four entities have no mutable state after construction. The `NestedArchiveWalker` IS mutable during
the recursion but is discarded once the outer JAR's processing is complete.
