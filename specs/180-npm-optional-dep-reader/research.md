# Research: npm / yarn / pnpm optional-dependency classification (m180)

**Date**: 2026-07-09
**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Decision 1 — Per-Reader Classification Site Table

Each row: `lockfile shape | reader file:line where component is constructed | current handling of optional | change required in m180`.

### npm `package-lock.json` (v2/v3) — US1

| Aspect | Current state | m180 change |
|--------|---------------|-------------|
| Reader | `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs` |  |
| Optional flag detection | `is_optional` boolean already computed at line 63-66 + line 97-100 (per-entry) | Reused verbatim — no parsing change |
| Current use of optional | Filter only: line 67 + 101 drop the component when `!include_dev && (is_dev \|\| is_optional)` | Extended: when the component IS emitted (i.e., `include_dev || (!is_dev && !is_optional)`, i.e., when include_dev=true OR the entry is runtime), the classifier ALSO checks `is_optional` |
| Current classifier | Line 308: `lifecycle_scope: if is_dev { Development } else { Runtime }` | Extended: three-way dispatch — `if is_dev { Development } else if is_optional { Optional } else { Runtime }` |
| Annotation emission | m147 peer-edge-targets already emitted on same reader (lines 272-289) | Add a parallel `extra_annotations.insert("mikebom:optional-derivation", "npm-optional-dependencies")` when `is_optional && !is_dev` |
| Peer-precedence guard (FR-006) | Peer detection already gated at line 201-204 during dep-edge walk (m178) | NEW: at annotation-insertion time, check the parent's `peerDependencies.<name>` presence — if the target ALSO appears there, skip both the Optional classification and the derivation annotation |

**Estimated code change**: ~15 lines in `package_lock.rs`, entirely within the existing entry-construction block.

### pnpm `pnpm-lock.yaml` (v9+) — US2

| Aspect | Current state | m180 change |
|--------|---------------|-------------|
| Reader | `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs` |  |
| Optional flag detection | NOT computed — only `is_dev` exists at line 276 | NEW: compute `is_optional` from the entry's `optional: true` field (parallel to line 276's `is_dev` extraction) |
| Current use of optional | None at the classifier layer; `optionalDependencies:` block IS traversed at line 33 for edge collection but its target components aren't classified | Extended: after `is_optional` is computed, the classifier dispatch becomes three-way like npm |
| Current classifier | Line 351: same shape as npm's line 308 | Same extension pattern |
| Annotation emission | No optional-related annotation exists on pnpm reader today | Add at the same site as npm's insertion |
| Peer-precedence guard | Similar structure to npm; needs verification during Phase 5 (per-US tasks phase) | Same approach: check peer-dep membership before Optional classification |

**Estimated code change**: ~20 lines in `pnpm_lock.rs` (larger than npm because we add the missing `is_optional` extraction).

### yarn v1 (`yarn.lock`) — US3

| Aspect | Current state | m180 change |
|--------|---------------|-------------|
| Reader | `mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs` |  |
| Optional flag detection | Partial: `optionalDependencies:` sub-block parsing exists at line 183 but only for edge walk (not classification) | NEW: build a `HashSet<String>` of optional-child-names by walking all parents' `optionalDependencies:` sub-blocks. Then classify each target entry by name-membership. |
| Current classifier | Line 378: `lifecycle_scope: None` | Extended: after computing the optional-child-name set, classify each entry: `if in_optional_set(name) { Optional } else { None }` — keeping None-fallback for entries with no signal |
| Annotation emission | No m147/m178 peer-edge state exists on yarn reader — needs research on whether yarn v1 has peer semantics | Add `mikebom:optional-derivation` insertion when classified as Optional |
| Peer-precedence guard | yarn v1 doesn't have first-class peer semantics in the lockfile the way npm's package-lock.json does; peer detection likely needs package.json cross-reference (see Decision 3 below) | Reader-level check: build peer-name set from package.json, exclude those from optional set |

**Estimated code change**: ~40 lines in `yarn_lock.rs` (larger than npm+pnpm because we need to build the parent-child cross-reference set AND yarn currently emits None so we need lifecycle plumbing).

### yarn Berry (v2/v3) — US3 sibling

| Aspect | Current state | m180 change |
|--------|---------------|-------------|
| Reader | Same file (`yarn_lock.rs`) handles both v1 and Berry per existing polymorphic path | Same |
| Optional flag detection | Berry's `dependenciesMeta.<name>.optional = true` lives in `package.json`, not `yarn.lock`. mikebom's yarn reader accesses package.json for other purposes (needs code audit to confirm the accessor) | Cross-reference against package.json's `dependenciesMeta` map when classifying yarn Berry entries |
| Classifier + annotation | Same three-way dispatch as v1 | Same |
| Peer-precedence guard | Berry uses `peerDependenciesMeta.<name>.optional = true` in package.json (same field name as npm); m180 checks this specific combo | Same guard shape as npm's US1 case |

**Estimated code change**: ~15 lines on top of the v1 changes (mostly the package.json cross-reference).

### bun `bun.lock` — US5

| Aspect | Current state | m180 change |
|--------|---------------|-------------|
| Reader | `mikebom-cli/src/scan_fs/package_db/npm/bun_lock.rs` (497 lines) |  |
| Optional flag detection | None — `lifecycle_scope: None` at lines 175 + 259 | Schema audit REQUIRED during Phase 5 to confirm bun's on-disk shape (bun docs claim it mirrors npm but the lockfile is a distinct binary/JSONC file) |
| Classifier | Two sites emit `lifecycle_scope: None` (workspace-member vs regular package) — both need the extension | Same three-way dispatch pattern |
| Annotation emission | None | Add per the m180 contract |
| Peer-precedence guard | Bun likely inherits npm's peer semantics; needs schema audit | Same shape once audited |

