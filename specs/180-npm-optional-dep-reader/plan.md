# Implementation Plan: npm / yarn / pnpm optional-dependency classification

**Branch**: `180-npm-optional-dep-reader` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/180-npm-optional-dep-reader/spec.md`

## Summary

Extend the four JavaScript-ecosystem lockfile readers — npm (`package-lock.json` v2/v3), pnpm (`pnpm-lock.yaml` v9+), yarn (v1 + Berry), bun (`bun.lock`) — to set `LifecycleScope::Optional` (m179's new variant) + emit `mikebom:optional-derivation = "npm-optional-dependencies"` when the lockfile flags a component as optional. Two of the four readers (npm, pnpm) already parse the `optional: true` boolean but currently use it only as a `--include-dev=false` filter signal — the fix is a one-line classifier update (bool-to-enum + annotation). Yarn and bun currently emit `lifecycle_scope: None` — they need new plumbing. Preserve the m178 peer-precedence rule: when a dep is BOTH peer AND peer-optional (`peerDependenciesMeta.<name>.optional = true`), the m178 `PROVIDED_DEPENDENCY_OF` classification wins — reader-time guard prevents setting `LifecycleScope::Optional` on peer-classified targets.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–179; no nightly required).

**Primary Dependencies**: Existing only — `serde_json` (JSON parsing for package-lock.json + package.json + bun.lock), `serde_yaml` (already used for pnpm-lock.yaml + yarn.lock parsing), `regex` (already used for yarn v1 line-format extraction), `tracing` (info/debug logs at classifier decisions), `anyhow`/`thiserror` (error propagation). Reuses m179's `LifecycleScope::Optional` variant + `RelationshipType::OptionalDependsOn` + `SpdxRelationshipType::OptionalDependencyOf` + `mikebom:optional-derivation` C122 parity catalog row — no new enum extensions needed. **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM. Matches every milestone since 002.

**Testing**: `cargo +stable test --workspace`. New tests: (a) unit tests per reader (npm, pnpm, yarn v1, yarn Berry, bun) exercising the `optional: true` → `LifecycleScope::Optional` mapping + annotation emission; (b) unit tests for the reader-time peer-precedence guard (a component in BOTH `peerDependencies` AND `optionalDependencies` gets peer classification, not optional); (c) integration tests via new fixtures under `mikebom-cli/tests/fixtures/optional_dep/{npm,pnpm,yarn}/` mirroring the m179 Cargo fixture pattern; (d) reuses m179's `optional_dep_classification.rs` integration harness for cross-format end-to-end assertions.

**Target Platform**: Same as every prior mikebom milestone — Linux + macOS user-space, no Windows-specific behavior. The classification is a pure Rust code path with no platform-specific dependencies.

**Project Type**: CLI + library (three-crate workspace: `mikebom-cli`, `mikebom-common`, `mikebom-ebpf` — last is untouched).

**Performance Goals**: Zero perceptible regression on end-to-end scan wall-clock. The four reader touch-ups add O(1) per lockfile entry (single boolean check + annotation insert). Fixture SBOM emission size grows only by the new `mikebom:optional-derivation` annotation on affected components (~50 bytes per touched component, matching m179 Cargo).

**Constraints**: (1) All 8 SC gates from spec.md — SC-002 (no `*_DEPENDENCY_OF` decrement) + SC-003 (CDX zero-drift for un-touched fixtures) + SC-004 (SPDX 3 zero-drift for ALL fixtures) are strict-equality gates. (2) FR-006 peer-precedence rule must be defended by a fixture-based test in US4. (3) Principle IV (Type-Driven Correctness): all classification routes through the existing `LifecycleScope::Optional` variant — no new fields, no raw string handling across function boundaries. (4) Principle V native-first: m180 rides on m179's KEEP-BOTH polarity — no new `mikebom:*` invention.

**Scale/Scope**: 5 user stories, 14 functional requirements, 8 success criteria. Estimated ~20-25 tasks across 5-6 phases (setup + foundational-reader-guard-utility + US1 npm + US2 pnpm + US3 yarn + US4 peer-precedence guard + polish). Optional US5 bun ships if reader-touch cost is low.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)**: ✅ PASS. No new C dependencies.
- **Principle II (eBPF-Only Observation)**: ✅ N/A. m180 is emission-time metadata transformation; no discovery-source changes.
- **Principle III (Fail Closed)**: ✅ PASS. When a reader detects an optional-dep signal, it sets `LifecycleScope::Optional` explicitly; if the signal is absent (or filtered out by the peer-precedence guard), the component retains its prior classification.
- **Principle IV (Type-Driven Correctness)**: ✅ PASS. All new state routes through the existing `LifecycleScope::Optional` variant from m179. String derivation-value `"npm-optional-dependencies"` rides in the `extra_annotations: HashMap<String, serde_json::Value>` bag (established `mikebom:*` annotation carrier).
- **Principle V (Specification Compliance / Native-First)**: ✅ PASS. m180 introduces no new `mikebom:*` fields — the entire signal flows through m179's existing C122 (`mikebom:optional-derivation`) + native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`. Zero new Principle V audit surface. Spec's Constitution Alignment section cites m179 as the audit-of-record.
- **Principle VI (Three-Crate Architecture)**: ✅ PASS. Changes span only `mikebom-cli/src/scan_fs/package_db/npm/` (4 reader files) + tests. No new crates. `mikebom-common` untouched (m179's variants suffice). `mikebom-ebpf` untouched.
- **Principle VII (Test Isolation)**: ✅ PASS. All new tests are unit + integration tests running under `cargo test --workspace` — no privileged tests.
- **Principle VIII (Completeness)**: ✅ PASS. m180 does not remove components; it re-classifies existing edges. Completeness invariants preserved.
- **Principle IX (Accuracy)**: ✅ PASS. Directly measured by SC-001 (CDX/SPDX 2.3 filter-set equality for JavaScript fixtures) + SC-007 (peer-precedence preserved).
- **Principle X (Transparency)**: ✅ PASS. The `mikebom:optional-derivation` annotation makes it observable that JavaScript lockfile classification populated the signal; `evidence.source_file_paths` distinguishes WHICH lockfile (npm-lock vs pnpm-lock vs yarn.lock vs bun.lock).
- **Principle XI (Enrichment)** + **XII (External Data Source Enrichment)**: ✅ N/A. m180 is manifest/lockfile-based classification only.
- **Strict Boundaries §1 (No lockfile-based discovery)**: ✅ PASS. Every classification target is already discovered by the existing readers; m180 only refines the classification on already-discovered components.
- **Strict Boundaries §4 (No `.unwrap()` in production)**: ✅ PASS. New code paths use `anyhow`/`thiserror` + existing per-reader error patterns.

**Result**: All gates PASS. Phase 0 authorized.

## Project Structure

### Documentation (this feature)

```text
specs/180-npm-optional-dep-reader/
├── plan.md              # This file
├── spec.md              # Feature spec
├── research.md          # Phase 0 output: reader survey + peer-guard design
├── data-model.md        # Phase 1 output: classifier extension points + precedence table
├── quickstart.md        # Phase 1 output: consumer + developer flows
├── contracts/           # Phase 1 output
│   ├── reader-classifier-extension.md         # Per-reader code shape
│   ├── peer-precedence-guard.md               # FR-006 contract
│   └── javascript-filter-parity.md            # SC-001 gate
├── checklists/
│   └── requirements.md  # Spec quality checklist (16/16 PASS)
└── tasks.md             # Phase 2 output (populated by /speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/package_db/npm/
├── package_lock.rs      # US1 — line ~308 classifier switch: is_optional → LifecycleScope::Optional
├── pnpm_lock.rs         # US2 — line ~351 classifier switch (mirrors US1)
├── yarn_lock.rs         # US3 — line ~378 currently emits `lifecycle_scope: None`; new
│                        #        plumbing: cross-reference optionalDependencies sub-blocks
├── bun_lock.rs          # US5 — lines ~175 + ~259 currently emit `lifecycle_scope: None`;
│                        #        wire the classification (contingent on bun schema audit
│                        #        during Phase 0)
└── mod.rs               # unchanged (top-level dispatch)

mikebom-cli/tests/
├── optional_dep_classification.rs           # UPDATED: extend the m179 integration
│                                            # harness with US1-US4 tests
├── optional_dep_npm_e2e.rs                  # NEW: US1 end-to-end fixture scan
├── optional_dep_pnpm_e2e.rs                 # NEW: US2 end-to-end fixture scan
├── optional_dep_yarn_e2e.rs                 # NEW: US3 end-to-end fixture scan
├── optional_dep_peer_precedence.rs          # NEW: US4 peer-precedence fixture scan
└── fixtures/optional_dep/
    ├── npm/                                 # NEW: {package.json, package-lock.json}
    ├── pnpm/                                # NEW: {package.json, pnpm-lock.yaml}
    ├── yarn-v1/                             # NEW: {package.json, yarn.lock v1}
    ├── yarn-berry/                          # NEW: {package.json, yarn.lock Berry, .yarnrc.yml}
    ├── bun/                                 # NEW: {package.json, bun.lock} (contingent US5)
    └── peer-optional/                       # NEW: US4 fixture — react as peer-optional dep

# Docs updates (optional; per US2 delivery split):
docs/reference/reading-a-mikebom-sbom.md     # UPDATED: extend m179's optional-derivation
                                             # subsection with the "npm-optional-dependencies"
                                             # value note.
```

**Structure Decision**: Single three-crate workspace (existing). No new crates. All source changes localized to `mikebom-cli/src/scan_fs/package_db/npm/` + tests. `mikebom-ebpf` untouched.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

All gates PASS. No justification table needed. Complexity note (not a violation): m180's 5 user stories touch 4 different lockfile parsers; each has slightly different shape (npm's per-entry bool, pnpm's dual-carrier surface, yarn v1's sub-block, Berry's package.json out-of-band signal, bun's TBD schema). This is unavoidable — each format is its own thing. The mitigating factor is that they all converge on the SAME `LifecycleScope::Optional` internal signal with the SAME `"npm-optional-dependencies"` derivation value, so the emission-side code path is uniform per m179's design.

