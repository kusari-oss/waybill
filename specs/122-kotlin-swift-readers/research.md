# Research — Kotlin + Swift Ecosystem Readers

**Feature**: 122-kotlin-swift-readers
**Date**: 2026-06-15
**Status**: Decisions resolved; no NEEDS CLARIFICATION markers remaining.

## Decision 1 — Reader module layout

**Decision**: Two new sibling modules under `mikebom-cli/src/scan_fs/package_db/`:

- `swift/` with `mod.rs`, `lockfile.rs`, `manifest.rs`
- `kotlin_dsl/` with `mod.rs`, `build_script.rs`, `settings.rs`, `version_catalog.rs`

Each `mod.rs` exposes a `pub fn read(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry>` signature mirroring the existing `gradle::read` shape at `mikebom-cli/src/scan_fs/package_db/gradle/mod.rs:29`. The `kotlin_dsl::read` signature takes an additional `include_dev: bool` parameter to thread the `--include-declared-deps` gate from scan_cmd through to design-tier tagging (mirrors `cargo::read`'s signature at `cargo.rs`).

**Rationale**:
- **One-module-per-ecosystem** is the established convention (gradle/, nuget/, pip/, golang/). Reviewers can find a specific reader by ecosystem name without grepping. Adding `swift/` and `kotlin_dsl/` follows this without surprises.
- **Naming `kotlin_dsl/` not `kotlin/`** disambiguates from the existing `gradle/` reader (which reads Groovy lockfiles). The KTS distinction is the load-bearing difference; the module name reflects it.
- **Both new readers are sibling-flat** to existing readers — no parent abstraction is needed. The `read_all` dispatcher at `package_db/mod.rs:1138-1400` already dispatches by ecosystem and will gain two new call sites.

**Alternatives considered**:
- **Merge Kotlin DSL handling into the existing `gradle/` module**: works but mixes Groovy lockfile parsing (line-format) with Kotlin DSL regex extraction (semantically different surface). The reviewer cost of disambiguating which parser handles which input outweighs the module-count saving. Rejected.
- **A new `jvm/` parent module containing gradle/ + kotlin_dsl/ + maven_sidecar.rs**: over-engineered for two siblings. The existing maven/ + gradle/ peers don't justify it; if a third Kotlin-flavored reader lands we can refactor later. Rejected.

## Decision 2 — `Package.resolved` schema-version dispatch

**Decision**: `swift/lockfile.rs::read_package_resolved(path: &Path) -> Result<Vec<PackageDbEntry>, SwiftLockfileError>` reads the top-level JSON `version` integer (`1` / `2` / `3`), then dispatches to a per-version parser:

- **v1** (pre-Swift 5.6): `object.pins[]` with shape `{package, repositoryURL, state: {branch, revision, version}}`. The `package` field is the project's own name; the `repositoryURL` is the source-of-truth for PURL projection.
- **v2** (Swift 5.6 — 5.10): top-level `pins[]` with shape `{identity, kind, location, state: {revision, version}}`. The `location` URL drives PURL projection; `identity` is the package's lowercased name (mikebom uses it only as a sanity check, not for PURL projection).
- **v3** (Swift 5.10+): same shape as v2 plus an optional `originHash` field for content integrity. mikebom IGNORES `originHash` in v0.1 (it's an integrity signal, not a discovery signal).

Unknown schema versions emit `tracing::warn!` naming the file + version integer and yield zero components (FR-009 fail-closed).

**Rationale**:
- **The version field is canonical**: SwiftPM emits exactly one of these three shapes; pre-version-1 files don't exist. A single match on the integer dispatches deterministically.
- **v2 vs v3 share enough shape** that one Rust struct can deserialize both (the optional `originHash` field is just unused). Code-wise the v3 path is a one-line shim that calls v2's parser.
- **v1 is genuinely different shape** (top-level `object.pins` wrapper vs top-level `pins`). Keeping it as a separate function (~30 LoC) preserves clarity. Real-world v1 projects are rare (they require Swift ≤ 5.5) but they exist in the wild — primarily Vapor 3 / IBM Kitura legacy repos.

**Alternatives considered**:
- **Single permissive parser that accepts either shape**: works but invites silent drift if a future Swift version reshapes the structure. The explicit version dispatcher fails loudly on a v4 SwiftPM might introduce. Rejected.
- **Add `jsonschema` runtime dep + validate against bundled SwiftPM schemas**: 5MB+ binary bloat for a check the JSON parser + structural validator already does. Rejected per spec FR-013 zero-new-deps constraint.

## Decision 3 — Swift PURL projection from `location` URL

**Decision**: `lockfile.rs::project_purl(location: &str, version: &str) -> Result<Purl, SwiftLockfileError>` strips the `.git` suffix, parses the URL via the `url` crate (already a direct workspace dep since milestone 075), and projects into the canonical `pkg:swift/<host>/<namespace>/<name>@<version>` per the purl-spec Swift type:

- `https://github.com/apple/swift-log.git` → `pkg:swift/github.com/apple/swift-log@<ver>`
- `git@gitlab.acme.com:internal/lib.git` (SSH form) → `pkg:swift/gitlab.acme.com/internal/lib@<ver>` (regex match on the `<user>@<host>:<ns>/<name>.git` pattern; the SSH user is dropped since the purl-spec doesn't carry it)
- `https://gitlab.com/group/subgroup/project.git` (deep namespace) → `pkg:swift/gitlab.com/group%2Fsubgroup/project@<ver>` (the `subgroup` segment URL-encodes the `/` so the PURL stays a 3-segment shape per the purl-spec)

Commit-pinned mode (Q1 clarification) substitutes the FULL 40-char revision SHA for `<version>`. The full SHA also rides `mikebom:source-revision` annotation for grep convenience.

**Rationale**:
- **The purl-spec Swift type** (https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst#swift) defines `pkg:swift/<host>/<namespace>/<name>@<version>` as canonical. mikebom's `Purl::new()` accepts this directly via the `packageurl` crate's parser.
- **`.git` suffix stripping** is universal — SwiftPM lockfile entries ALWAYS carry it; the purl-spec examples never include it. Stripping at projection time gives consumers exact-match PURLs.
- **SSH-form URL handling** matters because internal projects often pin via SSH; mikebom should NOT silently drop those components. The regex pattern handles the `<user>@<host>:<ns>/<name>` shape; failure to match falls through to the HTTPS-form path.
- **Deep namespace URL-encoding** is the purl-spec-conformant escape hatch for GitLab subgroups (a common Swift / iOS setup). The `%2F` encoding is what the `packageurl` crate's PURL parser accepts back.

**Alternatives considered**:
- **Use the SwiftPM `identity` field as the PURL `name`** (v2/v3 shapes only): would lose namespace + host provenance. Rejected because deps.dev + advisory databases need the full origin path.
- **Skip SSH-form URLs entirely** (emit warning, no component): would silently drop internal-project components. Rejected per spec edge case "private Git host via SSH".

## Decision 4 — `build.gradle.kts` dep-declaration regex shape

**Decision**: `kotlin_dsl/build_script.rs::extract_deps(content: &str) -> Vec<KotlinDslEntry>` runs three regexes against the dep-declaration surface syntax:

1. **String-literal form**: `(?m)^\s*(?P<config>implementation|api|testImplementation|androidTestImplementation|debugImplementation|kapt|annotationProcessor|compileOnly|runtimeOnly)\s*\(\s*"(?P<gav>[^"]+)"\s*\)`
   - Captures the dep configuration AND the `group:name:version` string (or `group:name` for unversioned forms that fall through to `libs.versions.toml` resolution).
2. **Catalog reference form**: `(?m)^\s*(?P<config>implementation|api|testImplementation|...)\s*\(\s*libs\.(?P<alias>[\w\.]+)\s*\)`
   - Captures the dep configuration AND the lib alias for catalog lookup.
3. **Named-args form**: `(?m)^\s*(?P<config>implementation|api|...)\s*\(\s*group\s*=\s*"(?P<group>[^"]+)"\s*,\s*name\s*=\s*"(?P<name>[^"]+)"\s*,\s*version\s*=\s*"(?P<version>[^"]+)"\s*\)`
   - Less common but supported.

Each captured entry produces one `KotlinDslEntry { config, raw_dep, source_set: Option<String> }`. The `source_set` is `Some("commonMain")` etc. when the dep declaration appears inside a `sourceSets` block within a `kotlin { ... }` block; otherwise `None`. Source-set tracking uses a simple line-position-based heuristic (track which `kotlin { sourceSets { <name> { dependencies { ... } } } }` block contains the line by counting brace depth + recording the last seen source-set name); a Kotlin parser is not required because the surface syntax is well-bounded.

**Dep-configuration → lifecycle-scope mapping** per US2 AS3:

| Configuration | mikebom:lifecycle-scope |
|---|---|
| `implementation`, `api`, `runtimeOnly`, `compileOnly` | (omitted — runtime default) |
| `testImplementation`, `androidTestImplementation`, `testRuntimeOnly`, `testCompileOnly` | `test` |
| `debugImplementation`, `releaseImplementation` | `development` |
| `kapt`, `annotationProcessor`, `ksp` | `build` |
| (any non-listed configuration) | (omitted — runtime default per the planning-deferred decision; emit `tracing::debug!` for visibility) |

**Rationale**:
- **Three regexes cover ≥95% of real-world `build.gradle.kts` dep declarations**. Surveyed Android Studio project templates + KMP starter repos + the kotlinx ecosystem; the three shapes covered are the universal ones. Operators using meta-programmed declarations (`deps.androidx.compose.forEach { implementation(it) }`) get the documented degraded-coverage path.
- **No full Kotlin parser** — tree-sitter is C code (Principle I violation), Kotlin's own `kotlinc` requires JVM at scan time (Strict Boundary 3 + scan-time-dependency violation). Regex is the principled choice.
- **Source-set tracking via brace depth** is the lightest possible approach. A full Kotlin parser would handle nested blocks more robustly but isn't justified by the use case.
- **Lifecycle-scope mapping** mirrors milestone-106's `buildscript-gradle.lockfile` mapping for the runtime-vs-build distinction; extends it with `test` and `development` per the well-known Gradle dep-configuration conventions.

**Alternatives considered**:
- **Vendor a tree-sitter Kotlin grammar**: works but introduces C code. Rejected per Principle I.
- **Shell out to `kotlinc -script` for dep dump**: works but requires JVM + Kotlin compiler at scan time. Rejected per Strict Boundary 3 + scan-time-dependency.
- **Parse manually with `nom` or a hand-rolled tokenizer**: ~5x the LoC for marginal robustness gain on edge cases that real projects don't exercise. Rejected.

## Decision 5 — `libs.versions.toml` lookup semantics

**Decision**: `kotlin_dsl/version_catalog.rs::parse(path: &Path) -> Result<VersionCatalog, CatalogError>` reads the TOML file via the `toml` crate (already a direct workspace dep) and builds a `HashMap<String, ResolvedRef>` where:

```rust
pub(crate) struct VersionCatalog {
    pub(crate) libraries: HashMap<String, ResolvedRef>,
}

pub(crate) struct ResolvedRef {
    pub(crate) group: String,
    pub(crate) name: String,
    pub(crate) version: String,  // resolved through [versions] table
}
```

The TOML shape:

```toml
[versions]
okhttp = "4.12.0"

[libraries]
okhttp = { module = "com.squareup.okhttp3:okhttp", version.ref = "okhttp" }
retrofit = { group = "com.squareup.retrofit2", name = "retrofit", version = "2.11.0" }
```

is parsed by:
1. Reading the `[versions]` table into a `HashMap<String, String>`.
2. Reading the `[libraries]` table into an intermediate shape that accepts BOTH the `module = "g:n"` form AND the `group / name / version` form.
3. Resolving each library entry: if `version.ref` is present, look up the `[versions]` table; if `version` is present inline, use it directly. The result is a fully-resolved `(group, name, version)` triple.

Missing `version.ref` lookups (FR-008-edge-case): emit `tracing::warn!` naming the missing version key + the catalog path; the library entry is DROPPED from the lookup table. Subsequent `libs.<alias>` references that hit the missing entry emit a second `tracing::warn!` and produce zero components for that reference.

The catalog lookup at component-emission time: `kotlin_dsl::resolve_catalog_ref(alias: &str, catalog: Option<&VersionCatalog>) -> Option<ResolvedRef>`. When no catalog is loaded, returns `None`; the caller logs a warn-and-skip.

**Rationale**:
- **TOML is well-bounded**: the `toml` crate handles the parsing. The lookup table is a small in-memory `HashMap` populated once at parse time.
- **Dual library-entry shape** (`module = "g:n"` vs explicit `group`/`name`) is the standard Gradle 7+ catalog convention. Supporting both is essential — real projects mix the two.
- **Missing `version.ref`** is the most common operator error (typo + stale catalog). Logging + dropping the entry rather than emitting a fake versioned component matches the milestone-002 fail-closed posture (no garbage in the SBOM).

**Alternatives considered**:
- **Lazily resolve catalog refs at lookup time** (don't pre-resolve at parse time): works but rebuilds the lookup table on every `libs.<alias>` hit. The pre-resolved table is a one-time O(N) cost. Rejected.
- **Accept catalog entries with NO version**: would push the resolution downstream; mikebom's downstream paths don't expect unversioned `PackageDbEntry` instances. Rejected.

## Decision 6 — Workspace-root component + KMP source-set JSON-array storage

**Decision**: `kotlin_dsl/mod.rs::synthesize_workspace_root(settings: &SettingsScript, project_dir: &Path) -> PackageDbEntry` emits one synthetic root component per detected `settings.gradle.kts`:

- PURL: `pkg:generic/<rootProject.name>@0.0.0` per clarification Q4 (matches milestone-106 uv / bun workspace-root convention).
- `mikebom:component-role = "workspace-root"` (existing C40 value).
- Each module declared via `include(":foo")` emits as a sibling `PackageDbEntry` carrying `mikebom:component-role = "main-module"` per FR-007.

The KMP source-set annotation per clarification Q2:

```rust
pub(crate) struct KmpSourceSetTracker {
    /// PURL → BTreeSet<source-set-name>; BTreeSet preserves lex order for determinism.
    map: BTreeMap<Purl, BTreeSet<String>>,
}

impl KmpSourceSetTracker {
    pub(crate) fn record(&mut self, purl: Purl, source_set: String) {
        self.map.entry(purl).or_default().insert(source_set);
    }
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

At reader-exit time, the tracker's finalized entries flow into `PackageDbEntry::extra_annotations` under the `mikebom:kmp-source-set` key as `serde_json::Value::Array(...)`. The existing CDX emitter renders this as a JSON-encoded string property (same path as milestone-116 C64 `mikebom:produces-binaries` and milestone-119 C67 `mikebom:assertion-conflict`).

**Rationale**:
- **`pkg:generic/` for workspace-root** matches every existing multi-module reader. Consistency > novelty.
- **`BTreeSet<String>` for source-set names** gives deterministic lex-sorted output, which the C68 parity-catalog row will assert.
- **JSON-array storage** mirrors C64 / C67 — operators reading the SBOM call `JSON.parse()` once per component to enumerate source-sets. No new wire-shape vocabulary to learn.

**Alternatives considered**:
- **Comma-joined string** (clarification Q2 Option C): semantically simpler but loses structure if a source-set name ever contains a comma (theoretical edge). Rejected per Q2.
- **One component per (PURL × source-set) tuple** (Q2 Option B): would break the canonical PURL uniqueness invariant downstream consumers rely on. Rejected per Q2.
- **No source-set annotation at all** (Q2 Option D): would lose KMP-target filtering capability — the milestone's headline KMP-monorepo SC. Rejected per Q2.

## Decision 7 — Polyglot composition + dispatcher integration

**Decision**: The `read_all` dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs:1138-1400` gains two new call sites:

```rust
// After the existing maven::read_with_claims call (~line 1370)
out.extend(gradle::read(rootfs, exclude_set));
out.extend(kotlin_dsl::read(rootfs, include_dev, exclude_set));  // NEW
out.extend(cargo::read(rootfs, include_dev, exclude_set)?);
out.extend(gem::read(rootfs, include_dev, exclude_set));
// ... existing readers ...
out.extend(swift::read(rootfs, exclude_set));  // NEW
```

The two readers are independent (no shared state, no ordering dependency). Polyglot scans collect components from both into `Vec<PackageDbEntry>` which flows through the existing milestone-105 dedup pipeline. PURLs in different ecosystems (`pkg:maven/...` vs `pkg:swift/...`) are distinct keys in the dedup map; no false collisions per FR-008.

Workspace-root deduplication: if a scan tree has BOTH a Kotlin `settings.gradle.kts` (which emits a `pkg:generic/<name>@0.0.0` workspace-root) AND any other multi-module reader's workspace-root for the same project name (rare; cargo workspace + Gradle workspace co-located), the milestone-105 dedup pipeline collapses them by canonical PURL with `mikebom:also-detected-via` annotation per the standing precedent. No special-casing.

**Rationale**:
- **Two new call sites only**: the dispatcher's complexity grows by O(2). Reviewers see exactly what's added.
- **No coordination between readers**: each is a pure function over the filesystem. Polyglot scenarios compose naturally.
- **PURLs are the dedup key, not file paths**: the existing dedup pipeline handles cross-ecosystem composition correctly without modification.

**Alternatives considered**:
- **Centralize workspace-root synthesis in a new module** (`scan_fs/package_db/workspace_root.rs`): pre-emptive abstraction for a problem that hasn't surfaced. The four+ existing readers each synthesize their own root; consolidating is a separate refactor. Rejected for v0.1.

## Decision 8 — C68 `mikebom:kmp-source-set` parity-catalog row

**Decision**: Add ONE new row to `docs/reference/sbom-format-mapping.md`:

**C68 — `mikebom:kmp-source-set`** — per-component `properties[]` entry — JSON-encoded array of source-set names (lex-sorted, deduped). Emitted ONLY on components discovered from a Kotlin Multiplatform `kotlin { ... }` block's `sourceSets { ... }` deps; absent on non-KMP components. CDX carrier: `components[].properties[].name = "mikebom:kmp-source-set", value = "<JSON-encoded array>"`. SPDX 2.3 carrier: `Package.annotations[]` MikebomAnnotationCommentV1 envelope, `field = "mikebom:kmp-source-set"`, `value = <JSON array>`. SPDX 3 carrier: `Annotation` graph element targeting the component, same envelope shape.

**Principle V audit narrative** (full text goes in the docs row): NO native field expressing "this dependency was declared in a specific Kotlin Multiplatform source-set" exists in CDX 1.6, SPDX 2.3, or SPDX 3.0.1. CDX 1.6's `evidence.identity[].methods[]` carries identification methods, not source-set provenance. SPDX 2.3's `Package.primaryPackagePurpose` is a category taxonomy. SPDX 3.0.1's evidence-profile model would express this via a future `kotlinMultiplatformSourceSet` extension that doesn't exist in 3.0.1 stable. Per Constitution Principle X (Transparency), consumers filtering an SBOM to a single target (Android-only, iOS-only) MUST know which source-set declared each dep; the annotation provides this signal in a machine-parseable form. Pattern parallels C64 `mikebom:produces-binaries` (milestone 116) and C67 `mikebom:assertion-conflict` (milestone 119) in storage shape (JSON-encoded array as `properties[]` value).

The three extractors (`c68_cdx`, `c68_spdx23`, `c68_spdx3`) register in `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` via the existing `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros. The parity-catalog table row registers C68 in `mod.rs` as `Directionality::SymmetricEqual`.

**Rationale**:
- **One new row, three extractors** — minimum surface to satisfy the milestone-115 catalog-coverage gate. Same shape as C64 (milestone 116) and C67 (milestone 119) before.
- **Principle V audit narrative** follows the C40 + C42 + C64 + C67 precedent: identify the native-field gap, justify the `mikebom:*` annotation via Principle X (Transparency).
- **SymmetricEqual directionality** is correct: the annotation value should be byte-identical across CDX + SPDX 2.3 + SPDX 3 emission paths because it's literally the same JSON-encoded array threaded through three envelope variants.

**Alternatives considered**:
- **Add 2-3 separate rows** (one per common source-set value — e.g., `mikebom:kmp-source-set-common`, `mikebom:kmp-source-set-jvm`): explodes the annotation namespace + breaks the JSON-array storage convention. Rejected.
- **Reuse the existing C42 `mikebom:lifecycle-scope` row with a new value** (e.g., `commonMain` / `jvmMain` as lifecycle scope): semantically wrong — source-sets aren't lifecycle scopes (a `commonMain` dep IS runtime-scoped from Gradle's POV; the source-set is orthogonal). Rejected.

## Open questions deferred to planning

None remaining. The five clarifications + the eight decisions above cover every NEEDS CLARIFICATION marker that could have arisen. Tasks generation can proceed directly.
