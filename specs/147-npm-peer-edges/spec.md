# Feature Specification: npm peerDependencies — emit as edges + annotate peer-kind

**Feature Branch**: `147-npm-peer-edges`
**Created**: 2026-06-28
**Status**: Draft
**Input**: User description: "Emit npm peerDependencies as DEPENDS_ON edges in dep graph (close orphan gap surfaced by Trivy comparison on looker-frontend lockfile). Mark each peer-driven edge with a mikebom:edge-kind annotation (or SPDX-native relationship type when available) so consumers preserving the install-vs-functional distinction can filter."

## Origin

External Trivy / Syft / mikebom comparison on an npm lockfile (`looker-frontend` package.json + package-lock.json scan, 2026-06-28) surfaced a real SBOM-quality gap. With a single representative lockfile:

| Tool | Components | Orphans (zero inbound edges, excluding root) |
|---|---:|---:|
| Syft 1.44 | 1000 | 151 |
| Trivy 0.71.2 | 615 | **0** |
| Mikebom (current main) | 671 | 5 |

The 5 mikebom orphans (`shell-quote@1.8.3`, `uuid@8.3.2`, and 3 others) are all reachable from root in Trivy via `peerDependencies` edges that Trivy emits and mikebom doesn't. Concrete trace from Trivy:

```
shell-quote@1.8.3
  ← react-devtools-core@6.1.5                              (regular)
  ← react-native@0.85.3                                    (regular)
  ← @react-native-async-storage/async-storage@1.24.0       ← PEER edge
  ← @solana-mobile/wallet-adapter-mobile@2.2.8
  ← @solana/wallet-adapter-react@0.15.39
  ← @clerk/clerk-js@6.11.3
  ← <synthetic node> ← looker-frontend (root)
```

The peer-edge from `async-storage` to `react-native` is declared in the lockfile's `packages` map as:

```json
"node_modules/@react-native-async-storage/async-storage": {
  "version": "1.24.0",
  "peerDependencies": { "react-native": "^0.0.0-0 || >=0.60 <1.0" },
  "dependencies": { "merge-options": "^3.0.4" }
}
```

Mikebom's reader at `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs:177-181` only walks `dependencies`, `devDependencies`, and `optionalDependencies` — `peerDependencies` is deliberately omitted per the comment at lines 168-176: "*Trivy and syft also skip peer-edges.*" That comment is **half-wrong** as of 2026-06: Syft matches it, Trivy contradicts it.

**Semantic decision**: in npm v7+ (the dominant production reality since 2021), peerDependencies are auto-installed by default. The lockfile entry only exists if the peer WAS installed — there's a real `resolved` URL, real `integrity` hash, real version pin. The functional dependency is real: `async-storage`'s code calls into `react-native` at runtime and won't function without it. Whether `async-storage` ITSELF installed `react-native` (it didn't — some ancestor did) is npm internal-baseball; SBOM consumers care about runtime reachability for vulnerability scanning + license compliance.

This milestone brings mikebom into parity with Trivy's npm-7+ convention by emitting peerDependencies as `DEPENDS_ON` edges, while preserving the install-vs-functional distinction via a `mikebom:peer-edge-targets` per-component annotation (Constitution Principle V parity-bridging carve-out — CDX has no native edge-typing; SPDX 2.3 has no `PEER_DEPENDENCY_OF` relationship type; SPDX 3 `LifecycleScopedRelationship.scope` enum doesn't include `peer`).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - npm peerDependencies emit as DEPENDS_ON edges (Priority: P1)

