# Feature Specification: Cross-ecosystem dep-name edge resolution

**Feature Branch**: `218-cross-ecosystem-edges`
**Created**: 2026-07-22
**Status**: Draft
**Input**: User description: "633 — bridge cross-ecosystem depends-name lookup for m216 pkg:generic/ Ruby app main-modules (closes #633)"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Gemfile-only Ruby app SBOM has real outgoing edges (Priority: P1)

An SBOM consumer (Guac ingestor, VEX generator, SBOM-merge tool, per-consumer graph-traversal tool) receives a waybill-produced SBOM for a Gemfile-only Ruby application. They start traversal at the SBOM's DESCRIBES root — the `pkg:generic/<slug>@<version>` main-module produced by milestone 216 — and expect to walk into the transitive gem dependency tree via DEPENDS_ON edges (CycloneDX `dependencies[].dependsOn`, SPDX 2.3 `DEPENDS_ON` relationships, SPDX 3 `Relationship` elements). Today those edges are silently dropped, and the root appears as a degenerate isolated node with no outgoing structure.

**Why this priority**: This is the primary graph-shape regression caused by shipping m216. Real-world consumers (see #633 impact section) currently see an SBOM where the application's identity is correct but its dependency graph is invisible from the root. Every other feature we ship on top of Ruby-app scanning presumes the edges work; this is the foundational fix.

**Independent Test**: Run a waybill scan against the transitive_parity fastlane fixture (`waybill-cli/tests/fixtures/transitive_parity/gem/`). Parse the emitted CycloneDX. Enumerate DEPENDS_ON edges whose source PURL equals the metadata.component's PURL. Assert the count exceeds zero — specifically, that the count matches the number of entries in the fixture's `Gemfile.lock` `DEPENDENCIES` block.

**Acceptance Scenarios**:

1. **Given** a Gemfile-only Ruby application with a `Gemfile.lock` DEPENDENCIES block listing gems `fastlane` and `bundler`, **When** waybill scans the project root, **Then** the emitted SBOM contains one DEPENDS_ON edge from the `pkg:generic/<slug>@<version>` main-module to each of `pkg:gem/fastlane@<version>` and `pkg:gem/bundler@<version>` (2 new outgoing edges).
2. **Given** the same fixture, **When** the resulting DEPENDS_ON edge count is compared against the pre-m216 baseline of 218 edges, **Then** the new count is at least `197 + (# DEPENDENCIES entries)`. The 21 synth-root fallback edges stay legitimately removed (real main-module now exists per m216); direct edges from the real main-module to the top-level DEPENDENCIES gems replace them.
3. **Given** an SBOM consumer traversing the graph from the DESCRIBES root, **When** they follow DEPENDS_ON edges, **Then** they can reach every top-level gem in the DEPENDENCIES block within one hop of the root.

---

### User Story 2 - Cross-ecosystem edges are annotated for consumer trust (Priority: P2)

A consumer inspecting a DEPENDS_ON edge from a `pkg:generic/` main-module to a `pkg:gem/` transitive dependency wants to know whether that edge was declared verbatim by a lockfile (`pkg:gem/` → `pkg:gem/` edges are lockfile-verbatim) or was inferred by waybill's cross-ecosystem bridge (`pkg:generic/` → `pkg:gem/` edges are inferred because the m216 builder's `depends[]` field is a list of bare gem names, not fully-qualified PURLs). Waybill emits a per-edge provenance annotation on cross-ecosystem edges so consumers can distinguish inferred from lockfile-verbatim edges when computing trust scores or reachability confidence intervals.

**Why this priority**: Constitution Principle X (Transparency) requires waybill to make inferred metadata visible. Precedent: m216 emitted `waybill:package-shape` for exactly this "how did this pkg:generic/ get here" transparency need. Downstream consumers building trust models (Guac, VEX generators keyed on edge confidence) need this signal. Not P1 because the P1 fix delivers the correctness win; the annotation is the transparency layer on top.

