# Contract: Pico Filter-Parity Gate (SC-001 + SC-002)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## Contract

For every mikebom scan whose output contains a non-empty CycloneDX 1.6 document AND a non-empty SPDX 2.3 document (emitted under `--spdx2-relationship-compat=full`), the following two consumer filters MUST produce byte-identical PURL sets:

**Filter A (CDX-based)**: collect every `metadata.component[*].purl` (root component) + every `components[*].purl` where `components[*].scope == "excluded"`.

**Filter B (SPDX 2.3-based)**: collect the PURL of every `Package` that appears as the `spdxElementId` (source-side) of any relationship whose `relationshipType` is one of:
- `TEST_DEPENDENCY_OF`
- `DEV_DEPENDENCY_OF`
- `BUILD_DEPENDENCY_OF`
- `OPTIONAL_DEPENDENCY_OF`

Formally: `set(Filter_A(scan.cdx)) == set(Filter_B(scan.spdx23))` for every scan mikebom emits.

## Reference implementation (jq recipes)

**Filter A** (CDX):
```bash
jq -r '[
  ( .metadata.component.purl // empty ),
  ( .components[] | select(.scope == "excluded") | .purl )
] | sort | unique | .[]' scan.cdx.json
```

**Filter B** (SPDX 2.3):
```bash
jq -r '
  # Build a SPDXRef → PURL lookup table
  ( [ .packages[] | { key: .SPDXID, value: (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator) } ] | from_entries ) as $purl_by_ref |
  [ .relationships[]
    | select(.relationshipType | test("^(TEST|DEV|BUILD|OPTIONAL)_DEPENDENCY_OF$"))
    | $purl_by_ref[.spdxElementId]
  ] | sort | unique | .[]
' scan.spdx23.json
```

Both recipes MUST produce the same sorted, deduplicated PURL list for the same scan.

## Test signature

`mikebom-cli/tests/optional_dep_classification.rs`:

```rust
#[test]
fn pico_filter_parity_yaml_v3_case() {
    // Given: a Go fixture whose go.mod transitively pulls in check.v1 (yaml.v3's test dep)
    let scan = mikebom_scan(fixture_path!("transitive_parity/golang"));

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
        "SC-001+SC-002: CDX excluded set MUST equal SPDX 2.3 typed-source set");
}
```

## Non-conformance

Any scan whose CDX excluded set is a proper superset OR proper subset of the SPDX 2.3 typed-source set is a REGRESSION. CI MUST fail. This is the flagship contract for the pico use case.

## Scope

Applies ONLY under `--spdx2-relationship-compat=full` (default). Under `--spdx2-relationship-compat=basic`, no typed dep-scope edges are emitted per FR-003 — the filter-parity check does not apply, and downstream consumers who requested basic mode explicitly opted out of the typed-signal contract.

## Fixture

The test fixture referenced above is `mikebom-cli/tests/fixtures/transitive_parity/golang/` (already exists via m083 audit infrastructure). If the check.v1 case is not currently present in that fixture, an m179 task adds it.