A security engineer scans an npm project and emits the SBOM to feed a vulnerability scanner. The scanner walks `dependsOn` edges from the root to identify which CVE-bearing components are reachable. Today, with mikebom, `react-native@0.85.3` (and its 4-deep transitive subtree including `shell-quote`, `uuid`, etc.) shows up as an orphan in the dep graph — no inbound edges — because the only path from root goes through a peerDependency declared by `async-storage`. The scanner treats the orphan subtree as "code that may or may not be present"; in reality it IS installed (`resolved` URL + `integrity` hash in the lockfile) and WILL execute at runtime. After this milestone, mikebom emits the `async-storage → react-native` edge as a plain `DEPENDS_ON` (matching Trivy's behavior), and the full chain becomes walkable from root.

**Why this priority**: The 5 orphans on a representative production lockfile are 5 real reachability bugs in security tooling fed by mikebom's SBOMs. The fix is a one-line section-list extension in the reader; the wire-format impact is purely additive (more edges, never fewer). This is the dominant correctness signal.

**Independent Test**: Scan any lockfile containing a peerDependencies entry where the peer is also present in the lockfile's `packages` map (the auto-installed case). Assert that the consumer's `dependsOn` set in the emitted CDX `dependencies[]` (and SPDX 2.3 `DEPENDS_ON` relationships, and SPDX 3 relationships) includes the peer's PURL. Verify orphan count drops by the corresponding number.

**Acceptance Scenarios**:

1. **Given** a `package-lock.json` v3 entry for package X declaring `"peerDependencies": { "Y": "^1.0.0" }` AND a top-level `node_modules/Y` entry resolving to `Y@1.2.3`, **When** mikebom scans the lockfile, **Then** the emitted CDX `dependencies[]` includes an entry where `ref == pkg:npm/X@<v>` and `dependsOn` contains `pkg:npm/Y@1.2.3`.
2. **Given** the same lockfile, **When** the SPDX 2.3 output is emitted, **Then** the same relationship is captured as `relationshipType: "DEPENDS_ON"` with `spdxElementId == X's SPDXID` and `relatedSpdxElement == Y's SPDXID`.
3. **Given** the same lockfile, **When** the SPDX 3 output is emitted, **Then** the relationship element targeting `Y` from `X` is present in `@graph` with `relationshipType: "dependsOn"`.
4. **Given** a lockfile where the peer is declared but NOT present in the lockfile's `packages` map (an unmet peer), **When** mikebom scans, **Then** NO edge is emitted (no phantom edges — the peer must actually be installed).
5. **Given** the `looker-frontend` audit lockfile (the original Trivy-comparison corpus), **When** mikebom scans it post-milestone, **Then** orphan count drops from 5 to 0 (matching Trivy's behavior on the same input).

---

### User Story 2 - peer-driven edges are annotated so consumers can filter (Priority: P2)

A compliance tooling author wants to write a downstream check that ignores peer-driven edges (e.g., for SPDX-license-attribution purposes where only "this package installed this package" matters, not "this package functionally requires this package via npm peer protocol"). Today, mikebom doesn't emit peer edges at all — so the consumer has no signal. After US1 lands, mikebom emits peer edges as plain `DEPENDS_ON`, but they're indistinguishable from regular dependencies. After US2 lands, each component that owns one or more peer-driven edges carries a `mikebom:peer-edge-targets` annotation listing the PURLs of its peer-edge targets, letting the consumer split the dep graph by edge kind.

**Why this priority**: Preserves the install-vs-functional semantic distinction that the pre-147 reader DID preserve (by omitting peer edges entirely). US1 trades this signal for completeness; US2 buys back the signal in a different shape. Lower priority than US1 because no production consumer is known to currently use the distinction — but cheap to add and prevents a future regression on consumers who later want it.

**Independent Test**: Scan a lockfile with a known peer-edge (e.g., `async-storage → react-native`), inspect the source component's `extra_annotations` (CDX `properties[]`, SPDX 2.3 + SPDX 3 mikebom-annotation envelope), assert `mikebom:peer-edge-targets` annotation is present with the peer's PURL as an array element.

**Acceptance Scenarios**:

1. **Given** a component X that emits exactly one peer-driven edge to Y (and zero regular edges), **When** the emitted SBOM is inspected, **Then** X carries a `mikebom:peer-edge-targets` annotation whose value is a JSON array `["pkg:npm/Y@<v>"]`.
2. **Given** a component X that emits one regular edge to Y AND one peer-driven edge to Z, **When** the emitted SBOM is inspected, **Then** X's `dependsOn` contains BOTH `Y` and `Z`, AND `mikebom:peer-edge-targets` lists ONLY `["pkg:npm/Z@<v>"]` (the peer-only one).
3. **Given** a component X with ONLY regular dependencies (no peer-driven edges), **When** the SBOM is inspected, **Then** X has NO `mikebom:peer-edge-targets` annotation (the annotation is OMITTED when the set would be empty — keeps wire-output minimal for the dominant non-peer case).
4. **Given** a dependency that's declared in BOTH `peerDependencies` AND `dependencies` for the same package X (a redundant but legal lockfile shape), **When** mikebom resolves the edge, **Then** the regular dependency takes precedence and NO `mikebom:peer-edge-targets` entry is added for that dep (it's classified as regular, not peer).
5. **Given** all three formats emit the same SBOM, **When** the `mikebom:peer-edge-targets` annotation is compared across CDX + SPDX 2.3 + SPDX 3, **Then** the value is byte-equivalent for every component that has the annotation (cross-format invariance per Constitution V).

---

### Edge Cases

- **Unmet peer (peer declared but not installed)**: the peer's name doesn't appear as a `node_modules/<name>` entry in the lockfile's `packages` map. The existing `resolve_dep_via_node_modules_walk` lookup returns `None`; no edge is emitted. Matches the v7+ reality where unmet peers don't auto-install (npm warns but proceeds). No `mikebom:peer-edge-targets` entry either.
- **Peer + regular dep declared for the same package** (rare; some packages list a dep in both lists for backwards compatibility with npm v6): treat as a regular dep (precedence: `dependencies` > `devDependencies` > `optionalDependencies` > `peerDependencies`). The edge emits once; `mikebom:peer-edge-targets` does NOT include this target (it's classified as a regular dep). Avoids double-emission.
- **Optional peer (`peerDependenciesMeta` has `{"optional": true}`)**: still emit the edge IF the peer is actually installed (presence in the `packages` map is the gate). Annotation logic identical to non-optional peers.
- **Peer that's also a root dependency**: the root's `dependencies` entry handles the regular edge; `async-storage`'s peer-declaration adds a SECOND edge from `async-storage` to the same peer. Both edges emit (root's regular + peer's peer-driven). `mikebom:peer-edge-targets` on `async-storage` lists the peer; on root it does NOT (root's edge is regular).
- **`package-lock.json` v1 / v2 (legacy format)**: peerDependencies in v1/v2 lockfiles are declared in the `dependencies.<pkg>.requires` map alongside regular requires, with no per-edge type distinction. The reader's v1/v2 walker MAY treat all `requires` entries as regular dependencies (current behavior); peer-kind annotation is unavailable for v1/v2 lockfiles (deferred — see Out of Scope §1). v3 lockfiles (npm 7+) carry the typed `peerDependencies` block and get the full milestone-147 treatment.
- **Cycle through peer-edges** (X declares Y as a peer; Y is implemented in a way that calls back into X — unusual but legal): both edges emit. The existing dep-graph cycle handling (mikebom doesn't reject cycles; CDX `dependencies[]` is a flat list of edges with no cycle restriction) applies unchanged.
- **Yarn / pnpm lockfiles**: out of v1.0 scope — this milestone is npm v3 lockfile only. Yarn's `peerDependencies` handling is similar in shape; pnpm's is structurally different (peers are part of the module-path key). Both deferred per Out of Scope §3.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The npm `package-lock.json` v3 reader at `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs` MUST walk `peerDependencies` alongside `dependencies`, `devDependencies`, and `optionalDependencies` when collecting `depends` for each `PackageDbEntry`. The section-list at line 177-181 MUST gain `"peerDependencies"` as a fourth entry.
- **FR-002**: The reader MUST resolve each peerDependency name via the existing `resolve_dep_via_node_modules_walk` lookup. If the peer is present in the lockfile's `packages` map (auto-installed), emit the edge. If the peer is absent (unmet peer), emit NO edge (no phantom edges).
- **FR-003**: When a package declares a name in BOTH `peerDependencies` AND any of the regular sections (`dependencies` / `devDependencies` / `optionalDependencies`), the regular declaration takes precedence — the edge emits once via the regular path; the peer-kind annotation does NOT include this target.
- **FR-004**: Each `PackageDbEntry` whose `peerDependencies` resolved to at least one EDGE (i.e., the peer is installed and produces a real `dependsOn` entry) MUST carry a `mikebom:peer-edge-targets` value in its `extra_annotations` map. The value MUST be a JSON array of PURL strings naming the peer-driven edge targets. Format example: `["pkg:npm/react-native@0.85.3"]`.
- **FR-005**: The `mikebom:peer-edge-targets` annotation MUST be OMITTED (key absent from `extra_annotations`) when the peer-driven edge set is empty (zero peer dependencies OR all peers are unmet OR all peers also appear as regular dependencies per FR-003).
- **FR-006**: The annotation MUST be observable in CDX 1.6 `properties[]`, SPDX 2.3 envelope annotations, and SPDX 3 envelope annotations — all three downstream emitters consume the same `extra_annotations` map and surface its values per the existing pattern (parity catalog row to be added; see SC-002).
- **FR-007**: The existing reader unit test at `package_lock.rs:680-711` (`peer_dependencies_are_skipped_declarative_not_install`) MUST be REPLACED with a new test asserting the milestone-147 behavior — peer-edges ARE emitted, AND `mikebom:peer-edge-targets` annotation IS present on the source component. The old test's assertion (`mlly.depends.is_empty()`) flips to its inverse. The doc-comment on the code at lines 168-176 MUST be rewritten to reflect the new policy.
- **FR-008**: All existing byte-identity SBOM golden tests under `mikebom-cli/tests/fixtures/golden/` that include npm lockfile content MUST be refreshed in the same PR. The refresh diff MUST be limited to (a) new `dependsOn` entries for peer-driven edges + (b) new `mikebom:peer-edge-targets` annotations on the source components. Reject any unrelated drift.
- **FR-009**: All changes MUST preserve Constitution Principle V (standards-native > `mikebom:*`). The `mikebom:peer-edge-targets` annotation is permitted as a parity-bridging carve-out because: CDX 1.6 `dependencies[].dependsOn[]` is a flat list of bom-refs with no per-edge metadata slot; SPDX 2.3 has typed relationship enums but NO `PEER_DEPENDENCY_OF` variant; SPDX 3 `LifecycleScopedRelationship.scope` enum is {`development`, `build`, `test`, `runtime`} with no `peer` value. No standards-native carrier exists for "this DEPENDS_ON edge is peer-driven" — the annotation is the only available channel. Documented in `docs/reference/sbom-format-mapping.md` per Principle V's parity-bridging clause.

### Key Entities

- **Peer-driven edge** — A `dependsOn` (CDX) / `DEPENDS_ON` (SPDX 2.3) / `dependsOn` (SPDX 3) relationship whose source is a `PackageDbEntry` from the npm reader, and whose existence is attributable to a `peerDependencies` declaration in the lockfile (not a `dependencies` / `devDependencies` / `optionalDependencies` declaration). Distinguishable from regular edges ONLY via the `mikebom:peer-edge-targets` annotation on the SOURCE component (the edge itself in CDX `dependencies[].dependsOn[]` is a bare bom-ref with no metadata).
- **`mikebom:peer-edge-targets` annotation** — Per-source-component annotation. Value: JSON array of PURL strings naming the targets of that component's peer-driven edges. Stored in `PackageDbEntry.extra_annotations` (existing channel; same one milestone 144's `mikebom:source-files-nested-url` uses) and surfaced uniformly across all three SBOM formats.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After this milestone, scanning the `looker-frontend` audit lockfile (the original Trivy-comparison corpus) yields ZERO orphan components — matching Trivy's behavior. Verifiable via the jq query documented in quickstart.md (re-counts orphans = components with zero inbound `dependsOn` edges excluding root).
- **SC-002**: A new parity-catalog row for `mikebom:peer-edge-targets` (likely `C97` or next available) is added to `mikebom-cli/src/parity/extractors/` with `Directionality::SymmetricEqual` directionality, asserting cross-format byte-equality of the annotation value.
- **SC-003**: A unit test in `package_lock.rs#mod tests` replaces the pre-147 `peer_dependencies_are_skipped_declarative_not_install` test. The replacement test asserts (a) `mlly.depends` CONTAINS `pkg:npm/pathe@2.0.3` (the v3-nested-resolution result for `mlly`'s peer of `pathe`), (b) `mlly.extra_annotations.get("mikebom:peer-edge-targets")` is `Some(Value::Array([...pathe PURL...]))`.
- **SC-004**: A unit test asserts the FR-003 precedence rule: a package declaring the same name in BOTH `peerDependencies` AND `dependencies` produces exactly ONE edge (via the regular dependency) and NO `mikebom:peer-edge-targets` annotation.
- **SC-005**: A unit test asserts the FR-002 unmet-peer rule: a package declaring a peer that is NOT in the lockfile's `packages` map produces NO edge and NO `mikebom:peer-edge-targets` annotation.
- **SC-006**: Byte-identity golden fixtures under `mikebom-cli/tests/fixtures/golden/` are refreshed; diffs are limited to new `dependsOn` entries + new `mikebom:peer-edge-targets` annotations. Existing license/version/PURL fields untouched.
- **SC-007**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` both pass clean (excepting the pre-existing local `sbomqs_parity` env-only failure documented in milestone-144 T001).
- **SC-008**: Operator-cadence verification (NOT CI-gated): re-running the looker-frontend lockfile scan post-milestone, `jq '[.components[] | select(.purl == "pkg:npm/react-native@0.85.3")] | length'` returns 1 AND `jq '[.dependencies[] | select(.dependsOn[]? == "pkg:npm/react-native@0.85.3")] | length'` returns ≥1 (at least one inbound edge — the peer-edge from async-storage).
- **SC-009**: Cross-tool diff comparison post-merge: mikebom's orphan count on the audit lockfile equals Trivy's (both 0). Operator-cadence; same fixture used in spec Origin.

## Assumptions

- **npm v3 lockfile is the dominant format.** npm 7+ writes v3 by default; npm 6 writes v1/v2. Production projects since ~2021 use v3 almost universally. The v1/v2 walker (which uses the older `dependencies.<pkg>.requires` map with no typed peer distinction) is out of scope for v1.0; if peer-edges need to be added there too, a follow-up milestone can extend.
- **The "lockfile-present" gate is the right semantic for emit-or-skip.** If a peer is installed (entry exists in `packages`), emit the edge; otherwise skip. This matches Trivy's behavior empirically AND matches the npm v7+ reality that the lockfile only records installed packages. Unmet peers (npm warns but doesn't install) don't appear in the lockfile → no edge.
- **CDX 1.6 has no native per-edge metadata.** `dependencies[].dependsOn[]` is `Array<bom-ref-string>` with no per-element annotation slot. The Principle-V audit confirmed: parity-bridging via component-level `mikebom:peer-edge-targets` is the only available channel. SPDX 2.3 + SPDX 3 follow the same shape for cross-format invariance.
- **The annotation is OMITTED when empty.** Components with zero peer-driven edges (the dominant case, ~99% of npm components) carry no `mikebom:peer-edge-targets` key in their annotations map — keeps wire-output minimal and goldens narrow.
- **Edge precedence is `dependencies` > `devDependencies` > `optionalDependencies` > `peerDependencies`.** A name appearing in multiple sections is resolved by the first section that lists it (regular first). This matches the existing section-walk ordering at `package_lock.rs:177-181`; we extend the list, not the algorithm.
- **Yarn, pnpm, bun lockfiles are out of v1.0 scope.** Yarn and pnpm have their own peerDependencies handling; pnpm in particular encodes peers as part of the module-path key (`/pkg/1.0.0_peer@2.0.0/`). Both are structurally different from npm's section-based model and warrant their own milestones.
- **No new Cargo dependencies needed.** The fix is purely a section-list extension + a new annotation-key emission. Uses existing `serde_json::Value::Array`, existing `extra_annotations` BTreeMap, existing PURL construction.
- **Operator-cadence verification (SC-008 + SC-009) is the right metric for "did this work in the real world".** In-tree unit tests (SC-003/SC-004/SC-005) provide CI-binding guards on the underlying code behavior. The operator re-running the looker-frontend scan + Trivy diff is the binding signal that the goal — closing the orphan gap — was achieved end-to-end.
- **Comment text update is mandatory.** The current comment at `package_lock.rs:168-176` asserts a justification ("Trivy and syft also skip peer-edges") that is no longer accurate. Updating the comment is non-optional per FR-007 — leaving the stale rationale in the source code would surprise future contributors.

## Out of Scope

- **npm v1 / v2 lockfile peer-edge emission.** The v1/v2 walker (used for npm <7 lockfiles) doesn't have a typed peerDependencies section — all requires are merged into one map. Adding peer-kind distinction there requires structural reader changes. Deferred to a follow-up if there's demand.
- **Yarn lockfile peer-edge emission.** Yarn's reader is a separate module (`yarn_lock.rs`) with its own dep-edge model. Out of scope per spec scope-bounding to npm v3.
- **pnpm lockfile peer-edge emission.** pnpm encodes peers as part of the module-path key (e.g., `/react@17.0.0_react-dom@17.0.0`); the resolution model is structurally different. Out of scope.
- **bun.lock peer-edge emission.** bun's lockfile reader is the newest (milestone 106); adding peer-edges there is a follow-up.
- **Edge-level annotation in CDX (per `dependsOn[]` element).** CDX 1.6 spec has no per-element metadata slot. Even if a future CDX version adds one, mikebom's v1.0 fix uses component-level annotation. Deferred.
- **SPDX 3 `LifecycleScopedRelationship.scope = "peer"` proposal.** SPDX 3 spec doesn't define `peer` as a scope value. If the SPDX 3 spec adds it in a future minor, mikebom can use it as the standards-native carrier and the `mikebom:peer-edge-targets` annotation can be deprecated. Spec-tracking; not in v1.0.
- **Filtering `--no-peer-edges` CLI flag.** The pre-147 behavior was effectively "no peer edges by default." Adding a flag to restore it for operators who prefer Syft-style output is technically simple but the use case is speculative — defer until a real consumer asks for it. The `mikebom:peer-edge-targets` annotation gives downstream filtering tools the signal they need without a CLI option.
- **Peer-edge support for ecosystems that don't have peers** (cargo, gem, pip, go, maven, etc.). The peer-dependency concept is specific to npm-shaped ecosystems. Other ecosystems' readers don't need any change.
- **Changes to mikebom's orphan-handling policy.** This milestone closes 5 of the 5 npm orphans on the audit corpus; other orphan sources (e.g., binary-tier components with no package-DB match) are unaffected. The orphan-fallback contract from milestone 133 stays unchanged.
- **New `mikebom:*` annotations beyond `mikebom:peer-edge-targets`.** No additional annotations introduced; the parity-bridging is for the single new edge-kind signal.
