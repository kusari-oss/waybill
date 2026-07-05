# Data Model: Milestone 163 (npm workspace-peer phantom empty-version edges)

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Phase-1 entity + type inventory. All entities are Rust types in `mikebom-cli/src/scan_fs/package_db/npm/` unless otherwise noted; wire-shape entities are per-format JSON constructs described in `contracts/annotations.md`.

## Rust types

### E1 — `CrossResolution` (NEW enum)

**Location**: NEW type in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs` (or a small `resolution.rs` sibling — see quickstart.md §3).

```rust
/// Milestone 163 (T003) — outcome of cross-workspace resolution for a
/// workspace-peer declared dep. Per Q1+Q2 unified disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CrossResolution {
    /// Resolved against a real lockfile entry (from the cross-workspace
    /// index) OR against a nested `node_modules/` install (FR-003
    /// closest-ancestor). The `version` string is the concrete pinned
    /// version.
    Resolved { version: String },
    /// Unresolvable — no lockfile entry AND no nested install. Per Q1+Q2
    /// unified disposition: the source workspace-peer emits the
    /// `mikebom:unresolved-declared-dep` annotation naming this dep;
    /// the edge is SUPPRESSED from `dependsOn`.
    Unresolved,
}
```

**Fields**: 2 variants. **Relationships**: produced by `resolve_for_workspace_peer()`; consumed by the reshaped `parse_root_package_json` emission branch.

**Validation rules**: closed 2-variant vocab. `Resolved.version` MUST be non-empty (empty would be a bug — the whole point is to avoid empty-version emissions).

### E2 — `CrossWorkspaceIndex` (NEW type alias)

**Location**: NEW type alias in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`.

```rust
/// Milestone 163 (T004) — scan-local map from npm package name → its
/// concrete lockfile-resolved version. Constructed once per scan after
/// Tier A (lockfile reads) completes; consulted per workspace-peer
/// during Tier C emission.
pub(crate) type CrossWorkspaceIndex = std::collections::HashMap<String, String>;
```

**Fields**: alias for `HashMap<name, version>`. **Relationships**: produced by `build_cross_workspace_index(&[PackageDbEntry])`; consumed by `resolve_for_workspace_peer()`.

