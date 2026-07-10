# Research: yarn v1 + Berry optional-dep classification (m181)

**Date**: 2026-07-10
**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Decision 1 — `build_entry` classification-input placement

**Chosen**: **Option A** — pass `optional_names: &HashSet<String>` (and a `name` self-reference for lookup) to `build_entry`, which conditionally sets `LifecycleScope::Optional` + inserts the `mikebom:optional-derivation` annotation.

**Rationale**: The alternative (post-process the returned `Vec<PackageDbEntry>`) adds a full sweep-and-mutate phase; Option A stays inside the existing constructor's control flow, matches m180's inline-classifier pattern (npm/pnpm both classify at the `PackageDbEntry` construction site), and preserves byte-identity of unchanged fields.

**Alternatives considered**:
- **Post-process** (`&mut` mutation over the returned vec): adds a second phase; harder to reason about which fields the mutation touches.
- **Per-variant duplicated `build_entry`**: loses the shared byte-identity guarantee that m106/m159/etc. depend on.

**Implementation shape**:
```rust
fn build_entry(
    name: &str,
    version: &str,
    source_path: &str,
    depends: Vec<String>,
    // NEW in m181:
    optional_names: &HashSet<String>,
) -> Option<PackageDbEntry> {
    let purl = build_npm_purl(name, version)?;
    let (lifecycle_scope, extra_annotations) = if optional_names.contains(name) {
        let mut ann: BTreeMap<String, serde_json::Value> = Default::default();
        ann.insert(
            "mikebom:optional-derivation".into(),
            serde_json::Value::String("npm-optional-dependencies".into()),
        );
        (Some(LifecycleScope::Optional), ann)
    } else {
        (None, BTreeMap::new())   // yarn v1 preserves pre-m181 None fallback
    };
    Some(PackageDbEntry {
        // ... existing fields ...
        lifecycle_scope,
        extra_annotations,
        // ... rest unchanged ...
    })
}
```

**Note**: v1 `lifecycle_scope: None` fallback is PRESERVED for non-Optional entries — this matches pre-m181 behavior and satisfies FR-013's regression guard.

## Decision 2 — Package.json access via `read_yarn_lock`

**Chosen**: `read_yarn_lock` reads BOTH `rootfs.join("yarn.lock")` AND `rootfs.join("package.json")`. The package.json is parsed as `serde_json::Value` (Null on any error) and threaded through to both `parse_v1` and `parse_berry` as an additional argument.

**Rationale**: Both parsers need the same source (root package.json) for the peer-precedence guard + Berry `dependenciesMeta` cross-reference. Reading once at the entry point avoids per-parser duplication.

**Alternatives considered**:
- **Global path-relative lookup at `build_entry` time**: violates Principle IV (untyped path threading); increases surface area of the classifier.
- **Delayed lazy read via a closure**: adds an `Fn()` parameter without measurable benefit for a single-file read at scan startup.

**Fail-safe contract** (FR-004): if `package.json` is missing OR unparseable, `read_yarn_lock` emits a `tracing::warn!` diagnostic and passes `Value::Null` to both parsers. Both parsers safely treat `Null` as "no optional entries + no peer-optional guards" — classification is skipped uniformly.

## Decision 3 — v1 accumulator split

**Chosen**: Convert `parse_v1`'s single `dep_names: Vec<String>` into a pair `(regular: Vec<String>, optional: Vec<String>)`. The `regular` accumulator receives entries from `dependencies:` sub-blocks; the `optional` accumulator receives entries from `optionalDependencies:` sub-blocks. The `depends` vector passed to `build_entry` is the union (edges don't care about the distinction). The v1 optional-name-set for the classifier is built as: **UNION of all parents' `optional` accumulators MINUS UNION of all parents' `regular` accumulators** (FR-007 diamond-shape rule enforced at set-union time).

**Rationale**: Preserves the existing edge-emission behavior (same `depends` vector) while capturing the distinction for the classifier. The set-difference approach cleanly enforces FR-007 without per-name special-casing.

**Alternatives considered**:
- **Track (parent, child, is_optional) triples**: precise but overkill; the FR-007 diamond rule only needs the child-name membership on both sides.
- **Only track `optional`, treat any name absent from `regular` as truly optional**: fails when a child is optional per one parent and doesn't appear as a dep of anyone else (would classify it correctly by chance but the logic is fragile).

