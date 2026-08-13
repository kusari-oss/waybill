# Implementation Plan: Gradle Transitive Dependency Resolution Ladder

**Branch**: `235-gradle-transitive-ladder` | **Date**: 2026-08-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/235-gradle-transitive-ladder/spec.md`

## Summary

**Primary requirement**: Extend the existing milestone-106 Gradle
lockfile reader with a **progressively-degrading resolution ladder**
that recovers transitive-edge information (US1 subprocess), works
without a JDK when a cache is warm (US2), and emits at least
direct-dep components when nothing else is available (US3) — with
per-scan transparency annotations (US4) so consumers know which tier
fired.

**Technical approach**:

- **US1 (Subprocess)** — Opt-in via `--gradle-resolve`. When enabled
  AND a `gradlew` / `gradlew.bat` wrapper is discoverable, spawn
  `./gradlew :<sub>:dependencies --configuration <config> --no-daemon`
  per subproject × configuration combination. Default configurations
  are `runtimeClasspath` + `testRuntimeClasspath` (per Clarifications
  Q1). Additional configs reachable via `--gradle-extra-configurations
  <list>`. Daemon mode reachable via `--gradle-daemon`. Buildscript
  classpath reachable via `--gradle-resolve-buildscript`. Timeout
  default 5 min (`--gradle-timeout-secs 300`); on timeout, kill the
  subprocess cleanly and degrade to the next tier. Parse the ASCII
  tree output into a typed struct + graph edges. Reuses the
  `std::process::Command` subprocess-with-timeout pattern from
  `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs` (m053) and
  `warm_cache.rs` (m173).

- **US2 (Cache reader)** — Walks `${GRADLE_USER_HOME:-~/.gradle}
  /caches/modules-2/metadata-2.*/descriptors/<group>/<artifact>/
  <version>/` and reads the cached POM + `.module` JSON metadata for
  each declared dependency. Reconstructs the graph by transitively
  walking `<dependency>` entries in POMs. Runs when US1 didn't fire
  (opt-out, missing tool, or timeout). Emits a
  `waybill:cache-freshness` annotation comparing cache-entry mtime
  against `build.gradle(.kts)` mtime — `fresh` if cache is newer,
  `stale` otherwise.

- **US3 (Static baseline)** — Regex-scoped DSL extractor for
  `build.gradle` (Groovy) + `build.gradle.kts` (Kotlin) — mirrors
  the milestone-225 Pants shell reader pattern. Recognized
  declarations: `implementation`, `api`, `runtimeOnly`,
  `compileOnly`, `testImplementation`, `testRuntimeOnly`,
  `annotationProcessor`. Multi-subproject enumeration via
  `settings.gradle(.kts)` `include(...)` line parsing. Version
  catalog references (`libs.<key>`) resolved via the existing
  milestone-122 `libs.versions.toml` reader. Emits components ONLY;
  no transitive edges (that's what US1/US2 are for).

- **US4 (Transparency)** — Every emitted SBOM that touches at least
  one Gradle project carries a document-scope annotation
  `waybill:gradle-resolution-tier` with value `subprocess`, `cache`,
  `static`, `lockfile-only` (m106 legacy path), or `mixed`.
  Per-subproject annotations `waybill:gradle-subproject-tier` name
  the specific tier per subproject when the aggregate is `mixed`.
  A secondary annotation `waybill:gradle-fallback-reason` records
  the cause of tier degradation. Reuses milestone-160's
  `waybill:go-resolution-step` pattern verbatim.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from
milestones 001–234; no nightly required for this user-space-only
reader-extension work).

**Primary Dependencies**: Existing only — `std::process::Command`
(subprocess spawn + timeout, pattern at `golang/mod_why.rs:154-173`
and `golang/warm_cache.rs`), `std::sync::mpsc` (subprocess-with-timeout
handshake, m055 pattern), `regex` (workspace; already used pervasively
for DSL extraction), `scan_fs::walk::safe_walk` (m054/m114 substrate;
no new `walkdir` crate), `quick-xml = "0.31"` (workspace; POM parsing
in US2 — same crate `maven.rs` uses), `serde` / `serde_json` (`.module`
JSON parsing in US2), `toml = "0.8"` (workspace; `libs.versions.toml`
reuse from m122), `tracing`, `anyhow`, `thiserror`, `clap` (new flags
via derive). **Zero new Cargo dependencies.**

**Storage**: N/A — all state is in-process for the duration of a
single scan. Mirrors every reader milestone since 002.

**Testing**:

- **Unit tests** — Groovy + Kotlin DSL regex extraction against
  hand-crafted fixture strings; version-catalog lookup;
  settings.gradle include-parsing; POM-cache traversal; ASCII-tree
  parser for the `./gradlew :sub:dependencies` output.
- **Integration tests** — full-scan against fixture Gradle projects:
  1. `wrapper_single_subproject` — has `./gradlew` + one subproject
     + one transitive dep. Verifies US1 emits the transitive edge.
  2. `wrapper_multi_subproject` — `./gradlew` + multi-subproject
     `settings.gradle`. Verifies subproject enumeration.
  3. `no_wrapper_with_lockfile` — no wrapper, `gradle.lockfile`
     present. Verifies m106 non-regression (FR-009).
  4. `no_wrapper_warm_cache` — no wrapper, warm cache. Verifies US2.
  5. `cold_clone_static_only` — no wrapper, no cache, no lockfile.
     Verifies US3 emits direct deps.
  6. `mixed_tier` — multi-subproject where different subprojects
     resolve via different tiers. Verifies `mixed` annotation.
- **Subprocess timeout test** — synthetic Gradle project that spawns
  a wrapper script pretending to be `./gradlew` and sleeps 30s;
  verifies waybill's 5-min timeout works AND the fallback fires.
- **Golden fixtures** — CDX + SPDX 2.3 + SPDX 3 goldens for the
  `wrapper_single_subproject` fixture (US1 path). Regenerable via
  `WAYBILL_UPDATE_CDX_GOLDENS=1` etc. per convention.
- **Parity catalog** — new C-row `waybill:gradle-resolution-tier`
  with symmetric-equal directionality across all three formats.

**Target Platform**: Linux + macOS (both ship POSIX `./gradlew`).
Windows via `.\gradlew.bat` (matches m100 host-portability posture);
US3 static parser is platform-independent; US2 cache reader works
identically on all three (Gradle uses the same `~/.gradle/` layout).

**Project Type**: Rust library extension — modifies
`waybill-cli/src/scan_fs/package_db/gradle/` (adds several sibling
modules alongside `lockfile.rs`), extends `waybill-cli/src/resolve/`
if a resolver-chain integration is needed for edge synthesis. No new
crates.

**Performance Goals**: PO-1 (post-merge observation, spec §Post-merge
observation targets) targets ≤90s for a real-world project
(`spring-projects/spring-boot`-scale) on a laptop-class host with
warm daemon — observed, not tested in CI. SC-005 (in-repo verified)
bounds subprocess timeout at 5 min default, 6 min ceiling for scan
exit.

**Constraints**:

- Zero new Cargo dependencies (workspace `Cargo.toml` untouched).
- No changes to Rust MSRV.
- No changes to nightly channel (this milestone is user-space stable
  only).
- No network access from the resolver itself (FR-012); external
  enrichment (deps.dev) continues to happen downstream unchanged.
- Subprocess execution is opt-in only (FR-001); no implicit
  `./gradlew` spawn.
- FR-009 non-regression: existing m106 lockfile-reader output MUST
  remain unchanged when a lockfile is present. The ladder tiers
  supplement, not replace.
- Constitution Principle I: the JDK is a runtime prerequisite of the
  invoked Gradle wrapper, NOT a compile-time dependency of waybill.
  No C source added, no libbpf bindings, no C toolchain in waybill's
  build pipeline (matches the m173 `go`-shell-out precedent and m053
  `git describe` precedent).

**Scale/Scope**: Adds ~1500 LOC of Rust (subprocess ladder + parsers
+ cache reader + static extractor + emission wiring + tests). No
Cargo.toml changes.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Waybill Constitution v2.1.0 principles evaluated:

- **I. Pure Rust, Zero C** — No new C source, no libbpf bindings, no
  new C compiler toolchains. Java is a runtime prerequisite of the
  Gradle wrapper being invoked (not a compile-time dependency of
  waybill). Matches m173's shell-out to `go` and m053's shell-out to
  `git describe`. ✅
- **II. eBPF-Only Observation** — Not applicable. This milestone
  extends the filesystem-scan path (`waybill sbom scan`) via package
  DB readers, not the eBPF trace path. Consistent with every reader
  milestone since 002. ✅
- **III. Fail Closed** — The tier ladder embodies fail-closed
  behavior: each tier can fail cleanly with a documented reason and
  degrade to the next. Zero dependencies from a failed subprocess do
  NOT become zero components — the fallback tiers fill the gap
  (US2 / US3 / m106 lockfile). The `waybill:gradle-fallback-reason`
  annotation surfaces every degradation transparently. ✅
- **IV. Type-Driven Correctness** — All new domain values use
  existing waybill_common newtypes (`Purl`, `SpdxExpression`).
  Introduces two new enums (`GradleResolutionTier`,
  `GradleFallbackReason`) with `#[derive(Debug, Clone, Copy, PartialEq)]`
  matching the existing `ResolutionStep` pattern at
  `golang/graph_resolver.rs`. Production code uses `anyhow` for
  application errors and `thiserror` for the parser-error enum. ✅
