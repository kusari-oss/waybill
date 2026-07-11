# Implementation Plan: Maven + Gradle optional-dependency classification

**Branch**: `184-maven-gradle-optional` | **Date**: 2026-07-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/184-maven-gradle-optional/spec.md`

## Summary

Extends the m179+m180+m181+m183 unified optional-dependency classification to the Java ecosystem's two dominant build systems. Both converge on the same `LifecycleScope::Optional` variant but emit DISTINCT derivation-annotation values (`"maven-optional-element"` and `"gradle-compile-only"`) — reflecting the semantic distinction between Maven's `<optional>` element and Gradle's `compileOnly` configuration.

**Technical approach**: Small, additive changes at two isolated code sites in `mikebom-cli/src/scan_fs/package_db/`:

- **maven.rs (US1)** — Extend `PomDependency` struct at line 578 with a new `optional: bool` field; extend `parse_pom_xml` walker at line 689 with an `<optional>` element handler (analogous to the existing `<scope>` / `<type>` handlers at line 761-768); modify the `pom_dep_to_entry` conversion at line 2347 to consult the new field and emit `LifecycleScope::Optional` + `mikebom:optional-derivation = "maven-optional-element"` when appropriate. Scope-wins-over-optional (FR-005) is enforced because the existing `lifecycle_scope_from_maven` at line 36 returns `Some(Test/Build/Runtime)` for explicit `<scope>` values; the new classifier only overrides when the resulting scope is the implicit Runtime (default) AND the `<optional>` flag is set.
- **gradle/lockfile.rs (US2)** — Add a `is_compile_only_shape(configs: &str) -> bool` helper that suffix-checks for `compileClasspath` presence AND `runtimeClasspath` absence in the raw configs list. Modify `read_gradle_lockfile` at line 38 to consult the helper and set `lifecycle_scope = Some(LifecycleScope::Optional)` + emit `mikebom:optional-derivation = "gradle-compile-only"` when the shape matches AND the existing `LifecycleScope::Build` (buildscript) classification is NOT already set. The existing `mikebom:gradle-configurations` annotation at line 119 stays unchanged (transparency preserved).

Zero new production Cargo dependencies. The C122 parity catalog row registered by m179 gains two new expected values (`"maven-optional-element"` + `"gradle-compile-only"`), tracked as a value-set update, not a new catalog row. The docstring at `parity/extractors/cdx.rs:866` already lists both values as placeholders since m179 — m184 makes them real without touching the docstring text.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–183; no nightly required for this user-space-only classification work).

**Primary Dependencies**: Existing only — `quick-xml = "0.31"` (Maven pom.xml parsing; already used pervasively in `maven.rs`), `serde`/`serde_json` (annotation values), `tracing` (info-level classifier-decision logs), `anyhow`/`thiserror` (error propagation). Reuses m179's `LifecycleScope::Optional` variant + `RelationshipType::OptionalDependsOn` + `SpdxRelationshipType::OptionalDependencyOf` + m180's `apply_lifecycle_scope_to_edges` at `mikebom-cli/src/scan_fs/mod.rs:1261`. **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM via the existing `extra_annotations` channel + `lifecycle_scope` field. Matches every milestone since 002.

**Testing**: `cargo +stable test --workspace` (unit + integration tests), `cargo +stable clippy --workspace --all-targets -- -D warnings` (lint). Golden regen via `MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`.

**Target Platform**: Linux + macOS user-space (unchanged from prior milestones). Windows-host smoke covered by milestone 100/101 infra.

**Project Type**: CLI (Rust binary + shared common crate). Existing three-crate architecture: `mikebom-cli`, `mikebom-common`, `xtask`.

**Performance Goals**: Classifier extension adds O(1) per `<dependency>` element (US1) and O(len(configs)) suffix-check per gradle.lockfile line (US2). Both are negligible relative to the existing per-file XML/text parse cost. No new subprocess calls, no new I/O, no network.

**Constraints**: SC-004/SC-005 byte-identity for non-Java fixtures + Java fixtures without m184 signals — the default classifier path (no `<optional>`, no compile-only shape) MUST produce byte-identical `PackageDbEntry` values compared to pre-m184. Golden regens gate this.

**Scale/Scope**: 2 user stories, 2 code sites, 2 distinct derivation-annotation values. Estimated ~25-30 tasks across 6 phases.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Principle I (Pure Rust, Zero C)** — PASS. Zero new Cargo dependencies. Existing `quick-xml` + `serde_json` + `tracing` cover all parsing + annotation-emission needs.

**Principle II (eBPF-Only Observation)** — N/A. m184 is user-space classifier work; `mikebom-ebpf` untouched.

**Principle III (Fail Closed)** — PASS. Every classifier decision has a documented default (no `<optional>` element → false; no compile-only shape → no classification). Malformed pom.xml / gradle.lockfile continues to warn-and-skip (existing behavior at `maven.rs` + `gradle/lockfile.rs:42`).

**Principle IV (Type-Driven Correctness)** — PASS. Classifier decisions flow through the existing `LifecycleScope` enum. No stringly-typed magic. The new `optional: bool` field on `PomDependency` is typed at struct definition time.

**Principle V (Specification Compliance + Native-first)** — PASS. Native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` is the primary signal (elevated from the current `DEPENDS_ON`). `mikebom:optional-derivation` values `"maven-optional-element"` and `"gradle-compile-only"` are the KEEP-BOTH supplements carrying WHICH Java-ecosystem construct produced the classification. Zero new `mikebom:*` annotations invented — the value-set of the existing C122 catalog row is extended.

