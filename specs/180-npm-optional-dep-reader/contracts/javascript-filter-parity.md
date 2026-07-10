# Contract: JavaScript-Ecosystem Filter-Parity Gate (SC-001)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## The Contract

For every JavaScript-ecosystem scan (npm / pnpm / yarn / bun lockfile) whose output contains a non-empty CycloneDX 1.6 document AND a non-empty SPDX 2.3 document (emitted under `--spdx2-relationship-compat=full`), the following two consumer filters MUST produce byte-identical PURL sets:

**Filter A (CDX-based)**: collect every `metadata.component[*].purl` (root component) + every `components[*].purl` where `components[*].scope == "excluded"`.

**Filter B (SPDX 2.3-based)**: collect the PURL of every `Package` that appears as the `spdxElementId` (source-side) of any relationship whose `relationshipType` is one of:
- `TEST_DEPENDENCY_OF`
- `DEV_DEPENDENCY_OF`
- `BUILD_DEPENDENCY_OF`
- `OPTIONAL_DEPENDENCY_OF`

`set(Filter_A(scan.cdx)) == set(Filter_B(scan.spdx23))` for every JavaScript scan.

This is the same SC-001 gate m179 established for Go + Cargo, extended to the four JavaScript-ecosystem readers.

## Reference Implementation

Same jq recipes as m179's `pico-filter-parity.md` — the shape doesn't depend on ecosystem.

## Test Signature

Follows the same shape as m179's `pico_filter_parity_yaml_v3_case`, applied per-fixture:

```rust
#[test]
fn us1_npm_filter_parity_fsevents_case() {
    // npm fixture with optionalDependencies: {fsevents: '^2'}
    let scan = mikebom_scan(fixture_path!("optional_dep/npm"));

    let cdx_excluded_purls: BTreeSet<String> = scan.cdx
        .components.iter()
        .filter(|c| c.scope == Some("excluded".into()))
        .map(|c| c.purl.clone())
        .collect();

    let spdx23_typed_source_purls: BTreeSet<String> = scan.spdx23
        .relationships.iter()
        .filter(|r| matches!(r.relationship_type.as_str(),
            "TEST_DEPENDENCY_OF" | "DEV_DEPENDENCY_OF"
            | "BUILD_DEPENDENCY_OF" | "OPTIONAL_DEPENDENCY_OF"))
        .filter_map(|r| scan.spdx23.package_purl(&r.spdx_element_id))
        .collect();

    assert_eq!(cdx_excluded_purls, spdx23_typed_source_purls,
        "SC-001: CDX excluded set MUST equal SPDX 2.3 typed-source set");
}
```

Parallel tests for pnpm, yarn v1, yarn Berry, and (contingent) bun.

## Non-conformance

Any JavaScript scan whose CDX excluded set is a proper superset OR proper subset of the SPDX 2.3 typed-source set is a REGRESSION. CI MUST fail.

Applies ONLY under `--spdx2-relationship-compat=full` (default). Under `basic`, no typed dep-scope edges are emitted per m228; the filter-parity check does not apply.

## Peer-Optional Interaction (see peer-precedence-guard.md)

Peer-optional deps (e.g., react in a peer-declared + optional-flagged pattern) MUST NOT appear in the SPDX 2.3 filter set — they emit as `PROVIDED_DEPENDENCY_OF` (m178) instead, which is NOT one of the typed dep-scope verbs this contract tests. Consequently, peer-optional deps DO NOT appear in CDX's `scope: "excluded"` either (their `lifecycle_scope` stays Runtime).

This preserves cross-format equality: `Filter_A` and `Filter_B` both exclude peer-optional deps → both sets stay balanced.
