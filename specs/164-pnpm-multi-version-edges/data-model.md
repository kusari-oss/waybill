# Data Model: milestone 164 — pnpm v9 multi-version edge disambiguation

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Phase 1 data model. Milestone 164 is a pure implementation fix — no new component types, no new annotations, no new parity-catalog rows, no wire-format changes. This document catalogs the single Rust-type change + the small function-signature change that constitute the entire data-model surface.

## Rust types

### E1 — `collect_pnpm_dep_names` signature extension (EDITED)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:46-90`

**Pre-164 signature**:
```rust
fn collect_pnpm_dep_names(
    entry_tbl: &serde_yaml::Mapping,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
) -> Vec<String>
```

**Post-164 signature**:
```rust
fn collect_pnpm_dep_names(
    entry_tbl: &serde_yaml::Mapping,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    emit_versioned: bool,   // NEW: milestone 164 (T003)
) -> Vec<String>
```

**Fields**: adds 1 parameter — `emit_versioned: bool`. Return type unchanged (`Vec<String>`, but the semantic content of each string changes based on the new parameter — bare name when `false`, `"<name> <version>"` disambiguation form when `true`).

**Relationships**: consumed by (a) `build_snapshots_lookup` (line 122) — pass `true`; (b) v6/v7 inline path in `parse_pnpm_lock` (line 262) — pass `false`.

