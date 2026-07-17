# Data Model: Root-Selector Workspace-Root Disambiguation

**Date**: 2026-07-17
**Purpose**: Document the one new internal annotation + the two code paths this fix touches. No new struct fields introduced.

## E1: `mikebom:is-cargo-workspace-toplevel` (NEW internal-only annotation)

**Location**: Stamped in `mikebom-cli/src/scan_fs/package_db/cargo.rs::build_cargo_main_module_entry` (line 363+).

**Wire shape**: `serde_json::Value::Bool(true)` — always the literal `true` when stamped. Never `false` (absence means "not a cargo workspace top-level").

**Emission gate**: Filtered from CDX / SPDX 2.3 / SPDX 3 emission via `is_internal_emission_key` at `root_selector.rs:437-439` (extended in m201). Consumers of emitted SBOMs NEVER see this annotation — matches the treatment of `mikebom:is-workspace-root`.

**Semantic**: "The Cargo.toml that produced THIS PackageDbEntry contains BOTH a `[package]` block AND a `[workspace]` block at manifest root." Equivalently: this crate IS the top-level workspace root, not a workspace member.

**Fires on**:
- `<repo>/Cargo.toml` with `[package] name = "app"` + `[workspace] members = ["helper"]` → app entry gets the annotation.
- Workspace-member Cargo.tomls like `<repo>/helper/Cargo.toml` with just `[package] name = "helper"` (no `[workspace]`) → helper entry does NOT get the annotation.
- Virtual-workspace Cargo.toml (`[workspace]` alone, no `[package]`) → no `[package]` = no main-module emitted at all = no annotation possible.
- Single-crate Cargo.toml (`[package]` alone, no `[workspace]`) → no `[workspace]` = no annotation. Single-crate correctness is preserved via the fallback filesystem-based check at `scan_fs/mod.rs:922-942` (the single-crate's Cargo.toml IS at rootfs).

**Consumer**: `mikebom-cli/src/scan_fs/mod.rs::stamp_is_workspace_root_annotation` (or equivalent), the post-scan stamping that currently derives `mikebom:is-workspace-root` from filesystem comparison. Post-m201, this consumer short-circuits: `if extra_annotations["mikebom:is-cargo-workspace-toplevel"] == Some(true) → is_workspace_root = true` (skipping the filesystem check for cargo-workspace-toplevel components).

**Validation rules**:
- Boolean literal (never null, never other type).
- Absence is normal (workspace members, non-cargo mainmods, single-crate cargo projects all lack it).
- Stamped ONLY by the cargo reader; other ecosystems never stamp it.
- Idempotent: re-stamping (via dedup or augment-in-place) preserves the true value.

## E2: `mikebom:is-workspace-root` (EXISTING annotation — semantics unchanged)

**Location**: `mikebom-cli/src/scan_fs/mod.rs:944-947` (stamping site) + `mikebom-cli/src/generate/root_selector.rs:304-309` (reader).

**Wire shape**: `serde_json::Value::Bool(true|false)`. Internal-only per existing `is_internal_emission_key`.

**Change** (m201 indirect effect):

Pre-m201, the boolean is derived exclusively from the filesystem comparison:
```rust
let is_workspace_root = match (manifest_path, canonical_root.as_ref()) {
    (Some(p), Some(canon_root)) => canonicalize(parent(p)) == canonicalize(canon_root),
    _ => false,
};
```

Post-m201, the derivation short-circuits via E1 first:
```rust
let is_workspace_root = if e1_annotation_true {
    true  // cargo-workspace-toplevel positive identifier
} else {
    // Filesystem-based fallback (unchanged from pre-m201).
    match (manifest_path, canonical_root.as_ref()) { ... }
};
```

**Result post-m201**:
- Cargo workspace TOP-LEVEL: E1 stamp true → is_workspace_root = true.
- Cargo workspace MEMBER: E1 stamp absent, filesystem check fails (member's manifest_parent != rootfs) → is_workspace_root = false. (Pre-m201, this was ERRONEOUSLY true due to the shared Cargo.lock path.)
- Cargo single-crate: E1 stamp absent (no [workspace]), filesystem check passes (crate's Cargo.toml IS at rootfs, and its Cargo.lock is too) → is_workspace_root = true. (Same as pre-m201 for single-crate case.)
- Non-cargo main-modules (npm/python/etc.): E1 stamp absent, filesystem check runs unchanged → same behavior as pre-m201.

**Semantic guarantee**: For any given scan, the count of cargo mainmods with `is_workspace_root = true` drops from N (all of them) to ≤1 (only the workspace top-level, or 0 for a virtual workspace). Non-cargo count unchanged.

## E3: `is_internal_emission_key` filter extension

**Location**: `mikebom-cli/src/generate/root_selector.rs:437-439`.

**Change**:

Pre-m201:
```rust
pub fn is_internal_emission_key(key: &str) -> bool {
    key == IS_WORKSPACE_ROOT_KEY
}
```

Post-m201:
```rust
pub fn is_internal_emission_key(key: &str) -> bool {
    matches!(key, IS_WORKSPACE_ROOT_KEY | "mikebom:is-cargo-workspace-toplevel")
}
```

**Effect**: The new annotation is filtered out of every emitted SBOM property/annotation bag by the CDX/SPDX emitters — matches existing treatment of `mikebom:is-workspace-root`. Consumers observe zero net change to emitted SBOM shape.

## Cross-cutting: FR-002 root-election flow (post-m201)

```text
[Scan produces main-modules with mikebom:component-role: main-module]
  │
  ├── For each main-module component:
  │      │
  │      ├── (Cargo mainmod only) build_cargo_main_module_entry stamps
  │      │       `mikebom:is-cargo-workspace-toplevel: true`
  │      │       IF Cargo.toml has [package] AND [workspace] blocks.
  │      │
  │      └── scan_fs/mod.rs post-processing:
  │             is_workspace_root = e1_annotation_true
  │               ? true
  │               : (manifest_parent == canonical_root)
  │             stamp `mikebom:is-workspace-root` accordingly.
  │
  ▼
[m127 root-selector]
  │
  ├── workspace_root_modules = [i for i in main_modules if is_workspace_root(components[i])]
  │
  ├── Ladder branch 3 (RepoRoot):
  │     if workspace_root_modules.len() == 1:
  │         WINNER = workspace_root_modules[0]  ← this is where vaultwarden now wins
  │         heuristic = "repo-root", confidence = 0.90
  │
  ├── Ladder branch 4 (EcosystemPriority):
  │     if workspace_root_modules.len() > 1:  ← still fires for genuine multi-root
  │         WINNER = pick_by_ecosystem(workspace_root_modules)
  │
  └── Ladder branches 5-7 (LCP, MavenCoord, SyntheticPlaceholder):
        Unchanged from pre-m201.
```

**Vaultwarden reproducer trace (post-m201)**:
- `vaultwarden`: Cargo.toml has [package] + [workspace] → E1 stamp true → is_workspace_root = true.
- `macros`: Cargo.toml has [package] only (no [workspace]) → E1 stamp absent. Filesystem check: manifest_parent = `<repo>/macros` != `<repo>` → is_workspace_root = false. (Pre-m201: shared Cargo.lock path fooled the check into `true`.)
- `scenarios`: npm mainmod. E1 stamp absent (cargo-only). Filesystem check: manifest_parent = `<repo>/playwright` != `<repo>` → is_workspace_root = false. (Same as pre-m201.)
- workspace_root_modules = [vaultwarden_idx] (length 1) → RepoRoot ladder fires → vaultwarden wins. Heuristic = "repo-root".
