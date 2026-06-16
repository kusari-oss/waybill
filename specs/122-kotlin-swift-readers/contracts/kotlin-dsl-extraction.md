# Contract: `build.gradle.kts` + `settings.gradle.kts` + `libs.versions.toml` extraction

**Feature**: 122-kotlin-swift-readers
**Date**: 2026-06-15
**Consumed by**: the Kotlin DSL reader at `mikebom-cli/src/scan_fs/package_db/kotlin_dsl/`; integration tests at `mikebom-cli/tests/scan_kotlin_dsl.rs`
**Spec mapping**: FR-004, FR-005, FR-006, FR-007, FR-009, FR-011, FR-013

## File envelope

The Kotlin DSL reader recognizes THREE file shapes:

1. **`build.gradle.kts`** — Kotlin DSL build script declaring dep configurations + (optionally) KMP `kotlin { ... }` source-set blocks. Lives in each module's directory.
2. **`settings.gradle.kts`** — Kotlin DSL settings script declaring multi-module workspace topology via `include(":module")` + `rootProject.name = "..."`. Lives at the workspace root.
3. **`gradle/libs.versions.toml`** — TOML version catalog. Lives under a `gradle/` subdirectory of the workspace root.

## `build.gradle.kts` dep declaration surface syntax

The reader matches THREE regex shapes (per research Decision 4). All three are line-anchored (each match starts at the beginning of a line) to avoid false-positives inside Kotlin multi-line string literals or comments.

### Shape 1 — Fully-qualified string-literal GAV

```kotlin
dependencies {
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    api("org.jetbrains.kotlin:kotlin-stdlib:1.9.20")
    testImplementation("io.kotest:kotest-runner-junit5:5.8.0")
}
```

Regex: `(?m)^\s*(?P<config>implementation|api|testImplementation|androidTestImplementation|debugImplementation|releaseImplementation|kapt|annotationProcessor|ksp|runtimeOnly|compileOnly|testRuntimeOnly|testCompileOnly)\s*\(\s*"(?P<gav>[^"]+)"\s*\)`

Captures:
- `config` — the dep configuration name (drives lifecycle-scope mapping)
- `gav` — the `group:name:version` string (or `group:name` for unversioned entries that need catalog lookup)

### Shape 2 — Version-catalog alias reference

```kotlin
dependencies {
    implementation(libs.okhttp)
    api(libs.kotlinx.coroutines.core)
}
```

Regex: `(?m)^\s*(?P<config>implementation|api|testImplementation|...)\s*\(\s*libs\.(?P<alias>[\w\.]+)\s*\)`

Captures:
- `config` — same as Shape 1
- `alias` — the catalog alias to look up in `libs.versions.toml` (dotted form preserved for nested aliases like `kotlinx.coroutines.core` → catalog key `kotlinx-coroutines-core`)

### Shape 3 — Named-arguments GAV (less common)

```kotlin
dependencies {
    implementation(group = "com.squareup.okhttp3", name = "okhttp", version = "4.12.0")
}
```

Regex: `(?m)^\s*(?P<config>implementation|api|...)\s*\(\s*group\s*=\s*"(?P<group>[^"]+)"\s*,\s*name\s*=\s*"(?P<name>[^"]+)"\s*,\s*version\s*=\s*"(?P<version>[^"]+)"\s*\)`

Captures:
- `config` — same
- `group`, `name`, `version` — split GAV components

### What's NOT matched

- Deps declared via metaprogramming (`deps.forEach { implementation(it) }`)
- Deps declared via custom DSL extensions (`coreDeps()` shorthand functions)
- Deps declared via Kotlin reflection (rare; usually only in deeply-custom build scripts)
- Deps inside `buildscript { ... }` blocks (those are build-tool plugins, not project deps — different lifecycle)
- Multi-line continued string concatenation (`"$group:$name:$version"`)

Each non-match is invisible to the reader; the operator gets a `tracing::debug!` line announcing that no deps were extracted from a `build.gradle.kts` if the file contained `dependencies { ... }` but zero regex matches. This is the documented "common surface syntax only" contract from spec Assumptions.

## Source-set tracking

KMP `kotlin { sourceSets { ... } }` blocks declare deps per-target:

```kotlin
kotlin {
    jvm()
    iosX64()
    android()
    sourceSets {
        commonMain {
            dependencies {
                implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2")
            }
        }
        jvmMain {
            dependencies {
                implementation("io.ktor:ktor-client-cio-jvm:2.3.7")
            }
        }
    }
}
```