**Validation rules**: keys are non-empty gem names; values are non-empty concrete versions (both invariants inherit from the lockfile parsers' post-conditions).

### E3 — `build_cross_workspace_index()` (NEW helper function)

**Location**: NEW function in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`.

```rust
/// Milestone 163 (T004) — build a name → version map from the already-
/// emitted Tier A (lockfile-derived) entries. Design-tier entries
/// (empty version) are skipped — they're precisely what we're about
/// to reshape.
///
/// Multi-version collision (same name resolved to two different versions
/// by two different lockfiles in the scan): the first encountered wins
/// (deterministic per-scan; the sort is anchored to
/// `candidate_project_roots` ordering which is filesystem-walker
/// deterministic).
pub(crate) fn build_cross_workspace_index(
    entries: &[PackageDbEntry],
) -> CrossWorkspaceIndex {
    let mut index = CrossWorkspaceIndex::new();
    for entry in entries {
        if entry.purl.as_str().starts_with("pkg:npm/")
            && !entry.version.is_empty()
        {
            index.entry(entry.name.clone()).or_insert_with(|| entry.version.clone());
        }
    }
    index
}
```

**Fields**: pure function. **Relationships**: reads `&[PackageDbEntry]`; returns `CrossWorkspaceIndex`.

### E4 — `resolve_for_workspace_peer()` (NEW helper function)

**Location**: NEW function in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`.

```rust
/// Milestone 163 (T005) — FR-003 + Q1+Q2 unified classifier for a
/// workspace peer's declared dep. Consults nested node_modules first
/// (closest-ancestor semantics matching Node.js's runtime resolver),
/// then falls through to the cross-workspace index.
pub(crate) fn resolve_for_workspace_peer(
    peer_root: &Path,
    dep_name: &str,
    cross_workspace_index: &CrossWorkspaceIndex,
) -> CrossResolution {
    // Step 1: FR-003 closest-ancestor — check the peer's own node_modules.
    let nested = peer_root
        .join("node_modules")
        .join(dep_name)
        .join("package.json");
    if nested.is_file() {
        if let Some(version) = read_installed_package_version(&nested) {
            if !version.is_empty() {
                return CrossResolution::Resolved { version };
            }
        }
    }
    // Step 2: fall through to cross-workspace index.
    match cross_workspace_index.get(dep_name) {
        Some(version) => CrossResolution::Resolved {
            version: version.clone(),
        },
        None => CrossResolution::Unresolved,
    }
}

fn read_installed_package_version(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    parsed
        .get("version")
        .and_then(|v| v.as_str())
        .map(String::from)
}
```

**Fields**: pure function. **Relationships**: reads filesystem (peer's own node_modules); consumes `CrossWorkspaceIndex`; returns `CrossResolution`.

### E5 — Reshaped `parse_root_package_json()` (EXISTING function, EDITED)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/walk.rs` — existing function extended.

The current signature:

```rust
pub(crate) fn parse_root_package_json(
    root: &serde_json::Value,
    source_path: &str,
    include_dev: bool,
) -> Vec<PackageDbEntry> { ... }
```

Milestone 163 extends it with an optional cross-workspace context:

```rust
pub(crate) fn parse_root_package_json(
    root: &serde_json::Value,
    source_path: &str,
    include_dev: bool,
    // Milestone 163 (T007): when Some(_), workspace-peer cross-resolution
    // is enabled. When None, current design-tier phantom emission is
    // preserved (used when the caller is NOT a workspace peer — e.g., a
    // truly standalone package.json scan).
    cross_workspace_ctx: Option<&CrossWorkspaceContext>,
) -> Vec<PackageDbEntry> { ... }

pub(crate) struct CrossWorkspaceContext<'a> {
    pub peer_root: &'a Path,
    pub index: &'a CrossWorkspaceIndex,
}
```

**Emission branch change** (per Q1+Q2 unified disposition):

For each declared dep in `dependencies:` + `devDependencies:` (when `include_dev`):

1. If `cross_workspace_ctx` is `None`: preserve pre-163 behavior — emit design-tier phantom with empty version + `requirement_range = <range-spec>`. (Backward compat for standalone-package.json scans.)
2. If `cross_workspace_ctx` is `Some(ctx)`:
   - Call `resolve_for_workspace_peer(ctx.peer_root, name, ctx.index)`.
   - On `Resolved { version }` → do NOT emit a design-tier phantom. The resolved entry already exists elsewhere in `entries` (from Tier A). Instead, accumulate the dep-name into the peer's `depends: Vec<String>` (see E6). The downstream graph resolver wires the edge to the real `pkg:npm/<name>@<version>` PURL.
   - On `Unresolved` → do NOT emit a design-tier phantom. Add the dep-name to a per-peer `unresolved_declared_deps: Vec<String>` accumulator (which then becomes the C115 annotation value on the peer's main-module component per E6).

### E6 — Peer main-module component emission (LOGIC CHANGE)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/mod.rs` — the main-module emission loop at line ~144 in the existing code.

Current behavior: for each workspace peer, if `parse_root_package_json` produced design-tier phantom entries, they get emitted individually. Post-163: those phantom entries are NEVER emitted (E5 rule). Instead:

- The peer's own main-module component (already emitted by milestone 066's main-module logic) gains:
  - `extra_annotations["mikebom:unresolved-declared-dep"]` = bare string (single unresolved) OR JSON array (multiple).
  - `depends: Vec<String>` populated with the RESOLVED dep-names (per E5 `Resolved` case). The downstream graph resolver in `scan_fs/mod.rs` already builds edges from these names via PURL-based dedup.

**Rationale**: The main-module component is the natural per-peer identity carrier — every peer has one (per milestone 066). No new component types needed.

## Wire types

### W1 — `mikebom:unresolved-declared-dep` (C115, per-component)

**Wire format**:
- **Single unresolved dep**: raw string of the dep name (e.g., `"@some/removed-package"`).
- **Multiple unresolved deps**: JSON array of sorted+deduplicated dep names (e.g., `["@a/pkg", "@b/pkg"]`).

**Emission conditions**: MUST appear on a component iff (a) the component is a workspace-peer main-module component AND (b) at least one declared dep in the peer's `package.json` was `Unresolved` per E4's classifier.

**Per-format shape**: see `contracts/annotations.md` §C115.

## Relationships

```text
npm::read() (Tier A completes for all project roots)
     │
     ├── build_cross_workspace_index(entries)  → HashMap<name, version>
     │
     └── for each workspace peer:
             │
             ├── parse_root_package_json(peer's package.json,
             │                            Some(CrossWorkspaceContext{peer_root, index}))
             │
             │   for each declared dep:
             │
             │       resolve_for_workspace_peer(peer_root, dep_name, index)
             │           → CrossResolution::Resolved{version} — accumulate name in peer's depends
             │           → CrossResolution::Unresolved — accumulate name in peer's unresolved-set
             │
             └── milestone-066 main-module emission for the peer:
                     │
                     ├── depends: Vec<String> = accumulated Resolved deps
                     └── extra_annotations["mikebom:unresolved-declared-dep"]
                             = single String OR JSON Array of accumulated Unresolved deps
```

## State transitions

**`CrossResolution` determination**:

```text
Input: peer_root, dep_name, cross_workspace_index

Step 1: Nested node_modules check.
    Read peer_root/node_modules/<dep_name>/package.json.
    If exists AND has non-empty `version` → Resolved{version}.

Step 2: Cross-workspace index lookup.
    If index contains dep_name → Resolved{version: index[dep_name]}.

Step 3: Fallthrough.
    → Unresolved.
```

**Idempotent**: same inputs (same peer_root filesystem + same index) produce same output.

## Data volume assumptions

- **Cross-workspace index size**: ~N where N is the total npm component count. For `test-podman-desktop`: 2835 entries. HashMap overhead: sub-megabyte.
- **Per-workspace-peer lookups**: O(deps declared in that peer's package.json). Typical: 5–20 deps × ~10 workspace peers = ~150 lookups per scan.
- **Annotation-value length**: bounded by dep-name length × count of unresolved deps per peer. Rare pathological case: peer with 100 unresolved deps → ~2 KB JSON array. Non-issue.

## Validation rules (aggregated)

| Rule | Enforcement |
|------|-------------|
| Zero empty-version PURLs post-163 (SC-004) | Enforced by construction — E5's Q1+Q2 unified rule NEVER emits a design-tier phantom when `cross_workspace_ctx` is `Some(_)`. Verified by unit test T014. |
| C115 emitted iff ≥1 unresolved declared dep | Guarded by `if !unresolved_deps.is_empty()` at annotation stamp site. Verified by unit test T017. |
| Every resolved dep produces a real edge, never a phantom | Enforced by E5 `Resolved` branch — accumulate name in `depends`, not in phantom-emission list. Verified by unit test T015. |
| FR-003 nested wins over cross-workspace index | Enforced by E4's step ordering. Verified by unit test T019. |
| Coverage advantage preserved (SC-005 ≥2835 npm components) | Enforced by construction — Tier A output is not touched. Only design-tier phantoms are reshaped. Verified by integration test T024 (asserts total npm component count). |
| Zero edges to non-existent PURLs (SC-002) | Enforced by construction — every emitted `depends` name will be dedup-matched against a real Tier A entry (which exists per the cross-workspace index hit). Unresolved names don't get emitted as edges. |
