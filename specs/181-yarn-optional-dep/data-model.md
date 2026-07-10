# Data Model: yarn v1 + Berry optional-dep classification (m181)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

## 1. Types Reused (Zero New Types)

### 1.1 `LifecycleScope::Optional` — no changes

m179 introduced the variant at `mikebom-common/src/resolution.rs:386`. m181 reuses verbatim.

### 1.2 `RelationshipType::OptionalDependsOn` — no changes

m179's variant + `SpdxRelationshipType::OptionalDependencyOf` emitter arm both reused verbatim.

### 1.3 `mikebom:optional-derivation` annotation — no schema change

m180 defined the annotation with an open value enum (per m179 FR-019). m181 uses value `"npm-optional-dependencies"` — SAME value m180 uses for npm/pnpm. Shared per the m180 research artifact Decision 1 table.

### 1.4 `peer_optional::is_peer_optional` helper — reused

Introduced in m180's `peer_optional.rs`. Currently marked `#[allow(dead_code)]` awaiting yarn usage. m181 consumes it → the attribute is REMOVED as part of delivery.

## 2. Types Introduced (Internal, Parser-Local)

### 2.1 `parse_v1` accumulator refactor — internal

**Before** (single accumulator):
```rust
let mut dep_names: Vec<String> = Vec::new();
```

**After** (dual accumulator):
```rust
let mut regular_dep_names: Vec<String> = Vec::new();
let mut optional_dep_names: Vec<String> = Vec::new();
```

Union merged into `depends` at end-of-entry (edge emission unchanged). `optional_dep_names` also flows into the per-scan optional-name set for the classifier.

### 2.2 Per-scan `optional_names` set — internal

```rust
type OptionalNamesSet = HashSet<String>;
```

Built once during scan; passed to `build_entry` for each component being emitted. Constructed differently per yarn variant:
- **v1**: union of all parents' `optional_dep_names` MINUS union of all parents' `regular_dep_names` MINUS peer-optional guard set (FR-005 + FR-007)
- **Berry**: `dependenciesMeta.<name>.optional = true` walk on root package.json MINUS peer-optional guard set (FR-005)

## 3. Function Signature Changes

### 3.1 `read_yarn_lock`

**Before**:
```rust
pub(super) fn read_yarn_lock(rootfs: &Path, _include_dev: bool) -> Option<Vec<PackageDbEntry>>
```

**After** (internal semantic change — no signature change):
- Additionally reads `rootfs.join("package.json")` alongside `rootfs.join("yarn.lock")`
- Parses as `serde_json::Value` (falls back to `Value::Null` on any error)
- Passes the parsed value to both `parse_v1` and `parse_berry`

**Contract**: `_include_dev` remains prefixed with `_` (still unused at this layer — m180 pattern preserved). The rootfs argument stays unchanged.

### 3.2 `parse_yarn_lock`

**Before**:
```rust
pub(super) fn parse_yarn_lock(text: &str, source_path: &str) -> Vec<PackageDbEntry>
```

**After**:
```rust
pub(super) fn parse_yarn_lock(
    text: &str,
    source_path: &str,
    pkg_json: &serde_json::Value,   // NEW — root package.json
) -> Vec<PackageDbEntry>
```

Existing tests will need to pass `&Value::Null` for `pkg_json` (backward-compatible for tests that don't exercise m181's classification).

### 3.3 `parse_v1` / `parse_berry`

Both gain the same `pkg_json: &Value` argument. Each also gains an internal set-construction pass BEFORE the main entry loop.

### 3.4 `build_entry`

**Before**:
```rust
fn build_entry(
    name: &str,
    version: &str,
    source_path: &str,
    depends: Vec<String>,
) -> Option<PackageDbEntry>
```

**After** (Option A from research.md Decision 1):
```rust
fn build_entry(
    name: &str,
    version: &str,
    source_path: &str,
    depends: Vec<String>,
    optional_names: &HashSet<String>,   // NEW — m181 classifier input
) -> Option<PackageDbEntry>
```

**Behavior**:
- If `optional_names.contains(name)` → set `lifecycle_scope: Some(LifecycleScope::Optional)` + insert `mikebom:optional-derivation = "npm-optional-dependencies"` into `extra_annotations`
- Otherwise → `lifecycle_scope: None`, `extra_annotations: BTreeMap::new()` (unchanged from pre-m181 behavior)

## 4. Classifier Dispatch Table (per user story)

Given a yarn scan whose root package.json declares:
- `dependencies: {"foo": "^1"}` (v1 / Berry runtime)
- `optionalDependencies` OR `dependenciesMeta: {"bar": {"optional": true}}` (v1 / Berry optional)
- `peerDependencies: {"react": "^18"}` + `peerDependenciesMeta: {"react": {"optional": true}}` (peer-optional)

The classification dispatch (per emitted component name X):

| X | v1 optional-set | v1 regular-set | Berry optional-set | Peer-optional | Final `lifecycle_scope` |
|---|-----------------|----------------|--------------------|---------------|-------------------------|
| foo | — | ✓ | — | — | `None` (v1 preserves None; Berry Runtime not touched) |
| bar (via `dep`) | — | ✓ | ✓ | — | `None` (diamond — regular wins) |
| bar (via `opt` only) | ✓ | — | ✓ | — | `Some(Optional)` + annotation |
| react | ✓ | — | ✓ | ✓ | `None` (peer-optional wins per FR-005) |

**Note on v1's `None` fallback**: yarn v1 currently emits `lifecycle_scope: None` for all runtime deps (line 378 pre-m181). m181 does NOT change this — it only elevates the OPTIONAL classification. Runtime deps still get `None`. This matches the m106 US5 byte-identity guarantee (SC-008).

## 5. Test Contract

**Unit tests in `yarn_lock.rs`**:
- `v1_optional_dep_populates_lifecycle_scope_optional` — the flagship US1 case
- `v1_diamond_regular_wins_over_optional` — FR-007 precedence
- `v1_peer_optional_stays_peer_not_optional` — FR-005 + US3 (v1 case)
- `berry_dependencies_meta_populates_lifecycle_scope_optional` — flagship US2 case
- `berry_peer_optional_stays_peer_not_optional` — FR-005 + US3 (Berry case)
- `berry_no_dependencies_meta_stays_none` — regression guard (Berry pre-m181 behavior preserved)
- `v1_no_optional_sub_blocks_stays_none` — regression guard (v1 pre-m181 behavior preserved)
- `pkg_json_missing_stays_lifecycle_scope_none` — FR-004 fail-safe

**Integration tests** (one per user story):
- `optional_dep_yarn_v1_e2e.rs` — v1 end-to-end scan → 3-format emission assertions
- `optional_dep_yarn_berry_e2e.rs` — Berry end-to-end
- `optional_dep_yarn_peer_precedence.rs` — US3 flagship

**Golden fixtures**:
- SC-004 CDX zero-drift gate: existing goldens unchanged (m181 affects no existing fixture)
- SC-005 SPDX 3 zero-drift gate: same
- SC-003 no-decrement gate: same
- SC-008 m106/m159 preservation: same
