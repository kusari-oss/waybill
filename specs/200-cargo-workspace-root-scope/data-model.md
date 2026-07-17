# Data Model: Cargo Workspace-Root [package] Runtime Classification

**Date**: 2026-07-16
**Purpose**: Document the two data structures + one classifier code path this fix touches. No new types introduced — the fix modifies an existing struct's population semantics.

## E1: `CargoTomlSections` (extended population semantics)

**Location**: `mikebom-cli/src/scan_fs/package_db/cargo.rs:689`.

**Existing shape** (unchanged by m200):

```rust
pub(crate) struct CargoTomlSections {
    pub(crate) prod_deps: HashSet<String>,      // ← ADDITIVE change: also carries [package].name
    pub(crate) dev_deps: HashSet<String>,
    pub(crate) build_deps: HashSet<String>,
    pub(crate) optional_deps: HashSet<String>,
}
```

**Change** (m200 FR-001):

Pre-fix, `prod_deps` is populated exclusively from `[dependencies]` table keys via `collect_section_keys(&parsed, "dependencies", &mut out.prod_deps)`.

Post-fix, `prod_deps` ALSO carries the root `[package].name` when the parsed manifest has one:

```rust
// After the three existing collect_section_keys() calls in parse_cargo_toml:
if let Some(root_name) = parsed
    .get("package")
    .and_then(|v| v.as_table())
    .and_then(|t| t.get("name"))
    .and_then(|v| v.as_str())
{
    out.prod_deps.insert(root_name.to_string());
}
```

**Validation rules** (inherited from existing struct):
- `HashSet<String>` — insertions dedupe naturally. Inserting a name already present via a `[dependencies]` self-reference (rare) is a no-op.
- Order-independence: BFS closure computed from `prod_deps` produces the same result regardless of insertion order (set-membership, not sequence-dependent).
- No validation on the name string itself — Cargo grammar already constrains valid names (`^[a-zA-Z][a-zA-Z0-9_-]*$`).

**Backwards compatibility**: Additive. Every pre-fix consumer of `CargoTomlSections.prod_deps` continues to see the pre-fix subset PLUS the new workspace-root entries. No consumer becomes invalid.

## E2: `prod_set` (BFS closure output — no shape change)

**Location**: `mikebom-cli/src/scan_fs/package_db/cargo.rs:924-980` (`compute_cargo_prod_set` + `cargo_bfs_closure`).

**Shape** (unchanged):

```rust
HashSet<(String, String)>   // (package_name, package_version) pairs
```

**Change** (m200 indirect effect):

The BFS is seeded from `CargoTomlSections.prod_deps`. Post-fix, workspace-root `[package].name` values enter the seed set, which means:
- The BFS walks starting from every workspace-root name in addition to every `[dependencies]` key.
- For each seed name, the BFS resolves it against Cargo.lock's `[[package]]` entries and walks the transitive closure of `dependencies = [...]` entries.
- Workspace-root `[[package]]` entries in Cargo.lock (which have `source = None`) are now visited by the BFS and added to `prod_set`.

**Downstream effect at cargo.rs:1098-1107** (the classifier cascade):

```rust
if prod_set.contains(&key) { Runtime }       // ← Workspace-root NOW hits this branch
else if build_set.contains(&key) { Build }
else { Development }                          // ← Pre-fix fallback for workspace-root
```

**Validation rules**: Set-membership check; no shape change.

## E3: Workspace-root emission on the emitted SBOM

**Location**: not a struct — an observable output.

**Pre-fix wire shape** (CDX 1.6 example, from test-vaultwarden scan):

```json
{
  "name": "vaultwarden",
  "version": "1.0.0",
  "purl": "pkg:cargo/vaultwarden@1.0.0",
  "scope": "excluded",                                       // ← WRONG
  "type": "library",
  "properties": [
    { "name": "mikebom:lifecycle-scope", "value": "development" }  // ← WRONG
  ]
}
```

**Post-fix wire shape** (same component):

```json
{
  "name": "vaultwarden",
  "version": "1.0.0",
  "purl": "pkg:cargo/vaultwarden@1.0.0",
  "scope": null,                                             // ← CORRECT
  "type": "application",                                     // ← type upgraded via m127 root election
  "properties": [
    // mikebom:lifecycle-scope annotation ABSENT (Runtime is the default,
    // omitted per the m179 default-Runtime-omitted convention)
  ]
}
```

**Note**: The `type: "library"` → `type: "application"` change is an INDIRECT consequence of the m127 root-selector now picking the workspace-root as `metadata.component` (was previously de-prioritized due to excluded scope). Post-fix, the workspace-root moves OUT of `components[]` entirely (it becomes `metadata.component`), so its wire shape in components[] is moot — the direct observable is the metadata.component shape.

For workspace-root [package] entries that DON'T become the m127-elected root (multi-workspace scan where only one wins the root election), the pre-fix vs post-fix diff is just `scope: "excluded"` → `scope: null` and removal of the `mikebom:lifecycle-scope: "development"` property.

## Cross-cutting: FR-002 vs FR-003 boundary

**FR-002** (in scope): workspace-root `[package]` → Runtime.
**FR-003** (guardrail): every non-root cargo entry retains its pre-fix classification.

The boundary is enforced by the additive-only nature of the seed change. Since seeding `[package].name` into `prod_deps` can only ADD elements to `prod_set`, the classifier's three-branch cascade behaves identically for every entry that was PREVIOUSLY in `prod_set` or `build_set`:

- Entry pre-fix in `prod_set` → Runtime. Post-fix in `prod_set` → Runtime. **No change.**
- Entry pre-fix in `build_set` and NOT in `prod_set` → Build. Post-fix: if the workspace-root seed reaches this entry via `[dependencies]` BFS walk, it MIGHT get promoted to Runtime. **This is the ONE regression risk to test.**
- Entry pre-fix in NEITHER → Development. Post-fix: same reasoning as above.

**Regression risk analysis**: The build_set is computed from `[build-dependencies]` seeds. A build-dep that is ALSO transitively reachable from the workspace-root's `[dependencies]` graph (rare — a crate serving dual roles) would flip from Build to Runtime post-fix. Test coverage: the m088 procmacro-edges fixture explicitly tests build-only transitive classification (`syn`, `quote` as build-only proc-macro helpers). If m088's assertions still pass post-fix, this regression class is empirically closed.

**Note on m179 "runtime wins over build" semantics**: m052/m179 already establish that a crate reachable via BOTH `[dependencies]` and `[build-dependencies]` gets classified as Runtime (prod_set is computed first, and the cascade short-circuits). The m200 fix ONLY adds more entries to prod_set — which extends the "Runtime wins" reach in a semantically consistent direction (workspace-root helper crates that were previously Build-only-inaccurate now correctly recognized as Runtime).
