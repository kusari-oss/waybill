# Quickstart: milestone 164 — pnpm v9 multi-version edge disambiguation

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Contributor onboarding for milestone 164. Assumes a working mikebom dev environment (per top-level `CLAUDE.md`).

## 1. Prerequisites

- Rust stable toolchain (workspace-managed).
- No external tooling required (integration test synthesizes fixture in tempdir).

Verify:
```bash
cargo +stable --version   # cargo 1.75+
```

## 2. Implementation overview

Milestone 164 is scoped to a SINGLE FILE:

- `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs` — the parser change.

Plus:
- `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` — one small update to `rewrite_dep_names` per research §R7 (preserve version segment through alias substitution).
- `mikebom-cli/tests/pnpm_multi_version.rs` — NEW SC-008 integration test.
- `mikebom-cli/tests/pnpm_multi_version_audit.rs` — NEW opt-in SC-010 audit test.

**Total surface**: 4 files (2 edited, 2 new). Matches milestone-087 cargo-fix footprint precedent.

## 3. Step-by-step implementation

### 3a. Extend `collect_pnpm_dep_names` signature (T003)

At `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:46-90`:

```rust
fn collect_pnpm_dep_names(
    entry_tbl: &serde_yaml::Mapping,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    emit_versioned: bool,                     // ← NEW
    versioned_counter: Option<&mut usize>,    // ← NEW (FR-009 tally)
    warn_counter: Option<&mut usize>,         // ← NEW (FR-009 tally)
) -> Vec<String> {
    let mut deps: Vec<String> = Vec::new();
    // ...existing section loop...
    for (dep_key, dep_value) in sub {
        // ...existing alias detection unchanged...
        let dep_pair_raw = format!("{dep_name}@{dep_ver_raw}");
        let stripped = dep_pair_raw.strip_prefix('/').unwrap_or(&dep_pair_raw);
        let Some((canon_name, canon_ver)) = parse_pnpm_key(stripped) else {
            tracing::debug!(dep = %dep_pair_raw, "pnpm-lock: skipping non-registry dep value");
            continue;
        };
        // Milestone 164 (T003): thread version through when caller requests.
        if emit_versioned {
            if canon_ver.is_empty() {
                tracing::warn!(
                    key = %stripped,
                    "pnpm-lock v9: peer-dep-suffixed key parsed to empty version; falling back to bare-name form"
                );
                if let Some(c) = warn_counter.as_deref_mut() { *c += 1; }
                deps.push(canon_name);
            } else {
                if let Some(c) = versioned_counter.as_deref_mut() { *c += 1; }
                deps.push(format!("{canon_name} {canon_ver}"));
            }
        } else {
            deps.push(canon_name);
        }
    }
    deps.sort();
    deps.dedup();
    deps
}
```

**Note**: `versioned_counter` and `warn_counter` use `Option<&mut usize>` so the v6/v7 caller can pass `None` without allocating a dummy counter.

### 3b. Update `build_snapshots_lookup` call-site (T004)

At `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:101-126`:

```rust
fn build_snapshots_lookup(
    root: &serde_yaml::Value,
    aliases: &mut Vec<super::alias_mapping::AliasResolution>,
    source_path: &str,
    versioned_counter: &mut usize,   // ← NEW (thread through from caller)
    warn_counter: &mut usize,        // ← NEW
) -> std::collections::HashMap<String, Vec<String>> {
    let mut out = std::collections::HashMap::new();
    // ...existing preamble...
    for (key, entry) in snapshots {
        // ...existing key parsing...
        let deps = collect_pnpm_dep_names(
            tbl,
            aliases,
            source_path,
            /* emit_versioned = */ true,
            Some(versioned_counter),
            Some(warn_counter),
        );
        out.insert(canonical, deps);
    }
    out
}
```

### 3c. Update `parse_pnpm_lock` v6/v7 inline call-site (T005)

At `pnpm_lock.rs:262`:

```rust
} else {
    collect_pnpm_dep_names(
        tbl,
        &mut aliases,
        source_path,
        /* emit_versioned = */ false,
        None,
        None,
    )
};
```

### 3d. Add tally locals + extend info log (T006)

Near the top of `parse_pnpm_lock`:

```rust
let mut multi_version_disambiguated_count: usize = 0;
let mut malformed_key_warn_count: usize = 0;
```

And thread these into `build_snapshots_lookup`:

```rust
let snapshots_lookup = build_snapshots_lookup(
    root,
    &mut aliases,
    source_path,
    &mut multi_version_disambiguated_count,
    &mut malformed_key_warn_count,
);
```

Extend the existing info log at `pnpm_lock.rs:373-377`:

```rust
tracing::info!(
    lockfile = %source_path,
    lockfile_version = %lock_version,
    packages_count = out.len(),
    snapshots_count = snapshots_lookup.len(),
    fell_back_to_snapshots = fell_back_count,
    multi_version_disambiguated_count = multi_version_disambiguated_count,
    malformed_key_warn_count = malformed_key_warn_count,
    "pnpm-lock parsed"
);
```

### 3e. Update `rewrite_dep_names` for alias composition (T007)

At `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs`:

