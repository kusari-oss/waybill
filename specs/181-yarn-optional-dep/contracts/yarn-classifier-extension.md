# Contract: Yarn Classifier Extension

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## Scope

Extends the yarn reader (`yarn_lock.rs`) — both v1 and Berry code paths — to classify optional-declared deps as `LifecycleScope::Optional` + emit `mikebom:optional-derivation = "npm-optional-dependencies"`. Zero changes to the emitter layer; all work is inside the reader.

## Reader-Level Contract

The single-file change follows this shape:

```rust
// 1. read_yarn_lock also loads package.json:
pub(super) fn read_yarn_lock(rootfs: &Path, _include_dev: bool) -> Option<Vec<PackageDbEntry>> {
    let yarn_lock_path = rootfs.join("yarn.lock");
    let text = std::fs::read_to_string(&yarn_lock_path).ok()?;
    let source_path = yarn_lock_path.to_string_lossy().into_owned();

    // NEW m181: parse root package.json; Value::Null on any error (FR-004 fail-safe).
    let pkg_json = std::fs::read_to_string(rootfs.join("package.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    if pkg_json.is_null() {
        tracing::debug!(
            path = %yarn_lock_path.display(),
            "package.json missing or unparseable — yarn optional-dep classification skipped"
        );
    }

    let out = parse_yarn_lock(&text, &source_path, &pkg_json);
    if out.is_empty() { None } else { Some(out) }
}

// 2. parse_yarn_lock threads pkg_json through:
pub(super) fn parse_yarn_lock(
    text: &str,
    source_path: &str,
    pkg_json: &serde_json::Value,
) -> Vec<PackageDbEntry> {
    if is_berry(text) {
        parse_berry(text, source_path, pkg_json)
    } else {
        parse_v1(text, source_path, pkg_json)
    }
}

// 3. Each parser builds the optional-name set + calls build_entry:
fn parse_v1(text: &str, source_path: &str, pkg_json: &serde_json::Value) -> Vec<PackageDbEntry> {
    // ... existing body-block loop, split accumulator into regular + optional ...

    // After parse completes: build classifier input set.
    let mut optional_names: HashSet<String> = optional_children_seen.difference(&regular_children_seen).cloned().collect();
    // FR-005 peer-precedence guard:
    optional_names.retain(|n| !crate::scan_fs::package_db::npm::peer_optional::is_peer_optional(n, pkg_json));

    // Build entries with the classifier input:
    for ((name, version), deps) in acc {
        if let Some(entry) = build_entry(&name, &version, source_path, deps, &optional_names) {
            out.push(entry);
        }
    }
    out
}

fn parse_berry(text: &str, source_path: &str, pkg_json: &serde_json::Value) -> Vec<PackageDbEntry> {
    // Build optional-name set from pkg_json[dependenciesMeta]:
    let mut optional_names: HashSet<String> = berry_optional_names_from_pkg_json(pkg_json);
    // FR-005 peer-precedence guard:
    optional_names.retain(|n| !crate::scan_fs::package_db::npm::peer_optional::is_peer_optional(n, pkg_json));

    // ... existing YAML mapping walk, calling build_entry with optional_names ...
}

// 4. build_entry gains the classifier parameter:
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
        (Some(LifecycleScope::Optional), ann)
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

## Per-Variant Deviations

### yarn v1 optional-set construction

The existing body-block loop at `parse_v1` line 168-195 already detects `dependencies:` and `optionalDependencies:` sub-blocks (line 183). m181's change:

- Introduce `let mut is_optional_block = false;` companion to the existing `let mut in_deps_block = false;`
- Set it when the trimmed line is `"optionalDependencies:"` (currently the same branch also sets `in_deps_block = true`)
- In the sub-block-content branch (leading_ws >= 4), route the collected name into `optional_dep_names` instead of `dep_names` when `is_optional_block`
- Post-loop: track UNION-of-all-parents' `optional_dep_names` and UNION-of-all-parents' `regular_dep_names` in scan-wide `HashSet`s
- Final `optional_names` = `optional_seen - regular_seen` (diamond-shape rule per FR-007)

### yarn Berry optional-set construction

Berry's optional info is NOT in the lockfile. Read from package.json:

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

## Emission Contract (unchanged from m179/m180)

Whenever a yarn-emitted component has `lifecycle_scope = Some(LifecycleScope::Optional)`:
- **CDX 1.6**: `scope: "excluded"` (auto via `is_non_runtime()`) + `properties[]` entry `mikebom:optional-derivation = "npm-optional-dependencies"`
- **SPDX 2.3 Full mode**: source-side of `OPTIONAL_DEPENDENCY_OF` (reversed direction per m052 convention) + annotation
- **SPDX 2.3 Basic mode**: natural-direction `DEPENDS_ON` + annotation still present
- **SPDX 3.0.1**: `mikebom:optional-derivation` annotation only (no native `lifecycleScope: optional`)

## Test Contract

Per data-model.md §5 — unit tests colocated with `yarn_lock.rs` tests module + integration tests under `tests/optional_dep_yarn_*_e2e.rs`.
