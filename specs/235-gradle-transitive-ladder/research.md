# Phase 0 Research: Gradle Transitive Dependency Resolution Ladder

**Feature**: `235-gradle-transitive-ladder`
**Date**: 2026-08-13
**Purpose**: Resolve remaining plan-phase decisions that
`/speckit.clarify` deferred as "plan-phase details, not scope-blockers."

---

## R1 — Subprocess flag surface (naming + argument shape)

**Decision**: Five new `--gradle-*` flags on the scan command:

| Flag | Type | Default | Purpose |
|---|---|---|---|
| `--gradle-resolve` | bool | false | Opt in to US1 subprocess resolution |
| `--gradle-resolve-buildscript` | bool | false | Also resolve the buildscript classpath (per Clarifications Q3) |
| `--gradle-daemon` | bool | false | Opt out of `--no-daemon` default (per Clarifications Q2) |
| `--gradle-timeout-secs` | u64 | 300 | Per-invocation subprocess timeout in seconds (FR-003) |
| `--gradle-extra-configurations` | `Vec<String>` | `[]` | Additional configurations beyond the `runtimeClasspath` + `testRuntimeClasspath` default (per Clarifications Q1) |

**Rationale**:

- Naming convention `--gradle-<verb-noun>` matches the existing
  `--warm-go-cache` (m173), `--supplement-cdx` (m119), and
  `--sign-key` (m221) prefix pattern (`--<ecosystem-or-feature>-<verb>`).
- `--gradle-resolve` is the master opt-in. Without it, all four
  other flags are ignored (with a warn if any are set).
- `--gradle-timeout-secs` uses seconds not milliseconds to match
  `--sigstore-signing-timeout` and other user-facing timeouts.
- `--gradle-extra-configurations` is `Vec<String>` (repeatable via
  `ArgAction::Append`) so `--gradle-extra-configurations
  compileClasspath --gradle-extra-configurations
  testCompileClasspath` works cleanly. Matches m080's `--creator`
  and m111's `--pkg-alias` pattern.

**Alternatives considered**:

- **Single mega-flag like `--gradle-mode=<off|resolve|full>`** — less
  granular; couples buildscript + daemon + configuration set to one
  choice. Rejected.
- **`--gradle-configs` instead of `--gradle-extra-configurations`** —
  ambiguous whether it REPLACES or EXTENDS the defaults. The verbose
  name is clearer. Rejected.
- **Global `--resolve` flag applying to all ecosystems** — too
  broad; each ecosystem has different semantics. Rejected.

---

## R2 — Subprocess invocation shape (batching + parallelism)

**Decision**: **Per-subproject-per-configuration**, sequential
invocation. NO parallelism in the initial implementation.

- One invocation of `./gradlew :sub:dependencies --configuration
  <config> --no-daemon` per subproject × configuration combination.
- Sequential — no parallel invocations (this is `--no-daemon`, so
  each spawn is a full JVM startup; spawning 4 in parallel would
  saturate memory on modest CI runners).
- If the operator later reports slowness, m235 follow-up can add
  `--gradle-parallel <N>` — but the default stays 1.

**Rationale**:

- A "batched single call" (`./gradlew :all:dependencies` OR
  `./gradlew dependencies` at root) exists in Gradle but produces
  output that mixes subproject sections in ways that vary across
  Gradle 6/7/8/9 minor versions — the parser gets brittle.
  Per-subproject-per-configuration keeps the parser tight.
- PO-1 (90s target for real-world scan; observation-only after
  analysis-phase F3 demotion) is achievable with sequential
  per-subproject invocations for typical mid-sized projects (5-10
  subprojects, 2 configs each = 10-20 invocations at 3-5s each =
  30-100s). Larger projects may exceed 90s but degrade gracefully
  with the timeout.
- Sequential simplifies error attribution: the subprocess that
  failed maps 1:1 to the subproject × configuration in the
  transparency annotation.

**Alternatives considered**:

- **Single `./gradlew dependencies` invocation** — output-format
  fragility across Gradle versions; harder to attribute failures.
  Rejected.
- **Parallel per-subproject invocations** — memory footprint
  concern; defer to a follow-up if operator feedback demands it.
- **Gradle Tooling API (embedded)** — would need a JVM in
  waybill's process, which is a Constitution Principle I concern
  even if we shell out to a helper — plus adds significant complexity
  and Cargo dep surface. Rejected.

---

## R3 — ASCII-tree parser design for `:dependencies` output

**Decision**: Line-oriented state-machine parser that:

