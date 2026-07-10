# Feature Specification: npm / yarn / pnpm optional-dependency classification

**Feature Branch**: `180-npm-optional-dep-reader`
**Created**: 2026-07-09
**Status**: Draft
**Input**: User description: "m180 — continuation of m179's unified `LifecycleScope::Optional` classification for the JavaScript ecosystem. Wire the npm / yarn / pnpm lockfile readers to detect optional-declared deps (via each lockfile's own per-entry mechanism) and set `LifecycleScope::Optional` + `mikebom:optional-derivation = "npm-optional-dependencies"`. Preserve the m178 peer-precedence guard: `peerDependenciesMeta.<name>.optional = true` MUST NOT downgrade a peer-classified edge — the SPDX 2.3 `PROVIDED_DEPENDENCY_OF` emission wins over `OPTIONAL_DEPENDENCY_OF` for those specific edges (per m179 FR-009 + FR-015). This is the m180 slot on m179's plan.md Decision 4 delivery cadence: US1 (pico Go fix) + US2 (research) + US3 (Cargo) shipped in m179; US4 (npm family) is m180."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — npm `package-lock.json` optional deps map to `LifecycleScope::Optional` (Priority: P1)

The most common JavaScript scan target is a Node.js project with an npm-generated `package-lock.json` v2/v3 lockfile. When the operator declares a dep in `package.json`'s top-level `optionalDependencies` field, npm's install-and-lock cycle propagates `optional: true` onto every corresponding entry in `packages.<path>` throughout the lockfile (both the direct entry AND every transitively-reachable-only-through-optional descendant). Today mikebom's npm reader USES this flag as a filter signal (drops the component when `--include-dev=false`) but does NOT translate it to `LifecycleScope::Optional` when the component IS emitted. As a consequence, the SPDX 2.3 output emits every optional dep as generic `DEPENDS_ON` — the same pico filter-parity gap m179 closed for Go, but for npm.

**Why this priority**: npm is mikebom's most-scanned JavaScript ecosystem; the pico use case that motivated m179 also directly applies here. Fixing npm delivers the same cross-format filter-parity value on the largest JavaScript user share.

**Independent Test**: Scan an npm project whose `package.json` declares `optionalDependencies: {"foo": "^1"}` and whose `package-lock.json` was generated with a hydrated tree. Verify CDX emits `foo` with `scope: "excluded"` AND SPDX 2.3 emits `foo OPTIONAL_DEPENDENCY_OF <root>` (reversed-direction per m052 convention) under `--spdx2-relationship-compat=full`.

**Acceptance Scenarios**:

1. **Given** a `package.json` with `optionalDependencies: {"fsevents": "^2"}` and a hydrated `package-lock.json` where the `packages/node_modules/fsevents` entry carries `optional: true`, **When** mikebom emits SPDX 2.3 under `--spdx2-relationship-compat=full`, **Then** the edge from the root to `fsevents` MUST emit as `fsevents OPTIONAL_DEPENDENCY_OF <root>` (reversed direction), NOT as `DEPENDS_ON`.
2. **Given** the same scan, **When** mikebom emits CDX 1.6, **Then** the `fsevents` component MUST carry `scope: "excluded"` (auto-inherited via `LifecycleScope::is_non_runtime()`) AND MUST carry a `mikebom:optional-derivation` property with value `"npm-optional-dependencies"`.
3. **Given** a transitively-reached optional dep (e.g., a dep of fsevents where every path through the graph passes through fsevents), **When** mikebom classifies the transitive, **Then** it MUST also emit `LifecycleScope::Optional` (npm propagates the `optional: true` flag through the tree; mikebom respects the propagation).

---

### User Story 2 — pnpm `pnpm-lock.yaml` optional deps map to `LifecycleScope::Optional` (Priority: P1)

pnpm-lock.yaml (v9+ per m157) has TWO carriers for optional-ness: (a) a top-level `optionalDependencies:` block per-importer that enumerates optional dep names, and (b) a per-package `optional: true` marker on individual `packages.<key>` entries when the dep is reached only through an optional edge. mikebom's pnpm reader today uses the per-package marker as a filter signal but does NOT translate it to `LifecycleScope::Optional` — same gap as npm.

**Why this priority**: pnpm is the second-most-common JavaScript lockfile mikebom scans (behind npm, ahead of yarn). Same user-facing filter-parity value.

