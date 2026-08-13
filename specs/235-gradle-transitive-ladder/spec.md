# Feature Specification: Gradle Transitive Dependency Resolution Ladder

**Feature Branch**: `235-gradle-transitive-ladder`
**Created**: 2026-08-13
**Status**: Draft
**Input**: User description: "gradle transitive dep-graph ladder — T1 subprocess + T2 cache + T4 static" (implicit continuation of the exploratory design conversation from the m234 close-out)

## Clarifications

### Session 2026-08-13

- Q: Which Gradle configurations should US1 resolve by default? → A:
  `runtimeClasspath` + `testRuntimeClasspath` — matches CISA 2026
  scope + milestone-184 lifecycle-scope emission (test-scope deps
  populate CDX `scope: test` and SPDX `TEST_DEPENDENCY_OF`).
  Additional configurations remain reachable via an opt-in flag
  (plan-phase decides the flag shape).
- Q: Should US1 invoke Gradle with `--no-daemon` by default? → A:
  Yes — `--no-daemon` is the default. Waybill is a scanner; leaving
  daemons alive after a scan surprises operators. Cold-start cost
  (~20-30s / invocation) is dwarfed by the actual `:dependencies`
  execution. Operators who want daemon speed can opt in via a flag
  (plan-phase decides the flag shape).
- Q: Should US1 resolve the buildscript classpath (plugin
  dependencies) by default? → A: No, opt-in only. Buildscript
  classpath is functionally distinct (consumed by the build tool,
  not shipped in the artifact) and doubles subprocess-call cost.
  Default US1 resolves project classpaths only; a separate
  `--gradle-resolve-buildscript` opt-in flag reaches buildscript.
  m106's existing `buildscript-gradle.lockfile` reader remains the
  primary coverage path for buildscript when a lockfile exists
  (per FR-009 non-regression).

## Context (informational)

Waybill's current Gradle support (milestone 106) reads
`gradle.lockfile` + `buildscript-gradle.lockfile` — the flat resolved
dependency list that Gradle emits when `dependencyLocking` is enabled.
When the lockfile exists, the emitted CDX/SPDX Gradle components are
version-accurate but **carry no transitive-edge information**:
consumers cannot answer "which of the runtime dependencies depend on
`log4j:log4j-core`" from the current output.

For Gradle projects that DON'T enable dependency-locking (probably the
majority of open-source projects), waybill emits **zero components**
today. There is no static-parse fallback and no cache-read fallback.

This milestone closes both gaps with a progressively-degrading
resolution ladder — modeled on the Go graph resolver's tier pattern
from milestones 055 / 091 / 160 / 172 (`ResolutionStep::*` with the
`waybill:go-resolution-step` transparency annotation per Principle X)
and the shell-out pattern from milestones 173 (`--warm-go-cache`) and
205 (`cargo metadata`).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Subprocess resolution against a Gradle wrapper (Priority: P1)

The waybill operator is scanning a repository that has a `gradlew` /
`gradlew.bat` wrapper and a working JDK on the scan host. They want
the emitted SBOM to reflect the **actual resolved dependency graph**
including transitive edges, version overrides, BOM effects, and
constraint resolution — the same graph the Gradle build itself would
resolve at compile / runtime.

Waybill invokes the wrapper (`./gradlew :<subproject>:dependencies
--configuration <configName>` for each subproject × configuration
combination) as an opt-in subprocess. The parsed output becomes the
authoritative source for that scan tier.

**Why this priority**: This is the ONLY mechanism that gives us the
same view Gradle itself has. Every other tier is an approximation.
Projects that ship a wrapper (the recommended Gradle convention) get
correct output on first try.

**Independent Test**: Scan a Gradle project that ships a wrapper and
has at least one transitive dependency (e.g., anything that pulls
`spring-boot-starter-web` and transitively `slf4j-api`). Verify the
emitted SBOM contains a `dependsOn` (or SPDX `DEPENDS_ON`) edge from
the direct dep to the transitive dep. Compare against
`./gradlew :app:dependencies` output for the same configuration to
confirm the edges match.

**Acceptance Scenarios**:

1. **Given** a Gradle project with `./gradlew` present + JDK on
   `$PATH` + `--gradle-resolve` opt-in flag, **When** waybill scans
   the project, **Then** the emitted SBOM contains resolved transitive
   edges for every subproject × configuration combination the
   operator opted into.
