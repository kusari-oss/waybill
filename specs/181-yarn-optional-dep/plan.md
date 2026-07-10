# Implementation Plan: yarn v1 + Berry optional-dependency classification

**Branch**: `181-yarn-optional-dep` | **Date**: 2026-07-10 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/181-yarn-optional-dep/spec.md`

## Summary

Extend the yarn reader (`yarn_lock.rs`) — both the v1 line-oriented parser and the Berry YAML parser — to classify optional-declared deps as `LifecycleScope::Optional` (m179) + emit `mikebom:optional-derivation = "npm-optional-dependencies"` (m180 value). Yarn v1: the parser at line 183 already recognizes `optionalDependencies:` sub-blocks but currently collapses them into a flat `dep_names` accumulator — m181 splits the accumulator so optional-child names carry through to the classifier. Yarn Berry: `dependenciesMeta.<name>.optional = true` lives in `package.json` (not `yarn.lock`) — the reader gains a package.json cross-reference. Both variants pipe through the m180 `is_peer_optional` guard (currently `#[allow(dead_code)]`) for the FR-005 peer-precedence rule. Root-project scope only; workspace-member `dependenciesMeta` explicitly deferred to a follow-up.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–180; no nightly required).

**Primary Dependencies**: Existing only — `serde_json` (package.json parsing; already used at `yarn_lock.rs:265+` for m159 alias annotations), `serde_yaml` (already used for Berry yarn.lock parsing), `tracing` (info/debug logs), `anyhow`/`thiserror` (error propagation). Reuses m179's `LifecycleScope::Optional` variant + `RelationshipType::OptionalDependsOn` + `SpdxRelationshipType::OptionalDependencyOf` + m180's C122 parity catalog row + m180's `peer_optional::is_peer_optional` helper. **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM.

**Testing**: `cargo +stable test --workspace`. New tests: (a) unit tests in yarn_lock.rs for v1 optional-child-set construction + Berry `dependenciesMeta` cross-reference + peer-optional guard interactions; (b) integration tests via new fixtures under `mikebom-cli/tests/fixtures/optional_dep/yarn-v1/` + `yarn-berry/` + `yarn-peer-optional/` mirroring m180's shape; (c) reuses m180's end-to-end scan pattern (`optional_dep_*_e2e.rs`).

**Target Platform**: Same as every prior mikebom milestone — Linux + macOS user-space, no Windows-specific behavior.

**Project Type**: CLI + library (three-crate workspace: `mikebom-cli`, `mikebom-common`, `mikebom-ebpf` — last untouched).

**Performance Goals**: Zero perceptible regression. m181 adds two small extractions per scan: (a) a `HashSet<String>` of optional-child names built during v1 body-block iteration (O(edges)); (b) a single `serde_json::from_str` on the root `package.json` (O(bytes)). Fixture SBOM emission size grows only by the `mikebom:optional-derivation` annotation on affected components (~50 bytes per touched component).

**Constraints**: (1) All 9 SC gates from spec.md — SC-003 (no `*_DEPENDENCY_OF` decrement) + SC-004 (CDX zero-drift for un-touched fixtures) + SC-005 (SPDX 3 zero-drift for ALL fixtures) are strict-equality gates. (2) SC-008 preserves the m106 US5 baseline + m159 alias behavior byte-identically. (3) Principle IV Type-Driven: all classification routes through m179's existing `LifecycleScope::Optional`. (4) Principle V native-first: rides on m180's KEEP-BOTH polarity.

