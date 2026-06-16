# Implementation Plan: Kotlin + Swift Ecosystem Readers

**Branch**: `122-kotlin-swift-readers` | **Date**: 2026-06-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/122-kotlin-swift-readers/spec.md`

## Summary

Two coordinated new ecosystem readers shipped under one milestone:

**Swift Package Manager reader** (`mikebom-cli/src/scan_fs/package_db/swift/`) — parses `Package.resolved` lockfiles (JSON, schema-version-dispatched across SwiftPM 1/2/3) and emits one component per `pins[]` entry as `pkg:swift/<host>/<namespace>/<name>@<version>` per the [purl-spec swift type](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst#swift). Commit-pinned mode (no `state.version`) uses the FULL 40-char revision SHA as the version segment (clarification Q1). `Package.swift` is detected but never parsed (clarification Q3); workspace-member emission from manifest content is deferred.

**Kotlin DSL Gradle reader** (`mikebom-cli/src/scan_fs/package_db/kotlin_dsl/`) — regex-extracts dependency declarations from `build.gradle.kts` (and `settings.gradle.kts` workspace topology) + resolves `libs.<alias>` references against `gradle/libs.versions.toml` version catalogs. Emits `pkg:maven/<group>/<name>@<version>` per the existing milestone-106 `pkg:maven/` lane. Multi-module workspaces emit a synthetic `pkg:generic/<rootProject.name>@0.0.0` workspace-root component (clarification Q4) and one main-module per `include(":module")` entry. KMP source-set provenance rides a JSON-encoded array under `mikebom:kmp-source-set` (clarification Q2). `build.gradle.kts`-only-discovered components are design-tier (`mikebom:sbom-tier = "design"`) gated by the existing `--include-declared-deps` flag (clarification Q5).

**Polyglot composition** — both readers integrate via the existing `scan_fs/package_db::read_all` dispatcher (the same shape `gradle::read` and `cargo::read` already use), so KMP polyglot monorepos containing both Android-side `build.gradle.kts` and iOS-side `Package.swift` + `Package.resolved` produce one SBOM with both `pkg:maven/...` and `pkg:swift/...` PURLs side-by-side without per-ecosystem coordination.

**Technical approach** — additive only, mirrors milestone-106's `gradle/` + `nuget/` + `pip/` module layout. Zero new Cargo dependencies (uses existing `serde_json` + `toml` + `regex`). The new readers respect milestone-113 `--exclude-path` and milestone-114 `safe_walk` invariants. One new annotation key (`mikebom:kmp-source-set`) adds one Principle V audit row (C68) to `docs/reference/sbom-format-mapping.md`. Cross-platform (no `#[cfg(unix)]` gating); Windows CI lane unchanged.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–121; no nightly required for this user-space-only ecosystem-expansion work).
**Primary Dependencies**: Existing only — `serde` + `serde_json` (`Package.resolved` JSON parsing — already used by every JSON-format reader), `toml = "0.8"` (`libs.versions.toml` parsing — already used by `cargo.rs` + `pip/`), `regex` (`build.gradle.kts` dep-declaration extraction — already used by milestone-106 + milestone-119), `tracing` (parse-error warnings), `anyhow` (error propagation). The PURL construction reuses `mikebom_common::types::purl::Purl::new()` which already supports the `pkg:swift/` ecosystem type per the upstream `packageurl` crate. **Zero new Cargo dependencies.**
**Storage**: N/A — all reader state is in-process per scan; the lookup table built from `libs.versions.toml` is constructed at parse time and dies with the scan. Mirrors every milestone since 002.
**Testing**: `cargo +stable test --workspace` covers the new unit tests + integration tests. Fixtures under `mikebom-cli/tests/fixtures/golden_inputs/swift_package_resolved/` (US1), `mikebom-cli/tests/fixtures/golden_inputs/kotlin_dsl_gradle/` (US2), and `mikebom-cli/tests/fixtures/golden_inputs/kmp_polyglot/` (US3). Each fixture is the smallest realistic project shape that exercises the contract. Per-file unit-test coverage of the parsers lives in `#[cfg(test)] mod tests` blocks inside `swift/lockfile.rs` and `kotlin_dsl/{build_script,settings,version_catalog}.rs`.
**Target Platform**: Linux x86_64 + macOS aarch64 + Windows x86_64 (the established workspace CI matrix). Per FR-013 / SC-007 the readers are cross-platform: macOS for Swift dev, Linux for Android Studio + KMP, Windows for KMP's growing Windows-target cohort. No `#[cfg(unix)]` gating. The milestone-101 Windows smoke test acceptance criterion (scan completes within 60s; output is structurally valid) extends to cover both new readers when their fixtures are vendored.
**Project Type**: Single-project Rust CLI (`mikebom-cli/`). Mirrors every prior ecosystem-expansion milestone (002 / 003 / 105 / 106).
**Performance Goals**: The readers are I/O-bound (parse a small handful of files per project). Each parser must add <50ms wall-clock to the scan of a typical project on the kusari-cli fixture baseline (the project's standing perf-bench fixture from milestone 090). The KMP polyglot scenario doubles the work but stays well under the perf-test ≤1.10× overhead budget from milestone 094. The new readers' short-circuit (no `Package.resolved` / no `build.gradle.kts` ⇒ skip) means non-Kotlin / non-Swift scans pay zero cost.
**Constraints**: Backwards-compatible. When neither ecosystem is present in the scan tree, the emitted SBOM is byte-identical to a pre-feature mikebom build per SC-007. The walker-audit gate (milestones 115 / 117) is preserved; readers use `safe_walk` via the existing `walk.rs` entry point so no new audit-allowlist entries are needed. The milestone-113 `--exclude-path` flag is honored uniformly. Per FR-013 no new runtime Cargo deps. Per FR-012 no network calls (lockfile-only parsing).
**Scale/Scope**: 2 new module directories (`swift/`, `kotlin_dsl/`) at ~250–350 LoC each — 4 files per module (parser entry point + each file-format parser + tests). 3 new test fixtures under `mikebom-cli/tests/fixtures/golden_inputs/`. 2 new integration tests (`mikebom-cli/tests/scan_swift.rs` + `mikebom-cli/tests/scan_kotlin_dsl.rs`). 1 new annotation key (`mikebom:kmp-source-set`) requires one new parity-catalog C-row + 3 extractor registrations (CDX + SPDX 2.3 + SPDX 3). 1 new section in `docs/ecosystems.md` covering both ecosystems. 1 new pointer line in the `docs/ecosystems.md` Coverage matrix. Diff size estimate: ~900 LoC production + ~500 LoC tests + ~250 LoC docs.

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | Zero new Cargo deps. The regex-based `build.gradle.kts` parser stays in pure Rust; no shell-out to `kotlinc` / `swiftc`. The clarification Q3 explicitly closes the door on `swift package dump-package` shell-outs (Option C was rejected). |
| II. eBPF-Only Observation | N/A | `sbom scan` path; trace path is untouched. |
| III. Fail Closed | ✓ | Parse failures emit `tracing::warn!` and yield zero components per FR-009; the walk continues on sibling files. NO partial state ever escapes — same fail-closed posture as every existing reader. `Package.swift`-without-`Package.resolved` is documented as a warn-and-skip, not a guess-the-deps degradation. |
| IV. Type-Driven Correctness | ✓ | New `KotlinDslEntry` + `SwiftLockfileEntry` types convert to the existing `PackageDbEntry` shape at reader boundaries. No new `.unwrap()` in production code. Existing `Purl::new()` validation covers PURL identity. |
| V. Specification Compliance | ✓ | The Swift PURL shape (`pkg:swift/<host>/<namespace>/<name>@<version>`) follows the purl-spec stable type definition per FR-014. SPDX 2.3 / SPDX 3 emission uses the existing scanner-side `Vec<PackageDbEntry>` channel — no new format-specific code paths. One new mikebom annotation key (`mikebom:kmp-source-set`) requires a new C68 row in `docs/reference/sbom-format-mapping.md` with a Principle V audit narrative naming the CDX 1.6 / SPDX 2.3 / SPDX 3 native-field gap (no native field expresses "which Kotlin Multiplatform source-set declared this dep"). The audit row follows the C40 component-role and C42 lifecycle-scope precedents. |
| VI. Three-Crate Architecture | ✓ | Lives entirely in `mikebom-cli/`; reuses `mikebom_common::types::purl::Purl` + `mikebom_common::resolution::*`. No new crates. |
| VII. Test Isolation | ✓ | Per-test fixtures under `tests/fixtures/golden_inputs/<ecosystem>/`. Unit tests use in-source `#[cfg(test)] mod tests` with `tempfile::tempdir()` for ad-hoc scenarios. No global state. |
| VIII. Completeness | ✓ | Two ecosystems closed (Kotlin DSL Gradle, SwiftPM); the milestone narrows the "scan a polyglot mobile monorepo" gap mikebom currently has. |
| IX. Accuracy | ✓ | PURL versioning is exact (lockfile-pinned for Swift; manifest-declared for Kotlin design-tier). Commit-pinned mode preserves the FULL revision SHA on the PURL version segment per Q1 clarification — preserves uniqueness without re-encoding. |
| X. Transparency | ✓ | `mikebom:source-files` carries the path that emitted each component. `mikebom:sbom-tier` distinguishes design-tier `build.gradle.kts` discovery from any lockfile-locked source. The new `mikebom:kmp-source-set` annotation makes target provenance auditable. Parse failures emit `tracing::warn!` naming the file. |
| XI. Enrichment | ✓ | The readers do NOT make network calls (FR-012). Enrichment (deps.dev / ClearlyDefined) continues to apply downstream on the emitted PURLs without per-ecosystem coordination. |
| XII. External Data Source Enrichment | N/A | No external data sources consulted at scan time. |
| Strict Boundary 1 (no lockfile-based discovery extrapolation) | ✓ | The readers parse lockfiles and emit one component per declared entry. No transitive extrapolation beyond what the lockfile already pinned. |
| Strict Boundary 2 (no MITM) | N/A | No network. |
| Strict Boundary 3 (no C code) | ✓ | Pure Rust + existing crates. |
| Strict Boundary 4 (no `.unwrap()` in production) | ✓ | `Result<_, _>` propagation via `?` + `anyhow::Context` per the standing project pattern. Test code uses the standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` gate. |

**Result**: Constitution Check PASSES. No violations. One new C-row (C68 `mikebom:kmp-source-set`) is added to `docs/reference/sbom-format-mapping.md` with full Principle V audit; that's the entire scope of the constitution-relevant changes.

## Project Structure

### Documentation (this feature)

```text
specs/122-kotlin-swift-readers/
├── plan.md                                       # This file (/speckit.plan output)
├── research.md                                   # Phase 0 — 6 implementation decisions
├── data-model.md                                 # Phase 1 — Swift + Kotlin reader entities; lookup tables; KMP tracking; integration with PackageDbEntry
├── contracts/
│   ├── swift-lockfile-format.md                  # Phase 1 — Package.resolved JSON shape + PURL projection rules
│   ├── kotlin-dsl-extraction.md                  # Phase 1 — regex surface contract + workspace topology + libs.versions.toml lookup
│   └── kmp-source-set-annotation.md              # Phase 1 — mikebom:kmp-source-set value shape + C68 Principle V audit
├── quickstart.md                                 # Phase 1 — operator-facing how-to
└── tasks.md                                      # Phase 2 output (/speckit.tasks, NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       ├── mod.rs                            # EXTENDED — register `swift::read` + `kotlin_dsl::read` in the dispatcher (~4 LoC additions near lines 1380-1400)
│   │       ├── swift/                            # NEW MODULE — ~250 LoC across 3 files
│   │       │   ├── mod.rs                        # Reader entry point + safe_walk integration
│   │       │   ├── lockfile.rs                   # Package.resolved JSON schema dispatcher + PURL projection
│   │       │   └── manifest.rs                   # Package.swift PRESENCE detection only (FR-002 — no content parsing)
│   │       └── kotlin_dsl/                       # NEW MODULE — ~350 LoC across 4 files
│   │           ├── mod.rs                        # Reader entry point + workspace orchestration
│   │           ├── build_script.rs               # build.gradle.kts dep-declaration regex extraction
│   │           ├── settings.rs                   # settings.gradle.kts include(...) parsing + rootProject.name
│   │           └── version_catalog.rs            # libs.versions.toml TOML parser + lookup table
│   └── parity/
│       └── extractors/
│           ├── cdx.rs                            # EXTENDED — c68_cdx extractor for mikebom:kmp-source-set (~3 LoC)
│           ├── spdx2.rs                          # EXTENDED — c68_spdx23 extractor (~3 LoC)
│           ├── spdx3.rs                          # EXTENDED — c68_spdx3 extractor (~3 LoC)
│           └── mod.rs                            # EXTENDED — register C68 in the table (~6 LoC)
├── tests/
│   ├── scan_swift.rs                             # NEW — US1 integration tests (~250 LoC across 5-7 tests)
│   ├── scan_kotlin_dsl.rs                        # NEW — US2 integration tests (~300 LoC across 7-9 tests)
│   ├── scan_kmp_polyglot.rs                      # NEW — US3 integration tests (~150 LoC across 3 tests)
│   └── fixtures/
│       └── golden_inputs/
│           ├── swift_package_resolved/           # NEW FIXTURE — minimal Package.swift + Package.resolved (v2 schema)
│           ├── kotlin_dsl_gradle/                # NEW FIXTURE — minimal build.gradle.kts + libs.versions.toml + settings.gradle.kts
│           └── kmp_polyglot/                     # NEW FIXTURE — androidApp/ + iosApp/ + shared/ minimal layout
docs/
├── ecosystems.md                                 # EXTENDED — new "## kotlin" + "## swift" sections + coverage-matrix entries (~150 LoC)
├── reference/
│   └── sbom-format-mapping.md                    # EXTENDED — ONE new C68 row for `mikebom:kmp-source-set` (~80 LoC of audit narrative)
└── user-guide/
    └── cli-reference.md                          # UNCHANGED — no new flags introduced
mikebom-common/                                    # UNCHANGED
mikebom-ebpf/                                      # UNCHANGED
```

**Structure Decision**: Single-project Rust CLI layout matching every ecosystem-expansion milestone (002 / 003 / 105 / 106). Each new ecosystem gets its own sub-module under `scan_fs/package_db/` consistent with the existing `gradle/` + `nuget/` + `pip/` + `golang/` layout. Per spec FR-010 + the milestone-106 precedent, this gives reviewers a per-ecosystem mental model + bounded LoC per reviewable unit.

## Complexity Tracking

No constitution violations. Two design choices worth flagging because they pre-empt likely review questions:

1. **`Package.swift` detection but no parsing** (Decision driven by clarification Q3). Operators expecting workspace-member emission from `Package.swift` content (`.package(path: "../shared")`) will see those entries missing in v0.1. The plan tracks this as a deferred phase-2 item; the spec's Assumptions section documents the rationale (executable Swift code + safety boundary). Alternative B (regex-based shallow `Package.swift` parser) was rejected because (a) `Package.swift` syntax is unconstrained Swift, (b) operators using local-path deps typically also use SwiftPM workspace mode where `Package.resolved` enumerates the resolved set, (c) the lockfile-only path covers the dominant operator workflow.

2. **Regex-based `build.gradle.kts` extraction** (Decision committed in Technical Context + spec Assumptions). Kotlin DSL is a full Kotlin expression language; a regex parser cannot handle deps declared via meta-programming (heavy `apply<>`, custom DSL extensions, reflection). The plan accepts the documented "common surface syntax only" contract; operators writing exotic dep declarations get `tracing::warn!` + degraded SBOM. Alternative (vendoring a tree-sitter Kotlin grammar or shelling out to `kotlinc -script` for dump-deps) was rejected: tree-sitter is C code, `kotlinc` adds a JVM dependency at scan time. Both violate Principle I + Strict Boundary 3.

These are documentary trade-offs, not constitution violations.
