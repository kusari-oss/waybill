# Data Model: Edit an emitted SBOM

## EditOp (one operation; the unit a policy file will list)

```text
EditOp {
  action:   DropComponents | DropAnnotations | Redact   // ReIdentify reserved, next milestone
  selector: Selector?            // DropComponents only
  params:   DropAnnotations { namespace }
          | Redact { class: Paths | Hosts | Names, mode: Remove | Pseudonymise, pattern? }
}
```
- `serde` (de)serialisable. The CLI flags parse into it, and the next milestone's policy file deserialises into the same type.
- Operations apply in the order given.

## Selector

```text
Selector { purl?: Glob, ecosystem?: [String], scope?: [Runtime|Development|Build|Test],
           tier?: [String], role?: [String], name?: Glob | Regex }
```
- All present fields must match (AND). Values within one field combine with OR.
- **An empty selector is an error**, so "drop everything" is never accidental.
- **Matching never selects the document's root or subject.** A selector that would select it is refused (FR-016).

## Per-format adapter (internal)

Each of `Cdx16`, `Spdx23` and `Spdx301` implements:
- `components() -> [ComponentView]`, where a view holds the identifier and the selector-relevant facts read per research R3;
- `edges()` / `set_edges(...)`: dependency edges with scope, per R4;
- `drop(ids)`: removes the components and every reference to them, per R4;
- `downgrade_completeness(changed_ids)`;
- `remove_annotations(namespace)`;
- `redaction_targets(class, pattern) -> [String]` and `rewrite_strings(map)`, per R5;
- `attach_derivation(record)`: the native link (R1) plus the `waybill:derivation` annotation;
- `serialise()`: sorted keys, 2-space indentation.

## DerivationRecord (`waybill:derivation`, document scope, catalogue C194)

```json
{
  "schema": "waybill-derivation/v1",
  "original": {
    "sha256": "<hex of the original file's bytes>",
    "format": "cyclonedx-1.6 | spdx-2.3 | spdx-3.0.1",
    "signature": { "kind": "jsf | dsse | sigstore-bundle | none", "material": { } }
  },
  "operations": [ { "category": "drop-components | drop-annotations | redact-paths | redact-hosts | redact-names",
                    "matched": 12, "changed": 12 } ],
  "ancestors": [ <the original's own waybill:derivation record, recursively> ],
  "tool": "waybill <version>",
  "created": "<RFC 3339>"
}
```
- **Never holds a removed or redacted value.** `category` is a closed set, and the counts are numbers.
- **`material`** is the original's signature, copied verbatim: the JSF object, the DSSE envelope, or the Sigstore bundle. It's absent when `kind` is `none`.
- **Serialisation** is canonical, with sorted keys, so the record is identical across the three formats (parity row C194, `SymmetricEqual`).

## Native derivation link (research R1)

| format | placement |
|---|---|
| CycloneDX 1.6 | root `externalReferences[]`: `{type:"bom", url:"urn:sha256:<hex>", hashes:[{alg:"SHA-256", content:<hex>}], comment:"waybill sbom edit: derived from"}` |
| SPDX 2.3 | `externalDocumentRefs[]`: `{externalDocumentId:"DocumentRef-original", spdxDocument:<original namespace or urn:sha256>, checksum:{SHA256}}`; relationship `SPDXRef-DOCUMENT AMENDS DocumentRef-original:SPDXRef-DOCUMENT` |
| SPDX 3.0.1 | `SpdxDocument.import[]`: `ExternalMap{externalSpdxId:<original doc IRI or urn:sha256>, verifiedUsing:[Hash sha256]}`; `Relationship{from:<original>, relationshipType:"amendedBy", to:[<this SpdxDocument>]}` |

**A second edit** appends a new link pointing at its immediate original. Earlier links are kept, as they are untouched content.

## Verification report (`waybill sbom verify-chain`)

```text
ChainReport { steps: [ LinkCheck ], ok: bool }
LinkCheck {
  document: path, sha256,
  own_signature:      Verified | Delegated{command} | Unsigned | Failed{reason},
  original_hash:      Matched | Mismatched | OriginalNotSupplied,
  original_signature: Verified | Delegated{command} | Unsigned | Failed | OriginalNotSupplied,
}
```
- `ok` is `false` if any field is `Failed` or `Mismatched`. The exit status follows `ok`.
- `Delegated` and `OriginalNotSupplied` are reported as-is, and never counted as verified (Principle X).
