# Implementation Plan: Unified optional-dependency classification across ecosystems

**Branch**: `179-spdx23-transitive-devscope` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/179-spdx23-transitive-devscope/spec.md`

## Summary

Close the pico-reported SBOM filter-parity gap between CycloneDX 1.6 (`scope: "excluded"` on 23 test-only components) and SPDX 2.3 (only 13 emitted as `TEST_DEPENDENCY_OF`, the other 10 emitted as generic `DEPENDS_ON`) by extending mikebom's classifier pass to consider m112's `build_inclusion = NotNeeded` signal in addition to m052's `lifecycle_scope`. Simultaneously introduce a unified `LifecycleScope::Optional` variant (per Q1) so every ecosystem that has a native optional-dep construct (Cargo, npm, pip, Maven, Gradle, Erlang) can populate a SINGLE internal signal that dispatches to SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` (new) and CDX `scope: "excluded"` (existing). New `mikebom:optional-derivation` annotation (per Q2) records the derivation mechanism per Principle V KEEP-BOTH polarity established in m178. SPDX 3.0.1 untouched (no native `optional` in its `LifecycleScopeType` enum).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–178; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `serde`/`serde_json` (JSON I/O across all emitters), `toml = "0.8"` (Cargo.toml + pyproject.toml parsing, already used pervasively), `quick-xml = "0.31"` (Maven pom.xml parsing, already used by `maven.rs`), `regex` (line-format extraction; already a workspace dep), `tracing` (info/warn logs on classifier decisions), `anyhow`/`thiserror` (error propagation). Reuses milestone-052 `apply_lifecycle_scope_to_edges` at `mikebom-cli/src/scan_fs/mod.rs:1261`, milestone-112 `BuildInclusion::NotNeeded` at `mikebom-common/src/resolution.rs:456`, milestone-178 `SpdxRelationshipType` enum extension pattern at `mikebom-cli/src/generate/spdx/relationships.rs`. Milestone-071 parity extractor infrastructure carries the new `mikebom:optional-derivation` annotation as a `SymmetricEqual` catalog entry alongside the existing `mikebom:build-inclusion-derivation`. **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM via the extended `lifecycle_scope` field + `extra_annotations` entries. Matches every milestone since 002.

**Testing**: `cargo +stable test --workspace`. New tests: (a) unit tests on `LifecycleScope::Optional`'s `is_non_runtime()` return + serde round-trip (`mikebom-common/src/resolution.rs`); (b) unit tests on the classifier's new dispatch table arms + precedence rules (FR-014, FR-015) — colocated with the m052 tests in `mikebom-cli/src/generate/spdx/relationships.rs`; (c) integration test per user story with a hand-authored fixture that exercises the new signal end-to-end (`mikebom-cli/tests/optional_dep_classification.rs`). Reuses milestone-090 fixture cache infrastructure where appropriate.

**Target Platform**: Same as every prior mikebom milestone — Linux + macOS user-space, no Windows-specific behavior. The classifier is a pure Rust code path with no platform-specific dependencies. Fingerprint-corpus / eBPF / kernel-scanner code is untouched.

**Project Type**: CLI + library (three-crate workspace: `mikebom-cli`, `mikebom-common`, `mikebom-ebpf` — the last is untouched).

**Performance Goals**: Zero perceptible regression on end-to-end scan wall-clock. The classifier pass runs in `O(n_components + n_edges)` and today measures sub-millisecond on the largest test fixtures. The new dispatch arms add O(1) work per edge; ecosystem readers gain O(1) work per manifest field checked. Fixture SBOM emission size grows only by the new `mikebom:optional-derivation` annotation on affected components (bounded by ecosystem coverage — the annotation adds ~50 bytes per touched component).