## Phase 0: Reader Survey & Peer-Guard Design Decisions

**Output**: `research.md` covering:

### Decision 1: Per-Reader Classification Site Table

A table per lockfile variant listing:
- The current code location (file:line) where components are constructed with `lifecycle_scope`
- The current handling of the optional signal (used-as-filter / not-detected / etc.)
- The proposed change

### Decision 2: Peer-Precedence Guard Placement

FR-006 mandates that peer-classified edges preserve their `PROVIDED_DEPENDENCY_OF` emission even when the target ALSO satisfies optional-classification. Three placement options:

| Option | Site | Complexity | Coupling |
|--------|------|-----------|----------|
| A (Recommended) | Reader-time — skip setting `LifecycleScope::Optional` when the same dep name appears in the parent's `peerDependencies` map | Low — reader already reads both maps | Low — no cross-file state |
| B | m179 classifier at `scan_fs/mod.rs:1281` — check peer-edges lookup before rewriting Optional→OptionalDependsOn | Medium — classifier gains new guard | Medium — classifier now knows peer-edge state |
| C | m178 SPDX 2.3 classifier at `spdx/relationships.rs:279` — new match arm `(Full, OptionalDependsOn) if peer_edges.contains(...) => ProvidedDependencyOf` | Low — single arm addition | High — classifier now must invert m179's rewrite |

