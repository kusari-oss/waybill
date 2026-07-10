# Feature Specification: yarn v1 + Berry optional-dependency classification

**Feature Branch**: `181-yarn-optional-dep`
**Created**: 2026-07-10
**Status**: Draft
**Input**: User description: "m181 — deferred US3 from m180. Wire the yarn.lock reader (v1 + Berry) to detect optional-declared deps and classify them as `LifecycleScope::Optional` + emit `mikebom:optional-derivation = "npm-optional-dependencies"`. Currently `build_entry` at `yarn_lock.rs:378` emits `lifecycle_scope: None`; this milestone plumbs the missing classification. Yarn v1: name-membership set built from `optionalDependencies:` sub-blocks. Yarn Berry: package.json's `dependenciesMeta.<name>.optional = true`. Reuses m180's shared `is_peer_optional` helper (currently marked `#[allow(dead_code)]` awaiting yarn usage) for the m178 peer-precedence guard."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — yarn v1 `optionalDependencies:` sub-block deps map to `LifecycleScope::Optional` (Priority: P1)

Yarn Classic (v1) projects still make up a meaningful slice of mikebom's JavaScript scan target base — corporate LTS Node.js codebases, older Ember/Meteor projects, and any project that hasn't migrated to Berry. yarn v1's `yarn.lock` text-format entries include an `optionalDependencies:` sub-block on parent entries naming child deps declared as optional. Today mikebom's yarn v1 parser (at `parse_v1` in `yarn_lock.rs`) READS these sub-blocks (line 183) but collapses them into the same `dep_names` vector as regular `dependencies:` — losing the optional distinction — and every emitted component gets `lifecycle_scope: None`.

**Why this priority**: Same pico filter-parity gap m180 closed for npm/pnpm, extended to yarn v1. Yarn v1 users get the CDX `scope: "excluded"` + SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` filter parity for free once the reader plumbs the classification through.

**Independent Test**: Scan a yarn v1 project whose `yarn.lock` has at least one parent entry with an `optionalDependencies:` sub-block. Verify (a) the target child component gets `LifecycleScope::Optional` + the derivation annotation, (b) CDX emits `scope: "excluded"` on it, (c) SPDX 2.3 emits `<child> OPTIONAL_DEPENDENCY_OF <parent>` under `--spdx2-relationship-compat=full`.

**Acceptance Scenarios**:

1. **Given** a yarn v1 `yarn.lock` where `parent-pkg` declares `optionalDependencies:` naming `optional-child`, **When** mikebom emits SPDX 2.3 under `--spdx2-relationship-compat=full`, **Then** `optional-child` MUST appear as source-side of an `OPTIONAL_DEPENDENCY_OF` edge (reversed direction per m052 convention), NOT as a plain `DEPENDS_ON` target.
2. **Given** the same scan, **When** mikebom emits CDX 1.6, **Then** `optional-child` MUST carry `scope: "excluded"` + `mikebom:optional-derivation = "npm-optional-dependencies"`.
3. **Given** a yarn v1 project where `regular-child` appears in a `dependencies:` sub-block, **When** mikebom emits SPDX 2.3, **Then** `regular-child` MUST NOT be classified as `LifecycleScope::Optional` — its edge stays `DEPENDS_ON`.

---

### User Story 2 — yarn Berry (v2+) `dependenciesMeta.<name>.optional = true` maps to `LifecycleScope::Optional` (Priority: P1)

Yarn Berry (v2, v3, v4+) stores optional-dep information out-of-band from the lockfile: `package.json`'s `dependenciesMeta.<name>.optional = true` marks a dep as optional. The lockfile itself (`yarn.lock` Berry-format, YAML-shaped) does NOT carry the flag. mikebom's Berry parser today reads only the lockfile and produces components with `lifecycle_scope: None`. This milestone plumbs the package.json cross-reference so the classification propagates.

**Why this priority**: Berry is the modern-yarn target; new projects choose Berry. Closes the same filter-parity gap as US1 for the growth-line yarn user.

**Independent Test**: Scan a yarn Berry project whose `package.json` declares `dependenciesMeta: {"foo": {"optional": true}}`. Verify (a) `foo` component gets `LifecycleScope::Optional` + annotation, (b) CDX emits `scope: "excluded"`, (c) SPDX 2.3 emits `foo OPTIONAL_DEPENDENCY_OF <root>`.

**Acceptance Scenarios**:

1. **Given** a yarn Berry project with `package.json` declaring `dependenciesMeta: {"foo": {"optional": true}}` and a matching yarn.lock entry for foo, **When** mikebom emits SPDX 2.3 under `--spdx2-relationship-compat=full`, **Then** `foo` MUST emit as source-side of `OPTIONAL_DEPENDENCY_OF`.
2. **Given** the same scan, **When** mikebom emits CDX 1.6, **Then** `foo` MUST carry `scope: "excluded"` + `mikebom:optional-derivation = "npm-optional-dependencies"`.

---

### User Story 3 — m178 peer-optional precedence guard for yarn (Priority: P1)

Yarn (both v1 and Berry) supports `peerDependencies` + `peerDependenciesMeta.<name>.optional = true` in package.json for peer-optional deps. Per m179 FR-006 + m180 US4's ratified precedence, the m178 `PROVIDED_DEPENDENCY_OF` classification MUST win over m180/m181's `OPTIONAL_DEPENDENCY_OF` for such edges. The `is_peer_optional` helper from m180 (currently marked `#[allow(dead_code)]` awaiting yarn usage) is the canonical guard — this milestone consumes it.

