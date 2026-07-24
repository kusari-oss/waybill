# Contract: `apply_scope_filter` post-discovery pass

**Feature**: 220-project-discovery-scope | **Related**: FR-003, FR-004, FR-005, FR-007

## Surface

Pure function in `waybill-cli/src/generate/project_discovery/filter.rs`:

```rust
pub fn apply_scope_filter(
    components: Vec<ResolvedComponent>,
    relationships: Vec<Relationship>,
    mode: ProjectDiscoveryMode,
    scan_root: &Path,
) -> (Vec<ResolvedComponent>, Vec<Relationship>, ProjectDiscoveryReport)
```

Consumes the resolved-component + relationship slices AFTER `enumerate_workspace_roots` populates the discovered-main-module set. Returns filtered slices + a diagnostic report.

## Algorithm

### Step 0: fast path for default mode

```rust
if mode == ProjectDiscoveryMode::All {
    return (components, relationships, ProjectDiscoveryReport::all_default());
}
```

Zero-cost. No clone, no BFS, no annotation walk. `Vec::into_iter` chained with `Vec::collect` is elided by the compiler here (just returns the same allocation).

### Step 1: enumerate + filter main-modules

```rust
let all_roots = crate::generate::split::enumerate_workspace_roots(&components, scan_root);
let in_scope_roots: Vec<SubprojectRoot> = all_roots
    .iter()
    .filter(|r| mode.is_root_in_scope(r, scan_root))
    .cloned()
    .collect();
```

`is_root_in_scope` (data-model E2) returns `true` under `All`; returns `true` only for depth-0 manifests under `RootOnly`/`Strict`.

### Step 2: BFS-projection from each in-scope root

```rust
let mut reachable: BTreeSet<String> = BTreeSet::new();
for root in &in_scope_roots {
    let proj = crate::generate::split::project_for_root(root, &components, &relationships);
    for c in &proj.components {
        reachable.insert(c.purl.as_str().to_string());
    }
}
```

Reuses m215's `project_for_root` verbatim. Correctness inherits from that (BFS over dep-edge relationships, self-contained relationship filtering, m127-avoidance via sibling-main-module demotion).

### Step 3: workspace-member inclusion pass (RootOnly only)

```rust
let mut workspace_members_followed = 0usize;
if mode.follows_workspace_members() {  // true for All + RootOnly; false for Strict
    let root_purls: BTreeSet<String> = in_scope_roots.iter()
        .map(|r| r.purl_string.clone()).collect();
    for c in &components {
        if reachable.contains(c.purl.as_str()) { continue; }  // already included via BFS
        if let Some(v) = c.extra_annotations.get("waybill:workspace-member") {
            if let Some(root_ref) = v.as_str() {
                if root_purls.contains(root_ref) {
                    reachable.insert(c.purl.as_str().to_string());
                    workspace_members_followed += 1;
                }
            }
        }
    }
}
```

This second pass is belt-and-suspenders for FR-004: some workspace-member components may not be BFS-reachable from a root (e.g., if the root doesn't explicitly `depends_on` the member — Cargo workspaces don't automatically create dep edges from root to members). The annotation-based follow-up captures them.

Under Strict, this step is skipped entirely — workspace members are dropped even if annotated.

### Step 4: filter component + relationship slices

```rust
let filtered_components: Vec<ResolvedComponent> = components.into_iter()
    .filter(|c| reachable.contains(c.purl.as_str())).collect();
let filtered_relationships: Vec<Relationship> = relationships.into_iter()
    .filter(|r| reachable.contains(&r.from) && reachable.contains(&r.to)).collect();
```

Preserves input ordering. Drops components + relationships whose endpoints are out of scope.

### Step 5: build report

```rust
let report = ProjectDiscoveryReport {
    mode,
    root_main_modules: in_scope_roots.len(),
    workspace_members_followed,
    nested_projects_ignored: all_roots.len().saturating_sub(in_scope_roots.len()),
};
```

## Determinism guarantees

For any two invocations of `apply_scope_filter` with identical inputs (same `Vec` values + same `mode` + same `scan_root`):
- Output slices are byte-identical (order-preserving filter).
- Report is byte-identical.
- No dependency on wall-clock, PID, host, env vars.

## Correctness invariants

1. **All mode is zero-op**: no filtering, no report population. SC-005 gate.
2. **In-scope roots retained**: every root that passes `is_root_in_scope` remains in filtered_components (with its main-module role intact).
3. **Workspace members retained under RootOnly**: any component tagged `waybill:workspace-member = <in-scope-root-PURL>` is in filtered_components.
4. **Independent nested main-modules dropped under RootOnly**: any main-module not at scan-root depth AND not annotated as a workspace member of any in-scope root is dropped.
5. **Workspace members dropped under Strict**: any component tagged `waybill:workspace-member` (regardless of value) is dropped unless BFS-reachable from a root-level manifest's OWN deps.
6. **No orphan relationships**: filtered_relationships contains only edges where both endpoints are in filtered_components.

## Cost analysis

- Step 0: O(1) under All.
- Step 1: O(N) — where N = discovered main-modules (typically ≤ 50).
- Step 2: O(R × (V + E)) — R in-scope roots × per-root BFS. Typically R ≤ 10, V + E ≤ 20k → ≤ 200k ops.
- Step 3: O(C) — where C = total components. Typically ≤ 10k.
- Step 4: O(C + Erel) — one pass per slice.
- Step 5: O(1).

Total: well under 100ms on the largest realistic scans. Trivial against typical scan wall-clock (dominated by reader I/O + BFS resolution).