Option A wins: the semantic distinction (peer-optional edge) is already present at reader time in the source manifest; guarding there keeps the classifier code paths orthogonal.

### Decision 3: Transitive Propagation Semantics

For npm and pnpm, the lockfile's per-entry `optional: true` flag already propagates through the nested tree (npm/pnpm resolvers set it on every entry reachable only through an optional edge). Mikebom respects this by reading the flag verbatim on each lockfile entry — no new BFS/DFS logic needed.

For yarn v1: the `optionalDependencies:` sub-block appears on each PARENT entry naming CHILDREN. Yarn v1 does NOT propagate the flag transitively in the lockfile — mikebom needs to walk from the parent's sub-block to identify which target entries should be classified optional. Design: build a set-of-optional-child-names from all parents' `optionalDependencies:` sub-blocks, then classify each target entry by name-membership.

For yarn Berry: `dependenciesMeta.<name>.optional = true` appears in `package.json`, not `yarn.lock`. Mikebom reads package.json separately per the existing yarn Berry path (needs verification during Phase 0).

For bun: schema audit required — will document in research.md after inspecting `bun.lock` real-world examples.

### Decision 4: Delivery Cadence

m180 delivers all 5 US as one PR if the code-cost stays low. If yarn (US3) proves expensive due to Berry's package.json cross-referencing, US3 may split into a follow-up PR on the same branch. Bun (US5) can defer to m181 if its schema audit reveals unexpected complexity.