2. **Given** the same project scanned WITHOUT the opt-in flag,
   **When** waybill runs, **Then** it degrades to a lower tier (US2
   or US3) rather than invoking Gradle by surprise. No implicit
   subprocess spawn.
3. **Given** the subprocess times out (e.g., a hostile build that
   sits in Gradle daemon startup), **When** the configured timeout
   elapses, **Then** waybill kills the subprocess cleanly, emits a
   transparency annotation naming the tier attempt + reason, and
   degrades to the next tier.
4. **Given** the operator scans a multi-subproject build with 40+
   subprojects, **When** waybill invokes Gradle, **Then** it uses a
   single subprocess call per configuration (`./gradlew
   :all:dependencies` or per-subproject batched), not 40 individual
   subprocess calls.

---

### User Story 2 — Local cache reconstruction when a fresh Gradle run isn't possible (Priority: P2)

The waybill operator is scanning a repository on a machine that has
NO JDK (or has one but is offline, or is CI-post-build where the
Gradle daemon has already exited). But the project has been built at
least once on this host — Gradle's local cache at
`~/.gradle/caches/modules-2/` holds resolved artifacts + POMs +
`.module` metadata from the most recent build.

Waybill reads the cache directly (no subprocess), reconstructs the
resolved graph by walking the cached POM / `.module` files, and
emits SBOM components + transitive edges frozen at the last-build
state.

**Why this priority**: This is the natural fallback for post-build CI
scans where waybill runs AFTER Gradle has done its work but the build
step has already released the daemon. Also the natural fallback for
airgapped or minimal-scan-host environments. It's P2 (not P1)
because it only works when the project has been resolved on this
host at least once; a cold-clone still needs US1.

**Independent Test**: Scan a Gradle project on a host that (a) has a
warm `~/.gradle/caches/modules-2/` matching the project's current
declared dependencies, but (b) has NO JDK visible to waybill. Verify
the emitted SBOM contains transitive edges matching what US1 would
have produced.

**Acceptance Scenarios**:

1. **Given** a Gradle project on a host with a warm Gradle cache but
   no JDK, **When** waybill scans, **Then** it reconstructs the
   graph from the cache and emits transitive edges.
2. **Given** the cache is warm for an OLD state (project's
   dependencies have been edited since last build), **When** waybill
   reads the cache, **Then** it emits the cached-state graph AND a
   transparency annotation noting "cache-derived; may not reflect
   current build.gradle state" so consumers can distinguish.
3. **Given** the cache is empty or missing entries for declared
   dependencies, **When** waybill runs, **Then** it degrades to US3
   (static baseline).

---

### User Story 3 — Static baseline for cold-clone / no-tools scans (Priority: P3)

The waybill operator is scanning a freshly-cloned Gradle repository
on a host with NO JDK and NO warm cache — nothing but the source
files. They want at least a **direct-dependency** SBOM so that
downstream tooling has some signal to work with, rather than an
empty output.

Waybill parses `build.gradle` and `build.gradle.kts` files (regex-
scoped DSL extraction like the Pants shell reader from milestone
225), reads `libs.versions.toml` (Gradle Version Catalog, already
handled by milestone 122), reads `settings.gradle(.kts)` to
enumerate subprojects, and emits components for every
`implementation` / `api` / `runtimeOnly` / `testImplementation` /
etc. line it recognizes. **No transitive edges** (that would require
resolution).

**Why this priority**: This is the baseline current-state waybill
emits **nothing** for a Gradle project without a lockfile — even
though the source files themselves declare dependencies clearly. P3
because the value delta (direct-only) is smaller than transitive
edges but non-zero.

**Independent Test**: Scan a Gradle project on a host with no JDK,
no warm cache, and no `gradle.lockfile`. Verify waybill emits at
least one component for every explicit `implementation(...)` line
in the project's `build.gradle` / `build.gradle.kts` files.

**Acceptance Scenarios**:

1. **Given** a Gradle project with no lockfile, no cache, no JDK
   available, **When** waybill scans, **Then** it emits components
   for every direct dependency declared in `build.gradle(.kts)`.
2. **Given** the project uses a Gradle Version Catalog
   (`libs.versions.toml`), **When** waybill scans, **Then** it
   resolves version references (e.g., `libs.spring.boot` →
   `org.springframework.boot:spring-boot:3.2.0`) via the existing
   milestone-122 lookup table.
