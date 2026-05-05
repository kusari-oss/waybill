# Contract — OpenVEX 0.2.0 `Product.identifiers` per-instance extension

This contract specifies how mikebom extends per-product VEX statements to carry per-instance identifiers (CDX `bom-ref` / SPDX `SPDXID`) without forking the OpenVEX schema. Per Q2 clarification: **hybrid path** — use the existing OpenVEX 0.2.0 `Product.identifiers` map, document the aggregation rule for pre-072 consumers.

## C-1 — Wire shape

OpenVEX 0.2.0's `Product` schema includes an open-ended `identifiers: { [identifier_type]: string }` field. Mikebom populates this with a documented set of identifier-type keys:

| Key | Value | When emitted |
|---|---|---|
| `purl` | The component's PURL string | Always — same as the legacy `@id` field, included for completeness |
| `cyclonedx-bom-ref` | The CDX `bom-ref` value | When the OpenVEX sidecar accompanies a CDX SBOM |
| `spdx-spdxid` | The SPDX `SPDXID` value | When the OpenVEX sidecar accompanies a SPDX 2.3 / SPDX 3 SBOM |

**Worked example** (full triple-format scan emits an OpenVEX statement with all three identifier-types — the CDX SBOM and SPDX SBOMs share the same component identity but use different per-format instance identifiers):

```json
{
  "@context": "https://openvex.dev/ns/v0.2.0",
  "statements": [{
    "vulnerability": { "name": "CVE-2024-12345", "@id": "..." },
    "products": [
      {
        "@id": "pkg:golang/golang.org/x/net@v0.28.0",
        "identifiers": {
          "purl": "pkg:golang/golang.org/x/net@v0.28.0",
          "cyclonedx-bom-ref": "pkg:golang/golang.org/x/net@v0.28.0?bomref=image-instance-3",
          "spdx-spdxid": "SPDXRef-image-instance-3-net"
        }
      }
    ],
    "status": "not_affected",
    "justification": "vulnerable_code_not_present"
  }]
}
```

## C-2 — Pre-072 consumer fallback (no breakage)

Pre-072 OpenVEX consumers (e.g., older `vexctl`, ad-hoc consumer scripts) match products by `Product.@id` (the PURL string) and ignore the `identifiers` map. This is supported and documented:

- `Product.@id` continues to carry the PURL string.
- Pre-072 consumers see VEX statements they can apply at PURL granularity; multiple per-instance statements with the same PURL collapse to a per-PURL view (the consumer effectively gets the OR / aggregation rule defined in C-3).
- No pre-072 consumer breaks; the `identifiers` map is purely additive metadata.

## C-3 — Per-PURL aggregation rule for pre-072 consumers

When multiple component instances share the same PURL but have different VEX states, a per-PURL aggregation rollup MUST follow this rule:

```text
aggregate_state(purl) =
    affected   if any instance is `affected`
  | affected   if any instance has no explicit VEX statement AND has binding.strength != "verified"
                 (i.e., "could be affected" defaults to "affected" until proven otherwise)
  | not_affected if every instance is explicitly `not_affected` AND all bindings are `verified`
  | under_investigation if any instance is `under_investigation` (and no `affected` overrides)
  | fixed if every instance is explicitly `fixed`
```

The headline expression: **`affected ⊕ unbound-and-not-explicitly-vexed = affected`**.

This rule is the user's worked-example concern — a verified `not_affected` on instance A doesn't mask an unbound instance B's potential affectedness. Pre-072 per-PURL consumers compute this aggregate and get the safe-by-default answer.

## C-4 — Post-072 consumer benefit

Post-072 consumers that recognize `identifiers` can apply VEX statements at instance granularity:

```text
for each product in statement.products:
    bom_ref = product.identifiers.get("cyclonedx-bom-ref")  # (or spdx-spdxid)
    if bom_ref is not None:
        apply statement to the specific component with bom-ref == bom_ref
    else:
        fall back to PURL match (legacy behavior)
```

Per-instance application is the user's worked-example resolution — instance A gets `not_affected` (verified-bound), instance B retains `affected` / `under_investigation` (unbound), no false safety claim from the aggregate.

## C-5 — `binding-unverified` caveat for `caveated` propagation

When `mikebom sbom enrich --vex-propagation-mode caveated` propagates a VEX statement onto a component whose `mikebom:source-document-binding.strength != "verified"`, the propagated statement carries a structured caveat. The caveat lives in TWO places per OpenVEX-friendly + mikebom-readable:

1. The OpenVEX `Statement.justification` field is set to the original justification value if present, or NULL if not. (Mikebom does NOT silently overwrite the justification field.)
2. A new mikebom-specific field `mikebom:vex-binding-status` is attached as a sibling field on the statement (open-ended per OpenVEX 0.2.0's "additional fields tolerated" posture):

```json
{
  "vulnerability": { "name": "CVE-2024-12345" },
  "products": [{
    "@id": "pkg:golang/golang.org/x/net@v0.28.0",
    "identifiers": {
      "purl": "pkg:golang/golang.org/x/net@v0.28.0",
      "cyclonedx-bom-ref": "pkg:golang/golang.org/x/net@v0.28.0?bomref=image-instance-3"
    }
  }],
  "status": "not_affected",
  "justification": "vulnerable_code_not_present",
  "mikebom:vex-binding-status": {
    "status": "unverified",
    "reason": "binding-strength-weak: lockfile + manifest match but no VCS commit recorded in source-tier scan",
    "source_statement_provenance": {
      "source_sbom_sha256": "e5f6...",
      "propagated_by": "mikebom-0.1.0-alpha.15 sbom enrich --vex-propagation-mode caveated"
    }
  }
}
```

When the binding IS `verified`, the `mikebom:vex-binding-status` field is omitted entirely (clean post-072 output).

When `--vex-propagation-mode strict`, the propagation is REFUSED for non-`verified` bindings — no statement appears in the output for that (vuln, instance) pair, and a structured refusal-rationale annotation is added to the target SBOM via the existing `mikebom:enrichment-patch` properties.

## C-6 — Stability commitment

- The identifier-type keys (`purl`, `cyclonedx-bom-ref`, `spdx-spdxid`) are stable across mikebom alpha versions post-072.
- Future identifier types (e.g., `sigstore-rekor-uuid`) MAY be added; readers MUST tolerate unknown keys (already required by OpenVEX 0.2.0's open-dictionary semantic).
- The `mikebom:vex-binding-status` field shape is stable; new sub-fields MAY be added (skip_serializing_if-gated) without breaking pre-072 consumers (which ignore the unknown sibling field anyway).
- The aggregation rule (C-3) is the documented fallback for pre-072 consumers; mikebom's own per-instance application (C-4) is the post-072 contract.