## Phase 1: Design & Contracts

### Data Model (`data-model.md`)

- **`LifecycleScope::Optional`** — no changes (m179's variant is reused verbatim). `is_non_runtime()` returns `true` → CDX `scope: "excluded"` auto-emission → FR-009.
- **`mikebom:optional-derivation` annotation** — no changes to the schema. New value `"npm-optional-dependencies"` (documented in the annotation contract but already permitted by m179 FR-019's open enum).
- **Reader classification dispatch table** — the exact per-lockfile classification logic (npm/pnpm/yarn v1/yarn Berry/bun) documented as a decision table.
- **Peer-precedence guard** — reader-time predicate + fallback semantics.

### Contracts (`contracts/`)

1. **`reader-classifier-extension.md`** — per-reader code shape: what to change, where to add the annotation, and how to detect the optional signal for each of npm/pnpm/yarn v1/yarn Berry/bun.
2. **`peer-precedence-guard.md`** — FR-006's precise contract: reader-time predicate (a target is peer-optional iff parent's `peerDependencies.<name>` exists AND `peerDependenciesMeta.<name>.optional == true`); wire-format outcome (`PROVIDED_DEPENDENCY_OF` wins). Includes the golden emission example demonstrating the precedence.
3. **`javascript-filter-parity.md`** — SC-001 acceptance gate: given a JavaScript scan whose CDX has N `scope: "excluded"` components, the SPDX 2.3 output MUST have N components appearing as source-side of typed dep-scope relationships. Includes jq recipes + test signatures.

### Quickstart (`quickstart.md`)

- **Consumer flow**: reader inherits everything from m179's quickstart. Note the new derivation value `"npm-optional-dependencies"`.
- **Developer flow**: how to extend a NEW JavaScript-ecosystem lockfile reader with the optional classifier — three-step recipe: (1) detect the optional signal from the lockfile shape, (2) apply peer-precedence guard, (3) set `LifecycleScope::Optional` + insert the annotation.

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` at end of Phase 1.

## Post-Design Constitution Re-check

After Phase 1 artifacts are written, re-verify:

- **Principle V (Native-first)**: Confirmed. m180 adds no new `mikebom:*` fields; it merely extends m179's existing derivation vocabulary with one more value. `docs/reference/sbom-format-mapping.md`'s C122 row already documents the annotation; m180 does NOT require an update to that row (the row explicitly states the value vocabulary is open per FR-019).
- **Principle IV (Type-Driven)**: Confirmed. All new state routes through the existing `LifecycleScope::Optional` variant; the derivation value is a string carried through the pre-existing `extra_annotations` bag.
- **Principle IX (Accuracy)**: Confirmed via SC-001 (JavaScript filter-set equality gate) + SC-007 (peer-precedence regression gate).

**Post-check result**: All gates hold. Ready for `/speckit-tasks`.
