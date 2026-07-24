# Contract: workspace-member preservation under `--project-discovery=root-only`

**Feature**: 220-project-discovery-scope | **Related**: FR-004, FR-005, FR-006, US2

## The invariant

Under `--project-discovery=root-only`, every component tagged `waybill:workspace-member = <root-purl>` where `<root-purl>` matches an in-scope root's PURL MUST be retained in the emitted SBOM. Independent nested projects (not tagged as workspace members) are dropped.

Under `--project-discovery=strict`, this rule does NOT apply — workspace members are dropped even if annotated.

## The signal: `waybill:workspace-member`

This is an EXISTING annotation set by per-ecosystem readers whose ecosystem supports workspaces. m220 consumes it verbatim — no new detection logic.

### Per-ecosystem detection today (as of m219)

| Ecosystem | Ecosystem-native workspace signal | Reader tags `waybill:workspace-member`? | Annotation value shape |
|--|--|--|--|
| **Cargo** | `[workspace] members = [...]` in root Cargo.toml | ✅ (via m127) | PURL of the workspace root |
| **npm / pnpm / yarn** | `"workspaces": [...]` in root package.json | ✅ (m147/m180) | PURL of the workspace root |
| **Go workspaces** | `use ("...")` in root `go.work` | ✅ (m161) | PURL derived from `go.work` root |
| **Maven** | `<modules>...</modules>` in root pom.xml | ✅ (m085) | PURL of the parent POM |
| **pyproject** (poetry/hatch/setuptools) | Varies per tool | ⚠️ Reader-dependent | Whatever the reader decides |
| **Gem** | No workspace concept | ❌ N/A | (each Gemfile/gemspec is independent) |
| **Composer/dart/etc.** | Varies | Reader-dependent | Whatever the reader decides |

**m220 does NOT extend this detection.** If a reader doesn't stamp the annotation, its "workspace members" (if any) look identical to independent nested projects and get dropped under root-only. That's a per-ecosystem reader improvement, not an m220 concern.

## Preservation algorithm

Per `scope-filter-algorithm.md` Step 3:

```rust
if mode.follows_workspace_members() {  // true for RootOnly; false for Strict
    let root_purls: BTreeSet<String> = in_scope_roots.iter()
        .map(|r| r.purl_string.clone()).collect();
    for c in &components {
        if reachable.contains(c.purl.as_str()) { continue; }
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

This is a **belt-and-suspenders** pass:
- **Belt**: BFS from in-scope roots (Step 2) captures most workspace members transitively (they appear as `depends_on` targets from the workspace root).
- **Suspenders**: Annotation-based follow-up captures members that AREN'T `depends_on` targets. This is common in Cargo: `[workspace] members` declares the member but doesn't automatically create a root→member dep edge. Without this pass, orphan workspace-member components would be dropped.

## Recursion (FR-005 nested workspaces)

If a workspace member is ITSELF a workspace (Cargo permits nested workspaces via `[workspace]` in a member's Cargo.toml), its own declared members MUST also be walked.

**How this works via annotations**:
- The outer workspace's members are tagged `waybill:workspace-member = <outer-root-purl>` by the reader.
- The inner workspace (which is a member of the outer) is itself a `[workspace]` root — the reader ALSO tags its members with `waybill:workspace-member = <inner-root-purl>`.
- Under RootOnly:
  - Outer workspace root is in-scope.
  - Its members are pulled in via the annotation-based Step 3 pass (annotation value = outer-root-PURL matches in_scope_roots).
  - The inner workspace root is itself a member → pulled in.
  - The inner workspace root becomes an "in-scope" root of its own — its members' annotations point at inner-root-PURL.
  - Currently `in_scope_roots` is fixed after Step 1 (only depth-0 roots). Inner-workspace-root is NOT in `in_scope_roots`.
  - **This is a correctness gap for FR-005 recursion.**

**Fix**: after Step 3 pulls in initial workspace members, expand `in_scope_roots` transitively: any pulled-in component that IS ITSELF a main-module (has `waybill:component-role = main-module`) joins in_scope_roots + gets its own annotation-based follow-up. Repeat until fixpoint.

**Pseudocode addition to Step 3**:

```rust
// FR-005 nested workspaces: fixpoint over annotation follow-up.
loop {
    let mut newly_added_root_purls: BTreeSet<String> = BTreeSet::new();
    for c in &components {
        // Component is IN reachable AND is a main-module → it's a NEW root.
        if reachable.contains(c.purl.as_str())
            && c.extra_annotations.get("waybill:component-role")
                .and_then(|v| v.as_str()) == Some("main-module")
        {
            let purl = c.purl.as_str().to_string();
            if !root_purls.contains(&purl) {
                newly_added_root_purls.insert(purl);
            }
        }
    }
    if newly_added_root_purls.is_empty() { break; }
    root_purls.extend(newly_added_root_purls);
    // Re-run the workspace-member pass with the expanded root_purls set.
    for c in &components {
        if reachable.contains(c.purl.as_str()) { continue; }
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

This fixpoint terminates because each iteration only adds; component set is bounded.

## Edge cases

- **A workspace member with NO deps of its own** (e.g., a stub crate in a Cargo workspace with just `[package] name = "stub"`): captured by Step 3's annotation follow-up. Its component is in `filtered_components`; no relationships involve it (empty).
- **A workspace member whose annotation value doesn't match any in-scope root** (shouldn't happen — the value is stamped by the reader from the workspace root's identity — but defensive check): the component is treated as "independent nested project" and dropped. Consumer-friendly failure mode (root-only was asked for; anything unclear gets dropped).
- **A component with BOTH main-module role AND workspace-member annotation** (an inner workspace root under an outer workspace): main-module role is preserved when the component passes into `filtered_components`. Multiple main-module roles in a single SBOM under root-only is EXPECTED — m127 root-selector at emit-time handles this via existing multi-main-module semantics.

## Test coverage matrix

| Fixture | Mode | Expected outcome |
|--|--|--|
| Cargo workspace (root [workspace], members crates/{a,b}) | RootOnly | Root main-module + a + b + all cargo deps |
| Cargo workspace + independent bench/Gemfile | RootOnly | Same as above; NO pkg:gem/* |
| Cargo workspace + independent bench/Gemfile | Strict | Root's own deps only; NO members; NO pkg:gem/* |
| Cargo workspace with nested inner workspace (member IS a workspace) | RootOnly | Outer root + inner root + all inner+outer members (fixpoint recursion) |
| npm workspaces (root package.json with "workspaces": ["packages/*"]) + independent nested Cargo.toml | RootOnly | npm root + all npm members; NO pkg:cargo/* |
| Go workspaces (go.work at root) + independent nested package.json | RootOnly | Go modules; NO pkg:npm/* |
