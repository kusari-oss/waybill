# Feature Specification: Resolver Trait + Chain Refactor

**Feature Branch**: `209-resolver-trait-chain`
**Created**: 2026-07-18
**Status**: Draft
**Input**: User description: "601: refactor resolve pipeline to Resolver trait plus chain architecture mirroring SBOMit plugin design. Extract each ecosystem resolver into a self-contained module implementing a common trait. Rewrite pipeline orchestration to iterate over a resolver chain rather than calling functions in a fixed sequence. Byte-identical resolution output on existing test corpus, net zero semantic change, enables per-resolver unit testing and additive ecosystem additions. Closes 601."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Add a new ecosystem resolver without touching orchestration code (Priority: P1)

An mikebom contributor wants to add support for a new packaging ecosystem (say, NuGet, or a proprietary internal registry). Today, that means editing `pipeline.rs` to add a new dispatch branch, threading new context parameters through the pipeline's function-call sequence, and hoping nothing downstream depends on the order the new resolver runs relative to the existing four. The change is invasive and fragile.

**Why this priority**: This is the entire structural payoff of the refactor. If contributors can't add ecosystems by dropping a single file, none of the follow-on SBOMit-alignment work (local cache-probe tier, per-phase diagnostics, minimal-SBOM output) gets easier — they all inherit the current pipeline's coupling.

**Independent Test**: Add a scaffolded `NugetResolver` implementing the resolver trait, register it in a single registration point, run the full test suite. Assert (a) NuGet resolution works end-to-end for a fixture attestation, (b) `pipeline.rs` was NOT edited (verified by `git diff mikebom-cli/src/resolve/pipeline.rs` showing zero lines changed in the resolver dispatch code).

**Acceptance Scenarios**:

1. **Given** a new ecosystem resolver implementation living in one file, **When** the contributor adds a single registration line, **Then** the pipeline dispatches to the new resolver on matching input without any orchestration-code edit.
2. **Given** a new resolver returns resolved components with confidence and technique metadata, **When** the pipeline runs, **Then** those components appear in the deduplicated output alongside components from other resolvers with the same identity-precedence rules as before.
3. **Given** a new resolver has its own priority ordering intent, **When** the resolver declares its priority via a trait method, **Then** the pipeline honors that priority without pipeline-code edits.

---

### User Story 2 — Unit-test any resolver in isolation without invoking downstream resolvers (Priority: P2)

Today's `resolve_url_with_context` at `mikebom-cli/src/resolve/url_resolver.rs` is 832 lines handling seven ecosystems (cargo, pypi, npm, golang, maven, rubygems, deb) in a single function. Unit tests can call it directly but can't test one ecosystem's branch without loading fixtures for all seven, and can't verify per-ecosystem confidence scoring without the full pipeline stack. Contributors patching the Maven branch have to grep across 800 lines to understand what they're changing.

**Why this priority**: Directly enables per-ecosystem correctness. Every ecosystem-specific bug we've triaged (m087 cargo workspace-version, m088 cargo proc-macro edges, m092 maven version extract, m147 npm peer edges, m157 pnpm v9 graph, m180 npm optional deps) touched code that mixes multiple ecosystems' logic in one function; isolated resolvers make those bug fixes narrower and safer.

**Independent Test**: `cargo test -p mikebom resolve::resolvers::maven` runs only Maven resolver tests, in under 100 ms, exercising every Maven-specific code path (GAV extraction, license inference, snapshot-version handling) without loading Cargo / Python / npm fixtures. Verify by asserting the test binary's coverage report shows Maven-file coverage at 100 % while other resolvers show 0 %.

**Acceptance Scenarios**:

1. **Given** a per-ecosystem resolver module, **When** the contributor runs a per-file test, **Then** only that ecosystem's resolution paths execute; no other resolver's fixtures or dependencies load.
2. **Given** a resolver's confidence score depends on internal signals (e.g., high confidence when the file-path matches a known cache layout, medium when it matches a URL pattern), **When** the resolver is tested in isolation, **Then** each confidence branch is exercisable via a targeted unit test without touching other resolvers.

---

### User Story 3 — Preserve per-component provenance signal about which resolver matched (Priority: P3)

Downstream consumers (audit tools, VEX correlators, confidence-based policy engines) rely on each component's resolution-technique field to know how it was identified — URL-pattern match, deps.dev hash lookup, path heuristic, hostname fallback. The refactor must preserve this signal cleanly: every resolver populates the technique field with a stable identifier so downstream automation isn't broken.

