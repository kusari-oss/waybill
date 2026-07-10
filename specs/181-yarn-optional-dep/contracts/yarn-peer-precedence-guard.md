# Contract: Yarn Peer-Precedence Guard (FR-005 / US3)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## Contract

When a dep name is BOTH declared as `peerDependencies.<name>` AND flagged as `peerDependenciesMeta.<name>.optional = true` in the root package.json, the m178 `PROVIDED_DEPENDENCY_OF` classification MUST win over m181's `OPTIONAL_DEPENDENCY_OF`:

- SPDX 2.3 relationship type: `PROVIDED_DEPENDENCY_OF` (m178) — NOT `OPTIONAL_DEPENDENCY_OF` (m181)
- CDX 1.6 target component: NOT `scope: "excluded"` (peer-optional stays lifecycle_scope=None)
- `mikebom:optional-derivation` annotation on the target: NOT present
- `mikebom:peer-edge-targets` annotation on the SOURCE (parent): present (m147/m178 unchanged)

## Guard Predicate

The reused m180 helper:

```rust
pub(crate) fn is_peer_optional(entry_name: &str, parent_pkg_json: &Value) -> bool {
    let has_peer_dep = parent_pkg_json
        .get("peerDependencies")
        .and_then(|v| v.as_object())
        .map(|obj| obj.contains_key(entry_name))
        .unwrap_or(false);
    let is_optional_peer = parent_pkg_json
        .get("peerDependenciesMeta")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get(entry_name))
        .and_then(|meta| meta.get("optional"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    has_peer_dep && is_optional_peer
}
```

## Delta from m180 US4

The predicate itself is IDENTICAL to m180 US4's guard. The DIFFERENCE is the source of the peer-optional signal:

| Aspect | m180 (npm/pnpm) | m181 (yarn v1 + Berry) |
|--------|-----------------|-------------------------|
| Source of `peer:true / optional:true` | Per-entry lockfile flags on `packages.<path>` (npm) / `packages.<key>` (pnpm) | ROOT package.json's `peerDependencies + peerDependenciesMeta` |
| Reason for the difference | npm/pnpm lockfiles carry `peer: true` on entries installed to satisfy peer deps | yarn's Plug'n'Play resolver moves this metadata into `.pnp.cjs` or into the source package.json declaration — yarn.lock does NOT have `peer: true` on lockfile entries the way npm/pnpm do |
| Guard mechanism | Skip Optional if entry's `peer && optional` flags are both true | Skip Optional if `is_peer_optional(name, root_pkg_json)` returns true |
| `#[allow(dead_code)]` on the helper | Present (helper unused; m180 uses inline lockfile-flag check) | **REMOVED** — m181 consumes the helper |

## Guard Placement (both variants)

```rust
// Inside parse_v1 or parse_berry, AFTER building the initial optional-name set:
let mut optional_names: HashSet<String> = /* v1 set-diff OR Berry walk */;
optional_names.retain(|n| !crate::scan_fs::package_db::npm::peer_optional::is_peer_optional(n, pkg_json));
```

The `retain` invocation runs once during set construction, not per component. Guard runtime is O(|optional_names| × O(pkg_json lookup)) = negligible.

## Wire-Format Outcome (unchanged from m180 US4)

Given a fixture like:

```json
{
  "peerDependencies": {"react": "^18"},
  "peerDependenciesMeta": {"react": {"optional": true}},
  "dependenciesMeta": {"react": {"optional": true}}   // Berry-style; only present on Berry US2 case
}
```

The SPDX 2.3 output MUST contain:

```json
{
  "relationships": [
    {
      "spdxElementId": "SPDXRef-<react>",
      "relationshipType": "PROVIDED_DEPENDENCY_OF",
      "relatedSpdxElement": "SPDXRef-<some-lib-or-root>"
    }
  ]
}
```

The SPDX 2.3 output MUST NOT contain any `OPTIONAL_DEPENDENCY_OF` edge whose source is react.

The CDX 1.6 output MUST show the react component WITHOUT `mikebom:optional-derivation` in its `properties[]`. React MUST NOT carry `scope: "excluded"` — its `lifecycle_scope` stays `None` (yarn's runtime default).

## Test Signature

```rust
#[test]
fn us3_peer_optional_react_emits_provided_not_optional_yarn() {
    let (cdx, spdx23) = run_scan(&fixture_path("yarn-peer-optional"), &[]);

    // (a) SPDX 2.3 has react as PROVIDED_DEPENDENCY_OF source
    // (b) SPDX 2.3 does NOT have react as OPTIONAL_DEPENDENCY_OF source
    // (c) CDX react component does NOT carry mikebom:optional-derivation
    // (d) CDX react component's scope is NOT "excluded"
    // ... (mirrors m180 US4 shape)
}
```

## Regression Guards (SC-008)

The m106 US5 baseline + m159 alias tests MUST continue to pass byte-identically. m181's changes are additive — the classifier only ELEVATES the lifecycle scope of matched names; every other code path stays unchanged.