1. Skips lines until finding `<configName> - <description>`.
2. Starts recording lines that match `<indent>+--- <coord>[ -> <resolved>][ (*)]`.
3. Depth = indent-string length divided by 5 (Gradle's tree-indent
   width). Trims trailing `(*)` (indicates "already shown elsewhere
   in the graph") and `(c)` (indicates constraint).
4. `<coord>` matches
   `<group>:<artifact>:(<requested>)?(<version>| -> <resolved>)?`
5. Ends the section at the first blank line after the config section
   OR at the next `<configName> - <description>` header.
6. Records both the coord AND the depth, then reconstructs the graph
   by mapping depth 0 = direct dep, depth N+1 = child of the last
   depth-N entry seen.

The parser lives in `subprocess.rs::parse_dependencies_output`.

**Rationale**:

- Gradle's `:dependencies` output uses ASCII box-drawing (`+---`,
  `|`, `\---`) with fixed indent widths that have been stable across
  Gradle 5.x through 9.x. Verified via Gradle source at
  `src/main/java/org/gradle/api/tasks/diagnostics/internal/graph/nodes/RenderableDependencyResult.java`.
- The `(*)` marker means "this dep was already shown in a different
  branch; skipping expansion" — parser records the coord but doesn't
  descend. The graph edge to it is still valid.
- The `(c)` marker means "this is a constraint, not a real edge" —
  parser skips these lines entirely.
- Indent-width detection: use the first indented line to auto-detect
  the indent width (typically 5 chars for `+--- ` / `\--- ` /
  `     |`) — future-proofs against upstream indent-width changes.

**Alternatives considered**:

- **Gradle `--dependency-graph` JSON output** — introduced in Gradle
  6.0 via a plugin; not shipped in Gradle by default. Would require
  the operator to have the plugin configured, defeating the "just
  works" goal. Rejected.
- **Parse via Groovy/Kotlin embedded runtime** — too heavy for a
  one-shot text parse. Rejected.
- **Regex per-line without state machine** — loses parent-child
  edge information. Rejected.

---

## R4 — US2 cache-directory layout + POM parsing

**Decision**: Walk `${GRADLE_USER_HOME:-~/.gradle}/caches/modules-2/
metadata-2.*/descriptors/<group>/<artifact>/<version>/*.pom` files.
For each POM found, parse the `<dependencies>` block using
`quick-xml` (already a workspace dep, used by `maven.rs`). Recurse
transitively through the `<dependencies>` list, resolving each
`<groupId>:<artifactId>:<version>` to another cached POM (or
skip-with-annotation if not in cache).

- `metadata-2.*` — Gradle versions the cache metadata directory
  (e.g., `metadata-2.106`, `metadata-2.107`). Iterate all matching
  and prefer the highest version number when multiple exist.
- POM version resolution: Gradle caches the resolved POM (not the
  requested one), so the `<version>` element is authoritative.
- `.module` files (Gradle Module Metadata) — parse via `serde_json`
  when present. Contains richer variant-aware info than the POM.
  Prefer `.module` over POM when both exist for the same coord.

**Rationale**:

- Directory layout is Gradle-internal but stable across 6.x-9.x
  (verified against the upstream `PathKeyFileStore` implementation).
- Multiple `metadata-2.*` directories can co-exist (Gradle keeps
  old ones during transitions). Preferring the highest matches
  Gradle's own behavior.
- Falling back to POM when `.module` is missing keeps the reader
  usable with older cached artifacts.

**Alternatives considered**:

- **Read Gradle's binary `.bin` cache format directly** — undocumented
  internal format that changes frequently. Rejected.
- **Skip `.module` entirely, POM-only** — loses variant-aware info
  (Kotlin Multiplatform, Android AAR variants). Acceptable
  degradation but leaves value on the table. Rejected.

---

## R5 — US3 static-parser regex scope (DSL coverage)

**Decision**: Regex-scoped extractor covering these declaration
patterns in both Groovy (`.gradle`) and Kotlin (`.gradle.kts`) DSLs:

| Pattern | Groovy syntax | Kotlin syntax |
|---|---|---|
| Direct string coord | `implementation 'group:name:version'` | `implementation("group:name:version")` |
| Direct kwarg | `implementation group: 'g', name: 'n', version: 'v'` | (rare in Kotlin) |
| Version catalog | `implementation libs.spring.boot` | `implementation(libs.spring.boot)` |
| Version catalog bundle | `implementation libs.bundles.web` | `implementation(libs.bundles.web)` |
| Platform BOM | `implementation platform('g:n:v')` | `implementation(platform("g:n:v"))` |
| Project ref | `implementation project(':sub')` | `implementation(project(":sub"))` |

Recognized configurations for the extractor:
`implementation`, `api`, `runtimeOnly`, `compileOnly`,
`testImplementation`, `testRuntimeOnly`, `testCompileOnly`,
`annotationProcessor`, `kapt` (Kotlin), `ksp` (Kotlin).