**Why this priority**: Not a new capability — it's a regression-prevention story. If the refactor drops or renames the technique-string signal, every downstream consumer breaks silently. Explicit story guarantees the invariant is tested.

**Independent Test**: Compare the technique values on the golden fixture set before and after the refactor. Assert 100 % preservation of technique strings; no rename, no missing values, no changes to the technique enum's serialization surface.

**Acceptance Scenarios**:

1. **Given** the pre-refactor test corpus, **When** the post-refactor pipeline runs the same inputs, **Then** every emitted component's technique value matches the pre-refactor output byte-for-byte.
2. **Given** a new resolver is added post-refactor, **When** it produces a resolved component, **Then** it populates the technique field with a value the downstream consumer catalog recognizes (or explicitly registers a new variant per the extensibility contract).

---

### Edge Cases

- **A single input matches multiple resolvers**: the pipeline runs each resolver in priority order; the highest-confidence result wins per the existing deduplication contract. Priority is a property of the resolver, not the pipeline.
- **A resolver panics or returns an error**: the pipeline catches the panic (per Constitution Principle IV — production code MUST NOT panic) and logs a WARN with the resolver's name; subsequent resolvers still run. A resolver's failure MUST NOT block the pipeline.
- **A resolver produces zero components on legitimate input**: not an error condition; the next resolver in the chain gets a chance. Matches existing behavior.
- **Two resolvers claim the same PURL with different confidence**: the existing deduplication merge logic runs unchanged; the higher-confidence entry wins the primary identity, the other becomes evidence.
- **A resolver depends on state from a previous resolver's output**: the trait interface does NOT allow this. Resolvers are stateless — they take input, return output, no cross-resolver coordination. If a use case genuinely needs staged resolution (e.g., "resolve URLs first, then use the results to enrich path matches"), that is a follow-up milestone with an explicit contract, not an emergent property of the trait design.
- **Async resolver (deps.dev network call) inside a sync pipeline**: the trait must accommodate both sync and async resolvers cleanly. This is a design decision recorded in the plan phase, not exposed as a user-visible concern.
- **CLI operator disables a resolver via `--skip-purl-validation`**: preserved behavior — the flag continues to disable the deps.dev-hash resolver specifically, unchanged from today.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST define a common Resolver interface exposing at minimum a name (stable identifier string) and a resolve method (takes a traced connection and pipeline context, returns a list of resolved components).
- **FR-002**: The system MUST split the current URL-resolution monolith into per-ecosystem resolver modules, one per ecosystem (cargo, pypi, npm, golang, maven, rubygems, deb). No ecosystem's regex or dispatch logic may live outside its own module.
- **FR-003**: The system MUST wrap the deps.dev hash-resolution logic as a Resolver-interface implementation while preserving all existing behavior (timeout, offline-mode gating, confidence 0.90).
- **FR-004**: The system MUST wrap the path-heuristic resolution logic as a Resolver-interface implementation while preserving all existing behavior (confidence 0.70).
- **FR-005**: The system MUST wrap the hostname-fallback resolution logic as a Resolver-interface implementation while preserving all existing behavior (confidence 0.40).
- **FR-006**: The pipeline MUST iterate over a resolver chain in priority order, dispatching each traced connection through every resolver in sequence. The chain composition MUST be defined in one place (a single registration point).
- **FR-007**: Each resolver MUST declare its own confidence score and technique identifier; the pipeline MUST NOT hard-code these values per resolver.
- **FR-008**: The pipeline output on the existing regression test corpus MUST be byte-identical to pre-refactor output, verified via full-golden-set comparison. Any deviation is a bug; the refactor is net-zero semantic change.
- **FR-009**: Each resolver MUST be unit-testable in isolation — a test targeting one resolver's file MUST NOT load fixtures or execute code paths belonging to other resolvers.
- **FR-010**: Adding a new ecosystem resolver MUST NOT require editing the pipeline dispatch code — verified by adding a scaffolded new resolver and asserting the pipeline dispatch file is unchanged.
- **FR-011**: The existing `--skip-purl-validation` CLI flag MUST continue to disable the deps.dev-hash resolver specifically, without disabling other resolvers.
- **FR-012**: The existing INFO logs at resolver-boundary transitions MUST be preserved with equivalent field content (resolver name, component count, confidence) — no operator-visible logging regression.
- **FR-013**: The system MUST catch panics inside individual resolvers, log a WARN naming the resolver, and continue running the remaining resolvers in the chain. A single resolver's failure MUST NOT abort the pipeline.
- **FR-014**: Resolvers MUST be stateless — no interior mutable state, no cross-resolver coordination via shared references. Each resolve call is a pure function of its inputs plus the read-only pipeline context.
- **FR-015**: The Resolver interface MUST accommodate both synchronous resolvers (URL, path, hostname) and asynchronous resolvers (deps.dev hash lookup with network I/O) via a single trait shape.
- **FR-016**: The resolver-registration point MUST be documented such that a contributor adding a new ecosystem can copy an existing resolver, adapt it to the new ecosystem, register it, and see the pipeline dispatch to it — all without reading pipeline-orchestration code.