**Estimated code change**: ~30-40 lines in `bun_lock.rs`, contingent on schema audit outcome. **US5 may defer to m181 if schema audit reveals unexpected complexity.**

## Decision 2 — Peer-Precedence Guard Placement

Per plan.md's summary table, **Option A (reader-time guard) wins**.

**Full contract**:

```text
For each lockfile entry E being classified:
  let name = E.name
  let is_optional = extract_optional_flag(E)   // reader-specific
  let is_dev = extract_dev_flag(E)             // reader-specific
  let peer_set = build_peer_set(E.parent_package_json)  // union of `peerDependencies` map keys

  if is_dev {
    lifecycle_scope = Development                              // m179 FR-015 precedence
  } else if is_optional && !peer_set.contains(name) {
    lifecycle_scope = Optional                                 // m180 US1-US4 flagship
    extra_annotations["mikebom:optional-derivation"] = "npm-optional-dependencies"
  } else {
    lifecycle_scope = Runtime                                  // unchanged
  }
```

**Peer-set construction**: derived from the PARENT's `package.json` — i.e., the manifest that DECLARES the dep, not the target's own package.json. This matches m178's peer-edge-targets computation shape.

**Precedence rationale**:
1. `Development` wins over everything (m179 FR-015: manifest-declared dev/build/test scopes are highest priority).
2. `Optional` wins over `Runtime` when the entry is not-dev-not-peer.
3. Peer classification (via m178's separate flow) preserves `PROVIDED_DEPENDENCY_OF` because the peer-optional target never gets `LifecycleScope::Optional` — its lifecycle stays whatever the peer flow set (typically Runtime for peer, but peer classification is edge-shape, not component-shape, so the target's `lifecycle_scope` may be Runtime or None).

**Alternative rejected**: Setting `LifecycleScope::Optional` AND relying on the m179 classifier at scan_fs/mod.rs:1281 to detect the peer-target collision. This creates cross-cutting state (classifier now needs peer-edge lookup + optional lookup + precedence logic in one pass) that Option A avoids by resolving the collision at the reader boundary where the source information already exists.

## Decision 3 — Transitive Propagation Semantics

### npm

npm's resolver stamps `optional: true` on every lockfile entry that is reached ONLY through an optional edge. mikebom reads this flag verbatim per lockfile entry → transitive propagation is FREE (npm did the work). No BFS/DFS needed on mikebom's side.

### pnpm

pnpm follows the same pattern: `packages.<key>.optional: true` is set by the pnpm resolver when the package is reachable only through optional edges. mikebom respects the flag verbatim once we compute `is_optional` (per Decision 1 pnpm row).

### yarn v1

yarn v1 does NOT propagate the flag transitively in the lockfile — each parent entry's `optionalDependencies:` sub-block only names its DIRECT optional children. Transitive-optional propagation requires mikebom to walk the graph.

**Design**: build the optional-child-name set from all parents (union of every parent's `optionalDependencies:` sub-block). Classify entries by name-membership only. This is a simplification (transitive-only-through-optional entries are NOT classified as Optional unless yarn also lists them in some other parent's optional sub-block) that trades off some fidelity for a small implementation footprint. Acceptable per FR-014's flexibility ("mikebom respects the propagation semantic of the underlying resolver" — yarn v1's own semantic doesn't propagate).

### yarn Berry

Berry propagates optional differently via `.pnp.cjs` at install time. mikebom's static-lockfile analysis approach reads yarn.lock, not `.pnp.cjs`. Design: same name-membership approach as v1, augmented by cross-referencing `package.json`'s `dependenciesMeta` map for the direct-declared optional signals.

### bun

Schema audit required in Phase 5 (implementation phase) — will document propagation semantic in bun_lock.rs comments alongside the classifier extension.

## Decision 4 — Delivery Cadence

Per plan.md's Phase 0 recommendation:

- **m180 (this milestone) ships**: US1 (npm) + US2 (pnpm) + US3 (yarn v1+Berry) + US4 (peer-precedence guard test) + optionally US5 (bun). Estimated 20-25 tasks.
- **Alternative split**: If US3 (yarn) proves expensive due to the parent-child cross-reference logic OR US5 (bun) proves expensive due to schema audit surprises, either can defer to m181 as a separate PR on this branch OR as a follow-up milestone.

**Rationale for bundling**: All 5 stories converge on the same emission-side signal (`LifecycleScope::Optional` + `mikebom:optional-derivation = "npm-optional-dependencies"`), so a single PR delivers a coherent "npm-family optional support" story. Splitting across milestones would fragment the user-visible outcome without meaningful modularity benefit.

## Open Questions

None. Q1/Q2 from m179 already answered the annotation shape + internal-model design. m180 rides on those decisions verbatim.

## Alternatives Considered (Not Adopted)

- **Alternative 1** — Distinct derivation value per lockfile variant (`npm-optional-dependencies-package-lock`, `npm-optional-dependencies-pnpm-lock`, etc.). Rejected via spec.md Assumption: the coarse single value is the right initial granularity; finer values can be added later without breaking annotation-consumer compatibility.
- **Alternative 2** — Classifier-time peer-precedence guard (Option B from plan.md). Rejected because it puts cross-cutting logic at the classifier layer where it doesn't belong.
- **Alternative 3** — Emit both `PROVIDED_DEPENDENCY_OF` AND `OPTIONAL_DEPENDENCY_OF` for peer-optional edges (dual classification). Rejected because SPDX 2.3 allows only one relationship type per edge; m179's Q1 answer already picked "peer wins" and m180 implements that decision.
