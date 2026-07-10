# Quickstart: yarn v1 + Berry optional-dep classification (m181)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Consumer flow (unchanged from m179/m180)

Same jq recipes work — the emission side is uniform. Yarn-classified components appear alongside npm/pnpm/Cargo/Go-classified optional deps under the same `mikebom:optional-derivation = "npm-optional-dependencies"` value.

### Distinguishing yarn vs npm/pnpm (if a consumer needs to)

Use `evidence.source_file_paths`:

```bash
# Optional components classified from a yarn.lock specifically:
jq -r '
  [ .components[]
    | select(.properties[]? | select(.name == "mikebom:optional-derivation" and .value == "npm-optional-dependencies"))
    | select(.evidence.occurrences[]?.location | test("yarn\\.lock$"))
    | .purl ]
  | sort | unique | .[]
' scan.cdx.json
```

The derivation value stays coarse (`"npm-optional-dependencies"` shared across npm/yarn/pnpm/bun) per m180 design intent; the format-native `source_file_paths` field is the drill-down source.

## Developer flow — extending yarn's optional-dep classifier

### Step 1: parse root package.json alongside yarn.lock

Extend `read_yarn_lock` to load package.json — pass as `serde_json::Value` (Null on any error):

```rust
let pkg_json = std::fs::read_to_string(rootfs.join("package.json"))
    .ok()
    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    .unwrap_or(serde_json::Value::Null);
```

### Step 2 (v1): split the sub-block accumulator

Inside `parse_v1`'s body-block loop:

```rust
// Existing:
let mut in_deps_block = false;
// NEW m181:
let mut is_optional_block = false;

// In the leading_ws == 2 branch:
if trimmed == "dependencies:" {
    in_deps_block = true;
    is_optional_block = false;
} else if trimmed == "optionalDependencies:" {
    in_deps_block = true;
    is_optional_block = true;  // NEW
}

// In the leading_ws >= 4 branch:
if is_optional_block {
    optional_dep_names.push(dep_clean);
} else {
    dep_names.push(dep_clean);
}
```

### Step 3 (Berry): walk `dependenciesMeta`

```rust
fn berry_optional_names_from_pkg_json(pkg_json: &serde_json::Value) -> HashSet<String> {
    pkg_json
        .get("dependenciesMeta")
        .and_then(|v| v.as_object())
        .into_iter()
        .flat_map(|obj| obj.iter())
        .filter(|(_, meta)| {
            meta.get("optional").and_then(|v| v.as_bool()) == Some(true)
        })
        .map(|(name, _)| name.to_string())
        .collect()
}
```

### Step 4: apply the peer-precedence guard

```rust
optional_names.retain(|n| !crate::scan_fs::package_db::npm::peer_optional::is_peer_optional(n, pkg_json));
```

### Step 5: extend `build_entry`

Add the `optional_names: &HashSet<String>` parameter:

```rust
fn build_entry(
    name: &str,
    version: &str,
    source_path: &str,
    depends: Vec<String>,
    optional_names: &HashSet<String>,
) -> Option<PackageDbEntry> {
    let purl = build_npm_purl(name, version)?;
    let (lifecycle_scope, extra_annotations) = if optional_names.contains(name) {
        let mut ann: BTreeMap<String, serde_json::Value> = Default::default();
        ann.insert(
            "mikebom:optional-derivation".to_string(),
            serde_json::Value::String("npm-optional-dependencies".to_string()),
        );
        (Some(mikebom_common::resolution::LifecycleScope::Optional), ann)
    } else {
        (None, BTreeMap::new())
    };
    Some(PackageDbEntry {
        // ... existing fields unchanged ...
        lifecycle_scope,
        extra_annotations,
        // ...
    })
}
```

### Step 6: verify — three test bursts

Unit tests:
```rust
#[test]
fn v1_optional_dep_populates_lifecycle_scope_optional() {
    let text = r#"
"parent-pkg@^1.0.0":
  version "1.0.0"
  optionalDependencies:
    optional-child "^2"
"#;
    let pkg_json = serde_json::Value::Null;
    let entries = parse_yarn_lock(text, "yarn.lock", &pkg_json);
    let child = entries.iter().find(|e| e.name == "optional-child").expect("emitted");
    assert_eq!(child.lifecycle_scope, Some(LifecycleScope::Optional));
    assert!(child.extra_annotations.contains_key("mikebom:optional-derivation"));
}
```

Integration tests: mirror the m180 `optional_dep_npm_e2e.rs` shape — new fixture + new e2e test file per user story.

Test the peer-precedence guard: create a fixture where react is BOTH peer-optional AND in `dependenciesMeta` (Berry) or in a parent's `optionalDependencies:` (v1). Assert `PROVIDED_DEPENDENCY_OF` wins.

### Step 7: golden regen + pre-PR gate

Same pattern as m180:
```
MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --workspace
./scripts/pre-pr.sh
```

## Removing the `#[allow(dead_code)]` marker

After m181 wires yarn to use `peer_optional::is_peer_optional`, the marker becomes unnecessary. Update `peer_optional.rs`:

```rust
// Before (m180):
#[allow(dead_code)] // Used by US3 yarn reader; the m180 US1/US2 readers use lockfile `peer` flag directly.
pub(crate) fn is_peer_optional(entry_name: &str, parent_pkg_json: &Value) -> bool { ... }

// After (m181):
pub(crate) fn is_peer_optional(entry_name: &str, parent_pkg_json: &Value) -> bool { ... }
```

Update the docstring's "Reader usage note" to reflect yarn now consumes the helper.
