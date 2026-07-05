# Annotation Wire Contracts: Milestone 163

**Date**: 2026-07-05
**Feature**: [spec.md](../spec.md) | **Plan**: [plan.md](../plan.md) | **Data Model**: [data-model.md](../data-model.md)

Per-format wire shape for the 1 new per-component annotation (C115) attached to workspace-peer main-module components whose `package.json` declared deps that couldn't be cross-resolved. Uses string value (single unresolved case) or JSON array (multiple case), matching the milestone-159 (C106/C107) + milestone-162 (C114) multi-value precedent.

## C115 — `mikebom:unresolved-declared-dep` (per-component)

### CycloneDX 1.6 (single unresolved dep)

```json
{
  "type": "application",
  "name": "docs",
  "purl": "pkg:npm/docs@0.0.0",
  "properties": [
    {"name": "mikebom:unresolved-declared-dep", "value": "@some/removed-package"}
  ]
}
```

### CycloneDX 1.6 (multiple unresolved deps)

```json
{
  "properties": [
    {"name": "mikebom:unresolved-declared-dep", "value": "[\"@a/pkg\", \"@b/pkg\"]"}
  ]
}
```

Value is a JSON-string-encoded array (matches milestone-159 C107 multi-alias + milestone-162 C114 multi-source shape). Consumers doing name-parsing pipe the value through `jq fromjson` to get a real array.

### SPDX 2.3

```json
{
  "annotations": [
    {"comment": "mikebom:unresolved-declared-dep=@some/removed-package", ...}
  ]
}
```

Multi-unresolved case: `comment = "mikebom:unresolved-declared-dep=[\"@a/pkg\", \"@b/pkg\"]"`.

### SPDX 3.0.1

```json
{
  "type": "Annotation",
  "statement": "mikebom:unresolved-declared-dep=@some/removed-package",
  "subject": "spdx:Package/docs",
  ...
}
```

**Value grammar**:

```text
value          ::= single_dep | multi_dep
single_dep     ::= <dep-name-as-declared-in-package-json>
multi_dep      ::= JSON-string-encoded array of sorted+deduplicated single_dep values
dep_name       ::= <as observed in package.json's dependencies: or devDependencies: block;
                    e.g., "@some/removed-package", "typo-package", "next">
```

**Emission conditions**: MUST appear on a component iff (a) the component is a workspace-peer main-module component (emitted via milestone 066's main-module logic on a `package.json` file inside a workspace peer directory) AND (b) at least one declared dep in that peer's `package.json` was `Unresolved` per the FR-004 classifier. When ALL declared deps resolved, the annotation is absent (implicit signal: consumer sees no annotation → all deps resolved cleanly).

## Parity catalog integration

Single row using the milestone-127 macro pattern at `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`:

```rust
// cdx.rs
cdx_anno!(c115_cdx, "mikebom:unresolved-declared-dep", component);

// spdx2.rs
spdx23_anno!(c115_spdx23, "mikebom:unresolved-declared-dep", component);

// spdx3.rs
spdx3_anno!(c115_spdx3, "mikebom:unresolved-declared-dep", component);
```

Registration in `mikebom-cli/src/parity/extractors/mod.rs` (adjacent to C114):

```rust
ParityExtractor { row_id: "C115", label: "mikebom:unresolved-declared-dep",           cdx: c115_cdx, spdx23: c115_spdx23, spdx3: c115_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
```

## Consumer jq recipes

```bash
# List every component with unresolved declared deps (CDX)
jq '.components[]
    | select((.properties // [])[] | .name == "mikebom:unresolved-declared-dep")
    | {name, purl,
       unresolved: (.properties[] | select(.name == "mikebom:unresolved-declared-dep") | .value)}' \
   sbom.cdx.json

# Zero-empty-version-PURL invariant check — MUST return 0 post-163
jq '[.components[].purl | select(test("^pkg:npm/[^@]+@$"))] | length' sbom.cdx.json

# Zero-phantom-edge invariant check — MUST return 0 post-163
jq '[.dependencies[].dependsOn[] | select(test("^pkg:npm/[^@]+@$"))] | length' sbom.cdx.json

# BFS reachability check (approximation — real BFS needs a graph walker)
# but this jq computes total npm components and root-reachable count
jq '{
  total_npm: [.components[] | select(.purl | startswith("pkg:npm/"))] | length,
  root_purl: .metadata.component.purl
}' sbom.cdx.json
```

## Byte-identity guarantee (SC-003)

For SBOMs from repos with NO npm components (no `package.json` in the scanned tree):

- C115 MUST NOT appear on any component.

For SBOMs from repos WITH npm components BUT where every declared dep in every workspace peer's `package.json` was successfully cross-resolved:

- C115 MUST NOT appear on any component.

Both guards keep SC-003 achievable: 10 non-`npm` milestone-090 fixtures × 3 formats = 30 goldens remain byte-identical to pre-163. The `npm` fixture's goldens MAY change if its `package.json` files declare deps that got cross-resolved (in which case the diff is limited to: (a) the design-tier phantom entries disappear from `components[]` array; (b) the workspace peer's `dependsOn` list points to concrete-version PURLs instead of empty-version PURLs).