The reader tracks source-set membership by counting brace depth from file start + recording the last identifier preceding a `{` block. When a `dependencies { ... }` block opens inside a `sourceSets` block, the most-recently-recorded source-set name becomes the `source_set` field on every `KotlinDslEntry` captured inside.

For deps declared in the top-level `dependencies { ... }` block (Android default — no KMP):

```kotlin
android { ... }

dependencies {
    implementation("androidx.compose.ui:ui:1.6.0")  // source_set = None
}
```

The `source_set` field is `None`. The `mikebom:kmp-source-set` annotation is NOT stamped on the resulting component.

## `settings.gradle.kts` parsing

The settings parser extracts two pieces of information:

1. **`rootProject.name = "..."`** — captured via `^\s*rootProject\.name\s*=\s*"(?P<name>[^"]+)"\s*$`. Used to construct the workspace-root PURL `pkg:generic/<name>@0.0.0`. Falls back to the workspace directory name when absent.
2. **`include(":module1", ":module2", ...)`** — captured via `^\s*include\(\s*("(?P<m>[^"]+)"(?:\s*,\s*"(?P<rest>[^"]+)")*)\s*\)\s*$`. Each captured module name (after stripping the leading colon) becomes a workspace member; mikebom expects a `build.gradle.kts` to exist at `<workspace>/<module-name>/build.gradle.kts`.