3. **Given** the operator scans a multi-subproject build, **When**
   waybill runs the static parser, **Then** subproject enumeration
   comes from `settings.gradle(.kts)` `include(...)` lines.

---

### User Story 4 — Transparency: consumers can tell which tier fired (Priority: P2)

The SBOM consumer downstream (auditor, vulnerability scanner,
compliance reviewer) needs to know **whether the emitted graph
came from Gradle itself (US1), from the local cache (US2), or from
static parse (US3)** — because the accuracy + freshness guarantee
differs by tier.

Waybill emits a document-scope annotation `waybill:gradle-resolution
-tier` on every scan that touches at least one Gradle project. The
annotation value is one of: `subprocess`, `cache`, `static`,
`lockfile-only` (the m106 legacy path when no ladder tier ran), or
`mixed` (different subprojects resolved via different tiers).

**Why this priority**: Same rationale as milestone 160's
`waybill:go-resolution-step` — Principle X (Transparency) requires
that limitations be surfaced structurally, not implicit. Consumers
can't act on data they can't assess.

**Independent Test**: Scan a project via each of US1 / US2 / US3
and verify the emitted SBOM's document-scope annotation names the
correct tier. Scan a project where subprojects resolve via
different tiers (some have wrappers, some don't) — verify the
annotation is `mixed`.

**Acceptance Scenarios**:

1. **Given** any Gradle project scan, **When** waybill emits the
   SBOM, **Then** the document scope contains
   `waybill:gradle-resolution-tier` with one of the enumerated
   values.
2. **Given** a multi-subproject scan where subprojects resolved via
   different tiers, **When** the SBOM emits, **Then** the tier is
   `mixed` AND per-subproject annotations name the specific tier
   for each subproject.
3. **Given** a US1 scan that timed out and fell back to US2,
   **When** the SBOM emits, **Then** the tier annotation names US2
   AND a secondary annotation records the US1 attempt + reason for
   fallback.

---

### Edge Cases

- **`./gradlew` present but no JDK on `$PATH`** — subprocess fails
  with a clear diagnostic; degrade to US2 or US3.
- **Gradle wrapper's `distributionUrl` points at a version the
  daemon hasn't downloaded yet** — first `./gradlew` call spends
  minutes downloading the distribution. Timeout handling must
  differentiate "still downloading Gradle" vs "build hung".
- **The project has a build script (`buildSrc/` or `build-logic/`)
  that resolves plugins on invocation** — subprocess call resolves
  buildSrc as a side effect, which we don't want counted. Solution:
  target `--configuration <specific>` per US1 so the resolution
  scope is bounded.
- **Cache holds artifacts for MULTIPLE resolved versions** (Gradle
  keeps historical copies) — US2 needs to pick the one currently
  referenced by the project's declared deps, not the most recent
  cache entry.
- **`settings.gradle` uses `include` with a variable / expression**
  (dynamic subproject enumeration) — US3 skips the subproject with
  a warn-level log; US1/US2 don't care (Gradle itself resolves the
  expression).
- **A single scan touches both a lockfile-having subproject
  (m106 path) and a lockfile-lacking subproject (ladder path)** —
  ladder tier is `mixed`; per-subproject annotation names each
  path.
- **Cache-derived graph is stale relative to current
  `build.gradle`** — US2 emits `waybill:cache-freshness = stale`
  when the timestamps disagree; consumers can filter accordingly.
- **The subprocess emits unparseable output** (e.g., Gradle
  version-format change) — subprocess tier fails cleanly with a
  transparency annotation; degrade to US2/US3.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST expose an opt-in flag (`--gradle-resolve`
  or equivalent) that authorizes waybill to invoke
  `./gradlew` / `gradle` subprocesses. Absent the flag, waybill
  MUST NOT spawn a Gradle subprocess.
- **FR-002**: When the opt-in flag is present AND a `gradlew` /
  `gradlew.bat` wrapper is discoverable, System MUST invoke
  `./gradlew :<sub>:dependencies --configuration <config> --no-daemon`
  for each configured subproject × configuration combination and
  parse the output into components + transitive edges. The
  `--no-daemon` flag is passed unconditionally in the default
  invocation shape (per Clarifications Q2); daemon usage is
  reachable only via an operator-opt-in flag. Default
  configurations resolved are `runtimeClasspath` +
  `testRuntimeClasspath` per subproject (per Clarifications Q1);
  buildscript classpath is NOT resolved by default (per
  Clarifications Q3) — it requires a separate
  `--gradle-resolve-buildscript` opt-in flag.
