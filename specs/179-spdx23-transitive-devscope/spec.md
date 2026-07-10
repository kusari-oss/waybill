# Feature Specification: Unified optional-dependency classification across ecosystems

**Feature Branch**: `179-spdx23-transitive-devscope`
**Created**: 2026-07-09
**Status**: Ready for `/speckit-plan` (Q1 + Q2 answered 2026-07-09)
**Input**: User description: "m179 — pico surfaced a real gap: `Mikebom knows 23 packages are test-only. In CDX, all 23 get "scope": "excluded". In SPDX, only 13 get marked (the root's direct deps, via TEST_DEPENDENCY_OF). The other 10 are test-only packages of other packages — those get written as plain DEPENDS_ON.` User direction: fix the SPDX-2.3 filter-parity gap AND survey every ecosystem's optional-dependency construct so mikebom exposes a unified 'is-optional' signal from one internal model. User's semantic split: emit `TEST_DEPENDENCY_OF` for test-only-transitives (Go m112 case), emit `OPTIONAL_DEPENDENCY_OF` for feature-flagged optionals (Cargo `optional = true` case), each via the SPDX 2.3 native vocabulary."

## Clarifications

### Session 2026-07-09

- Q: How should mikebom's internal model represent the new "Optional" classification signal? → A: Extend the existing `LifecycleScope` enum with a new `Optional` variant. Fits the existing `is_non_runtime()` helper for automatic CDX filter parity; naturally participates in the m052 precedence table (dev/build/test/optional as 4 mutually-exclusive lifecycle scopes).
- Q: What should the derivation annotation for the new `Optional` signal be named and shaped? → A: `mikebom:optional-derivation`, string-typed component-level annotation with enum-shaped values `cargo-optional-true`, `npm-optional-dependencies`, `pip-extras-require`, `maven-optional-element`, `gradle-compile-only`, `erlang-optional-applications`. Symmetric with the existing `mikebom:build-inclusion-derivation` from m112; each per-ecosystem reader emits the value that matches the mechanism it detected.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Consumer filters not-in-production components identically across CDX and SPDX 2.3 (Priority: P1)

