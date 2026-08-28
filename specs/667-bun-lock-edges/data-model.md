# Data Model: bun.lock transitive-edge emission

## Entities

### `BunKeyPath` (transient, in-reader)

The slash-separated string identifying a `packages`-map entry. Newly exercised by the fix; the pre-fix reader used it only as a raw key in `serde_json::Map` lookups.

**Shape**: `&str` (borrowed from the `serde_json::Map` key set) — no owned newtype introduced.

**Segmentation rules** (encoded in the R2 resolver):
- Segments are scope-atomic: `@scope/name` is ONE segment (spans two slash-delimited components); `name` is ONE segment (one component).
- A key path is a sequence of these segments joined by `/`.
- Example: `"@fast-csv/format/@types/node"` → 2 segments (`"@fast-csv/format"`, `"@types/node"`).
- Example: `"foo/bar/baz"` → 3 segments (`"foo"`, `"bar"`, `"baz"`).

### `PackagesKeysIndex` (transient, in-reader; pass 1 output)

Reader-local index mapping every `packages`-map key to the emitted component's `<name> <version>` disambiguation string.

```rust
type PackagesKeysIndex<'a> = std::collections::HashMap<&'a str, String>;
//                                                    ^^^^     ^^^^^^
//                                                    key path  "<name> <version>"
```

**Construction**: Pass 1 iterates `packages` once. For each entry, parses the first tuple element (`<name>@<source-spec>`), applies override resolution (per pre-fix behavior), and inserts `key → format!("{name} {resolved_version}")`.

**Consumption**: Pass 2's R2 resolver checks `packages_keys_index.contains_key(candidate_key)` at each walk step; on match, extracts the disambiguation string.

**Ownership**: created + consumed within `parse_bun_lock()`; no lifetime longer than one reader invocation.

### `PackageDbEntry.depends: Vec<String>` (existing)

The fix populates this on non-workspace `PackageDbEntry`s emitted from the `packages`-map walk (pre-fix at `bun_lock.rs:256` = `Vec::new()`).

**Post-fix content**: each string is either:
- `"<name> <version>"` (space-separated; the resolved-and-versioned form). This is the SUCCESS shape for every edge that resolves via R2's walker.
- Bare `"<name>"` (no space). ONLY used if the resolver returns `None` AND we choose to emit an unresolved fallback edge — **NOT USED by this fix**. Per FR-011, unresolved edges are warn-and-dropped, not emitted with bare-name fallback.

**Deduplication**: `depends_set: std::collections::BTreeMap<String, String>` per parent, dep-name-keyed with `<name> <version>` values. Matches `package_lock.rs:181` convention. Final `Vec<String>` = `depends_set.into_values().collect()` — deterministic order (BTreeMap sorts by dep-name key).

**Precedence when a dep-name appears in multiple sections**: prefer the version-pinned form over bare-name (matches `package_lock.rs:273-279`). Since bun's resolver always produces `<name> <version>` on success and unresolved edges are dropped, this precedence rarely fires in practice — but the machinery is present for consistency.

### `LifecycleScope::Optional` + `RelationshipType::OptionalDependsOn` (existing, m179)

The fix reuses these verbatim. See m179 spec + `waybill_common/src/resolution.rs:415-525`.

