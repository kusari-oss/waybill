# Contract: Peer-Precedence Guard (FR-006 / US4)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## The Contract

When a dep is declared BOTH as `peerDependencies.<name>` AND as an optional dep (either directly via `optionalDependencies.<name>` OR indirectly via `peerDependenciesMeta.<name>.optional = true`), the m178 peer classification MUST win over m180's optional classification:

- SPDX 2.3 relationship type: `PROVIDED_DEPENDENCY_OF` (m178) — NOT `OPTIONAL_DEPENDENCY_OF` (m180).
- `mikebom:peer-edge-targets` annotation on the SOURCE component: present (m178 semantic preserved).
- `mikebom:optional-derivation` annotation on the TARGET component: NOT present (m180 semantic short-circuited by reader-time guard).
- Target's `lifecycle_scope`: NOT `Optional` (guard prevents the write).

## Precedence Predicate

The reader-time guard predicate is:

```rust
fn is_peer_optional(entry_name: &str, parent_package_json: &Value) -> bool {
    let has_peer_dep = parent_package_json
        .get("peerDependencies")
        .and_then(|v| v.as_object())
        .map(|obj| obj.contains_key(entry_name))
        .unwrap_or(false);
    let is_optional_peer = parent_package_json
        .get("peerDependenciesMeta")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get(entry_name))
        .and_then(|meta| meta.get("optional"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    has_peer_dep && is_optional_peer
}
```

The guard fires when BOTH conditions are true. If only one is true (peer without optional flag, or standalone `peerDependenciesMeta` entry without a matching `peerDependencies` entry), the classification follows the non-peer-optional path.

## Wire-Format Contract

Given a fixture like:

```json
{
  "name": "my-app",
  "version": "0.1.0",
  "peerDependencies": {"react": "^18"},
  "peerDependenciesMeta": {"react": {"optional": true}}
}
```

The SPDX 2.3 output MUST contain:

```json
{
  "relationships": [
    {
      "spdxElementId": "SPDXRef-<react>",
      "relationshipType": "PROVIDED_DEPENDENCY_OF",
      "relatedSpdxElement": "SPDXRef-<my-app>"
    }
  ]
}
```

The SPDX 2.3 output MUST NOT contain any `OPTIONAL_DEPENDENCY_OF` edge whose `spdxElementId` maps to react's PURL.

The CDX 1.6 output MUST show the `react` component WITHOUT `mikebom:optional-derivation` in its `properties[]`. The source component (`my-app`) MUST carry `mikebom:peer-edge-targets` including react's PURL (per m178).

## Non-conformance

Any scan whose SPDX 2.3 output contains BOTH:
- A `PROVIDED_DEPENDENCY_OF` edge for `<react>`, AND
- An `OPTIONAL_DEPENDENCY_OF` edge for `<react>`,

is a REGRESSION. CI MUST fail on such fixtures. This is the flagship contract for the peer-optional precedence rule.

## Test Signature

```rust
#[test]
fn us4_peer_optional_dep_emits_provided_not_optional() {
    let (cdx, spdx23, spdx3) = run_scan(&fixture_path("peer-optional"), &[]);

    // SPDX 2.3: react MUST appear as PROVIDED_DEPENDENCY_OF, NEVER as OPTIONAL_DEPENDENCY_OF.
    let react_spdx_id = find_package_by_name(&spdx23, "react")
        .and_then(|p| p.get("SPDXID").and_then(|v| v.as_str()))
        .unwrap()
        .to_string();
    let relationships = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .unwrap();
    let has_provided = relationships.iter().any(|r| {
        r.get("relationshipType").and_then(|v| v.as_str()) == Some("PROVIDED_DEPENDENCY_OF")
            && r.get("spdxElementId").and_then(|v| v.as_str()) == Some(&react_spdx_id)
    });
    let has_optional = relationships.iter().any(|r| {
        r.get("relationshipType").and_then(|v| v.as_str()) == Some("OPTIONAL_DEPENDENCY_OF")
            && r.get("spdxElementId").and_then(|v| v.as_str()) == Some(&react_spdx_id)
    });
    assert!(has_provided, "peer-optional react MUST appear as PROVIDED_DEPENDENCY_OF");
    assert!(!has_optional, "peer-optional react MUST NOT appear as OPTIONAL_DEPENDENCY_OF");

    // CDX: react component MUST NOT carry mikebom:optional-derivation.
    let react_cdx = find_component_by_name(&cdx, "react").unwrap();
    assert!(
        find_property(react_cdx, "mikebom:optional-derivation").is_none(),
        "peer-optional react MUST NOT carry mikebom:optional-derivation"
    );
}
```

## Scope

Applies to all four JavaScript-ecosystem readers (npm, pnpm, yarn, bun). Each reader implements the same guard at its own classifier site.
