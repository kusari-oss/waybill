# Quickstart: npm / yarn / pnpm optional-dep classification (m180)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Consumer flow — filter not-in-production JavaScript components

Same recipes as m179's quickstart — the emission side is uniform per m179's design. New for m180: the `mikebom:optional-derivation` annotation value `"npm-optional-dependencies"` identifies JavaScript-ecosystem-lockfile classifications.

### CycloneDX 1.6

```bash
# Get all PURLs that are NOT in the production deployment
jq -r '[
  ( .metadata.component.purl // empty ),
  ( .components[] | select(.scope == "excluded") | .purl )
] | sort | unique | .[]' scan.cdx.json > excluded-purls.txt
```

### SPDX 2.3 (under default --spdx2-relationship-compat=full)

```bash
jq -r '
  ( [ .packages[] | { key: .SPDXID, value: (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator) } ] | from_entries ) as $purl_by_ref |
  [ .relationships[]
    | select(.relationshipType | test("^(TEST|DEV|BUILD|OPTIONAL)_DEPENDENCY_OF$"))
    | $purl_by_ref[.spdxElementId]
  ] | sort | unique | .[]
' scan.spdx23.json > excluded-purls.txt
```

Both recipes MUST return the same sorted PURL set for the same JavaScript scan (contract: `contracts/javascript-filter-parity.md`).

### Distinguishing JavaScript vs. Rust optionals

To filter to JavaScript-classified components only, use the `mikebom:optional-derivation` annotation:

```bash
# Optional components classified by a JavaScript lockfile (npm/pnpm/yarn/bun)
jq -r '[
  .components[]
  | select(.properties[]? | select(.name == "mikebom:optional-derivation" and .value == "npm-optional-dependencies"))
  | .purl
] | sort | unique | .[]' scan.cdx.json
```

## Developer flow — extending a NEW JavaScript-ecosystem lockfile reader

Template: the npm `package_lock.rs` change (US1) is the canonical example.

### Step 1: detect the optional signal

Parse the ecosystem-specific field. For npm:

```rust
// Already exists in package_lock.rs at lines 63-66 + 97-100:
let is_optional = entry
    .get("optional")
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
```

For yarn v1 (name-based membership test):

```rust
// Pre-pass to build the optional-child-name set from every parent's
// `optionalDependencies:` sub-block:
let optional_names: HashSet<String> = extract_optional_children(&parsed_lockfile);

// Per-entry:
let is_optional = optional_names.contains(&entry_name);
```

### Step 2: apply the peer-precedence guard

Cross-reference the parent's `package.json` for peer + peer-optional deps:

```rust
fn is_peer_optional(entry_name: &str, parent_package_json: &Value) -> bool {
    let has_peer = parent_package_json
        .get("peerDependencies")
        .and_then(|v| v.as_object())
        .map(|m| m.contains_key(entry_name))
        .unwrap_or(false);
    let is_optional_peer = parent_package_json
        .get("peerDependenciesMeta")
        .and_then(|v| v.get(entry_name))
        .and_then(|m| m.get("optional"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    has_peer && is_optional_peer
}
```

### Step 3: apply the three-way classifier

```rust
let lifecycle_scope = if is_dev {
    Some(LifecycleScope::Development)
} else if is_optional && !is_peer_optional(&entry_name, parent_pkg_json) {
    Some(LifecycleScope::Optional)
} else {
    Some(LifecycleScope::Runtime)   // or None for yarn v1
};

let mut extra_annotations = /* existing bag */;
if matches!(lifecycle_scope, Some(LifecycleScope::Optional)) {
    extra_annotations.insert(
        "mikebom:optional-derivation".to_string(),
        serde_json::Value::String("npm-optional-dependencies".to_string()),
    );
}
```

### Step 4: verify

Add a unit test (mirror m179's `cargo_optional_true_populates_optional_deps`):

```rust
#[test]
fn npm_optional_true_sets_lifecycle_scope_optional() {
    let lockfile = r#"{
        "packages": {
            "node_modules/fsevents": {
                "version": "2.3.3",
                "optional": true
            }
        }
    }"#;
    let entries = parse_package_lock(lockfile, /* include_dev */ true);
    let fsevents = entries.iter().find(|e| e.name == "fsevents").unwrap();
    assert_eq!(fsevents.lifecycle_scope, Some(LifecycleScope::Optional));
    assert_eq!(
        fsevents.extra_annotations.get("mikebom:optional-derivation"),
        Some(&serde_json::Value::String("npm-optional-dependencies".into()))
    );
}
```

### Step 5: add peer-precedence guard test

```rust
#[test]
fn npm_peer_optional_dep_stays_peer_not_optional() {
    // package.json with peerDependencies: {react: '^18'}, peerDependenciesMeta: {react: {optional: true}}
    let entries = parse_with_fixture("peer_optional/package_lock");
    let react = entries.iter().find(|e| e.name == "react").unwrap();
    // Peer classification wins — no Optional lifecycle, no annotation.
    assert_ne!(react.lifecycle_scope, Some(LifecycleScope::Optional));
    assert!(react.extra_annotations.get("mikebom:optional-derivation").is_none());
}
```

## Testing your integration

Same as m179:

1. `./scripts/pre-pr.sh` must pass clean.
2. `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --workspace` → verify ADDITIVE changes only.
3. `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --workspace` → verify additive `OPTIONAL_DEPENDENCY_OF` edges + no decrement in existing `*_DEPENDENCY_OF` counts.
4. `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --workspace` → verify annotation additions only (no new `lifecycleScope` params).
5. `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo test --workspace` → m078 conformance gate.
6. Parity CI (`cargo test --workspace -- parity_symmetric_equal`) → C122 shows `SymmetricEqual` polarity for the new fixtures.