**Scale/Scope**: 3 user stories (all P1), 13 functional requirements, 9 success criteria. Estimated ~20-25 tasks across 6 phases (setup + foundational-plumbing + US1 v1 + US2 Berry + US3 peer-precedence + polish).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)**: ✅ PASS. No new C dependencies.
- **Principle II (eBPF-Only Observation)**: ✅ N/A. m181 is emission-time metadata transformation; no discovery-source changes.
- **Principle III (Fail Closed)**: ✅ PASS. When package.json is missing or unparseable (FR-004), the reader falls back to lockfile-only parsing with a `tracing::warn!` diagnostic; components stay unclassified rather than getting a wrong classification. When optional detection succeeds, `LifecycleScope::Optional` is set explicitly.
- **Principle IV (Type-Driven Correctness)**: ✅ PASS. All new state routes through m179's existing `LifecycleScope::Optional` variant + m180's shared `is_peer_optional` predicate. String derivation value rides in the existing `extra_annotations` bag.
- **Principle V (Specification Compliance / Native-First)**: ✅ PASS. Zero new `mikebom:*` invention — the signal flows through m180's existing C122 (`mikebom:optional-derivation`) + native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`. Zero new Principle V audit surface. Spec's Constitution Alignment section cites m179/m180 as the audit-of-record.
- **Principle VI (Three-Crate Architecture)**: ✅ PASS. Changes span only `mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs` (single reader file) + tests. No new crates. `mikebom-common` untouched. `mikebom-ebpf` untouched.
- **Principle VII (Test Isolation)**: ✅ PASS. All new tests are unit + integration under `cargo test --workspace` — no privileged tests.
- **Principle VIII (Completeness)**: ✅ PASS. m181 does not remove components; it re-classifies existing edges.
- **Principle IX (Accuracy)**: ✅ PASS. Directly measured by SC-001 + SC-002 (yarn CDX/SPDX 2.3 filter-set equality) + SC-007 (peer-precedence preserved).
- **Principle X (Transparency)**: ✅ PASS. The `mikebom:optional-derivation` annotation identifies JavaScript-ecosystem-lockfile classification; `evidence.source_file_paths` narrows to yarn.lock specifically.
- **Principle XI (Enrichment)** + **XII (External Data Source Enrichment)**: ✅ N/A. m181 is manifest/lockfile-based classification only.
- **Strict Boundaries §1 (No lockfile-based discovery)**: ✅ PASS. Every classification target is already discovered by the existing yarn reader; m181 only refines classification.
- **Strict Boundaries §4 (No `.unwrap()` in production)**: ✅ PASS.

**Result**: All gates PASS. Phase 0 authorized.

## Project Structure

### Documentation (this feature)

```text
specs/181-yarn-optional-dep/
├── plan.md              # This file
├── spec.md              # Feature spec
├── research.md          # Phase 0 output: reader survey + plumbing design
├── data-model.md        # Phase 1 output: classifier extension + entities
├── quickstart.md        # Phase 1 output: developer flow
├── contracts/           # Phase 1 output
│   ├── yarn-classifier-extension.md   # Per-variant code shape
│   └── yarn-peer-precedence-guard.md  # FR-005 contract (delta from m180 US4)
├── checklists/
│   └── requirements.md  # Spec quality checklist (16/16 PASS)
└── tasks.md             # Phase 2 output (populated by /speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/package_db/npm/
├── yarn_lock.rs         # Single reader touched — five changes:
│                        #   1. read_yarn_lock signature UNCHANGED; internal
│                        #      semantic gains a package.json read alongside
│                        #      the yarn.lock read. parse_yarn_lock signature
│                        #      GAINS a third `pkg_json: &serde_json::Value`
│                        #      parameter (all existing test callers updated
│                        #      to pass `&Value::Null` — backward-compatible
│                        #      for tests that don't exercise m181
│                        #      classification). See data-model.md §3.1-§3.2.
│                        #      FR-004 fail-safe: Value::Null on missing or
│                        #      unparseable package.json.
│                        #   2. parse_v1 body-block loop separates optionalDeps
│                        #      accumulator (FR-001)
│                        #   3. parse_berry cross-references
│                        #      `dependenciesMeta.<name>.optional` from
│                        #      package.json (FR-003)
│                        #   4. build_entry gains classification parameters
│                        #      (FR-006 — Option A from research.md Decision 1)
│                        #   5. Both variants apply is_peer_optional guard
│                        #      (FR-005 — consumes the m180 helper)
├── peer_optional.rs     # #[allow(dead_code)] marker REMOVED (helper now used)
└── mod.rs               # unchanged (top-level dispatch)

mikebom-cli/tests/
├── optional_dep_yarn_v1_e2e.rs           # NEW: US1 end-to-end fixture scan
├── optional_dep_yarn_berry_e2e.rs        # NEW: US2 end-to-end fixture scan
├── optional_dep_yarn_peer_precedence.rs  # NEW: US3 peer-precedence fixture scan
└── fixtures/optional_dep/
    ├── yarn-v1/                 # NEW: {package.json, yarn.lock v1}
    ├── yarn-berry/              # NEW: {package.json with dependenciesMeta,
    │                            #        yarn.lock Berry}
    └── yarn-peer-optional/      # NEW: US3 fixture with peer-optional react

docs/reference/reading-a-mikebom-sbom.md  # UPDATED: extend m180's
                                          # optional-derivation subsection with
                                          # yarn coverage note.
```

**Structure Decision**: Single three-crate workspace (existing). No new crates. Source changes concentrated in ONE reader file (`yarn_lock.rs`); test changes span three new integration test files + three new fixture dirs.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