- **FR-003**: System MUST enforce a configurable subprocess timeout
  (default 5 minutes; overrideable per-scan) and MUST kill the
  subprocess cleanly on timeout, emitting a transparency
  annotation naming the tier + reason for fallback.
- **FR-004**: When US1 is unavailable (no wrapper, no JDK, or
  operator declined the opt-in), System MUST attempt US2 — read
  `~/.gradle/caches/modules-2/metadata-2.*/` for cached resolved
  artifacts + POMs + `.module` metadata files.
- **FR-005**: When US1 + US2 both fail (no cache, or cache missing
  entries for declared deps), System MUST attempt US3 — static
  parse of `build.gradle` / `build.gradle.kts` /
  `settings.gradle(.kts)` / `libs.versions.toml`.
- **FR-006**: System MUST emit a document-scope annotation
  `waybill:gradle-resolution-tier` on every scan that touches at
  least one Gradle project. Value MUST be one of `subprocess`,
  `cache`, `static`, `lockfile-only`, or `mixed`.
- **FR-007**: When the resolution tier is `mixed`, System MUST
  emit per-subproject annotations naming the specific tier used
  for each subproject.
- **FR-008**: When a tier is attempted and falls back, System MUST
  emit a secondary annotation recording the attempted tier + the
  reason for fallback (`timeout`, `missing-tool`, `parse-error`,
  `cache-miss`, `no-source-files`).
- **FR-009**: System MUST NOT break the existing milestone-106
  lockfile-reader behavior. When a `gradle.lockfile` OR
  `buildscript-gradle.lockfile` is present, m106's flat-list
  output MUST continue to be emitted UNCHANGED, and the ladder
  tiers MUST supplement it with transitive-edge information (not
  replace it).
- **FR-010**: US3 static parser MUST support Gradle Version Catalog
  (`libs.versions.toml`) lookups per the existing milestone-122
  infrastructure — a `libs.<key>` reference in `build.gradle(.kts)`
  MUST resolve to the coordinate declared in the catalog TOML.
- **FR-011**: US3 static parser MUST support multi-subproject builds
  via `settings.gradle(.kts)` `include(...)` enumeration.
- **FR-012**: System MUST NOT fetch anything from the network to
  perform Gradle resolution. External-source enrichment (deps.dev,
  etc.) continues to happen after the resolver returns, per the
  existing Principle XII pipeline — the resolver itself is offline.
- **FR-013**: All three tiers MUST emit PURLs in the existing
  `pkg:maven/<group>/<name>@<version>` shape that the m106 lockfile
  reader uses, so downstream dedup + enrichment + reconciliation
  works unchanged.
- **FR-014**: System MUST log at INFO level, once per scan, a
  one-line summary naming which tier fired for each subproject
  (e.g., `gradle-resolver: :app=subprocess, :lib=cache, :tests=static`).
- **FR-015**: When the subprocess writes unparseable output (Gradle
  version-format change, garbled stream), System MUST emit a
  transparency annotation, fall back to the next tier, and NOT
  crash the scan.

### Key Entities

- **Gradle project boundary**: A directory containing a
  `build.gradle` OR `build.gradle.kts` file. Multi-subproject
  builds are recognized via a root-level `settings.gradle(.kts)`
  with `include(...)` lines.
- **Resolution tier**: One of `subprocess`, `cache`, `static`,
  `lockfile-only`, or `mixed` (aggregate) — determined per
  subproject × configuration combination.
- **Configuration**: A Gradle-native concept naming a specific
  classpath (`runtimeClasspath`, `compileClasspath`,
  `testRuntimeClasspath`, etc.). The default set for waybill is
  **`runtimeClasspath` + `testRuntimeClasspath` for each
  subproject** (per Clarifications Q1). Test-scope entries map to
  CDX `scope: test` / SPDX `TEST_DEPENDENCY_OF` via the existing
  milestone-052 / milestone-184 emission path. Additional
  configurations reachable via an opt-in flag whose shape is a
  plan-phase decision.
