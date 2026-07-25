# Contract: workspace-member preservation under `--project-discovery=root-only`

**Feature**: 220-project-discovery-scope | **Related**: FR-004, FR-005, FR-006, US2

## The invariant

Under `--project-discovery=root-only`, every component whose `waybill:workspace-member` annotation names a scan-root-relative directory that is in scope MUST be retained in the emitted SBOM. Independent nested projects (living under their own subdirectory, not in scope) are dropped.

Under `--project-discovery=strict`, the same directory-scoped pass runs but SKIPS main-modules: the root project's own dependencies ride along, its workspace members do not.

## The signal: `waybill:workspace-member`

This is an EXISTING annotation set by m176's `scan_fs::tag_components_with_workspace_member` on every component whose evidence points at a manifest inside the scan root. m220 consumes it verbatim — no new detection logic.

### Value shape (empirically verified, m220)

The value is a **JSON-encoded array of scan-root-relative workspace DIRECTORIES** carried in a JSON string — `"[\".\"]"`, `"[\"bench\"]"`, `"[\"services/api\"]"` — each derived by `derive_workspace_root` from the parent directory of one of the component's OWN `evidence.source_file_paths`. Root-level manifests use the `"."` sentinel.

It is **self-descriptive directory provenance**, NOT a back-reference to a workspace root's PURL, and it is stamped on plain transitive dependencies too (`itoa`, `serde`, `rack`), not just on main-modules. Membership is therefore decided by **directory identity**, never by matching a root's identifier.

That shape is exactly what makes it useful here: under m064's augment-in-place, every cargo main-module in a workspace (root AND members) records the SHARED workspace `Cargo.lock` as its only evidence path, so root and members alike carry `"[\".\"]"` and are retained together under RootOnly — while an independent nested project carries its own subdirectory and is not.

### Per-ecosystem coverage today (as of m219)

| Ecosystem | Ecosystem-native workspace signal | Components carry a directory tag? | Effect under RootOnly |
|--|--|--|--|
| **Cargo** | `[workspace] members = [...]` in root Cargo.toml | ✅ (m176, shared `Cargo.lock` ⇒ `["."]`) | Root + all members retained together |
| **npm / pnpm / yarn** | `"workspaces": [...]` in root package.json | ✅ (m176, per-manifest directory) | Members under the root directory retained |
| **Go workspaces** | `use ("...")` in root `go.work` | ✅ (m176, per-`go.mod` directory) | Members under the root directory retained |
| **Maven** | `<modules>...</modules>` in root pom.xml | ✅ (m176, per-`pom.xml` directory) | Modules under the root directory retained |
| **pyproject** (poetry/hatch/setuptools) | Varies per tool | ✅ (m176, per-manifest directory) | Directory-scoped, same rule |
| **Gem** | No workspace concept | ✅ (m176, per-`Gemfile.lock` directory) | Root-directory gems retained; nested Gemfiles dropped |
| **Composer/dart/etc.** | Varies | ✅ (m176, per-manifest directory) | Directory-scoped, same rule |

**m220 does NOT extend this detection.** If a reader's evidence paths don't place a component inside an in-scope directory, it looks identical to an independent nested project and gets dropped under root-only. That's a per-ecosystem reader improvement, not an m220 concern.

## Preservation algorithm

Per `scope-filter-algorithm.md` Step 3. The pass runs under BOTH non-default modes; `skip_main_modules` is the Strict lever:

```rust
let mut in_scope_dirs: BTreeSet<String> = components.iter()
    .filter(|c| root_purls.contains(c.purl.as_str()))
    .flat_map(member_dirs)          // decode the JSON directory array
    .collect();
in_scope_dirs.extend(in_scope_roots.iter().map(|r| dir_key(&r.source_dir)));

let skip_main_modules = !mode.follows_workspace_members();  // true for Strict
for c in &components {
    if reachable.contains(c.purl.as_str()) { continue; }
    if skip_main_modules && is_main_module(c) { continue; }
    if member_dirs(c).iter().any(|d| in_scope_dirs.contains(d)) {
        reachable.insert(c.purl.as_str().to_string());
        workspace_members_followed += 1;
    }
}
```

This is a **belt-and-suspenders** pass:
- **Belt**: BFS from in-scope roots (Step 2) captures most workspace members transitively (they appear as `depends_on` targets from the workspace root).
- **Suspenders**: Annotation-based follow-up captures members that AREN'T `depends_on` targets. This is common in Cargo: `[workspace] members` declares the member but doesn't automatically create a root→member dep edge. Without this pass, orphan workspace-member components would be dropped.

## Recursion (FR-005 nested workspaces)

If a workspace member is ITSELF a workspace (Cargo permits nested workspaces via `[workspace]` in a member's Cargo.toml), its own declared members MUST also be walked.

**How this works via directory tags**:
- The outer root's own directory (`"."`) seeds `in_scope_dirs`.
- Step 2's BFS and/or the Step 3 pass pull the inner workspace root into `reachable`. The inner root's evidence lives under its own subdirectory, so it carries e.g. `["crates/inner"]`.
- That directory is NOT yet in `in_scope_dirs`, so the inner workspace's own members would be dropped.
- **Fix**: fold every reachable main-module's directories back into `in_scope_dirs` and re-run the pass. Repeat until fixpoint.

**Pseudocode addition to Step 3** (RootOnly only — Strict never follows members, so it has no fixpoint):

```rust
while follows_members {
    let mut newly_added: BTreeSet<String> = BTreeSet::new();
    for c in &components {
        if !reachable.contains(c.purl.as_str()) || !is_main_module(c) { continue; }
        for d in member_dirs(c) {
            if !in_scope_dirs.contains(&d) { newly_added.insert(d); }
        }
    }
    if newly_added.is_empty() { break; }
    in_scope_dirs.extend(newly_added);
    annotation_follow_up(&components, &in_scope_dirs, false,
                         &mut reachable, &mut workspace_members_followed);
}
```

This fixpoint terminates because each iteration only adds directories; the directory set is bounded by the component set.

## Edge cases

- **A workspace member with NO deps of its own** (e.g., a stub crate in a Cargo workspace with just `[package] name = "stub"`): captured by Step 3's annotation follow-up. Its component is in `filtered_components`; no relationships involve it (empty).
- **A component whose directory tag names no in-scope directory**: treated as an independent nested project and dropped. Consumer-friendly failure mode (root-only was asked for; anything unclear gets dropped).
- **Strict on a cargo workspace**: root and members are indistinguishable by directory (m064 gives them all the shared `Cargo.lock`, hence `["."]`). Step 1b narrows `in_scope_roots` using m201's `waybill:is-workspace-root` flag — the only reader-derived root-vs-member discriminator that exists — and Step 3's `skip_main_modules` keeps the members out while letting the root's own plain dependencies through. When no in-scope root carries that flag (single-crate projects, virtual manifests, every non-cargo ecosystem), Strict deliberately degrades to RootOnly behaviour rather than inventing a new heuristic (FR-006).
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