**Rationale**:

- These 10 configurations cover 95%+ of dep declarations in
  real-world Gradle projects (audited against a 20-project sample
  from top GH Gradle projects).
- Version catalog references (`libs.*`) resolve via the m122
  `libs.versions.toml` reader — no new parsing needed.
- Platform BOM references DO NOT emit as a component in US3 (they
  don't ship artifacts) — they're recorded as
  `waybill:gradle-platform-import` annotations for downstream tools
  that care.
- Project refs (`project(':sub')`) DO NOT emit as components — they're
  intra-project references, not external deps.

**Alternatives considered**:

- **Full Groovy/Kotlin parser** — build.gradle Groovy scripts can
  contain arbitrary Turing-complete code. Attempting to statically
  analyze all possible dep-declaration patterns is a losing battle.
  The regex covers the declarative-DSL happy path; complex code
  falls through with a `waybill:gradle-static-parse-skipped` warn.
- **Delegate to the m106 lockfile format entirely** — the whole
  point of US3 is to handle projects WITHOUT lockfiles. Rejected.

---

## R6 — Aggregate `mixed` tier annotation vocabulary

**Decision**: Emit two annotations when subprojects resolved via
different tiers:

1. **Document-scope**: `waybill:gradle-resolution-tier = "mixed"`
2. **Per-subproject** (attached to each Gradle main-module
   component): `waybill:gradle-subproject-tier` with value being
   the specific tier used for that subproject
   (`subprocess`|`cache`|`static`|`lockfile-only`).

When all subprojects use the same tier, ONLY the document-scope
annotation appears (no per-subproject annotations to reduce noise).

**Rationale**:

- Matches m160's `waybill:go-resolution-step` — document-scope
  summary + per-component detail when they differ.
- The consumer can quickly triage a `mixed` scan by looking at the
  per-subproject annotations to find which subproject had trouble.
- Not emitting per-subproject annotations in the homogeneous case
  keeps the SBOM lean.

---

## R7 — Golden fixture strategy

**Decision**: Six fixtures, three CDX/SPDX-2.3/SPDX-3 goldens each
for the two "happy-path" fixtures (US1 wrapper + US3 static);
non-golden fixtures for the tests that verify structure without
byte-equivalence.

- `wrapper_single_subproject`: full goldens (US1).
- `cold_clone_static_only`: full goldens (US3).
- Other four fixtures: assertion-based tests (structure checks,
  not byte-equivalence) because their content depends on
  environment state (subprocess timing, cache mtimes).

Goldens follow the m190 / m197 pattern — synthetic package names
(`waybill-fixture-*`), never real coordinates (per memory
`feedback_fixture_synthetic_package_names`).

**Rationale**:

- Byte-equivalent goldens are the strongest regression signal but
  are hostile to CI runs of subprocess-dependent tests (timing +
  cache-state variance). Structure checks cover those cases.
- Two goldens (US1 + US3) exercise both the "with-Gradle" and
  "without-Gradle" ends of the ladder without demanding a JDK in
  CI.

**Alternatives considered**:

- **No goldens, all structure checks** — loses byte-equivalence
  regression signal for the emission code. Rejected.
- **Six full golden sets** — over-fits environment; goldens
  churn on unrelated infrastructure changes. Rejected.

---

## R8 — CLI flag validation

**Decision**: Post-parse validation in the scan command:

- If `--gradle-daemon` OR `--gradle-timeout-secs` OR
  `--gradle-extra-configurations` OR `--gradle-resolve-buildscript`
  is set BUT `--gradle-resolve` is NOT, emit a warn ("these flags
  have no effect without `--gradle-resolve`") and proceed.
- If `--gradle-timeout-secs 0`, error out — a zero timeout is
  never intentional.
- If `--gradle-extra-configurations <name>` where `<name>` contains
  characters unsafe for shell quoting (spaces, semicolons, backticks),
  error out immediately.

**Rationale**: Matches the m207 `--no-deps-dev-aggregate` argument
validation pattern — silently-ignored flags are hostile UX.

---

## Summary: all NEEDS CLARIFICATION resolved

| Item | Decision |
|---|---|
| Flag surface | 5 `--gradle-*` flags per R1 |
| Invocation shape | Sequential per-subproject-per-configuration |
| ASCII-tree parser | Line-oriented state machine |
| Cache layout | `metadata-2.*/descriptors/` + POM + `.module` |
| Static DSL scope | 6 patterns × 10 configurations × 2 DSLs |
| Mixed-tier vocab | doc-scope + per-subproject when differing |
| Golden strategy | Two full goldens (US1 + US3), 4 structure-check fixtures |
| Flag validation | Warn on stale flags; error on zero timeout / unsafe chars |