**Change site**: `parse_v1` body-block loop at line 168-195. Introduce a `let mut is_optional_block = false;` companion to the existing `let mut in_deps_block = false;`. Set it when `trimmed == "optionalDependencies:"`; append to `optional_names_local` instead of `dep_names` in that branch. Both are merged into `depends` at end-of-entry.

## Decision 4 — Yarn Berry `dependenciesMeta` extraction

**Chosen**: One-pass `serde_json::Value` walk on root package.json:

```rust
fn berry_optional_names_from_pkg_json(pkg_json: &Value) -> HashSet<String> {
    pkg_json
        .get("dependenciesMeta")
        .and_then(|v| v.as_object())
        .into_iter()
        .flat_map(|obj| obj.iter())
        .filter(|(_, meta)| {
            meta.get("optional").and_then(|v| v.as_bool()) == Some(true)
        })
        .map(|(name, _)| name.to_string())
        .collect()
}
```

**Rationale**: Berry's `dependenciesMeta` is a flat top-level map in package.json — no nested traversal needed. Failure modes (missing field, wrong type) all collapse to the empty set via the option chain.

**Alternatives considered**:
- **Deep-scan multiple sources**: Berry ALSO supports `dependenciesMeta` at workspace-member level. Deferred to a follow-up per spec Assumption 3.

## Decision 5 — Peer-precedence guard placement

**Chosen**: Guard runs during optional-name-set construction — for each candidate name, call `is_peer_optional(name, &pkg_json)`; if true, REMOVE from the set. Same location for both v1 and Berry (right after Decisions 3+4 build the initial set).

**Rationale**: The guard is a pure filter on the set of names to classify. Applying it once upstream (rather than per-`build_entry` call) minimizes the surface and matches the m180 pattern (guard runs at reader-entry-construction, not at emission time).

**Alternatives considered**:
- **Classifier-time guard inside `build_entry`**: possible but wasteful (guard would run per component; upstream filter runs per unique name).
- **Emission-time guard in the SPDX 2.3 classifier**: rejected because it moves cross-cutting logic to the wrong layer (already dismissed in m180 US4).

**Consequence**: The `#[allow(dead_code)]` attribute on `peer_optional::is_peer_optional` is REMOVED as part of m181 delivery. The function's docstring "Reader-usage note" (that mentions yarn as the future consumer) is updated to state yarn now uses it.

## Decision 6 — Delivery cadence

**Chosen**: **Single-PR delivery** for all three US.

**Rationale**: US1 (v1) + US2 (Berry) + US3 (peer-guard) all touch the same file (`yarn_lock.rs`) with converging patterns (all three consume the same `serde_json::Value` from the new package.json plumbing; all three set `LifecycleScope::Optional` via the same `build_entry` extension). Splitting would create a temporary state where the file is half-migrated to the new signature — not worth the overhead for what should be ~20 tasks.

**Alternatives considered**:
- **Split by yarn variant** (v1 first, Berry follow-up): would leave the `is_peer_optional` marker `#[allow(dead_code)]` half-removed for a period.
- **Split by user story** (US1+US2 first, US3 guard follow-up): risky — would ship an intermediate state where peer-optional deps get incorrectly classified as Optional, then get fixed. Bad regression pattern.

**Fallback**: if implementation reveals a surprise in yarn Berry's `dependenciesMeta` parsing (e.g., yarn's own `dependenciesMeta` shape differs from what's documented), US2 can defer to m182 while US1+US3 ship first. Deferred decision — made at tasks-time if the audit surfaces one.

## Open Questions

None. All Q1/Q2 decisions from m179+m180 are pre-ratified: single `"npm-optional-dependencies"` value, peer-precedence rule, KEEP-BOTH polarity.

## Alternatives Considered (Not Adopted)

- **Per-variant derivation value** (`"yarn-v1-optional-dependencies"` vs `"yarn-berry-dependencies-meta-optional"`): rejected via m180 design ratification — the underlying concept is the same npm-registry optionalDependencies pattern; coarser value is correct until a consumer surfaces a need for finer granularity.
- **Workspace-member `dependenciesMeta` cross-reference**: deferred to follow-up milestone per spec.md Assumption 3. m181 single-workspace root-scope is sufficient for the pico filter-parity outcome.
- **`.pnp.cjs` parsing** for Plug'n'Play resolvers: yarn Berry can be configured to use Plug'n'Play instead of `node_modules`; PnP metadata lives in `.pnp.cjs` which is a JavaScript file (not JSON). Parsing this is a much bigger scope — for m181 we rely on `package.json + yarn.lock` only.
