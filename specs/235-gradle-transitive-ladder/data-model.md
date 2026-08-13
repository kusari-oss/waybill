# Phase 1 Data Model: Gradle Transitive Dependency Resolution Ladder

**Feature**: `235-gradle-transitive-ladder`
**Date**: 2026-08-13

All types are in-process per-scan; nothing persists. Domain types
follow Constitution Principle IV (newtypes / enums; no raw `String`
across function boundaries for domain values).

---

## Enum: `GradleResolutionTier`

**File**: `waybill-cli/src/scan_fs/package_db/gradle/tier.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GradleResolutionTier {
    /// US1 — spawned `./gradlew :sub:dependencies --no-daemon`
    /// and parsed the ASCII tree. Highest accuracy; same view Gradle has.
    Subprocess,
    /// US2 — walked `${GRADLE_USER_HOME}/caches/modules-2/` +
    /// reconstructed graph from cached POMs / .module files.
    /// Full graph frozen at last-build state.
    Cache,
    /// US3 — regex-scoped extraction from `build.gradle(.kts)` +
    /// version catalog + settings.gradle. Direct deps only.
    Static,
    /// m106 legacy path — read `gradle.lockfile` / `buildscript-gradle.lockfile`
    /// but ladder didn't fire (no wrapper, no cache, no source files).
    /// Flat list; no transitive edges.
    LockfileOnly,
}

impl GradleResolutionTier {
    pub fn as_annotation_str(&self) -> &'static str { ... }
}
```

Field types + rationale: `Copy` because it's a small enum used
pervasively. `Hash` for use as a `HashMap` key in the aggregate.
`Serialize` for annotation emission.

---

## Enum: `GradleFallbackReason`

**File**: same as above.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GradleFallbackReason {
    Timeout,          // Subprocess killed at timeout
    MissingTool,      // `./gradlew` / `gradle` not on PATH or no JDK
    ParseError,       // Subprocess output couldn't be parsed
    CacheMiss,        // US2 cache didn't contain declared deps
    NoSourceFiles,    // US3 couldn't find build.gradle(.kts) to parse
    OperatorOptOut,   // --gradle-resolve was NOT passed
    SubprocessError,  // Subprocess exited non-zero (build script broken)
}
```

---

## Struct: `GradleResolvedGraph`

**File**: `waybill-cli/src/scan_fs/package_db/gradle/ladder.rs`

```rust
#[derive(Debug, Clone)]
pub struct GradleResolvedGraph {
    pub components: Vec<PackageDbEntry>,      // Emitted components
    pub edges: Vec<(Purl, Purl, EdgeScope)>,  // (parent, child, scope)
    pub tier: GradleResolutionTier,
    pub fallback_history: Vec<(GradleResolutionTier, GradleFallbackReason)>,
}
```

`fallback_history` records every tier that was tried and failed
before the winning tier; used by the transparency annotation.

`EdgeScope` is a small enum (`Runtime`, `Test`, `Buildscript`, `Optional`)
that maps to the existing `LifecycleScope` for CDX/SPDX emission.

---

## Struct: `SubprojectRoot`

**File**: `waybill-cli/src/scan_fs/package_db/gradle/ladder.rs`

```rust
#[derive(Debug, Clone)]
pub struct SubprojectRoot {
    pub name: String,          // e.g., "app", "core", "shared"
    pub path: PathBuf,         // e.g., ./app/
    pub build_file: PathBuf,   // build.gradle or build.gradle.kts
    pub graph: GradleResolvedGraph,
}
```

One per subproject enumerated from `settings.gradle(.kts)`
`include(...)` lines (US3) or from the subprocess enumeration
(US1's initial `./gradlew projects` call).

---

## Struct: `GradleScanSummary`

**File**: `waybill-cli/src/scan_fs/package_db/gradle/ladder.rs`

```rust
#[derive(Debug)]
pub struct GradleScanSummary {
    pub subprojects: Vec<SubprojectRoot>,
    pub aggregate_tier: GradleResolutionTier,  // "mixed" is not here — see below
    pub aggregate_mixed: bool,                 // true if subprojects differ
}
```

The `aggregate_tier` field holds one of the four base tiers; when
`aggregate_mixed == true`, the annotation writer emits `"mixed"` and
walks the subprojects to emit per-subproject annotations. This
keeps the enum small (no `Mixed` variant) and the aggregate logic in
the annotation writer, not the data type.

---

## Struct: `GradleCliFlags` (added to existing scan args)

**File**: `waybill-cli/src/cli/scan_cmd.rs`. `GradleCliFlags` is a
new struct in this file; it's wired into the existing `ScanArgs`
`#[derive(Args)]` struct via `#[command(flatten)]` (matches the m076
`EnrichArgs` precedent used at `sbom_cmd.rs:8`).