- **V. Specification Compliance** — Emitted PURLs remain
  `pkg:maven/<group>/<name>@<version>` per FR-013. CDX + SPDX 2.3
  + SPDX 3 emissions unchanged. New annotations use the
  `waybill:gradle-*` namespace only where no standards-native
  construct exists (transitive-edge coverage IS captured natively via
  CDX `dependencies[]` + SPDX `DEPENDS_ON`; the tier annotation is
  waybill-native because no standard expresses "which resolution
  method fired"). Standards-native-precedence audit: transitive
  edges → native (CDX dependencies[], SPDX DEPENDS_ON); tier →
  waybill-native (no standard equivalent); fallback reason →
  waybill-native (no standard equivalent). Documented via a new
  entry in `docs/reference/sbom-format-mapping.md` (T-task in the
  tasks phase). ✅
- **VI. Three-Crate Architecture** — No new crates. All new code
  lands in `waybill-cli/src/scan_fs/package_db/gradle/`. ✅
- **VII. Test Isolation** — No new privileged tests. All new unit
  + integration tests run without root or CAP_BPF. Subprocess
  timeout test uses a synthetic wrapper script; no real Gradle
  daemon required to exercise the code path (though optional fixture
  integration tests behind a `WAYBILL_TEST_REAL_GRADLE` env var may
  run real gradle if the operator has a JDK — off in CI). ✅
- **VIII. Completeness** — The entire point of this milestone is to
  increase graph completeness for Gradle projects. Current state:
  zero components for Gradle projects without lockfiles. Post-fix:
  US1 gives full graph, US2 gives full graph frozen at last-build
  state, US3 gives direct deps. Fewer false negatives across the
  board. ✅
- **IX. Accuracy** — Each tier's accuracy claim is surfaced via the
  `waybill:gradle-resolution-tier` annotation (US4). Consumers can
  distinguish `subprocess` (highest accuracy — same view Gradle
  itself has) from `cache` (frozen at last-build) from `static`
  (direct-only, no transitive resolution). ✅
- **X. Transparency** — The tier annotation IS the transparency
  mechanism. Matches m160's `waybill:go-resolution-step` pattern
  verbatim. ✅
- **XI. Enrichment** — deps.dev and PurlDB enrichment continue to
  happen downstream unchanged; the resolver returns a
  `Vec<PackageDbEntry>` that flows through the existing enrichment
  pipeline. Not touched by this milestone. ✅
- **XII. External Data Source Enrichment** — US1 subprocess and US2
  cache reads are external-data-source enrichments (Gradle
  wrapper + Gradle cache respectively). Neither introduces
  components that don't appear in the project's declared dependency
  graph. FR-012 restates the offline requirement. ✅

**Strict Boundaries**:
- **No lockfile-based dependency discovery** — The m106 reader IS
  technically lockfile-based discovery. Constitution Principle II
  says "no static manifest/lockfile parsing... as a dependency
  source" for the TRACE path; the scan_fs path (where m106 lives)
  is a distinct mode. Principle XII permits lockfile parsing for
  enrichment on the trace-mode; the scan-mode path treats lockfiles
  as the primary discovery source per every reader milestone since
  002. m235 preserves this posture: subprocess + cache + static
  reads all live in scan_fs, not trace. ✅
- **No MITM proxy** — unchanged. ✅
- **No C code** — no new C. ✅
- **No `.unwrap()` in production** — new code follows the
  `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention for
  test modules; production paths use `anyhow`/`thiserror`
  throughout. ✅
- **No file-tier duplicates in default mode** — unchanged. ✅

**Pre-PR Verification**: `cargo +stable clippy --workspace
--all-targets` + `cargo +stable test --workspace` both continue to
work. New subprocess-integration test optionally gated behind
`WAYBILL_TEST_REAL_GRADLE=1` env var so unprivileged / no-JDK CI
lanes are unaffected.

**Gate**: PASS. No violations, no waivers needed.

## Project Structure

### Documentation (this feature)

```text
specs/235-gradle-transitive-ladder/
├── plan.md              # This file (/speckit.plan output)
├── research.md          # Phase 0 — subprocess flag surface, cache layout,
│                        #   ASCII-tree parser design, static extractor scope
├── data-model.md        # Phase 1 — GradleResolutionTier / GradleFallbackReason
│                        #   enums + GradleResolvedGraph / SubprojectRoot structs
├── quickstart.md        # Phase 1 — 5 scenarios (opt-in scan, cache-only,
│                        #   static-only, mixed, timeout)
├── contracts/
│   ├── gradle-subprocess.md     # US1 CLI + parser contract
│   ├── gradle-cache-reader.md   # US2 filesystem layout + parser
│   ├── gradle-static-parser.md  # US3 regex scope + DSL coverage
│   └── gradle-annotations.md    # US4 annotation vocabulary
├── checklists/
│   └── requirements.md  # From /speckit.specify (16/16 pass)
└── tasks.md             # Phase 2 output (from /speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/gradle/
├── mod.rs               # MODIFIED — dispatch to lockfile.rs (existing)
│                        #   THEN dispatch to ladder tiers per Gradle project
├── lockfile.rs          # UNCHANGED — m106 reader; ladder supplements this
├── subprocess.rs        # NEW — US1: spawn `./gradlew :sub:dependencies
│                        #   --no-daemon --configuration <c>`; parse ASCII tree
├── cache_reader.rs      # NEW — US2: walk ~/.gradle/caches/modules-2/
├── static_parser.rs     # NEW — US3: regex-scoped DSL extraction for
│                        #   build.gradle + build.gradle.kts +
│                        #   settings.gradle(.kts)
├── version_catalog.rs   # NEW — libs.versions.toml lookup wrapper
│                        #   (reuses m122 kotlin_dsl/version_catalog reader)
├── tier.rs              # NEW — GradleResolutionTier + GradleFallbackReason
│                        #   enums (matches m160's ResolutionStep shape)
└── ladder.rs            # NEW — orchestrator: for each Gradle project
                         #   directory, try US1 → US2 → US3 in order,
                         #   record fallback reasons, aggregate to `mixed`.

waybill-cli/src/generate/
└── gradle_annotations.rs  # NEW — emit waybill:gradle-resolution-tier
                           #   document-scope + per-subproject annotations
                           #   across CDX / SPDX 2.3 / SPDX 3

waybill-cli/src/parity/extractors/
└── gradle_resolution_tier.rs  # NEW — parity extractor for the new C-row

waybill-cli/tests/
├── fixtures/golden_inputs/gradle/
│   ├── wrapper_single_subproject/    # NEW fixture — US1 golden
│   ├── wrapper_multi_subproject/     # NEW fixture — subproject enumeration
│   ├── no_wrapper_with_lockfile/     # NEW fixture — m106 non-regression
│   ├── no_wrapper_warm_cache/        # NEW fixture — US2 golden
│   ├── cold_clone_static_only/       # NEW fixture — US3 golden
│   └── mixed_tier/                   # NEW fixture — mixed-annotation golden
└── gradle_ladder.rs     # NEW integration test suite

docs/reference/
└── sbom-format-mapping.md  # MODIFIED — add C-row for gradle-resolution-tier
                            #   with SymmetricEqual directionality

waybill-cli/src/cli/scan_cmd.rs (extend the existing `ScanArgs`
`#[derive(Args)]` struct with `#[command(flatten)]` on a new
`GradleCliFlags` field — matches the m076 `EnrichArgs` precedent):
    --gradle-resolve                    (bool; opt-in for US1)
    --gradle-resolve-buildscript        (bool; opt-in for buildscript classpath)
    --gradle-daemon                     (bool; opt-out of --no-daemon default)
    --gradle-timeout-secs <N>           (u64; default 300)
    --gradle-extra-configurations <s>   (comma-separated; extends the default
                                         runtimeClasspath + testRuntimeClasspath)
```

**Structure Decision**: This is a Rust-source milestone extending
an existing package_db reader. The `gradle/` subdirectory grows from
2 files (mod.rs, lockfile.rs) to 8 files (adds subprocess,
cache_reader, static_parser, version_catalog, tier, ladder,
gradle_annotations). One new parity extractor, one new golden set,
one new integration test binary. Zero new crates, zero new Cargo
deps.

## Complexity Tracking

*Not required — Constitution Check passed with no violations.*