**Principle VI (Three-Crate Architecture)** — PASS. Changes confined to `mikebom-cli/src/scan_fs/package_db/maven.rs` + `mikebom-cli/src/scan_fs/package_db/gradle/lockfile.rs`. Zero changes to `mikebom-common`, `mikebom-ebpf`, or `xtask`.

**Principle VII (Test Isolation)** — PASS. New unit tests colocated with the code under test in `maven.rs` and `gradle/lockfile.rs`. Integration tests via existing regression fixtures + new fixtures for the two US flavors.

**Principle VIII (Completeness)** — PASS. m184 closes the remaining Java-family filter-parity gap for Maven and Gradle lockfiles. Deferred cases (inherited-optional, DSL parsing, sbt/Mill) documented in spec.md.

**Principle IX (Accuracy)** — PASS. US1 and US2 both correct silent misclassification paths where Maven `<optional>true</optional>` and Gradle `compileOnly` deps have been emitting as Runtime since m052 (Maven scope classifier) shipped. SC-001/002 set-equality gates enforce filter-parity per pico's needs.

**Principle X (Transparency)** — PASS. C122 parity annotation carries the classifier's decision provenance across all three formats byte-identically. The distinct-values design (not merged into one) faithfully identifies WHICH mechanism produced the classification.

**Principle XI (Enrichment)** — N/A. No external-data enrichment for m184.

**Principle XII (External Data Source Enrichment)** — N/A. Same as XI.

**Result**: All 12 principles PASS. No violations to justify. No Complexity Tracking table needed.

## Project Structure

### Documentation (this feature)

```text
specs/184-maven-gradle-optional/
├── plan.md                  # This file
├── research.md              # Phase 0 output (4 decisions)
├── data-model.md            # Phase 1 output (PomDependency extension + classifier decision matrices)
├── quickstart.md            # Phase 1 output (operator + developer worked examples)
├── contracts/
│   ├── classifier-decision-matrix.md  # US1 pom.xml + US2 gradle.lockfile canonical tables
│   └── derivation-value-set.md         # C122 catalog value-set (post-m184: 5 total values)
├── checklists/
│   └── requirements.md      # 16/16 PASS from /speckit-specify
├── spec.md                  # Feature specification
└── tasks.md                 # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           ├── maven.rs               # US1 — PomDependency.optional field, parse_pom_xml handler, pom_dep_to_entry classifier
│           └── gradle/
│               ├── lockfile.rs        # US2 — is_compile_only_shape helper + classifier at read_gradle_lockfile
│               └── mod.rs             # UNCHANGED — dispatch layer
└── tests/
    └── fixtures/
        └── golden/
            ├── cyclonedx/maven.cdx.json    # regen: additive if any pre-m184 fixture happens to contain <optional> or compile-only shape
            ├── spdx-2.3/maven.spdx.json    # regen: net-increment on OPTIONAL_DEPENDENCY_OF where applicable
            └── spdx-3/maven.spdx3.json     # regen: additive annotations
```

**Structure Decision**: Two-file, single-crate scope inside `mikebom-cli/src/scan_fs/package_db/`. No cross-crate coordination. No shared helper file needed — each reader classifies at construction time (analogous to m183 US1 poetry.rs pattern), not via a downstream post-pass. Follows the m179/m180/m181/m183 shape with per-format independence.

## Complexity Tracking

*No violations to justify — all 12 constitution principles PASS.*
