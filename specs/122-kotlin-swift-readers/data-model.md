# Data Model — Kotlin + Swift Ecosystem Readers

**Feature**: 122-kotlin-swift-readers
**Date**: 2026-06-15

This feature introduces FOUR new in-process entity types and extends ONE existing field channel. No persisted entities — all state lives in memory for the duration of a single scan, matching the milestone-002+ posture. No databases, no caches.

## Entity 1 — `SwiftLockfileEntry` (parsed Package.resolved entry)

**Location**: New struct at `mikebom-cli/src/scan_fs/package_db/swift/lockfile.rs`.

**Definition**:

```rust
pub(crate) struct SwiftLockfileEntry {
    /// Project package name (lowercased; equals the SwiftPM `identity` field
    /// on v2/v3 schemas, equals the `package` field on v1).
    pub(crate) identity: String,
    /// Source-of-truth URL for PURL projection. Strips the `.git` suffix at
    /// projection time (Decision 3); preserved verbatim here so the
    /// `mikebom:source-files` annotation can carry the original.
    pub(crate) location: String,
    /// Pinned version string when the `state.version` field was present,
    /// otherwise `None`. v0.1 falls back to the revision SHA per FR-003 /
    /// clarification Q1.
    pub(crate) version: Option<String>,
    /// Git commit SHA the lockfile pinned (40-char hex). Required field
    /// on every SwiftPM schema version; mikebom uses it as the PURL
    /// version segment for commit-pinned mode.
    pub(crate) revision: String,
    /// Branch the operator was tracking when the lockfile was written
    /// (v1 schema only; v2/v3 don't surface it). mikebom IGNORES this
    /// field — it's diagnostic only.
    pub(crate) branch: Option<String>,
}
```

**Lifecycle**:

1. **Construction** (`lockfile::read_package_resolved(path)`): reads bytes via `std::fs::read`, parses JSON via `serde_json::from_slice`, dispatches on top-level `version` integer per Decision 2, produces `Vec<SwiftLockfileEntry>` on success.
2. **Projection** (`lockfile::project_to_package_db_entries(entries, source_path)`): each `SwiftLockfileEntry` becomes one `PackageDbEntry` via the per-entry PURL projection (`pkg:swift/<host>/<ns>/<name>@<version>`) + the standard `mikebom:source-files` annotation pointing at the `Package.resolved` path.
3. **Destruction**: implicit at end of reader-exit. The intermediate `Vec<SwiftLockfileEntry>` is consumed by the projection step and dropped before `read()` returns.

**Invariants**:

1. **`revision` is exactly 40 lowercase hex characters** (SwiftPM enforces this). The reader validates the length + character class at parse time; a non-conforming entry warns + drops.
2. **`location` is a parseable URL** (HTTPS-form or SSH-form). Failures fall through to the SSH-form regex; if neither matches, the entry warns + drops.
3. **Construction goes ONLY through the schema-version dispatcher** — never via a generic JSON deserializer that ignores the version field.

## Entity 2 — `VersionCatalog` (parsed libs.versions.toml)

**Location**: New struct at `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/version_catalog.rs`.

**Definition**:

```rust
pub(crate) struct VersionCatalog {
    /// Resolved alias → (group, name, version) lookup table.
    /// Aliases are kebab-case (`okhttp`, `kotlinx-coroutines`) per
    /// Gradle 7+ convention.
    pub(crate) libraries: BTreeMap<String, ResolvedRef>,
    /// Path to the .toml file the catalog was loaded from. Used in
    /// downstream `mikebom:source-files` annotations.
    pub(crate) source_path: PathBuf,
}

pub(crate) struct ResolvedRef {
    pub(crate) group: String,
    pub(crate) name: String,
    pub(crate) version: String,
}
```

**Lifecycle**:

1. **Construction** (`version_catalog::parse(path)`): reads bytes, parses TOML, reads `[versions]` into a temp `HashMap<String, String>`, walks `[libraries]` resolving `version.ref` references against the versions map. Failures (TOML parse failure, missing `version.ref` target, malformed `module = "g:n"` string) warn + drop the specific entry; the parser returns a (possibly empty) `VersionCatalog`.
2. **Consumption** (`build_script::resolve_catalog_ref(catalog, alias)`): single-method lookup. Returns `Some(ResolvedRef)` on success or `None` when the alias isn't in the catalog (downstream caller warns + skips the dep).
3. **Destruction**: implicit at end of reader-exit. One catalog per scan tree; lifetime is bounded by the `kotlin_dsl::read` call.

