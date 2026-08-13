# Contract: `static_parser.rs` (US3 static parser)

**File**: `waybill-cli/src/scan_fs/package_db/gradle/static_parser.rs`
**Consumers**: `ladder.rs`, `cache_reader.rs` (calls the direct-dep
extraction step to seed its declared-coords list)

---

## Entry point

```rust
pub fn resolve_via_static_parse(
    project_dir: &Path,
) -> Result<GradleResolvedGraph, GradleStaticError>;
```

## Contract

1. **Enumerate subprojects**: Read `project_dir/settings.gradle`
   OR `project_dir/settings.gradle.kts`. Parse `include(...)` /
   `include ...` lines to extract subproject paths.
   - Recognized Groovy: `include 'app', 'core'`, `include ":app"`.
   - Recognized Kotlin: `include("app", "core")`,
     `include(":app")`.
   - Unrecognized (dynamic expressions, method references): log
     a warn and skip that subproject.

2. **Per subproject**:
   - Look for `build.gradle` OR `build.gradle.kts` in the
     subproject directory.
   - If neither exists, skip the subproject with a warn.
   - Parse the build file (see regex table below).

3. **Regex extraction table** (per research R5):

   | Configuration | Groovy regex | Kotlin regex |
   |---|---|---|
   | `implementation` | `implementation\s+['"]([^'"]+)['"]` | `implementation\(\s*"([^"]+)"\s*\)` |
   | `api` | `api\s+['"]([^'"]+)['"]` | `api\(\s*"([^"]+)"\s*\)` |
   | `runtimeOnly` | `runtimeOnly\s+['"]([^'"]+)['"]` | `runtimeOnly\(\s*"([^"]+)"\s*\)` |
   | `testImplementation` | `testImplementation\s+['"]([^'"]+)['"]` | `testImplementation\(\s*"([^"]+)"\s*\)` |
   | Version catalog | `implementation\s+libs\.(\S+)` | `implementation\(libs\.(\S+)\)` |
   | Platform BOM | `implementation\s+platform\s*\(?\s*['"]([^'"]+)['"]` | `implementation\(\s*platform\(\s*"([^"]+)"\s*\)\s*\)` |
   | Project ref | `implementation\s+project\s*\(\s*[':]([^)']+)[':]\s*\)` | `implementation\(\s*project\(\s*":([^"]+)"\s*\)\s*\)` |

4. **Coord parsing**: For direct string matches, split the captured
   `<group>:<artifact>:<version>` on colon (must have exactly 3
   parts, else log a warn and skip).

5. **Version catalog resolution**:
   - For every `libs.<key>` match, look up `<key>` in
     `libs.versions.toml` at `project_dir/gradle/libs.versions.toml`
     or `project_dir/../gradle/libs.versions.toml`. Reuses m122's
     existing reader.
   - If catalog lookup fails, log a warn and skip.

6. **Platform BOM handling**: DO NOT emit a component for the BOM
   itself. Instead, attach a `waybill:gradle-platform-import`
   annotation to the enclosing subproject's main-module component
   with the BOM coordinate as value.

7. **Project ref handling**: DO NOT emit a component. These are
   intra-project references handled by the multi-subproject
   `SubprojectRoot` graph.

8. **Configuration → EdgeScope mapping**:
   - `implementation`, `api`, `runtimeOnly`, `compileOnly` → `Runtime`
   - `testImplementation`, `testRuntimeOnly`, `testCompileOnly` → `Test`
   - `annotationProcessor`, `kapt`, `ksp` → `Buildscript`

9. **Edges**: US3 emits ONLY component entries, NO transitive edges
   (that's US1/US2's job). The returned `GradleResolvedGraph.edges`
   is empty.

10. **Success**: return `GradleResolvedGraph { tier: Static, ... }`.

## Failure modes

| Condition | Outcome |
|---|---|
| No `build.gradle(.kts)` found in scan | `GradleStaticError::NoSourceFiles` |
| `settings.gradle(.kts)` has unparseable `include(...)` expression | Warn + skip that subproject; do not fail overall |
| `libs.versions.toml` reference to non-existent key | Warn + skip that dep |
| Regex captures a `group:artifact` without version | Log warn; skip that dep (no defaulting) |

## Test hooks

- Regex extraction covered by unit tests against hand-crafted
  fixture strings.
- Version catalog resolution covered against fixture
  `libs.versions.toml` files.