**Why this priority**: Regression pin for the m178/m181 interaction. Elevated to P1 (same as m180 US4) because a silent regression on peer-precedence would go unnoticed without a specific fixture-based test.

**Independent Test**: Scan a yarn fixture where `package.json` has BOTH `peerDependencies: {"react": "^18"}` AND `peerDependenciesMeta: {"react": {"optional": true}}`, AND `dependenciesMeta: {"react": {"optional": true}}` (Berry-style optional). Verify SPDX 2.3 emits `PROVIDED_DEPENDENCY_OF` on the react edge, NOT `OPTIONAL_DEPENDENCY_OF`. The `mikebom:optional-derivation` annotation MUST NOT appear on react.

**Acceptance Scenarios**:

1. **Given** a yarn Berry project with react in BOTH `peerDependencies + peerDependenciesMeta.<name>.optional = true` AND `dependenciesMeta.<name>.optional = true`, **When** mikebom emits SPDX 2.3 under Full mode, **Then** the react edge MUST emit as `PROVIDED_DEPENDENCY_OF` (m178 semantic wins), NOT as `OPTIONAL_DEPENDENCY_OF`.
2. **Given** the same fixture, **When** mikebom emits CDX 1.6, **Then** the react component MUST NOT carry `mikebom:optional-derivation`.

---

### Edge Cases

