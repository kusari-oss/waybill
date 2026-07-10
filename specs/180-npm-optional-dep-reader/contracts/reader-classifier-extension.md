# Contract: Per-Reader Classifier Extension

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## Scope

Extends the four JavaScript-ecosystem lockfile readers (`npm/package_lock.rs`, `npm/pnpm_lock.rs`, `npm/yarn_lock.rs`, `npm/bun_lock.rs`) to classify components with `LifecycleScope::Optional` (m179) when the lockfile flags them as optional, and to insert the `mikebom:optional-derivation = "npm-optional-dependencies"` annotation on the same components.

## Reader-Level Contract

For each of the four readers, the extension follows this shape:

```rust
// Inside the per-lockfile-entry construction block:
let is_dev = extract_dev_flag(&entry);
let is_optional = extract_optional_flag(&entry);
let is_peer_of_parent = check_parent_peer_deps(&entry_name, &parent_package_json);

let lifecycle_scope = if is_dev {
    Some(LifecycleScope::Development)   // m179 FR-015 precedence — unchanged
} else if is_optional && !is_peer_of_parent {
    Some(LifecycleScope::Optional)      // m180 US1-US3 flagship
} else {
    // Reader-specific runtime fallback — npm/pnpm use Runtime,
    // yarn v1 currently uses None (m180 preserves this).
    default_runtime_scope()
};

let mut extra_annotations = BTreeMap::new();
// (Existing annotations — m147 peer-edge-targets, etc. — go here unchanged.)
if matches!(lifecycle_scope, Some(LifecycleScope::Optional)) {
    extra_annotations.insert(
        "mikebom:optional-derivation".to_string(),
        serde_json::Value::String("npm-optional-dependencies".to_string()),
    );
}
```

## Per-Reader Deviations

### npm (`package_lock.rs`)

- `extract_dev_flag`: `entry.get("dev").and_then(|v| v.as_bool()).unwrap_or(false)` (already at lines 59-62 + 93-96).
- `extract_optional_flag`: `entry.get("optional").and_then(|v| v.as_bool()).unwrap_or(false)` (already at lines 63-66 + 97-100).
- `check_parent_peer_deps`: derived from `parent_package_json.get("peerDependencies")` — mikebom's npm reader already parses this at m147's peer-edge-detection site (lines 200-220). Reuse.
- Runtime fallback: `Some(LifecycleScope::Runtime)`.

### pnpm (`pnpm_lock.rs`)

- `extract_dev_flag`: reader already has this at line 276.
- `extract_optional_flag`: NEW — `tbl.get("optional").and_then(|v| v.as_bool()).unwrap_or(false)`. Add around line 279 mirroring the `is_dev` extraction.
- `check_parent_peer_deps`: needs verification — pnpm's peer handling in the reader is at lines 33 + 655+. May need small extension to expose the parent-peer-set to the classifier.
- Runtime fallback: `Some(LifecycleScope::Runtime)`.

### yarn v1 (`yarn_lock.rs`)

- `extract_dev_flag`: yarn v1 has no dev flag in the lockfile; devDependencies live in package.json only. Reader stays with `is_dev = false` for lockfile-only entries.
- `extract_optional_flag`: NEW — pre-pass to build `optional_names: HashSet<String>` from every parent's `optionalDependencies:` sub-block (already parsed at line 183 for edge walk). Classifier checks `optional_names.contains(&name)`.
- `check_parent_peer_deps`: cross-reference package.json's `peerDependencies` — yarn reader needs to plumb this in.
- Runtime fallback: `None` (preserve pre-m180 behavior; yarn v1 doesn't classify runtime today).

### yarn Berry (v2/v3)

- Same reader file, polymorphic branch.
- `extract_optional_flag`: cross-reference `package.json`'s `dependenciesMeta.<name>.optional = true` field.
- Other predicates same as v1.

### bun (`bun_lock.rs`)

- Full contract TBD after Phase 5 schema audit.
- Same three-way dispatch shape once the schema is confirmed.

## Test Contract

Each reader gets:
- 1 unit test asserting `optional = true` sets `LifecycleScope::Optional` + emits the annotation.
- 1 unit test asserting peer-optional stays peer-classified (no Optional, no annotation).
- 1 unit test asserting `dev = true` still wins over optional (regression guard for m179 FR-015).

## Verification via existing infra

- m179's `optional_dep_classification.rs` integration harness handles the cross-format SC-001 assertion.
- Golden fixture regeneration follows the m179 T028-T030 pattern.
- Parity extractor C122 (registered in m179) validates `mikebom:optional-derivation` byte-identity across CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 automatically.
