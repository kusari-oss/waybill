# Data model: SPDX 3 dependency completeness

## DependencyClaims (new, shared with CycloneDX)

Produced by `dependency_claims(components, complete_ecosystems, reachable_set, degraded_ecosystems, integrity, root)`:

- `complete: set<purl>`: components whose ecosystem is enumerated completely and whose graph was resolved, meaning every component in the ecosystem is reachable and the ecosystem is not degraded;
- `unknown: set<purl>`: components in an enumerated ecosystem whose graph was not resolved;
- `root_complete: bool`: trace integrity is clean (`incomplete_first_party_only`) and there are components.

Components in neither set carry no claim.

`build_compositions` (CycloneDX) and the SPDX 3 grouping pass both read it.

## Dependency set (grouped relationship)

Key: `(from, element type, scope)`. Value: the sorted set of target IRIs.

Its completeness is a function of `from`, per `contracts/spdx3-completeness.md`.

## Unknown-leaf relationship

For `from ∈ unknown` with no grouped relationship: `to = [NoAssertionElement]`, `completeness = noAssertion`.