**Constraints**: (1) All 8 SC gates from spec.md — SC-004 (CDX byte-identity for un-touched fixtures) and SC-005 (SPDX 3 byte-identity for ALL fixtures) are strict-equality gates that constrain the implementation to avoid unrelated churn on the emission code paths. (2) The `--spdx2-relationship-compat=basic` flag contract (FR-003 + SC-006) must be honored — no new typed-verb emission in basic mode. (3) Principle IV (`Type-Driven Correctness`): all changes route through the newtype `LifecycleScope` enum + `RelationshipType` enum + `SpdxRelationshipType` enum — no raw strings across function boundaries. (4) Principle V (`Standards-native first`): the new `mikebom:optional-derivation` annotation is a Principle V KEEP-BOTH carve-out per m178 precedent; the native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` is the primary signal, the annotation carries derivation source that the standard doesn't natively express.

**Scale/Scope**: 7 user stories, 19 functional requirements, 8 success criteria. Estimated ~35-45 tasks across 6-7 phases (setup + foundational core-model change + US1 pico fix + US2 research artifact + US3-US7 per-ecosystem coverage + polish). Ecosystem coverage in the plan spans 24 mikebom-supported ecosystems (per SC-007's enumeration); each one gets a survey row in `research.md` even if the verdict is "no equivalent construct".

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)**: ✅ PASS. No new C dependencies. Every change is Rust-side.
- **Principle II (eBPF-Only Observation)**: ✅ N/A. m179 is emission-time metadata transformation; no discovery-source changes.
- **Principle III (Fail Closed)**: ✅ PASS. When a reader detects an optional-dep construct, it sets `LifecycleScope::Optional` explicitly; if detection fails (malformed manifest, unknown construct), the component retains its prior classification (no silent fallback to `Runtime`).
- **Principle IV (Type-Driven Correctness)**: ✅ PASS. All new state routes through the extended `LifecycleScope` enum + new `OptionalDependsOn` variant on `RelationshipType` + new `OptionalDependencyOf` variant on `SpdxRelationshipType`. String derivation-values (`cargo-optional-true`, etc.) round-trip only through the `extra_annotations: HashMap<String, Value>` bag which is the established `mikebom:*` annotation carrier and already accepts arbitrary string keys/values.
- **Principle V (Specification Compliance / Native-First)**: ✅ PASS with explicit KEEP-BOTH polarity. The native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` relationship type is the primary signal for the `Optional` classification; the `mikebom:optional-derivation` annotation is a finer-grained supplement carrying WHICH mechanism populated the signal (Cargo `optional = true` vs. npm `optionalDependencies` vs. Maven `<optional>`), which the standard doesn't natively encode. This matches m178's peer-edge-targets pattern and follows the KEEP-BOTH polarity documented in `docs/reference/sbom-format-mapping.md` per m178. Spec's Constitution Alignment section (spec.md) cites the audit result inline. Follow-up: extend `sbom-format-mapping.md` with a new C-row for `OPTIONAL_DEPENDENCY_OF` under the KEEP-BOTH polarity.
- **Principle VI (Three-Crate Architecture)**: ✅ PASS. Changes span `mikebom-common` (new `LifecycleScope::Optional` variant + new `RelationshipType::OptionalDependsOn`) and `mikebom-cli` (classifier + emitter + per-ecosystem reader touch-ups). No new crates. `mikebom-ebpf` untouched.
- **Principle VII (Test Isolation)**: ✅ PASS. All new tests are unit + integration tests running under `cargo test --workspace` — no privileged tests.
- **Principle VIII (Completeness)**: ✅ PASS. m179 does not remove components; it re-classifies existing edges. Completeness invariants preserved.
- **Principle IX (Accuracy)**: ✅ PASS. The reported user-visible failure IS an accuracy defect — same scan yields non-interchangeable filter results across formats. m179 restores accuracy per SC-001 + SC-002.
- **Principle X (Transparency)**: ✅ PASS. The `mikebom:optional-derivation` annotation (FR-019) makes it observable WHICH mechanism populated each `Optional` classification — an operator can audit whether a component was flagged optional due to Cargo, npm, pip, Maven, Gradle, Erlang, or a future mechanism.
- **Principle XI (Enrichment)**: ✅ N/A. m179 uses no external enrichment sources.
- **Principle XII (External Data Source Enrichment)**: ✅ N/A. m179 is manifest-based classification only.
- **Strict Boundaries §1 (No lockfile-based discovery)**: ✅ PASS. Every new signal is populated by an existing reader that already discovered the component; m179 only refines classification on already-discovered components.
- **Strict Boundaries §4 (No `.unwrap()` in production)**: ✅ PASS. New code paths use `anyhow`/`thiserror`; classifier code uses total match arms via exhaustive pattern.
- **Strict Boundaries §5 (File-tier dedupe)**: ✅ N/A. m179 does not touch file-tier emission.

