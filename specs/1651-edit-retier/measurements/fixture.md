# T001: the fixture's tiers

`waybill --offline sbom scan --path waybill-cli/tests/fixtures/sbom_edit_retier/project --no-deep-hash`, CycloneDX, at the branch's base `f0c6706`:

| Component | Scope | Tier |
|---|---|---|
| `pkg:npm/waybill-fixture-declared-dep` | required | design |
| `pkg:npm/waybill-fixture-dev-dep` | excluded | design |
| `pkg:npm/waybill-fixture-declared@2.0.0` | required | source |
| `pkg:npm/waybill-fixture-locked@1.0.0` | required | source |
| `pkg:npm/waybill-fixture-locked-dep@1.2.3` | required | source |
| `pkg:generic/project@0.0.0` (root) | — | none |

At the document level:
- `metadata.lifecycles` is `[design, pre-build]`;
- `waybill:graph-completeness` is `partial`, with reason `transitive-edges-unresolvable: npm`.

Every case the contract lists has a subject here: both `design` scopes, `source` components for R2's chain, and the completeness reason for C1.