**Independent Test**: Scan a pnpm project with `optionalDependencies: {foo: '^1'}` in `pnpm-lock.yaml`'s importers block. Verify CDX + SPDX 2.3 emissions match the same shape as US1.

**Acceptance Scenarios**:

1. **Given** a `pnpm-lock.yaml` v9 with `importers/.:optionalDependencies:{foo: ...}` AND a `packages/foo@1.2.3:optional: true` entry, **When** mikebom emits SPDX 2.3, **Then** `foo` MUST emit as source-side of `OPTIONAL_DEPENDENCY_OF`.
2. **Given** the same scan, **When** mikebom emits CDX 1.6, **Then** `foo` MUST carry `scope: "excluded"` + `mikebom:optional-derivation = "npm-optional-dependencies"`.

---

### User Story 3 — yarn.lock v1 + Berry (v2/3) optional deps map to `LifecycleScope::Optional` (Priority: P2)

yarn v1 and Berry (v2/v3) use different lockfile shapes but share the concept of `optionalDependencies:`. In yarn v1, each parent entry may have an `optionalDependencies:` sub-block naming children declared as optional. In Berry, the mechanism uses `dependenciesMeta.<name>.optional: true` in `package.json` (parsed via yarn's own metadata). mikebom's yarn.lock reader today parses `optionalDependencies:` sub-blocks but LEAVES the resulting component's `lifecycle_scope` unset (line 378 emits `lifecycle_scope: None`). m180 wires the missing plumbing.

**Why this priority**: yarn's user share is smaller than npm/pnpm but non-trivial; also, this user story exposes a bigger internal change (the reader currently returns `None`, requiring lifecycle plumbing rather than a simple bool-to-enum switch), so it merits its own story for isolated regression review.

**Independent Test**: Scan two yarn-fixtures — one v1, one Berry — each with at least one optional-declared dep. Verify all three format emissions.

**Acceptance Scenarios**:

1. **Given** a yarn v1 `yarn.lock` where a parent entry declares `optionalDependencies: {baz: "^1"}`, **When** mikebom emits SPDX 2.3, **Then** `baz` MUST emit as source-side of `OPTIONAL_DEPENDENCY_OF`.
2. **Given** a yarn Berry `yarn.lock` where a package.json's `dependenciesMeta.baz.optional = true`, **When** mikebom emits SPDX 2.3, **Then** `baz` MUST emit as source-side of `OPTIONAL_DEPENDENCY_OF`.

---

### User Story 4 — m178 peer-optional precedence regression guard (Priority: P1)

When a dep is BOTH in `peerDependencies` AND `peerDependenciesMeta.<name>.optional = true`, m178's `PROVIDED_DEPENDENCY_OF` classification MUST win over m180's `OPTIONAL_DEPENDENCY_OF`. m179 FR-009 already codified this precedence; m180 delivers the implementing test as its own user story so a future contributor can't accidentally regress the precedence rule without a failing test.

**Why this priority**: Regression guard for the m178/m180 interaction. The current codebase must respect this precedence; a fixture-based end-to-end test pins the invariant. Elevated to P1 (not P2 as one might expect for a "guard" story) because the peer/optional edge case is subtle enough that a silent regression would go unnoticed in casual review.

**Independent Test**: Scan a fixture whose `package.json` has both `peerDependencies: {"react": "^18"}` and `peerDependenciesMeta: {"react": {"optional": true}}`. Verify SPDX 2.3 emits `react PROVIDED_DEPENDENCY_OF <root>` (m178 semantic wins), NOT `react OPTIONAL_DEPENDENCY_OF <root>`. The `mikebom:peer-edge-targets` annotation on the source component must be present; the `mikebom:optional-derivation` annotation on `react` MUST NOT be present.

**Acceptance Scenarios**:

1. **Given** a `package.json` with an entry in both `peerDependencies` AND `peerDependenciesMeta.<name>.optional = true`, **When** mikebom emits SPDX 2.3 under `--spdx2-relationship-compat=full`, **Then** the edge MUST emit as `PROVIDED_DEPENDENCY_OF` (m178 semantic), NOT as `OPTIONAL_DEPENDENCY_OF`.
2. **Given** the same fixture, **When** mikebom emits CDX 1.6, **Then** the target component MUST carry the m147 `mikebom:peer-edge-targets`-related state (peer-edge-targets set on the source, no `mikebom:optional-derivation` on the target — peer classification is exclusive).

---

### User Story 5 — bun.lock optional handling (Priority: P3)

Bun's `bun.lock` (JSONC) lockfile follows the npm schema fairly closely — deps and workspaces have optional-dep semantics that surface via similar mechanisms. mikebom already has a `bun_lock.rs` reader (per the file inventory at `mikebom-cli/src/scan_fs/package_db/npm/bun_lock.rs`). m180 wires the optional signal.

**Why this priority**: Bun's user share in mikebom's scan target base is small; nice-to-have but not urgent. Delivered as its own story so it can be evaluated for defer-to-m181 if the coverage-cost/user-value ratio is low.

**Independent Test**: Scan a bun-locked fixture with an optional dep declared. Verify SPDX 2.3 emits `OPTIONAL_DEPENDENCY_OF`.

**Acceptance Scenarios**:

1. **Given** a `bun.lock` with an entry flagged as optional per bun's own schema, **When** mikebom emits SPDX 2.3, **Then** the entry MUST emit as source-side of `OPTIONAL_DEPENDENCY_OF`.

---

### Edge Cases

- **Component reachable via BOTH an optional edge AND a regular runtime edge** (diamond-shape): The regular runtime edge wins the classification (per m179's stricter-scope-wins deduplication rule). This matches npm's own installer behavior: if any path to the dep is non-optional, npm installs it unconditionally.
- **Package appears in `dependencies` AND `optionalDependencies` at the same package.json level**: This is a user-authoring error but must not crash mikebom. Follow npm's own resolver behavior — the regular `dependencies` entry wins.
- **Package appears in `devDependencies` AND `optionalDependencies`**: `Development` scope wins over `Optional` per m179 FR-015 precedence (manifest-declared lifecycle scope dominates).
- **Empty `optionalDependencies: {}`**: No classifier activity; no annotations emitted; regression-preserving.
- **`--include-dev=false` filter combined with optional deps**: Per m179's edge-case handling, `LifecycleScope::Optional` participates in the "non-runtime scope filter" — `Optional` components are filtered out along with `Dev/Build/Test` when `--include-dev=false` (the existing `is_non_runtime()` check drives this uniformly).
- **Peer-optional (`peerDependenciesMeta.<name>.optional = true`) collides with `optionalDependencies` at the same package.json level** (unusual authoring pattern): The peer-optional classification wins per US4's precedence rule; the entry is classified as peer, not optional.
- **`--spdx2-relationship-compat=basic` mode**: All typed dep-scope emissions collapse to `DEPENDS_ON` per m228; no new `OPTIONAL_DEPENDENCY_OF` edges are emitted.
- **Ecosystems other than npm/yarn/pnpm/bun**: Unaffected by m180 — this milestone only touches JavaScript-ecosystem readers.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The npm `package-lock.json` reader MUST classify components with `optional: true` (and `dev: false`) as `LifecycleScope::Optional`, replacing the current fallback to `LifecycleScope::Runtime`. The classification MUST propagate through the nested tree per npm's own semantic (a dep reachable only through an optional path inherits `optional: true` on its lockfile entry, which m180 respects).
- **FR-002**: The pnpm `pnpm-lock.yaml` reader (v9+ per m157) MUST classify components carrying the per-package `optional: true` marker as `LifecycleScope::Optional`. The signal comes from both the `importers/.:optionalDependencies:` block (direct declarations) AND the `packages.<key>:optional: true` per-package marker (transitive propagation) — mikebom respects whichever surface the pnpm resolver populated.
- **FR-003**: The yarn.lock reader (v1 + Berry) MUST classify components reached via a parent's `optionalDependencies:` sub-block as `LifecycleScope::Optional`. Berry's `dependenciesMeta.<name>.optional = true` in package.json MUST also be honored (requires cross-referencing the parsed package.json's dep-meta section against the lockfile's resolved entries).
- **FR-004**: The bun.lock reader MUST classify components flagged as optional per bun's own schema as `LifecycleScope::Optional`.
- **FR-005**: All four readers (npm, pnpm, yarn, bun) MUST insert the `mikebom:optional-derivation` annotation on the classified component with value `"npm-optional-dependencies"`. The single value covers all four JavaScript-ecosystem lockfile variants — they are all facets of the same npm-registry-based optionalDependencies concept, per the m179 research artifact's Decision 1 table (Cargo has its own value, npm/yarn/pnpm/bun share one).
- **FR-006**: When a component is classified BOTH via peer-optional (`peerDependenciesMeta.<name>.optional = true`) AND via optional-dependencies (top-level `optionalDependencies` or transitive optional flag), the m178 peer classification MUST win. The component MUST emit `PROVIDED_DEPENDENCY_OF` in SPDX 2.3 (m178 semantic), NOT `OPTIONAL_DEPENDENCY_OF` (m180 semantic). The `mikebom:peer-edge-targets` annotation on the source component MUST be present; the `mikebom:optional-derivation` annotation on the target component MUST NOT be present.
- **FR-007**: Regular `dependencies` (runtime) that ALSO appear in `optionalDependencies` at the same package.json level MUST be classified as `LifecycleScope::Runtime` (per npm's own resolver semantic — non-optional wins). This is the "diamond-shape stricter-wins" edge case.
- **FR-008**: `devDependencies` MUST continue to classify as `LifecycleScope::Development`; the m180 Optional classification MUST NOT override the m179 FR-015 precedence.
- **FR-009**: The CDX 1.6 emission MUST show `scope: "excluded"` for every component classified as `LifecycleScope::Optional` — driven automatically by the m179 `is_non_runtime()` extension; no new CDX-side logic required in m180.
- **FR-010**: The SPDX 3.0.1 emission MUST NOT include a native `lifecycleScope: optional` on any relationship (SPDX 3.0.1 has no such enum value per m179 FR-017); the `mikebom:optional-derivation` annotation on the component IS the SPDX 3 classification carrier.
- **FR-011**: Under `--spdx2-relationship-compat=basic`, all new `OPTIONAL_DEPENDENCY_OF` emissions collapse to natural-direction `DEPENDS_ON` per m228's contract. Zero new typed dep-scope edges emitted under basic mode.
- **FR-012**: For scans that do NOT exercise the new signal (npm/yarn/pnpm/bun fixtures with no optional-declared deps, plus every non-JavaScript ecosystem fixture), the emitted CDX 1.6, SPDX 2.3, and SPDX 3.0.1 documents MUST be byte-identical to the pre-m180 baseline. Regression guard against unintended reader-behavior drift.
- **FR-013**: The `--include-dev=false` filter MUST filter out `LifecycleScope::Optional` components alongside `Dev/Build/Test` (via the existing `is_non_runtime()` check) — same semantic as m179 Cargo.
- **FR-014**: When a component with `LifecycleScope::Optional` reaches a transitive component with no explicit lockfile classification, mikebom MUST propagate the Optional scope through the transitive edge (matching npm/yarn/pnpm's own tree-walking semantic). The transitive component gets `LifecycleScope::Optional`.

### Key Entities

- **Component (`ResolvedComponent`)**: Reuses m179's extended `lifecycle_scope: Option<LifecycleScope>` field. `LifecycleScope::Optional` variant already exists — m180 just wires four more reader sites to populate it.
- **`mikebom:optional-derivation` annotation**: Reused from m179; new value `"npm-optional-dependencies"` (single value covering npm/yarn/pnpm/bun).
- **Lockfile schemas** (audit surface, not internal type):
  - `package-lock.json` (npm v2/v3): per-entry `optional: bool` in `packages.<path>` objects.
  - `pnpm-lock.yaml` (v9+): `importers.<key>.optionalDependencies` map + per-package `optional: true` marker.
  - `yarn.lock` (v1): `optionalDependencies:` sub-block in each parent entry.
  - `yarn.lock` (Berry v2/3): `dependenciesMeta.<name>.optional` in package.json (out-of-band signal).
  - `bun.lock`: bun's own schema for the flag.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of any JavaScript project whose lockfile carries at least one optional-declared dep MUST have the SET of PURLs marked `scope: "excluded"` in CDX 1.6 equal to the SET of PURLs that appear as source-side of any typed dep-scope relationship (`TEST_DEPENDENCY_OF` / `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `OPTIONAL_DEPENDENCY_OF`) in SPDX 2.3. Same pico filter-parity gate as m179 SC-001, extended to the JavaScript ecosystem.
- **SC-002**: Zero mikebom regression fixture SPDX 2.3 golden files experience NET-DECREMENT in `*_DEPENDENCY_OF` edge counts pre-vs-post m180. Net-INCREMENT is expected wherever a fixture exercises the new classification.
- **SC-003**: Zero drift in any mikebom CDX 1.6 golden file that does not exercise the new signal (regression guard — same shape as m179 SC-004).
- **SC-004**: Zero drift in any mikebom SPDX 3.0.1 golden file (m180 does not touch SPDX 3 emission per FR-010).
- **SC-005**: Under `--spdx2-relationship-compat=basic`, the SPDX 2.3 golden files show zero new `OPTIONAL_DEPENDENCY_OF` edges — every optional-classified edge is natural-direction `DEPENDS_ON` under basic mode (m228 escape hatch preserved).
- **SC-006**: The `mikebom:optional-derivation` annotation with value `"npm-optional-dependencies"` MUST appear byte-identically in CDX 1.6, SPDX 2.3, and SPDX 3.0.1 output for every fixture that exercises the m180 classification. Parity extractor (C122, registered in m179) validates this automatically via `SymmetricEqual` polarity.
- **SC-007**: For fixtures exercising the m178 peer-optional collision case (US4), the emitted SPDX 2.3 output MUST NOT contain the peer-optional target as source-side of `OPTIONAL_DEPENDENCY_OF` — every such edge MUST emit as `PROVIDED_DEPENDENCY_OF` (m178 semantic preserved).
- **SC-008**: Existing m178 npm regression fixtures MUST continue to emit their existing `PROVIDED_DEPENDENCY_OF` edges byte-identically (no regression on the peer-edge count from m178).

## Assumptions

- The reader may safely assume that npm's own lockfile-population logic is the authoritative source for the `optional: true` propagation through the nested tree. m180 does NOT re-derive optional-ness from a live `npm install` or from package.json's top-level `optionalDependencies` field — the lockfile IS the authority (same design principle as every other lockfile-driven classifier in mikebom).
- The reader may safely assume that pnpm's `pnpm-lock.yaml` v9 shape (per m157) is the current mikebom baseline. Older shapes (v6, v5.4) are already handled by the pnpm reader's existing polymorphic path; m180 extends the classification step, not the parsing step.
- The reader may safely assume that yarn Berry's `dependenciesMeta.<name>.optional` field appears in `package.json`, not in `yarn.lock`. The reader accesses it via mikebom's existing package.json parser (rather than adding a new file-scan pass).
- The `mikebom:optional-derivation` annotation reuses m179's schema. Value `"npm-optional-dependencies"` is intentionally shared across all four JavaScript lockfile variants (npm/yarn/pnpm/bun) because they express the same underlying concept (an entry in optionalDependencies). Follow-up milestones MAY add finer-grained values if a consumer needs to distinguish "was this from npm-lock vs pnpm-lock" — for now, coarse is correct.
- m178's peer-optional precedence rule is already codified in the m179 spec's FR-009. m180 IMPLEMENTS the precedence via a reader-time guard: before setting `LifecycleScope::Optional`, the reader checks whether the component is ALSO in the `peerDependencies` set; if so, the peer classification wins and the Optional signal is NOT set.
- m180 does NOT introduce new external dependencies (no new Cargo crates, no new subprocesses, no new fixture-fetching mechanisms). The existing npm/yarn/pnpm/bun readers already parse the relevant lockfile shapes — m180 only extends the classification step.
- The `include_dev` gating already applied in the existing readers correctly extends to `LifecycleScope::Optional` via the m179-extended `is_non_runtime()` helper (no new gating logic needed).
- Golden fixture regeneration (`MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`) will show additive-only changes on fixtures that exercise the new signal — same regeneration pattern established by m179's T028-T030.

## Constitution Alignment

**Principle V (v1.4.0)**: Direct continuation of m179's KEEP-BOTH polarity. Native SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` is the primary signal (elevated from generic `DEPENDS_ON`); `mikebom:optional-derivation = "npm-optional-dependencies"` is the finer-grained supplement (identifying the JavaScript-ecosystem lockfile origin). No new Principle V audit surface — m180 rides on the C122 catalog row m179 registered.

**Principle IX (Accuracy)**: The pico filter-parity gap m179 closed for Go now closes for npm/yarn/pnpm/bun — same measurable accuracy gain (SC-001).

**Principle X (Transparency)**: Consumers of the emitted SBOM can distinguish "this component was classified optional by a JavaScript lockfile" via the `mikebom:optional-derivation` value, and can drill into WHICH lockfile via the component's `source_type` / `evidence.source_file_paths` fields (existing mikebom transparency signals — no new field needed).
