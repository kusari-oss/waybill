# Data Model: npm / yarn / pnpm optional-dependency classification (m180)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

## 1. Types Extended (or NOT Extended)

### 1.1 `LifecycleScope` enum — NO CHANGES

m179 introduced `LifecycleScope::Optional` at `mikebom-common/src/resolution.rs:386`. m180 reuses this variant verbatim — no new variant, no `as_str()` change, no `is_non_runtime()` change. All classifier writes converge on `Some(LifecycleScope::Optional)`.

### 1.2 `RelationshipType` enum — NO CHANGES

m179 introduced `RelationshipType::OptionalDependsOn` at `mikebom-common/src/resolution.rs:494`. m180 relies on m179's classifier at `scan_fs/mod.rs:1281-1288` to rewrite `DependsOn → OptionalDependsOn` whenever the target has `LifecycleScope::Optional`. Zero new dispatch logic in m180.

### 1.3 `SpdxRelationshipType` enum — NO CHANGES

m179 introduced `SpdxRelationshipType::OptionalDependencyOf` at `mikebom-cli/src/generate/spdx/relationships.rs:47`. m180 relies on m179's classifier arm at line 279-290 to emit `OPTIONAL_DEPENDENCY_OF` (reversed direction). Zero new arm in m180.

### 1.4 `mikebom:optional-derivation` annotation — VALUE VOCABULARY EXTENDED

m179 defined the annotation with an open enum (per FR-019). m180 adds `"npm-optional-dependencies"` to the value vocabulary. The catalog entry at `parity/extractors/cdx.rs:669+` (C122) is unchanged — it doesn't enumerate values, it just names the annotation key. m179's `sbom-format-mapping.md` C122 row does list the value vocabulary but explicitly allows additions per FR-019.

## 2. Reader-Level Classification Dispatch Table

Applied at each of the four JavaScript-ecosystem readers. The exact site is documented per-reader in research.md Decision 1.

For each lockfile entry E about to be constructed:

```text
let name = E.name
let is_dev = extract_dev_flag(E)                                 # reader-specific
let is_optional = extract_optional_flag(E)                        # reader-specific
let is_peer_of_parent = parent_package_json_has_peer_dep(name)   # from parent's package.json

# Precedence (highest wins):
if is_dev:
    lifecycle_scope = Development                                 # m179 FR-015
    # annotation: (unchanged — dev-scope has its own m052 annotation)
elif is_optional and NOT is_peer_of_parent:                       # m180 US1-US3, US5
    lifecycle_scope = Optional
    extra_annotations["mikebom:optional-derivation"] = "npm-optional-dependencies"
else:
    lifecycle_scope = Runtime                                      # unchanged
    # annotation: (none)
```

**Note on peer-precedence** (US4 / FR-006): the `is_peer_of_parent` predicate short-circuits the Optional classification. This means:
- If a dep is BOTH `peerDependencies.<name>` AND `peerDependenciesMeta.<name>.optional = true` (peer-optional), the target's `lifecycle_scope` stays Runtime (not Optional).
- m178's peer-edge classifier (which runs later at SPDX emission time) picks up the edge based on the parent's `peerDependencies` map — it does NOT consult `lifecycle_scope`. So the peer semantic still fires: SPDX emits `PROVIDED_DEPENDENCY_OF` (m178).
- The Optional annotation is NOT inserted, so consumers reading `mikebom:optional-derivation` don't see this component (which is correct — it's a peer, not an optional).

## 3. Per-Reader Details

### 3.1 npm (`package_lock.rs`)

**Insertion sites**: 
- Line 308: change `if is_dev { Development } else { Runtime }` → three-way match.
- Around lines 278-289 (m147 peer-edge annotation site): add the parallel `mikebom:optional-derivation` insert when Optional-classified.

**Peer-precedence data source**: the `entry_is_peer_of_parent` predicate uses the reader's existing per-parent peer_edge_targets logic (m147 US2 built this at lines 200-220). Reuse.

### 3.2 pnpm (`pnpm_lock.rs`)

**Insertion sites**:
- New extraction: compute `let is_optional = tbl.get("optional").and_then(|v| v.as_bool()).unwrap_or(false)` mirroring line 276's `is_dev` pattern (add around line 279).
- Line 351: three-way match.
- Around the same block: insert the annotation.

**Peer-precedence data source**: pnpm's peer-dep detection currently lives in the same reader's edge-walk logic (line 33's `peerDependencies` section handling). Extend to also record per-parent peer sets for the classifier guard.

### 3.3 yarn v1 (`yarn_lock.rs`)

**Insertion sites**: reader currently emits `lifecycle_scope: None` at line 378. Design:
- Pre-pass (before the entry construction loop): walk every parent entry's `optionalDependencies:` sub-block (already parsed at line 183). Build `HashSet<String>` of optional child names.
- Line 378: three-way match — `if is_dev { Development } else if optional_names.contains(&name) { Optional } else { None }` — keeping None as the runtime fallback since yarn v1 doesn't currently classify runtime.
- Insert the annotation when Optional.

**Peer-precedence data source**: yarn v1 has no first-class peer semantics in the lockfile. Cross-reference `package.json`'s `peerDependencies` — mikebom's yarn reader already touches package.json (needs code audit to confirm; see quickstart.md Step 2).

### 3.4 yarn Berry (v2/v3)

Same reader file (`yarn_lock.rs`), polymorphic path. Extension:
- Cross-reference `package.json`'s `dependenciesMeta.<name>.optional = true` for Berry-specific optional signals.
- Same three-way classifier match.

### 3.5 bun (`bun_lock.rs`)

**Insertion sites**: reader currently emits `lifecycle_scope: None` at lines 175 + 259. Schema audit in implementation phase; design mirrors yarn v1's structure once the audit surfaces bun's optional-flag location.

## 4. Test Contract

**Unit tests per reader**:
- `npm_optional_true_populates_lifecycle_scope_optional` (mirrors m179's Cargo `cargo_optional_true_populates_optional_deps` in shape).
- `pnpm_optional_true_populates_lifecycle_scope_optional`.
- `yarn_v1_optional_dependencies_subblock_populates_lifecycle_scope_optional`.
- `yarn_berry_dependencies_meta_optional_populates_lifecycle_scope_optional`.
- `bun_lock_optional_populates_lifecycle_scope_optional` (contingent on US5).

**Peer-precedence unit tests** (one per reader):
- `npm_peer_optional_dep_stays_peer_not_optional`.
- `pnpm_peer_optional_dep_stays_peer_not_optional`.
- `yarn_peer_optional_dep_stays_peer_not_optional`.

**Integration tests** (per user story):
- `optional_dep_npm_e2e.rs` — end-to-end fixture scan → assertions per SC-001 shape.
- `optional_dep_pnpm_e2e.rs` — same for pnpm.
- `optional_dep_yarn_e2e.rs` — same for yarn v1 + Berry.
- `optional_dep_peer_precedence.rs` — US4 flagship regression guard.

**Golden fixtures**:
- SC-003 CDX zero-drift gate: `MIKEBOM_UPDATE_CDX_GOLDENS=1` regeneration shows ADDITIVE changes only on the new npm/pnpm/yarn fixtures; existing fixtures unchanged.
- SC-004 SPDX 3 zero-drift gate: same.
- SC-002 no-decrement gate: `MIKEBOM_UPDATE_SPDX_GOLDENS=1` regeneration shows ADDITIVE `OPTIONAL_DEPENDENCY_OF` edges only.
- SC-008 m178 peer-edge preservation: existing m178 npm regression tests continue to pass (no changes to the m178 emission code path).
