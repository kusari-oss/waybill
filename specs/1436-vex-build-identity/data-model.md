# Data model: Nix closure VEX that scanners honour

## BuildIdentity

The `nix_drv` qualifier on a patched component's PURL (research R1).

| field | type | rule |
|---|---|---|
| `drv_hash` | 32 chars, Nix base32 alphabet | the hash part of the `.drv` store path whose patches resolve at least one CVE for this component |

- **Present iff** the component has at least one `pedigree.patches[].resolves` entry **and** those entries come from exactly one derivation (research R4).
- **Absent otherwise.** The PURL is then byte-identical to today's (FR-001, clarification 2).
- **The PURL** is `<today's PURL>?nix_drv=<drv_hash>`, using the `Purl` type's own qualifier encoding.

## BackportClaim

One per (component, resolved CVE). This is what `pedigree.patches[].resolves` already holds.

| field | source |
|---|---|
| component | the closure component (its qualified PURL and bom-ref) |
| cve | `resolves[].id` |
| grade | m1035 evidence grade, unchanged |
| provenance | the patch's identity, as the grade note carries it today |

## VEX statement (closure backports)

There is exactly one per BackportClaim whose component has a BuildIdentity.

| carrier | field | value |
|---|---|---|
| OpenVEX sidecar | `vulnerability.name` | cve |
| | `products[0].@id` and `identifiers.purl` | the qualified PURL |
| | `products[0].subcomponents` | none |
| | `status` | `fixed` |
| | `impact_statement` / status notes | m1035 grade note and provenance (FR-004) |
| CycloneDX `vulnerabilities[]` | `id` | cve |
| | `affects[0].ref` | the component's bom-ref |
| | `analysis.state` | `resolved` (research R5) |
| | `analysis.detail` | the same note |

No `affected` statement is emitted for a BackportClaim (FR-003). No statement names an unqualified PURL for a backport.

## UnscopedClaims

A count, **C220 `waybill:vex-claims-unscoped`**, at document scope.

- **What it counts:** the BackportClaims that got no statement because their component's patches come from several derivations (research R4).
- **Encoding:** a decimal string, emitted only when greater than zero. That is C193's shape and rationale.
- **Formats:** all three, with parity.

## Invariants

- Every statement's product PURL is a component PURL in the same document, or is counted by C193 when projection dropped it.
- No document holds a backport `affected` statement, or a CycloneDX `exploitable` entry, for a (component, CVE) that the component's own `resolves` lists (SC-003).
- The qualified PURL is identical in CycloneDX, SPDX 2.3 (`externalRefs` purl), SPDX 3 (`software_packageUrl`) and the OpenVEX product (FR-006).
- Every relationship endpoint is a component in the document (#980).