**Invariants**:

1. **Every `ResolvedRef` in `libraries` has a non-empty `group`, `name`, AND `version`** — entries that fail to resolve are DROPPED at parse time. Downstream consumers never see partially-resolved entries.
2. **Construction goes through `parse(path)`** — there is no public constructor; consumers cannot build a `VersionCatalog` from arbitrary data.
3. **`source_path` is the verbatim path the parser was called with** (no canonicalization).

## Entity 3 — `KotlinDslEntry` (parsed build.gradle.kts dep declaration)

**Location**: New struct at `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/build_script.rs`.

**Definition**:

```rust
pub(crate) struct KotlinDslEntry {
    /// The dep configuration name as written in source: `implementation`,
    /// `api`, `testImplementation`, etc. Drives lifecycle-scope mapping.
    pub(crate) config: String,
    /// The raw dep token from source. Either a fully-formed GAV string
    /// (`com.squareup.okhttp3:okhttp:4.12.0`), a partial GAV (`g:n` —
    /// catalog-resolved later), or a catalog alias (`libs.okhttp`).
    /// The kind is determined by the regex that matched.
    pub(crate) raw: KotlinDepRaw,
    /// `Some("commonMain")` etc. when the declaration appears inside a
    /// `kotlin { sourceSets { <name> { dependencies { ... } } } }`
    /// block; `None` for top-level `dependencies { ... }` declarations
    /// (the Android default).
    pub(crate) source_set: Option<String>,
    /// 1-indexed source line for diagnostics; threaded into `tracing::warn!`
    /// on resolution failures so operators can grep for the issue.
    pub(crate) source_line: u32,
}

pub(crate) enum KotlinDepRaw {
    /// `implementation("com.squareup.okhttp3:okhttp:4.12.0")`
    Gav { group: String, name: String, version: String },
    /// `implementation("com.squareup.okhttp3:okhttp")` — no version;
    /// resolves via `version.ref` or catalog lookup.
    PartialGav { group: String, name: String },
    /// `implementation(libs.okhttp)`
    CatalogAlias { alias: String },
}
```

**Lifecycle**:

1. **Construction** (`build_script::extract_deps(content)`): runs the three regexes from Decision 4 against the file content; each match produces one `KotlinDslEntry`. Source-set tracking (brace-depth heuristic) populates the `source_set` field.
2. **Projection** (`build_script::resolve_and_emit(entries, catalog, project_purl) -> Vec<PackageDbEntry>`): each entry is resolved against the optional `VersionCatalog`; resolved entries produce one `PackageDbEntry` carrying the maven PURL + the lifecycle-scope mapping + the `mikebom:source-files` annotation + the `mikebom:kmp-source-set` JSON-array annotation (when applicable). Unresolvable entries (e.g., a `CatalogAlias` when no catalog loaded) warn + drop.
3. **Destruction**: implicit at end of reader-exit.

**Invariants**:

