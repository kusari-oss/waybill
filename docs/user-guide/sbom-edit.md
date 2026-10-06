# Editing an SBOM

Generate once, derive what you distribute. `waybill sbom scan` describes
everything it found. What you hand to a customer, an auditor or a public
registry is usually less:

- no development and test dependencies;
- no internal file paths;
- no internal hostnames;
- no names of private packages.

`waybill sbom edit` derives that version from the full one. The derivative
says what it is: it carries the original's hash and signature and the
operations applied, so a recipient can check it against the original
without seeing anything the edit removed.

```bash
# 1. Generate and sign, as usual.
waybill sbom scan --path ./product --output cyclonedx-json=full.cdx.json --sign-key ./vendor.pem

# 2. Derive the distributable version.
waybill sbom edit full.cdx.json -o customer.cdx.json \
  --drop 'scope=development,test' \
  --drop 'tier=file' \
  --drop-annotations 'waybill:' \
  --redact paths \
  --redact 'hosts:pseudonymise=*.corp.acme.example' \
  --redact 'names:pseudonymise=@acme/*' --redact-key-file ./redact.key \
  --sign-key ./vendor.pem

# 3. Whoever holds the original can verify the chain.
waybill sbom verify-chain customer.cdx.json --original full.cdx.json --key ./vendor.pub
```