### Key Entities *(include if feature involves data)*

- **Resolver (interface)**: The common shape every ecosystem resolver implements. Exposes a name, a priority, a confidence score, a technique identifier, and a resolve function. Stateless and independently testable.
- **Resolver chain**: The ordered list of resolvers the pipeline dispatches through. Constructed at pipeline startup from the compile-in registration point; each entry is a resolver implementation.
- **Resolution context**: The per-pipeline-invocation context passed to every resolver (config flags, timeout budgets, etc.). Read-only from the resolver's perspective; the pipeline manages its lifetime.
- **Resolved component**: The output of a resolver's resolve call — a component identity (PURL, name, version) plus resolution evidence (confidence, technique, source connection identifiers).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The full existing resolution test corpus (unit and integration) passes with 100 % byte-identical output between pre-refactor and post-refactor pipelines. Any golden-fixture regeneration is a bug requiring root-cause analysis, not blanket regen.
- **SC-002**: Adding a new hypothetical ecosystem resolver requires editing exactly one file (the new resolver's module) plus one line in the registration point. Verified by producing a proof-of-concept NuGet resolver and asserting the git-diff scope.
- **SC-003**: Each resolver's per-file unit test suite runs in under 100 ms, exercising every internal code path of that resolver, and does NOT load fixtures or execute code paths from any other resolver.
- **SC-004**: The refactored pipeline's wall-clock resolution time does not regress by more than 5 % relative to the pre-refactor pipeline on the existing benchmark corpus.
- **SC-005**: Every downstream consumer of the resolution-technique signal (audit tools, confidence-based policy engines) receives identical technique values before and after the refactor — 100 % preservation, verified on the golden fixture set.
- **SC-006**: A contributor unfamiliar with the pipeline can add a new ecosystem resolver by reading only the resolver-authoring documentation and one existing resolver as an example, without needing to open the pipeline-dispatch file.

## Assumptions

- **Compile-in registration**: Resolvers are registered at compile time via a single source-code line in the pipeline setup module. Dynamic loading of external plugin binaries is out of scope for this milestone (tracked separately at #453).
- **Stateless resolver contract**: Resolvers hold no mutable state across invocations. If a future use case genuinely needs staged resolution (resolver B enriches from resolver A's output), that is a separate design conversation — the trait interface for this milestone is stateless.
- **Deduplication contract unchanged**: The existing post-pipeline deduplication pass runs unchanged. The refactor changes how resolvers are dispatched, not how their results are merged.
- **Priority declared per resolver, not per chain-position**: Each resolver returns its priority via a trait method rather than being ordered by position in the registration list. Two resolvers cannot declare the same priority (compile-time or startup-time check).
- **Sync + async in one trait**: The trait handles both synchronous resolvers (URL, path, hostname) and asynchronous resolvers (deps.dev). The specific mechanism is a plan-phase design decision.
- **Panic safety**: Existing test suite already exercises Constitution Principle IV's no-panic-in-production discipline; the refactor preserves that. Individual resolver panics are caught at the pipeline layer per FR-013.
- **CLI surface unchanged**: No new CLI flags. `--skip-purl-validation` continues to work per FR-011.
- **Documentation deliverable in scope**: A new resolver-authoring guide (docs page) is part of the milestone deliverable, matching FR-016's testable claim.
- **No public-API exposure**: The Resolver trait is crate-internal to `mikebom-cli`. Third-party crates cannot implement resolvers without a follow-up plugin-system milestone (#453). This matches Constitution Principle VI (Three-Crate Architecture).
- **Sequencing with SBOMit-pattern series**: This milestone unblocks #604 (5-phase pipeline refactor) and #605 (local cache-probe resolver tier) — both become straightforward additions after the trait chain exists. Either can wait for this to land; neither blocks it.
