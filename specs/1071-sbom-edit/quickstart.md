# Quickstart: milestone 1071

```bash
# 1. Generate and sign, as today.
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

# 3. The recipient verifies the chain.
waybill sbom verify-chain customer.cdx.json --original full.cdx.json --key ./vendor.pub
```

## What to check
- `customer.cdx.json`:
  - validates against the CycloneDX 1.6 schema;
  - has no dev or test component and no `waybill:` annotation other than `waybill:derivation`;
  - has a root `externalReferences` entry of type `bom` carrying `full.cdx.json`'s SHA-256.
- `grep` finds no internal path, no `corp.acme.example` host, and no `@acme/` name in it.
- `verify-chain` reports the derivative's signature `Verified`, the original's hash `Matched`, and the original's signature `Verified`. Change one byte in either file and it fails.
- The same edit on `full.spdx.json` and `full.spdx3.json` passes `waybill sbom parity-check` against the edited CycloneDX.