**Result**: All gates PASS. Phase 0 authorized.

## Project Structure

### Documentation (this feature)

```text
specs/179-spdx23-transitive-devscope/
├── plan.md              # This file
├── spec.md              # Feature spec (Q1/Q2 answered)
├── research.md          # Phase 0 output: ecosystem survey + design decisions
├── data-model.md        # Phase 1 output: internal type extensions + precedence table
├── quickstart.md        # Phase 1 output: user + developer quickstarts
├── contracts/           # Phase 1 output: 4 contracts
│   ├── internal-model-extension.md        # LifecycleScope + RelationshipType + SpdxRelationshipType
│   ├── spdx23-optional-dependency-of.md   # Wire-format contract
│   ├── mikebom-optional-derivation.md     # Annotation contract
│   └── pico-filter-parity.md              # SC-001 + SC-002 acceptance gate
├── checklists/
│   └── requirements.md  # Spec quality checklist (16/16 PASS)
└── tasks.md             # Phase 2 output (populated by /speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-common/src/
├── resolution.rs        # +LifecycleScope::Optional, +RelationshipType::OptionalDependsOn
└── (unchanged elsewhere)

mikebom-cli/src/
├── scan_fs/
│   ├── mod.rs                       # +apply_lifecycle_scope_to_edges extension
│   │                                # (LifecycleScope::Optional → OptionalDependsOn
│   │                                #  + BuildInclusion::NotNeeded (lifecycle=None) → TestDependsOn)
│   └── package_db/
│       ├── cargo.rs                 # +parse optional = true in [dependencies]
│       ├── npm/
│       │   ├── mod.rs               # +parse optionalDependencies in package.json
│       │   ├── package_lock.rs      # +propagate optional flag through lockfile
│       │   ├── yarn_lock.rs         # +propagate optional flag through yarn.lock
│       │   └── pnpm_lock.rs         # +propagate optional flag through pnpm-lock.yaml
│       ├── pip/
│       │   ├── pyproject.rs         # +parse [project.optional-dependencies.<extra>]
│       │   ├── setup_py.rs          # +parse extras_require
│       │   └── setup_cfg.rs         # +parse [options.extras_require]
│       ├── maven.rs                 # +parse <optional>true</optional>
│       ├── gradle/                  # +parse compileOnly configuration
│       │   └── (existing files)
│       └── erlang.rs                # +populate LifecycleScope::Optional for
│                                    # optional_applications (existing detection)
└── generate/
    ├── spdx/
    │   ├── relationships.rs         # +SpdxRelationshipType::OptionalDependencyOf
    │   │                            # +classifier match arm (Full mode)
    │   ├── v3_relationships.rs      # +match arm (no lifecycleScope for Optional per FR-017)
    │   └── (v3_annotations.rs      # unchanged; annotation round-trips generically)
    ├── cyclonedx/
    │   └── builder.rs               # Already handles LifecycleScope::Optional via
    │                                # is_non_runtime() → scope: "excluded" (FR-006 automatic)
    └── (parity/                    # +register mikebom:optional-derivation catalog row)

mikebom-cli/tests/
├── optional_dep_classification.rs   # NEW: per-US integration tests (US1-US7)
├── transitive_parity_go.rs          # UPDATED: SC-001 flagship gate (pico's yaml.v3 case)
└── (parity_symmetric_equal.rs      # UPDATED: register mikebom:optional-derivation)

docs/reference/
├── reading-a-mikebom-sbom.md        # +section on OPTIONAL_DEPENDENCY_OF consumption
├── sbom-format-mapping.md           # +C-row for OPTIONAL_DEPENDENCY_OF (KEEP-BOTH)
└── (existing files unchanged)
```