Split each dep on the first space to separate `name` from `version`. Look up `name` in `alias_map`. If found, emit `format!("{aliased_name} {version}")` (with version preserved) or bare `aliased_name` (if no version).

Pseudocode:
```rust
pub fn rewrite_dep_names(deps: &[String], alias_map: &AliasMap) -> Vec<String> {
    deps.iter().map(|dep| {
        let (name, version_opt) = split_first_space(dep);
        match alias_map.get(name) {
            Some(aliased) => match version_opt {
                Some(ver) => format!("{} {}", aliased.aliased_name, ver),
                None => aliased.aliased_name.clone(),
            },
            None => dep.clone(),
        }
    }).collect()
}

fn split_first_space(s: &str) -> (&str, Option<&str>) {
    match s.find(' ') {
        Some(idx) => (&s[..idx], Some(&s[idx + 1..])),
        None => (s, None),
    }
}
```

### 3f. Write unit tests (T008-T014)

Inside the existing `#[cfg(test)] mod tests` block at end of `pnpm_lock.rs`. Follow the milestone-163 T024/T024a naming convention (`t008_*` through `t014_*`). Each test synthesizes a minimal `serde_yaml::Mapping` and asserts the expected output shape. See research §R5 for the enumerated test list.

### 3g. Write integration test (T015)

Create `mikebom-cli/tests/pnpm_multi_version.rs`. Synthesize a tempdir with:
- `pnpm-lock.yaml` v9 containing two versions of `@shared/lib` (`1.0.0` and `2.0.0`) with two workspace peers each declaring a different version.
- `pnpm-workspace.yaml` listing the workspace peers.
- Two peer `package.json` files.

Invoke the release binary via `env!("CARGO_BIN_EXE_mikebom")`. Parse emitted CDX. Assert per SC-008.

### 3h. Optional real-testbed audit (T017)

Create `mikebom-cli/tests/pnpm_multi_version_audit.rs`. Gated behind `MIKEBOM_PNPM_MULTIVER_AUDIT=1`. If a cached `podman-desktop` is available (via `MIKEBOM_FIXTURES_DIR`), scan it and assert multi-version orphans ≤ 30 AND BFS reachability ≥ 93%.

## 4. Testing

```bash
# Full pre-PR gate
./scripts/pre-pr.sh

# Unit tests only (pnpm-lock module)
cargo +stable test --bin mikebom scan_fs::package_db::npm::pnpm_lock

# Integration test
cargo +stable test --test pnpm_multi_version

# Optional real-testbed audit (only if you have a podman-desktop cache)
MIKEBOM_PNPM_MULTIVER_AUDIT=1 \
    MIKEBOM_FIXTURES_DIR=/tmp/podman-desktop-cache \
    cargo +stable test --test pnpm_multi_version_audit
```

## 5. Debugging: tracing recipes

```bash
# See per-lockfile summary with the two new milestone-164 fields
RUST_LOG=mikebom_cli::scan_fs::package_db::npm::pnpm_lock=info \
    mikebom sbom scan --path <podman-desktop-clone> 2>&1 \
    | grep 'pnpm-lock parsed'

# Expected fields include:
#   multi_version_disambiguated_count=<N>
#   malformed_key_warn_count=<W>
```

## 6. Empirical verification against live podman-desktop

Post-implementation, verify SC-001 + SC-002 on a fresh clone:

```bash
TMPDIR=$(mktemp -d)
git clone --depth 1 https://github.com/podman-desktop/podman-desktop.git "$TMPDIR/podman-desktop"

./target/release/mikebom --offline sbom scan \
    --path "$TMPDIR/podman-desktop" \
    --output "$TMPDIR/out.cdx.json" \
    --no-deep-hash

# BFS reachability check
python3 <<'PY'
import json, collections
sbom = json.load(open('/tmp/xxx/out.cdx.json'))  # replace path
root = sbom['metadata']['component']['purl']
deps = sbom['dependencies']
adj = {d['ref']: d.get('dependsOn', []) for d in deps}
visited, q = set(), collections.deque([root])
while q:
    c = q.popleft()
    if c in visited: continue
    visited.add(c)
    for t in adj.get(c, []): q.append(t)
npm = [c['purl'] for c in sbom['components'] if c.get('purl','').startswith('pkg:npm/')]
reach = sum(1 for p in npm if p in visited)
print(f"BFS reachability: {reach}/{len(npm)} = {reach/len(npm)*100:.1f}%")
# Expected: ≥93% (up from 77.4% pre-164 baseline)
PY
```

## 7. Common pitfalls

- **Forgetting to update `rewrite_dep_names`**: milestone-159's alias post-processing will silently break for versioned inputs. Symptom: aliased deps become bare after rewrite even though the version was present in the input.
- **Off-by-one in `split_first_space`**: use `s.find(' ')` (first space), NOT `s.rfind(' ')` (last space). Scoped package names never contain spaces, so first-space split is safe.
- **Failing to update the info log**: milestone-164's FR-009 requires the two new counter fields. Missing them fails the FR-009 unit test.
- **Passing `emit_versioned=true` in the v6/v7 path**: User Story 2 byte-identity guard violated. Existing v6/v7 golden tests catch this.