- **yarn v1 dep appearing in BOTH `dependencies:` AND `optionalDependencies:` sub-blocks of the same parent**: per npm's own semantic (non-optional wins in a conflict), the reader MUST classify the dep as Runtime, not Optional. The `optionalDependencies:` classification is skipped when the same name also appears in `dependencies:`.
- **yarn v1 dep declared as optional by one parent AND as regular by another** (diamond-shape): the stricter (Runtime) classification wins — same rule as m180's diamond-shape.
- **yarn Berry `dependenciesMeta` entry with NO matching `dependencies` entry** (an authoring anomaly): mikebom does NOT synthesize a phantom component. The entry only classifies as Optional if the lockfile actually resolved a matching component. Zero components are added by m181 — this is a classification-only milestone.
- **Yarn workspace member package.json declaring its own `dependenciesMeta`**: m181 initially scopes to the ROOT project's package.json only. Workspace-member `dependenciesMeta` may defer to a follow-up milestone (documented in Assumptions).
- **`--spdx2-relationship-compat=basic`**: all typed dep-scope edges collapse to natural-direction `DEPENDS_ON` per m228 — same behavior as m179/m180.
- **`--include-dev=false`**: `LifecycleScope::Optional` components are filtered out alongside `Dev/Build/Test` via `is_non_runtime()` — same behavior as m180.
- **Package.json missing** (rare in a yarn-locked project but possible under some CI setups): the reader falls back to lockfile-only parsing; no Optional classification is possible; components stay `None` per pre-m181 behavior (fail-safe regression).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The yarn v1 parser (`parse_v1` in `yarn_lock.rs`) MUST distinguish between `dependencies:` and `optionalDependencies:` sub-blocks when walking a parent entry's body. It MUST accumulate a set of optional-child names (deduped across all parents) for the classifier pass.
- **FR-002**: For yarn v1, each emitted component whose name matches an optional-child name (and does NOT also appear in any parent's regular `dependencies:` sub-block per FR-007's diamond-shape rule) MUST be classified as `LifecycleScope::Optional` and MUST carry `mikebom:optional-derivation = "npm-optional-dependencies"`.
- **FR-003**: The yarn Berry parser (`parse_berry`) MUST cross-reference the root project's `package.json` for `dependenciesMeta.<name>.optional = true` entries. Each emitted Berry component whose name matches such an entry MUST be classified as `LifecycleScope::Optional` + emit the annotation.
- **FR-004**: The `read_yarn_lock` entry point MUST plumb the root `package.json` (as `serde_json::Value`) through to both v1 and Berry parsers so the parsers can access `peerDependencies` + `peerDependenciesMeta` + `dependenciesMeta` fields. If `package.json` is missing or unparseable, the reader falls back to lockfile-only parsing with a `tracing::warn!` diagnostic; no components are classified as Optional in that case.
- **FR-005**: When a component name appears BOTH in an `optionalDependencies:` sub-block (v1) or `dependenciesMeta` (Berry) AND in the parent's `peerDependencies` map with `peerDependenciesMeta.<name>.optional = true` (peer-optional), the m178 peer classification MUST win — the component's `lifecycle_scope` stays Runtime (or None if unclassifiable), the `mikebom:optional-derivation` annotation MUST NOT be emitted on it, and m178's `PROVIDED_DEPENDENCY_OF` continues to fire via the existing m147 peer-edge flow. This reuses the shared `is_peer_optional` helper introduced in m180 (`peer_optional.rs`).
- **FR-006**: `build_entry` at `yarn_lock.rs:378` MUST be extended to accept a `LifecycleScope` classification and an `extra_annotations` map, OR the reader MUST post-process the returned `Vec<PackageDbEntry>` to apply the classification + annotation before returning to the caller. The specific plumbing is a planning-phase decision.
- **FR-007**: When a component name appears in BOTH `optionalDependencies:` and regular `dependencies:` sub-blocks of the same parent's yarn v1 entry, the Runtime classification wins (non-optional beats optional per npm's own resolver semantic). The `mikebom:optional-derivation` annotation MUST NOT be emitted on such a component.
- **FR-008**: The CDX 1.6 emission MUST show `scope: "excluded"` for every yarn-classified `LifecycleScope::Optional` component — auto-inherited via the m179 `is_non_runtime()` extension.
- **FR-009**: The SPDX 3.0.1 emission MUST NOT include a native `lifecycleScope: optional` value on any relationship (SPDX 3.0.1 has no such enum value per m179 FR-017); the annotation IS the SPDX 3 classification carrier.
- **FR-010**: Under `--spdx2-relationship-compat=basic`, all new `OPTIONAL_DEPENDENCY_OF` emissions collapse to natural-direction `DEPENDS_ON` per m228's contract. Zero new typed dep-scope edges under basic mode.
- **FR-011**: For scans that do NOT exercise the new yarn signal (all non-yarn ecosystem fixtures + yarn fixtures with no optional-declared deps), the emitted CDX 1.6, SPDX 2.3, and SPDX 3.0.1 documents MUST be byte-identical to the pre-m181 baseline (regression guard, same shape as m180 FR-012).
- **FR-012**: The `--include-dev=false` filter MUST filter out `LifecycleScope::Optional` components alongside `Dev/Build/Test` (via the existing `is_non_runtime()` check).
- **FR-013**: The existing yarn regression tests (m106 US5, m159 aliases) MUST continue to pass byte-identically — the m181 change is additive.

### Key Entities