```rust
#[derive(Debug, Clone, clap::Args)]
pub struct GradleCliFlags {
    /// Opt in to Gradle subprocess resolution (US1). Requires JDK on $PATH.
    #[arg(long)]
    pub gradle_resolve: bool,

    /// Also resolve the buildscript classpath. Requires --gradle-resolve.
    #[arg(long, requires = "gradle_resolve")]
    pub gradle_resolve_buildscript: bool,

    /// Use Gradle daemon (default: --no-daemon). Requires --gradle-resolve.
    #[arg(long, requires = "gradle_resolve")]
    pub gradle_daemon: bool,

    /// Per-subprocess timeout in seconds. Default: 300 (5 min).
    #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..))]
    pub gradle_timeout_secs: u64,

    /// Additional configurations to resolve beyond the default
    /// runtimeClasspath + testRuntimeClasspath. Repeatable.
    #[arg(long, action = clap::ArgAction::Append)]
    pub gradle_extra_configurations: Vec<String>,
}
```

`clap`'s `requires = "gradle_resolve"` gives us R8's stale-flag
validation for free (clap emits its own error message if a
dependent flag is used without the parent). The zero-timeout error
is enforced by the `range(1..)` value_parser.

---

## Struct: `SubprocessOutcome`

**File**: `waybill-cli/src/scan_fs/package_db/gradle/subprocess.rs`

```rust
pub enum SubprocessOutcome {
    Success(GradleResolvedGraph),
    Timeout,
    NonZeroExit { status: i32, stderr_tail: String },
    ParseError { line: usize, snippet: String },
    ToolMissing,
}
```

The ladder orchestrator maps each non-Success variant to a
`GradleFallbackReason` before descending.

---

## Cache-reader types (US2)

**File**: `waybill-cli/src/scan_fs/package_db/gradle/cache_reader.rs`

```rust
pub struct GradleCache {
    pub root: PathBuf,   // e.g., ~/.gradle/caches/modules-2/
    pub metadata_dir: PathBuf,  // highest-numbered metadata-2.* subdirectory
}

impl GradleCache {
    pub fn discover() -> Result<Self, GradleCacheError>;
    pub fn resolve(&self, coord: &MavenCoord)
        -> Result<CachedPomOrModule, GradleCacheError>;
}

pub enum CachedPomOrModule {
    Pom(quick_xml_parsed_pom),
    Module(serde_json_module),
    Both { pom: ..., module: ... },  // Prefer module when reading
}

pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
}
```

`MavenCoord` isn't a new domain type in the strict Principle IV
sense — it's a struct with three named string fields, but the
strings are unwrapped only during POM/Module parsing where the
external XML/JSON provides the values. Purls are constructed from
this via `Purl::new("maven", &format!("{}/{}", group, artifact), version)`
at emission time.

---

## Relationships

```
              (scan args)
                   │
                   ▼
           GradleCliFlags
                   │
                   ▼
   scan_fs::package_db::gradle::read()  ── m106 lockfile.rs (unchanged)
                   │                    ┐
                   ▼                    │  supplements ONLY;
        gradle::ladder::resolve()       │  m106 output remains
              (per project dir)         │  authoritative for
                   │                    │  lockfile-having projects
    ┌──────────────┼──────────────┐    │
    ▼              ▼              ▼    │
subprocess.rs  cache_reader.rs  static_parser.rs
   (US1)          (US2)             (US3)
    │              │                 │
    └──────────────┴─────────────────┘
                   │
                   ▼
           GradleResolvedGraph
                   │
                   ▼
        GradleScanSummary (per scan)
                   │
                   ▼
      gradle_annotations.rs emits
        waybill:gradle-resolution-tier
        (+ per-subproject when mixed)
                   │
                   ▼
   Existing CDX/SPDX 2.3/SPDX 3 emitters
```

**Invariant**: `GradleScanSummary` is built once per scan (not per
project) and flows through the emission pipeline to attach the
document-scope annotation. Per-subproject annotations attach to
each Gradle main-module `PackageDbEntry` at construction time (they
travel with the component).