All gates PASS. No justification table needed. Complexity note (not a violation): yarn's two-format polymorphism (v1 line-format vs Berry YAML) means m181 touches two distinct code paths in the same file. This is not new complexity — it's the same v1/Berry split every yarn milestone has navigated since m106.

## Phase 0: Reader Design Decisions

**Output**: `research.md` covering 6 decisions:

### Decision 1: Where does `build_entry` get its classification input?

Three placement options for FR-006's plumbing:

| Option | Approach | Trade-off |
|--------|----------|-----------|
| A (Recommended) | Pass `optional_names: &HashSet<String>` to `build_entry` (or a wrapper) | Small — parameters added to a shared 5-line function; both parsers pre-build the sets |
| B | Post-process: parsers return `Vec<PackageDbEntry>`, then walk it AFTER emission and mutate `lifecycle_scope` + `extra_annotations` | Adds a `&mut` mutation phase; extra sweep |
| C | Duplicate `build_entry` per variant | Loses shared byte-identity guarantee |

Option A wins.

### Decision 2: Package.json parsing — where does the file live?

Existing `read_yarn_lock(rootfs: &Path, include_dev: bool)` reads `rootfs.join("yarn.lock")`. m181 also reads `rootfs.join("package.json")` — same rootfs — parses as `serde_json::Value` — passes to both parsers. Missing/unparseable → `Value::Null` and classification safely yields no optional entries.

### Decision 3: v1 accumulator split — how far to refactor?

Convert single `dep_names: Vec<String>` accumulator into a pair `(regular, optional)` — merged into `depends` when building the entry (dep edges don't care about distinction). But `optional_dep_names` is ALSO passed to the classifier for name-match.

Diamond-shape (FR-007): if a name appears in `optional_dep_names` from parent A AND in `dep_names` from parent B, Runtime wins. Enforced during set-union pass.

### Decision 4: Yarn Berry `dependenciesMeta` extraction

Simple walk on root package.json:

```
package_json["dependenciesMeta"]
    .as_object()
    .into_iter()
    .flat_map(|obj| obj.iter())
    .filter(|(_, meta)| meta.get("optional").and_then(|v| v.as_bool()) == Some(true))
    .map(|(name, _)| name.to_string())
    .collect::<HashSet<String>>()
```

### Decision 5: Peer-precedence guard placement

Both parsers apply identically: for each name in the optional-set, check `is_peer_optional(name, &package_json)`. If true, REMOVE from the optional-set BEFORE passing to the classifier. Guard runs once during set construction.

Removes the `#[allow(dead_code)]` marker on `peer_optional::is_peer_optional`.

### Decision 6: Delivery cadence

All three US ship in one PR — they share the same file + converging patterns. Fallback split by variant if implementation surprises arise.

## Phase 1: Design & Contracts

### Data Model (`data-model.md`)

- **`LifecycleScope::Optional`** — reused verbatim from m179. Zero changes.
- **`mikebom:optional-derivation` annotation** — reused from m180 with value `"npm-optional-dependencies"`. Zero schema change.
- **`OptionalNameSet<String>`** (parser-local, per-scan) — the yarn-parser pre-computed set of names classified as Optional per FR-005 + FR-007. Internal type.
- **Yarn v1 dependency-sub-block accumulator** — refactored from `Vec<String>` to `(Vec<String>, Vec<String>)` = `(regular, optional)`. Internal.
- **Root `package.json` `serde_json::Value`** — new plumbing through `read_yarn_lock` → `parse_v1` / `parse_berry`.

### Contracts (`contracts/`)

1. **`yarn-classifier-extension.md`** — per-parser plumbing shape
2. **`yarn-peer-precedence-guard.md`** — delta from m180 US4 (yarn's peer-optional source is package.json, not the lockfile entry — yarn lockfiles don't carry `peer: true` the way npm/pnpm lockfiles do)

### Quickstart (`quickstart.md`)

- **Consumer flow**: unchanged from m179/m180 — same jq recipes work
- **Developer flow**: extends m180's template with a yarn-specific section

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` at end of Phase 1.

## Post-Design Constitution Re-check

- **Principle V**: Confirmed. Zero new `mikebom:*` fields; extends m180's derivation vocabulary with one more emission site.
- **Principle IV**: Confirmed. All new state routes through m179's `LifecycleScope::Optional`.
- **Principle IX**: Confirmed via SC-001 + SC-002 + SC-007.

**Post-check result**: All gates hold. Ready for `/speckit-tasks`.
