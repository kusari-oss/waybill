# Quickstart: Milestone 163 (npm workspace-peer phantom empty-version edges)

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Contributor onboarding for milestone 163. Assumes a working mikebom dev environment (per top-level `CLAUDE.md`).

## 1. Prerequisites

- Rust stable toolchain (workspace-managed).
- No external tooling required (integration test synthesizes a monorepo in a tempdir).

Verify:

```bash
cargo +stable --version                       # expect: cargo 1.75+
```

## 2. Implementation overview

Milestone 163 is targeted at 2 files:

- `mikebom-cli/src/scan_fs/package_db/npm/walk.rs` — parser + resolution logic
- `mikebom-cli/src/scan_fs/package_db/npm/mod.rs` — top-level reader flow (Tier A → cross-workspace index construction → Tier C with cross-resolution context)

Plus 4 parity-catalog registrations + 1 new integration test file. Total: ~6 edited files, 1 new test file.

## 3. Step-by-step implementation

### 3a. Add new types (T003–T005)

In `walk.rs` add:

```rust
// Milestone 163: cross-workspace resolution types.

pub(crate) enum CrossResolution {
    Resolved { version: String },
    Unresolved,
}

pub(crate) type CrossWorkspaceIndex = std::collections::HashMap<String, String>;

pub(crate) struct CrossWorkspaceContext<'a> {
    pub peer_root: &'a std::path::Path,
    pub index: &'a CrossWorkspaceIndex,
}
```

### 3b. Add cross-workspace index builder (T006)

```rust
pub(crate) fn build_cross_workspace_index(
    entries: &[PackageDbEntry],
) -> CrossWorkspaceIndex {
    let mut index = CrossWorkspaceIndex::new();
    for entry in entries {
        if entry.purl.as_str().starts_with("pkg:npm/") && !entry.version.is_empty() {
            index.entry(entry.name.clone()).or_insert_with(|| entry.version.clone());
        }
    }
    index
}
```

### 3c. Add per-peer resolver (T007)

```rust
pub(crate) fn resolve_for_workspace_peer(
    peer_root: &std::path::Path,
    dep_name: &str,
    cross_workspace_index: &CrossWorkspaceIndex,
) -> CrossResolution {
    // Step 1: FR-003 closest-ancestor check.
    let nested = peer_root
        .join("node_modules")
        .join(dep_name)
        .join("package.json");
    if nested.is_file() {
        if let Ok(text) = std::fs::read_to_string(&nested) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(v) = parsed.get("version").and_then(|v| v.as_str()) {
                    if !v.is_empty() {
                        return CrossResolution::Resolved { version: v.to_string() };
                    }
                }
            }
        }
    }
    // Step 2: cross-workspace index fallback.
    match cross_workspace_index.get(dep_name) {
        Some(v) => CrossResolution::Resolved { version: v.clone() },
        None => CrossResolution::Unresolved,
    }
}
```

### 3d. Reshape `parse_root_package_json()` (T008)

Extend the signature:

```rust
pub(crate) fn parse_root_package_json(
    root: &serde_json::Value,
    source_path: &str,
    include_dev: bool,
    cross_workspace_ctx: Option<&CrossWorkspaceContext<'_>>,
) -> (Vec<PackageDbEntry>, WorkspacePeerAccumulator) {
    // ... existing parse ...
    // For each declared dep:
    //   if ctx.is_none() → preserve pre-163 phantom emission (backward compat)
    //   if ctx.is_some() → call resolve_for_workspace_peer + accumulate into
    //     WorkspacePeerAccumulator { resolved_deps, unresolved_deps }
}

pub(crate) struct WorkspacePeerAccumulator {
    pub resolved_deps: Vec<String>,  // names to feed into peer's `depends`
    pub unresolved_deps: Vec<String>, // names for C115 annotation
}
```

### 3e. Wire it into `mod.rs::npm::read` (T009–T010)

After Tier A completes for every project root, build the cross-workspace index. For every workspace peer's tier-C fallback, pass `Some(CrossWorkspaceContext)`. After parsing, stamp the peer's main-module component's `depends` + `mikebom:unresolved-declared-dep` annotation.

### 3f. Register C115 in parity catalog (T011–T015)

Update 4 files:

- `mikebom-cli/src/parity/extractors/cdx.rs` — `cdx_anno!(c115_cdx, ...)`
- `mikebom-cli/src/parity/extractors/spdx2.rs` — `spdx23_anno!()`
- `mikebom-cli/src/parity/extractors/spdx3.rs` — `spdx3_anno!()`
- `mikebom-cli/src/parity/extractors/mod.rs` — `ParityExtractor` entry + import lines

Also update `docs/reference/sbom-format-mapping.md` with the C115 row.

## 4. Testing

```bash
# Full pre-PR gate
./scripts/pre-pr.sh

# Unit tests only (npm module)
cargo +stable test --bin mikebom scan_fs::package_db::npm

# Integration test
cargo +stable test --test npm_phantom_edges
```

## 5. Debugging: tracing recipes

```bash
# See cross-workspace index construction + per-peer resolution outcomes
RUST_LOG=mikebom_cli::scan_fs::package_db::npm=info \
    mikebom sbom scan --path <fixture> 2>&1 \
    | grep 'workspace-peer'
```

## 6. Common pitfalls

- **Passing `None` as cross-workspace context when we should be passing `Some(_)`** — the peer's phantom emission won't get reshaped. Guard: only pass `None` when the caller is a truly standalone package.json scan (i.e., the root doesn't have a lockfile OR workspaces field).
- **FR-003 nested-first ordering** — if you check the cross-workspace index FIRST, the nested version is masked. Check the peer's own `node_modules/` first.
- **Multi-source annotation shape** — single unresolved dep → bare string; multiple → JSON array. Matches milestone-159 C107 + milestone-162 C114 precedent.

## 7. Verify SC-002 spot-check

Post-fix, the previously-phantom edges MUST NOT appear:

```bash
# Zero-empty-version-PURL invariant
jq '[.components[].purl | select(test("^pkg:npm/[^@]+@$"))] | length' out.cdx.json
# Expected: 0

# Zero-phantom-edge invariant
jq '[.dependencies[].dependsOn[] | select(test("^pkg:npm/[^@]+@$"))] | length' out.cdx.json
# Expected: 0
```