**Independent Test**: Given the same fastlane fixture, invoke a waybill CycloneDX scan and parse the emitted dependencies. For each DEPENDS_ON edge whose source PURL starts with `pkg:generic/` and whose target starts with `pkg:gem/`, assert the emitted SBOM contains a per-edge (or per-component-with-payload) annotation naming the bridge — e.g., `waybill:cross-ecosystem-inference` with a value identifying the source-ecosystem-to-target-ecosystem transition and the lookup path used.

**Acceptance Scenarios**:

1. **Given** the m216 pkg:generic/ main-module resolves `depends[]` gem names to `pkg:gem/` PURLs during the resolver pass, **When** the resulting DEPENDS_ON edges are emitted, **Then** each such edge carries a `waybill:cross-ecosystem-inference` provenance annotation.
2. **Given** a same-ecosystem edge (`pkg:gem/` → `pkg:gem/`), **When** the SBOM is inspected, **Then** it does NOT carry the cross-ecosystem-inference annotation (the annotation is exclusively for edges that crossed ecosystem boundaries during resolution).
3. **Given** the SBOM parity gate compares CDX, SPDX 2.3, and SPDX 3 outputs, **When** the annotation exists on any edge in one format, **Then** an equivalent annotation exists on the same edge in the other two formats (Principle V: standards-native precedence — CDX `dependencies[i].properties[]`, SPDX 2.3 relationship-level annotations, SPDX 3 `Annotation` elements on the Relationship IRI).

---

### User Story 3 - Fix generalizes beyond Ruby to future m216-alikes (Priority: P3)

Future milestones will add m216-alike readers for other ecosystems where a source-tree application has no upstream registry identity (pip apps declared via `pyproject.toml` with no `[project.name]`, npm CLI tools declared via `bin.<name>` scripts with no published package, cargo binary-only crates with no `[package]` version, Go binary modules where `go install` on an unpublished repo produces a real binary, etc.). Each of these will produce a `pkg:generic/<slug>@<version>` main-module whose `depends[]` field lists bare package names from the ecosystem-specific manifest. The fix landed for #633 must also bridge those cross-ecosystem lookups — not just `generic → gem`.

**Why this priority**: This is a design constraint on the P1 fix, not a separate deliverable. If the P1 resolver is written as `if source_ecosystem == "generic" && lookup_fails { try target_ecosystem == "gem" }`, we lock in a Ruby-only escape hatch that we'll have to repeat for pip / npm / cargo / go / dart / composer / etc. If instead it's written as `if source_ecosystem == "generic" && lookup_fails { try every other ecosystem present in this scan's resolver index }`, one implementation covers all m216-alikes forever. The P3 status reflects that we don't need a pip/npm/cargo fixture to ship the P1 fix — but the design must anticipate them.

**Independent Test**: Author (or synthesize) a minimal test fixture representing a hypothetical pip-app main-module (`pkg:generic/my-pip-app@0.0.0-unknown` with `depends: ["requests", "click"]`) alongside `pkg:pypi/requests@2.31.0` and `pkg:pypi/click@8.1.7` components. Run the resolver pass. Assert two DEPENDS_ON edges emitted from the generic main-module to the two pypi components. The fixture does not need a real pip reader — the test can hand-construct `PackageDbEntry` records and run only the resolver pass. Test proves the resolver is ecosystem-agnostic beyond the m216 gem case.

**Acceptance Scenarios**:

1. **Given** a synthetic scan with a `pkg:generic/` main-module and `pkg:pypi/` transitive components matching by name, **When** the resolver pass runs, **Then** DEPENDS_ON edges form correctly between them.
2. **Given** a synthetic scan with a `pkg:generic/` main-module and both `pkg:gem/` AND `pkg:pypi/` components sharing a name (`"json"` is a real name in both ecosystems), **When** the resolver pass runs, **Then** the resolver either emits edges to both (ambiguous match: prefer emitting-with-annotation over silently-picking-one), OR the ambiguity is resolved by a documented tie-break rule (e.g., prefer the ecosystem that appears elsewhere in the same scan's non-generic main-modules).

---

### Edge Cases

- **Same-name gems in multiple ecosystems** (e.g., `json` exists in `pkg:gem/`, `pkg:pypi/`, `pkg:npm/`): the resolver MUST NOT silently pick one and drop the others. Either emit edges to every matching ecosystem with a `waybill:cross-ecosystem-inference-ambiguous` annotation on each, OR apply a documented tie-break rule (e.g., prefer the ecosystem that ANY same-scan non-generic main-module belongs to; only if that yields no unique match, emit all with the ambiguous annotation).
- **No match in any ecosystem**: the `depends[]` gem name doesn't resolve to any registered component. Silent-drop is unacceptable (loses information). The resolver MUST emit a document-scope diagnostic annotation naming the unresolved names and the source main-module PURL. Precedent: milestone 158's `waybill:graph-completeness` orphan-reason system.
- **Recursive main-modules** (unlikely but possible: a main-module A whose `depends[]` includes another main-module B by name): the resolver must not confuse cross-ecosystem lookup with main-module-to-main-module edges. Same-ecosystem lookup takes precedence over cross-ecosystem fallback.
- **Version mismatch across ecosystems**: main-module `depends[]` doesn't carry version constraints (per m216 spec), so the resolver picks the version already present in the resolver index for the target ecosystem/name. If multiple versions of the same target-ecosystem package exist (m164 multi-version pnpm case), the resolver MUST fan out to all versions (behavior consistent with existing multi-version handling).
- **Empty `depends[]`**: m216 emits `pkg:generic/` main-modules whose `depends[]` is empty when `Gemfile.lock` has an empty DEPENDENCIES block. Resolver contributes zero edges. No annotation emitted. Byte-identity preserved for that case.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The dependency-name resolver MUST attempt cross-ecosystem lookup when a same-ecosystem lookup fails AND the source main-module's ecosystem is `generic`. The cross-ecosystem search iterates over every ecosystem present in the resolver index and returns matches by normalized name.
- **FR-002**: When the cross-ecosystem search yields exactly one match, the resolver MUST emit a DEPENDS_ON edge from the source main-module to the matched target component.
- **FR-003**: When the cross-ecosystem search yields more than one match (same name registered in multiple non-generic ecosystems), the resolver MUST apply a tie-break rule to select the preferred target ecosystem. Tie-break preference order: (1) any ecosystem that appears elsewhere in the same scan's non-generic main-modules; (2) alphabetic order of ecosystem name as a last-resort deterministic pick. If (1) yields no unique winner and (2) is used, the resolver MUST emit each match's edge and annotate every emitted edge with a `waybill:cross-ecosystem-inference-ambiguous` annotation listing the alternate matches that were also considered.
- **FR-004**: When the cross-ecosystem search yields zero matches, the resolver MUST NOT emit an edge. The unresolved dep name MUST be recorded in a document-scope diagnostic annotation `waybill:cross-ecosystem-inference-unresolved` whose value is a JSON array of `{source_purl, unresolved_name}` records. Absence of this annotation means all cross-ecosystem lookups succeeded.
- **FR-005**: Every DEPENDS_ON edge produced by the cross-ecosystem bridge MUST carry a per-edge (or per-component-with-array-payload) `waybill:cross-ecosystem-inference` annotation identifying the ecosystem transition and the resolver path (source→target ecosystem string + lookup mechanism identifier).
- **FR-006**: Same-ecosystem edges (source and target belong to the same ecosystem) MUST NOT carry the `waybill:cross-ecosystem-inference` annotation. The annotation is exclusively for edges that crossed ecosystem boundaries during resolution.
- **FR-007**: The three-format parity contract (Principle V) MUST hold: for every edge that carries `waybill:cross-ecosystem-inference` in CycloneDX, the equivalent SPDX 2.3 relationship carries an equivalent annotation, and so does the SPDX 3 Relationship. Delivered via a new parity-catalog row.
- **FR-008**: Bytes-identity for non-generic-main-module scans MUST be preserved. Scans whose main-modules are all in the ecosystem's native namespace (`pkg:gem/`, `pkg:pypi/`, `pkg:cargo/`, `pkg:npm/`, `pkg:golang/`, etc.) MUST produce byte-identical SBOM output before and after this milestone. The new annotations only fire when a `pkg:generic/` main-module is involved.
- **FR-009**: The fix MUST be ecosystem-agnostic. It MUST NOT hard-code `"generic" → "gem"`. It MUST work for future m216-alike readers producing `pkg:generic/` main-modules for pip apps, npm CLI tools, cargo binary-only crates, etc., without further code change.
- **FR-010**: The transitive_parity_gem fixture's edge-count baseline MUST be updated to reflect the recovered edges. The updated baseline MUST be accompanied by a comment explaining the m216 → this-milestone delta (analogous to the m216 comment currently at the fixture site) so future contributors can trace the number.
- **FR-011**: The document-scope diagnostic annotation `waybill:cross-ecosystem-inference-unresolved` MUST be omitted entirely (not emitted as an empty array) when zero cross-ecosystem lookups failed. Absence is the semantic for "everything resolved cleanly" — matches the m176 workspaces-detected + m217 go-toolchain-detected silence-on-absence precedent.
- **FR-012**: The resolver's cross-ecosystem lookup MUST use the same dep-name normalization that same-ecosystem lookup uses (per-ecosystem `normalize_dep_name` behavior). The normalization applied is the TARGET ecosystem's rules — because the target is what the name will actually be indexed under (e.g., a pypi-target lookup normalizes to lowercase-underscore-hyphen collapsed form; a gem-target lookup applies the gem-side rules).
- **FR-013**: The resolver MUST log a single INFO-level line summarizing cross-ecosystem-bridge activity per scan: number of edges resolved via cross-ecosystem bridge, number of unresolved names, number of ambiguous multi-ecosystem matches. Format matches the m127/m161/m173 ladder-summary INFO-log precedent.

### Key Entities *(include if feature involves data)*

- **Cross-ecosystem edge**: A DEPENDS_ON relationship where the source component's PURL ecosystem (`purl.type`) differs from the target component's PURL ecosystem. Introduced by this milestone; the existing resolver only produces same-ecosystem edges. Carries provenance annotation identifying the bridge mechanism.
- **Cross-ecosystem-bridge decision record**: Per resolver-pass, a summary of every cross-ecosystem lookup that was attempted, its outcome (resolved / ambiguous / unresolved), and the source/target ecosystems involved. Aggregated into the FR-013 INFO log and the FR-004 unresolved-names document-scope annotation.
- **Ambiguity tie-break rule**: A deterministic function mapping (`ambiguous name`, `candidate ecosystems`, `scan's other main-module ecosystems`) → (`chosen ecosystem` OR `emit-all-with-annotation` sentinel). Per FR-003.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the transitive_parity fastlane fixture (`waybill-cli/tests/fixtures/transitive_parity/gem/`), the emitted CycloneDX contains at least 2 DEPENDS_ON edges from the `pkg:generic/` main-module to the DEPENDENCIES-declared gems. Measured via `jq '.dependencies[] | select(.ref | startswith("pkg:generic/")) | .dependsOn | length'` on the emitted CDX.
- **SC-002**: The total DEPENDS_ON edge count in the fastlane fixture recovers to at least `197 + (# DEPENDENCIES entries)` edges — recovering direct edges from the pkg:generic/ main-module while the 21 synth-root fallback edges stay legitimately removed. Exact target derived by re-computing the baseline at implementation time.
- **SC-003**: 100% of DEPENDS_ON edges whose source PURL is `pkg:generic/` and target PURL is any non-generic ecosystem carry the `waybill:cross-ecosystem-inference` provenance annotation.
- **SC-004**: Zero DEPENDS_ON edges whose source and target belong to the same ecosystem carry the `waybill:cross-ecosystem-inference` annotation. Confirmed by a bidirectional parity-extractor invariant.
- **SC-005**: The three-format parity gate (`every_catalog_row_has_an_extractor` + parity-catalog roundtrip) is green with the new C-row(s) registered across CDX / SPDX 2.3 / SPDX 3.
- **SC-006**: Byte-identity preserved for the 11-fixture `cdx_regression` / `spdx_regression` / `spdx3_regression` baselines that don't ship a `pkg:generic/` main-module. Zero goldens require regeneration for existing non-Ruby-app fixtures.
- **SC-007**: A synthetic test fixture demonstrating a hypothetical pip-app cross-ecosystem lookup (`pkg:generic/` main-module → `pkg:pypi/` components) produces the expected edges — proving FR-009 ecosystem-agnosticism without needing a real pip-app reader in this milestone.
- **SC-008**: For a scan with a `Gemfile.lock` whose DEPENDENCIES block references a gem name that isn't registered anywhere in the resolver index, the emitted SBOM contains a document-scope `waybill:cross-ecosystem-inference-unresolved` annotation naming the offending name — proving FR-004 silent-drop-is-unacceptable.

## Assumptions

- The primary reader affected is the m216 Gemfile-only Ruby main-module reader at `waybill-cli/src/scan_fs/package_db/gem.rs`. This is the only currently-shipped reader producing `pkg:generic/` main-modules with populated `depends[]` fields. Future m216-alike readers will inherit the fix automatically per FR-009.
- Fix location is the graph-dep-name resolver in `waybill-cli/src/scan_fs/mod.rs:779` (or thereabouts — line number is approximate). Fixing at the resolver is the localized, ecosystem-agnostic choice; fixing at the m216 builder would require every future m216-alike to re-implement the same bridge.
- The `PackageDbEntry.depends` field is a list of bare package names (strings), not fully-qualified PURLs. This is the m216 shape and matches every other ecosystem's dep-name-based resolution path. This milestone does NOT change `depends[]`'s shape; it only extends the resolver's lookup strategy.
- Cross-ecosystem edges carry provenance annotation per Constitution Principle X (Transparency). Precedent already established by m216 (`waybill:package-shape`), m134 (`waybill:purl-collisions-detected`), m176 (`waybill:workspaces-detected`), m217 (`waybill:go-toolchain-detected`) — same-ecosystem edges get no annotation because they're lockfile-verbatim; cross-ecosystem edges get annotated because they're waybill-inferred.
- The parity catalog gains one new row: `C137 waybill:cross-ecosystem-inference` (per-edge scope) OR up to three new rows if `waybill:cross-ecosystem-inference-ambiguous` and `waybill:cross-ecosystem-inference-unresolved` are distinct catalog entries (final split determined during plan phase). The `every_catalog_row_has_an_extractor` bidirectional test governs the split.
- The tie-break rule (FR-003) prefers ecosystems that appear elsewhere in the same scan's non-generic main-modules. Rationale: a scan with a Rails app whose main-module is `pkg:gem/rails-app@1.0.0` AND a Ruby CLI helper whose main-module is `pkg:generic/helper@0.0.0-unknown` should route the CLI's ambiguous `json` dep to `pkg:gem/` (matching the Rails app's ecosystem) rather than to `pkg:pypi/` or `pkg:npm/`. This mirrors real polyglot-repo patterns.
- The fastlane fixture (`waybill-cli/tests/fixtures/transitive_parity/gem/`) is the primary integration-test surface. The fixture's edge-count comment installed by m216 (218 → 197 direction) will be updated to reflect the m216→m218 delta (197 → 197 + DEPENDENCIES-count new edges).
- Non-goals: (1) creating a same-name-across-ecosystems disambiguator for anything OTHER than the FR-003 tie-break rule (no LLM-based semantic matching, no registry lookups, no version-constraint negotiation); (2) fixing the resolver's `(ecosystem, name)` keying for non-generic-source lookups (they work today — only generic-source lookups are broken); (3) inventing a new `PackageDbEntry` field to carry pre-resolved PURLs at m216-build time (rejected: m216-alikes shouldn't have to know about waybill's resolver internals).
- Not blocking m216: this milestone is a graph-completeness improvement on top of m216, not a correctness regression for m216 users (SBOM identity is correct; only outgoing edges are missing). Precedent set in the issue's own "Not blocking m216" clause.