A consumer of mikebom-produced SBOMs (e.g., pico) wants to strip components not present in the production build from an ingested SBOM before feeding downstream tooling (vulnerability scanners, license auditors, deployment gates). Today the consumer's filter reads CDX's native `scope: "excluded"` field to identify not-in-production components and successfully filters all 23 in the reported yaml.v3 case; the same filter reading SPDX 2.3 typed relationships (`TEST_DEPENDENCY_OF`, `DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`) only catches 13 (the root's direct edges), leaving 10 transitive test-noise components indistinguishable from real runtime deps. The user asks that when a dependency link is not-in-production in mikebom's internal model — regardless of whether that classification came from a manifest declaration, a build-graph analysis, or a feature-flag check — mikebom writes the semantically appropriate SPDX 2.3 typed relationship at every level of the tree.

**Why this priority**: This is the flagship reported user gap. It closes the pico filter-parity divergence and unblocks the "same scan yields the same package set across formats" invariant that downstream consumers depend on.

**Independent Test**: Scan a Go project that transitively pulls in a test-only transitive dep (yaml.v3 → check.v1). Emit both CDX 1.6 and SPDX 2.3. In CDX, count components with `scope: "excluded"`. In SPDX 2.3, count components that appear as the source-side of any typed dep-scope relationship (`TEST_DEPENDENCY_OF` / `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `OPTIONAL_DEPENDENCY_OF`). Both counts MUST match.

**Acceptance Scenarios**:

1. **Given** a Go project where `go mod why` classifies `check.v1` as not-needed-for-production (transitive test-only), **When** mikebom emits SPDX 2.3 under `--spdx2-relationship-compat=full`, **Then** the edge from `yaml.v3` to `check.v1` MUST be emitted as `check.v1 TEST_DEPENDENCY_OF yaml.v3` (reversed-direction convention per m052), NOT as generic `DEPENDS_ON`.
2. **Given** the same scan, **When** the consumer counts CDX components with `scope: "excluded"` and SPDX components that appear as the source-side of any typed dep-scope relationship, **Then** the two counts MUST match exactly.
3. **Given** the same scan, **When** the consumer walks the SPDX relationship graph and collects the set of PURLs that are the source-side of any typed dep-scope relationship, **Then** that set MUST equal the set of PURLs that CDX conveys via `component.scope == "excluded"` — no format-specific filter logic, no `mikebom:*` property parsing required.

---

### User Story 2 — Ecosystem survey establishes the unified `is-optional` design (Priority: P1)

Before mikebom wires per-ecosystem optional-dep classifiers, the project needs an explicit design decision on how the internal model represents "optional" as a first-class concept. Today `lifecycle_scope: Option<LifecycleScope>` (values `Runtime | Development | Build | Test`) and `build_inclusion: Option<BuildInclusion>` (values `Included | NotNeeded | Unknown`) between them cover manifest-declared scope and `go mod why` build-inclusion analysis. Neither carries a bit for "declared as optional / feature-flagged / weakly-referenced". Each supported ecosystem must be surveyed for its optional-dep concept and mapped to a unified internal representation before per-ecosystem readers can populate it consistently.

**Why this priority**: Foundational — without a design decision, per-ecosystem classifiers would each invent their own signal shape, guaranteeing drift. This user story is a research deliverable that gates US3-US7. It MUST be completed and reviewed before any per-ecosystem classifier ships.

**Independent Test**: A design document exists at `specs/179-spdx23-transitive-devscope/research.md` (populated during the `/speckit-plan` phase) containing (a) the ecosystem survey table below, (b) the chosen internal signal shape (a new `LifecycleScope::Optional` variant OR a new `is_optional: bool` field on `ResolvedComponent` OR a new `BuildInclusion::Optional` variant — decided in Q1 below), (c) the internal-state → SPDX 2.3 relationship-type dispatch table.

**Acceptance Scenarios**:

1. **Given** the completed survey table, **When** a reviewer scans every mikebom-supported ecosystem row, **Then** each row MUST identify either (i) a specific ecosystem construct that maps to "optional" (with the exact manifest field / lockfile shape) OR (ii) an explicit "no equivalent construct" verdict with a citation to that ecosystem's dependency spec.
2. **Given** the completed design table, **When** a reviewer walks the internal-state → SPDX 2.3 dispatch, **Then** every valid combination of `(lifecycle_scope, build_inclusion, is_optional-or-equivalent)` MUST have exactly one wire-format target relationship type (no undefined behavior, no ambiguous fallthrough).

---

### User Story 3 — Cargo `optional = true` deps emit `OPTIONAL_DEPENDENCY_OF` (Priority: P2)

Cargo projects declare feature-gated dependencies with `optional = true` in the `[dependencies]` table (activated only when a feature that references them via `dep:<name>` is enabled). These deps may or may not appear in a production build depending on the feature set. Today mikebom's Cargo reader does not distinguish them — every `[dependencies]` entry becomes a plain runtime edge in the SBOM. m179 extends the Cargo reader to detect `optional = true`, populate the new internal signal chosen by Q1, and emit `OPTIONAL_DEPENDENCY_OF` in SPDX 2.3 for those edges.

**Why this priority**: First concrete ecosystem beyond the flagship P1 fix. Cargo is one of the most commonly-scanned ecosystems in mikebom's user base; the feature-flag pattern is idiomatic Rust. Ships as its own user story so it can regress-test independently and be verified against a real Rust fixture.

**Independent Test**: Scan a Rust fixture whose `Cargo.toml` contains at least one `optional = true` dep. Confirm that dep appears in CDX with `scope: "excluded"` AND appears in SPDX 2.3 as the source-side of an `OPTIONAL_DEPENDENCY_OF` relationship.

**Acceptance Scenarios**:

1. **Given** a Cargo.toml containing `foo = { version = "1", optional = true }`, **When** mikebom emits SPDX 2.3 under `--spdx2-relationship-compat=full`, **Then** the resolved `foo` component MUST appear as the source-side of an `OPTIONAL_DEPENDENCY_OF` edge whose target is the enclosing crate.
2. **Given** the same fixture, **When** mikebom emits CDX 1.6, **Then** the `foo` component MUST carry `scope: "excluded"` (per m052's existing `is_non_runtime()` check applied to the new `LifecycleScope::Optional` variant — OR equivalent per Q1 outcome).

---

### User Story 4 — npm `optionalDependencies` emit `OPTIONAL_DEPENDENCY_OF` (Priority: P2)

npm's `package.json` supports two forms of optional deps: (a) the top-level `optionalDependencies` field (installer skips failures), (b) `peerDependenciesMeta.<name>.optional = true` (m147 already handles peer edges; this is a variant that MUST NOT overwrite m178's `PROVIDED_DEPENDENCY_OF` semantic). m179 extends the npm reader (and its yarn/pnpm siblings via existing lockfile parsing) to detect form (a) and populate the new internal signal.

**Why this priority**: Second concrete ecosystem. Broad user-base impact. The peer/optional interaction (form b) needs an explicit contract with m178 — m179 must not disturb m178's `PROVIDED_DEPENDENCY_OF` semantic for peer edges.

**Independent Test**: Scan an npm fixture whose `package.json` has an entry under `optionalDependencies`. Confirm CDX `scope: "excluded"` + SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`. Also scan a fixture with `peerDependenciesMeta.<name>.optional = true` and confirm the m178 `PROVIDED_DEPENDENCY_OF` emission is UNCHANGED.

**Acceptance Scenarios**:

1. **Given** `optionalDependencies: { "foo": "^1" }` in `package.json`, **When** mikebom emits SPDX 2.3, **Then** the `foo` component MUST appear as source-side of an `OPTIONAL_DEPENDENCY_OF` edge.
2. **Given** `peerDependencies: { "bar": "*" }, peerDependenciesMeta: { "bar": { "optional": true } }`, **When** mikebom emits SPDX 2.3, **Then** the `bar` edge MUST remain `PROVIDED_DEPENDENCY_OF` per m178 — the peer-classification precedes the optional-classification.

---

### User Story 5 — pip `extras_require` emits `OPTIONAL_DEPENDENCY_OF` (Priority: P3)

Python's `pyproject.toml` `[project.optional-dependencies.<extra>]` (PEP 621), `setup.py` `extras_require`, and `setup.cfg` `[options.extras_require]` all declare optional extras. m179 extends the pip reader to detect these and populate the new signal.

**Why this priority**: Third-most-scanned ecosystem in mikebom's coverage. Extras are widely used (e.g., `pandas[test]`, `django[argon2]`). Shipped as a lower priority than Cargo/npm because Python's install semantics differ (extras are opt-in at install time, not build-time) — the "optional" mapping is semantically correct but user-facing impact is lower per-unit.

**Independent Test**: Scan a Python fixture with `pyproject.toml` declaring `[project.optional-dependencies.dev]`. Confirm each dep in that section appears as source-side of an `OPTIONAL_DEPENDENCY_OF` edge.

**Acceptance Scenarios**:

1. **Given** `[project.optional-dependencies.dev] = ["pytest>=7"]`, **When** mikebom emits SPDX 2.3, **Then** `pytest` MUST appear as source-side of `OPTIONAL_DEPENDENCY_OF`.

---

### User Story 6 — Maven `<optional>true</optional>` + Gradle `compileOnly` emit `OPTIONAL_DEPENDENCY_OF` (Priority: P3)

Maven's `<optional>true</optional>` element on a `<dependency>` marks a dep as not-transitively-propagated (consumers of THIS artifact must re-declare if they want it). Gradle's `compileOnly` configuration is semantically similar (available at compile time, not packaged at runtime). Both map naturally to `OPTIONAL_DEPENDENCY_OF`.

**Why this priority**: JVM ecosystem is well-supported by mikebom; adding these tightens fidelity. Lower priority than Python because the "optional in production build" question is muddied by shading + fat-jar patterns common in JVM builds.

**Independent Test**: Scan a Maven fixture with `<optional>true</optional>` in `pom.xml`. Confirm `OPTIONAL_DEPENDENCY_OF` in SPDX 2.3 + `scope: "excluded"` in CDX.

**Acceptance Scenarios**:

1. **Given** `<optional>true</optional>` in `pom.xml`, **When** mikebom emits SPDX 2.3, **Then** that dep MUST appear as source-side of `OPTIONAL_DEPENDENCY_OF`.

---

### User Story 7 — Erlang `optional` app dep kind normalization (Priority: P3)

The Erlang reader (m141) already emits a `mikebom:erlang-app-dep-kind` annotation with values `required | included | optional`. Today the `optional` value is a `mikebom:*` property — Principle V says it should be a native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` relationship when the mapping is available. m179 normalizes this: when the reader classifies a dep as `optional`, it populates the unified signal AND continues emitting the finer-grained annotation (Principle V KEEP-BOTH polarity).

**Why this priority**: The signal already exists in the reader — this is normalization work, not new classification. Lower priority because Erlang users are a small slice of the base, but shipping it consolidates the design.

**Independent Test**: Scan an existing Erlang fixture with an `optional` app dep. Confirm the annotation remains + the new `OPTIONAL_DEPENDENCY_OF` edge appears in SPDX 2.3.

**Acceptance Scenarios**:

1. **Given** a `.app.src` declaring `{optional_applications, [foo]}`, **When** mikebom emits SPDX 2.3, **Then** `foo` MUST appear as source-side of `OPTIONAL_DEPENDENCY_OF` AND the `mikebom:erlang-app-dep-kind` annotation MUST remain with value `optional`.

---

### Edge Cases

- **Component with `build_inclusion = NotNeeded` AND the new `Optional` signal set**: Under Q1's dispatch table, `Optional` (semantic: "declared but may or may not be present") wins over `NotNeeded` (semantic: "we ran a build-graph analysis and confirmed it isn't reached") for the SPDX 2.3 type selection — the manifest declaration is more authoritative than the build-graph analysis.
- **Component with manifest `Test/Dev/Build` scope AND the new `Optional` signal set**: `Test/Dev/Build` wins (m052 precedence continues per FR-013). Rare — most ecosystems keep test-scope and optional-scope disjoint — but defense-in-depth.
- **Component reachable via BOTH a manifest-declared runtime edge (from workspace-member A) AND a manifest-declared optional edge (from workspace-member B)**: The stricter classification wins (runtime), matching how mikebom deduplicates on scope today.
- **`--include-dev=false` filter**: Components filtered out at earlier pipeline stages MUST NOT get typed dep-scope relationships (they're not in the SBOM).
- **`--spdx2-relationship-compat=basic` mode**: All typed dep-scope emission collapses to natural-direction `DEPENDS_ON`, including the new `OPTIONAL_DEPENDENCY_OF` path. m228's contract holds uniformly.
- **Ecosystems with no "optional" construct** (per the ecosystem-survey verdict): No classifier logic; components remain classified only by existing `lifecycle_scope` / `build_inclusion` signals. Not a regression.
- **Root component's own edges**: Unaffected — m179 only touches the dep-edge classifier pass, not containment (`DESCRIBES` / `CONTAINS`).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom's internal `LifecycleScope` enum (`mikebom-common/src/resolution.rs`) MUST gain a new `Optional` variant. Every ecosystem reader that detects an optional-dep declaration MUST populate `component.lifecycle_scope = Some(LifecycleScope::Optional)` — no per-reader improvised signals, no companion booleans, no parallel enums.
- **FR-002**: The classifier pass at `mikebom-cli/src/scan_fs/mod.rs:1261` (`apply_lifecycle_scope_to_edges`) MUST be extended to read (a) `lifecycle_scope = Some(LifecycleScope::Optional)` AND (b) `build_inclusion = NotNeeded`. Its dispatch table under `--spdx2-relationship-compat=full` MUST route:
    - target `lifecycle_scope = Test` → internal `TestDependsOn` → SPDX `TEST_DEPENDENCY_OF` (existing m052)
    - target `lifecycle_scope = Development` → internal `DevDependsOn` → SPDX `DEV_DEPENDENCY_OF` (existing m052)
    - target `lifecycle_scope = Build` → internal `BuildDependsOn` → SPDX `BUILD_DEPENDENCY_OF` (existing m052)
    - target `lifecycle_scope = Optional` (new) → internal `OptionalDependsOn` (new) → SPDX `OPTIONAL_DEPENDENCY_OF` (new)
    - target `build_inclusion = NotNeeded` AND `lifecycle_scope = None` → internal `TestDependsOn` → SPDX `TEST_DEPENDENCY_OF` (m179 flagship fix — the pico case)
- **FR-003**: Under `--spdx2-relationship-compat=basic`, ALL of the above typed emissions MUST collapse to natural-direction `DEPENDS_ON` per m228's contract. Zero new `*_DEPENDENCY_OF` edges under basic mode.
- **FR-004**: The `SpdxRelationshipType` enum MUST gain an `OptionalDependencyOf` variant that serializes to the SPDX 2.3 wire value `"OPTIONAL_DEPENDENCY_OF"`.
- **FR-005**: The internal `RelationshipType` enum MUST gain an `OptionalDependsOn` variant. It follows the same reversed-direction convention as the other `*DependsOn` variants: internal `(A) OptionalDependsOn (B)` → SPDX `(B) OPTIONAL_DEPENDENCY_OF (A)` "B is an optional dependency of A".
- **FR-006**: The `LifecycleScope::is_non_runtime()` helper — which drives CDX `scope: "excluded"` — MUST return `true` for the new `LifecycleScope::Optional` variant, preserving the CDX filter-parity invariant automatically. Because Q1's answer extends the existing enum, this is the same one-line match-arm addition m052 used for `Test|Development|Build`.
- **FR-007**: Every mikebom-supported ecosystem MUST be surveyed for its optional-dep construct in the research artifact (`research.md`). The survey table MUST include one row per ecosystem with columns: ecosystem name, manifest construct (or "no equivalent"), lockfile construct (or "no equivalent"), lifecycle-vs-optional distinction (some ecosystems muddle these), test/dev/build/optional dispatch verdict.
- **FR-008**: The Cargo reader MUST detect `optional = true` in `[dependencies]` / `[target.<cfg>.dependencies]` and set `component.lifecycle_scope = Some(LifecycleScope::Optional)` + `mikebom:optional-derivation = "cargo-optional-true"` on the target component.
- **FR-009**: **[Deferred to m180 per tasks.md Implementation Strategy]** The npm reader (and its yarn/pnpm siblings) MUST detect `optionalDependencies` entries in `package.json` and set `LifecycleScope::Optional` + `mikebom:optional-derivation = "npm-optional-dependencies"`. `peerDependenciesMeta.<name>.optional = true` MUST NOT re-classify — m178's `PROVIDED_DEPENDENCY_OF` semantic wins for those edges (precedence per FR-015 defended by test).
- **FR-010**: **[Deferred to m181 per tasks.md Implementation Strategy]** The pip reader MUST detect `[project.optional-dependencies.<extra>]` (pyproject.toml), `extras_require` (setup.py), `[options.extras_require]` (setup.cfg) and set `LifecycleScope::Optional` + `mikebom:optional-derivation = "pip-extras-require"`.
- **FR-011**: **[Deferred to m182 per tasks.md Implementation Strategy]** The Maven reader MUST detect `<optional>true</optional>` on `<dependency>` elements and set `LifecycleScope::Optional` + `mikebom:optional-derivation = "maven-optional-element"`.
- **FR-012**: **[Deferred to m182 per tasks.md Implementation Strategy]** The Gradle reader SHOULD detect `compileOnly` configuration deps and set `LifecycleScope::Optional` + `mikebom:optional-derivation = "gradle-compile-only"`. "SHOULD" rather than "MUST" acknowledges the Assumption-section scope-release-valve — if build-model resolution proves out of reach for the m182 delivery slice, US6 MAY be further split into a Maven-only PR + a Gradle-later PR.
- **FR-013**: **[Deferred to m183 per tasks.md Implementation Strategy]** The Erlang reader (m141) MUST set `LifecycleScope::Optional` + `mikebom:optional-derivation = "erlang-optional-applications"` when it classifies a dep as `optional_applications` in `.app.src`. The existing `mikebom:erlang-app-dep-kind` annotation MUST remain present with byte-identical value (Principle V KEEP-BOTH: native signal + finer annotation).
- **FR-014**: When a target component has BOTH `lifecycle_scope = Some(LifecycleScope::Optional)` AND `build_inclusion = Some(BuildInclusion::NotNeeded)`, the `Optional` classification MUST win the dispatch (`lifecycle_scope` is checked first at FR-002's dispatch table; the manifest declaration is more authoritative than the build-graph analysis).
- **FR-015**: When a target component has BOTH `lifecycle_scope = Some(LifecycleScope::Optional)` initially set by an ecosystem reader AND is later re-tagged via manifest-precedence to `Some(Test|Development|Build)`, the `Test|Development|Build` classification MUST win (m052 precedence continues; consistent with the m112 "never downgrade an existing test tag" rule). Implementation note: FR-001's constraint that all readers set the ONE `lifecycle_scope` field means this precedence is enforced by the ordering of reader passes + deduplication rules already in place.
- **FR-016**: The CDX 1.6 emission MUST show `scope: "excluded"` for every component with `lifecycle_scope = Some(LifecycleScope::Optional)` (via FR-006's `is_non_runtime()` extension), matching the m052 + m112 CDX behavior — CDX filter parity holds automatically without new CDX-side logic.
- **FR-017**: The SPDX 3.0.1 emission MUST NOT change materially. SPDX 3's `LifecycleScopeType` enum does not include an `optional` value at spec version 3.0.1; m179 continues to emit no `lifecycleScope` parameter for `Optional`-classified components on SPDX 3 relationships. If a future SPDX 3 update adds it, a follow-up milestone can hook it in. The `mikebom:optional-derivation` annotation MUST round-trip through SPDX 3 emission for parity per SC-008.
- **FR-018**: The `mikebom:build-inclusion` and `mikebom:build-inclusion-derivation` annotations MUST continue to be emitted with byte-identical value in both CDX, SPDX 2.3, and SPDX 3 output. Same rationale as m178's `mikebom:peer-edge-targets`: the annotation carries derivation source (which the standard doesn't natively express).
- **FR-019**: The new `mikebom:optional-derivation` annotation MUST be emitted alongside every `LifecycleScope::Optional` classification, with a string value drawn from the enum vocabulary `{cargo-optional-true, npm-optional-dependencies, pip-extras-require, maven-optional-element, gradle-compile-only, erlang-optional-applications}`. The value MUST appear byte-identically in CDX 1.6, SPDX 2.3, and SPDX 3.0.1 output for the same scan (parity gate per SC-008). Additional value strings MAY be added by future ecosystem coverage milestones without changing the annotation name.

### Key Entities

- **Component (`ResolvedComponent`)**: Uses its existing `lifecycle_scope: Option<LifecycleScope>` field to carry the new `Optional` classification via the extended `LifecycleScope::Optional` variant. No new field is added. Existing consumers (`is_non_runtime()`, m052 typed-edge classifier, CDX `scope: "excluded"` emitter, deduplicator's scope-precedence table) all pick up the new variant via one match-arm addition each.
- **`LifecycleScope` enum**: Extended from `{Runtime, Development, Build, Test}` → `{Runtime, Development, Build, Test, Optional}`. `is_non_runtime()` returns `true` for `Optional`.
- **Relationship (`Relationship`) + `RelationshipType`**: Gains a new `OptionalDependsOn` variant. Existing variants are unchanged.
- **SPDX 2.3 relationship type vocabulary (`SpdxRelationshipType`)**: Gains a new `OptionalDependencyOf` variant serialized as `"OPTIONAL_DEPENDENCY_OF"`. Existing variants unchanged.
- **`mikebom:optional-derivation` annotation**: New component-level `extra_annotations` entry populated by every ecosystem reader that sets `LifecycleScope::Optional`. String-typed enum-shaped value per FR-019. Round-trips through all three format emitters (CDX 1.6, SPDX 2.3, SPDX 3.0.1) verbatim.
- **Ecosystem survey table**: A research-phase artifact (not runtime state) enumerating each supported ecosystem's optional-dep construct and its dispatch verdict. Emitted at `specs/179-spdx23-transitive-devscope/research.md` during the plan phase.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the reported Go scan whose CDX 1.6 output contains 23 components with `scope: "excluded"`, the SPDX 2.3 output (under `--spdx2-relationship-compat=full`) MUST contain 23 distinct components that appear as source-side of at least one typed dep-scope relationship — pico's filter yields byte-identical package sets across formats.
- **SC-002**: For every mikebom regression fixture, the set of PURLs that appear as source-side of any typed dep-scope relationship in SPDX 2.3 MUST equal the set of PURLs marked `scope: "excluded"` in the paired CDX file — cross-format filter-set equality, verified as a new CI gate.
- **SC-003**: Zero mikebom regression fixture SPDX 2.3 golden files show NET-DECREMENT in `*_DEPENDENCY_OF` edge counts pre-vs-post m179. Net-INCREMENT is expected for any fixture where the new classification signals populate.
- **SC-004**: Zero drift in any mikebom CDX 1.6 golden file that does not exercise a new `Optional`-signal reader (US3-US7). CDX filter parity for those fixtures is inherited from FR-006.
- **SC-005**: Zero drift in any mikebom SPDX 3.0.1 golden file (m179 does not touch SPDX 3 emission).
- **SC-006**: Under `--spdx2-relationship-compat=basic`, the SPDX 2.3 golden files show zero new `OPTIONAL_DEPENDENCY_OF` edges — every `Optional`-classified edge is natural-direction `DEPENDS_ON` under basic mode.
- **SC-007**: The ecosystem survey table (research.md) covers every ecosystem that appears in `mikebom-cli/src/scan_fs/package_db/mod.rs`'s reader dispatch (Cargo, npm/yarn/pnpm, pip/uv/poetry, Maven, Gradle, gem/bundler, composer, cocoapods, elixir/mix, erlang/rebar, scala/sbt, haskell/cabal-stack, dart/pub, cmake, bazel, conan, vcpkg, west, Go, NuGet, Homebrew, alpm, dpkg, apk, rpm, ipk, opkg, yocto). Every row MUST have either a manifest-construct citation OR an explicit "no equivalent" verdict.
- **SC-008**: The `mikebom:build-inclusion-derivation` (m112 legacy) + new `mikebom:optional-derivation` (per Q2) annotations MUST be emitted with byte-identical value across CDX 1.6, SPDX 2.3, and SPDX 3.0.1 for every fixture that exercises them — same round-trip parity as m147's `mikebom:peer-edge-targets`.

## Assumptions

- The reader may safely assume that `apply_lifecycle_scope_to_edges` (`scan_fs/mod.rs:1261`) is the single dispatch site for `DependsOn → typed variant` rewrites. m179 extends this pass — it does not introduce a competing pass.
- The reader may safely assume that CDX 1.6's `scope: "excluded"` (driven by `LifecycleScope::is_non_runtime()` OR `build_inclusion = NotNeeded`) and SPDX 2.3's typed dep-scope verbs are the two canonical native signals for "not-in-production-deployment" per Principle V.
- SPDX 3.0.1's `LifecycleScopeType` enum lacks an `optional` value at spec version 3.0.1 — m179 leaves SPDX 3 emission untouched. A future SPDX 3 spec update may add one, at which point a follow-up milestone hooks it in.
- m112's `NotNeeded` classifier does not currently carry a "why NotNeeded" reason code beyond `go-mod-why`. Emitting `TEST_DEPENDENCY_OF` for all m112 NotNeeded is a semantic overloading (some are declared-but-truly-unused deps, not test-only-transitives) — accepted per user direction to close the pico gap; a follow-up milestone may add a reason code and refine the dispatch.
- m228's `--spdx2-relationship-compat=<full|basic>` flag is the correct opt-out mechanism for consumers that don't want typed dep-scope verbs. m179 hooks into the same flag rather than introducing a new one.
- The Cargo `optional = true` semantics apply at the `[dependencies]` table level. `[dev-dependencies]` and `[build-dependencies]` are already classified by their table, not by any `optional = true` inside — so an `optional = true` inside `[dev-dependencies]` continues to classify as `Development` (per FR-015 precedence).
- The npm `optionalDependencies` field is disjoint from `dependencies`/`devDependencies` per npm's schema; if a package appears in both, that's a package.json authoring error and mikebom follows npm's own behavior (deps win over optionalDependencies).
- Gradle `compileOnly` classification MAY require build-model resolution beyond what the current mikebom Gradle reader supports; if the classification is infeasible for m179 delivery, US6 MAY be split into a follow-up milestone (FR-012 becomes "add a placeholder + document limitation" rather than "fully wire it").
- The Erlang reader already has the `optional` classification internally (the `mikebom:erlang-app-dep-kind` annotation proves it) — US7 is normalization, not new classification work.
- Pico (the reporter) filters on native fields only. m179 preserves that consumer contract by preserving Principle V's native-first design + a KEEP-BOTH polarity for the derivation annotation (m178 pattern).

## Constitution Alignment

**Principle V** (v1.4.0): "standards-native fields take precedence over `mikebom:`-prefixed properties." m179 is a direct continuation of m178's KEEP-BOTH polarity: the internal model carries a classification (`Optional` signal); the standard (SPDX 2.3) has a native way to express it (`OPTIONAL_DEPENDENCY_OF`); we elevate the native emission and keep a `mikebom:*` derivation annotation as a finer-grained supplement (the annotation records WHICH mechanism populated the signal, which the standard doesn't natively express).

**Principle II** (Fail Closed): m179 must not silently swap the semantic of edges under `--spdx2-relationship-compat=basic` — that flag is the operator's opt-out from typed verbs, and it MUST remain honored (FR-003).

**Principle IX** (Accuracy): The reported user-visible failure is that the same scan yields non-interchangeable SBOMs across formats. m179 restores accuracy by making the two format outputs semantically equivalent for the "not-in-production" filter question (SC-001, SC-002).

**Principle X** (Transparency): The derivation annotation (FR-019, Q2) MUST make it observable WHICH mechanism populated each `Optional` classification — operator can audit whether a component was flagged optional due to Cargo, npm, pip, Maven, Gradle, Erlang, or something else. This is a Principle X (Transparency) requirement even after Principle V native-first emission.