- **Component (`ResolvedComponent`)**: Reuses m179's `LifecycleScope::Optional` variant and the `mikebom:optional-derivation` annotation. Zero new types.
- **`mikebom:optional-derivation` annotation**: Reused verbatim from m180 with value `"npm-optional-dependencies"` (shared across npm/yarn/pnpm/bun per m180 design).
- **Yarn v1 `optionalDependencies:` sub-block**: text-format construct at 4-space indent inside a parent entry's 2-space-indent body block.
- **Yarn Berry `package.json` `dependenciesMeta.<name>.optional`**: out-of-band JSON boolean.
- **Root package.json access via `read_yarn_lock`**: new plumbing to enable peer-precedence guard + Berry cross-reference.
- **Shared `is_peer_optional` helper** (from m180 `peer_optional.rs`): consumed by yarn — closes the `#[allow(dead_code)]` marker that m180 left as a "yarn will use this" placeholder.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of a yarn v1 fixture with N `optionalDependencies:` sub-block children MUST have the SET of yarn-emitted PURLs marked `scope: "excluded"` in CDX 1.6 equal to the SET of PURLs that appear as source-side of any typed dep-scope relationship (`OPTIONAL_DEPENDENCY_OF` primarily; `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` if the reader marked them) in SPDX 2.3. Same pico filter-parity gate as m179/m180 SC-001, extended to yarn v1.
- **SC-002**: Same set-equality gate for yarn Berry fixtures with `dependenciesMeta` entries.
- **SC-003**: Zero mikebom regression fixture SPDX 2.3 golden files experience NET-DECREMENT in `*_DEPENDENCY_OF` edge counts pre-vs-post m181. Net-INCREMENT is expected on yarn fixtures with the new classification.
- **SC-004**: Zero drift in any mikebom CDX 1.6 golden file that does not exercise the new signal (regression guard on non-yarn fixtures — same shape as m180 SC-003).
- **SC-005**: Zero drift in any mikebom SPDX 3.0.1 golden file (m181 does not touch SPDX 3 emission per FR-009).
- **SC-006**: Under `--spdx2-relationship-compat=basic`, yarn goldens show zero new `OPTIONAL_DEPENDENCY_OF` edges — every optional-classified edge is natural-direction `DEPENDS_ON` (m228 escape hatch preserved).
- **SC-007**: For fixtures exercising the peer-optional collision case (US3), the emitted SPDX 2.3 output MUST NOT contain the peer-optional target as source-side of `OPTIONAL_DEPENDENCY_OF` — every such edge MUST emit as `PROVIDED_DEPENDENCY_OF` (m178 semantic preserved).
- **SC-008**: Existing yarn regression tests (m106 US5 baseline + m159 alias tests) MUST continue to pass byte-identically.
- **SC-009**: The `mikebom:optional-derivation` annotation with value `"npm-optional-dependencies"` MUST appear byte-identically in CDX 1.6, SPDX 2.3, and SPDX 3.0.1 output for every yarn fixture that exercises the m181 classification. C122 parity extractor (registered in m179) validates this automatically via `SymmetricEqual` polarity.

## Assumptions

- The reader may safely assume that yarn v1's `optionalDependencies:` sub-block is the canonical source of optional-dep information in v1 lockfiles (mirroring npm's `optionalDependencies` map in package.json).
- The reader may safely assume that yarn Berry's `dependenciesMeta.<name>.optional = true` in package.json is the canonical source for Berry's optional-dep information. Berry's lockfile does NOT carry this flag — cross-referencing package.json is unavoidable.
- The m181 scope is restricted to the ROOT project's `package.json`. Yarn workspaces (both v1 and Berry) MAY have per-workspace `package.json` files with their own `dependenciesMeta` / `peerDependenciesMeta`. Workspace-member cross-reference is DEFERRED to a follow-up milestone — a single-workspace fixture is the m181 delivery target. Documented as a Known Limitation in the m181 tasks.md if needed.
- The single `"npm-optional-dependencies"` derivation value is reused from m180 (no new value for yarn's classification — the underlying concept is the same npm-registry optionalDependencies pattern regardless of package manager, per m179's research artifact Decision 1 table).
- m180's `is_peer_optional` helper at `peer_optional.rs` (currently `#[allow(dead_code)]`) is the ready-made peer-precedence guard. m181 consumes it, allowing the `#[allow(dead_code)]` marker to be removed.
- Golden fixture regeneration (`MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`) will show additive-only changes on fixtures that exercise the new signal — same regeneration pattern established by m179's T028-T030.
- The reader plumbing change from FR-004 (parse + pass package.json JSON) does NOT require a new Cargo dependency (`serde_json` is already pervasive).
- The yarn v1 dep-block parser at line 183 already recognizes the `optionalDependencies:` keyword — m181's implementation cost for FR-001 is scoping the accumulator to per-sub-block-source rather than a flat merge.

## Constitution Alignment

**Principle V** (v1.4.0): Direct continuation of m180's KEEP-BOTH polarity. Native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` is the primary signal (elevated from generic `DEPENDS_ON`); `mikebom:optional-derivation = "npm-optional-dependencies"` is the finer-grained supplement. Zero new `mikebom:*` invention — the entire signal flows through the C122 catalog row m179 registered. No new Principle V audit surface.

**Principle IX** (Accuracy): The pico filter-parity gap m179 closed for Go + Cargo, m180 for npm + pnpm, now closes for yarn v1 + Berry — same measurable accuracy gain (SC-001, SC-002).

**Principle X** (Transparency): Consumers of the emitted SBOM can distinguish "this component was classified optional by a yarn lockfile" via the shared `mikebom:optional-derivation = "npm-optional-dependencies"` value + `evidence.source_file_paths` field pointing at the specific `yarn.lock` variant.
