# Quickstart: Gradle Transitive Dependency Resolution Ladder

**Feature**: `235-gradle-transitive-ladder`
**Audience**: waybill operators, SBOM consumers, contributors.

---

## Scenario A — I want the full transitive graph. My host has a JDK.

```bash
waybill sbom scan --path ./my-gradle-project \
    --gradle-resolve \
    --format cyclonedx-json \
    --output sbom.cdx.json
```

**What happens**:
1. Waybill discovers the Gradle project via `build.gradle(.kts)`
   or `settings.gradle(.kts)`.
2. If `./gradlew` is present, waybill spawns
   `./gradlew :sub:dependencies --configuration runtimeClasspath
   --no-daemon` (and testRuntimeClasspath) per subproject.
3. Each subprocess is bounded by a 5-minute timeout by default.
4. Parses the ASCII tree output → components + transitive edges.
5. Emitted SBOM carries `waybill:gradle-resolution-tier =
   "subprocess"` at document scope.

**Expected result**: full transitive dependency tree in
`sbom.cdx.json`. `dependencies[]` array contains parent → child
edges matching what `./gradlew :app:dependencies` reports.

---

## Scenario B — Post-build CI scan. JDK is available but daemon has released.

Same as Scenario A. The `--no-daemon` default (per Clarifications
Q2) means waybill's subprocess starts a fresh JVM — no interference
with any active build state. On a warm cache the JVM is fast even
without daemon.

---

## Scenario C — Airgapped scan. No JDK, but the project was built here recently.

```bash
waybill sbom scan --path ./my-gradle-project \
    --format cyclonedx-json \
    --output sbom.cdx.json
```

Note: NO `--gradle-resolve` flag. US1 is opt-in.

**What happens**:
1. Waybill discovers the Gradle project.
2. US1 is skipped (opt-out).
3. US2 fires: walks `~/.gradle/caches/modules-2/metadata-2.<N>/`,
   finds cached POMs matching the project's declared dependencies,
   reconstructs the graph transitively.
4. Emitted SBOM carries `waybill:gradle-resolution-tier = "cache"`
   at document scope + `waybill:cache-freshness =
   "fresh"|"stale"` per component.

**Expected result**: full transitive tree frozen at last-build
state. Consumers see the freshness annotation and can decide
whether it's authoritative.

---

## Scenario D — Cold clone. No JDK, no cache, no lockfile.

Same command as Scenario C. Everything downgrades to US3.

**What happens**:
1. US1 skipped (opt-out).
2. US2 skipped (`GradleCacheError::CacheAbsent`).
3. m106 lockfile reader finds no `gradle.lockfile` — no entries.
4. US3 fires: parses `build.gradle` / `build.gradle.kts` /
   `settings.gradle(.kts)` / `libs.versions.toml`.
5. Emits ONE COMPONENT per direct dependency declaration. NO
   transitive edges.
6. Document scope carries `waybill:gradle-resolution-tier =
   "static"`.

**Expected result**: SBOM with direct deps only. Consumers see the
tier annotation and know transitives are missing.

---

## Scenario E — Multi-subproject build with mixed availability.

Some subprojects have `./gradlew` reachable, others don't (maybe
some are in a subdirectory that lost its wrapper symlink).

```bash
waybill sbom scan --path ./monorepo \
    --gradle-resolve \
    --format cyclonedx-json \
    --output sbom.cdx.json
```

**What happens**:
1. Per-subproject, waybill tries the ladder:
   - `app` has `./gradlew` → US1 → `subprocess`
   - `legacy-service` no wrapper, has warm cache → US2 → `cache`
   - `experimental` no wrapper, no cache, has `build.gradle.kts`
     → US3 → `static`
2. Aggregate summary: `mixed`.
3. Document-scope annotation: `waybill:gradle-resolution-tier =
   "mixed"`.
4. Per-subproject annotations attach to each Gradle main-module:
   - `pkg:maven/com.example/app` → `waybill:gradle-subproject-tier
     = "subprocess"`
   - `pkg:maven/com.example/legacy-service` →
     `waybill:gradle-subproject-tier = "cache"`
   - `pkg:maven/com.example/experimental` →
     `waybill:gradle-subproject-tier = "static"`

**Expected result**: consumer can filter or aggregate by tier when
producing quality reports.

---

## Scenario F — Gradle subprocess times out.

The wrapper's `distributionUrl` points at an old Gradle version
that isn't cached locally. First invocation spends 8 minutes
downloading the distribution.

```bash
waybill sbom scan --path ./stale-wrapper-project \
    --gradle-resolve \
    --format cyclonedx-json \
    --output sbom.cdx.json
```

**What happens**:
1. US1 spawns `./gradlew` — first invocation stalls on Gradle
   distribution download.
2. At 5 minutes (default), waybill sends SIGTERM to the child;
   after 2s grace, SIGKILL.
3. Ladder degrades to US2 (or US3 if no cache).
4. Document-scope annotations include:
   - `waybill:gradle-resolution-tier = "cache"` (or `"static"`)
   - `waybill:gradle-fallback-reason = "timeout"` (records that
     US1 was tried and timed out)

**Expected result**: scan completes cleanly even when subprocess
hangs. Consumer sees the fallback reason and knows to investigate.

**Operator override**:
```bash
waybill sbom scan --path ./stale-wrapper-project \
    --gradle-resolve \
    --gradle-timeout-secs 900 \
    --format cyclonedx-json \
    --output sbom.cdx.json
```

The 15-minute timeout gives the distribution download time to
finish on the first invocation.

---

## Scenario G — Include buildscript classpath.

```bash
waybill sbom scan --path ./my-gradle-project \
    --gradle-resolve \
    --gradle-resolve-buildscript \
    --format cyclonedx-json \
    --output sbom.cdx.json
```

**What happens**:
1. US1 fires per-subproject × per-configuration as normal.
2. ADDITIONALLY, per subproject, waybill spawns
   `./gradlew :sub:buildEnvironment --no-daemon` (or the equivalent
   `dependencies` under the buildscript scope).
3. Buildscript dependencies emit with
   `waybill:lifecycle-scope = "build"` per the existing m184
   emission path.

**Expected result**: SBOM includes both project runtime deps AND
plugin/buildscript deps.

---

## Scenario H — Additional configurations beyond the defaults.

```bash
waybill sbom scan --path ./my-gradle-project \
    --gradle-resolve \
    --gradle-extra-configurations compileClasspath \
    --gradle-extra-configurations testCompileClasspath \
    --format cyclonedx-json \
    --output sbom.cdx.json
```

**What happens**:
1. Default set (`runtimeClasspath` + `testRuntimeClasspath`) is
   resolved as usual.
2. Additionally, `compileClasspath` + `testCompileClasspath` are
   resolved.
3. Total subprocess invocations: 4 × #subprojects.
4. Each dependency emitted carries a scope annotation reflecting
   which configuration(s) it came from.

**Expected result**: compile-only deps (typically annotations,
processors) appear in the SBOM.