**Structure Decision**: Single three-crate workspace (existing). No new crates. Changes span `mikebom-common` (2 enum extensions), `mikebom-cli/src/scan_fs/` (classifier pass + 6-7 reader touch-ups), `mikebom-cli/src/generate/spdx/` (2 emitter arms), and docs. `mikebom-ebpf` untouched.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

All gates PASS. No justification table needed. Complexity note (not a violation): m179 has an unusually broad ecosystem-coverage scope (7 user stories touching 6+ ecosystem readers). This is a deliberate design choice per the user's "broadest scope" answer to the scoping question. `tasks.md` will document the delivery-cadence decision (single-PR bundle vs. incremental delivery via subsequent PRs on the same branch or as follow-up milestones) once the survey shape is known.

## Phase 0: Ecosystem Survey & Design Decisions

**Output**: `research.md` covering:

### Decision 1: Which ecosystems have a native optional-dep construct?

A per-ecosystem survey table with columns: `ecosystem | manifest construct | lockfile construct | classifier verdict`. The verdict values are `native + implement in m179` (has a well-defined construct, will implement), `native + defer` (has a construct but scoping notes push it to a follow-up milestone), `no equivalent` (no construct exists; row present for audit-completeness).

Ecosystems covered (from SC-007's enumeration): Cargo, npm, yarn, pnpm, pip (pyproject / setup.py / setup.cfg / uv), Poetry, Maven, Gradle, gem (Bundler), Composer, CocoaPods, Elixir (mix), Erlang (rebar), Scala (sbt), Haskell (cabal + stack), Dart (pub), CMake, Bazel, Conan, vcpkg, west, Go, NuGet, Homebrew, alpm, dpkg, apk, rpm, ipk, opkg, Yocto.

### Decision 2: Precedence rules for multi-signal components

Given a component with multiple classifier signals set — `lifecycle_scope = Some(Test)` AND `build_inclusion = Some(NotNeeded)`; OR `lifecycle_scope = Some(Optional)` AND `build_inclusion = Some(NotNeeded)`; etc. — which wire-format relationship type wins? The table encodes FR-014 + FR-015's rules and cross-references the m112 "never downgrade an existing test tag" invariant at `mikebom-cli/src/scan_fs/package_db/mod.rs:1201`.

### Decision 3: Where does the derivation annotation live in each emitter?

CDX 1.6 emits `mikebom:optional-derivation` as a `component.properties[]` entry (matches m112's `mikebom:build-inclusion-derivation`). SPDX 2.3 emits as a `Package.annotations[].annotationComment` payload wrapping the `MikebomAnnotationCommentV1` envelope (matches the parity extractor's existing carrier). SPDX 3 emits as a `spdx:Annotation` node with `spdx:statement` payload (matches m112's SPDX 3 emission). Parity extractor at `mikebom-cli/src/parity/extractors/` gains one new row (`SymmetricEqual` directionality).

### Decision 4: Delivery cadence

Given the substantial scope (7 user stories touching 6+ ecosystem readers + 2 core-model changes + emitter changes + docs), the plan recommends a phased delivery:
- **m179 delivers**: US1 (pico fix — the flagship) + US2 (research artifact) + US3 (Cargo — cleanest test signal, most-scanned ecosystem after Go) + core-model change (`LifecycleScope::Optional`, `OptionalDependsOn`, `OptionalDependencyOf`) + one SPDX 2.3 emitter arm.
- **m180 delivers**: US4 (npm/yarn/pnpm) — biggest per-unit user impact.
- **m181 delivers**: US5 (pip variants).
- **m182 delivers**: US6 (Maven + Gradle).
- **m183 delivers**: US7 (Erlang normalization).

This is a recommendation; `/speckit-tasks` may adjust the split if the ecosystem-coverage scope proves smaller than estimated.

## Phase 1: Design & Contracts

### Data Model (`data-model.md`)

- **`LifecycleScope` enum** — extended from `{Runtime, Development, Build, Test}` to `{Runtime, Development, Build, Test, Optional}`. `as_str()` gains `"optional"` value. `is_non_runtime()` unchanged (returns `!matches!(self, Runtime)` — the new variant inherits the correct behavior automatically per FR-006).
- **`lifecycle_scope_is_legacy_dev()` helper** — MUST NOT be updated to include `Optional`. Rationale: this helper is a milestone-052 compat bridge for `is_dev: Option<bool>` semantics; `Optional` is a NEW classification that didn't exist in the legacy `is_dev` model. If a caller of this helper wants "any non-runtime scope", they use `is_non_runtime()`; if they want "legacy dev semantics", they should NOT auto-include Optional.
- **`RelationshipType` enum** — extended from `{DependsOn, DevDependsOn, BuildDependsOn, TestDependsOn}` to add `OptionalDependsOn`. serde `rename_all = "snake_case"` inherits automatically to `"optional_depends_on"`.
- **`SpdxRelationshipType` enum** — extended with `OptionalDependencyOf`. Serializes to wire value `"OPTIONAL_DEPENDENCY_OF"` (SPDX 2.3 §11.1).
- **Classifier dispatch table** — the complete precedence-ordered dispatch table with columns `(lifecycle_scope, build_inclusion) → RelationshipType → SpdxRelationshipType`. Full mode + basic mode both covered.
- **`mikebom:optional-derivation` annotation** — value vocabulary + emission carrier per emitter.

### Contracts (`contracts/`)

1. **`internal-model-extension.md`** — the exact `LifecycleScope`, `RelationshipType`, `SpdxRelationshipType` extensions. Includes serde round-trip test signatures (unit-test-visible contract).
2. **`spdx23-optional-dependency-of.md`** — the wire-format contract: reversed direction (matching m052 convention), SPDX 2.3 §11.1 spec text citation, basic-mode fallthrough behavior. Includes a golden emission example.
3. **`mikebom-optional-derivation.md`** — annotation name + value vocabulary + per-emitter carrier + parity extractor registration. Includes cross-format byte-identity requirement (SC-008).
4. **`pico-filter-parity.md`** — the SC-001 + SC-002 acceptance gate: given a scan whose CDX carries N `scope: "excluded"` components, the SPDX 2.3 output MUST have N components that appear as source-side of any typed dep-scope relationship. Includes the jq recipe consumers use.

### Quickstart (`quickstart.md`)

- **Consumer flow**: how to filter test-noise / not-in-production components from a mikebom-produced SPDX 2.3 document using ONLY native `*_DEPENDENCY_OF` verbs (no `mikebom:*` prop parsing). jq recipe included.
- **Developer flow**: how to add a new per-ecosystem optional-dep classifier — follow the Cargo reader change as the template; populate `LifecycleScope::Optional` on the target component + emit the `mikebom:optional-derivation` annotation with the ecosystem-specific value.

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` at end of Phase 1 to record m179's tech context in `CLAUDE.md`.

## Post-Design Constitution Re-check

After Phase 1 design artifacts are written, re-verify:

- **Principle V (Native-first)**: Confirmed. The new `OPTIONAL_DEPENDENCY_OF` emission IS the native primary signal; the `mikebom:optional-derivation` annotation IS a Principle V KEEP-BOTH carve-out for the "which mechanism populated it" question that neither CDX nor SPDX 2.3/3 encodes natively. The `sbom-format-mapping.md` C-row update in `docs/` records this per Principle V's audit requirement.
- **Principle IV (Type-Driven)**: Confirmed. All new state routes through typed enums; no raw strings on function boundaries. The `mikebom:optional-derivation` annotation value is the ONE string place, and it's carried through the pre-existing `extra_annotations: HashMap<String, serde_json::Value>` bag that already accepts arbitrary payloads.
- **Principle IX (Accuracy)**: Confirmed via SC-001 (23=23 gate) + SC-002 (set-equality gate).

**Post-check result**: All gates hold. Ready for `/speckit-tasks`.