Recursive nested workspaces (a module's `settings.gradle.kts` that itself declares `include(...)`) are NOT treated as workspace roots — only the OUTERMOST settings file drives workspace topology. The outer reader's nested `kotlin_dsl::read` walk simply finds the inner `build.gradle.kts` and adds it as a regular module under the existing root.

## `libs.versions.toml` parsing

The version-catalog parser reads the TOML file and constructs a `BTreeMap<String, ResolvedRef>` lookup table.

### TOML shape

```toml
[versions]
okhttp = "4.12.0"
kotlin = "1.9.20"
ktor = "2.3.7"

[libraries]
okhttp = { module = "com.squareup.okhttp3:okhttp", version.ref = "okhttp" }
retrofit = { group = "com.squareup.retrofit2", name = "retrofit", version = "2.11.0" }
ktor-client-cio = { module = "io.ktor:ktor-client-cio-jvm", version.ref = "ktor" }
```

The `[libraries]` table accepts BOTH forms:

1. `module = "g:n"` + `version.ref = "<versions-key>"` — module-style + version-ref
2. `group = "g"` + `name = "n"` + `version = "..."` — split GAV with inline version

The parser:
1. Reads `[versions]` into a temp `HashMap<String, String>`.
2. For each library entry:
   - Parse the `module` value (splitting on `:`) OR the split `group` / `name` keys
   - Resolve `version.ref` against the versions map OR use the inline `version` literal
   - On success: insert `(alias → ResolvedRef { group, name, version })`
   - On failure (malformed `module`, missing `version.ref` target, etc.): emit `tracing::warn!` naming the alias + the catalog path; the entry is DROPPED from the lookup table

### Lookup at component-emission time

For each `KotlinDslEntry::CatalogAlias { alias }`, the resolver calls `catalog.libraries.get(&convert_dotted_to_dashed(alias))` (since Kotlin DSL uses `libs.kotlinx.coroutines.core` for catalog key `kotlinx-coroutines-core`). On hit: produce a fully-resolved `(group, name, version)`. On miss: emit `tracing::warn!` + skip the dep.

## Output: `PackageDbEntry` shape per resolved dep

| Field | Value |
|---|---|
| `purl` | `pkg:maven/<group>/<name>@<version>` |
| `name` | `<name>` |
| `version` | `<version>` |
| `extra_annotations` | `{ "mikebom:source-files": "<path-to-build.gradle.kts>" }` + (when applicable) `{ "mikebom:kmp-source-set": "[\"commonMain\",...]" }` (JSON-encoded array per the C68 row) |
| `lifecycle_scope` | mapped per the table below |
| `sbom_tier` | `Some("design")` for `build.gradle.kts`-only-discovered components per clarification Q5; downstream dedup against `gradle.lockfile`-discovered entries (milestone 106) wins the tier as `source` |

### Dep-configuration → lifecycle-scope mapping

| Configuration | `LifecycleScope` |
|---|---|
| `implementation`, `api`, `runtimeOnly`, `compileOnly` | `None` (runtime default — annotation omitted) |
| `testImplementation`, `androidTestImplementation`, `testRuntimeOnly`, `testCompileOnly` | `Some(Test)` |
| `debugImplementation`, `releaseImplementation` | `Some(Development)` |
| `kapt`, `annotationProcessor`, `ksp` | `Some(Build)` |
| (any non-listed) | `None` (runtime default — emit `tracing::debug!` for visibility) |

## Workspace-root emission

Per clarification Q4 + FR-007, when a `settings.gradle.kts` is present at any directory in the scan tree, the reader emits ONE additional `PackageDbEntry`:

| Field | Value |
|---|---|
| `purl` | `pkg:generic/<rootProject.name>@0.0.0` |
| `name` | `<rootProject.name>` (or the workspace directory name if `rootProject.name` absent) |
| `version` | `"0.0.0"` |
| `extra_annotations` | `{ "mikebom:component-role": "workspace-root", "mikebom:source-files": "<path-to-settings.gradle.kts>" }` |
| `lifecycle_scope` | `None` |
| `sbom_tier` | `Some("source")` |

Each `include(":module")` module emits an ADDITIONAL `PackageDbEntry` carrying `mikebom:component-role = "main-module"` per the milestone-106 workspace convention.

## Error semantics

| Error class | Cause | Behavior |
|---|---|---|
| `Io { path, source }` | File unreadable | `tracing::warn!` naming path + io::Error; zero components for this file; walk continues |
| `RegexNoMatch { path }` | `build.gradle.kts` parsed; `dependencies { ... }` block found; zero regex matches inside | `tracing::debug!` naming path (the operator may be using meta-programmed deps); zero components for this file; walk continues |
| `CatalogParseError { path, source }` | `libs.versions.toml` unparseable | `tracing::warn!`; catalog is treated as empty; `build.gradle.kts` deps using `libs.<alias>` references warn + drop one-by-one |
| `MissingCatalogAlias { path, alias }` | `libs.<alias>` reference doesn't match any catalog entry | `tracing::warn!` naming the alias + the referencing `build.gradle.kts`; the specific dep drops; other deps in the same file continue |
| `UnknownConfiguration { path, config }` | `build.gradle.kts` declares a dep using a non-listed Gradle dep-configuration | `tracing::debug!`; the dep is captured normally with no lifecycle-scope annotation |

Per Constitution Principle III + FR-009, no error condition aborts the scan. The reader's contract is "warn + skip the bad parts; emit everything else."

## Worked example

Project layout:

```text
my-kmp-lib/
├── settings.gradle.kts         # rootProject.name = "my-kmp-lib", include(":app", ":shared")
├── gradle/
│   └── libs.versions.toml      # [versions] okhttp = "4.12.0", [libraries] okhttp = { module = "com.squareup.okhttp3:okhttp", version.ref = "okhttp" }
├── app/
│   └── build.gradle.kts        # dependencies { implementation(libs.okhttp); testImplementation("io.kotest:kotest-runner-junit5:5.8.0") }
└── shared/
    └── build.gradle.kts        # kotlin { sourceSets { commonMain { dependencies { implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2") } } } }
```

Emits five `PackageDbEntry` records:

1. `pkg:generic/my-kmp-lib@0.0.0` — workspace-root, sbom-tier=source, component-role=workspace-root, source-files=settings.gradle.kts
2. `pkg:generic/app@0.0.0` — main-module, source-files=app/build.gradle.kts (the module's own component; emitted because `include(":app")` declared it)
3. `pkg:generic/shared@0.0.0` — main-module, source-files=shared/build.gradle.kts
4. `pkg:maven/com.squareup.okhttp3/okhttp@4.12.0` — design-tier (no gradle.lockfile), source-files=app/build.gradle.kts (resolved via libs.versions.toml)
5. `pkg:maven/io.kotest/kotest-runner-junit5@5.8.0` — test-scope, design-tier, source-files=app/build.gradle.kts
6. `pkg:maven/org.jetbrains.kotlinx/kotlinx-serialization-json@1.6.2` — design-tier, source-files=shared/build.gradle.kts, `mikebom:kmp-source-set = "[\"commonMain\"]"`

The `--include-declared-deps` flag (auto-on for `--path` scans per clarification Q5) gates whether components 4-6 surface in the emitted SBOM; the workspace-root + main-modules (1-3) emit unconditionally.