1. **`config` is always one of the documented Gradle dep-configuration names** — the regex match restricts the captured value to the known set. Unknown configurations are NOT captured (they don't match the regex; operators get a `tracing::debug!` line announcing the skip).
2. **`KotlinDepRaw::Gav.version` is non-empty** when present (the regex requires three colon-separated segments).
3. **`source_set` reflects the LAST source-set seen** when nested blocks are present — the heuristic doesn't model multi-source-set membership directly; that's tracked via the post-emission `KmpSourceSetTracker` (Entity 4).

## Entity 4 — `KmpSourceSetTracker` (post-emission KMP source-set aggregation)

**Location**: New struct at `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/mod.rs`.

**Definition**:

```rust
pub(crate) struct KmpSourceSetTracker {
    /// Canonical PURL → set of source-set names that declared it.
    /// BTreeSet preserves lex order for determinism.
    map: BTreeMap<Purl, BTreeSet<String>>,
}

impl KmpSourceSetTracker {
    pub(crate) fn new() -> Self {
        Self { map: BTreeMap::new() }
    }
    pub(crate) fn record(&mut self, purl: Purl, source_set: String) {
        self.map.entry(purl).or_default().insert(source_set);
    }
    /// Project into (PURL, JSON-array) tuples ready to stamp onto
    /// PackageDbEntry::extra_annotations under the
    /// `mikebom:kmp-source-set` key.
    pub(crate) fn finalize(self) -> Vec<(Purl, serde_json::Value)> {
        self.map
            .into_iter()
            .map(|(purl, set)| {
                let arr: Vec<serde_json::Value> =
                    set.into_iter().map(serde_json::Value::String).collect();
                (purl, serde_json::Value::Array(arr))
            })
            .collect()
    }
}
```

**Lifecycle**:

1. **Construction** (`KmpSourceSetTracker::new()`): zero state.
2. **Recording** (`record(purl, source_set)`): called once per `KotlinDslEntry` that has a `Some(source_set)`. Idempotent — duplicate records are absorbed by the BTreeSet's deduplication.
3. **Finalization** (`finalize()`): consumes the tracker and yields `(Purl, serde_json::Value::Array)` tuples ready to stamp onto each component's `extra_annotations`.
4. **Destruction**: implicit at end of reader-exit. One tracker per scan; lives entirely within the `kotlin_dsl::read` call.

**Invariants**:

1. **Determinism**: the BTreeSet's lex order is what consumers grep against. Two scans of the same project tree produce byte-identical `mikebom:kmp-source-set` values (modulo the existing per-scan timestamp / serialNumber fields).
2. **PURL uniqueness preserved**: each canonical PURL maps to AT MOST ONE `BTreeSet<String>` of source-sets. No duplicate components emerge from the tracker — the dep's single component carries the combined source-set provenance.
3. **No empty arrays in finalized output**: every entry in `map` has at least one source-set (the recording call requires a non-empty source-set string).

## Entity 5 — `extra_annotations` channel extension (no new entity, one new key)

**Location**: Existing field on `PackageDbEntry` at `mikebom-cli/src/scan_fs/package_db/mod.rs`.

**Definition** (pre-existing):

```rust
pub extra_annotations: BTreeMap<String, serde_json::Value>,
```

**New key this feature stamps**:

| Key | Scope | Value shape | Emission gating |
|---|---|---|---|
| `mikebom:kmp-source-set` | per-component | `serde_json::Value::Array(Vec<String>)` (JSON-encoded at emission time) | Only on components discovered from a Kotlin Multiplatform `kotlin { sourceSets { <name> { dependencies { ... } } } }` block. Absent on non-KMP components. |

**Emission path**: existing serialization at `generate/cyclonedx/builder.rs:965-973` (per-component properties) automatically renders this as a `properties[]` entry whose value is a JSON-encoded string of the array — same path as milestone-116 C64 `mikebom:produces-binaries` and milestone-119 C67 `mikebom:assertion-conflict`. SPDX 2.3 + SPDX 3 use the existing `MikebomAnnotationCommentV1` envelope.

## Validation rules summary

| Rule | Source | Where enforced |
|---|---|---|
| `Package.resolved` schema version dispatch | Decision 2 / FR-009 | `swift::lockfile::read_package_resolved` |
| Swift PURL projection from URL | Decision 3 / FR-014 | `swift::lockfile::project_purl` |
| Commit-pinned PURL uses full SHA | clarification Q1 / FR-003 | `swift::lockfile::project_purl` |
| `Package.swift` detection-only (no content parsing) | clarification Q3 / FR-002 | `swift::manifest::detect` (returns `bool`, never returns deps) |
| `build.gradle.kts` regex-based dep extraction | Decision 4 / FR-004 | `kotlin_dsl::build_script::extract_deps` |
| `libs.versions.toml` lookup table | Decision 5 / FR-005 | `kotlin_dsl::version_catalog::parse` + `build_script::resolve_catalog_ref` |
| KMP source-set JSON-array storage | clarification Q2 / Decision 6 / FR-006 | `kotlin_dsl::KmpSourceSetTracker::finalize` |
| Workspace-root `pkg:generic/<rootProject.name>@0.0.0` PURL | clarification Q4 / Decision 6 / FR-007 | `kotlin_dsl::synthesize_workspace_root` |
| Design-tier gating for `build.gradle.kts`-discovered components | clarification Q5 / FR-004 | `kotlin_dsl::read` reads the `include_dev` parameter (threaded from scan_cmd) |
| Parse failures → `tracing::warn!` + zero components | FR-009 | each reader's error-handling branch |
| Polyglot composition via existing dispatcher | Decision 7 / FR-008 | `mikebom-cli/src/scan_fs/package_db/mod.rs:1138-1400` (`read_all`) |
| No network calls | FR-012 | absent imports — neither reader pulls in `reqwest` |
| `--exclude-path` honored | FR-011 | `safe_walk` integration (every reader uses the existing helper) |

No state transitions; the readers are pure functions over the filesystem, called once per scan.
