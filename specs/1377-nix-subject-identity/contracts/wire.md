# Wire contract: milestone 1377

What a consumer sees, per format. Applies only to scans that resolved a
closure; every other document is byte-identical (FR-011).

## 1. The subject

| format | carrier | value for the openssh wrapper flake |
| --- | --- | --- |
| CycloneDX 1.6 | `metadata.component` | the closure root's component: `name` `openssh`, `version` `10.3p1`, its PURL, and `cpe` `cpe:2.3:a:openbsd:openssh:10.3p1:*:*:*:*:*:*:*` |
| SPDX 2.3 | `documentDescribes` / `DESCRIBES` | that component's existing package SPDXID |
| SPDX 3.0.1 | `SpdxDocument.rootElement` | that component's existing package IRI |

With no declared or curated CPE, CycloneDX `metadata.component` carries **no**
`cpe` (FR-012), and neither SPDX format synthesises one.

## 2. Root-selection heuristic (C69, existing row)

- New value `"nix-evaluated-package"`, confidence `0.99`.
- Emission gate unchanged: present only when `losers` is non-empty.
- `docs/reference/sbom-format-mapping.md` C69 lists the new value.

## 3. CPE provenance, native carriers (new behaviour on closure components)

**CycloneDX**: one entry appended to `component.evidence.identity[]`:

```json
{ "field": "cpe",
  "concludedValue": "cpe:2.3:a:gnu:bash:5.3:9:*:*:*:*:*:*",
  "confidence": 0.9,
  "methods": [ { "technique": "manifest-analysis", "confidence": 0.9,
                 "value": "nixpkgs meta.identifiers.cpe" } ] }
```

For a curated CPE: `"technique": "other"`, `"value": "waybill curated CPE table"`,
confidence `0.7`. The two confidences are ordinal (declared over curated) and
deliberate, not calibrated, as with m1366's `0.8`.

**SPDX 2.3**: `externalRefs[]` entry `SECURITY`/`cpe23Type`, `comment` exactly one of:

- `"CPE declared by nixpkgs meta.identifiers.cpe"`
- `"CPE from waybill's curated CPE table"`

**SPDX 3.0.1**: `ExternalIdentifier` `cpe23`, `comment` with the same two strings.

## 4. `waybill:cpe-provenance` (new row C212)

Per component, only on nixpkgs-declared CPEs. Value: `{"basis":"drv"|"name-version","overrode":"<cpe>"}`,
with `overrode` omitted when the curated table agreed or had no row.

| format | carrier |
| --- | --- |
| CycloneDX | `component.properties[]` `waybill:cpe-provenance`, JSON-string value |
| SPDX 2.3 | `annotations[].comment` `waybill-annotation/v1` envelope |
| SPDX 3.0.1 | `Annotation.statement`, same envelope |

Parity: `Directionality::SymmetricEqual`, `order_sensitive: false`.

Justification clause for the mapping doc: no format has a field for the join
basis of an upstream-declared identifier, or for an identifier that was
considered and overridden.

## 5. C184 `waybill:nix-closure` (existing row, extended)

The JSON gains

```json
"identifiers": { "wanted": N, "received": N, "by-drv": N, "by-name-version": N,
                 "declared": N, "rejected": N }
```

beside the existing `licenses` object. No new row.

## 6. Internal only, never emitted

The evaluated-package tag (data-model.md) is registered in
`is_internal_emission_key` and appears in no document.