- **Cache entry**: A directory under
  `~/.gradle/caches/modules-2/metadata-2.*/descriptors/<group>/<artifact>/<version>/`
  holding cached POM / `.module` metadata used by US2 to
  reconstruct the graph without invoking Gradle.
- **Fallback reason**: One of `timeout`, `missing-tool`,
  `parse-error`, `cache-miss`, `no-source-files` — attached to the
  secondary annotation when a tier degrades.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a fixture Gradle project with a wrapper and one
  known-transitive dep (e.g., `spring-boot-starter-web` →
  `slf4j-api`), the US1 subprocess path emits the transitive edge
  in the SBOM. Verified end-to-end via a golden CDX fixture.
- **SC-002**: For a fixture Gradle project scanned with warm cache
  but no JDK, the US2 cache-reader emits the same set of
  transitive edges the US1 path would (byte-equivalent
  `dependencies[]` array modulo the tier annotation).
- **SC-003**: For a fixture Gradle project scanned with no
  lockfile, no cache, no JDK, the US3 static parser emits at least
  one component per direct dependency declared in
  `build.gradle(.kts)`. Absent tier fires zero components today;
  post-fix, at least 90% of declared direct deps are surfaced.
- **SC-004**: Every emitted SBOM that touches a Gradle project
  carries the `waybill:gradle-resolution-tier` document-scope
  annotation. Verified via a parity extractor test asserting the
  annotation is present in every fixture.
- **SC-005**: Subprocess tier timeouts (US1 default 5 min) never
  hang the scan indefinitely — the scan process exits within 6
  minutes of the timeout even if the Gradle daemon refuses to die.

## Post-merge observation targets *(non-blocking)*

These targets are NOT verified by in-repo tasks. They are quality
outcomes the maintainer group observes over the first weeks after
merge and treats as follow-up bugs if missed. Analysis-phase
finding F3 demoted them from Success Criteria because they'd
require a hostile-to-CI external corpus setup (matches Constitution
Pre-PR Verification posture — CI stays fast + hermetic).

- **PO-1** (was SC-006): A scan of a real-world open-source Gradle
  project (`spring-projects/spring-boot`-scale) via US1 completes
  in under 90 seconds on a laptop-class machine with a warm
  Gradle daemon and produces a graph with ≥95% coverage of the
  direct-dep set (verified against `./gradlew :app:dependencies`
  output). Observed manually by the release lead within one week
  of merge; if missed, opens a follow-up performance-tuning
  milestone.

## Assumptions

- The existing milestone-106 lockfile reader stays as the fastest
  path when `gradle.lockfile` is present — the ladder tiers ADD
  transitive edges on top of it, not REPLACE it. This spec's US1
  path also runs on lockfile-having projects to enrich the flat
  list with transitive info.
- The subprocess opt-in flag is per-scan, not per-project. If the
  operator opts in, waybill will invoke Gradle in every discovered
  Gradle project directory during the scan; there's no per-project
  granularity in this milestone.
- The subprocess tier requires a working JDK on `$PATH` — waybill
  does NOT install a JDK, and the Constitution Principle I
  (Pure Rust, Zero C) is unaffected because Java is a runtime
  dependency of the Gradle tool being invoked, not a compile-time
  dependency of waybill.
- The cache reader (US2) reads `~/.gradle/caches/modules-2/` in
  its default location. Custom `GRADLE_USER_HOME` overrides via
  environment variable are respected; other custom cache paths
  are out of scope.
- The static parser (US3) targets Groovy DSL (`build.gradle`) and
  Kotlin DSL (`build.gradle.kts`). Both are line-oriented
  DSL-extraction-friendly (regex-scoped, matches milestone 225's
  Pants shell reader pattern). Complex-Groovy-expression parsing
  (helper methods, dynamic `include`) is out of scope; those
  cases log a warn and skip.
- Network fetching of POMs (the T3 tier we discussed but deferred)
  is a separate future milestone — it would be shared
  infrastructure with the Maven reader and is scoped OUT of m235.
- The `.gradle/` PROJECT cache (per-project, holds resolution
  history for THAT project specifically) is a lower-quality signal
  than the USER cache at `~/.gradle/caches/`. If both exist, US2
  prefers the user cache. If the operator has ONLY the project
  cache warm, that's a lower-tier variant we may or may not
  address (plan-phase decision).