The editor works on CycloneDX 1.6, SPDX 2.3 and SPDX 3.0.1 JSON, and writes
the format it reads. The same operations on the three formats of one scan
give three documents that agree under `waybill sbom parity-check`. Flag
details are in the [CLI reference](cli-reference.md#waybill-sbom-edit).

## Operations

Operations apply in command-line order. Each reports how many items it
matched and changed. One that matches nothing is reported, not an error.

### Dropping components: `--drop <selector>`

A selector is `;`-separated `key=value[,value...]` terms. Terms combine with
AND; values within a term combine with OR.

| Key | Matches |
|---|---|
| `purl` | the PURL, as a glob |
| `ecosystem` | the PURL type: `npm`, `cargo`, `deb`, … |
| `scope` | `runtime`, `development`, `build`, `test`, `optional` |
| `tier` | the component's tier: `source`, `design`, `analyzed`, …, or `file` |
| `role` | the component's role or type |
| `name` | the name, as a glob, or `re:<regex>` |

Examples: `scope=development,test`, `ecosystem=npm;name=@acme/*`, `tier=file`.

Dropping a component removes everything that refers to it:

- its dependency edges;
- annotations and relationships about it;
- licence and vulnerability entries that only it used.

Its dependents are connected to its dependencies, so nothing reachable before
becomes unreachable. A bridged edge keeps a non-runtime scope: a dependency
reached through a dev-only link stays dev-only.

A changed dependency list is no longer known complete, and the document says
so:

- CycloneDX: an `incomplete` composition;
- SPDX 3: `completeness: incomplete` on the relationship;
- all three formats: `waybill:graph-completeness` becomes `unknown`.

A selector that matches the document's root is refused: the result would
describe nothing.

### Dropping annotations: `--drop-annotations <namespace>`

This removes annotations whose field starts with the namespace (`waybill:`,
`waybill:graph-`, …). Two are never removed:

- `waybill:generation-context`: every document states which mode produced it (constitution, Operating Modes);
- `waybill:derivation`: the record this command adds.

### Redacting: `--redact <class>[:<mode>][=<pattern>]`

| Class | What is replaced | Default mode | Pattern |
|---|---|---|---|
| `paths` | every path-bearing field: source files, file paths, evidence locations, workspace members, read sets, … | `remove` | optional; all paths if omitted |
| `hosts` | hostnames matching the pattern, wherever they appear (URLs, identifiers, creators) | `pseudonymise` | required |
| `names` | component names matching the pattern, in every form the document writes them: plain, in PURLs (`%40acme/…`), in CPEs (`\@acme\/…`), and in the component's own download URLs | `pseudonymise` | required |

The two modes:

- **`remove`** replaces each value with an opaque marker:
  - `redacted-path-<n>` for paths;
  - `redacted-host-<n>.invalid` for hosts;
  - `redacted-<n>` for names.

  Distinct values keep distinct markers, so the components stay distinct.
- **`pseudonymise`** replaces each value with `redacted-` plus an HMAC of the
  value under your key (`--redact-key-file`). The same value under the same
  key gives the same token in every document. A recipient can correlate
  components across your SBOMs without learning their names.

Rewritten PURLs still parse as PURLs, and hosts stay valid hostnames under
the reserved `.invalid` top-level domain.

Redaction fails closed. After replacing, waybill searches the whole output,
including inside base64-encoded material, for every form it replaced. If any
remains, nothing is written.

#### Keys

The key file is read as bytes, with a trailing newline ignored. It is never
written to the output or logged. Keep it secret: anyone holding it can test a
guessed name against a pseudonym. Rotate it to break linkability with SBOMs
you have already published.

#### What redaction does not hide

Redaction is not anonymisation. A redacted component can still be identified
from what remains:

- **Content hashes** of a public package identify it exactly.
- **Version strings, licences and download sizes** narrow it down.
- **The dependency graph's shape** is itself a fingerprint. A component with
  one well-known dependency set is often recognisable.
- **Pseudonyms are linkable** by design, across documents and across time,
  by anyone; and they are reversible by guessing, by anyone holding the key.
- **The derivation record's original hash** lets anyone holding a candidate
  original confirm it was the source.

Drop what must not be identified, rather than only redacting it.

## The derivation record

Every edited document carries a `waybill:derivation` annotation (catalogue
row C194):

```json
{
  "schema": "waybill-derivation/v1",
  "original": {
    "sha256": "…",
    "format": "cyclonedx-1.6",
    "signature": { "kind": "jsf", "embedded": true, "material": { … } }
  },
  "operations": [
    { "category": "drop-components", "matched": 2, "changed": 2 },
    { "category": "redact-hosts", "matched": 1, "changed": 1 }
  ],
  "ancestors": [],
  "tool": "waybill 0.11.0",
  "created": "2026-10-06T00:00:00Z"
}
```

The record holds counts, never values.

- **`original.signature`** holds the original's signature material without
  any signed payload. If that material contains a value the edit redacted,
  such as a keyless certificate naming an internal host, it holds only the
  material's digest, with `reason: contains-redacted-values`.
- **`ancestors`** holds the original's own record, when the original was
  itself edited.

Each format also links to the original natively:

| Format | Link | The derived document's identity |
|---|---|---|
| CycloneDX | root `externalReferences[]` entry of type `bom`, `url: urn:sha256:<hash>`, with a SHA-256 hash | same `serialNumber`, `version` incremented |
| SPDX 2.3 | `externalDocumentRefs[]` entry `DocumentRef-original` with its SHA-256, and `SPDXRef-DOCUMENT AMENDS DocumentRef-original:SPDXRef-DOCUMENT` | new `documentNamespace` |
| SPDX 3 | an `import` `ExternalMap` for the original document, verified by SHA-256, and a `Relationship` original `amendedBy` derived | new `SpdxDocument` IRI; elements keep theirs |

The original's own signature does not survive an edit, because it signs
bytes that no longer exist. Its material moves into the record. Sign the
output with `--sign-key` or `--sign`, exactly as with `sbom scan`.

## Verifying a chain

```bash
waybill sbom verify-chain customer.cdx.json \
  --original full.cdx.json --key vendor.pub
```

For each step, newest first, this reports three checks:

- **own signature:** the document's signature, in-document (CycloneDX JSF) or
  in its sidecar (`.sig.json` DSSE, `.sig.bundle.json` Sigstore);
- **original hash:** whether the supplied original is the one the record names;
- **original signature:** the signature the record carries, checked against
  the supplied original's bytes.

Pass `--original` once per step to follow a chain of edits. A step without
its original is reported as `original-not-supplied`, not failed.

Static-key signatures verify only against keys you pass with `--key`. A key
embedded in a signature is never trusted, since anyone can re-sign with
their own. Passing any `--key` also requires the derived document itself to
be signed, so a stripped signature fails rather than reading as unsigned.

Keyless (Sigstore) signatures report **`delegated`**. waybill checks that
the bundle signs this document's digest, but it holds no Sigstore trust
root. So the certificate chain, the signer's identity and the transparency
log entry are for `cosign`, and verify-chain prints the exact
`cosign verify-blob` command to run. A delegated check is never counted as
verified.

The exit status is non-zero if any check failed or mismatched.

## Limits

- **One document per invocation.** The OpenVEX sidecar a scan writes next
  to an SPDX document (`waybill.openvex.json`) is not edited. It names
  components by PURL, so edit or withhold it separately when you drop or
  redact components.
- **SPDX 2.3 key order.** waybill writes SPDX 2.3 in field order, not sorted.
  An edited SPDX 2.3 document is written with sorted keys: the same JSON,
  reordered. CycloneDX and SPDX 3 keep their bytes outside what changed.
- **Not yet.** Policy files describing a standard edit, and re-identification
  of pseudonyms for key holders, are planned for a follow-up milestone.