**How the fix uses them**: on a target entry that becomes optional (via at least one parent's `optionalDependencies` or `optionalPeers` reference), the reader mutates the target's `lifecycle_scope` field to `Some(LifecycleScope::Optional)` (mirroring `package_lock.rs`'s m180 pattern). The edge itself is represented via the target's presence in the parent's `depends` list + the target's `lifecycle_scope = Optional` — the graph builder at `scan_fs/mod.rs:897-910` then emits `RelationshipType::OptionalDependsOn` instead of `DependsOn` for that edge.

**Runtime-vs-optional precedence**: if a target appears in ONE parent's `dependencies` AND ANOTHER parent's `optionalDependencies`, the runtime edge WINS — the target's `lifecycle_scope` stays `None` (or whatever pre-existing scope), NOT `Optional`. Matches m180's semantics; verified via `package_lock.rs` behavior. Concrete rule: the reader mutates `lifecycle_scope = Some(Optional)` only if EVERY reference to this target comes via optional/optional-peers.

### `waybill:optional-derivation` annotation (existing, m180)

String annotation carried on the TARGET component's `PackageDbEntry.extra_annotations` map. New values introduced by this fix:

| Value | Meaning |
|-------|---------|
| `"bun-optional-dependencies"` | Target reached exclusively via at least one parent's `optionalDependencies` section (never via a hard `dependencies` or `peerDependencies` in any parent). |
| `"bun-optional-peers"` | Target reached exclusively via at least one parent's `optionalPeers` section AND never via any other section. If a target is reached via BOTH `optionalDependencies` AND `optionalPeers` on different parents, the `-dependencies` variant wins (matches m180's precedence). |

**Emission gate**: only when `lifecycle_scope = Some(Optional)` (per the runtime-vs-optional precedence rule above). If any hard-edge reference exists, no `waybill:optional-derivation` annotation is emitted.

**Existing pattern (for reference)**:
| Value | Reader | Docs |
|-------|--------|------|
| `"npm-optional-dependencies"` | `package_lock.rs:324` | m180 |
| `"yarn-optional-dependencies"` | `yarn_lock.rs` | m181 |
| `"pip-extras"` | `pip/mod.rs` | m183 |
| **`"bun-optional-dependencies"`** | **`bun_lock.rs` (new)** | **this feature** |
| **`"bun-optional-peers"`** | **`bun_lock.rs` (new)** | **this feature** |

## Data Flow (single scan)

```
                    ┌─────────────────────────────────┐
                    │  bun.lock (JSONC on disk)       │
                    └───────────────┬─────────────────┘
                                    │ read_bun_lock()
                                    ▼
                    ┌─────────────────────────────────┐
                    │  serde_json::Value              │
                    │  (parsed lockfile)              │
                    └───────────────┬─────────────────┘
                                    │ parse_bun_lock()
                                    ▼
              ┌──────────────────────────────────────────┐
              │  Step 1: workspace-members emit          │  (pre-fix, unchanged)
              │  (bun_lock.rs:105-197)                   │
              └──────────────────────────┬───────────────┘
                                          │
                                          ▼
              ┌──────────────────────────────────────────┐
              │  Step 2: packages-map component emit     │  (pre-fix, unchanged)
              │  (bun_lock.rs:205-279)                   │
              │  → Vec<PackageDbEntry> (components only) │
              └──────────────────────────┬───────────────┘
                                          │
                                          ▼
              ┌──────────────────────────────────────────┐
              │  NEW Step 2.5 (PASS 1):                  │  ← Feature 667 addition
              │  build PackagesKeysIndex                 │
              │  from every emitted component            │
              └──────────────────────────┬───────────────┘
                                          │
                                          ▼
              ┌──────────────────────────────────────────┐
              │  NEW Step 2.6 (PASS 2):                  │  ← Feature 667 addition
              │  for each packages-map key K:            │
              │    for each section in                   │
              │      [deps, peer, opt, opt-peers]:       │
              │      for each dep_name → range:          │
              │        resolved_key = resolve_bun_key(   │
              │            K, dep_name,                  │
              │            PackagesKeysIndex.keys())      │
              │        if resolved_key.is_some():        │
              │            parent.depends.push(          │
              │              PackagesKeysIndex[key])     │
              │            if opt or opt-peers:          │
              │              mark target for optional-tag│
              │        else: warn+drop (FR-011)          │
              │                                          │
              │  Final: apply optional-tag per           │
              │  runtime-vs-optional precedence rule.    │
              └──────────────────────────┬───────────────┘
                                          │
                                          ▼
              ┌──────────────────────────────────────────┐
              │  Vec<PackageDbEntry> (with edges)        │
              └──────────────────────────┬───────────────┘
                                          │ handed to package_db::read_all
                                          ▼
              ┌──────────────────────────────────────────┐
              │  Graph builder at scan_fs/mod.rs:897     │  (pre-fix, unchanged)
              │  resolves entry.depends → Purl edges via │
              │  name_to_purl secondary key              │
              │  ("<ecosystem>", "<name> <version>")     │
              └──────────────────────────────────────────┘
```

## Validation Rules

- **V1 — Two-pass ordering**: Pass 1 MUST complete building `PackagesKeysIndex` before Pass 2 begins. Otherwise a parent's dep-name resolution could miss a target the reader hasn't yet emitted. Enforced structurally by placing Pass 2's loop after Pass 1's loop, both after Step 2's component emission.
- **V2 — Zero component churn**: Pass 2 MUST NOT modify the pre-fix components' identity fields (`purl`, `name`, `version`, `arch`, `source_path`, `hashes`, `extra_annotations["waybill:evidence-kind"]`). Pass 2 is permitted to mutate ONLY: `entry.depends: Vec<String>`, `entry.lifecycle_scope: Option<LifecycleScope>`, `entry.extra_annotations["waybill:optional-derivation"]` (on targets). Enforced by code review; unit test `test_pass2_does_not_change_component_count_or_purls` asserts pre-Pass-2 vs post-Pass-2 component list equality on identity fields.
- **V3 — Scope-aware segmentation**: `resolve_bun_key` MUST NOT split `parent_key` on bare `/`; segmentation MUST treat `@<scope>/<name>` as one atomic segment. Enforced via SC-005 test vectors (R2 test-vector block).
- **V4 — Warn-log completeness**: every dropped edge MUST emit exactly ONE `tracing::warn!` line with the FR-011 reason string set. Enforced by unit tests that scan the log capture buffer for expected line counts.
- **V5 — Optional precedence**: a target reached via BOTH a hard section (`dependencies`/`peerDependencies`) AND an optional section (`optionalDependencies`/`optionalPeers`) MUST NOT be tagged optional. Enforced by unit test.
- **V6 — Optional-derivation string precedence**: a target reached via BOTH `optionalDependencies` AND `optionalPeers` (never via a hard section) MUST be tagged `"bun-optional-dependencies"` (the `-dependencies` variant wins). Matches m180's precedent.

## State Transitions

### Per-target lifecycle_scope state machine (feature-relevant subset)

```
                        [pre-Pass-2: lifecycle_scope = None]
                                        │
                                        ▼
     ┌─────────────────────────────────────────────────────────────────┐
     │  For each parent's edge to this target, in walk order           │
     │  (dependencies → peerDependencies → optionalDeps → optionalPeers)│
     └────────────────────┬────────────────────────────┬──────────────┘
                          │                            │
       ── section is ──┐  │                       ── section is ──┐  │
       hard (deps or   │  │                       optional (opts  │  │
       peer)           │  │                       or opt-peers)   │  │
                       ▼  ▼                                       ▼  ▼
              ┌─────────────────┐                        ┌─────────────────┐
              │ any_hard = true │                        │ any_optional_*  │
              │ (state stays or │                        │ = true          │
              │ upgrades to     │                        │ (state stays or │
              │ non-optional)   │                        │ upgrades to     │
              └────────┬────────┘                        │ Optional IFF    │
                       │                                 │ no hard edge)   │
                       │                                 └────────┬────────┘
                       └──────────┬──────────────────────┬────────┘
                                  │                      │
                                  ▼                      ▼
                    ┌────────────────────────────────────────────┐
                    │  Post-Pass-2 finalize:                     │
                    │  IF any_hard:  lifecycle_scope stays None  │
                    │  ELSE IF any_optional_*:                   │
                    │    lifecycle_scope = Some(Optional)        │
                    │    waybill:optional-derivation =           │
                    │      "bun-optional-dependencies" if the    │
                    │      optionalDependencies section was hit  │
                    │      first, else "bun-optional-peers"      │
                    └────────────────────────────────────────────┘
```