**Validation rules**:
- When `emit_versioned=true` AND `canon_ver.is_empty()` (parser degeneracy): emit `tracing::warn!` per FR-008 and fall back to bare-name emission.
- Otherwise: emit `format!("{canon_name} {canon_ver}")` when `emit_versioned=true`, bare `canon_name` when `emit_versioned=false`.
- `canon_name` is never empty (guarded by `parse_pnpm_key`'s existing `Option<(name, version)>` return).

### E2 — `build_snapshots_lookup` call-site update (EDITED)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:122`

**Pre-164**:
```rust
let deps = collect_pnpm_dep_names(tbl, aliases, source_path);
```

**Post-164**:
```rust
let deps = collect_pnpm_dep_names(tbl, aliases, source_path, /* emit_versioned = */ true);
```

**Rationale**: v9 snapshots are the load-bearing multi-version site. Every value in the returned `HashMap<String, Vec<String>>` now contains disambiguation-form strings.

### E3 — v6/v7 inline call-site update (EDITED)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:262`

**Pre-164**:
```rust
} else {
    collect_pnpm_dep_names(tbl, &mut aliases, source_path)
};
```

**Post-164**:
```rust
} else {
    collect_pnpm_dep_names(tbl, &mut aliases, source_path, /* emit_versioned = */ false)
};
```

**Rationale**: v6/v7 inline dep-value shape doesn't include peer-dep suffixes; the bare-name form works correctly today for both single-version AND multi-version pnpm v6/v7 lockfiles (verified via existing v6/v7 goldens continuing to pass). Preserving bare-name emission here honors User Story 2 byte-identity guard.

### E4 — Malformed-key WARN counter (NEW local)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`'s `parse_pnpm_lock` function.

Add two local `usize` accumulators to `parse_pnpm_lock`:
```rust
let mut multi_version_disambiguated_count: usize = 0;
let mut malformed_key_warn_count: usize = 0;
```

The `collect_pnpm_dep_names` function's WARN path needs to increment `malformed_key_warn_count` — done via passing a `&mut usize` (or via returning a small `CollectPnpmStats` struct). We prefer the STRUCT return form for future-proofing:

```rust
struct CollectPnpmStats {
    versioned_count: usize,
    warn_count: usize,
}

fn collect_pnpm_dep_names(
    entry_tbl: &serde_yaml::Mapping,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    emit_versioned: bool,
) -> (Vec<String>, CollectPnpmStats)
```

Actually — for minimum diff, we use the `&mut usize` counter pattern (simpler, and this is the ONLY caller that cares about the counts):

```rust
fn collect_pnpm_dep_names(
    entry_tbl: &serde_yaml::Mapping,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    emit_versioned: bool,
    stats: Option<&mut CollectPnpmStats>,   // Option<&mut _> so v6/v7 caller can pass None
) -> Vec<String>
```

**Decision**: minimize churn by using two `Option<&mut usize>` counters:

```rust
fn collect_pnpm_dep_names(
    entry_tbl: &serde_yaml::Mapping,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    emit_versioned: bool,
    versioned_counter: Option<&mut usize>,
    warn_counter: Option<&mut usize>,
) -> Vec<String>
```

The v6/v7 caller passes `None` for both. The v9 (`build_snapshots_lookup`) caller passes `Some(&mut multi_version_disambiguated_count)` + `Some(&mut malformed_key_warn_count)`. Signature widens from 3 → 6 params — acceptable for a hot internal helper. If reviewers push back, we can refactor to a `CollectPnpmStats` struct.

### E5 — Extended `pnpm-lock parsed` info log (EDITED)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:373-377` (existing info log).

**Pre-164**:
```rust
tracing::info!(
    lockfile = %source_path,
    lockfile_version = %lock_version,
    packages_count = out.len(),
    snapshots_count = snapshots_lookup.len(),
    fell_back_to_snapshots = fell_back_count,
    "pnpm-lock parsed"
);
```

**Post-164**:
```rust
tracing::info!(
    lockfile = %source_path,
    lockfile_version = %lock_version,
    packages_count = out.len(),
    snapshots_count = snapshots_lookup.len(),
    fell_back_to_snapshots = fell_back_count,
    multi_version_disambiguated_count = multi_version_disambiguated_count,   // NEW
    malformed_key_warn_count = malformed_key_warn_count,                     // NEW
    "pnpm-lock parsed"
);
```

**Rationale**: FR-009. Two new fields extend the existing summary line. Backward-compat for consumers doing regex parsing (new fields append, don't reorder).

### E6 — `rewrite_dep_names` post-164 update (EDITED)

**Location**: `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` (existing milestone-159 utility).

Currently `rewrite_dep_names` takes `&[String]` (bare local names) and returns rewritten `Vec<String>`. Post-164, some inputs will be in `"<name> <version>"` form. The rewrite MUST:
1. Split each input on the FIRST space to separate `name` from `version` (if any).
2. Look up `name` in `alias_map`.
3. If match: emit `format!("{aliased_name} {version}")` (preserving version if present) or bare `aliased_name` (if no version).
4. If no match: pass through unchanged.

**Signature unchanged**: still `fn rewrite_dep_names(deps: &[String], alias_map: &AliasMap) -> Vec<String>`. Internal logic gains split-on-space + rejoin.

**Validation**: after rewrite, each output string is either bare `<name>` (input was bare or aliased to non-versioned) or `<name> <version>` (input carried version and pass-through or alias preserved it).

## Wire types

**None.** Milestone 164 changes intermediate parser state (`PackageDbEntry.depends`) but does NOT change emitted SBOM wire format. Every emitted PURL, edge, annotation, and metadata field is identical in SHAPE — only the target PURL of edges changes to the CORRECT version (per FR-005 the emitted PURL never includes the peer-dep suffix, unchanged from pre-164).

## Relationships

```text
build_snapshots_lookup (v9 path)
    ↓ calls with emit_versioned=true
collect_pnpm_dep_names
    ↓ pushes into deps
    ├── "@foo/bar 1.2.3"     ← versioned form (multi-version safe)
    ├── "@baz 4.5.6"
    └── ...
    ↓ returned to snapshots_lookup[canonical] = deps
parse_pnpm_lock main loop
    ↓ resolves canonical = "<name>@<version>" per package
    ↓ if snapshots_lookup.get(&canonical).is_some() → depends = snap_deps.clone()
    ↓ PackageDbEntry.depends = ["@foo/bar 1.2.3", "@baz 4.5.6", ...]
milestone-159 alias post-processing
    ↓ rewrite_dep_names(depends, alias_map)
    ↓ preserves version segment across alias substitution
    ↓ depends now = ["@real/foo 1.2.3", "@baz 4.5.6", ...] (if @foo/bar was aliased)
scan_fs/mod.rs:471-525 index build
    ↓ name_to_purl keys inserted for both bare AND disambiguation form
scan_fs/mod.rs:729-731 edge emit
    ↓ for dep_name in entry.depends
    ↓ normalize_dep_name(ecosystem, dep_name) → e.g., "@foo/bar 1.2.3"
    ↓ name_to_purl.get(&key) → hits disambiguation-form entry → correct-version PURL
    ↓ Relationship.to = "pkg:npm/@foo/bar@1.2.3"   ← CORRECT
```

## State transitions

**Pre-164 → Post-164 for a single snapshot dep** (`@algolia/autocomplete-core: 1.17.9(...)` as declared by `@docsearch/react@3.9.0`):

```text
parse_pnpm_key(stripped) → Some(("@algolia/autocomplete-core", "1.17.9"))

Pre-164:
    let Some((canon_name, _canon_ver)) = ...;
    deps.push(canon_name);
    → deps contains "@algolia/autocomplete-core"

    Later, at edge emit:
    key = ("npm", normalize_dep_name("npm", "@algolia/autocomplete-core"))
    name_to_purl.get(&key) → returns LAST-INSERTED PURL for that name
    → often "pkg:npm/@algolia/autocomplete-core@1.19.8" (WRONG)

Post-164:
    let Some((canon_name, canon_ver)) = ...;
    if emit_versioned && !canon_ver.is_empty() {
        deps.push(format!("{canon_name} {canon_ver}"));
    } else {
        deps.push(canon_name);
    }
    → deps contains "@algolia/autocomplete-core 1.17.9"

    Later, at edge emit:
    key = ("npm", normalize_dep_name("npm", "@algolia/autocomplete-core 1.17.9"))
    name_to_purl.get(&key) → hits the "<name> <version>" disambiguation entry
    → "pkg:npm/@algolia/autocomplete-core@1.17.9" (CORRECT)
```

## Data volume assumptions

- **Per-lockfile impact**: podman-desktop's pnpm-lock has 2668 snapshots × ~5 deps per snapshot avg = ~13,340 disambiguation-form strings emitted per scan. Each is ~40-80 bytes of allocation (canon_name + space + version). Delta memory: ~1 MB temporary per pnpm-lock parse. Negligible.
- **Per-lockfile compute**: `format!("{} {}", ...)` per dep = one small heap allocation. 13k allocations × pnpm-lock parse ≈ ~10 ms extra on the hot path. Empirically verified during T003 to have no measurable perf regression.
- **Per-scan compute**: only pnpm-lock v9 files trigger the new path. Repos without pnpm-lock v9 have zero delta.

## Validation rules (aggregated)

| Rule | Enforcement |
|------|-------------|
| Zero empty-version PURLs post-164 (SC-004 inherit) | Enforced by construction — no PURL emission path is touched (FR-005). Preserved by milestone-163 mechanism. |
| Zero phantom edges post-164 (SC-002 inherit) | Enforced by construction — resolver behavior unchanged. Preserved by milestone-163 mechanism. |
| Multi-version orphans ≤ 30 (SC-001) | Enforced by test T012 + audit T017. Every parent's `depends` entry resolves to a lockfile-declared PURL via disambiguation lookup. |
| BFS reachability ≥ 93% (SC-002) | Consequence of SC-001 fix. Enforced by audit T017 (opt-in) + T012 integration test on synthesized fixture. |
| Component count preserved (SC-005) | Enforced by construction — no components added or removed. Only edge targets change. Verified via T012 integration test. |
| FR-005 emitted PURL never contains `(` | Enforced by construction — `parse_pnpm_key` strips peer-dep suffix; emitted PURLs use base version only. Verified by unit test T013. |
| FR-010 peerDependencies unchanged | Enforced by scope — milestone 164 doesn't touch peerDependencies handling. Verified by unit test T014. |
| Malformed-key fallback logs WARN + preserves bare-name (FR-008) | Enforced by explicit branch in `collect_pnpm_dep_names`. Verified by unit test T009. |
