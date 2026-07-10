# Quickstart: Optional-Dependency Classification (m179)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Consumer flow — filter not-in-production components from a mikebom SBOM

If you're building a downstream tool (like pico) that wants to strip test-noise and optional deps from a mikebom-produced SBOM before running vulnerability/license analysis, use ONE OF the following per your ingest format. Both filters MUST yield the same PURL set (contract: SC-001 + SC-002).

### CycloneDX 1.6

```bash
# Get all PURLs that are NOT in the production deployment
jq -r '[
  ( .metadata.component.purl // empty ),
  ( .components[] | select(.scope == "excluded") | .purl )
] | sort | unique | .[]' scan.cdx.json > excluded-purls.txt
```

### SPDX 2.3

```bash
# Same PURL set, via SPDX 2.3's typed-relationship-type vocabulary
jq -r '
  ( [ .packages[] | { key: .SPDXID, value: (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator) } ] | from_entries ) as $purl_by_ref |
  [ .relationships[]
    | select(.relationshipType | test("^(TEST|DEV|BUILD|OPTIONAL)_DEPENDENCY_OF$"))
    | $purl_by_ref[.spdxElementId]
  ] | sort | unique | .[]
' scan.spdx23.json > excluded-purls.txt
```

**Note**: this works ONLY under `--spdx2-relationship-compat=full` (the mikebom default). If your operator explicitly requested `--spdx2-relationship-compat=basic`, every dep-scope classification collapses to `DEPENDS_ON` and the SPDX 2.3 filter above returns an empty set — that's the m228 escape hatch's intended behavior.

### SPDX 3.0.1

SPDX 3.0.1 does not have `OPTIONAL_DEPENDENCY_OF` in its native `LifecycleScopeType` enum. For SPDX 3 consumption, use the `mikebom:optional-derivation` annotation instead — but note this is a `mikebom:*` annotation (Principle V KEEP-BOTH carve-out), not a native construct. Prefer SPDX 2.3 for filter-parity consumers until SPDX 3.x adds the value.

## Filter semantic — what does "excluded" / typed-dep-scope actually mean?

A component is filtered out if it satisfies ANY of:

- **Manifest-declared test scope**: e.g., Cargo `[dev-dependencies]`, Maven `<scope>test</scope>`, npm `devDependencies` → `TEST_DEPENDENCY_OF` (existing m052).
- **Manifest-declared build-only scope**: e.g., Cargo `[build-dependencies]`, Maven `<scope>provided</scope>` → `BUILD_DEPENDENCY_OF` (existing m052).
- **Manifest-declared dev scope**: general dev-purpose → `DEV_DEPENDENCY_OF` (existing m052).
- **Manifest-declared optional**: Cargo `optional = true`, npm `optionalDependencies`, pip `[project.optional-dependencies.<extra>]`, Maven `<optional>true</optional>`, Gradle `compileOnly`, Erlang `optional_applications` → `OPTIONAL_DEPENDENCY_OF` (m179 NEW).
- **Build-graph analysis inferred not-needed**: Go `go mod why` classifier flagged the transitive as not-in-production → `TEST_DEPENDENCY_OF` (m179 NEW, m112 signal).

Any of these signals justifies filtering. For finer-grained information (e.g., "was it optional because Cargo said so, or because Maven said so?"), consult the `mikebom:optional-derivation` component-level annotation.

## Developer flow — add a new per-ecosystem optional-dep classifier

Template: Cargo reader (`mikebom-cli/src/scan_fs/package_db/cargo.rs`) is the canonical example (US3).

### Step 1: Detect the optional-declared construct in the manifest

Parse the ecosystem-specific manifest field. For Cargo:

```rust
// In cargo.rs, parsing [dependencies] table entries
if let toml::Value::Table(dep_spec) = dep_value {
    if dep_spec.get("optional").and_then(|v| v.as_bool()) == Some(true) {
        // This dep is Cargo-optional
        entry.lifecycle_scope = Some(LifecycleScope::Optional);
        entry.extra_annotations.insert(
            "mikebom:optional-derivation".to_string(),
            serde_json::Value::String("cargo-optional-true".to_string()),
        );
    }
}
```

### Step 2: Respect precedence

- If the manifest entry is under a table that already implies scope (e.g., Cargo `[dev-dependencies]`, `[build-dependencies]`), do NOT overwrite the existing `lifecycle_scope = Some(Development|Build|Test)`. Optional is checked ONLY within the runtime-scoped table (e.g., Cargo `[dependencies]`).
- The classifier at `mikebom-cli/src/scan_fs/mod.rs:1261` enforces the final precedence at emit time, so a stray Optional set alongside Development is not fatal — but readers SHOULD not emit conflicting signals.

### Step 3: Verify

Add a unit test in the reader's `#[cfg(test)]` block:

```rust
#[test]
fn cargo_optional_true_sets_lifecycle_scope_optional() {
    let manifest = r#"
        [package]
        name = "foo"
        version = "0.1.0"

        [dependencies]
        bar = { version = "1", optional = true }
    "#;
    let entries = parse_cargo_manifest(manifest);
    let bar = entries.iter().find(|e| e.name == "bar").unwrap();
    assert_eq!(bar.lifecycle_scope, Some(LifecycleScope::Optional));
    assert_eq!(
        bar.extra_annotations.get("mikebom:optional-derivation"),
        Some(&serde_json::Value::String("cargo-optional-true".into()))
    );
}
```

### Step 4: Update the ecosystem survey row

Edit `docs/reference/sbom-format-mapping.md` and `specs/179-spdx23-transitive-devscope/research.md` to reflect that the ecosystem is now covered.

### Step 5: Add integration test

Add a test to `mikebom-cli/tests/optional_dep_classification.rs`:

```rust
#[test]
fn cargo_optional_dep_emits_optional_dependency_of() {
    let scan = mikebom_scan(fixture_path!("optional_dep/cargo"));
    // ... assertions per SC-001, SC-002 gates ...
}
```

## Testing your integration

After landing a new per-ecosystem classifier:

1. `./scripts/pre-pr.sh` must pass clean (clippy + full workspace test).
2. `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --workspace` regenerates CDX goldens; verify ADDITIVE changes only.
3. `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --workspace` regenerates SPDX 2.3 goldens; verify new `OPTIONAL_DEPENDENCY_OF` edges + NO decrements in existing `*_DEPENDENCY_OF` counts.
4. `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --workspace` regenerates SPDX 3 goldens; verify only annotation additions (no new `lifecycleScope` params).
5. Run `spdx3-validate` (via existing m078 conformance harness) — must pass.
6. Update `docs/reference/reading-a-mikebom-sbom.md` if there's a new consumer-facing recipe.
